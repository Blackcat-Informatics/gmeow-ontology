"""Temporal slice — the two CROSS-SLICE invariants the slicetest harness can't reach.

Most of this slice's structural invariants now live as declarative
``gmeow:StructuralAssertion`` cells in ``tests/structural.ttl``, auto-discovered
and run by the native Rust harness (``crates/slicetest``, ``make slicetest``). See
``dsl/tests/MIGRATION-LEDGER.md`` for the per-test pytest→DSL mapping.

The two functions that remain here are CROSS-SLICE: their subjects are NOT
declared in ``temporal/module.ttl`` — they live in other slices (email, contacts,
names) — so the module-scoped ASK harness (which loads only this slice's
``module.ttl``) cannot see them. They are faithfully tested only over the FULL
merged graph (``load_merged_graph(include_imports=True)``), which is what these
two retain.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import Graph, URIRef
from gmeow_rdf.compat.rdflib.namespace import RDFS

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"


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


def test_interpersonal_relationship_is_a_gufo_relator() -> None:
    # CROSS-SLICE: InterpersonalRelationship (core/contacts, core/names) is a
    # relator (mediates + depends on its players), NOT a Situation — the
    # Relator-vs-Situation decision is load-bearing. Declared in those slices, so
    # this is a merged-graph integration check.
    graph = _graph()
    assert (
        URIRef(GMEOW + "InterpersonalRelationship"),
        RDFS.subClassOf,
        URIRef(GUFO + "Relator"),
    ) in graph
