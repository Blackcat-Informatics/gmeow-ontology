# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for the ``logic:distinctBody`` inequality body guard (issue #503).

The guard adds a rule-level ``?A != ?B`` constraint to the GMEOW Logic IR — an
additive, byte-stable extension that mirrors how issue #502 added
``logic:negatedBody``.  Covered here:

1. A ``logic:Rule`` with a ``logic:distinctBody`` node parses into
   ``LogicRule.distinct_pairs``.
2. Round-trip isomorphism: parse → ``project_canonical_rdf12`` → re-parse yields
   an equal rule (the inequality guard survives serialize→re-parse).
3. ``project_nemo`` emits the ``?A != ?B`` inequality constraint.
4. (The guarded materialization itself — derive when distinct, skip when equal —
   is Rust-authoritative since #651; see the section-4 note below.)
5. Byte-stability: a rule with NO guard is canonically/sort-key-identical to its
   pre-#503 form.
"""

from __future__ import annotations

from pathlib import Path

from rdflib import RDF, BNode, Graph, Literal, Namespace

from gmeow_tools.config import LOGIC_NAMESPACE
from gmeow_tools.logic_frontend import WARNING, parse_logic_source
from gmeow_tools.logic_ir import (
    LogicAxiom,
    LogicProgram,
    LogicRule,
)
from gmeow_tools.logic_projections import (
    project_canonical_rdf12,
    project_nemo,
)

LOGIC = Namespace(LOGIC_NAMESPACE)
EX = Namespace("https://example.org/test/")


# --------------------------------------------------------------------------- #
# Graph fixtures
# --------------------------------------------------------------------------- #


def _distinct_rule_graph() -> Graph:
    """A ``logic:Rule`` whose body has two atoms guarded by ``?A != ?B``.

    Models: ``hasPair(?A, ?B) :- stereotypeOf(?C, ?A), stereotypeOf(?C, ?B),
    ?A != ?B`` — the "≥2 distinct values" idiom the foundation-lowering rules
    need (a class with more than one stereotype).
    """
    g = Graph()
    g.add((EX.s, RDF.type, LOGIC.Kind))

    rule = EX.distinctRule
    g.add((rule, RDF.type, LOGIC.Rule))

    head = BNode("dist_head")
    g.add((head, RDF.subject, Literal("?A")))
    g.add((head, RDF.predicate, LOGIC.hasPair))
    g.add((head, RDF.object, Literal("?B")))
    g.add((rule, LOGIC.head, head))

    body_a = BNode("dist_body_a")
    g.add((body_a, RDF.subject, Literal("?C")))
    g.add((body_a, RDF.predicate, LOGIC.stereotypeOf))
    g.add((body_a, RDF.object, Literal("?A")))
    g.add((rule, LOGIC.body, body_a))

    body_b = BNode("dist_body_b")
    g.add((body_b, RDF.subject, Literal("?C")))
    g.add((body_b, RDF.predicate, LOGIC.stereotypeOf))
    g.add((body_b, RDF.object, Literal("?B")))
    g.add((rule, LOGIC.body, body_b))

    # The inequality guard: NO rdf:predicate, only rdf:subject / rdf:object.
    distinct = BNode("dist_guard")
    g.add((distinct, RDF.subject, Literal("?A")))
    g.add((distinct, RDF.object, Literal("?B")))
    g.add((rule, LOGIC.distinctBody, distinct))

    return g


# --------------------------------------------------------------------------- #
# (1) Parsing
# --------------------------------------------------------------------------- #


def test_distinct_body_parses_into_distinct_pairs() -> None:
    """A logic:distinctBody node parses into LogicRule.distinct_pairs."""
    prog, diags = parse_logic_source(_distinct_rule_graph())

    errors = [d for d in diags if d.severity == "ERROR"]
    assert not errors, f"Unexpected errors: {errors}"

    assert len(prog.rules) == 1
    rule = prog.rules[0]
    # Canonicalised: members sorted within the pair, pairs sorted as a whole.
    assert rule.distinct_pairs == (("?A", "?B"),)
    # The guard is NOT a body atom — body stays exactly the two positive atoms.
    assert len(rule.body) == 2


def test_distinct_body_missing_subject_emits_warning() -> None:
    """A logic:distinctBody node lacking rdf:subject emits a WARNING, is skipped."""
    g = Graph()
    g.add((EX.s, RDF.type, LOGIC.Kind))
    rule = EX.badGuardRule
    g.add((rule, RDF.type, LOGIC.Rule))

    head = BNode("bg_head")
    g.add((head, RDF.subject, Literal("?A")))
    g.add((head, RDF.predicate, LOGIC.hasPair))
    g.add((head, RDF.object, Literal("?B")))
    g.add((rule, LOGIC.head, head))

    body = BNode("bg_body")
    g.add((body, RDF.subject, Literal("?A")))
    g.add((body, RDF.predicate, LOGIC.stereotypeOf))
    g.add((body, RDF.object, Literal("?B")))
    g.add((rule, LOGIC.body, body))

    # Guard with only rdf:object — missing rdf:subject.
    distinct = BNode("bg_guard")
    g.add((distinct, RDF.object, Literal("?B")))
    g.add((rule, LOGIC.distinctBody, distinct))

    prog, diags = parse_logic_source(g)

    warning_codes = [d.code for d in diags if d.severity == WARNING]
    assert "MALFORMED_RULE_BODY" in warning_codes
    assert prog.rules[0].distinct_pairs == ()


def test_distinct_body_constant_term_emits_warning() -> None:
    """A distinctBody guard with a non-variable (constant) term is rejected.

    A constant would parse here but then crash the materializer — the guard
    variable is never bound by the body (MaterializationError).  The frontend must
    reject it with a WARNING and skip the guard (issue #503 review, PR #605).
    """
    g = Graph()
    g.add((EX.s, RDF.type, LOGIC.Kind))
    rule = EX.constGuardRule
    g.add((rule, RDF.type, LOGIC.Rule))

    head = BNode("cg_head")
    g.add((head, RDF.subject, Literal("?A")))
    g.add((head, RDF.predicate, LOGIC.hasPair))
    g.add((head, RDF.object, Literal("?B")))
    g.add((rule, LOGIC.head, head))

    body = BNode("cg_body")
    g.add((body, RDF.subject, Literal("?A")))
    g.add((body, RDF.predicate, LOGIC.stereotypeOf))
    g.add((body, RDF.object, Literal("?B")))
    g.add((rule, LOGIC.body, body))

    # Guard whose subject is a CONSTANT IRI string, not a ?-variable.
    distinct = BNode("cg_guard")
    g.add((distinct, RDF.subject, Literal(str(EX.notAVar))))
    g.add((distinct, RDF.object, Literal("?B")))
    g.add((rule, LOGIC.distinctBody, distinct))

    prog, diags = parse_logic_source(g)

    warning_codes = [d.code for d in diags if d.severity == WARNING]
    assert "MALFORMED_RULE_BODY" in warning_codes
    # The malformed guard is skipped — no inequality pair is recorded.
    assert prog.rules[0].distinct_pairs == ()


def test_distinct_pairs_canonicalised_symmetric() -> None:
    """Inequality is symmetric: members are sorted within each pair."""
    head = LogicAxiom(subject="?A", predicate=str(LOGIC.hasPair), obj="?B")
    body = (
        LogicAxiom(subject="?C", predicate=str(LOGIC.stereotypeOf), obj="?A"),
        LogicAxiom(subject="?C", predicate=str(LOGIC.stereotypeOf), obj="?B"),
    )
    # Constructed reversed-member, multiple pairs out of order.
    rule = LogicRule(
        head=head,
        body=body,
        distinct_pairs=(("?B", "?A"), ("?D", "?C")),
    )
    assert rule.distinct_pairs == (("?A", "?B"), ("?C", "?D"))


# --------------------------------------------------------------------------- #
# (2) Round-trip isomorphism
# --------------------------------------------------------------------------- #


def test_distinct_body_round_trip_isomorphic(tmp_path: Path) -> None:
    """parse → project_canonical_rdf12 → re-parse yields an equal rule."""
    prog, _ = parse_logic_source(_distinct_rule_graph())
    assert prog.rules[0].distinct_pairs == (("?A", "?B"),)

    result = project_canonical_rdf12(prog)
    ttl_path = tmp_path / "distinct.ttl"
    ttl_path.write_text(result.content, encoding="utf-8")

    reparsed, diags = parse_logic_source(ttl_path)
    errors = [d for d in diags if d.severity == "ERROR"]
    assert not errors, f"Re-parse produced errors: {errors}"

    assert len(reparsed.rules) == 1
    # The whole rule (head + body + guard) must round-trip identically.
    assert reparsed.rules[0] == prog.rules[0]
    assert reparsed.rules[0].distinct_pairs == (("?A", "?B"),)


def test_distinct_body_in_canonical_rdf12_no_predicate() -> None:
    """The emitted distinctBody node carries no rdf:predicate (comparisons lack one)."""
    prog, _ = parse_logic_source(_distinct_rule_graph())
    result = project_canonical_rdf12(prog)
    assert result.graph is not None

    distinct_nodes = list(result.graph.objects(None, LOGIC.distinctBody))
    assert distinct_nodes, "Expected a logic:distinctBody node in the projection"
    for node in distinct_nodes:
        assert (node, RDF.subject, None) in result.graph
        assert (node, RDF.object, None) in result.graph
        # NO rdf:predicate on an inequality guard node.
        assert (node, RDF.predicate, None) not in result.graph


# --------------------------------------------------------------------------- #
# (3) Nemo projection emits the ?A != ?B constraint
# --------------------------------------------------------------------------- #


def test_nemo_emits_inequality_constraint() -> None:
    """project_nemo output contains the ?A != ?B inequality constraint."""
    prog, _ = parse_logic_source(_distinct_rule_graph())
    result = project_nemo(prog)
    assert "?A != ?B" in result.content


# --------------------------------------------------------------------------- #
# (4) The guarded materialization itself (derive when distinct, skip when equal)
# is Rust-authoritative since #651 — pinned by the ``foundation/relcomp-under-
# mediated`` conformance case (whose STRATA rule carries the ``?R1 != ?R2``
# guard) and the ``crates/logic`` cargo tests, not by a Python oracle re-run.
# --------------------------------------------------------------------------- #


# --------------------------------------------------------------------------- #
# (5) Byte-stability: a rule with NO guard is unchanged
# --------------------------------------------------------------------------- #


def test_no_guard_rule_byte_stable() -> None:
    """A rule with distinct_pairs=() has the exact pre-#503 sort/canonical form."""
    head = LogicAxiom(subject="?A", predicate=str(LOGIC.hasPair), obj="?B")
    body = (LogicAxiom(subject="?C", predicate=str(LOGIC.stereotypeOf), obj="?A"),)
    rule = LogicRule(head=head, body=body)

    # Empty distinct_pairs ⇒ NO trailing distinct segment in the sort key.
    expected = f"{head._sort_key()}\x00{body[0]._sort_key()}"
    assert rule._sort_key() == expected

    # Empty distinct_pairs ⇒ NO "distinct" key in canonical().
    prog = LogicProgram(axioms=(), rules=(rule,), profiles=())
    rule_dict = prog.canonical()["rules"][0]
    assert "distinct" not in rule_dict


def test_guarded_rule_adds_distinct_only_when_present() -> None:
    """A guarded rule's canonical() carries a 'distinct' key; sort key differs."""
    head = LogicAxiom(subject="?A", predicate=str(LOGIC.hasPair), obj="?B")
    body = (
        LogicAxiom(subject="?C", predicate=str(LOGIC.stereotypeOf), obj="?A"),
        LogicAxiom(subject="?C", predicate=str(LOGIC.stereotypeOf), obj="?B"),
    )
    guarded = LogicRule(head=head, body=body, distinct_pairs=(("?A", "?B"),))
    unguarded = LogicRule(head=head, body=body)

    # The guard changes the sort key (so guarded/unguarded rules don't collide).
    assert guarded._sort_key() != unguarded._sort_key()

    prog = LogicProgram(axioms=(), rules=(guarded,), profiles=())
    rule_dict = prog.canonical()["rules"][0]
    assert rule_dict["distinct"] == [["?A", "?B"]]
