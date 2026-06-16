# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Conformance runner for the Logic v1 monotonic core (issue #501, Task 7).

This module is the **Python oracle runner** — it wires :mod:`~.logic_frontend`,
:mod:`~.logic_materialize`, :mod:`~.logic_projections`, and :mod:`~.logic_explain`
into the single ``run()`` function required by the runner contract in
``conformance/logic/runner/README.md``.

Runner contract
---------------
.. code-block:: text

    run(case_dir, mode) -> RunnerOutputs(
        materialized,   # forward-chase N-Quads (world = named graph)
        projections,    # all projection back-ends + preservation ledger
        explanations,   # per-derived-quad explanation skeletons (cited-IRI sets)
        verdicts,       # POPULATED: world-indexed boolean truth verdicts (JSON)
        witnesses,      # POPULATED: empty dict (no within-world contradictions
                        #            in v1 monotonic Horn; deferred to #503/#504)
        answers,        # DEFERRED: goal/counterfactual answers (#504/#505)
        certification,  # POPULATED (#502): static profile/decidability verdict
        budget_status,  # POPULATED (#502): aggregate budget-governor status
        incomplete,     # POPULATED (#502): True iff a budget ceiling was tripped
    )

Populated vs deferred (v1 monotonic scope)
------------------------------------------
* ``materialized`` — fully populated: the forward Horn chase result serialized
  as N-Quads, one named graph per world.
* ``projections`` — fully populated: all 7 projection back-ends (OWL-DL, OWL-EL,
  Datalog, N3, gUFO, Canonical-RDF12, Nemo) plus the preservation ledger RDF graph
  and JSON ledger.
* ``explanations`` — populated for every derived quad that has a non-trivial
  derivation; each entry carries the cited-IRI/rule-IRI skeleton as
  ``frozenset[str]``.
* ``verdicts`` — populated as world-indexed ``{world_iri: {"quads": count,
  "status": "consistent"}}`` — the monotonic v1 oracle has no negation so
  every world that materializes without error is ``consistent``.  Full
  modality/truth verdicts (``conceivable``, ``refuted``, etc.) belong to #503.
* ``witnesses`` — always ``{}`` in the v1 monotonic core; within-world
  contradictions cannot arise (no negation, no inconsistency).  Deferred to #503.
* ``answers`` — always ``{}``; goal/counterfactual resolution deferred to
  #504/#505.

Comparison helpers (used by Task 8/9 diff logic)
-------------------------------------------------
* :func:`compare_rdf` — blank-node-aware graph isomorphism (reuses rdflib).
* :func:`compare_canonical_json` — sorted-keys canonical JSON equality.
* :func:`compare_explanation_skeleton` — cited-IRI/rule-IRI set equality,
  ignoring surface prose.

Case discovery
--------------
:func:`discover_cases` finds every directory under ``conformance/logic/cases/``
that contains both ``input.logic.ttl`` and ``profile.json``.  Missing or
malformed files are hard failures (no silent skip).
"""

from __future__ import annotations

import json
import logging
from dataclasses import dataclass, field
from pathlib import Path

from rdflib import ConjunctiveGraph, Graph, URIRef
from rdflib.compare import isomorphic

from gmeow_tools.logic_certify import certify_program
from gmeow_tools.logic_explain import Explanation, explain
from gmeow_tools.logic_frontend import LogicParseError, parse_logic_source
from gmeow_tools.logic_ir import LogicProgram, SemanticProfileId
from gmeow_tools.logic_materialize import (
    BudgetParams,
    MaterializationError,
    MaterializationResult,
    materialize_program,
)
from gmeow_tools.logic_projections import (
    ProjectionResult,
    build_projection_report,
    project_canonical_rdf12,
    project_datalog,
    project_gufo,
    project_n3,
    project_nemo,
    project_owl_dl,
    project_owl_el,
)

_log = logging.getLogger(__name__)


# --------------------------------------------------------------------------- #
# Exceptions
# --------------------------------------------------------------------------- #


class RunnerError(Exception):
    """Raised when a conformance case is malformed or the run fails.

    Hard-fail: the runner never silently skips a malformed case.
    """


# --------------------------------------------------------------------------- #
# Output types
# --------------------------------------------------------------------------- #


@dataclass(frozen=True, slots=True)
class ProjectionOutputs:
    """All projection back-end results for one case run.

    Attributes:
        results: The 7 :class:`~.logic_projections.ProjectionResult` objects
            (one per back-end).
        report_graph: The preservation report as an rdflib
            :class:`~rdflib.Graph`.
        ledger_json: The preservation ledger as a canonical JSON dict.
            Keys are target names; values are ``{preservation, complexity,
            lossy_drops}`` dicts.
    """

    results: tuple[ProjectionResult, ...]
    report_graph: Graph
    ledger_json: dict[str, dict[str, object]]


@dataclass(frozen=True, slots=True)
class RunnerOutputs:
    """Complete runner output for one conformance case.

    Attributes:
        case_dir: The case directory that was run.
        mode: The engine/fragment mode requested (e.g. ``"native"``).
        program: The parsed :class:`~.logic_ir.LogicProgram`.

        materialized: The chase result.  **Populated** for v1 monotonic core.
        materialized_nquads: N-Quads serialization of ``materialized`` (all
            worlds as named graphs, sorted for determinism).  **Populated**.

        projections: All projection back-ends + ledger.  **Populated**.

        explanations: Per-derived-quad explanation skeletons.  **Populated**
            (one :class:`~.logic_explain.Explanation` per derived quad, ordered
            by world then quad position).

        verdicts: World-indexed truth verdicts JSON dict.  **Populated** as a
            minimal ``{world_iri: {"quads": n, "status": "consistent"}}``
            mapping.  Full modality/truth verdicts deferred to #503.

        witnesses: Contradiction witnesses.  **Always ``{}``** in v1 monotonic
            core (no negation → no within-world contradiction).  Deferred to
            #503.

        answers: Goal/counterfactual answer sets.  **Always ``{}``** — deferred
            to #504/#505.

        certification: The static profile/decidability verdict for ``program``
            against the case's declared profile, as the deterministic sorted-key
            dict produced by
            :meth:`~.logic_certify.CertificationVerdict.to_json`.  **Populated**
            for every case (issue #502, Task 5).

        budget_status: The aggregate budget-governor status copied from
            :attr:`~.logic_materialize.MaterializationResult.budget_status` —
            ``"ok"`` when the chase reached fixpoint within budget (or ran
            unbounded), ``"exhausted"`` when a declared ceiling was tripped.
            **Populated** for every case.

        incomplete: ``True`` iff the materialization was a sound partial because
            a budget ceiling was tripped (i.e. ``budget_status == "exhausted"``).
            Mirrors :attr:`~.logic_materialize.MaterializationResult.incomplete`.
    """

    case_dir: Path
    mode: str
    program: LogicProgram

    # v1 populated outputs
    materialized: MaterializationResult
    materialized_nquads: str
    projections: ProjectionOutputs
    explanations: tuple[Explanation, ...]

    # v1 minimal/stub outputs (documented deferred)
    verdicts: dict[str, dict[str, object]]
    witnesses: dict[str, object]  # always {}
    answers: dict[str, object]  # always {}

    # issue #502 (Task 5): certifier + budget governor surfaced as artifacts
    certification: dict[str, object]
    budget_status: str
    incomplete: bool


# --------------------------------------------------------------------------- #
# Case discovery
# --------------------------------------------------------------------------- #


@dataclass(frozen=True, slots=True)
class ConformanceCase:
    """Metadata for a discovered conformance case directory.

    Attributes:
        case_dir: Absolute path to the case directory.
        case_id: Human-readable identifier (relative path under ``cases/``).
        profile: Parsed contents of ``profile.json``.
    """

    case_dir: Path
    case_id: str
    profile: dict[str, object]


def discover_cases(conformance_root: Path) -> list[ConformanceCase]:
    """Discover all conformance cases under ``conformance_root/cases/``.

    A case directory is any subdirectory (recursively, up to 2 levels) that
    contains both ``input.logic.ttl`` and ``profile.json``.  Missing or
    unreadable files are hard failures.

    Args:
        conformance_root: The ``conformance/logic/`` directory.

    Returns:
        A sorted list of :class:`ConformanceCase` instances.

    Raises:
        RunnerError: If ``conformance_root/cases/`` does not exist.
    """
    cases_root = conformance_root / "cases"
    if not cases_root.is_dir():
        raise RunnerError(
            f"Conformance cases directory does not exist: {cases_root}. "
            "Expected conformance/logic/cases/ to be present."
        )

    found: list[ConformanceCase] = []
    # Walk up to 2 levels: cases/<category>/<case>/
    for category_dir in sorted(cases_root.iterdir()):
        if not category_dir.is_dir():
            continue
        for case_dir in sorted(category_dir.iterdir()):
            if not case_dir.is_dir():
                continue
            input_path = case_dir / "input.logic.ttl"
            profile_path = case_dir / "profile.json"
            if not input_path.exists() or not profile_path.exists():
                continue
            # Hard-fail on malformed profile.json
            try:
                profile_raw = profile_path.read_text(encoding="utf-8")
                profile_data = json.loads(profile_raw)
            except (OSError, json.JSONDecodeError) as exc:
                raise RunnerError(
                    f"Case {case_dir.name}: cannot read profile.json: {exc}"
                ) from exc
            if not isinstance(profile_data, dict):
                raise RunnerError(
                    f"Case {case_dir.name}: profile.json must be a JSON object, "
                    f"got {type(profile_data).__name__}"
                )
            case_id = f"{category_dir.name}/{case_dir.name}"
            found.append(
                ConformanceCase(
                    case_dir=case_dir,
                    case_id=case_id,
                    profile=profile_data,
                )
            )

    return found


# --------------------------------------------------------------------------- #
# N-Quads serialization
# --------------------------------------------------------------------------- #


def _materialize_to_nquads(result: MaterializationResult) -> str:
    """Serialize a :class:`~.logic_materialize.MaterializationResult` to N-Quads.

    Produces a deterministic N-Quads string with one line per quad, sorted
    lexicographically by (graph, subject, predicate, object) — the canonical
    order used by :mod:`~.logic_materialize`.

    Args:
        result: The materialization result from the forward chase.

    Returns:
        A canonical, sorted N-Quads string.  Empty if no worlds/quads.
    """
    lines: list[str] = []
    for quad in result.quads:
        # N-Quads line: <s> <p> <o_n3> <g> .
        # object is already in N3 form (IRI or literal)
        obj_nq = quad.obj
        lines.append(f"<{quad.subject}> <{quad.predicate}> {obj_nq} <{quad.graph}> .")
    return "\n".join(sorted(lines)) + ("\n" if lines else "")


# --------------------------------------------------------------------------- #
# Projection outputs builder
# --------------------------------------------------------------------------- #


def _run_projections(program: LogicProgram) -> ProjectionOutputs:
    """Run all 7 projection back-ends and build the preservation ledger.

    Args:
        program: The compiled logic program to project.

    Returns:
        A :class:`ProjectionOutputs` with all 7 results, the report graph,
        and the JSON ledger.

    Raises:
        RunnerError: If any projection raises unexpectedly.
    """
    try:
        r_dl = project_owl_dl(program)
        r_el = project_owl_el(program)
        r_datalog = project_datalog(program)
        r_n3 = project_n3(program)
        r_gufo = project_gufo(program)
        r_rdf12 = project_canonical_rdf12(program)
        r_nemo = project_nemo(program)
    except Exception as exc:
        raise RunnerError(f"Projection failed: {exc}") from exc

    all_projections = [r_dl, r_el, r_datalog, r_n3, r_gufo, r_rdf12, r_nemo]

    try:
        report_graph = build_projection_report(program, all_projections)
    except Exception as exc:
        raise RunnerError(f"build_projection_report failed: {exc}") from exc

    ledger_json: dict[str, dict[str, object]] = {
        proj.target: {
            "preservation": proj.preservation.value,
            "complexity": proj.complexity,
            "lossy_drops": list(proj.lossy_drops),
        }
        for proj in all_projections
    }

    return ProjectionOutputs(
        results=tuple(all_projections),
        report_graph=report_graph,
        ledger_json=ledger_json,
    )


# --------------------------------------------------------------------------- #
# Explanation builder
# --------------------------------------------------------------------------- #


def _run_explanations(result: MaterializationResult) -> tuple[Explanation, ...]:
    """Produce explanation skeletons for every derived quad in ``result``.

    Asserted quads (rule_iri == ``logic:assert``) are included — their
    explanation is trivial (depth-0 step, the fact itself).  The list is
    sorted by (graph, subject, predicate, object) for determinism.

    Args:
        result: The materialization result.

    Returns:
        A tuple of :class:`~.logic_explain.Explanation` objects, one per quad.

    Raises:
        RunnerError: If an explanation cannot be built for any quad.
    """
    from gmeow_tools.logic_explain import ExplainError

    explanations: list[Explanation] = []
    for quad in result.quads:
        try:
            exp = explain(result, quad, onto_graph=None)
            explanations.append(exp)
        except ExplainError as exc:
            raise RunnerError(
                f"explain() failed for quad "
                f"({quad.subject!r}, {quad.predicate!r}, {quad.obj!r}) "
                f"in world {quad.graph!r}: {exc}"
            ) from exc

    return tuple(explanations)


# --------------------------------------------------------------------------- #
# Verdicts builder (v1 minimal)
# --------------------------------------------------------------------------- #


def _build_verdicts(result: MaterializationResult) -> dict[str, dict[str, object]]:
    """Build the minimal v1 world-indexed truth verdicts.

    In the v1 monotonic Horn oracle there is no negation and therefore no
    within-world contradiction.  Every world that materializes without error is
    ``consistent``.  Full modality/truth verdicts (``conceivable``, ``refuted``,
    ``necessary``, etc.) require the paraconsistency and standpoint layers from
    #503 and are deferred.

    Args:
        result: The materialization result.

    Returns:
        A JSON-serializable dict mapping world IRI → ``{"quads": n, "status":
        "consistent"}``.
    """
    world_quad_counts: dict[str, int] = {}
    for quad in result.quads:
        world_quad_counts[quad.graph] = world_quad_counts.get(quad.graph, 0) + 1

    return {
        world_iri: {
            "quads": world_quad_counts.get(world_iri, 0),
            "status": "consistent",
        }
        for world_iri in sorted(result.worlds)
    }


# --------------------------------------------------------------------------- #
# Budget governor params (issue #502)
# --------------------------------------------------------------------------- #


def _parse_budget_params(
    case_dir: Path, profile_data: dict[str, object]
) -> BudgetParams | None:
    """Read the optional ``budget_params`` object from a case's ``profile.json``.

    The object is optional and every field inside it is optional.  Recognised
    keys are ``time_ms``, ``max_rule_firings`` and ``max_answers`` (all positive
    integers, matching :class:`~.logic_materialize.BudgetParams`).  When the key
    is absent the chase runs unbounded (``None``) — the #501 default that keeps
    the existing corpus byte-identical.

    Hard-fail (no silent coercion) on a malformed value: a non-object
    ``budget_params``, an unknown key, or a non-integer / non-positive ceiling is
    a :class:`RunnerError`, never a degraded default.

    Args:
        case_dir: The case directory (used only for error messages).
        profile_data: The parsed ``profile.json`` dict.

    Returns:
        A :class:`~.logic_materialize.BudgetParams` when ``budget_params`` is
        present, else ``None``.
    """
    raw = profile_data.get("budget_params")
    if raw is None:
        return None
    if not isinstance(raw, dict):
        raise RunnerError(
            f"Case {case_dir.name}: profile.json budget_params must be a JSON "
            f"object, got {type(raw).__name__}"
        )

    allowed = {"time_ms", "max_rule_firings", "max_answers"}
    unknown = sorted(set(raw) - allowed)
    if unknown:
        raise RunnerError(
            f"Case {case_dir.name}: profile.json budget_params has unknown "
            f"key(s) {unknown}; allowed keys are {sorted(allowed)}"
        )

    def _ceiling(key: str) -> int | None:
        if key not in raw:
            return None
        value = raw[key]
        # bool is an int subclass; reject it explicitly so `true`/`false`
        # cannot masquerade as a 1/0 ceiling.
        if isinstance(value, bool) or not isinstance(value, int):
            raise RunnerError(
                f"Case {case_dir.name}: profile.json budget_params.{key} must be "
                f"a positive integer, got {value!r}"
            )
        if value <= 0:
            raise RunnerError(
                f"Case {case_dir.name}: profile.json budget_params.{key} must be "
                f"a positive integer, got {value}"
            )
        return value

    return BudgetParams(
        time_ms=_ceiling("time_ms"),
        max_rule_firings=_ceiling("max_rule_firings"),
        max_answers=_ceiling("max_answers"),
    )


# --------------------------------------------------------------------------- #
# Public API: run()
# --------------------------------------------------------------------------- #


def run(case_dir: Path, mode: str = "native") -> RunnerOutputs:
    """Execute the runner contract for one conformance case.

    Parses ``case_dir/input.logic.ttl``, materializes the forward Horn chase,
    runs all projection back-ends, and produces explanation skeletons for every
    derived quad.

    Args:
        case_dir: Path to the conformance case directory.  Must contain
            ``input.logic.ttl`` and ``profile.json``.  If ``input.nq`` is
            also present it is loaded as the world-fact ConjunctiveGraph
            (one named graph per world) before the chase runs.
        mode: Engine/fragment mode selector.  Only ``"native"`` is supported
            in the v1 monotonic oracle; passing any other value raises
            :class:`RunnerError`.

    Returns:
        A :class:`RunnerOutputs` with all populated and stub outputs.

    Raises:
        RunnerError: If the input is malformed, the profile is unsupported,
            the materialization fails, or any projection fails.
        FileNotFoundError: If ``input.logic.ttl`` or ``profile.json`` do not
            exist.
    """
    if mode != "native":
        raise RunnerError(
            f"Unsupported mode {mode!r}: the v1 runner only implements 'native'. "
            "Other modes (owl-dl, owl-el, datalog) are deferred to later rungs."
        )

    input_path = case_dir / "input.logic.ttl"
    profile_path = case_dir / "profile.json"

    if not input_path.exists():
        raise RunnerError(
            f"Case {case_dir.name}: input.logic.ttl not found at {input_path}"
        )
    if not profile_path.exists():
        raise RunnerError(
            f"Case {case_dir.name}: profile.json not found at {profile_path}"
        )

    # Parse the profile
    try:
        profile_data = json.loads(profile_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise RunnerError(
            f"Case {case_dir.name}: cannot read profile.json: {exc}"
        ) from exc

    # Parse the logic: source
    try:
        program, diagnostics = parse_logic_source(input_path)
    except LogicParseError as exc:
        raise RunnerError(
            f"Case {case_dir.name}: parse_logic_source failed: {exc}"
        ) from exc

    for diag in diagnostics:
        _log.debug(
            "parse diagnostic [%s] %s: %s", diag.severity, diag.code, diag.message
        )

    # Resolve the semantic profile to use for materialization
    semantic_profile_str = str(
        profile_data.get("semantic_profile", "PositiveHornProfile")
    )
    # Resolve the declared profile to the typed enum for the certifier.  An
    # unknown localname is a hard failure (no silent fallback): the case author
    # must declare a real SemanticProfileId.
    try:
        declared_profile = SemanticProfileId(semantic_profile_str)
    except ValueError as exc:
        raise RunnerError(
            f"Case {case_dir.name}: unknown semantic_profile "
            f"{semantic_profile_str!r} in profile.json — must be one of "
            f"{[str(p) for p in SemanticProfileId]}"
        ) from exc

    # The v1 oracle only handles PositiveHornProfile
    if semantic_profile_str != "PositiveHornProfile":
        _log.warning(
            "Case %s: semantic_profile %r is not PositiveHornProfile; "
            "v1 oracle will materialize with PositiveHorn semantics (loss recorded).",
            case_dir.name,
            semantic_profile_str,
        )

    # Static certification against the DECLARED profile (issue #502, Task 5).
    # This is pure analysis over the IR and never raises; a non-empty
    # ``violations`` list is surfaced (not raised) so the conformance diff can
    # compare it as a golden artifact.
    certification = certify_program(program, declared_profile).to_json()

    # Build the input ConjunctiveGraph for the materializer.
    # Projection-only cases carry no named-graph worlds (flat Turtle program);
    # world-indexed cases (worlds-A, worlds-B, paraconsistency, explanation)
    # supply facts as ``input.nq`` (N-Quads, one named graph per world).
    # If ``input.nq`` is absent the chase runs over an empty graph (zero
    # asserted quads → zero derived quads; projections still work unchanged).
    input_graph: ConjunctiveGraph = ConjunctiveGraph()
    input_nq_path = case_dir / "input.nq"
    if input_nq_path.exists():
        try:
            nq_text = input_nq_path.read_text(encoding="utf-8")
            if nq_text.strip():
                import io as _io

                input_graph.parse(_io.StringIO(nq_text), format="nquads")
        except Exception as exc:
            raise RunnerError(
                f"Case {case_dir.name}: cannot parse input.nq: {exc}"
            ) from exc

    # Optional budget governor (issue #502, Task 5).  Absent ``budget_params``
    # means unbounded (the #501 default): the chase runs to full fixpoint and
    # the result is byte-identical to pre-#502 behaviour.
    budget = _parse_budget_params(case_dir, profile_data)

    # Materialize
    try:
        mat_result = materialize_program(
            program,
            input_graph,
            profile=SemanticProfileId.POSITIVE_HORN,
            budget=budget,
        )
    except MaterializationError as exc:
        raise RunnerError(
            f"Case {case_dir.name}: materialize_program failed: {exc}"
        ) from exc

    # N-Quads serialization
    nquads_str = _materialize_to_nquads(mat_result)

    # Projections
    proj_outputs = _run_projections(program)

    # Explanations (over whatever quads exist; empty for projection-only cases)
    explanations = _run_explanations(mat_result)

    # Verdicts (v1 minimal)
    verdicts = _build_verdicts(mat_result)

    return RunnerOutputs(
        case_dir=case_dir,
        mode=mode,
        program=program,
        materialized=mat_result,
        materialized_nquads=nquads_str,
        projections=proj_outputs,
        explanations=explanations,
        verdicts=verdicts,
        witnesses={},  # deferred to #503
        answers={},  # deferred to #504/#505
        certification=certification,
        budget_status=mat_result.budget_status,
        incomplete=mat_result.incomplete,
    )


# --------------------------------------------------------------------------- #
# Comparison helpers (used by Task 8/9 diff logic)
# --------------------------------------------------------------------------- #


def compare_rdf(
    actual_graph: Graph,
    expected_graph: Graph,
) -> list[str]:
    """Compare two rdflib Graphs by blank-node-aware graph isomorphism.

    Reuses rdflib's :func:`~rdflib.compare.isomorphic` (same function used by
    :func:`~.generator.rdf_compare`).  Does not compare serialization bytes —
    only graph structure and named nodes.

    Args:
        actual_graph: The freshly produced graph.
        expected_graph: The committed golden graph.

    Returns:
        An empty list on match, or a list of one error string describing the
        mismatch.
    """
    if isomorphic(actual_graph, expected_graph):
        return []
    # Produce a minimal diff: count triples in each and list unique triples
    actual_only = set(actual_graph) - set(expected_graph)
    expected_only = set(expected_graph) - set(actual_graph)
    lines = [
        f"RDF graph mismatch: {len(actual_graph)} vs {len(expected_graph)} triples"
    ]
    for triple in sorted(str(t) for t in actual_only)[:5]:
        lines.append(f"  actual only: {triple}")
    for triple in sorted(str(t) for t in expected_only)[:5]:
        lines.append(f"  expected only: {triple}")
    return lines


def compare_canonical_json(
    actual: dict[str, object],
    expected: dict[str, object],
) -> list[str]:
    """Compare two JSON dicts by canonical form: sorted keys, normalized literals.

    Serializes both dicts with sorted keys and no whitespace variation, then
    compares the canonical strings.  This is the runner's canonical-JSON
    comparison rule for ``verdicts.json`` and ``answers/*.json``.

    Args:
        actual: The freshly produced JSON dict.
        expected: The committed golden JSON dict.

    Returns:
        An empty list on match, or a list of one error string describing the
        mismatch.
    """
    actual_canon = json.dumps(actual, sort_keys=True, ensure_ascii=False)
    expected_canon = json.dumps(expected, sort_keys=True, ensure_ascii=False)
    if actual_canon == expected_canon:
        return []
    return [
        f"Canonical JSON mismatch:\n"
        f"  actual:   {actual_canon[:200]}\n"
        f"  expected: {expected_canon[:200]}"
    ]


def compare_explanation_skeleton(
    actual_cited_iris: frozenset[str],
    expected_cited_iris: frozenset[str],
) -> list[str]:
    """Compare two explanation skeletons by their cited-IRI/rule-IRI sets.

    The conformance runner compares ``explanation/<q>.md`` on the cited-IRI
    skeleton, NEVER on surface prose.  Two skeletons are equal iff their
    ``cited_iris`` frozensets are identical.

    Args:
        actual_cited_iris: The ``Explanation.cited_iris`` from the fresh run.
        expected_cited_iris: The committed golden cited-IRI set.

    Returns:
        An empty list on match, or a list describing the missing/extra IRIs.
    """
    if actual_cited_iris == expected_cited_iris:
        return []
    missing = sorted(expected_cited_iris - actual_cited_iris)
    extra = sorted(actual_cited_iris - expected_cited_iris)
    lines: list[str] = ["Explanation skeleton mismatch (cited-IRI sets differ):"]
    for iri in missing[:10]:
        lines.append(f"  missing (expected but not produced): <{iri}>")
    for iri in extra[:10]:
        lines.append(f"  extra   (produced but not expected): <{iri}>")
    return lines


# --------------------------------------------------------------------------- #
# Case-level diff: actual vs committed expected/
# --------------------------------------------------------------------------- #


@dataclass
class CaseDiffResult:
    """The result of diffing a runner output against its committed ``expected/`` files.

    Attributes:
        case_id: Human-readable case identifier.
        passed: True when all diffs are empty.
        diffs: Flat list of diff error strings (empty on pass).
    """

    case_id: str
    passed: bool
    diffs: list[str] = field(default_factory=list)


def _read_case_profile(case_dir: Path) -> dict[str, object]:
    """Re-read a case's ``profile.json`` for the diff phase.

    :func:`diff_case` receives only :class:`RunnerOutputs`, which deliberately
    does not carry the raw profile dict; the opt-in flags driving the
    certification/budget missing-golden rules live in ``profile.json``, so the
    diff re-reads it here.  The file was already validated as a JSON object by
    :func:`run` / :func:`discover_cases`, so a parse error at this point is a
    bug and surfaces as an empty profile (no opt-in) rather than a crash.

    Args:
        case_dir: The case directory.

    Returns:
        The parsed ``profile.json`` dict, or an empty dict if it is absent or
        unreadable (treated as "no opt-in").
    """
    profile_path = case_dir / "profile.json"
    if not profile_path.exists():
        return {}
    try:
        data = json.loads(profile_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}
    return data if isinstance(data, dict) else {}


def _parse_cited_iri_skeleton(text: str) -> frozenset[str]:
    """Parse the cited-iri-skeleton block from an explanation markdown file.

    Reads every non-empty line between the ``<!-- cited-iri-skeleton`` opening
    comment and its closing ``-->`` marker.  Lines are stripped of leading and
    trailing whitespace before collection.

    Args:
        text: The full text of an explanation ``.md`` golden file.

    Returns:
        A frozenset of IRI strings extracted from the skeleton block.
    """
    lines = text.splitlines()
    in_block = False
    iris: list[str] = []
    for line in lines:
        if line.strip() == "<!-- cited-iri-skeleton":
            in_block = True
            continue
        if in_block:
            if line.strip() == "-->":
                break
            iri = line.strip()
            if iri:
                iris.append(iri)
    return frozenset(iris)


def _parse_explanation_reifier(text: str) -> str:
    """Parse the target_quad_reifier from the prose header of an explanation file.

    Looks for the line: ``# Explanation for `<REIFIER>```

    Args:
        text: The full text of an explanation ``.md`` golden file.

    Returns:
        The reifier IRI string, or empty string if the header is not found.
    """
    prefix = "# Explanation for `<"
    suffix = ">`"
    for line in text.splitlines():
        if line.startswith(prefix) and line.endswith(suffix):
            return line[len(prefix) : -len(suffix)]
    return ""


def diff_case(outputs: RunnerOutputs) -> CaseDiffResult:
    """Diff a :class:`RunnerOutputs` against the committed ``expected/`` files.

    Checks every committed expected artifact and calls the appropriate
    comparison helper.  Missing expected files are treated as mismatches only
    if the corresponding output is non-trivial.

    The comparison rules follow the runner README contract exactly:
    * RDF artifacts (``projection-report.ttl``, ``*.ttl``) — graph isomorphism.
    * JSON artifacts (``preservation-ledger.json``) — canonical JSON.
    * Explanation skeletons — cited-IRI set comparison (not prose).

    Args:
        outputs: The fresh :class:`RunnerOutputs` from :func:`run`.

    Returns:
        A :class:`CaseDiffResult` with all diffs collected.
    """
    case_dir = outputs.case_dir
    case_id = f"{case_dir.parent.name}/{case_dir.name}"
    diffs: list[str] = []

    expected_root = case_dir / "expected"
    proj_expected = expected_root / "projections"

    # --- Projection RDF targets -------------------------------------------
    if proj_expected.is_dir():
        rdf_targets = {
            "owl-dl": "owl-dl.ttl",
            "owl-el": "owl-el.ttl",
            "gufo": "gufo.ttl",
            "canonical-rdf12": "canonical-rdf12.ttl",
        }
        proj_by_target = {pr.target: pr for pr in outputs.projections.results}

        for target_name, filename in rdf_targets.items():
            expected_path = proj_expected / filename
            pr = proj_by_target.get(target_name)
            if not expected_path.exists():
                # A missing golden is a hard failure when the runner produced a
                # non-trivial graph for this target.  Silent skipping would leave
                # projections untested (verification-honesty violation).
                if pr is not None and pr.graph is not None:
                    diffs.append(
                        f"[{case_id}] projection {target_name}: golden "
                        f"{filename} is missing from expected/projections/ — "
                        f"run `gmeow-dev conformance --update` to generate it"
                    )
                continue
            if pr is None or pr.graph is None:
                diffs.append(f"[{case_id}] projection {target_name}: no graph produced")
                continue
            expected_graph = Graph()
            try:
                expected_graph.parse(str(expected_path), format="turtle")
            except Exception as exc:
                diffs.append(f"[{case_id}] cannot parse expected {filename}: {exc}")
                continue
            rdf_diffs = compare_rdf(pr.graph, expected_graph)
            for d in rdf_diffs:
                diffs.append(f"[{case_id}] {target_name}: {d}")

        # --- Projection report ---
        report_path = proj_expected / "projection-report.ttl"
        if report_path.exists():
            expected_report = Graph()
            try:
                expected_report.parse(str(report_path), format="turtle")
            except Exception as exc:
                diffs.append(
                    f"[{case_id}] cannot parse expected projection-report.ttl: {exc}"
                )
            else:
                rdf_diffs = compare_rdf(
                    outputs.projections.report_graph, expected_report
                )
                for d in rdf_diffs:
                    diffs.append(f"[{case_id}] projection-report: {d}")

        # --- Preservation ledger JSON ---
        ledger_path = proj_expected / "preservation-ledger.json"
        if ledger_path.exists():
            try:
                expected_ledger = json.loads(ledger_path.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError) as exc:
                diffs.append(
                    f"[{case_id}] cannot parse expected preservation-ledger.json: {exc}"
                )
            else:
                json_diffs = compare_canonical_json(
                    outputs.projections.ledger_json,  # type: ignore[arg-type]
                    expected_ledger,
                )
                for d in json_diffs:
                    diffs.append(f"[{case_id}] preservation-ledger: {d}")

    # --- Verdicts ---
    verdicts_path = expected_root / "verdicts.json"
    if verdicts_path.exists():
        try:
            expected_verdicts = json.loads(verdicts_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            diffs.append(f"[{case_id}] cannot parse expected verdicts.json: {exc}")
        else:
            json_diffs = compare_canonical_json(outputs.verdicts, expected_verdicts)  # type: ignore[arg-type]
            for d in json_diffs:
                diffs.append(f"[{case_id}] verdicts: {d}")

    # --- Certification verdict (issue #502, Task 5) -----------------------
    #
    # Missing-golden rule (OPT-IN, mirrors the projections precedent but
    # inverted for corpus safety):
    #   * The runner ALWAYS produces a non-trivial certification dict, so a
    #     blanket "non-trivial output ⇒ require golden" rule (as used for
    #     projections) would force every legacy #501 case to grow a
    #     certification.json — they have none, so the corpus would go red.
    #   * Instead certification is COMPARED only when the case opts in: either
    #     the golden ``expected/certification.json`` already exists, OR
    #     ``profile.json`` sets ``"certify": true``.
    #   * Opt-in WITHOUT a golden is still a hard fail (verification-honesty:
    #     a declared certification expectation must be backed by a committed
    #     file, never silently skipped).
    # This keeps the #501 corpus green (no opt-in, no golden ⇒ no diff) while
    # the Task 6 profiles/decidability cases get full coverage by committing the
    # golden (which auto-enables the comparison).
    cert_path = expected_root / "certification.json"
    profile_data = _read_case_profile(case_dir)
    cert_opt_in = bool(profile_data.get("certify", False))
    if cert_path.exists():
        try:
            expected_cert = json.loads(cert_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            diffs.append(f"[{case_id}] cannot parse expected certification.json: {exc}")
        else:
            for d in compare_canonical_json(outputs.certification, expected_cert):
                diffs.append(f"[{case_id}] certification: {d}")
    elif cert_opt_in:
        diffs.append(
            f"[{case_id}] certification: golden certification.json is missing "
            f'from expected/ but profile.json declares "certify": true — '
            f"run `gmeow-dev conformance --update` to generate it"
        )

    # --- Budget governor markers (issue #502, Task 5) ---------------------
    #
    # Missing-golden rule (DECLARES-BUDGET ⇒ REQUIRE-GOLDEN):
    #   * A case only carries a budget marker when it declares ``budget_params``
    #     in profile.json.  Such a case asserts a budget outcome, so its golden
    #     ``expected/budget.json`` is REQUIRED — absence is a hard fail.
    #   * A case with NO ``budget_params`` runs unbounded (budget_status "ok",
    #     incomplete False — the trivial #501 outcome); it must NOT require a
    #     budget.json golden.  When the golden is absent and no budget is
    #     declared we skip silently (nothing non-trivial to test).
    #   * If a budget.json golden exists it is always compared, regardless of
    #     declaration (so a hand-committed golden is honoured).
    budget_path = expected_root / "budget.json"
    declares_budget = "budget_params" in profile_data
    actual_budget: dict[str, object] = {
        "budget_status": outputs.budget_status,
        "incomplete": outputs.incomplete,
    }
    if budget_path.exists():
        try:
            expected_budget = json.loads(budget_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            diffs.append(f"[{case_id}] cannot parse expected budget.json: {exc}")
        else:
            for d in compare_canonical_json(actual_budget, expected_budget):
                diffs.append(f"[{case_id}] budget: {d}")
    elif declares_budget:
        diffs.append(
            f"[{case_id}] budget: golden budget.json is missing from expected/ "
            f"but profile.json declares budget_params — "
            f"run `gmeow-dev conformance --update` to generate it"
        )

    # --- Materialized N-Quads ---
    mat_path = expected_root / "materialized.nq"
    if mat_path.exists():
        expected_nq_text = mat_path.read_text(encoding="utf-8")
        # Parse both as ConjunctiveGraphs for isomorphism comparison
        actual_cg: ConjunctiveGraph = ConjunctiveGraph()
        expected_cg: ConjunctiveGraph = ConjunctiveGraph()
        try:
            if outputs.materialized_nquads.strip():
                import io

                actual_cg.parse(
                    io.StringIO(outputs.materialized_nquads), format="nquads"
                )
            if expected_nq_text.strip():
                import io

                expected_cg.parse(io.StringIO(expected_nq_text), format="nquads")
        except Exception as exc:
            diffs.append(f"[{case_id}] materialized.nq parse error: {exc}")
        else:
            # Compare per named graph to detect cross-world leaks (Gap-1 fix).
            # A union comparison would allow a quad in the wrong graph to pass
            # as long as the (S,P,O) triple existed in ANY world — defeating the
            # world-isolation invariant required by issue #501 AC(a)/(b).
            actual_graph_iris: set[URIRef] = {
                ctx.identifier
                for ctx in actual_cg.contexts()
                if isinstance(ctx.identifier, URIRef)
            }
            expected_graph_iris: set[URIRef] = {
                ctx.identifier
                for ctx in expected_cg.contexts()
                if isinstance(ctx.identifier, URIRef)
            }
            # Report named graphs present on one side but not the other.
            for extra_iri in sorted(actual_graph_iris - expected_graph_iris):
                diffs.append(
                    f"[{case_id}] materialized.nq: named graph present in actual"
                    f" but not expected: <{extra_iri}>"
                )
            for missing_iri in sorted(expected_graph_iris - actual_graph_iris):
                diffs.append(
                    f"[{case_id}] materialized.nq: named graph present in expected"
                    f" but not actual: <{missing_iri}>"
                )
            # Per-shared-graph triple comparison using the existing compare_rdf helper.
            for graph_iri in sorted(actual_graph_iris & expected_graph_iris):
                actual_g: Graph = Graph(store=actual_cg.store, identifier=graph_iri)
                expected_g: Graph = Graph(store=expected_cg.store, identifier=graph_iri)
                rdf_diffs = compare_rdf(actual_g, expected_g)
                for d in rdf_diffs:
                    diffs.append(f"[{case_id}] materialized.nq [<{graph_iri}>]: {d}")

    # --- Explanation skeletons ---
    expl_expected = expected_root / "explanation"
    if expl_expected.is_dir():
        produced = {e.target_quad_reifier: e for e in outputs.explanations}
        for md_path in sorted(expl_expected.glob("*.md")):
            md_text = md_path.read_text(encoding="utf-8")
            committed_iris = _parse_cited_iri_skeleton(md_text)
            reifier = _parse_explanation_reifier(md_text)
            actual = produced.get(reifier)
            if actual is None:
                diffs.append(
                    f"[{case_id}] explanation {md_path.name}: golden cites reifier"
                    f" <{reifier}> but the runner produced no explanation for it"
                )
                continue
            diffs.extend(
                f"[{case_id}] explanation {md_path.name}: {d}"
                for d in compare_explanation_skeleton(actual.cited_iris, committed_iris)
            )

    return CaseDiffResult(
        case_id=case_id,
        passed=(len(diffs) == 0),
        diffs=diffs,
    )
