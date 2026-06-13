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

from gmeow_tools.config import FIXTURES_DIR, MAPPING_DSL_DIR, MAPPINGS_DIR, SLICES_DIR

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


def _prefix(term: str) -> str:
    return term.split(":", 1)[0] if ":" in term else ""


# --------------------------------------------------------------------------- #
# SSSOM layer
# --------------------------------------------------------------------------- #


def _read_sssom(path: Path) -> list[dict[str, str]]:
    rows: list[dict[str, str]] = []
    header: list[str] | None = None
    for line in path.read_text(encoding="utf-8").splitlines():
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
    """Target qname → best up-bucket across all SSSOM cells (projection targets)."""
    best: dict[str, str] = {}
    for path in sorted(MAPPINGS_DIR.glob("*.sssom.tsv")):
        for row in _read_sssom(path):
            bucket, _gmeow, target = classify_sssom(
                row["subject_id"], row["predicate_id"], row["object_id"]
            )
            if _prefix(target) not in PROJECTION_PREFIXES:
                continue
            cur = best.get(target)
            if cur is None or _SSSOM_RANK.get(bucket, 0) > _SSSOM_RANK.get(cur, 0):
                best[target] = bucket
    return best


# --------------------------------------------------------------------------- #
# Structural (ProjectionMapping) layer
# --------------------------------------------------------------------------- #


def _qname(uri: URIRef, graph: Graph) -> str:
    s = str(uri)
    # most-specific prefix first: a longer namespace can be a prefix of none
    for pfx, ns in sorted(graph.namespaces(), key=lambda kv: -len(str(kv[1]))):
        if s.startswith(str(ns)):
            return f"{pfx}:{s[len(str(ns)) :]}"
    return s


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


def structural_best_classes() -> dict[str, str]:
    """Target qname → best structural invertibility class across all cells."""
    best: dict[str, str] = {}
    for path in _projection_files():
        graph = Graph().parse(path, format="turtle")
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
    targets: set[str] = set()
    for pred in (GM.toClass, GM.toPredicate, GM.edoalTarget):
        for obj in graph.objects(binding, pred):
            if not isinstance(obj, URIRef):
                continue
            q = _qname(obj, graph)
            if _prefix(q) in PROJECTION_PREFIXES:
                targets.add(q)
    for atom in _template_atoms(graph, binding):
        for pred in (GM.tPred, GM.tObjValue):
            for obj in graph.objects(atom, pred):
                if not isinstance(obj, URIRef):
                    continue
                q = _qname(obj, graph)
                if _prefix(q) in PROJECTION_PREFIXES:
                    targets.add(q)
    return targets


# --------------------------------------------------------------------------- #
# Combined real-data baseline
# --------------------------------------------------------------------------- #


def _used_target_terms(path: Path) -> set[str]:
    graph = Graph().parse(path, format="turtle")
    # most-specific (longest) namespace first, so overlapping namespaces don't
    # contract to the wrong prefix and skew the baseline
    ns2pfx = sorted(
        ((str(ns), pfx) for pfx, ns in graph.namespaces()), key=lambda kv: -len(kv[0])
    )

    def q(uri: object) -> str:
        s = str(uri)
        for ns, pfx in ns2pfx:
            if s.startswith(ns):
                return f"{pfx}:{s[len(ns) :]}"
        return s

    preds = {q(p) for p in set(graph.predicates())}
    types = {q(o) for o in set(graph.objects(None, RDF.type)) if isinstance(o, URIRef)}
    return {t for t in (preds | types) if _prefix(t) in PROJECTION_PREFIXES}


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
        for term in sorted(_used_target_terms(path)):
            cls = combined_class(term, sssom, struct)
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
