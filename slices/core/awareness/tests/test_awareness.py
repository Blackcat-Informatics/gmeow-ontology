# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Awareness slice — the two invariants the native slicetest harness can't reach.

Most of this slice's structural invariants now live as declarative
``gmeow:StructuralAssertion`` cells in ``tests/structural.ttl``, auto-discovered
and run by the native Rust harness (``crates/slicetest``, ``make slicetest``); the
annotation-completeness invariant is subsumed by the global ``make validate`` gate
(SHACL ``Gmeow*Shape`` for classes/properties + the Rust ``structural_lint``
guardian for the value-vocab individuals). See ``dsl/tests/MIGRATION-LEDGER.md``
for the per-test pytest→DSL mapping.

The two functions that remain here are genuinely UNREACHABLE by the
module-scoped ASK harness and so stay in Python:

* ``test_level_ranks_are_zero_through_five`` — a closed numeric SET-EQUALITY over
  the six ``gmeow:levelRank`` values: a SPARQL ASK can assert each rank exists but
  not that the set is EXACTLY ``{0,1,2,3,4,5}`` and no others.
* ``test_manifest_depends_only_on_kernel_and_temporal`` — set-equality over
  ``manifest.ttl``, which ``run_structural_cell`` never loads (it builds the store
  from ``module.ttl`` + ``examples/`` only).
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import Graph, URIRef

GMEOW = "https://blackcatinformatics.ca/gmeow/"
SLICE_IRI = URIRef("https://blackcatinformatics.ca/gmeow/slices/awareness")
SLICE_DEPENDS_ON = URIRef(GMEOW + "sliceDependsOn")
_MODULE = Path(__file__).resolve().parents[1] / "module.ttl"
_MANIFEST = Path(__file__).resolve().parents[1] / "manifest.ttl"

_LEVELS = (
    "levelHyperalert",
    "levelAlert",
    "levelRelaxed",
    "levelDrowsy",
    "levelObtunded",
    "levelUnresponsive",
)


def _t(name: str) -> URIRef:
    """A gmeow-namespaced term URI."""
    return URIRef(GMEOW + name)


def _graph() -> Graph:
    g = Graph()
    g.parse(_MODULE, format="turtle")
    return g


def test_level_ranks_are_zero_through_five() -> None:
    """Each level individual carries a gmeow:levelRank, and the six ranks are
    exactly {0,1,2,3,4,5} (high arousal → low)."""
    g = _graph()
    rank = _t("levelRank")
    ranks = set()
    for indiv in _LEVELS:
        values = list(g.objects(_t(indiv), rank))
        assert len(values) == 1, f"{indiv} should carry exactly one levelRank"
        ranks.add(int(values[0]))
    assert ranks == {0, 1, 2, 3, 4, 5}, f"unexpected rank set: {ranks}"


def test_manifest_depends_only_on_kernel_and_temporal() -> None:
    """Manifest dependency hygiene (no over-declaration): the asserted foreign IRIs
    are gmeow:Agent (kernel) and the time-scoped-relation reification seam
    (temporal), so sliceDependsOn is exactly {kernel, temporal} — mentation,
    metacognition, and imagination are consumed by reference, never declared."""
    g = Graph()
    g.parse(_MANIFEST, format="turtle")
    deps = set(g.objects(SLICE_IRI, SLICE_DEPENDS_ON))
    assert deps == {
        URIRef(GMEOW + "slices/kernel"),
        URIRef(GMEOW + "slices/temporal"),
    }, f"unexpected deps: {deps}"
