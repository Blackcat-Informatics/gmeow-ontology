"""Flattened, broadly-consumable export views of the GMEOW vocabulary.

The canonical artifact is the OWL 2 DL ontology; this module projects it into
plainer, lossy-but-useful views for consumers that don't speak RDF/OWL:

* **CSV term dictionaries** (classes, properties, individuals) — the simple
  tabular view, plus a **CSVW** descriptor so the tables are self-describing;
* a **Markdown term reference** — human-readable and diffable;
* a **JSONL term catalog** — one record per term, for tooling / RAG / embeddings;
* an **``llms.txt`` bundle** — the whole vocabulary in one LLM-ingestable file;
* the **dataset forms** (#377): lossless N-Quads/TriG 1.2 (statement layer
  included), the **statements JSONL** AI bundle, and lossy-but-declared
  **SKOS** / **OBO Graphs** / **ShEx** projections.

Everything is generated from the committed GTS snapshot (the narrow waist,
#267): the asserted merged graph in its default graph plus the SSSOM
alignment axioms in their named graph — no reasoning, no Docker, no RDF
parser in this module at all.
These views flatten reified relators and the RDF-star validity/provenance layer;
they are an entry point to the vocabulary, not a substitute for the OWL source.
"""

from __future__ import annotations

import csv
import json
from collections.abc import Iterable, Sequence
from dataclasses import dataclass, field
from functools import lru_cache
from pathlib import Path

from gmeow_tools.config import (
    DIST_DIR,
    GTS_GRAPH_ALIGNMENTS,
    GTS_SNAPSHOT_FILE,
    NAMESPACE,
    ONTOLOGY_IRI,
    PREFIXES,
    PROJECT_ROOT,
)
from gmeow_tools.generator import Generator, _rel, register
from gmeow_tools.gts_views import FoldView, load_fold
from gts.nquads import term_token, to_nquads

_OWL = "http://www.w3.org/2002/07/owl#"
_RDFS = "http://www.w3.org/2000/01/rdf-schema#"
_SKOS = "http://www.w3.org/2004/02/skos/core#"


def _resolve_meta(title: str | None, version: str | None) -> tuple[str, str]:
    """Explicit meta when given, else the snapshot's (cached)."""
    if title is not None and version is not None:
        return title, version
    default_title, default_version = _default_meta()
    return title or default_title, version or default_version


@lru_cache(maxsize=1)
def _default_meta() -> tuple[str, str]:
    return fold_meta(load_fold())


#: Property rdf:type IRI → short kind label.
_PROPERTY_KINDS: dict[str, str] = {
    _OWL + "ObjectProperty": "object",
    _OWL + "DatatypeProperty": "datatype",
    _OWL + "AnnotationProperty": "annotation",
}

#: Alignment predicate IRI → short relation tag for the flattened views.
_ALIGN_TAGS: dict[str, str] = {
    _OWL + "equivalentClass": "equivalentClass",
    _OWL + "equivalentProperty": "equivalentProperty",
    _RDFS + "subClassOf": "subClassOf",
    _RDFS + "subPropertyOf": "subPropertyOf",
    _SKOS + "closeMatch": "closeMatch",
    _SKOS + "exactMatch": "exactMatch",
    _SKOS + "relatedMatch": "relatedMatch",
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


# --------------------------------------------------------------------------- #
# The fold path (narrow waist #267): everything below reads ONLY the snapshot
# --------------------------------------------------------------------------- #

_RDF_TYPE = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
_OWL_UNION = _OWL + "unionOf"
_OWL_INTERSECTION = _OWL + "intersectionOf"
_DCT_TITLE = "http://purl.org/dc/terms/title"
_OWL_VERSION_INFO = _OWL + "versionInfo"


def fold_meta(view: FoldView) -> tuple[str, str]:
    """The vocabulary title and version, from the snapshot's ontology header."""
    onto = view.tid_of_iri(ONTOLOGY_IRI)
    if onto is None:
        msg = f"ontology header {ONTOLOGY_IRI} not present in the snapshot"
        raise ValueError(msg)
    title_tid = view.value(onto, _DCT_TITLE)
    version_tid = view.value(onto, _OWL_VERSION_INFO)
    if title_tid is None or version_tid is None:
        msg = "ontology header lacks dcterms:title / owl:versionInfo"
        raise ValueError(msg)
    return view.lex(title_tid), view.lex(version_tid)


def _fold_curies(view: FoldView, s_tid: int, p_iri: str) -> list[str]:
    """Named object IRIs as sorted CURIEs (anonymous nodes never leak)."""
    return sorted(
        curie(view.lex(o)) for o in view.objects(s_tid, p_iri) if view.is_iri(o)
    )


def _fold_alignments(view: FoldView, s_tid: int) -> list[str]:
    """``tag=curie`` alignment strings from the snapshot's alignments graph."""
    out: list[str] = []
    for p, o in view.predicate_objects(s_tid, scope=GTS_GRAPH_ALIGNMENTS):
        tag = _ALIGN_TAGS.get(view.lex(p), curie(view.lex(p)))
        out.append(f"{tag}={curie(view.lex(o))}")
    return sorted(out)


def _fold_describe_node(view: FoldView, tid: int) -> str:
    """A CURIE or a serialized union/intersection description of a class node."""
    if view.is_iri(tid):
        return curie(view.lex(tid))
    if view.is_bnode(tid):
        union_head = view.value(tid, _OWL_UNION)
        if union_head is not None:
            return " | ".join(
                _fold_describe_node(view, item) for item in view.rdf_list(union_head)
            )
        intersection_head = view.value(tid, _OWL_INTERSECTION)
        if intersection_head is not None:
            return " & ".join(
                _fold_describe_node(view, item)
                for item in view.rdf_list(intersection_head)
            )
    return view.lex(tid)


def collect_terms(view: FoldView | None = None) -> list[Term]:
    """Collect every GMEOW class, property, and named individual as a Term.

    Reads ONLY the GTS snapshot (the narrow waist): vocabulary from the
    default graph, alignments from the alignments named graph, public text
    via the fold-side language boundary.

    Args:
        view: A fold view (defaults to loading the committed snapshot).

    Returns:
        Terms sorted by (category, CURIE).
    """
    if view is None:
        view = load_fold()

    def in_namespace(tid: int) -> bool:
        return view.is_iri(tid) and view.lex(tid).startswith(NAMESPACE)

    classes = {t for t in view.subjects_by_type(_OWL + "Class") if in_namespace(t)}
    properties: dict[int, str] = {}
    for ptype, kind in _PROPERTY_KINDS.items():
        for t in view.subjects_by_type(ptype):
            if in_namespace(t):
                properties[t] = kind

    label_iri, definition_iri = _RDFS + "label", _SKOS + "definition"
    functional_tid = view.tid_of_iri(_OWL + "FunctionalProperty")

    terms: list[Term] = []
    for t in classes:
        terms.append(
            Term(
                category="class",
                iri=view.lex(t),
                curie=curie(view.lex(t)),
                label=view.public_text(t, label_iri),
                definition=view.public_text(t, definition_iri),
                parents=_fold_curies(view, t, _RDFS + "subClassOf"),
                alignments=_fold_alignments(view, t),
            )
        )

    for t, kind in properties.items():
        domain_tid = view.value(t, _RDFS + "domain")
        range_tid = view.value(t, _RDFS + "range")
        terms.append(
            Term(
                category="property",
                iri=view.lex(t),
                curie=curie(view.lex(t)),
                label=view.public_text(t, label_iri),
                definition=view.public_text(t, definition_iri),
                prop_kind=kind,
                domain=(
                    _fold_describe_node(view, domain_tid)
                    if domain_tid is not None
                    else ""
                ),
                range=(
                    _fold_describe_node(view, range_tid)
                    if range_tid is not None
                    else ""
                ),
                functional=(
                    functional_tid is not None
                    and view.has(t, _RDF_TYPE, functional_tid)
                ),
                sub_property_of=_fold_curies(view, t, _RDFS + "subPropertyOf"),
                alignments=_fold_alignments(view, t),
            )
        )

    declared = classes | set(properties)
    seen: set[int] = set()
    for cls in classes:
        for t in view.subjects_by_type(view.lex(cls)):
            if not in_namespace(t) or t in declared or t in seen:
                continue
            seen.add(t)
            terms.append(
                Term(
                    category="individual",
                    iri=view.lex(t),
                    curie=curie(view.lex(t)),
                    label=view.public_text(t, label_iri),
                    definition=view.public_text(t, definition_iri),
                    types=sorted(
                        curie(view.lex(o))
                        for o in view.objects(t, _RDF_TYPE)
                        if o in classes
                    ),
                    alignments=_fold_alignments(view, t),
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


def write_csvw(dist_dir: Path, *, title: str | None = None) -> Path:
    """Write a CSVW (tabular metadata) descriptor for the term CSVs."""

    def table(url: str, columns: list[str]) -> dict[str, object]:
        return {
            "url": url,
            "tableSchema": {"columns": [{"name": c, "titles": c} for c in columns]},
        }

    descriptor: dict[str, object] = {
        "@context": "http://www.w3.org/ns/csvw",
        "dc:title": f"{_resolve_meta(title, None)[0]} — term dictionaries",
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


def write_markdown(
    terms: list[Term],
    dist_dir: Path,
    *,
    title: str | None = None,
    version: str | None = None,
) -> Path:
    """Write a human-readable Markdown term reference."""
    classes = [t for t in terms if t.category == "class"]
    properties = [t for t in terms if t.category == "property"]
    individuals = [t for t in terms if t.category == "individual"]
    resolved_title, resolved_version = _resolve_meta(title, version)
    lines = [
        f"# {resolved_title} — term reference",
        "",
        f"Generated from the GMEOW {resolved_version} vocabulary "
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


def write_llms_txt(
    terms: list[Term],
    dist_dir: Path,
    *,
    title: str | None = None,
    version: str | None = None,
) -> Path:
    """Write an ``llms.txt``-style single-file bundle of the whole vocabulary."""
    classes = [t for t in terms if t.category == "class"]
    properties = [t for t in terms if t.category == "property"]
    individuals = [t for t in terms if t.category == "individual"]
    resolved_title, resolved_version = _resolve_meta(title, version)
    lines = [
        f"# {resolved_title}",
        "",
        "> A reasoning-centric, OWL 2 DL, gUFO-grounded super-vocabulary that "
        "unifies a person's or organization's digital existence (entities, "
        "contacts, email, trust/keys, time) and aligns it to schema.org, FOAF, "
        "PROV, the WOT schema, Wikidata, and more.",
        "",
        f"Vocabulary {resolved_version}. Namespace: {NAMESPACE}. "
        "Each term below is "
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


# --------------------------------------------------------------------------- #
# Dataset / semantic-web tiers (#377, #12): still fold-only shims
# --------------------------------------------------------------------------- #

_RDF_REIFIES = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies"
_SKOS_MATCHES = (_SKOS + "exactMatch", _SKOS + "closeMatch", _SKOS + "relatedMatch")


def write_nquads(view: FoldView, dist_dir: Path) -> Path:
    """Write the complete dataset as N-Quads 1.2 (``gmeow.nq``).

    LOSSLESS: every quad in every graph PLUS the reifier bindings and
    statement annotations — ``gts.nquads.to_nquads`` is the one writer for
    this form (P4); internal language tags are remapped to public BCP-47 at
    this projection boundary (#287).
    """
    path = dist_dir / "gmeow.nq"
    path.write_text(to_nquads(view.graph, view.tag_map()), encoding="utf-8")
    return path


def write_trig(view: FoldView, dist_dir: Path) -> Path:
    """Write the dataset as TriG 1.2 (``gmeow.trig``).

    Same content as ``gmeow.nq`` (LOSSLESS, same term renderer): the default
    graph carries the ontology base plus the reifier/annotation statement
    layer; the statements and alignments named graphs become labeled blocks.
    """
    g = view.graph
    lang_map = view.tag_map()

    def tok(tid: int) -> str:
        return term_token(g, tid, lang_map)

    default_lines: list[str] = []
    named: dict[str, list[str]] = {}
    for s, p, o, gname in g.quads:
        line = f"{tok(s)} {tok(p)} {tok(o)} ."
        if gname is None:
            default_lines.append(line)
        else:
            named.setdefault(g.terms[gname].value or "", []).append(line)
    for rid, (s, p, o) in g.reifiers.items():
        quoted = f"<<( {tok(s)} {tok(p)} {tok(o)} )>>"
        default_lines.append(f"{tok(rid)} <{_RDF_REIFIES}> {quoted} .")
    for r, p, v in g.annotations:
        default_lines.append(f"{tok(r)} {tok(p)} {tok(v)} .")

    blocks = [
        "# The GMEOW dataset as TriG 1.2 — same content as gmeow.nq (lossless).",
        "# Default graph: ontology base + RDF 1.2 statement layer;",
        "# named graphs: statements / alignments partitions.",
        "",
        *default_lines,
    ]
    for graph_iri in sorted(named):
        blocks += ["", f"<{graph_iri}> {{"]
        blocks += [f"    {line}" for line in named[graph_iri]]
        blocks.append("}")
    path = dist_dir / "gmeow.trig"
    path.write_text("\n".join(blocks) + "\n", encoding="utf-8")
    return path


def _public_value(view: FoldView, tid: int) -> object:
    """``python_value`` with language tags mapped to public BCP-47 (#287)."""
    val = view.python_value(tid)
    if isinstance(val, dict) and "lang" in val:
        tag_map = view.tag_map()
        lang = str(val["lang"])
        return {"value": val["value"], "lang": tag_map.get(lang, lang)}
    return val


def write_statements_jsonl(view: FoldView, dist_dir: Path) -> Path:
    """Write the reified statement layer as flat JSONL (``gmeow-statements.jsonl``).

    One reified statement per line — subject/predicate/object as CURIEs or
    scalars plus its annotation map (confidence, assertedAt, accordingTo, …):
    the AI-bundle companion to ``gmeow-terms.jsonl`` and the same flat shape
    family as ``gmeow audit --json`` (P13 — no RDF required of consumers).
    """
    grouped: dict[int, dict[str, list[object]]] = {}
    for r, p, v in view.annotations():
        key = view.curie(view.lex(p))
        grouped.setdefault(r, {}).setdefault(key, []).append(_public_value(view, v))

    rows: list[str] = []
    for rid, (s, p, o) in sorted(
        view.reifiers().items(), key=lambda kv: view.nq_token(kv[0])
    ):
        annotations = {
            key: values[0] if len(values) == 1 else sorted(values, key=str)
            for key, values in sorted(grouped.get(rid, {}).items())
        }
        record = {
            "id": view.python_value(rid),
            "subject": _public_value(view, s),
            "predicate": view.curie(view.lex(p)),
            "object": _public_value(view, o),
            "annotations": annotations,
        }
        rows.append(json.dumps(record, ensure_ascii=False))
    path = dist_dir / "gmeow-statements.jsonl"
    path.write_text("\n".join(rows) + "\n", encoding="utf-8")
    return path


def _ttl_literal(text: str, lang: str | None = None) -> str:
    """A Turtle literal token (JSON string escaping is valid Turtle ECHAR/UCHAR)."""
    lit = json.dumps(text, ensure_ascii=False)
    return f"{lit}@{lang}" if lang else lit


def write_skos(
    view: FoldView,
    dist_dir: Path,
    *,
    title: str | None = None,
    version: str | None = None,
) -> Path:
    """Write the SKOS extract (``gmeow-skos.ttl``) — classes as a concept scheme.

    LOSSY BY DESIGN (declared): classes only, typed ``skos:Concept`` on their
    ORIGINAL IRIs; ``subClassOf`` within the namespace becomes ``skos:broader``;
    SKOS mapping rows are carried from the alignments graph. All OWL axioms,
    properties, and individuals are dropped. STANDALONE projection: merging it
    with the OWL form puns every class as an individual — don't.
    """
    resolved_title, resolved_version = _resolve_meta(title, version)
    classes = sorted(
        (
            t
            for t in view.subjects_by_type(_OWL + "Class")
            if view.is_iri(t) and view.lex(t).startswith(NAMESPACE)
        ),
        key=lambda t: curie(view.lex(t)),
    )
    class_iris = {view.lex(t) for t in classes}

    lines = [
        "# The GMEOW vocabulary as a SKOS concept scheme — a LOSSY projection:",
        "# classes only (typed skos:Concept on their original IRIs);",
        "# subClassOf → skos:broader; SKOS mapping rows carried from the",
        "# alignments graph. OWL axioms, properties, and individuals are",
        "# dropped. STANDALONE: never merge with the OWL form (class punning).",
        "@prefix skos: <http://www.w3.org/2004/02/skos/core#> .",
        "@prefix dcterms: <http://purl.org/dc/terms/> .",
        "@prefix owl: <http://www.w3.org/2002/07/owl#> .",
        f"@prefix gmeow: <{NAMESPACE}> .",
        "",
        f"<{ONTOLOGY_IRI}> a skos:ConceptScheme ;",
        f"    dcterms:title {_ttl_literal(resolved_title + ' — SKOS extract')} ;",
        f"    owl:versionInfo {_ttl_literal(resolved_version)} .",
    ]

    top_concepts: list[str] = []
    bodies: list[str] = []
    for t in classes:
        c = curie(view.lex(t))
        label, label_lang = view.public_literal(t, _RDFS + "label")
        definition, definition_lang = view.public_literal(t, _SKOS + "definition")
        broader = sorted(
            curie(view.lex(o))
            for o in view.objects(t, _RDFS + "subClassOf")
            if view.is_iri(o) and view.lex(o) in class_iris
        )
        if not broader:
            top_concepts.append(c)
        stanza = [f"{c} a skos:Concept ;", f"    skos:inScheme <{ONTOLOGY_IRI}> ;"]
        if label:
            stanza.append(f"    skos:prefLabel {_ttl_literal(label, label_lang)} ;")
        if definition:
            stanza.append(
                f"    skos:definition {_ttl_literal(definition, definition_lang)} ;"
            )
        for b in broader:
            stanza.append(f"    skos:broader {b} ;")
        for p, o in view.predicate_objects(t, scope=GTS_GRAPH_ALIGNMENTS):
            if view.lex(p) in _SKOS_MATCHES and view.is_iri(o):
                tag = view.lex(p).rsplit("#", 1)[-1]
                stanza.append(f"    skos:{tag} <{view.lex(o)}> ;")
        stanza[-1] = stanza[-1].rstrip(" ;") + " ."
        bodies += ["", *stanza]

    for c in top_concepts:
        lines.append(f"<{ONTOLOGY_IRI}> skos:hasTopConcept {c} .")
    path = dist_dir / "gmeow-skos.ttl"
    path.write_text("\n".join(lines + bodies) + "\n", encoding="utf-8")
    return path


def write_obographs(
    view: FoldView,
    dist_dir: Path,
    *,
    version: str | None = None,
) -> Path:
    """Write the class hierarchy as OBO Graphs JSON (``gmeow-obographs.json``).

    LOSSY BY DESIGN (declared in the graph's meta): GMEOW classes as nodes
    (label + definition), IRI-to-IRI ``rdfs:subClassOf`` as ``is_a`` edges;
    blank-node restrictions, properties, and individuals are dropped.
    External parents appear as bare nodes so no edge dangles.
    """
    _, resolved_version = _resolve_meta(None, version)
    label_iri, definition_iri = _RDFS + "label", _SKOS + "definition"
    classes = sorted(
        (
            t
            for t in view.subjects_by_type(_OWL + "Class")
            if view.is_iri(t) and view.lex(t).startswith(NAMESPACE)
        ),
        key=view.lex,
    )
    nodes: list[dict[str, object]] = []
    edges: list[dict[str, str]] = []
    for t in classes:
        iri = view.lex(t)
        node: dict[str, object] = {"id": iri, "type": "CLASS"}
        label = view.public_text(t, label_iri)
        if label:
            node["lbl"] = label
        definition = view.public_text(t, definition_iri)
        if definition:
            node["meta"] = {"definition": {"val": definition}}
        nodes.append(node)
        edges += [
            {"sub": iri, "pred": "is_a", "obj": view.lex(o)}
            for o in view.objects(t, _RDFS + "subClassOf")
            if view.is_iri(o)
        ]
    known = {str(n["id"]) for n in nodes}
    for iri in sorted({e["obj"] for e in edges} - known):
        nodes.append({"id": iri, "type": "CLASS"})

    doc = {
        "graphs": [
            {
                "id": ONTOLOGY_IRI,
                "meta": {
                    "version": resolved_version,
                    "basicPropertyValues": [
                        {
                            "pred": "http://www.w3.org/2000/01/rdf-schema#comment",
                            "val": (
                                "LOSSY projection: GMEOW classes and IRI-only "
                                "is_a edges; blank-node restrictions, properties, "
                                "and individuals are dropped. The OWL source is "
                                "canonical."
                            ),
                        }
                    ],
                },
                "nodes": nodes,
                "edges": edges,
            }
        ]
    }
    path = dist_dir / "gmeow-obographs.json"
    path.write_text(
        json.dumps(doc, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    return path


def _shex_domains(view: FoldView, prop: int, class_iris: set[str]) -> list[str]:
    """The named in-namespace domain classes of a property, unions expanded."""
    out: set[str] = set()
    for d in view.objects(prop, _RDFS + "domain"):
        if view.is_iri(d):
            candidates = [d]
        elif view.is_bnode(d):
            union_head = view.value(d, _OWL_UNION)
            candidates = view.rdf_list(union_head) if union_head is not None else []
        else:
            candidates = []
        out.update(
            view.lex(c)
            for c in candidates
            if view.is_iri(c) and view.lex(c) in class_iris
        )
    return sorted(out)


def write_shex(view: FoldView, dist_dir: Path) -> Path:
    """Write ShEx shapes (``gmeow.shex``) — one shape per domained class.

    LOSSY BY DESIGN (declared): a shape per GMEOW class that is the (named or
    union-expanded) domain of at least one object/datatype property; range
    IRIs become value-shape references (in-namespace classes) or node-kind /
    datatype constraints; functional → ``?``, else ``*``. OWL restrictions,
    pure blank-node domains, and annotation properties are not translated.
    """
    class_iris = {
        view.lex(t)
        for t in view.subjects_by_type(_OWL + "Class")
        if view.is_iri(t) and view.lex(t).startswith(NAMESPACE)
    }
    functional_tid = view.tid_of_iri(_OWL + "FunctionalProperty")

    per_class: dict[str, list[str]] = {}
    for ptype, kind in (
        (_OWL + "ObjectProperty", "object"),
        (_OWL + "DatatypeProperty", "datatype"),
    ):
        for prop in view.subjects_by_type(ptype):
            if not view.lex(prop).startswith(NAMESPACE):
                continue
            range_tid = view.value(prop, _RDFS + "range")
            if range_tid is not None and view.is_iri(range_tid):
                range_iri = view.lex(range_tid)
                if range_iri in class_iris:
                    value_expr = f"@{curie(range_iri)}"
                elif kind == "datatype":
                    value_expr = curie(range_iri)
                else:
                    value_expr = "IRI"
            else:
                value_expr = "IRI" if kind == "object" else "LITERAL"
            card = (
                "?"
                if functional_tid is not None
                and view.has(prop, _RDF_TYPE, functional_tid)
                else "*"
            )
            constraint = f"{curie(view.lex(prop))} {value_expr} {card}"
            for domain_iri in _shex_domains(view, prop, class_iris):
                per_class.setdefault(curie(domain_iri), []).append(constraint)

    lines = [
        "# ShEx shapes for the GMEOW vocabulary — a LOSSY projection:",
        "# one shape per class that is the (named or union-expanded) domain of",
        "# an object/datatype property; functional → '?', else '*'. OWL",
        "# restrictions, pure blank-node domains, and annotation properties",
        "# are not translated. The OWL source is canonical.",
        "PREFIX gmeow: <" + NAMESPACE + ">",
        "PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>",
        "PREFIX gufo: <http://purl.org/nemo/gufo#>",
        "",
    ]
    for cls in sorted(per_class):
        lines.append(f"{cls} {{")
        for constraint in sorted(set(per_class[cls])):
            lines.append(f"    {constraint} ;")
        lines[-1] = lines[-1].rstrip(" ;")
        lines += ["}", ""]
    path = dist_dir / "gmeow.shex"
    path.write_text("\n".join(lines).rstrip("\n") + "\n", encoding="utf-8")
    return path


# --------------------------------------------------------------------------- #
# Registered generator
# --------------------------------------------------------------------------- #


@register
class ExportGenerator(Generator):
    """Generate flattened export views (CSV/CSVW, Markdown, JSONL, llms.txt)."""

    name: str = "exports"

    _cached_inputs: Sequence[Path] | None = None

    @property
    def inputs(self) -> Sequence[Path]:
        """Canonical inputs for the export generator."""
        if self._cached_inputs is not None:
            return self._cached_inputs
        self._cached_inputs = [GTS_SNAPSHOT_FILE]
        return self._cached_inputs

    @property
    def outputs(self) -> Sequence[Path]:
        """Committed outputs for the export generator."""
        return [
            DIST_DIR / "gmeow-classes.csv",
            DIST_DIR / "gmeow-properties.csv",
            DIST_DIR / "gmeow-individuals.csv",
            DIST_DIR / "gmeow-terms.csvw.json",
            DIST_DIR / "gmeow-terms.jsonl",
            DIST_DIR / "gmeow-terms.md",
            DIST_DIR / "llms.txt",
            DIST_DIR / "gmeow.nq",
            DIST_DIR / "gmeow.trig",
            DIST_DIR / "gmeow-statements.jsonl",
            DIST_DIR / "gmeow-skos.ttl",
            DIST_DIR / "gmeow-obographs.json",
            DIST_DIR / "gmeow.shex",
        ]

    def render(self, staging: Path) -> None:
        """Render flattened export views from the GTS snapshot."""
        out_dir = staging / DIST_DIR.relative_to(PROJECT_ROOT)
        out_dir.mkdir(parents=True, exist_ok=True)
        view = load_fold()
        title, version = fold_meta(view)
        terms = collect_terms(view)
        write_csvs(terms, out_dir)
        write_csvw(out_dir, title=title)
        write_jsonl(terms, out_dir)
        write_markdown(terms, out_dir, title=title, version=version)
        write_llms_txt(terms, out_dir, title=title, version=version)
        write_nquads(view, out_dir)
        write_trig(view, out_dir)
        write_statements_jsonl(view, out_dir)
        write_skos(view, out_dir, title=title, version=version)
        write_obographs(view, out_dir, version=version)
        write_shex(view, out_dir)

    def compare(self, fresh: Path, committed: Path) -> list[str]:
        """Skip drift for git-ignored export artifacts."""
        if not committed.exists():
            return []
        if not fresh.exists():
            rel = _rel(committed)
            return [f"{rel} (not produced in staging)"]
        if fresh.read_bytes() != committed.read_bytes():
            rel = _rel(committed)
            return [f"{rel}"]
        return []
