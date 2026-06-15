# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Up-projection invertibility audit (#449).

Measures how much of the consumer-vocabulary → GMEOW *up-projection* the existing
alignment / FnO / projection machinery already gives us when read backwards, and
where the real work is. Two cell populations are classified:

* **SSSOM alignment cells** (term ↔ term): reversibility follows the relation —
  ``exactMatch``/``equivalent*`` reverse cleanly; ``closeMatch`` lifts *with a
  provenance-stamped claim*; ``relatedMatch``/``narrowMatch`` are down-only.
* **ProjectionMapping cells** (structural CONSTRUCTs): a plain ``toClass``/
  ``toPredicate`` reverses mechanically; ``mint`` cells need a hand-authored
  inverse; ``path``/``filter``/multi-leg cells invert with care.

The headline is the **real-data baseline**: of the target terms actually used in
the vendored ``bii``/``paudley`` snapshots, how many have a liftable path. The
input is fixed real RDF, so the number cannot be moved by authoring fixtures —
only by extending GMEOW or its cells.
"""

from __future__ import annotations

from collections.abc import Iterator
from dataclasses import dataclass, field
from pathlib import Path

from rdflib import RDF, Graph, Namespace, URIRef
from rdflib.term import Node

from gmeow_tools.config import (
    FIXTURES_DIR,
    MAPPING_DSL_DIR,
    MAPPINGS_DIR,
    PREFIXES,
    SLICES_DIR,
)

GM = Namespace("https://blackcatinformatics.ca/gmeow/")
_GM_PFX = "gmeow:"

#: Consumer vocabularies we actually project to — the interop surface that turns
#: up in real source files. Alignments to anything else (Wikidata, Brick, LCSH …)
#: are reference/authority links, not up-projection targets.
PROJECTION_PREFIXES: frozenset[str] = frozenset(
    {
        "schema",
        "foaf",
        "doap",
        "vcard",
        "vcardx",
        "org",
        "time",
        "sioc",
        "bibo",
        "bf",
        "gedcom",
        "rel",
        "cc",
        "odrl",
        "dcterms",
        "dc",
        "spdx",
        "prov",
        "geo",
        "geosparql",
        "sosa",
        "skos",
        "ical",
        "oa",
        "iiif",
        "exif",
        "wgs84",
        "mads",
        "codemeta",
    }
)

#: FnO transforms whose value map is invertible (compositional / geometric /
#: mathematical). Everything else is a lossy reduction or a computed score and
#: cannot be inverted for up-projection.
INVERTIBLE_FN: frozenset[str] = frozenset(
    {
        "fnComposeAddress",
        "fnComposeBcp",
        "fnComposeMailmapMapping",
        "fnLatLongToWktPoint",
        "fnLatLongToGeoUri",
        "fnRatioToCents",
        "fnPitchToFrequency",
    }
)

# Up-projection buckets, ranked best→worst for "best available path" selection.
_SSSOM_RANK = {
    "clean-reversible": 3,
    "liftable-generalizing": 2,
    "liftable-with-claim": 1,
}
_STRUCT_RANK = {
    "simple-1to1": 3,
    "structural-guarded": 2,
    "structural-multileg": 2,
    "structural-mint": 1,
}
_SSSOM_LIFTABLE = frozenset(_SSSOM_RANK)
_STRUCT_LIFTABLE = frozenset(_STRUCT_RANK) - {"structural-mint"}

_CORPUS = ("bii", "paudley")

#: Full namespace IRIs we project to, resolved from PROJECTION_PREFIXES. Matching
#: is by IRI (not qname), so the SAME term under different prefixes — e.g. the
#: geosparql namespace as ``geo:`` in our cells but ``geosparql:`` in a real file
#: — is recognised as one term, never a false gap.
PROJECTION_NS: frozenset[str] = frozenset(
    PREFIXES[p] for p in PROJECTION_PREFIXES if p in PREFIXES
)
#: Prefixes sorted by descending namespace length, for stable canonical qnames.
_CANON_PREFIXES: tuple[tuple[str, str], ...] = tuple(
    sorted(PREFIXES.items(), key=lambda kv: -len(kv[1]))
)


def _to_iri(curie: str) -> str:
    """Resolve a ``prefix:local`` curie to its full IRI via the canonical prefixes.

    Anything already an IRI or carrying an unknown prefix is passed through.
    """
    if ":" not in curie or curie.startswith(("http://", "https://")):
        return curie
    pfx, local = curie.split(":", 1)
    ns = PREFIXES.get(pfx)
    return f"{ns}{local}" if ns else curie


def _in_projection_ns(iri: str) -> bool:
    return any(iri.startswith(ns) for ns in PROJECTION_NS)


def _canon_qname(iri: str) -> str:
    """Canonical display qname for an IRI (longest matching namespace wins)."""
    for pfx, ns in _CANON_PREFIXES:
        if iri.startswith(ns):
            return f"{pfx}:{iri[len(ns) :]}"
    return iri


def _prefix(term: str) -> str:
    return term.split(":", 1)[0] if ":" in term else ""


# --------------------------------------------------------------------------- #
# SSSOM layer
# --------------------------------------------------------------------------- #


def _parse_sssom_text(text: str) -> list[dict[str, str]]:
    """Parse SSSOM TSV *text* into rows (the source-agnostic core)."""
    rows: list[dict[str, str]] = []
    header: list[str] | None = None
    for line in text.splitlines():
        if line.startswith("#") or not line.strip():
            continue
        cells = line.split("\t")
        if header is None:
            header = cells
            continue
        row = dict(zip(header, cells, strict=False))
        if row.get("subject_id") and row.get("predicate_id") and row.get("object_id"):
            rows.append(row)
    return rows


def _read_sssom(path: Path) -> list[dict[str, str]]:
    return _parse_sssom_text(path.read_text(encoding="utf-8"))


def iter_sssom_records() -> list[dict[str, str]]:
    """Every SSSOM row, from the repo source tree or, failing that, the bundle.

    The dev fast-path reads ``generated/mappings/*.sssom.tsv`` directly; a
    wheel-only install (no source tree) falls back to the SSSOM files folded into
    the bundle (#bundle — the CLI razor: ``gmeow`` needs no repo).
    """
    from gmeow_tools.bundle import bundled_sssom

    files = sorted(MAPPINGS_DIR.glob("*.sssom.tsv"))
    if files:
        rows: list[dict[str, str]] = []
        for path in files:
            rows.extend(_read_sssom(path))
        return rows
    return [
        row
        for _name, data in sorted(bundled_sssom().items())
        for row in _parse_sssom_text(data.decode("utf-8"))
    ]


def classify_sssom(subj: str, pred: str, obj: str) -> tuple[str, str, str]:
    """Classify one SSSOM row for up-projection → (bucket, gmeow_term, target)."""
    if subj.startswith(_GM_PFX) and not obj.startswith(_GM_PFX):
        gmeow, target, gmeow_is_subject = subj, obj, True
    elif obj.startswith(_GM_PFX) and not subj.startswith(_GM_PFX):
        gmeow, target, gmeow_is_subject = obj, subj, False
    else:
        return ("both-or-neither-gmeow", subj, obj)

    rel = pred.split(":", 1)[-1]
    if rel in ("exactMatch", "equivalentClass", "equivalentProperty", "sameAs"):
        bucket = "clean-reversible"
    elif rel == "closeMatch":
        bucket = "liftable-with-claim"
    elif rel in ("broadMatch", "narrowMatch"):
        # subject broadMatch object ⇒ subject is broader. Lifting target→gmeow is
        # faithful only when gmeow is the broader side (target ⊆ gmeow).
        gmeow_is_broader = gmeow_is_subject == (rel == "broadMatch")
        bucket = "liftable-generalizing" if gmeow_is_broader else "down-only-narrowing"
    elif rel in ("relatedMatch", "subClassOf"):
        bucket = "down-only-related"
    else:
        bucket = f"other:{rel}"
    return (bucket, gmeow, target)


def sssom_best_buckets() -> dict[str, str]:
    """Target IRI → best up-bucket across all SSSOM cells (projection targets)."""
    best: dict[str, str] = {}
    for row in iter_sssom_records():  # repo OR bundle (#bundle — never undercounts)
        bucket, _gmeow, target = classify_sssom(
            row["subject_id"], row["predicate_id"], row["object_id"]
        )
        # a non-gmeow↔gmeow row contributes no up-bucket; skip it so its
        # rank-0 bucket can't mask a real down-only classification → false GAP
        if bucket == "both-or-neither-gmeow":
            continue
        iri = _to_iri(target)
        if not _in_projection_ns(iri):
            continue
        cur = best.get(iri)
        if cur is None or _SSSOM_RANK.get(bucket, 0) > _SSSOM_RANK.get(cur, 0):
            best[iri] = bucket
    return best


# --------------------------------------------------------------------------- #
# Structural (ProjectionMapping) layer
# --------------------------------------------------------------------------- #


def _rdf_list(graph: Graph, node: Node | None) -> list[Node]:
    out: list[Node] = []
    seen: set[Node] = set()
    while node is not None and node != RDF.nil and node not in seen:
        seen.add(node)
        first = graph.value(node, RDF.first)
        if first is not None:
            out.append(first)
        node = graph.value(node, RDF.rest)
    return out


def _projection_files() -> list[Path]:
    return sorted((MAPPING_DSL_DIR / "projections").glob("*.ttl")) + sorted(
        SLICES_DIR.glob("*/*/mappings/*.ttl")
    )


def iter_projection_graphs() -> list[Graph]:
    """Every projection/structural cell file, parsed — from the repo or the bundle.

    The dev fast-path parses ``dsl/mappings/projections/*.ttl`` + slice mappings
    directly; a wheel-only install (no source tree) parses the same files folded
    into the bundle (#bundle — the CLI razor: ``gmeow`` needs no repo).
    """
    paths = _projection_files()
    if paths:
        return [Graph().parse(path, format="turtle") for path in paths]
    from gmeow_tools.bundle import bundled_cells_under

    return [
        Graph().parse(data=data, format="turtle")
        for _rel, data in sorted(
            bundled_cells_under("dsl/mappings/projections/").items()
        )
    ]


def structural_best_classes() -> dict[str, str]:
    """Target IRI → best structural invertibility class across all cells."""
    best: dict[str, str] = {}
    for graph in iter_projection_graphs():
        for cell in graph.subjects(RDF.type, GM.ProjectionMapping):
            pattern = graph.value(cell, GM.hasMappingPattern)
            has_mint = pattern is not None and any(graph.objects(pattern, GM.mint))
            has_guard = pattern is not None and (
                any(graph.objects(pattern, GM.path))
                or any(graph.objects(pattern, GM.filter))
            )
            for binding in graph.objects(cell, GM.hasBinding):
                targets = _emitted_targets(graph, binding)
                atoms = list(_template_atoms(graph, binding))
                if has_mint:
                    cls = "structural-mint"
                elif has_guard:
                    cls = "structural-guarded"
                elif len(atoms) > 1:
                    cls = "structural-multileg"
                else:
                    cls = "simple-1to1"
                for term in targets:
                    cur = best.get(term)
                    if cur is None or _STRUCT_RANK.get(cls, 0) > _STRUCT_RANK.get(
                        cur, 0
                    ):
                        best[term] = cls
    return best


def _template_atoms(graph: Graph, binding: Node) -> Iterator[Node]:
    for ta in graph.objects(binding, GM.templateAtoms):
        yield from _rdf_list(graph, ta)


def _emitted_targets(graph: Graph, binding: Node) -> set[str]:
    """Projection-target IRIs a binding emits, filtered to projection namespaces.

    Reads ``toClass``/``toPredicate``/``edoalTarget`` and the template atoms'
    ``tPred``/``tObjValue``; only URIRefs in a projection namespace are kept.
    """
    targets: set[str] = set()
    for pred in (GM.toClass, GM.toPredicate, GM.edoalTarget):
        for obj in graph.objects(binding, pred):
            if isinstance(obj, URIRef) and _in_projection_ns(str(obj)):
                targets.add(str(obj))
    for atom in _template_atoms(graph, binding):
        for pred in (GM.tPred, GM.tObjValue):
            for obj in graph.objects(atom, pred):
                if isinstance(obj, URIRef) and _in_projection_ns(str(obj)):
                    targets.add(str(obj))
    return targets


# --------------------------------------------------------------------------- #
# Combined real-data baseline
# --------------------------------------------------------------------------- #


def _used_target_terms(path: Path) -> set[str]:
    """Full target-vocabulary IRIs used (as predicate or rdf:type) in a file."""
    graph = Graph().parse(path, format="turtle")
    preds = {str(p) for p in set(graph.predicates())}
    types = {
        str(o) for o in set(graph.objects(None, RDF.type)) if isinstance(o, URIRef)
    }
    return {iri for iri in (preds | types) if _in_projection_ns(iri)}


def combined_class(term: str, sssom: dict[str, str], struct: dict[str, str]) -> str:
    """Best combined up-projection class for a target term across both layers."""
    s = sssom.get(term)
    t = struct.get(term)
    if s == "clean-reversible" or t == "simple-1to1":
        return "clean"
    if s in _SSSOM_LIFTABLE or t in _STRUCT_LIFTABLE:
        return "liftable-with-claim"
    if t == "structural-mint":
        return "hard-mint"
    if s in ("down-only-related", "down-only-narrowing"):
        return "down-only"
    return "GAP"


@dataclass
class FileBaseline:
    """Up-projection coverage for one real source file."""

    name: str
    per_term: dict[str, str]  # target qname → combined class
    per_vocab: dict[str, dict[str, int]] = field(default_factory=dict)

    @property
    def liftable(self) -> int:
        """Count of target terms with a liftable (clean or with-claim) path."""
        return sum(
            1 for c in self.per_term.values() if c in ("clean", "liftable-with-claim")
        )

    @property
    def total(self) -> int:
        """Total distinct projection-target terms used by the file."""
        return len(self.per_term)


@dataclass
class AuditReport:
    """The full per-file up-projection audit plus the corpus gap list."""

    files: list[FileBaseline]
    gaps: list[str]  # distinct GAP terms across the corpus
    sssom_total: int
    struct_total: int

    @property
    def liftable(self) -> int:
        """Liftable target terms across the whole corpus."""
        return sum(f.liftable for f in self.files)

    @property
    def total(self) -> int:
        """Total target terms across the whole corpus."""
        return sum(f.total for f in self.files)


def run_audit() -> AuditReport:
    """Classify both layers and compute the real-data baseline over the corpus."""
    sssom = sssom_best_buckets()
    struct = structural_best_classes()
    files: list[FileBaseline] = []
    gaps: set[str] = set()
    for name in _CORPUS:
        path = FIXTURES_DIR / "external" / f"{name}.ttl"
        if not path.exists():
            continue
        per_term: dict[str, str] = {}
        per_vocab: dict[str, dict[str, int]] = {}
        for iri in sorted(_used_target_terms(path)):
            cls = combined_class(iri, sssom, struct)  # matched by IRI
            term = _canon_qname(iri)  # displayed as a canonical qname
            per_term[term] = cls
            per_vocab.setdefault(_prefix(term), {}).setdefault(cls, 0)
            per_vocab[_prefix(term)][cls] += 1
            if cls == "GAP":
                gaps.add(term)
        files.append(FileBaseline(name=name, per_term=per_term, per_vocab=per_vocab))
    return AuditReport(
        files=files,
        gaps=sorted(gaps),
        sssom_total=len(sssom),
        struct_total=len(struct),
    )


def render_markdown(report: AuditReport) -> str:
    """Render the audit as the committed Markdown report."""
    lines: list[str] = []
    lines.append("# Up-projection invertibility audit (#449)\n")
    lines.append(
        "Generated by `gmeow up-projection-audit`. Coverage is measured on the "
        "vendored real-world snapshots `tests/fixtures/coverage/external/"
        "{bii,paudley}.ttl` — fixed real RDF, so the number moves only by "
        "extending GMEOW or its cells, never by authoring fixtures.\n"
    )
    lift, tot = report.liftable, report.total
    pct = (100 * lift // tot) if tot else 0
    lines.append(f"## Headline: {lift}/{tot} target terms liftable ({pct}%)\n")
    agg: dict[str, int] = {}
    for f in report.files:
        for c in f.per_term.values():
            agg[c] = agg.get(c, 0) + 1
    lines.append("| bucket | count | meaning |")
    lines.append("|---|---|---|")
    rows = [
        ("clean", "free 1:1 reverse (symmetric SSSOM or simple structural cell)"),
        (
            "liftable-with-claim",
            "closeMatch / multi-leg — lift with a provenance-stamped claim",
        ),
        ("hard-mint", "structural minting cell — needs a hand-authored inverse"),
        ("down-only", "relatedMatch / narrowMatch — no faithful lift"),
        ("GAP", "no liftable cell either layer — GMEOW coverage gap"),
    ]
    for key, meaning in rows:
        lines.append(f"| {key} | {agg.get(key, 0)} | {meaning} |")
    lines.append("")
    for f in report.files:
        lines.append(f"## {f.name}: {f.liftable}/{f.total} liftable\n")
        lines.append("| vocab | liftable/total | breakdown |")
        lines.append("|---|---|---|")
        for v in sorted(f.per_vocab, key=lambda v: -sum(f.per_vocab[v].values())):
            vc = f.per_vocab[v]
            vl = vc.get("clean", 0) + vc.get("liftable-with-claim", 0)
            lines.append(f"| {v} | {vl}/{sum(vc.values())} | {vc} |")
        lines.append("")
    lines.append(f"## Coverage gaps ({len(report.gaps)} distinct terms)\n")
    lines.append(
        "Used in the real files with no liftable cell in either layer. Triage: "
        "*has-concept-needs-cell* / *pass-through* (authority links) / "
        "*genuine GMEOW gap* (model it or declare out-of-coverage).\n"
    )
    by_vocab_gaps: dict[str, list[str]] = {}
    for t in report.gaps:
        by_vocab_gaps.setdefault(_prefix(t), []).append(t)
    for v in sorted(by_vocab_gaps):
        lines.append(f"- **{v}**: {', '.join(by_vocab_gaps[v])}")
    lines.append("")
    return "\n".join(lines)
