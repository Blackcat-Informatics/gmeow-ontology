"""The plan ⟂ execution slice (#226).

GMEOW models occurrences richly (Event, Participation, Activity, Allen relations)
but had no prescriptive layer. This module de-conflates plan from execution:
Procedure / ProcedureStep (InformationObject) vs Execution (Event), with reified
ControlFlow and DataFlow relators following the Participation idiom.

Asserted-TBox structural invariants have been migrated to the declarative slicetest
DSL at slices/extensions/procedures/tests/structural.ttl (#867). Only dynamic /
numeric checks that cannot be expressed as module-scoped SPARQL ASK cells remain here.
"""

from __future__ import annotations

from functools import lru_cache

from gmeow_rdf.compat.rdflib import Graph, Namespace

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)


@lru_cache(maxsize=1)
def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# Seed profiles — numeric / cardinality checks (cannot be expressed as ASK)
# --------------------------------------------------------------------------- #


def test_ingestion_procedure_has_six_steps() -> None:
    g = _graph()
    steps = list(g.objects(GM.procedureIngestionCanonical, GM.hasProcedureStep))
    assert len(steps) == 6
