# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Tests for the logic: RDF 1.2 front-end parser (issue #500, Task 1).

Module under test: ``logic_frontend.py``.

Covers:
* Parsing an in-memory graph with ``logic:`` axioms → populated LogicProgram.
* SemanticProfile declarations → LogicProfile entries.
* RDF 1.2 reified statements with scope annotations → scoped LogicAxiom.
* Classic rdf:Statement with scope annotations → scoped LogicAxiom.
* Malformed graph (missing predicate) → WARNING diagnostic emitted, not raised.
* Empty graph → LogicParseError raised.
* Non-existent file → LogicParseError raised.
* logic:Rule nodes → LogicRule entries.
* Unknown logic:SemanticProfile IRI → WARNING diagnostic.
* Confidence annotation → ContextualScope.confidence populated.
* Modality annotation → ContextualScope.modality populated.
* Provenance annotation → ContextualScope.provenance populated.
"""

from __future__ import annotations

from collections.abc import Sequence
from pathlib import Path

import pytest
from rdflib import RDF, XSD, BNode, Graph, Literal, Namespace
from rdflib.term import Node

from gmeow_tools.config import LOGIC_NAMESPACE
from gmeow_tools.logic_frontend import (
    WARNING,
    LogicParseError,
    parse_logic_source,
)
from gmeow_tools.logic_ir import (
    LogicModality,
    SemanticProfileId,
)

LOGIC = Namespace(LOGIC_NAMESPACE)
EX = Namespace("https://example.org/test/")


# --------------------------------------------------------------------------- #
# Helpers
# --------------------------------------------------------------------------- #


def _empty_graph() -> Graph:
    return Graph()


def _minimal_graph() -> Graph:
    """A graph with one logic: axiom and one SemanticProfile declaration."""
    g = Graph()
    # Declare a Person as a logic:Kind instance
    g.add((EX.Person, RDF.type, LOGIC.Kind))
    # Declare the profile
    g.add((LOGIC.PositiveHornProfile, RDF.type, LOGIC.SemanticProfile))
    return g


# --------------------------------------------------------------------------- #
# Basic happy-path tests
# --------------------------------------------------------------------------- #


def test_parse_empty_graph_raises() -> None:
    with pytest.raises(LogicParseError, match="empty"):
        parse_logic_source(_empty_graph())


def test_parse_minimal_graph_succeeds() -> None:
    prog, _diags = parse_logic_source(_minimal_graph())
    # Should have found at least one axiom (rdf:type logic:Kind)
    assert len(prog.axioms) >= 1
    rdf_type_str = str(RDF.type)
    kind_iri = LOGIC_NAMESPACE + "Kind"
    matching = [
        a for a in prog.axioms if a.predicate == rdf_type_str and a.obj == kind_iri
    ]
    assert matching, f"Expected a rdf:type logic:Kind axiom; got {prog.axioms}"


def test_parse_profiles_extracted() -> None:
    g = _minimal_graph()
    prog, _diags = parse_logic_source(g)
    assert len(prog.profiles) == 1
    assert prog.profiles[0].profile_id is SemanticProfileId.POSITIVE_HORN


def test_parse_multiple_profiles() -> None:
    g = Graph()
    g.add((EX.s, RDF.type, LOGIC.Kind))
    g.add((LOGIC.PositiveHornProfile, RDF.type, LOGIC.SemanticProfile))
    g.add((LOGIC.StableModelProfile, RDF.type, LOGIC.SemanticProfile))
    prog, _diags = parse_logic_source(g)
    profile_ids = {p.profile_id for p in prog.profiles}
    assert SemanticProfileId.POSITIVE_HORN in profile_ids
    assert SemanticProfileId.STABLE_MODEL in profile_ids


def test_parse_logic_relation_axiom() -> None:
    """A triple with a logic: predicate (not rdf:type) should be extracted."""
    g = Graph()
    g.add((EX.Person, LOGIC.rigidlyAppliesTo, EX.Human))
    prog, _diags = parse_logic_source(g)
    pred_str = LOGIC_NAMESPACE + "rigidlyAppliesTo"
    matching = [a for a in prog.axioms if a.predicate == pred_str]
    assert matching, "Expected a logic:rigidlyAppliesTo axiom"
    assert matching[0].subject == str(EX.Person)
    assert matching[0].obj == str(EX.Human)
    assert not matching[0].obj_is_literal


def test_parse_literal_object_sets_flag() -> None:
    """A literal object sets obj_is_literal=True."""
    g = Graph()
    g.add((EX.s, LOGIC.confidence, Literal("0.9", datatype=XSD.decimal)))
    prog, _diags = parse_logic_source(g)
    pred_str = LOGIC_NAMESPACE + "confidence"
    matching = [a for a in prog.axioms if a.predicate == pred_str]
    assert matching
    assert matching[0].obj_is_literal is True


# --------------------------------------------------------------------------- #
# Source IRI provenance
# --------------------------------------------------------------------------- #


def test_parse_source_iri_is_stored() -> None:
    prog, _ = parse_logic_source(_minimal_graph(), source_iri="https://example.org/src")
    assert prog.source_iri == "https://example.org/src"


def test_parse_source_iri_defaults_to_none_for_graph() -> None:
    prog, _ = parse_logic_source(_minimal_graph())
    assert prog.source_iri is None


# --------------------------------------------------------------------------- #
# Contextual scope via classic rdf:Statement reification
# --------------------------------------------------------------------------- #


def test_parse_classic_reification_with_scope() -> None:
    """An rdf:Statement node with logic: scope annotations produces a scoped axiom."""
    g = Graph()
    # The base triple (not reified separately — the rdf:Statement IS the axiom)
    stmt = BNode("stmt1")
    g.add((stmt, RDF.type, RDF.Statement))
    g.add((stmt, RDF.subject, EX.Person))
    g.add((stmt, RDF.predicate, LOGIC.Kind))
    g.add((stmt, RDF.object, EX.HumanKind))
    # Scope annotations
    g.add((stmt, LOGIC.confidence, Literal("0.85", datatype=XSD.decimal)))
    g.add((stmt, LOGIC.provenance, EX.agent))

    prog, diags = parse_logic_source(g)

    kind_str = LOGIC_NAMESPACE + "Kind"
    scoped = [
        a
        for a in prog.axioms
        if a.predicate == kind_str and a.scope.confidence is not None
    ]
    assert scoped, f"No scoped axiom found; axioms={prog.axioms}, diags={diags}"
    ax = scoped[0]
    assert ax.scope.confidence == pytest.approx(0.85)
    assert ax.scope.provenance == str(EX.agent)


def test_parse_modality_annotation() -> None:
    g = Graph()
    stmt = BNode("stmt2")
    g.add((stmt, RDF.type, RDF.Statement))
    g.add((stmt, RDF.subject, EX.s))
    g.add((stmt, RDF.predicate, LOGIC.suppliesIdentity))
    g.add((stmt, RDF.object, EX.o))
    g.add((stmt, LOGIC.confidence, Literal("0.7", datatype=XSD.decimal)))
    g.add((stmt, LOGIC.modality, Literal("epistemic")))

    prog, _diags = parse_logic_source(g)

    pred_str = LOGIC_NAMESPACE + "suppliesIdentity"
    scoped = [
        a
        for a in prog.axioms
        if a.predicate == pred_str and a.scope.confidence is not None
    ]
    assert scoped
    assert scoped[0].scope.modality is LogicModality.EPISTEMIC


# --------------------------------------------------------------------------- #
# Diagnostics for malformed input
# --------------------------------------------------------------------------- #


def test_malformed_reification_missing_predicate_emits_diagnostic() -> None:
    """An rdf:Statement node with a scope but no rdf:predicate should emit a WARNING."""
    g = Graph()
    stmt = BNode("bad_stmt")
    g.add((stmt, RDF.type, RDF.Statement))
    g.add((stmt, RDF.subject, EX.s))
    # deliberately omit rdf:predicate
    g.add((stmt, RDF.object, EX.o))
    g.add((stmt, LOGIC.confidence, Literal("0.5", datatype=XSD.decimal)))

    _prog, diags = parse_logic_source(g)

    warning_codes = [d.code for d in diags if d.severity == WARNING]
    assert "MISSING_PREDICATE" in warning_codes, (
        f"Expected MISSING_PREDICATE; got {diags}"
    )


def test_unknown_semantic_profile_emits_diagnostic() -> None:
    """An IRI declared as logic:SemanticProfile but not a known individual → WARNING."""
    g = Graph()
    g.add((EX.s, RDF.type, LOGIC.Kind))
    unknown_profile = EX.SomeCustomProfile
    g.add((unknown_profile, RDF.type, LOGIC.SemanticProfile))

    prog, diags = parse_logic_source(g)

    warning_codes = [d.code for d in diags if d.severity == WARNING]
    assert "UNKNOWN_PROFILE" in warning_codes, f"Expected UNKNOWN_PROFILE; got {diags}"
    # The unknown profile must NOT appear in prog.profiles
    assert all(p.profile_id.value != "SomeCustomProfile" for p in prog.profiles)


def test_invalid_confidence_emits_diagnostic() -> None:
    """A confidence value outside [0, 1] emits a diagnostic and is skipped."""
    g = Graph()
    stmt = BNode("bad_conf")
    g.add((stmt, RDF.type, RDF.Statement))
    g.add((stmt, RDF.subject, EX.s))
    g.add((stmt, RDF.predicate, LOGIC.Kind))
    g.add((stmt, RDF.object, EX.o))
    # confidence > 1 should fail
    g.add((stmt, LOGIC.confidence, Literal("2.5", datatype=XSD.decimal)))

    _prog, diags = parse_logic_source(g)

    warning_codes = [d.code for d in diags if d.severity == WARNING]
    assert "INVALID_CONFIDENCE" in warning_codes, (
        f"Expected INVALID_CONFIDENCE; got {diags}"
    )


# --------------------------------------------------------------------------- #
# File-loading error paths
# --------------------------------------------------------------------------- #


def test_nonexistent_file_raises(tmp_path: Path) -> None:
    bad = tmp_path / "nonexistent.ttl"
    with pytest.raises(LogicParseError, match="does not exist"):
        parse_logic_source(bad)


def test_invalid_turtle_raises(tmp_path: Path) -> None:
    bad = tmp_path / "bad.ttl"
    bad.write_text("this is not valid turtle @@@ !!!", encoding="utf-8")
    with pytest.raises(LogicParseError):
        parse_logic_source(bad)


def test_file_source_iri_defaults_to_file_uri(tmp_path: Path) -> None:
    ttl = tmp_path / "test.ttl"
    ttl.write_text(
        "@prefix logic: <https://blackcatinformatics.ca/logic/> .\n"
        "@prefix ex: <https://example.org/> .\n"
        "ex:Person a logic:Kind .\n",
        encoding="utf-8",
    )
    prog, _ = parse_logic_source(ttl)
    assert prog.source_iri == ttl.as_uri()


# --------------------------------------------------------------------------- #
# Rule extraction (forward-compat: absent logic:Rule → empty list, no error)
# --------------------------------------------------------------------------- #


def test_no_rule_nodes_yields_empty_rules() -> None:
    g = _minimal_graph()
    prog, _diags = parse_logic_source(g)
    assert prog.rules == ()


def test_rule_node_extracted() -> None:
    """A logic:Rule node with head and body atoms is extracted to a LogicRule."""
    g = Graph()
    # A minimal axiom so the graph is non-empty beyond the rule
    g.add((EX.s, RDF.type, LOGIC.Kind))

    # The rule node
    rule = EX.myRule
    g.add((rule, RDF.type, LOGIC.Rule))

    # Head atom (expressed as a reified-style triple node)
    head = BNode("head_atom")
    g.add((head, RDF.subject, EX.x))
    g.add((head, RDF.predicate, LOGIC.rigidlyAppliesTo))
    g.add((head, RDF.object, EX.y))
    g.add((rule, LOGIC.head, head))

    # Body atom
    body_atom = BNode("body_atom")
    g.add((body_atom, RDF.subject, EX.x))
    g.add((body_atom, RDF.predicate, LOGIC.mediates))
    g.add((body_atom, RDF.object, EX.z))
    g.add((rule, LOGIC.body, body_atom))

    prog, _diags = parse_logic_source(g)

    assert len(prog.rules) == 1
    r = prog.rules[0]
    assert r.head.predicate == LOGIC_NAMESPACE + "rigidlyAppliesTo"
    assert len(r.body) == 1
    assert r.body[0].predicate == LOGIC_NAMESPACE + "mediates"


def test_rule_missing_head_emits_diagnostic() -> None:
    """A logic:Rule node with no logic:head emits a WARNING and is skipped."""
    g = Graph()
    g.add((EX.s, RDF.type, LOGIC.Kind))
    rule = EX.badRule
    g.add((rule, RDF.type, LOGIC.Rule))
    # no logic:head added

    prog, diags = parse_logic_source(g)

    assert prog.rules == ()
    warning_codes = [d.code for d in diags if d.severity == WARNING]
    assert "MISSING_RULE_HEAD" in warning_codes


# --------------------------------------------------------------------------- #
# INVALID_COMPLEXITY_CLASS diagnostic
# --------------------------------------------------------------------------- #


def test_empty_complexity_class_emits_diagnostic() -> None:
    """An empty logic:complexityClass literal emits INVALID_COMPLEXITY_CLASS."""
    g = Graph()
    g.add((EX.s, RDF.type, LOGIC.Kind))
    # Declare a known profile individual
    g.add((LOGIC.PositiveHornProfile, RDF.type, LOGIC.SemanticProfile))
    # Attach an empty complexityClass literal — should trigger the diagnostic
    g.add((LOGIC.PositiveHornProfile, LOGIC.complexityClass, Literal("")))

    prog, diags = parse_logic_source(g)

    warning_codes = [d.code for d in diags if d.severity == WARNING]
    assert "INVALID_COMPLEXITY_CLASS" in warning_codes, (
        f"Expected INVALID_COMPLEXITY_CLASS; got {diags}"
    )
    # The profile is still added (fail-soft), but with no complexity
    assert len(prog.profiles) == 1
    assert prog.profiles[0].complexity is None


def test_whitespace_complexity_class_emits_diagnostic() -> None:
    """A whitespace-only complexityClass literal emits INVALID_COMPLEXITY_CLASS."""
    g = Graph()
    g.add((EX.s, RDF.type, LOGIC.Kind))
    g.add((LOGIC.PositiveHornProfile, RDF.type, LOGIC.SemanticProfile))
    g.add((LOGIC.PositiveHornProfile, LOGIC.complexityClass, Literal("   ")))

    _prog, diags = parse_logic_source(g)

    warning_codes = [d.code for d in diags if d.severity == WARNING]
    assert "INVALID_COMPLEXITY_CLASS" in warning_codes, (
        f"Expected INVALID_COMPLEXITY_CLASS; got {diags}"
    )


# --------------------------------------------------------------------------- #
# Order-independence: two parses of equivalent graphs yield equal programs
# --------------------------------------------------------------------------- #


def test_parse_is_order_independent() -> None:
    """Two graphs with the same triples in different order yield equal programs."""

    def make_graph(order: Sequence[tuple[Node, Node, Node]]) -> Graph:
        g = Graph()
        for triple in order:
            g.add(triple)
        return g

    triples = [
        (EX.Person, RDF.type, LOGIC.Kind),
        (EX.Employee, RDF.type, LOGIC.Role),
        (LOGIC.PositiveHornProfile, RDF.type, LOGIC.SemanticProfile),
    ]

    prog1, _ = parse_logic_source(make_graph(triples))
    prog2, _ = parse_logic_source(make_graph(list(reversed(triples))))

    assert prog1 == prog2
    assert prog1.canonical() == prog2.canonical()
