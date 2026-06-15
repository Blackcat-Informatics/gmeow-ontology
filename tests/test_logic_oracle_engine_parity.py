# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
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
The test is guarded by ``pytest.importorskip("gmeow_logic")`` — it skips
cleanly when the native extension is not installed locally (``make logic-py``
has not been run), but is fully required in CI where the ``python`` job
builds the extension before running pytest.  The marker filter in CI is
``-m "not docker and not pyoxigraph_ci"`` — this test carries no such
markers, so it always runs in CI once the extension is built.
"""

from __future__ import annotations

import io
from pathlib import Path
from typing import NamedTuple

import pytest
from rdflib import ConjunctiveGraph

gmeow_logic = pytest.importorskip(
    "gmeow_logic",
    reason="gmeow_logic native extension not installed — run 'make logic-py' first",
)

from gmeow_tools.logic_frontend import parse_logic_source  # noqa: E402
from gmeow_tools.logic_ir import SemanticProfileId  # noqa: E402
from gmeow_tools.logic_materialize import DerivedQuad, materialize_program  # noqa: E402
from gmeow_tools.logic_projections import project_nemo  # noqa: E402

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

# Minimum expected case IDs (issue #501 AC-d corpus).
_REQUIRED_CASE_IDS: frozenset[str] = frozenset(
    [
        "worlds-A/contested-standpoint",
        "worlds-B/no-occurrence-gate",
        "explanation/transitive-derivation",
        "paraconsistency/cross-world-isolation",
    ]
)

# --------------------------------------------------------------------------- #
# Helpers: Nemo rules extraction
# --------------------------------------------------------------------------- #


def _extract_rules_section(nemo_content: str) -> str:
    """Return only the ``% === Rules ===`` section from a ``.rls`` string.

    The ``project_nemo`` output includes two sections:

    * ``% === Ground facts (axioms) ===`` — schema-level metadata axioms
      (logic:type declarations, logic:head / logic:body reification nodes).
      These are NOT materialization inputs; they describe the rule structure
      in RDF terms, and including them in the engine call produces spurious
      output predicates (logic:body, logic:head etc.) with blank-node-like
      IRI objects that the Rust decoder cannot round-trip.

    * ``% === Rules ===`` — the actual Nemo inference rules.  These are the
      only input the engine needs to reproduce the oracle's chase.

    Returns everything from the rules-section header to the end of string.
    If the header is absent (no rules in the program), returns an empty string.
    """
    marker = "% === Rules ==="
    idx = nemo_content.find(marker)
    if idx == -1:
        return ""
    # Return the rule text after the section header
    return nemo_content[idx + len(marker) :].strip()


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
    from gmeow_tools.logic_explain import explain  # local import to avoid CI overhead

    engine_rule_iris: set[str] = {
        str(d["rule_iri"])
        for d in engine_dicts
        if str(d["rule_iri"]) != "https://blackcatinformatics.ca/logic/assert"
    }

    # Collect ALL cited IRIs from the oracle's explanation pool.
    oracle_cited_all: set[str] = set()
    for oq in oracle_result.quads:
        exp = explain(oracle_result, oq, onto_graph=None)
        oracle_cited_all.update(exp.cited_iris)

    for eng_rule_iri in engine_rule_iris:
        assert eng_rule_iri in oracle_cited_all, (
            f"[{case_id}] engine rule_iri {eng_rule_iri!r} is NOT present in the "
            f"oracle's explanation cited-IRI pool — possible rule IRI mismatch.\n"
            f"  Oracle cited IRIs (first 20): {sorted(oracle_cited_all)[:20]}"
        )


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
