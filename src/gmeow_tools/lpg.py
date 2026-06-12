"""Labeled Property Graph (LPG) transcoder — RDF → property graph export.

Consumes the GTS snapshot (the narrow waist, #267): the statement layer's
quads from their named graph, the reifier bindings and annotations straight
from the fold tables. Emits nodes + edges with statement metadata as edge
properties — the headline feature where GMEOW's RDF-1.2-first design pays
off. No RDF parser appears in this module at all.
"""

from __future__ import annotations

import csv
import hashlib
import json
from collections import defaultdict
from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path
from xml.etree.ElementTree import Element, SubElement, tostring

from gmeow_tools.config import (
    GTS_GRAPH_STATEMENTS,
    GTS_SNAPSHOT_FILE,
    LPG_DIR,
    NAMESPACE,
    PREFIXES,
    PROJECT_ROOT,
)
from gmeow_tools.generator import Generator, register
from gmeow_tools.gts_views import FoldView, load_fold

#: GMEOW namespace IRI (trailing slash).
_GMEOW_NS = NAMESPACE

#: RDF-star reifies predicate.
_RDF_REIFIES = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies"

#: Properties that we treat as statement-level metadata (edge properties).
_STATEMENT_META_PROPS: frozenset[str] = frozenset(
    {
        f"{_GMEOW_NS}confidence",
        f"{_GMEOW_NS}assertedAt",
        f"{_GMEOW_NS}validFrom",
        f"{_GMEOW_NS}validUntil",
        f"{_GMEOW_NS}accordingTo",
        f"{_GMEOW_NS}mappedFrom",
        f"{_GMEOW_NS}importanceLevel",
        f"{_GMEOW_NS}standpointModality",
    }
)

#: RDF types we skip as node labels (RDF/OWL infrastructure).
_SKIP_LABELS: frozenset[str] = frozenset(
    {
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#Statement",
        "http://www.w3.org/2002/07/owl#Axiom",
        "http://www.w3.org/2002/07/owl#NamedIndividual",
    }
)

#: TBox predicates to drop from the edge set.
_TBOX_PREDICATES: frozenset[str] = frozenset(
    {
        "http://www.w3.org/2000/01/rdf-schema#subClassOf",
        "http://www.w3.org/2000/01/rdf-schema#subPropertyOf",
        "http://www.w3.org/2000/01/rdf-schema#domain",
        "http://www.w3.org/2000/01/rdf-schema#range",
        "http://www.w3.org/2002/07/owl#equivalentClass",
        "http://www.w3.org/2002/07/owl#equivalentProperty",
        "http://www.w3.org/2002/07/owl#disjointWith",
        "http://www.w3.org/2002/07/owl#imports",
        "http://www.w3.org/2002/07/owl#inverseOf",
        "http://www.w3.org/2002/07/owl#propertyChainAxiom",
        "http://www.w3.org/2002/07/owl#allValuesFrom",
        "http://www.w3.org/2002/07/owl#someValuesFrom",
        "http://www.w3.org/2002/07/owl#hasValue",
        "http://www.w3.org/2002/07/owl#cardinality",
        "http://www.w3.org/2002/07/owl#minCardinality",
        "http://www.w3.org/2002/07/owl#maxCardinality",
        "http://www.w3.org/2002/07/owl#onProperty",
        "http://www.w3.org/2002/07/owl#onClass",
    }
)


@dataclass(slots=True, frozen=True)
class LPGNode:
    """One LPG node."""

    id: str
    labels: tuple[str, ...]
    properties: dict[str, object]


@dataclass(slots=True, frozen=True)
class LPGEdge:
    """One LPG edge (relationship)."""

    id: str
    source: str
    target: str
    type: str
    properties: dict[str, object]


class LPGGraph:
    """In-memory LPG representation."""

    def __init__(self) -> None:
        """Create an empty LPG graph."""
        self._nodes: dict[str, LPGNode] = {}
        self._edges: list[LPGEdge] = []
        self._edge_index: dict[tuple[str, str, str], list[LPGEdge]] = defaultdict(list)

    def add_node(self, node: LPGNode) -> None:
        """Add or merge a node by ID."""
        existing = self._nodes.get(node.id)
        if existing is None:
            self._nodes[node.id] = node
            return
        # Merge labels and properties
        merged_labels = tuple(sorted(set(existing.labels) | set(node.labels)))
        merged_props = dict(existing.properties)
        merged_props.update(node.properties)
        self._nodes[node.id] = LPGNode(
            id=node.id, labels=merged_labels, properties=merged_props
        )

    def add_edge(self, edge: LPGEdge) -> None:
        """Add an edge."""
        self._edges.append(edge)
        self._edge_index[(edge.source, edge.target, edge.type)].append(edge)

    @property
    def nodes(self) -> list[LPGNode]:
        """Return nodes sorted by ID for determinism."""
        return [self._nodes[k] for k in sorted(self._nodes)]

    @property
    def edges(self) -> list[LPGEdge]:
        """Return edges sorted by ID for determinism."""
        return sorted(self._edges, key=lambda e: e.id)

    def edges_for(self, source: str, target: str, type_: str) -> list[LPGEdge]:
        """Return edges matching the (source, target, type) key."""
        return self._edge_index.get((source, target, type_), [])


def _curie(iri: str) -> str:
    """Return a CURIE for ``iri`` if a known prefix matches, else the full IRI."""
    for prefix, namespace in sorted(PREFIXES.items(), key=lambda x: -len(x[1])):
        if iri.startswith(namespace):
            return f"{prefix}:{iri[len(namespace) :]}"
    return iri


def _short_key(iri: str) -> str:
    """Return a short property/relationship key from an IRI."""
    curie_val = _curie(iri)
    if ":" in curie_val:
        return curie_val.split(":", 1)[1]
    return curie_val


def _edge_id(source: str, target: str, type_: str, props: dict[str, object]) -> str:
    """Deterministic edge ID from components."""
    props_str = json.dumps(props, sort_keys=True, ensure_ascii=False)
    payload = f"{source}|{target}|{type_}|{props_str}"
    h = hashlib.sha256(payload.encode()).hexdigest()[:16]
    return f"edge:{h}"


# --------------------------------------------------------------------------- #
# The fold path (narrow waist #267): build the LPG from the GTS snapshot
# --------------------------------------------------------------------------- #

_RDF_TYPE_IRI = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
_XSD_NS = "http://www.w3.org/2001/XMLSchema#"


def _fold_value(view: FoldView, tid: int) -> object:
    """Convert a fold term to the lpg scalar form.

    Parity with the old pyoxigraph ``_value_from_term``, including its
    ``_bnode:`` rendering.
    """
    term = view.term(tid)
    if view.is_literal(tid):
        dt = view.datatype(tid)
        lex = term.value or ""
        if dt == _XSD_NS + "integer":
            try:
                return int(lex)
            except ValueError:
                return lex  # malformed input degrades to the raw lexical
        if dt in (_XSD_NS + "decimal", _XSD_NS + "double", _XSD_NS + "float"):
            try:
                return float(lex)
            except ValueError:
                return lex
        if dt == _XSD_NS + "boolean":
            return lex.lower() in ("true", "1")
        if term.lang is not None:
            return {"value": lex, "lang": term.lang}
        return lex
    if view.is_iri(tid):
        return _curie(term.value or "")
    if view.is_bnode(tid):
        return f"_bnode:{term.value or ''}"
    return view.nq_token(tid)


def _accumulate(bucket: dict[str, object], key: str, value: object) -> None:
    """Single value → value; repeats → list (the old row-accumulate idiom)."""
    existing = bucket.get(key)
    if existing is None:
        bucket[key] = value
    elif isinstance(existing, list):
        existing.append(value)
    else:
        bucket[key] = [existing, value]


def build_lpg(
    view: FoldView,
    *,
    scope: str | None = GTS_GRAPH_STATEMENTS,
    drop_tbox: bool = True,
) -> LPGGraph:
    """Build an LPG from the GTS fold — quads scoped to the statement layer.

    The fold-table counterpart of the SPARQL path: ``reifiers``/``annot``
    are direct table reads (the producer routed ``rdf:reifies`` and
    reifier-subject triples there, so the scoped quads are exactly the base
    triples and the reifies/annotation FILTERs vanish).
    """
    lpg = LPGGraph()
    tbox_predicates = _TBOX_PREDICATES if drop_tbox else frozenset()

    # --- Reifier metadata from the fold tables ---
    reifier_meta: dict[str, dict[str, object]] = defaultdict(dict)
    reifier_triple: dict[str, tuple[str, str, str]] = {}
    reifier_iris: set[str] = set()
    for rid, (qs, qp, qo) in view.reifiers().items():
        reifier = view.lex(rid)
        reifier_iris.add(reifier)
        reifier_triple[reifier] = (view.lex(qs), view.lex(qp), view.lex(qo))
    for rid, p, v in view.annotations():
        reifier = view.lex(rid)
        reifier_iris.add(reifier)
        _accumulate(reifier_meta[reifier], view.lex(p), _fold_value(view, v))

    triple_meta: dict[tuple[str, str, str], list[dict[str, object]]] = defaultdict(list)
    for reifier_key, (ts, tp, to_) in reifier_triple.items():
        meta = reifier_meta.get(reifier_key, {})
        triple_meta[(ts, tp, to_)].append({_short_key(k): v for k, v in meta.items()})

    # --- One pass over the scoped quads, bucketed by object kind ---
    type_tid = view.tid_of_iri(_RDF_TYPE_IRI)
    node_labels: dict[str, set[str]] = defaultdict(set)
    node_props: dict[str, dict[str, object]] = defaultdict(dict)
    object_rows: list[tuple[str, str, str]] = []

    for s, p, o, _g in view.quads(scope):
        if view.is_bnode(s):
            continue
        subject = view.lex(s)
        if p == type_tid:
            if subject in reifier_iris:
                continue
            type_iri = view.lex(o)
            if type_iri not in _SKIP_LABELS:
                node_labels[subject].add(_curie(type_iri))
            node_labels.setdefault(subject, set())
        elif view.is_literal(o):
            if subject in reifier_iris:
                continue
            _accumulate(
                node_props[subject],
                _short_key(view.lex(p)),
                _fold_value(view, o),
            )
            node_labels.setdefault(subject, set())
        elif view.is_iri(o):
            obj = view.lex(o)
            if subject in reifier_iris or obj in reifier_iris:
                continue
            node_labels.setdefault(subject, set())
            node_labels.setdefault(obj, set())
            object_rows.append((subject, view.lex(p), obj))

    # IRI annotation VALUES are referenced entities and become (possibly
    # isolated) nodes — the old path's "all distinct resources" straggler
    # union caught them via the reifier-subject annotation triples.
    for _rid, _p, v in view.annotations():
        if view.is_iri(v):
            value_iri = view.lex(v)
            if value_iri not in reifier_iris:
                node_labels.setdefault(value_iri, set())

    # --- Nodes ---
    for resource, labels in node_labels.items():
        props = dict(node_props.get(resource, {}))
        props["uri"] = resource
        if labels:
            props["types"] = sorted(labels)
        lpg.add_node(
            LPGNode(
                id=_curie(resource),
                labels=tuple(sorted(labels)),
                properties=props,
            )
        )

    # --- Edges ---
    for subject, predicate, obj in object_rows:
        if predicate in tbox_predicates:
            continue
        source_id, target_id = _curie(subject), _curie(obj)
        edge_type = _short_key(predicate)
        meta_list = triple_meta.get((subject, predicate, obj), []) or [{}]
        for meta in meta_list:
            lpg.add_edge(
                LPGEdge(
                    id=_edge_id(source_id, target_id, edge_type, meta),
                    source=source_id,
                    target=target_id,
                    type=edge_type,
                    properties=dict(meta),
                )
            )

    return lpg


# --------------------------------------------------------------------------- #
# Serializers
# --------------------------------------------------------------------------- #


def _escape_csv_value(value: object) -> str:
    """Escape a value for CSV output."""
    if value is None:
        return ""
    if isinstance(value, list | tuple | dict):
        return json.dumps(value, ensure_ascii=False)
    return str(value)


def serialize_generic_csv(lpg: LPGGraph, out_dir: Path) -> tuple[Path, Path]:
    """Emit generic ``nodes.csv`` + ``edges.csv`` into *out_dir*.

    Returns:
        Paths to the written node and edge files.
    """
    out_dir.mkdir(parents=True, exist_ok=True)
    nodes_path = out_dir / "nodes.csv"
    edges_path = out_dir / "edges.csv"

    node_keys: set[str] = set()
    for node in lpg.nodes:
        node_keys.update(node.properties.keys())
    node_cols = ["id", "labels", "uri", *sorted(node_keys - {"id", "labels", "uri"})]

    edge_keys: set[str] = set()
    for edge in lpg.edges:
        edge_keys.update(edge.properties.keys())
    edge_cols = [
        "id",
        "source",
        "target",
        "type",
        *sorted(edge_keys - {"id", "source", "target", "type"}),
    ]

    with nodes_path.open("w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=node_cols, lineterminator="\n")
        writer.writeheader()
        for node in lpg.nodes:
            row: dict[str, str] = {
                "id": node.id,
                "labels": ";".join(node.labels),
                "uri": str(node.properties.get("uri", "")),
            }
            for k in node_cols[3:]:
                row[k] = _escape_csv_value(node.properties.get(k))
            writer.writerow(row)

    with edges_path.open("w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=edge_cols, lineterminator="\n")
        writer.writeheader()
        for edge in lpg.edges:
            row = {
                "id": edge.id,
                "source": edge.source,
                "target": edge.target,
                "type": edge.type,
            }
            for k in edge_cols[4:]:
                row[k] = _escape_csv_value(edge.properties.get(k))
            writer.writerow(row)

    return nodes_path, edges_path


def serialize_neo4j_csv(lpg: LPGGraph, out_dir: Path) -> list[Path]:
    """Emit Neo4j admin-import typed CSVs into *out_dir*/neo4j/.

    Returns:
        List of written file paths.
    """
    neo_dir = out_dir / "neo4j"
    neo_dir.mkdir(parents=True, exist_ok=True)
    written: list[Path] = []

    nodes_path = neo_dir / "nodes.csv"
    node_keys: set[str] = set()
    for node in lpg.nodes:
        node_keys.update(node.properties.keys())
    node_cols = ["id:ID", ":LABEL", *sorted(node_keys - {"uri"})]

    with nodes_path.open("w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=node_cols, lineterminator="\n")
        writer.writeheader()
        for node in lpg.nodes:
            row: dict[str, str] = {
                "id:ID": node.id,
                ":LABEL": ";".join(node.labels) if node.labels else "Resource",
            }
            for k in node_cols[2:]:
                row[k] = _escape_csv_value(node.properties.get(k))
            writer.writerow(row)
    written.append(nodes_path)

    by_type: dict[str, list[LPGEdge]] = defaultdict(list)
    for edge in lpg.edges:
        by_type[edge.type].append(edge)

    edge_keys: set[str] = set()
    for edge in lpg.edges:
        edge_keys.update(edge.properties.keys())
    edge_prop_cols = sorted(edge_keys)

    for type_name, edges in sorted(by_type.items()):
        safe_name = type_name.replace(":", "_")
        path = neo_dir / f"edges_{safe_name}.csv"
        cols = ["id:ID", ":START_ID", ":END_ID", ":TYPE", *edge_prop_cols]
        with path.open("w", newline="", encoding="utf-8") as f:
            writer = csv.DictWriter(f, fieldnames=cols, lineterminator="\n")
            writer.writeheader()
            for edge in edges:
                edge_row: dict[str, str] = {
                    "id:ID": edge.id,
                    ":START_ID": edge.source,
                    ":END_ID": edge.target,
                    ":TYPE": type_name,
                }
                for k in edge_prop_cols:
                    edge_row[k] = _escape_csv_value(edge.properties.get(k))
                writer.writerow(edge_row)
        written.append(path)

    return written


def _cypher_escape(value: object) -> str:
    """Escape a Python value for Cypher."""
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int | float):
        return str(value)
    if isinstance(value, list | tuple):
        inner = ", ".join(_cypher_escape(v) for v in value)
        return f"[{inner}]"
    if isinstance(value, dict):
        inner = ", ".join(f"{k}: {_cypher_escape(v)}" for k, v in value.items())
        return f"{{{inner}}}"
    s = str(value).replace("\\", "\\\\").replace('"', '\\"')
    return f'"{s}"'


def serialize_cypher(lpg: LPGGraph, out_path: Path) -> Path:
    """Emit an openCypher ``CREATE`` script.

    Compatible with Neo4j, Memgraph, and Kuzu (ISO GQL converging).
    """
    out_path.parent.mkdir(parents=True, exist_ok=True)
    lines: list[str] = [
        "// GMEOW LPG export — generated by `gmeow export lpg`",
        "// DO NOT EDIT — regenerate from canonical sources.",
        "",
    ]

    for node in lpg.nodes:
        labels = "".join(f":{lbl.replace(':', '_')}" for lbl in node.labels)
        if not labels:
            labels = ":Resource"
        props = dict(node.properties)
        props["uri"] = node.id
        prop_str = ", ".join(
            f"{k}: {_cypher_escape(v)}" for k, v in sorted(props.items())
        )
        lines.append(f"CREATE (n{labels} {{{prop_str}}});")
        lines.append("")

    for edge in lpg.edges:
        rel_type = edge.type.replace(":", "_")
        if edge.properties:
            prop_str = ", ".join(
                f"{k}: {_cypher_escape(v)}" for k, v in sorted(edge.properties.items())
            )
            lines.append(
                f"MATCH (a), (b) WHERE a.uri = {_cypher_escape(edge.source)} "
                f"AND b.uri = {_cypher_escape(edge.target)} "
                f"CREATE (a)-[:{rel_type} {{{prop_str}}}]->(b);"
            )
        else:
            lines.append(
                f"MATCH (a), (b) WHERE a.uri = {_cypher_escape(edge.source)} "
                f"AND b.uri = {_cypher_escape(edge.target)} "
                f"CREATE (a)-[:{rel_type}]->(b);"
            )

    out_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return out_path


def serialize_graphml(lpg: LPGGraph, out_path: Path) -> Path:
    """Emit GraphML XML."""
    out_path.parent.mkdir(parents=True, exist_ok=True)

    root = Element("graphml")
    root.set("xmlns", "http://graphml.graphdrawing.org/xmlns")

    node_keys: set[str] = {"label"}
    edge_keys: set[str] = {"label"}
    for node in lpg.nodes:
        node_keys.update(node.properties.keys())
    for edge in lpg.edges:
        edge_keys.update(edge.properties.keys())

    for key in sorted(node_keys):
        k_elem = SubElement(root, "key")
        k_elem.set("id", key)
        k_elem.set("for", "node")
        k_elem.set("attr.name", key)
        k_elem.set("attr.type", "string")

    for key in sorted(edge_keys):
        k_elem = SubElement(root, "key")
        k_elem.set("id", key)
        k_elem.set("for", "edge")
        k_elem.set("attr.name", key)
        k_elem.set("attr.type", "string")

    graph = SubElement(root, "graph")
    graph.set("id", "G")
    graph.set("edgedefault", "directed")

    for node in lpg.nodes:
        n_elem = SubElement(graph, "node")
        n_elem.set("id", node.id)
        for label in node.labels:
            d = SubElement(n_elem, "data")
            d.set("key", "label")
            d.text = label
        for k, v in sorted(node.properties.items()):
            d = SubElement(n_elem, "data")
            d.set("key", k)
            d.text = _escape_csv_value(v)

    for edge in lpg.edges:
        e_elem = SubElement(graph, "edge")
        e_elem.set("id", edge.id)
        e_elem.set("source", edge.source)
        e_elem.set("target", edge.target)
        d_label = SubElement(e_elem, "data")
        d_label.set("key", "label")
        d_label.text = edge.type
        for k, v in sorted(edge.properties.items()):
            d = SubElement(e_elem, "data")
            d.set("key", k)
            d.text = _escape_csv_value(v)

    xml_bytes = tostring(root, encoding="unicode")
    header = '<?xml version="1.0" encoding="UTF-8"?>\n'
    out_path.write_text(header + xml_bytes + "\n", encoding="utf-8")
    return out_path


# --------------------------------------------------------------------------- #
# Public API
# --------------------------------------------------------------------------- #


def _write_all(lpg: LPGGraph, out_dir: Path, target: str) -> list[Path]:
    """Write all requested formats to *out_dir*; return written paths."""
    valid_targets = {"all", "csv", "neo4j", "cypher", "graphml"}
    if target not in valid_targets:
        msg = f"Unsupported target {target!r}. Expected one of: {sorted(valid_targets)}"
        raise ValueError(msg)
    written: list[Path] = []
    targets = ["csv", "neo4j", "cypher", "graphml"] if target == "all" else [target]

    if "csv" in targets:
        nodes, edges = serialize_generic_csv(lpg, out_dir)
        written.extend([nodes, edges])

    if "neo4j" in targets:
        written.extend(serialize_neo4j_csv(lpg, out_dir))

    if "cypher" in targets:
        written.append(serialize_cypher(lpg, out_dir / "gmeow.cypher"))

    if "graphml" in targets:
        written.append(serialize_graphml(lpg, out_dir / "gmeow.graphml"))

    return written


# --------------------------------------------------------------------------- #
# Registered generator
# --------------------------------------------------------------------------- #


@register
class LpgGenerator(Generator):
    """Export GMEOW to Labeled Property Graph (LPG) formats."""

    name: str = "lpg"
    _rendered_outputs: Sequence[Path] | None = None

    @property
    def inputs(self) -> Sequence[Path]:
        """Canonical inputs for the LPG generator."""
        return [GTS_SNAPSHOT_FILE]

    @property
    def outputs(self) -> Sequence[Path]:
        """Committed outputs for the LPG generator (dynamically discovered)."""
        if self._rendered_outputs is not None:
            return self._rendered_outputs
        # Scan the current committed LPG tree to discover dynamic outputs.
        if not LPG_DIR.exists():
            return []
        return [p for p in LPG_DIR.rglob("*") if p.is_file()]

    def render(self, staging: Path) -> None:
        """Render LPG artifacts into the staging tree."""
        lpg = build_lpg(load_fold())
        out_dir = staging / LPG_DIR.relative_to(PROJECT_ROOT)
        written = _write_all(lpg, out_dir, "all")
        self._rendered_outputs = [
            PROJECT_ROOT / p.relative_to(staging) for p in written
        ]
