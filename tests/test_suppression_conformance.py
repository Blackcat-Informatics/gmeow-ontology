# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Generated leak-conformance suite (#282, CONSTITUTION P10).

"The leak is prevented" is the constitution's claim; this suite makes it a
CI-proven property of every present and FUTURE projection profile — the
tests parametrize over the live :data:`gmeow_tools.projections.PROFILES`
registry, so adding a profile yields its leak-conformance tests with zero
additional authoring.

Three layers:

* **Structural**: every generated CONSTRUCT carries the injected
  ``displayable false`` guard on every required subject variable of every
  branch — re-derived from the mapping DSL, so the artifact and the compiler
  can never disagree.
* **Behavioral, withhold**: a canary fixture's ``SUPPRESSED-CANARY``
  literals never appear in any profile's serialized output; the
  ``CONTROL-CANARY`` twin (identical but displayable) proves coverage, so
  the suppressed half can never pass vacuously.
* **Behavioral, coarsen**: the suppress-gen fixture's precise coordinates
  never appear in any profile's output (the declared coarsening branches
  publish the enclosing city instead — pinned by tests/test_suppress_gen.py;
  here every OTHER profile proves it does not leak them either).

The hand-written cases in tests/test_suppress_gen.py remain as regression
tests over this generated suite.
"""

from __future__ import annotations

import pytest
from rdflib import Graph

from gmeow_tools.config import FIXTURES_DIR, PROJECTION_QUERY_DIR
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.projections import PROFILES, project_graph

_CANARY_FILE = FIXTURES_DIR / "suppression-canary.ttl"
_COARSEN_FILE = FIXTURES_DIR / "suppress-gen.ttl"

#: Profiles that project appellations — the control canary MUST surface in
#: these, proving the canary pattern is genuinely covered (no vacuous pass).
_NAME_PROFILES = ("foaf", "schema-org", "vcard")

#: The marked place's precise values in suppress-gen.ttl; the city's
#: coordinates (51.5072, -0.1276) are the only publishable form.
_PRECISE_COORDS = ("51.500001", "-0.124999")


@pytest.fixture(scope="module")
def source() -> Graph:
    graph = load_merged_graph(include_imports=False)
    graph.parse(_CANARY_FILE, format="turtle")
    graph.parse(_COARSEN_FILE, format="turtle")
    return graph


@pytest.fixture(scope="module")
def projections(source: Graph) -> dict[str, str]:
    """Every profile's serialized projection of the canary corpus."""
    return {
        name: project_graph(name, source).serialize(format="turtle")
        for name in PROFILES
    }


@pytest.mark.parametrize("profile", sorted(PROFILES))
def test_suppressed_canary_never_leaks(
    profile: str, projections: dict[str, str]
) -> None:
    """displayable false never surfaces — in ANY profile, present or future."""
    assert "SUPPRESSED-CANARY" not in projections[profile], (
        f"profile {profile} leaked a displayable-false value"
    )


@pytest.mark.parametrize("profile", sorted(PROFILES))
def test_precise_coarsened_values_never_leak(
    profile: str, projections: dict[str, str]
) -> None:
    """A coarsenTo-marked place's precise coordinates appear in no profile."""
    for precise in _PRECISE_COORDS:
        assert precise not in projections[profile], (
            f"profile {profile} leaked a precise value past gmeow:coarsenTo"
        )


@pytest.mark.parametrize("profile", _NAME_PROFILES)
def test_control_canary_proves_coverage(
    profile: str, projections: dict[str, str]
) -> None:
    """The displayable twin DOES project — the leak tests are not vacuous."""
    assert "CONTROL-CANARY" in projections[profile], (
        f"profile {profile} no longer projects the control canary — the "
        f"suppression conformance tests would be vacuous"
    )


@pytest.mark.parametrize("profile", sorted(PROFILES))
def test_every_branch_carries_its_injected_guards(profile: str) -> None:
    """Structural seal: the committed artifact contains every derived guard.

    Re-derives the guard set from the mapping DSL (the same
    ``_suppression_anchors`` the compiler uses) and asserts each one appears
    in the committed query — the generated artifact can never silently drop
    the injection.
    """
    from gmeow_tools.mapping_compile import (
        _default_suppression_vocab,
        _injected_guards,
    )
    from gmeow_tools.mapping_dsl import load_dsl

    query = (PROJECTION_QUERY_DIR / f"{profile}.rq").read_text(encoding="utf-8")
    dsl = load_dsl()
    vocab = _default_suppression_vocab()
    checked = 0
    for cell in dsl.projections:
        if not any(b.profile == profile for b in cell.bindings):
            continue
        for guard in _injected_guards(cell.pattern, vocab):
            assert guard in query, f"profile {profile}: missing guard {guard}"
            checked += 1
    if checked == 0:
        pytest.skip(f"profile {profile} has no projection cells in the DSL")
