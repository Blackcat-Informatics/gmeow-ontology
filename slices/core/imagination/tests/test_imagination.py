# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Imagination slice — the one invariant the native slicetest harness can't reach.

This slice's structural invariants now live as declarative
``gmeow:StructuralAssertion`` cells in ``tests/structural.ttl``, auto-discovered
and run by the native Rust harness (``crates/slicetest``, ``make slicetest``); the
annotation-completeness invariant is subsumed by the global ``make validate`` gate
(SHACL ``Gmeow*Shape`` for the class/properties + the Rust ``structural_lint``
guardian for the value-vocab ``origin*`` individuals). See
``dsl/tests/MIGRATION-LEDGER.md`` for the per-test pytest→DSL mapping.

The one function that remains here is genuinely UNREACHABLE by the module-scoped
ASK harness and so stays in Python:

* ``test_manifest_depends_only_on_kernel`` — set-equality over ``manifest.ttl``,
  which ``run_structural_cell`` never loads (it builds the store from
  ``module.ttl`` + ``examples/`` only).
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import Graph, URIRef

GMEOW = "https://blackcatinformatics.ca/gmeow/"
SLICE_IRI = URIRef("https://blackcatinformatics.ca/gmeow/slices/imagination")
SLICE_DEPENDS_ON = URIRef(GMEOW + "sliceDependsOn")
_MANIFEST = Path(__file__).resolve().parents[1] / "manifest.ttl"


def test_manifest_depends_only_on_kernel() -> None:
    """Manifest dependency hygiene (no over-declaration): the single asserted
    foreign IRI is gmeow:Agent (domain of the spine), so sliceDependsOn lists
    KERNEL ALONE — mentation / logic / deception / epistemics are consumed by
    reference, never declared."""
    g = Graph()
    g.parse(_MANIFEST, format="turtle")
    deps = set(g.objects(SLICE_IRI, SLICE_DEPENDS_ON))
    assert deps == {URIRef(GMEOW + "slices/kernel")}, f"unexpected deps: {deps}"
