"""Structural guards for the archaeological evidence layer (#173) — retained tests.

The asserted-TBox invariants (class hierarchy, property shapes, functional/
non-functional markers, observation-bridge subproperties, and the
PhysicalCarrierType value vocabulary) have been migrated to declarative
slicetest cells in:
    slices/extensions/archaeological-evidence/tests/structural.ttl

The two functions below are RETAINED because they require evaluation that
slicetest cells (scopeModule SPARQL ASK) cannot safely perform:

  * test_attested_on_carrier_exists — the subject gmeow:attestedOnCarrier is
    defined in slices/extensions/lexicon/module.ttl, not in the
    archaeological-evidence module; a scopeModule cell would silently miss it,
    turning the guard into a no-op.

  * test_no_primary_or_preferred_archaeological_terms — dynamically sweeps the
    entire merged graph's subject set for any GMEOW-prefixed term whose local
    name begins with "primary" or "preferred"; a module-scoped cell would
    silently narrow this whole-ontology regression guard to a single graph.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Namespace

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# Lexicon hook — cross-slice subject, cannot be scopeModule
# --------------------------------------------------------------------------- #


def test_attested_on_carrier_exists() -> None:
    g = _graph()
    # Subject is in slices/extensions/lexicon/module.ttl — retained because
    # a scopeModule cell scoped to archaeological-evidence would silently miss it.
    assert (GM.attestedOnCarrier, RDF.type, OWL.ObjectProperty) in g
    assert (GM.attestedOnCarrier, RDF.type, OWL.FunctionalProperty) not in g
    assert (GM.attestedOnCarrier, RDFS.domain, GM.UsageAttestation) in g
    assert (GM.attestedOnCarrier, RDFS.range, GM.PhysicalObject) in g


# --------------------------------------------------------------------------- #
# No preferred/primary terms — whole-graph sweep, cannot be scopeModule
# --------------------------------------------------------------------------- #


def test_no_primary_or_preferred_archaeological_terms() -> None:
    g = _graph()
    offenders = []
    for s in set(g.subjects()):
        if isinstance(s, Namespace) or not str(s).startswith(GMEOW):
            continue
        local = str(s)[len(GMEOW) :].lower()
        if "/" not in local and local.startswith(("primary", "preferred")):
            offenders.append(str(s))
    assert offenders == [], f"preferred/primary terms must not exist: {offenders}"
