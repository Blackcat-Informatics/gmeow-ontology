# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Tests for the PyO3 gmeow_logic.materialize binding (issue #499, Task 4).

Covers:
* AC#2 — round-trip: every input quad comes back with the correct world and
  all derivation metadata fields present and well-typed.
* AC#4 — empty-case oracle parity: materialize on empty input → empty result,
  and the Python oracle (gmeow_tools.logic_frontend + LogicProgram) also
  produces an empty canonical output for empty logic source.

The module is skipped cleanly (pytest.importorskip) when the native extension
has not been installed, which allows the test suite to run in environments
where maturin develop has not been executed yet (e.g. pure-Python CI lanes).
"""

from __future__ import annotations

import pytest

gmeow_logic = pytest.importorskip(
    "gmeow_logic",
    reason=("gmeow_logic native extension not installed — run 'make logic-py' first"),
)

# ── Fixtures ──────────────────────────────────────────────────────────────────


# N-Quads covering two distinct named-graph worlds.
# Each line is a quad: <subject> <predicate> <object> <graph> .
_S1 = "<http://example.org/s/1>"
_S2 = "<http://example.org/s/2>"
_S3 = "<http://example.org/s/3>"
_P_TYPE = "<http://example.org/p/type>"
_P_NAME = "<http://example.org/p/name>"
_O_THING = "<http://example.org/o/Thing>"
_O_FOO = "<http://example.org/o/Foo>"
_O_BAR = "<http://example.org/o/Bar>"
_W_ALPHA = "<http://world/Alpha>"
_W_BETA = "<http://world/Beta>"

_TWO_WORLD_NQUADS = (
    f"{_S1} {_P_TYPE} {_O_THING} {_W_ALPHA} .\n"
    f"{_S2} {_P_NAME} {_O_FOO} {_W_ALPHA} .\n"
    f"{_S3} {_P_TYPE} {_O_BAR} {_W_BETA} .\n"
)

_EXPECTED_SUBJECTS = {
    "<http://example.org/s/1>",
    "<http://example.org/s/2>",
    "<http://example.org/s/3>",
}

_EXPECTED_WORLDS = {"http://world/Alpha", "http://world/Beta"}

_REQUIRED_META_FIELDS = {
    "graph",
    "subject",
    "predicate",
    "object",
    "graph_component",
    "derivation_id",
    "rule_iri",
    "source_quad_ids",
    "profile",
    "budget_status",
}


# ── AC#2: round-trip with derivation metadata ────────────────────────────────


def test_materialize_returns_all_input_quads() -> None:
    """Every input quad must appear in the result (subject round-trips)."""
    result = gmeow_logic.materialize("", _TWO_WORLD_NQUADS)
    assert isinstance(result, list), f"expected list, got {type(result)}"
    assert len(result) == 3, f"expected 3 quads back, got {len(result)}"
    returned_subjects = {r["subject"] for r in result}
    assert returned_subjects == _EXPECTED_SUBJECTS, (
        f"subject set mismatch: {returned_subjects!r} != {_EXPECTED_SUBJECTS!r}"
    )


def test_materialize_all_metadata_fields_present() -> None:
    """Every result dict must contain all required metadata fields."""
    result = gmeow_logic.materialize("", _TWO_WORLD_NQUADS)
    for i, quad_dict in enumerate(result):
        missing = _REQUIRED_META_FIELDS - set(quad_dict.keys())
        assert not missing, f"quad[{i}] missing metadata fields: {missing!r}"


def test_materialize_graph_field_is_correct_world() -> None:
    """The 'graph' field must match the named-graph IRI in the input quad."""
    result = gmeow_logic.materialize("", _TWO_WORLD_NQUADS)
    returned_worlds = {r["graph"] for r in result}
    assert returned_worlds == _EXPECTED_WORLDS, (
        f"world set mismatch: {returned_worlds!r} != {_EXPECTED_WORLDS!r}"
    )


def test_materialize_graph_equals_graph_component() -> None:
    """'graph' and 'graph_component' must be identical on every quad."""
    result = gmeow_logic.materialize("", _TWO_WORLD_NQUADS)
    for i, quad_dict in enumerate(result):
        assert quad_dict["graph"] == quad_dict["graph_component"], (
            f"quad[{i}]: graph={quad_dict['graph']!r} != "
            f"graph_component={quad_dict['graph_component']!r}"
        )


def test_materialize_budget_status_is_ok() -> None:
    """All asserted (input) quads must carry budget_status='ok'."""
    result = gmeow_logic.materialize("", _TWO_WORLD_NQUADS)
    for i, quad_dict in enumerate(result):
        assert quad_dict["budget_status"] == "ok", (
            f"quad[{i}]: expected budget_status='ok', "
            f"got {quad_dict['budget_status']!r}"
        )


def test_materialize_derivation_id_is_nonempty_string() -> None:
    """derivation_id must be a non-empty IRI string on every quad."""
    result = gmeow_logic.materialize("", _TWO_WORLD_NQUADS)
    for i, quad_dict in enumerate(result):
        assert isinstance(quad_dict["derivation_id"], str), (
            f"quad[{i}]: derivation_id is not a str"
        )
        assert quad_dict["derivation_id"], f"quad[{i}]: derivation_id is empty"


def test_materialize_rule_iri_is_nonempty_string() -> None:
    """rule_iri must be a non-empty string on every quad."""
    result = gmeow_logic.materialize("", _TWO_WORLD_NQUADS)
    for i, quad_dict in enumerate(result):
        assert isinstance(quad_dict["rule_iri"], str), (
            f"quad[{i}]: rule_iri is not a str"
        )
        assert quad_dict["rule_iri"], f"quad[{i}]: rule_iri is empty"


def test_materialize_source_quad_ids_is_list() -> None:
    """source_quad_ids must be a list on every quad."""
    result = gmeow_logic.materialize("", _TWO_WORLD_NQUADS)
    for i, quad_dict in enumerate(result):
        got_type = type(quad_dict["source_quad_ids"])
        assert isinstance(quad_dict["source_quad_ids"], list), (
            f"quad[{i}]: source_quad_ids is not a list, got {got_type}"
        )


def test_materialize_profile_is_nonempty_string() -> None:
    """profile must be a non-empty string on every quad."""
    result = gmeow_logic.materialize("", _TWO_WORLD_NQUADS)
    for i, quad_dict in enumerate(result):
        assert isinstance(quad_dict["profile"], str), f"quad[{i}]: profile is not a str"
        assert quad_dict["profile"], f"quad[{i}]: profile is empty"


def test_materialize_world_isolation() -> None:
    """Alpha-world quads must not appear in Beta-world quads."""
    result = gmeow_logic.materialize("", _TWO_WORLD_NQUADS)
    alpha_quads = [r for r in result if r["graph"] == "http://world/Alpha"]
    beta_quads = [r for r in result if r["graph"] == "http://world/Beta"]
    assert len(alpha_quads) == 2, f"expected 2 Alpha quads, got {len(alpha_quads)}"
    assert len(beta_quads) == 1, f"expected 1 Beta quad, got {len(beta_quads)}"

    alpha_subjects = {r["subject"] for r in alpha_quads}
    beta_subjects = {r["subject"] for r in beta_quads}
    assert not alpha_subjects & beta_subjects, (
        f"cross-world subject leak: {alpha_subjects & beta_subjects!r}"
    )


# ── AC#4: empty-case oracle parity ───────────────────────────────────────────


def test_materialize_empty_input_returns_empty_list() -> None:
    """materialize on empty input must return an empty list."""
    result = gmeow_logic.materialize("", "")
    assert result == [], f"expected [], got {result!r}"


def test_materialize_whitespace_only_input_returns_empty_list() -> None:
    """materialize on whitespace-only input must return an empty list."""
    result = gmeow_logic.materialize("", "   \n  \t  ")
    assert result == [], f"expected [], got {result!r}"


def test_empty_case_oracle_parity() -> None:
    """AC#4: empty materialize output == empty LogicProgram canonical output.

    The Python oracle (parse_logic_source) raises LogicParseError on empty
    graphs, so the canonical empty-program state is constructed directly
    (zero axioms, rules, profiles).  Both the Rust materialize and the Python
    oracle must agree: empty in → empty/zero out.
    """
    from gmeow_tools.logic_frontend import LogicParseError
    from gmeow_tools.logic_ir import LogicProgram

    # Rust side: empty input → empty result list.
    rust_result = gmeow_logic.materialize("", "")
    assert rust_result == [], "Rust materialize: empty input must return empty list"

    # Python oracle side: an empty graph raises LogicParseError; the canonical
    # empty LogicProgram (no axioms/rules/profiles) represents the zero state.
    empty_program = LogicProgram(axioms=(), rules=(), profiles=())
    canonical = empty_program.canonical()

    assert canonical["axioms"] == [], (
        f"oracle: empty LogicProgram should have no axioms, got {canonical['axioms']!r}"
    )
    assert canonical["rules"] == [], (
        f"oracle: empty LogicProgram should have no rules, got {canonical['rules']!r}"
    )
    assert canonical["profiles"] == [], (
        f"oracle: empty LogicProgram should have no profiles, "
        f"got {canonical['profiles']!r}"
    )

    # The parity assertion: both sides agree on the empty/trivial case.
    # Rust: 0 materialized quads.  Python: 0 axioms, 0 rules, 0 profiles.
    assert len(rust_result) == 0
    assert len(canonical["axioms"]) == 0
    assert len(canonical["rules"]) == 0
    assert len(canonical["profiles"]) == 0

    # Confirm the oracle would raise on empty graph input (fail-fast contract).
    from rdflib import Graph

    with pytest.raises(LogicParseError, match="empty"):
        from gmeow_tools.logic_frontend import parse_logic_source

        parse_logic_source(Graph())


# ── AC#5: real inference with transitivity rules ─────────────────────────────

# N-Quads: a subClassOf chain in one world.
#   Dog subClassOf Mammal (Alpha)
#   Mammal subClassOf Animal (Alpha)
_SUBCLASS_PRED = "<https://blackcatinformatics.ca/logic/subClassOf>"
_DOG = "<http://example.org/Dog>"
_MAMMAL = "<http://example.org/Mammal>"
_ANIMAL = "<http://example.org/Animal>"
_W_ALPHA = "<http://world/Alpha>"

_CHAIN_NQUADS = (
    f"{_DOG} {_SUBCLASS_PRED} {_MAMMAL} {_W_ALPHA} .\n"
    f"{_MAMMAL} {_SUBCLASS_PRED} {_ANIMAL} {_W_ALPHA} .\n"
)

# Transitivity rule in Nemo IRI-predicate syntax.
# Derives: ?X subClassOf ?Z if ?X subClassOf ?Y and ?Y subClassOf ?Z.
# The head context uses ?C0 (the context from the first body atom) — Nemo
# requires every head variable to appear in the body (safety constraint).
_TRANSITIVITY_RULES = """\
<https://blackcatinformatics.ca/logic/subClassOf>(?X, ?Z, ?C0) :-
    <https://blackcatinformatics.ca/logic/subClassOf>(?X, ?Y, ?C0),
    <https://blackcatinformatics.ca/logic/subClassOf>(?Y, ?Z, ?C1) .
"""


def test_materialize_real_inference_derives_transitive_quad() -> None:
    """AC#5: transitivity rule fires and the derived quad (Dog, Animal) appears.

    This is the critical witness that the Nemo chase actually runs rules —
    the Dog → Animal link is not present in the input but must appear in the
    result after a two-step transitivity derivation.
    """
    result = gmeow_logic.materialize(_TRANSITIVITY_RULES, _CHAIN_NQUADS)
    assert isinstance(result, list)

    # Collect all (subject, object) pairs for the subClassOf predicate
    sco_pred = "https://blackcatinformatics.ca/logic/subClassOf"
    sco_pairs = {
        (r["subject"], r["object"]) for r in result if r["predicate"] == sco_pred
    }

    # The transitive closure fact: Dog subClassOf Animal
    expected_subject = "<http://example.org/Dog>"
    expected_object = "<http://example.org/Animal>"
    assert (expected_subject, expected_object) in sco_pairs, (
        f"Expected transitive closure (Dog, Animal) not found in derived facts.\n"
        f"Derived subClassOf pairs: {sorted(sco_pairs)}"
    )


def test_materialize_inference_world_isolation() -> None:
    """World isolation holds under real inference: quads stay in the input world.

    The transitivity derivation fires within world Alpha (the world of both
    EDB facts).  No quads should appear in any other world.
    """
    result = gmeow_logic.materialize(_TRANSITIVITY_RULES, _CHAIN_NQUADS)
    assert isinstance(result, list)
    assert len(result) > 0, "expected at least some derived quads"

    worlds_found = {r["graph"] for r in result}
    # All quads must belong to Alpha
    assert worlds_found == {"http://world/Alpha"}, (
        f"Expected only world Alpha, got: {worlds_found!r}"
    )


def test_materialize_inference_input_quads_still_present() -> None:
    """Input (EDB) quads are still present in the result alongside derived quads.

    Nemo returns EDB facts as derived predicates, so the input chain must
    appear alongside the newly derived transitive quad.
    """
    result = gmeow_logic.materialize(_TRANSITIVITY_RULES, _CHAIN_NQUADS)
    sco_pred = "https://blackcatinformatics.ca/logic/subClassOf"
    sco_pairs = {
        (r["subject"], r["object"]) for r in result if r["predicate"] == sco_pred
    }

    # Both input quads must still be present
    assert ("<http://example.org/Dog>", "<http://example.org/Mammal>") in sco_pairs, (
        "Input quad Dog→Mammal missing from derived result"
    )
    mammal_animal = ("<http://example.org/Mammal>", "<http://example.org/Animal>")
    assert mammal_animal in sco_pairs, (
        "Input quad Mammal→Animal missing from derived result"
    )


# ── AC#6: real provenance on derived quads ────────────────────────────────────

_ASSERT_RULE_IRI = "https://blackcatinformatics.ca/logic/assert"
_ANON_RULE_IRI = "https://blackcatinformatics.ca/logic/rule/anonymous"

# Named-rule variant: uses #[name("...")] so rule_iri flows through as the IRI.
_NAMED_RULE_IRI = "https://blackcatinformatics.ca/logic/rules/subClassOf-transitivity"
_NAMED_TRANSITIVITY_RULES = f"""\
#[name("{_NAMED_RULE_IRI}")]
<https://blackcatinformatics.ca/logic/subClassOf>(?X, ?Z, ?C0) :-
    <https://blackcatinformatics.ca/logic/subClassOf>(?X, ?Y, ?C0),
    <https://blackcatinformatics.ca/logic/subClassOf>(?Y, ?Z, ?C1) .
"""


def test_derived_quad_has_nonempty_source_quad_ids() -> None:
    """Task 4 AC: derived quad must carry non-empty source_quad_ids (real antecedents).

    The Dog→Animal transitive closure fact is derived from Dog→Mammal and
    Mammal→Animal.  Its source_quad_ids must be non-empty, identifying those
    antecedent quads by their reifier IRIs.
    """
    result = gmeow_logic.materialize(_TRANSITIVITY_RULES, _CHAIN_NQUADS)
    sco_pred = "https://blackcatinformatics.ca/logic/subClassOf"

    # Find the derived Dog→Animal transitive quad (not in input).
    derived = [
        r
        for r in result
        if r["predicate"] == sco_pred
        and r["subject"] == "<http://example.org/Dog>"
        and r["object"] == "<http://example.org/Animal>"
    ]
    assert len(derived) == 1, (
        f"Expected exactly one Dog→Animal derived quad, got {len(derived)}"
    )
    dog_animal = derived[0]

    # The derived quad must have real antecedents.
    assert isinstance(dog_animal["source_quad_ids"], list), (
        "source_quad_ids must be a list"
    )
    assert len(dog_animal["source_quad_ids"]) > 0, (
        f"Derived Dog→Animal quad has empty source_quad_ids — "
        f"real provenance antecedents were not populated.\n"
        f"Full quad: {dog_animal!r}"
    )
    # Every source IRI must be a non-empty string (reifier IRI format).
    for src in dog_animal["source_quad_ids"]:
        assert isinstance(src, str) and src, (
            f"source_quad_ids entry is not a non-empty string: {src!r}"
        )


def test_derived_quad_rule_iri_is_not_assert_sentinel() -> None:
    """Task 4 AC: derived quads must carry the firing rule IRI, not logic:assert.

    Asserted (EDB) facts carry rule_iri = logic:assert.  A derived (IDB) quad
    — like Dog→Animal — must carry a different rule_iri (the fired rule).
    """
    result = gmeow_logic.materialize(_TRANSITIVITY_RULES, _CHAIN_NQUADS)
    sco_pred = "https://blackcatinformatics.ca/logic/subClassOf"

    derived = [
        r
        for r in result
        if r["predicate"] == sco_pred
        and r["subject"] == "<http://example.org/Dog>"
        and r["object"] == "<http://example.org/Animal>"
    ]
    assert len(derived) == 1, (
        f"Expected exactly one Dog→Animal derived quad, got {len(derived)}"
    )
    dog_animal = derived[0]

    assert dog_animal["rule_iri"] != _ASSERT_RULE_IRI, (
        f"Derived quad must NOT carry rule_iri=logic:assert — "
        f"got {dog_animal['rule_iri']!r} (assert sentinel means the quad was "
        f"treated as asserted rather than derived)"
    )


def test_named_rule_iri_flows_through_to_derived_quad() -> None:
    """Task 4 AC: when a rule carries #[name('iri')], that IRI appears in rule_iri.

    Uses _NAMED_TRANSITIVITY_RULES which emits the rule name as a full IRI.
    The derived Dog→Animal quad must carry exactly that IRI as its rule_iri.
    """
    result = gmeow_logic.materialize(_NAMED_TRANSITIVITY_RULES, _CHAIN_NQUADS)
    sco_pred = "https://blackcatinformatics.ca/logic/subClassOf"

    derived = [
        r
        for r in result
        if r["predicate"] == sco_pred
        and r["subject"] == "<http://example.org/Dog>"
        and r["object"] == "<http://example.org/Animal>"
    ]
    assert len(derived) == 1, (
        f"Expected exactly one Dog→Animal derived quad, got {len(derived)}"
    )
    dog_animal = derived[0]

    assert dog_animal["rule_iri"] == _NAMED_RULE_IRI, (
        f"Named-rule IRI did not flow through to derived quad.\n"
        f"Expected: {_NAMED_RULE_IRI!r}\n"
        f"Got:      {dog_animal['rule_iri']!r}"
    )


def test_asserted_quads_carry_assert_sentinel_rule_iri() -> None:
    """Asserted (EDB) quads must carry rule_iri = logic:assert.

    This is the complement of the derived-quad test: input facts must be tagged
    with the assert sentinel, not with a logic rule IRI.
    """
    result = gmeow_logic.materialize("", _TWO_WORLD_NQUADS)
    for i, quad_dict in enumerate(result):
        assert quad_dict["rule_iri"] == _ASSERT_RULE_IRI, (
            f"quad[{i}]: asserted quad should carry rule_iri={_ASSERT_RULE_IRI!r}, "
            f"got {quad_dict['rule_iri']!r}"
        )
