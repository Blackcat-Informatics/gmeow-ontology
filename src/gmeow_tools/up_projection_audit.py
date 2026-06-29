# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Rust-backed up-projection invertibility audit.

The audit headline and Markdown report are computed natively: each liftable
external target term is realized as a ``logic:Correspondence`` and run through
the five correspondence gates, so the headline is a gate-verdict ledger (proved
/ claimed / excluded / unsupported), not a heuristic bucket count. This module
is the thin Python surface — it gathers the SSSOM, projection-cell, and corpus
inputs and hands them to the Rust gate-audit; it computes no count itself. The
input-gathering helpers (``_sssom_texts``, ``_projection_ttls``,
``_ontology_nt``, ``_canon_qname`` …) are shared by the other up-projection
surfaces.
"""

from __future__ import annotations

from functools import lru_cache
from types import ModuleType
from typing import TypedDict, cast

from gmeow_tools.config import (
    EXTERNAL_FIXTURES_DIR,
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


class TierCounts(TypedDict):
    """The four-tier gate-verdict counts for one vocabulary (or the whole audit)."""

    proved: int
    claimed: int
    red_excluded: int
    unsupported: int
    liftable: int
    total: int


class GateAudit(TypedDict):
    """The gate-derived audit result: the rendered Markdown plus the verdict ledger."""

    markdown: str
    totals: TierCounts
    per_vocab: dict[str, TierCounts]
    gaps: list[str]
    proved: int
    claimed: int
    red_excluded: int
    unsupported: int
    liftable: int
    total: int


def _corpus_ttls() -> list[tuple[str, str]]:
    """The vendored real-world corpus snapshots as ``(name, turtle_text)`` pairs.

    Fixed real RDF (``tests/fixtures/coverage/external/{bii,paudley}.ttl``) — the
    audit number moves only by extending GMEOW or its cells, never by authoring
    fixtures. The Turtle→NT conversion happens natively in the gate audit, so no
    rdflib parse is needed here.
    """
    corpus: list[tuple[str, str]] = []
    for name in ("bii", "paudley"):
        path = EXTERNAL_FIXTURES_DIR / f"{name}.ttl"
        corpus.append((name, path.read_text(encoding="utf-8")))
    return corpus


def gate_audit() -> GateAudit:
    """The gate-derived up-projection audit through Rust.

    Returns the rendered Markdown report plus the gate-verdict ledger (``proved``
    / ``claimed`` / ``red_excluded`` / ``unsupported`` counts, overall and
    per-vocabulary, with the coverage-gap terms). No count is computed in Python.
    """
    return cast(
        "GateAudit",
        _pipeline().up_projection_gate_audit(
            list(_sssom_texts()), list(_projection_ttls()), _corpus_ttls()
        ),
    )
