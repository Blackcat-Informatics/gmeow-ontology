"""Temporal slice cross-slice invariant the slicetest harness cannot reach.

Most of this slice's structural invariants now live as declarative
``gmeow:StructuralAssertion`` cells in ``tests/structural.ttl``, auto-discovered
and run by the native Rust harness (``crates/slicetest``, ``make slicetest``). See
``dsl/tests/MIGRATION-LEDGER.md`` for the per-test pytest→DSL mapping.

The remaining function is CROSS-SLICE: its subjects are NOT declared in
``temporal/module.ttl`` — they live in other slices (email, contacts, names) —
so the module-scoped ASK harness (which loads only this slice's ``module.ttl``)
cannot see them. It is faithfully tested only over the FULL merged graph
(``load_merged_graph(include_imports=True)``).
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import Graph, URIRef
from gmeow_rdf.compat.rdflib.namespace import RDFS

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    # Imports included so the gufo:Situation grounding resolves.
    return load_merged_graph(include_imports=True)


def test_reified_residence_and_tenure_are_time_scoped() -> None:
    # CROSS-SLICE: MailboxResidence (extensions/email) and AddressTenure
    # (core/contacts) declare their ⊑ TimeScopedRelation edge in their OWN slice
    # modules, not in temporal's — so this is a merged-graph integration check.
    graph = _graph()
    for term in ("MailboxResidence", "AddressTenure"):
        assert (
            URIRef(GMEOW + term),
            RDFS.subClassOf,
            URIRef(GMEOW + "TimeScopedRelation"),
        ) in graph
