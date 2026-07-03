# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""The teleology core slice (#350, EPIC #348).

Retained tests (not migrated to slices/core/teleology/tests/structural.ttl):

  test_no_preferred_or_primary_goal_terms — dynamic whole-graph sweep over
    all subjects filtered by IRI prefix; scoping to the teleology module would
    silently narrow the live-set intent.

Migrated to slices/core/teleology/tests/structural.ttl (#867, #1120):
  - test_intrinsic_modes_are_grounded (module-local half)
      → saIntentionalModeIsCategory, saDesireIsKindIntentionalMode,
        saIntentionIsKindIntentionalMode
  - test_intentional_mode_reparented_under_mental_moment (cross-slice negative half)
      → saIntentionalModeNotDirectlyIntrinsicMode

Migrated to slices/core/teleology/tests/competency.ttl (#1120):
  - test_competency_teleology_modes_query
      → cqTeleologyModes

Migrated to crates/validate/tests/conformance_teleology.rs (#867):
  test_wellformed_teleology_fixture_conforms → wellformed_teleology_fixture_conforms
  test_malformed_teleology_fixture_is_flagged → malformed_teleology_fixture_is_flagged

All asserted-TBox structural invariants whose subjects are defined in
slices/core/teleology/module.ttl have been migrated to
slices/core/teleology/tests/structural.ttl (#867).
"""

from __future__ import annotations

from purrdf.compat.rdflib import Graph, Namespace

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


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
