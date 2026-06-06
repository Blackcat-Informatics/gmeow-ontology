"""Flattened, broadly-consumable export views of the GMEOW vocabulary.

The canonical artifact is the OWL 2 DL ontology; this module projects it into
plainer, lossy-but-useful views for consumers that don't speak RDF/OWL:

* **CSV term dictionaries** (classes, properties, individuals) — the simple
  tabular view, plus a **CSVW** descriptor so the tables are self-describing;
* a **Markdown term reference** — human-readable and diffable;
* a **JSONL term catalog** — one record per term, for tooling / RAG / embeddings;
* an **``llms.txt`` bundle** — the whole vocabulary in one LLM-ingestable file.

Everything is generated from the *asserted* merged graph (no reasoning, no
Docker) plus the SSSOM alignment axioms, so it runs anywhere ``rdflib`` does.
These views flatten reified relators and the RDF-star validity/provenance layer;
they are an entry point to the vocabulary, not a substitute for the OWL source.
"""

from __future__ import annotations

import csv
import json
from collections.abc import Iterable
from dataclasses import dataclass, field
from pathlib import Path
from typing import TypeGuard

from rdflib import OWL, RDF, RDFS, SKOS, BNode, Graph, URIRef

from gmeow_tools.config import (
    DIST_DIR,
    NAMESPACE,
    ONTOLOGY_IRI,
    PREFIXES,
    TITLE,
    VERSION,
)
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.mappings import build_alignment_graph, load_mappings

#: Property rdf:type → short kind label.
_PROPERTY_KINDS: dict[URIRef, str] = {
    OWL.ObjectProperty: "object",
    OWL.DatatypeProperty: "datatype",
    OWL.AnnotationProperty: "annotation",
}

#: Alignment predicate IRI → short relation tag for the flattened views.
_ALIGN_TAGS: dict[str, str] = {
    str(OWL.equivalentClass): "equivalentClass",
    str(OWL.equivalentProperty): "equivalentProperty",
    str(RDFS.subClassOf): "subClassOf",
    str(RDFS.subPropertyOf): "subPropertyOf",
    str(SKOS.closeMatch): "closeMatch",
    str(SKOS.exactMatch): "exactMatch",
    str(SKOS.relatedMatch): "relatedMatch",
}


@dataclass(slots=True)
class Term:
    """One flattened vocabulary term (class, property, or individual)."""

    category: str  # "class" | "property" | "individual"
    iri: str
    curie: str
    label: str
    definition: str
    parents: list[str] = field(default_factory=list)  # CURIEs
    prop_kind: str = ""  # object|datatype|annotation (properties only)
    domain: str = ""  # CURIE (properties only)
    range: str = ""  # CURIE (properties only)
    functional: bool = False
    sub_property_of: list[str] = field(default_factory=list)  # CURIEs
    types: list[str] = field(default_factory=list)  # CURIEs (individuals only)
    alignments: list[str] = field(default_factory=list)  # "tag=curie"

    def as_record(self) -> dict[str, object]:
        """Return a JSON-serializable record for the JSONL catalog."""
        rec: dict[str, object] = {
            "category": self.category,
            "curie": self.curie,
            "iri": self.iri,
            "label": self.label,
            "definition": self.definition,
        }
        if self.category == "class":
            rec["subClassOf"] = self.parents
        elif self.category == "property":
            rec |= {
                "propertyKind": self.prop_kind,
                "domain": self.domain,
                "range": self.range,
                "functional": self.functional,
                "subPropertyOf": self.sub_property_of,
            }
        else:
            rec["types"] = self.types
        if self.alignments:
            rec["alignments"] = self.alignments
        return rec


def curie(iri: str) -> str:
    """Compact an IRI to ``prefix:local`` using the longest matching prefix.

    Args:
        iri: The full IRI.

    Returns:
        A CURIE if a known prefix matches, else the IRI unchanged.
    """
    best_prefix = ""
    best_ns = ""
    for prefix, namespace in PREFIXES.items():
        if iri.startswith(namespace) and len(namespace) > len(best_ns):
            best_prefix, best_ns = prefix, namespace
    if best_ns:
        return f"{best_prefix}:{iri[len(best_ns) :]}"
    return iri


def _text(graph: Graph, subject: URIRef, predicate: URIRef) -> str:
    """Return the first literal value of ``predicate`` on ``subject`` as text."""
    value = graph.value(subject, predicate)
    return str(value) if value is not None else ""


def _curies(graph: Graph, subject: URIRef, predicate: URIRef) -> list[str]:
    """Return named object IRIs of ``predicate`` on ``subject`` as sorted CURIEs.

    Anonymous objects (blank-node OWL restrictions/chains) are skipped so their
    internal ids never leak into the flattened parent/super lists.
    """
    return sorted(
        curie(str(o))
        for o in graph.objects(subject, predicate)
        if isinstance(o, URIRef)
    )


def _alignments(alignments: Graph, subject: URIRef) -> list[str]:
    """Return ``tag=curie`` alignment strings for a term."""
    out: list[str] = []
    for predicate, obj in alignments.predicate_objects(subject):
        tag = _ALIGN_TAGS.get(str(predicate), curie(str(predicate)))
        out.append(f"{tag}={curie(str(obj))}")
    return sorted(out)


def _in_namespace(subject: object) -> TypeGuard[URIRef]:
    """Return whether a node is a GMEOW-namespace IRI (narrows to URIRef)."""
    return isinstance(subject, URIRef) and str(subject).startswith(NAMESPACE)


def _describe_node(graph: Graph, node: object) -> str:
    """Return a CURIE or a serialized union/intersection description of a class node."""
    if isinstance(node, URIRef):
        return curie(str(node))
    if isinstance(node, BNode):
        # Check union class
        union_list = graph.value(node, OWL.unionOf)
        if union_list:
            elements = []
            curr = union_list
            while curr and curr != RDF.nil:
                if not isinstance(curr, (URIRef, BNode)):
                    break
                first = graph.value(curr, RDF.first)
                if first:
                    elements.append(_describe_node(graph, first))
                curr = graph.value(curr, RDF.rest)  # type: ignore[assignment]
            return " | ".join(elements)
        # Check intersection class
        intersection_list = graph.value(node, OWL.intersectionOf)
        if intersection_list:
            elements = []
            curr = intersection_list
            while curr and curr != RDF.nil:
                if not isinstance(curr, (URIRef, BNode)):
                    break
                first = graph.value(curr, RDF.first)
                if first:
                    elements.append(_describe_node(graph, first))
                curr = graph.value(curr, RDF.rest)  # type: ignore[assignment]
            return " & ".join(elements)
    return str(node)


def collect_terms(
    graph: Graph | None = None, alignments: Graph | None = None
) -> list[Term]:
    """Collect every GMEOW class, property, and named individual as a Term.

    Args:
        graph: The asserted merged graph (defaults to loading it, no imports).
        alignments: The expanded SSSOM alignment graph (defaults to loading it).

    Returns:
        Terms sorted by (category, CURIE).
    """
    if graph is None:
        graph = load_merged_graph(include_imports=False)
    if alignments is None:
        alignments = build_alignment_graph(load_mappings())

    classes: set[URIRef] = {
        s for s in graph.subjects(RDF.type, OWL.Class) if _in_namespace(s)
    }
    properties: dict[URIRef, str] = {}
    for ptype, kind in _PROPERTY_KINDS.items():
        for s in graph.subjects(RDF.type, ptype):
            if isinstance(s, URIRef) and str(s).startswith(NAMESPACE):
                properties[s] = kind

    terms: list[Term] = []

    for s in classes:
        terms.append(
            Term(
                category="class",
                iri=str(s),
                curie=curie(str(s)),
                label=_text(graph, s, RDFS.label),
                definition=_text(graph, s, SKOS.definition),
                parents=_curies(graph, s, RDFS.subClassOf),
                alignments=_alignments(alignments, s),
            )
        )

    for s, kind in properties.items():
        domain_val = graph.value(s, RDFS.domain)
        range_val = graph.value(s, RDFS.range)
        terms.append(
            Term(
                category="property",
                iri=str(s),
                curie=curie(str(s)),
                label=_text(graph, s, RDFS.label),
                definition=_text(graph, s, SKOS.definition),
                prop_kind=kind,
                domain=(
                    _describe_node(graph, domain_val) if domain_val is not None else ""
                ),
                range=(
                    _describe_node(graph, range_val) if range_val is not None else ""
                ),
                functional=(s, RDF.type, OWL.FunctionalProperty) in graph,
                sub_property_of=_curies(graph, s, RDFS.subPropertyOf),
                alignments=_alignments(alignments, s),
            )
        )

    # Named individuals typed by a GMEOW class. Use the type index per class
    # (graph.subjects(RDF.type, cls)) rather than scanning every subject.
    declared = classes | set(properties)
    seen: set[URIRef] = set()
    for cls in classes:
        for s in graph.subjects(RDF.type, cls):
            if not _in_namespace(s) or s in declared or s in seen:
                continue
            seen.add(s)
            terms.append(
                Term(
                    category="individual",
                    iri=str(s),
                    curie=curie(str(s)),
                    label=_text(graph, s, RDFS.label),
                    definition=_text(graph, s, SKOS.definition),
                    types=sorted(
                        curie(str(t))
                        for t in graph.objects(s, RDF.type)
                        if t in classes
                    ),
                    alignments=_alignments(alignments, s),
                )
            )

    return sorted(terms, key=lambda t: (t.category, t.curie))


# --------------------------------------------------------------------------- #
# Writers
# --------------------------------------------------------------------------- #

_CLASS_COLUMNS = ["curie", "label", "definition", "subClassOf", "alignments", "iri"]
_PROPERTY_COLUMNS = [
    "curie",
    "label",
    "definition",
    "propertyKind",
    "domain",
    "range",
    "functional",
    "subPropertyOf",
    "alignments",
    "iri",
]
_INDIVIDUAL_COLUMNS = ["curie", "label", "definition", "types", "alignments", "iri"]


def _write_csv(path: Path, columns: list[str], rows: Iterable[dict[str, str]]) -> None:
    """Write a CSV with the given columns."""
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=columns)
        writer.writeheader()
        writer.writerows(rows)


def write_csvs(terms: list[Term], dist_dir: Path) -> list[Path]:
    """Write the class / property / individual term-dictionary CSVs."""
    classes = [t for t in terms if t.category == "class"]
    properties = [t for t in terms if t.category == "property"]
    individuals = [t for t in terms if t.category == "individual"]

    class_path = dist_dir / "gmeow-classes.csv"
    _write_csv(
        class_path,
        _CLASS_COLUMNS,
        (
            {
                "curie": t.curie,
                "label": t.label,
                "definition": t.definition,
                "subClassOf": "; ".join(t.parents),
                "alignments": "; ".join(t.alignments),
                "iri": t.iri,
            }
            for t in classes
        ),
    )

    property_path = dist_dir / "gmeow-properties.csv"
    _write_csv(
        property_path,
        _PROPERTY_COLUMNS,
        (
            {
                "curie": t.curie,
                "label": t.label,
                "definition": t.definition,
                "propertyKind": t.prop_kind,
                "domain": t.domain,
                "range": t.range,
                "functional": "true" if t.functional else "false",
                "subPropertyOf": "; ".join(t.sub_property_of),
                "alignments": "; ".join(t.alignments),
                "iri": t.iri,
            }
            for t in properties
        ),
    )

    individual_path = dist_dir / "gmeow-individuals.csv"
    _write_csv(
        individual_path,
        _INDIVIDUAL_COLUMNS,
        (
            {
                "curie": t.curie,
                "label": t.label,
                "definition": t.definition,
                "types": "; ".join(t.types),
                "alignments": "; ".join(t.alignments),
                "iri": t.iri,
            }
            for t in individuals
        ),
    )
    return [class_path, property_path, individual_path]


def write_csvw(dist_dir: Path) -> Path:
    """Write a CSVW (tabular metadata) descriptor for the term CSVs."""

    def table(url: str, columns: list[str]) -> dict[str, object]:
        return {
            "url": url,
            "tableSchema": {"columns": [{"name": c, "titles": c} for c in columns]},
        }

    descriptor: dict[str, object] = {
        "@context": "http://www.w3.org/ns/csvw",
        "dc:title": f"{TITLE} — term dictionaries",
        "dc:source": ONTOLOGY_IRI,
        "tables": [
            table("gmeow-classes.csv", _CLASS_COLUMNS),
            table("gmeow-properties.csv", _PROPERTY_COLUMNS),
            table("gmeow-individuals.csv", _INDIVIDUAL_COLUMNS),
        ],
    }
    path = dist_dir / "gmeow-terms.csvw.json"
    path.write_text(
        json.dumps(descriptor, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    return path


def write_jsonl(terms: list[Term], dist_dir: Path) -> Path:
    """Write the JSONL term catalog (one record per term)."""
    path = dist_dir / "gmeow-terms.jsonl"
    lines = [json.dumps(t.as_record(), ensure_ascii=False) for t in terms]
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return path


def write_markdown(terms: list[Term], dist_dir: Path) -> Path:
    """Write a human-readable Markdown term reference."""
    classes = [t for t in terms if t.category == "class"]
    properties = [t for t in terms if t.category == "property"]
    individuals = [t for t in terms if t.category == "individual"]
    lines = [
        f"# {TITLE} — term reference",
        "",
        f"Generated from the GMEOW {VERSION} vocabulary "
        f"({len(classes)} classes, {len(properties)} properties, "
        f"{len(individuals)} individuals). The OWL source is canonical.",
        "",
        "## Classes",
        "",
    ]
    for t in classes:
        lines.append(f"### {t.label or t.curie} (`{t.curie}`)")
        if t.definition:
            lines.append(f"\n{t.definition}")
        if t.parents:
            lines.append(f"\n*Subclass of:* {', '.join(f'`{p}`' for p in t.parents)}")
        if t.alignments:
            lines.append(f"\n*Aligns:* {', '.join(f'`{a}`' for a in t.alignments)}")
        lines.append("")
    lines += ["## Properties", ""]
    for t in properties:
        lines.append(f"### {t.label or t.curie} (`{t.curie}`)")
        if t.definition:
            lines.append(f"\n{t.definition}")
        meta = f"*{t.prop_kind} property*"
        if t.domain or t.range:
            meta += f" — `{t.domain or '?'}` → `{t.range or '?'}`"
        if t.functional:
            meta += " (functional)"
        lines.append(f"\n{meta}")
        if t.alignments:
            lines.append(f"\n*Aligns:* {', '.join(f'`{a}`' for a in t.alignments)}")
        lines.append("")
    if individuals:
        lines += ["## Individuals", ""]
        for t in individuals:
            lines.append(f"### {t.label or t.curie} (`{t.curie}`)")
            if t.definition:
                lines.append(f"\n{t.definition}")
            if t.types:
                lines.append(f"\n*Type:* {', '.join(f'`{x}`' for x in t.types)}")
            lines.append("")
    path = dist_dir / "gmeow-terms.md"
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return path


def write_llms_txt(terms: list[Term], dist_dir: Path) -> Path:
    """Write an ``llms.txt``-style single-file bundle of the whole vocabulary."""
    classes = [t for t in terms if t.category == "class"]
    properties = [t for t in terms if t.category == "property"]
    individuals = [t for t in terms if t.category == "individual"]
    lines = [
        f"# {TITLE}",
        "",
        "> A reasoning-centric, OWL 2 DL, gUFO-grounded super-vocabulary that "
        "unifies a person's or organization's digital existence (entities, "
        "contacts, email, trust/keys, time) and aligns it to schema.org, FOAF, "
        "PROV, the WOT schema, Wikidata, and more.",
        "",
        f"Vocabulary {VERSION}. Namespace: {NAMESPACE}. Each term below is "
        "`curie` — definition; the OWL source is canonical.",
        "",
        "## Classes",
        "",
    ]
    for t in classes:
        sub = f" (⊑ {', '.join(t.parents)})" if t.parents else ""
        lines.append(f"- {t.curie}{sub}: {t.definition or t.label}")
    lines += ["", "## Properties", ""]
    for t in properties:
        sig = (
            f" [{t.domain or '?'} → {t.range or '?'}]" if (t.domain or t.range) else ""
        )
        fn = " (functional)" if t.functional else ""
        lines.append(f"- {t.curie}{sig}{fn}: {t.definition or t.label}")
    if individuals:
        lines += ["", "## Individuals", ""]
        for t in individuals:
            types = f" (a {', '.join(t.types)})" if t.types else ""
            lines.append(f"- {t.curie}{types}: {t.definition or t.label}")
    path = dist_dir / "llms.txt"
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return path


def export_all(dist_dir: Path = DIST_DIR) -> list[Path]:
    """Generate every flattened export view into ``dist_dir``.

    Args:
        dist_dir: Target directory (created if absent).

    Returns:
        The list of written paths.
    """
    dist_dir.mkdir(parents=True, exist_ok=True)
    terms = collect_terms()
    written = write_csvs(terms, dist_dir)
    written.append(write_csvw(dist_dir))
    written.append(write_jsonl(terms, dist_dir))
    written.append(write_markdown(terms, dist_dir))
    written.append(write_llms_txt(terms, dist_dir))
    return written
