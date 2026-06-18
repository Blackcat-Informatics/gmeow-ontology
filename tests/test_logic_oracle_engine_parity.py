# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Oracle ≡ Engine parity gate (issue #501, Task 9 — CAPSTONE).

Proves that the Python oracle and the Rust Nemo engine produce IDENTICAL
materialization on the #501 corpus (Principle 7): every derived quad set,
world assignment, and provenance skeleton must be equal.

The four tested cases (with ``input.nq`` + ``input.logic.ttl``) are:

* ``worlds-A/contested-standpoint``     — contested sovereignty, two worlds
* ``worlds-B/no-occurrence-gate``       — transitive causal chain, two worlds
* ``explanation/transitive-derivation`` — transitive subOf derivation
* ``paraconsistency/cross-world-isolation`` — cross-world isolation invariant

Test design
-----------
For each case:

1. Parse ``input.logic.ttl`` → :class:`~gmeow_tools.logic_ir.LogicProgram`.
2. Read ``input.nq`` for the EDB facts.
3. **Python oracle**: :func:`~gmeow_tools.logic_materialize.materialize_program`
   → :class:`~gmeow_tools.logic_materialize.MaterializationResult`.
4. **Rust engine**: extract the Nemo rules from
   :func:`~gmeow_tools.logic_projections.project_nemo`'s output (the
   ``% === Rules ===`` section only — schema axiom declarations are not
   materialization inputs), then call
   ``gmeow_logic.materialize(rules, input_nq_text)`` → list of dicts.
5. **Assert parity**:
   a. Same world set.
   b. Same quad set per world (blank-node-aware by rdflib isomorphism, or
      direct string comparison since Skolemization has already been applied).
   c. For each matched derived quad: ``rule_iri`` agrees on oracle vs engine.
   d. Explanation skeleton: the cited-IRI set for the derived quads is a
      subset of the proof-trace IRIs that the oracle computes.

Guard
-----
The native extension is required. Missing ``gmeow_logic`` is a test-environment
failure, not a skip. The Makefile and CI build it before running pytest.
"""

from __future__ import annotations

import io
from pathlib import Path
from typing import NamedTuple

import pytest
from rdflib import ConjunctiveGraph

from tests._required_native import require_gmeow_logic

gmeow_logic = require_gmeow_logic()

from gmeow_tools.logic_certify import certify_program  # noqa: E402
from gmeow_tools.logic_frontend import parse_logic_source  # noqa: E402
from gmeow_tools.logic_ir import SemanticProfileId  # noqa: E402
from gmeow_tools.logic_materialize import (  # noqa: E402
    BudgetParams,
    DerivedQuad,
    materialize_program,
)
from gmeow_tools.logic_projections import (  # noqa: E402
    extract_nemo_rules_section,
    project_nemo,
)

# --------------------------------------------------------------------------- #
# Corpus root + case discovery
# --------------------------------------------------------------------------- #

_CONFORMANCE_ROOT = Path(__file__).resolve().parents[1] / "conformance" / "logic"


def _discover_parity_cases() -> list[Path]:
    """Return case directories that have both ``input.logic.ttl`` and ``input.nq``."""
    cases_root = _CONFORMANCE_ROOT / "cases"
    found: list[Path] = []
    if not cases_root.is_dir():
        return found
    for category_dir in sorted(cases_root.iterdir()):
        if not category_dir.is_dir():
            continue
        for case_dir in sorted(category_dir.iterdir()):
            if not case_dir.is_dir():
                continue
            has_logic = (case_dir / "input.logic.ttl").exists()
            if has_logic and (case_dir / "input.nq").exists():
                found.append(case_dir)
    return found


_PARITY_CASES = _discover_parity_cases()
_PARITY_IDS = [f"{p.parent.name}/{p.name}" for p in _PARITY_CASES]

# Cases whose projected rule set is intentionally NON-stratified (recursion
# through negation-as-failure).  These are valid CERTIFY-parity cases — the
# certifier's whole job is to FLAG them — but they cannot be MATERIALIZED: Nemo's
# stratified engine refuses a non-stratified program (``NonStratifiedProgram``),
# and the Python v1 oracle has no well-founded / stable-model evaluator either.
# They are therefore skipped by the materialization parity test (which would have
# no defined answer to compare) while still exercised by the certify parity test.
_NON_MATERIALIZABLE_CASE_IDS: frozenset[str] = frozenset(
    [
        "decidability/non-stratified-flagged",
        "profiles/well-founded",
        "profiles/stable-model",
    ]
)

# Minimum expected case IDs.  The first four are the issue #501 AC-d world corpus;
# the remainder are the issue #502 Task 6 certifier/budget corpus whose rule sets
# are `.rls`-projectable and therefore subject to oracle ≡ engine parity.
_REQUIRED_CASE_IDS: frozenset[str] = frozenset(
    [
        # #501 world corpus
        "worlds-A/contested-standpoint",
        "worlds-B/no-occurrence-gate",
        "explanation/transitive-derivation",
        "paraconsistency/cross-world-isolation",
        # #502 decidability corpus
        "decidability/stratified-certifies",
        "decidability/non-stratified-flagged",
        "decidability/budget-exhaustion",
        # #502 profiles corpus
        "profiles/positive-horn",
        "profiles/stratified-naf",
        "profiles/well-founded",
        "profiles/stable-model",
    ]
)

# The semantic profile each case declares (its profile.json ``semantic_profile``),
# keyed by case id.  Used by the CERTIFY parity assertion to call both the Python
# oracle (``certify_program``) and the Rust engine (``gmeow_logic.certify``)
# against the SAME declared profile, and to diff the two verdicts field-for-field.
_CASE_PROFILE: dict[str, SemanticProfileId] = {
    "worlds-A/contested-standpoint": SemanticProfileId.POSITIVE_HORN,
    "worlds-B/no-occurrence-gate": SemanticProfileId.POSITIVE_HORN,
    "explanation/transitive-derivation": SemanticProfileId.POSITIVE_HORN,
    "paraconsistency/cross-world-isolation": SemanticProfileId.POSITIVE_HORN,
    "decidability/stratified-certifies": SemanticProfileId.STRATIFIED_NAF,
    "decidability/non-stratified-flagged": SemanticProfileId.STRATIFIED_NAF,
    "decidability/budget-exhaustion": SemanticProfileId.POSITIVE_HORN,
    "profiles/positive-horn": SemanticProfileId.POSITIVE_HORN,
    "profiles/stratified-naf": SemanticProfileId.STRATIFIED_NAF,
    "profiles/well-founded": SemanticProfileId.WELL_FOUNDED,
    "profiles/stable-model": SemanticProfileId.STABLE_MODEL,
}

# --------------------------------------------------------------------------- #
# Helpers: Nemo rules extraction
# --------------------------------------------------------------------------- #


# The canonical extractor lives in ``logic_projections`` (the runner and the
# ``logic-certify`` CLI use the same Rust-authoritative certification path);
# this alias keeps the call sites below readable.
_extract_rules_section = extract_nemo_rules_section


# --------------------------------------------------------------------------- #
# Helpers: term canonicalization for comparison
# --------------------------------------------------------------------------- #


class _QuadKey(NamedTuple):
    """Canonical comparison key for a materialized quad.

    ``subject``, ``predicate``, ``obj``, ``world`` are all plain IRI/N3 strings
    after canonicalization.  This lets us compare oracle quads (DerivedQuad) to
    engine quads (dict) without depending on object identity.
    """

    subject: str
    predicate: str
    obj: str
    world: str


def _oracle_quad_key(q: DerivedQuad) -> _QuadKey:
    """Derive a comparison key from an oracle :class:`~DerivedQuad`."""
    return _QuadKey(
        subject=q.subject,
        predicate=q.predicate,
        obj=q.obj,
        world=q.graph,
    )


def _engine_quad_key(d: dict[str, object]) -> _QuadKey:
    """Derive a comparison key from an engine result dict.

    The engine returns subject/predicate/object as wrapped strings
    (``<iri>`` for IRIs).  We strip the outer angle brackets to match the
    oracle's plain IRI strings for subject and predicate.  The object may be
    an IRI (``<iri>``) or a literal; we keep its display form as-is so it
    matches the oracle's N3-canonical ``obj`` field.
    """

    def _strip_brackets(s: str) -> str:
        if s.startswith("<") and s.endswith(">"):
            return s[1:-1]
        return s

    return _QuadKey(
        subject=_strip_brackets(str(d["subject"])),
        predicate=_strip_brackets(str(d["predicate"])),
        obj=str(d["object"]),
        world=str(d["graph"]),
    )


# --------------------------------------------------------------------------- #
# Main parity assertion helpers
# --------------------------------------------------------------------------- #


def _assert_world_sets_equal(
    case_id: str,
    oracle_worlds: frozenset[str],
    engine_worlds: frozenset[str],
) -> None:
    """Assert that oracle and engine agree on the set of worlds present."""
    missing_from_engine = oracle_worlds - engine_worlds
    extra_in_engine = engine_worlds - oracle_worlds
    errors: list[str] = []
    if missing_from_engine:
        errors.append(
            f"  worlds in oracle but NOT in engine: {sorted(missing_from_engine)}"
        )
    if extra_in_engine:
        errors.append(
            f"  worlds in engine but NOT in oracle: {sorted(extra_in_engine)}"
        )
    assert not errors, f"[{case_id}] world-set mismatch:\n" + "\n".join(errors)


def _assert_quad_sets_equal(
    case_id: str,
    oracle_keys: set[_QuadKey],
    engine_keys: set[_QuadKey],
) -> None:
    """Assert oracle and engine quad sets are identical."""
    missing = oracle_keys - engine_keys
    extra = engine_keys - oracle_keys

    errors: list[str] = []
    if missing:
        errors.append("  quads in oracle but NOT in engine (missing derivations):")
        for q in sorted(missing)[:10]:
            errors.append(f"    {q}")
    if extra:
        errors.append("  quads in engine but NOT in oracle (spurious derivations):")
        for q in sorted(extra)[:10]:
            errors.append(f"    {q}")
    assert not errors, (
        f"[{case_id}] quad-set mismatch (oracle ≠ engine):\n" + "\n".join(errors)
    )


def _assert_rule_iris_agree(
    case_id: str,
    oracle_quads: tuple[DerivedQuad, ...],
    engine_dicts: list[dict[str, object]],
) -> None:
    """For matched derived quads, assert rule_iri is identical.

    Only checks quads that are **derived** (oracle rule_iri ≠ ``logic:assert``).
    Asserted (EDB) quads carry ``logic:assert`` on both sides by definition.
    """
    assert_sentinel = "https://blackcatinformatics.ca/logic/assert"

    # Build engine index: quad_key → rule_iri
    engine_index: dict[_QuadKey, str] = {
        _engine_quad_key(d): str(d["rule_iri"]) for d in engine_dicts
    }

    errors: list[str] = []
    for oq in oracle_quads:
        if oq.rule_iri == assert_sentinel:
            continue  # skip EDB quads
        key = _oracle_quad_key(oq)
        eng_rule = engine_index.get(key)
        if eng_rule is None:
            # Quad missing from engine — already caught by _assert_quad_sets_equal.
            continue
        if oq.rule_iri != eng_rule:
            errors.append(
                f"  quad {key}:\n"
                f"    oracle  rule_iri = {oq.rule_iri!r}\n"
                f"    engine  rule_iri = {eng_rule!r}"
            )

    assert not errors, f"[{case_id}] rule_iri mismatch on derived quads:\n" + "\n".join(
        errors
    )


# --------------------------------------------------------------------------- #
# Parametrized parity test
# --------------------------------------------------------------------------- #


@pytest.mark.parametrize("case_dir", _PARITY_CASES, ids=_PARITY_IDS)
def test_oracle_engine_parity(case_dir: Path) -> None:
    """Assert oracle ≡ engine on a single corpus case (Principle 7).

    Steps:
    1. Parse ``input.logic.ttl`` → :class:`LogicProgram`.
    2. Python oracle: :func:`materialize_program` on ``input.nq`` EDB.
    3. Rust engine: ``gmeow_logic.materialize(rules, input_nq)`` where
       ``rules`` is the inference-only section from ``project_nemo``.
    4. Assert: same world set, same quad set, matching rule_iri on derived
       quads.
    """
    case_id = f"{case_dir.parent.name}/{case_dir.name}"
    if case_id in _NON_MATERIALIZABLE_CASE_IDS:
        pytest.skip(
            f"[{case_id}] non-stratified rule set: Nemo's stratified engine "
            f"refuses to materialize it and the v1 Python oracle has no "
            f"well-founded/stable-model evaluator — covered by the CERTIFY "
            f"parity test instead (it is flagged, never materialized)"
        )
    input_logic = case_dir / "input.logic.ttl"
    input_nq_path = case_dir / "input.nq"

    # ── 1. Parse logic program ─────────────────────────────────────────────────
    program, _diagnostics = parse_logic_source(input_logic)
    assert len(program.axioms) + len(program.rules) > 0, (
        f"[{case_id}] logic program is empty"
    )

    # ── 2. Python oracle ───────────────────────────────────────────────────────
    input_nq_text = input_nq_path.read_text(encoding="utf-8")
    input_cg: ConjunctiveGraph = ConjunctiveGraph()
    if input_nq_text.strip():
        input_cg.parse(io.StringIO(input_nq_text), format="nquads")

    oracle_result = materialize_program(
        program,
        input_cg,
        profile=SemanticProfileId.POSITIVE_HORN,
    )

    oracle_keys: set[_QuadKey] = {_oracle_quad_key(q) for q in oracle_result.quads}
    oracle_worlds: frozenset[str] = oracle_result.worlds

    # ── 3. Rust engine ─────────────────────────────────────────────────────────
    nemo_result = project_nemo(program)
    rules_only = _extract_rules_section(nemo_result.content)

    engine_dicts: list[dict[str, object]] = gmeow_logic.materialize(
        rules_only, input_nq_text
    )

    engine_keys: set[_QuadKey] = {_engine_quad_key(d) for d in engine_dicts}
    engine_worlds: frozenset[str] = frozenset(str(d["graph"]) for d in engine_dicts)

    # ── 4a. World-set parity ───────────────────────────────────────────────────
    _assert_world_sets_equal(case_id, oracle_worlds, engine_worlds)

    # ── 4b. Quad-set parity ────────────────────────────────────────────────────
    # On divergence, print both sets for debugging before asserting.
    if oracle_keys != engine_keys:
        print(f"\n[{case_id}] DIVERGENCE DETECTED — oracle vs engine quad sets:")
        print("  Oracle quads:")
        for q in sorted(oracle_keys):
            print(f"    {q}")
        print("  Engine quads:")
        for q in sorted(engine_keys):
            print(f"    {q}")

    _assert_quad_sets_equal(case_id, oracle_keys, engine_keys)

    # ── 4c. Rule IRI parity on derived quads ──────────────────────────────────
    _assert_rule_iris_agree(case_id, oracle_result.quads, engine_dicts)

    # ── 4d. Explanation skeleton: all cited IRIs reachable in oracle proof ─────
    # The explanation module builds the cited-IRI set from the derivation tree.
    # We verify that the rule IRIs the engine returns for derived quads are
    # ALL present in the oracle's explanation cited-IRI set.
    # (This is a subset check — the engine may not produce every IRI the oracle
    # cites; we only assert no hallucinated IRIs appear on the engine side.)
    # The native explanation engine (``gmeow_logic.explain``, issue #497) is
    # already bound at module scope via importorskip.
    engine_rule_iris: set[str] = {
        str(d["rule_iri"])
        for d in engine_dicts
        if str(d["rule_iri"]) != "https://blackcatinformatics.ca/logic/assert"
    }

    # Collect ALL cited IRIs from the oracle's explanation pool, via the native
    # engine over the oracle's materialized quads (one explanation per quad).
    explain_payload = [
        {
            "graph": oq.graph,
            "subject": oq.subject,
            "predicate": oq.predicate,
            "obj": oq.obj,
            "derivation_id": oq.derivation_id,
            "rule_iri": oq.rule_iri,
            "source_quad_ids": list(oq.source_quad_ids),
        }
        for oq in oracle_result.quads
    ]
    oracle_cited_all: set[str] = set()
    for exp_row in gmeow_logic.explain(explain_payload):
        oracle_cited_all.update(str(c) for c in exp_row["cited_iris"])

    for eng_rule_iri in engine_rule_iris:
        assert eng_rule_iri in oracle_cited_all, (
            f"[{case_id}] engine rule_iri {eng_rule_iri!r} is NOT present in the "
            f"oracle's explanation cited-IRI pool — possible rule IRI mismatch.\n"
            f"  Oracle cited IRIs (first 20): {sorted(oracle_cited_all)[:20]}"
        )


# --------------------------------------------------------------------------- #
# Certifier parity: Rust certify ≡ Python certify_program (issue #502)
# --------------------------------------------------------------------------- #


@pytest.mark.parametrize("case_dir", _PARITY_CASES, ids=_PARITY_IDS)
def test_certify_oracle_engine_parity(case_dir: Path) -> None:
    """Assert the Rust certifier ≡ the Python certifier on a corpus case.

    The static profile/decidability certifier is mirrored in Rust
    (``gmeow_logic.certify``) and Python (``certify_program``).  Per Principle 7
    the two MUST agree field-for-field on every certifiable case.  We feed the
    ``% === Rules ===`` section of ``project_nemo(program).rls`` to the Rust
    engine (the rule-extraction approach Task 4 documented — the ground-fact
    axioms are not certification inputs) and the parsed :class:`LogicProgram` to
    the Python oracle, both against the case's declared profile, then compare the
    two ``CertificationVerdict`` dicts exactly (``certified``,
    ``decidability_class``, ``profile_id``, and the sorted ``violations`` list).
    """
    case_id = f"{case_dir.parent.name}/{case_dir.name}"
    profile = _CASE_PROFILE.get(case_id)
    if profile is None:
        pytest.skip(f"[{case_id}] no declared profile in _CASE_PROFILE — not certified")

    program, _diagnostics = parse_logic_source(case_dir / "input.logic.ttl")

    # Python oracle verdict.
    oracle_verdict = certify_program(program, profile).to_json()

    # Rust engine verdict over the projected rule section only.
    rules_only = _extract_rules_section(project_nemo(program).content)
    engine_verdict = gmeow_logic.certify(rules_only, str(profile))

    assert engine_verdict == oracle_verdict, (
        f"[{case_id}] certifier divergence (Rust ≠ Python) for profile "
        f"{profile!s}:\n"
        f"  oracle (Python): {oracle_verdict}\n"
        f"  engine (Rust)  : {engine_verdict}\n"
        f"  projected rules:\n{rules_only}"
    )


# --------------------------------------------------------------------------- #
# Budget parity: Rust materialize ≡ Python materialize_program under a ceiling
# --------------------------------------------------------------------------- #


def test_budget_oracle_engine_parity() -> None:
    """Assert oracle ≡ engine under a budget ceiling on ``budget-exhaustion``.

    The ``decidability/budget-exhaustion`` case is a positive transitive-closure
    program whose full fixpoint exceeds the declared ``max_rule_firings`` ceiling.
    Both engines run the chase to fixpoint and then truncate the DERIVED quads to
    the canonical-sort prefix (asserted EDB facts are always kept), so the kept
    set is an evaluation-order-independent, sound partial — identical on both
    sides.  We assert:

    * the same ``budget_status`` (``"exhausted"``), and
    * the same truncated quad set (subject/predicate/object/world), proving the
      partial result is sound (a subset of the full closure) and engine-agnostic.
    """
    case_id = "decidability/budget-exhaustion"
    case_dir = _CONFORMANCE_ROOT / "cases" / "decidability" / "budget-exhaustion"
    assert case_dir.is_dir(), f"[{case_id}] case directory missing: {case_dir}"

    program, _diagnostics = parse_logic_source(case_dir / "input.logic.ttl")
    input_nq_text = (case_dir / "input.nq").read_text(encoding="utf-8")
    input_cg: ConjunctiveGraph = ConjunctiveGraph()
    if input_nq_text.strip():
        input_cg.parse(io.StringIO(input_nq_text), format="nquads")

    ceiling = 2

    # ── Python oracle under the budget ─────────────────────────────────────────
    oracle_result = materialize_program(
        program,
        input_cg,
        profile=SemanticProfileId.POSITIVE_HORN,
        budget=BudgetParams(max_rule_firings=ceiling),
    )
    oracle_status = oracle_result.budget_status
    oracle_keys: set[_QuadKey] = {_oracle_quad_key(q) for q in oracle_result.quads}

    # ── Rust engine under the budget ───────────────────────────────────────────
    rules_only = _extract_rules_section(project_nemo(program).content)
    engine_dicts: list[dict[str, object]] = gmeow_logic.materialize(
        rules_only, input_nq_text, max_rule_firings=ceiling
    )
    engine_status = str(engine_dicts[0]["budget_status"]) if engine_dicts else "ok"
    engine_keys: set[_QuadKey] = {_engine_quad_key(d) for d in engine_dicts}

    # ── Assertions ─────────────────────────────────────────────────────────────
    assert oracle_status == "exhausted", (
        f"[{case_id}] Python oracle did not report exhaustion under "
        f"max_rule_firings={ceiling}; got {oracle_status!r}"
    )
    assert oracle_result.incomplete is True, (
        f"[{case_id}] Python oracle did not mark the result incomplete"
    )
    assert engine_status == oracle_status, (
        f"[{case_id}] budget_status mismatch: oracle={oracle_status!r} "
        f"engine={engine_status!r}"
    )
    _assert_quad_sets_equal(case_id, oracle_keys, engine_keys)


# --------------------------------------------------------------------------- #
# Guard: corpus discovery must not be empty (Gap-11 fix)
# --------------------------------------------------------------------------- #


def test_parity_cases_discovered() -> None:
    """Assert that all required #501 AC-d corpus cases are discovered.

    This non-parametrized guard ensures that if ``_discover_parity_cases()``
    ever returns an empty list (e.g. due to a renamed directory or a broken
    conformance root), pytest's parametrize silently producing ZERO test
    instances is caught here rather than letting the AC-d gate pass vacuously.

    The four required case IDs correspond to the four world-indexed cases
    described in issue #501 and the module-level docstring.
    """
    discovered_ids: frozenset[str] = frozenset(_PARITY_IDS)
    missing: frozenset[str] = _REQUIRED_CASE_IDS - discovered_ids
    assert not missing, (
        f"Required parity cases not discovered — conformance corpus may be broken.\n"
        f"  Missing case IDs: {sorted(missing)}\n"
        f"  Discovered IDs:   {sorted(discovered_ids)}\n"
        f"  Corpus root: {_CONFORMANCE_ROOT / 'cases'}"
    )
