# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""The teleology core slice (#350, EPIC #348).

Retained tests (not migrated to slices/core/teleology/tests/structural.ttl):

  test_intrinsic_modes_are_grounded — asserts (gmeow:MentalMoment,
    rdfs:subClassOf, logic:Mode); gmeow:MentalMoment is defined in the
    mentation slice (cross-slice subject), so a scopeModule cell would
    silently miss it.

  test_no_preferred_or_primary_goal_terms — dynamic whole-graph sweep over
    all subjects filtered by IRI prefix; scoping to the teleology module would
    silently narrow the live-set intent.

  test_competency_teleology_modes_query — reads an external .rq file and
    asserts the result set; generated-artifact / SPARQL-result check.

Migrated to crates/validate/tests/conformance_teleology.rs (#867):
  test_wellformed_teleology_fixture_conforms → wellformed_teleology_fixture_conforms
  test_malformed_teleology_fixture_is_flagged → malformed_teleology_fixture_is_flagged

All asserted-TBox structural invariants whose subjects are defined in
slices/core/teleology/module.ttl have been migrated to
slices/core/teleology/tests/structural.ttl (#867).
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import RDFS, Graph, Namespace
from gmeow_rdf.compat.rdflib.query import ResultRow

from gmeow_tools.config import COMPETENCY_DIR
from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
LOGIC = Namespace("https://blackcatinformatics.ca/logic/")


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# Cross-slice structural invariant (RETAINED — cross-slice subject)
# --------------------------------------------------------------------------- #


def test_intrinsic_modes_are_grounded() -> None:
    g = _graph()
    # Reparented under gmeow:MentalMoment (#556); MentalMoment ⊑ logic:Mode
    # supplies the native-logic branch, so IntentionalMode stays grounded in Mode
    # transitively rather than by a direct (now-removed) subClassOf assertion.
    # gmeow:MentalMoment is defined in the mentation slice (cross-slice subject);
    # this assertion cannot be scoped to the teleology module.
    assert (GM.MentalMoment, RDFS.subClassOf, LOGIC.Mode) in g


# --------------------------------------------------------------------------- #
# Whole-graph sweep (RETAINED — dynamic live-set, not scopable to one module)
# --------------------------------------------------------------------------- #


def test_no_preferred_or_primary_goal_terms() -> None:
    """No preferredGoal / primaryIntention selectors exist (Principle 9)."""
    g = _graph()
    banned = ("primarygoal", "preferredgoal", "primaryintention", "preferredintention")
    offenders = [
        str(s)
        for s in set(g.subjects())
        if str(s).startswith(GMEOW)
        and "/" not in str(s)[len(GMEOW) :]
        and str(s)[len(GMEOW) :].lower().startswith(banned)
    ]
    assert offenders == []


# --------------------------------------------------------------------------- #
# Competency (RETAINED — external .rq file, SPARQL result check)
# --------------------------------------------------------------------------- #


def test_competency_teleology_modes_query() -> None:
    query = (COMPETENCY_DIR / "teleology-modes.rq").read_text(encoding="utf-8")
    modes: set[object] = set()
    for row in _graph().query(query):
        assert isinstance(row, ResultRow)
        modes.add(row[0])
    assert {GM.Desire, GM.Intention, GM.Commitment} <= modes
