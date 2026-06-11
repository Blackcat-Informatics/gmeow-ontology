"""Flattened, broadly-consumable export views of the GMEOW vocabulary.

The canonical artifact is the OWL 2 DL ontology; this module projects it into
plainer, lossy-but-useful views for consumers that don't speak RDF/OWL:

* **CSV term dictionaries** (classes, properties, individuals) — the simple
  tabular view, plus a **CSVW** descriptor so the tables are self-describing;
* a **Markdown term reference** — human-readable and diffable;
* a **JSONL term catalog** — one record per term, for tooling / RAG / embeddings;
* an **``llms.txt`` bundle** — the whole vocabulary in one LLM-ingestable file.

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
