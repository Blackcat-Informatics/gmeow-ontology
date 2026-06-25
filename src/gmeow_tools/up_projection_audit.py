# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Rust-backed up-projection invertibility audit (#942)."""

from __future__ import annotations

from dataclasses import dataclass, field
from functools import lru_cache
from types import ModuleType
from typing import cast

from gmeow_rdf.compat.rdflib import Graph

from gmeow_tools.config import (
    FIXTURES_DIR,
    MAPPING_DSL_DIR,
    MAPPINGS_DIR,
    PREFIXES,
    SLICES_DIR,
)

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

_PREFIX_BY_NS = sorted(PREFIXES.items(), key=lambda item: len(item[1]), reverse=True)


def _pipeline() -> ModuleType:
    from gmeow_native import pipeline

    return cast(ModuleType, pipeline)


def _to_iri(curie: str) -> str:
    if curie.startswith(("http://", "https://", "urn:")) or ":" not in curie:
        return curie
    prefix, local = curie.split(":", 1)
    namespace = PREFIXES.get(prefix)
    return namespace + local if namespace is not None else curie


def _in_projection_ns(iri: str) -> bool:
    return _prefix(_canon_qname(iri)) in PROJECTION_PREFIXES


def _canon_qname(iri: str) -> str:
    for prefix, namespace in _PREFIX_BY_NS:
        if iri.startswith(namespace):
            return f"{prefix}:{iri[len(namespace) :]}"
    return iri


def _prefix(term: str) -> str:
    return term.split(":", 1)[0] if ":" in term else "(iri)"


@lru_cache(maxsize=1)
def _sssom_texts() -> tuple[str, ...]:
    files = sorted(MAPPINGS_DIR.glob("*.sssom.tsv"))
    if files:
        return tuple(path.read_text(encoding="utf-8") for path in files)
    from gmeow_tools.bundle import bundled_sssom

    return tuple(
        data.decode("utf-8") for _name, data in sorted(bundled_sssom().items())
    )


@lru_cache(maxsize=1)
def _projection_ttls() -> tuple[str, ...]:
    files = sorted((MAPPING_DSL_DIR / "projections").glob("*.ttl")) + sorted(
        SLICES_DIR.glob("*/*/mappings/*.ttl")
    )
    if files:
        return tuple(path.read_text(encoding="utf-8") for path in files)
    from gmeow_tools.bundle import bundled_cells_under

    return tuple(
        data.decode("utf-8")
        for _rel, data in sorted(
            bundled_cells_under("dsl/mappings/projections/").items()
        )
    )


@lru_cache(maxsize=1)
def _ontology_nt() -> str:
    from gmeow_tools.graph import shared_merged_graph

    data = shared_merged_graph(include_imports=False).serialize(
        format="nt", encoding="utf-8"
    )
    return data.decode("utf-8")


def classify_sssom(subj: str, pred: str, obj: str) -> tuple[str, str, str]:
    """Classify one SSSOM row through the Rust up-projection authority."""
    return cast(
        tuple[str, str, str],
        tuple(_pipeline().up_projection_classify_sssom(subj, pred, obj)),
    )


def combined_class(term: str, sssom: dict[str, str], struct: dict[str, str]) -> str:
    """Best combined up-projection class for a target term."""
    return cast(str, _pipeline().up_projection_combined_class(term, sssom, struct))


@dataclass
class FileBaseline:
    """Up-projection coverage for one real source file."""

    name: str
    per_term: dict[str, str]
    per_vocab: dict[str, dict[str, int]] = field(default_factory=dict)

    @property
    def liftable(self) -> int:
        """Count of target terms with a liftable clean or claim path."""
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
    gaps: list[str]
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
    """Classify both layers and compute the real-data baseline through Rust."""
    corpus: list[tuple[str, str]] = []
    for name in ("bii", "paudley"):
        path = FIXTURES_DIR / "external" / f"{name}.ttl"
        if not path.exists():
            continue
        graph = Graph().parse(path, format="turtle")
        corpus.append(
            (
                name,
                graph.serialize(format="nt", encoding="utf-8").decode("utf-8"),
            )
        )
    raw = _pipeline().up_projection_audit_nt(
        list(_sssom_texts()), list(_projection_ttls()), corpus
    )
    files = [
        FileBaseline(
            name=file["name"],
            per_term=dict(file["per_term"]),
            per_vocab={
                vocab: dict(counts) for vocab, counts in file["per_vocab"].items()
            },
        )
        for file in raw["files"]
    ]
    return AuditReport(
        files=files,
        gaps=list(raw["gaps"]),
        sssom_total=raw["sssom_total"],
        struct_total=raw["struct_total"],
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
        for vocab in sorted(f.per_vocab, key=lambda v: -sum(f.per_vocab[v].values())):
            counts = f.per_vocab[vocab]
            liftable = counts.get("clean", 0) + counts.get("liftable-with-claim", 0)
            lines.append(f"| {vocab} | {liftable}/{sum(counts.values())} | {counts} |")
        lines.append("")
    lines.append(f"## Coverage gaps ({len(report.gaps)} distinct terms)\n")
    lines.append(
        "Used in the real files with no liftable cell in either layer. Triage: "
        "*has-concept-needs-cell* / *pass-through* (authority links) / "
        "*genuine GMEOW gap* (model it or declare out-of-coverage).\n"
    )
    if report.gaps:
        lines.append("| term |")
        lines.append("|---|")
        lines.extend(f"| `{term}` |" for term in report.gaps)
        lines.append("")
    return "\n".join(lines)
