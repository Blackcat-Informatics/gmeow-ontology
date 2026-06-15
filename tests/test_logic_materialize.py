# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Tests for the Python oracle forward materializer (issue #501, Task 1).

Covers:
* Transitive closure: a->b, b->c derives a->c (basic forward-chase correctness).
* World isolation: Alpha-world facts never appear in Beta-world derived output.
* No-occurrence gate: a valid fixture entails zero gufo:Event tokens; a fixture
  WITH a token Event raises NoOccurrenceViolationError.
* Determinism: same input twice produces identical IRIs and identical output.
* Empty-case: empty input produces empty result (oracle parity with AC#4).
* Golden-roundtrip: quad_reifier_iri and derivation_id_iri match every committed
  golden in fixtures/logic/determinism-goldens.json.
"""

from __future__ import annotations

import json
from hashlib import sha1
from pathlib import Path

import pytest
from rdflib import ConjunctiveGraph, Literal, Namespace, URIRef
from rdflib.namespace import RDF, RDFS, XSD

from gmeow_tools.config import NAMESPACE, PREFIXES
from gmeow_tools.logic_ir import (
    ContextualScope,
    LogicAxiom,
    LogicProfile,
    LogicProgram,
    LogicRule,
    SemanticProfileId,
)
from gmeow_tools.logic_materialize import (
    DerivedQuad,
    MaterializationResult,
    NoOccurrenceViolationError,
    derivation_id_iri,
    materialize_program,
    parse_nquads,
    quad_reifier_iri,
)

# --------------------------------------------------------------------------- #
# Paths
# --------------------------------------------------------------------------- #

_FIXTURES_DIR = Path(__file__).resolve().parent / "fixtures" / "logic"
_GOLDENS_PATH = _FIXTURES_DIR / "determinism-goldens.json"

# --------------------------------------------------------------------------- #
# Shared namespaces / IRIs for fixtures
# --------------------------------------------------------------------------- #

_EX = Namespace("http://example.org/")

# World IRIs
_W_ALPHA = URIRef("http://world/Alpha")
_W_BETA = URIRef("http://world/Beta")

# Test predicate and individuals
_RELATED = URIRef("http://example.org/related")
_A = URIRef("http://example.org/a")
_B = URIRef("http://example.org/b")
_C = URIRef("http://example.org/c")
_D = URIRef("http://example.org/d")


# --------------------------------------------------------------------------- #
# Fixture helpers
# --------------------------------------------------------------------------- #


def _empty_program() -> LogicProgram:
    return LogicProgram(axioms=(), rules=(), profiles=())


def _horn_program_no_rules() -> LogicProgram:
    return LogicProgram(
        axioms=(),
        rules=(),
        profiles=(LogicProfile(profile_id=SemanticProfileId.POSITIVE_HORN),),
    )


def _transitivity_program() -> LogicProgram:
    rule = LogicRule(
        head=LogicAxiom(
            subject="?x",
            predicate=str(_RELATED),
            obj="?z",
            obj_is_literal=False,
        ),
        body=(
            LogicAxiom(
                subject="?x",
                predicate=str(_RELATED),
                obj="?y",
                obj_is_literal=False,
            ),
            LogicAxiom(
                subject="?y",
                predicate=str(_RELATED),
                obj="?z",
                obj_is_literal=False,
            ),
        ),
        scope=ContextualScope(
            provenance=("https://blackcatinformatics.ca/logic/rules/transitivity")
        ),
    )
    return LogicProgram(
        axioms=(),
        rules=(rule,),
        profiles=(LogicProfile(profile_id=SemanticProfileId.POSITIVE_HORN),),
    )


def _cg_with_quads(
    quads: list[tuple[URIRef | Literal, URIRef, URIRef | Literal, URIRef]],
) -> ConjunctiveGraph:
    cg: ConjunctiveGraph = ConjunctiveGraph()
    for s, p, o, g in quads:
        named_graph = cg.get_context(g)
        named_graph.add((s, p, o))
    return cg


# --------------------------------------------------------------------------- #
# Seam-contract assertion helper
# --------------------------------------------------------------------------- #


def _assert_seam_contract(quad: DerivedQuad) -> None:
    assert isinstance(quad.graph, str) and quad.graph
    assert isinstance(quad.subject, str) and quad.subject
    assert isinstance(quad.predicate, str) and quad.predicate
    assert isinstance(quad.obj, str) and quad.obj
    assert quad.graph_component == quad.graph
    assert isinstance(quad.derivation_id, str) and quad.derivation_id
    assert isinstance(quad.rule_iri, str) and quad.rule_iri
    assert isinstance(quad.source_quad_ids, list)
    assert isinstance(quad.profile, str) and quad.profile
    assert quad.budget_status == "ok"


# --------------------------------------------------------------------------- #
# Tests: empty-case oracle parity
# --------------------------------------------------------------------------- #


class TestEmptyCase:
    def test_empty_nquads_parse(self) -> None:
        cg = parse_nquads("")
        assert len(list(cg.quads())) == 0

    def test_whitespace_nquads_parse(self) -> None:
        cg = parse_nquads("   \n\t  ")
        assert len(list(cg.quads())) == 0

    def test_materialize_empty_graph_empty_program(self) -> None:
        cg: ConjunctiveGraph = ConjunctiveGraph()
        result = materialize_program(_empty_program(), cg)
        assert isinstance(result, MaterializationResult)
        assert result.quads == ()
        assert result.worlds == frozenset()
        assert result.input_quad_count == 0
        assert result.derived_quad_count == 0
        assert result.loss_entries == ()

    def test_materialize_nquads_empty_roundtrip(self) -> None:
        cg = parse_nquads("")
        result = materialize_program(_empty_program(), cg)
        assert result.quads == ()
        assert result.worlds == frozenset()


# --------------------------------------------------------------------------- #
# Tests: transitive closure
# --------------------------------------------------------------------------- #


class TestTransitiveClosure:
    def _make_input(self) -> ConjunctiveGraph:
        return _cg_with_quads(
            [
                (_A, _RELATED, _B, _W_ALPHA),
                (_B, _RELATED, _C, _W_ALPHA),
            ]
        )

    def test_derived_triple_present(self) -> None:
        cg = self._make_input()
        result = materialize_program(_transitivity_program(), cg)
        derived_spg = {(q.subject, q.predicate, q.obj, q.graph) for q in result.quads}
        expected = (str(_A), str(_RELATED), _C.n3(), str(_W_ALPHA))
        assert expected in derived_spg

    def test_input_quads_present(self) -> None:
        cg = self._make_input()
        result = materialize_program(_transitivity_program(), cg)
        subjects = {q.subject for q in result.quads}
        assert str(_A) in subjects
        assert str(_B) in subjects

    def test_seam_contract_on_all_quads(self) -> None:
        cg = self._make_input()
        result = materialize_program(_transitivity_program(), cg)
        for q in result.quads:
            _assert_seam_contract(q)

    def test_three_hop_chain(self) -> None:
        e = URIRef("http://example.org/e")
        cg = _cg_with_quads(
            [
                (_A, _RELATED, _B, _W_ALPHA),
                (_B, _RELATED, _C, _W_ALPHA),
                (_C, _RELATED, _D, _W_ALPHA),
            ]
        )
        result = materialize_program(_transitivity_program(), cg)
        spo = {(q.subject, q.predicate, q.obj) for q in result.quads}
        assert (str(_A), str(_RELATED), _B.n3()) in spo
        assert (str(_B), str(_RELATED), _C.n3()) in spo
        assert (str(_C), str(_RELATED), _D.n3()) in spo
        assert (str(_A), str(_RELATED), _C.n3()) in spo, "a->c missing"
        assert (str(_B), str(_RELATED), _D.n3()) in spo, "b->d missing"
        assert (str(_A), str(_RELATED), _D.n3()) in spo, "a->d missing"
        _ = e  # suppress unused variable (defined for readability above)

    def test_count_is_correct(self) -> None:
        cg = self._make_input()
        result = materialize_program(_transitivity_program(), cg)
        alpha_quads = [q for q in result.quads if q.graph == str(_W_ALPHA)]
        assert len(alpha_quads) == 3

    def test_derived_rule_iri(self) -> None:
        cg = self._make_input()
        result = materialize_program(_transitivity_program(), cg)
        derived = [q for q in result.quads if q.subject == str(_A) and q.obj == _C.n3()]
        assert len(derived) == 1
        assert "transitivity" in derived[0].rule_iri

    def test_derived_source_quad_ids_nonempty(self) -> None:
        cg = self._make_input()
        result = materialize_program(_transitivity_program(), cg)
        derived = [q for q in result.quads if q.subject == str(_A) and q.obj == _C.n3()]
        assert len(derived) == 1
        assert len(derived[0].source_quad_ids) > 0


# --------------------------------------------------------------------------- #
# Tests: world isolation
# --------------------------------------------------------------------------- #


class TestWorldIsolation:
    def _make_two_world_input(self) -> ConjunctiveGraph:
        return _cg_with_quads(
            [
                (_A, _RELATED, _B, _W_ALPHA),
                (_C, _RELATED, _D, _W_BETA),
            ]
        )

    def test_worlds_present(self) -> None:
        cg = self._make_two_world_input()
        result = materialize_program(_transitivity_program(), cg)
        assert str(_W_ALPHA) in result.worlds
        assert str(_W_BETA) in result.worlds

    def test_alpha_subjects_not_in_beta(self) -> None:
        cg = self._make_two_world_input()
        result = materialize_program(_transitivity_program(), cg)
        beta_subjects = {q.subject for q in result.quads if q.graph == str(_W_BETA)}
        assert str(_A) not in beta_subjects
        assert str(_B) not in beta_subjects

    def test_beta_subjects_not_in_alpha(self) -> None:
        cg = self._make_two_world_input()
        result = materialize_program(_transitivity_program(), cg)
        alpha_subjects = {q.subject for q in result.quads if q.graph == str(_W_ALPHA)}
        assert str(_C) not in alpha_subjects
        assert str(_D) not in alpha_subjects

    def test_single_hop_no_derivation(self) -> None:
        cg = self._make_two_world_input()
        result = materialize_program(_transitivity_program(), cg)
        alpha_quads = [q for q in result.quads if q.graph == str(_W_ALPHA)]
        beta_quads = [q for q in result.quads if q.graph == str(_W_BETA)]
        assert len(alpha_quads) == 1
        assert len(beta_quads) == 1

    def test_transitive_chains_stay_in_own_world(self) -> None:
        e = URIRef("http://example.org/e")
        f = URIRef("http://example.org/f")
        cg = _cg_with_quads(
            [
                (_A, _RELATED, _B, _W_ALPHA),
                (_B, _RELATED, _C, _W_ALPHA),
                (_D, _RELATED, e, _W_BETA),
                (e, _RELATED, f, _W_BETA),
            ]
        )
        result = materialize_program(_transitivity_program(), cg)
        alpha_quads = [q for q in result.quads if q.graph == str(_W_ALPHA)]
        beta_quads = [q for q in result.quads if q.graph == str(_W_BETA)]
        assert len(alpha_quads) == 3, f"Alpha: expected 3, got {len(alpha_quads)}"
        assert len(beta_quads) == 3, f"Beta: expected 3, got {len(beta_quads)}"
        alpha_derived = {(q.subject, q.obj) for q in alpha_quads}
        beta_derived = {(q.subject, q.obj) for q in beta_quads}
        assert (str(_A), _C.n3()) in alpha_derived
        assert (str(_A), _C.n3()) not in beta_derived
        assert (str(_D), f.n3()) in beta_derived
        assert (str(_D), f.n3()) not in alpha_derived


# --------------------------------------------------------------------------- #
# Tests: no-occurrence gate
# --------------------------------------------------------------------------- #


class TestNoOccurrenceGate:
    def test_no_event_tokens_clean(self) -> None:
        some_class = URIRef("http://example.org/SomeClass")
        cg = _cg_with_quads([(_A, RDF.type, some_class, _W_ALPHA)])
        result = materialize_program(_horn_program_no_rules(), cg)
        assert len(result.quads) == 1

    def test_event_token_raises(self) -> None:
        gufo_event = URIRef("http://purl.org/nemo/gufo#Event")
        cg = _cg_with_quads([(_A, RDF.type, gufo_event, _W_ALPHA)])
        with pytest.raises(NoOccurrenceViolationError) as exc_info:
            materialize_program(_horn_program_no_rules(), cg)
        err = exc_info.value
        assert err.world_iri == str(_W_ALPHA)
        assert err.token_iri == str(_A)
        assert "Event" in err.event_type

    def test_event_subclass_token_raises(self) -> None:
        gufo_event = URIRef("http://purl.org/nemo/gufo#Event")
        my_event = URIRef("http://example.org/MyEvent")
        cg = _cg_with_quads(
            [
                (my_event, RDFS.subClassOf, gufo_event, _W_ALPHA),
                (_A, RDF.type, my_event, _W_ALPHA),
            ]
        )
        with pytest.raises(NoOccurrenceViolationError) as exc_info:
            materialize_program(_horn_program_no_rules(), cg)
        err = exc_info.value
        assert err.world_iri == str(_W_ALPHA)
        assert err.event_type == str(my_event)

    def test_event_subclass_no_token_clean(self) -> None:
        gufo_event = URIRef("http://purl.org/nemo/gufo#Event")
        my_event = URIRef("http://example.org/MyEvent")
        cg = _cg_with_quads([(my_event, RDFS.subClassOf, gufo_event, _W_ALPHA)])
        result = materialize_program(_horn_program_no_rules(), cg)
        assert any(q.subject == str(my_event) for q in result.quads)

    def test_event_in_beta_raises_for_beta(self) -> None:
        gufo_event = URIRef("http://purl.org/nemo/gufo#Event")
        cg = _cg_with_quads(
            [
                (_A, _RELATED, _B, _W_ALPHA),
                (_C, RDF.type, gufo_event, _W_BETA),
            ]
        )
        with pytest.raises(NoOccurrenceViolationError) as exc_info:
            materialize_program(_horn_program_no_rules(), cg)
        err = exc_info.value
        assert err.world_iri == str(_W_BETA)


# --------------------------------------------------------------------------- #
# Tests: determinism
# --------------------------------------------------------------------------- #


class TestDeterminism:
    def test_same_input_same_reifier_iris(self) -> None:
        iri1 = quad_reifier_iri(_A, _RELATED, _B)
        iri2 = quad_reifier_iri(_A, _RELATED, _B)
        assert iri1 == iri2

    def test_derivation_order_independent(self) -> None:
        rule = "https://blackcatinformatics.ca/logic/rules/test"
        src1 = quad_reifier_iri(_A, _RELATED, _B)
        src2 = quad_reifier_iri(_B, _RELATED, _C)
        d_fwd = derivation_id_iri(rule, [src1, src2])
        d_rev = derivation_id_iri(rule, [src2, src1])
        assert d_fwd == d_rev

    def test_materialize_twice_same_result(self) -> None:
        cg1 = _cg_with_quads(
            [(_A, _RELATED, _B, _W_ALPHA), (_B, _RELATED, _C, _W_ALPHA)]
        )
        cg2 = _cg_with_quads(
            [(_A, _RELATED, _B, _W_ALPHA), (_B, _RELATED, _C, _W_ALPHA)]
        )
        r1 = materialize_program(_transitivity_program(), cg1)
        r2 = materialize_program(_transitivity_program(), cg2)
        assert r1.quads == r2.quads

    def test_derivation_iris_deterministic(self) -> None:
        cg1 = _cg_with_quads(
            [(_A, _RELATED, _B, _W_ALPHA), (_B, _RELATED, _C, _W_ALPHA)]
        )
        cg2 = _cg_with_quads(
            [(_A, _RELATED, _B, _W_ALPHA), (_B, _RELATED, _C, _W_ALPHA)]
        )
        r1 = materialize_program(_transitivity_program(), cg1)
        r2 = materialize_program(_transitivity_program(), cg2)
        ids1 = sorted(q.derivation_id for q in r1.quads)
        ids2 = sorted(q.derivation_id for q in r2.quads)
        assert ids1 == ids2

    def test_source_quad_ids_deterministic(self) -> None:
        cg1 = _cg_with_quads(
            [(_A, _RELATED, _B, _W_ALPHA), (_B, _RELATED, _C, _W_ALPHA)]
        )
        cg2 = _cg_with_quads(
            [(_A, _RELATED, _B, _W_ALPHA), (_B, _RELATED, _C, _W_ALPHA)]
        )
        r1 = materialize_program(_transitivity_program(), cg1)
        r2 = materialize_program(_transitivity_program(), cg2)
        d1 = [q for q in r1.quads if q.subject == str(_A) and q.obj == _C.n3()]
        d2 = [q for q in r2.quads if q.subject == str(_A) and q.obj == _C.n3()]
        assert len(d1) == 1 and len(d2) == 1
        assert sorted(d1[0].source_quad_ids) == sorted(d2[0].source_quad_ids)


# --------------------------------------------------------------------------- #
# Tests: golden roundtrip
# --------------------------------------------------------------------------- #


@pytest.mark.skipif(
    not _GOLDENS_PATH.exists(), reason="determinism-goldens.json not found"
)
class TestGoldenRoundtrip:
    @pytest.fixture(scope="class")
    def goldens(self) -> dict[str, object]:
        return dict(json.loads(_GOLDENS_PATH.read_text(encoding="utf-8")))

    def _get_quad_golden(
        self, goldens: dict[str, object], golden_id: str
    ) -> dict[str, object]:
        g_list = goldens["quad_reifier_goldens"]
        assert isinstance(g_list, list)
        result = next(
            (x for x in g_list if isinstance(x, dict) and x["_id"] == golden_id),
            None,
        )
        assert result is not None, f"Golden {golden_id!r} not found"
        assert isinstance(result, dict)
        return result

    def _get_deriv_golden(
        self, goldens: dict[str, object], golden_id: str
    ) -> dict[str, object]:
        d_list = goldens["derivation_id_goldens"]
        assert isinstance(d_list, list)
        result = next(
            (x for x in d_list if isinstance(x, dict) and x["_id"] == golden_id),
            None,
        )
        assert result is not None, f"Golden {golden_id!r} not found"
        assert isinstance(result, dict)
        return result

    def test_quad_reifier_golden_iri_triple(self, goldens: dict[str, object]) -> None:
        g = self._get_quad_golden(goldens, "golden-1-iri-triple")
        s = URIRef(str(g["subject"]))
        p = URIRef(str(g["predicate"]))
        o = URIRef(str(g["object"]))
        assert quad_reifier_iri(s, p, o) == g["reifier_iri"]

    def test_quad_reifier_golden_lang_literal(self, goldens: dict[str, object]) -> None:
        g = self._get_quad_golden(goldens, "golden-2-lang-literal")
        s = URIRef(str(g["subject"]))
        p = URIRef(str(g["predicate"]))
        o = Literal(str(g["object"]), lang=str(g["object_lang"]))
        assert quad_reifier_iri(s, p, o) == g["reifier_iri"]

    def test_quad_reifier_golden_xsd_decimal(self, goldens: dict[str, object]) -> None:
        g = self._get_quad_golden(goldens, "golden-3-xsd-decimal")
        s = URIRef(str(g["subject"]))
        p = URIRef(str(g["predicate"]))
        o = Literal(str(g["object"]), datatype=XSD.decimal)
        assert quad_reifier_iri(s, p, o) == g["reifier_iri"]

    def test_quad_reifier_golden_plain_string(self, goldens: dict[str, object]) -> None:
        g = self._get_quad_golden(goldens, "golden-4-plain-string")
        s = URIRef(str(g["subject"]))
        p = URIRef(str(g["predicate"]))
        # Plain Literal: rdflib sets xsd:string, .n3() elides it
        o = Literal(str(g["object"]))
        assert quad_reifier_iri(s, p, o) == g["reifier_iri"]

    def test_derivation_id_golden_two_sources(self, goldens: dict[str, object]) -> None:
        g = self._get_deriv_golden(goldens, "golden-5-two-sources")
        raw = g["source_reifier_iris"]
        assert isinstance(raw, list)
        src_iris: list[str] = [str(x) for x in raw]
        result_fwd = derivation_id_iri(str(g["rule_iri"]), src_iris)
        result_rev = derivation_id_iri(str(g["rule_iri"]), list(reversed(src_iris)))
        assert result_fwd == g["derivation_iri"]
        assert result_rev == g["derivation_iri"]

    def test_derivation_id_golden_assert_sentinel(
        self, goldens: dict[str, object]
    ) -> None:
        g = self._get_deriv_golden(goldens, "golden-6-assert-sentinel")
        raw = g["source_reifier_iris"]
        assert isinstance(raw, list)
        src_iris: list[str] = [str(x) for x in raw]
        result = derivation_id_iri(str(g["rule_iri"]), src_iris)
        assert result == g["derivation_iri"]

    def test_all_quad_reifier_goldens_n3_consistent(
        self, goldens: dict[str, object]
    ) -> None:
        g_list = goldens["quad_reifier_goldens"]
        assert isinstance(g_list, list)
        for entry in g_list:
            assert isinstance(entry, dict)
            if str(entry.get("_id", "")).startswith("_"):
                continue
            canonical = str(entry["n3_canonical"])
            digest = sha1(canonical.encode("utf-8")).hexdigest()
            expected_iri = f"{NAMESPACE}reifier/{digest}"
            assert expected_iri == entry["reifier_iri"], (
                f"{entry['_id']}: canonical {canonical!r} -> sha1 {digest} -> "
                f"expected {expected_iri!r} "
                f"but golden has {entry['reifier_iri']!r}"
            )


# --------------------------------------------------------------------------- #
# Tests: seam data contract on N-Quads input
# --------------------------------------------------------------------------- #


class TestNQuadsInput:
    _TWO_WORLD_NQ = (
        "<http://example.org/s1> <http://example.org/p/type> "
        "<http://example.org/o/Thing> <http://world/Alpha> .\n"
        "<http://example.org/s2> <http://example.org/p/name> "
        "<http://example.org/o/Foo> <http://world/Alpha> .\n"
        "<http://example.org/s3> <http://example.org/p/type> "
        "<http://example.org/o/Bar> <http://world/Beta> .\n"
    )

    def test_all_meta_fields_present(self) -> None:
        cg = parse_nquads(self._TWO_WORLD_NQ)
        result = materialize_program(_horn_program_no_rules(), cg)
        for quad in result.quads:
            _assert_seam_contract(quad)

    def test_worlds_isolated(self) -> None:
        cg = parse_nquads(self._TWO_WORLD_NQ)
        result = materialize_program(_horn_program_no_rules(), cg)
        alpha_quads = [q for q in result.quads if q.graph == "http://world/Alpha"]
        beta_quads = [q for q in result.quads if q.graph == "http://world/Beta"]
        assert len(alpha_quads) == 2
        assert len(beta_quads) == 1
        alpha_subjects = {q.subject for q in alpha_quads}
        beta_subjects = {q.subject for q in beta_quads}
        assert not (alpha_subjects & beta_subjects)

    def test_budget_status_always_ok(self) -> None:
        cg = parse_nquads(self._TWO_WORLD_NQ)
        result = materialize_program(_horn_program_no_rules(), cg)
        for q in result.quads:
            assert q.budget_status == "ok"

    def test_graph_equals_graph_component(self) -> None:
        cg = parse_nquads(self._TWO_WORLD_NQ)
        result = materialize_program(_horn_program_no_rules(), cg)
        for q in result.quads:
            assert q.graph == q.graph_component


# --------------------------------------------------------------------------- #
# Tests: profile IRI in result
# --------------------------------------------------------------------------- #


class TestProfileField:
    def test_positive_horn_profile_iri(self) -> None:
        logic_ns = PREFIXES["logic"]
        cg = _cg_with_quads([(_A, _RELATED, _B, _W_ALPHA)])
        result = materialize_program(_horn_program_no_rules(), cg)
        expected_profile = f"{logic_ns}PositiveHornProfile"
        for q in result.quads:
            assert q.profile == expected_profile
