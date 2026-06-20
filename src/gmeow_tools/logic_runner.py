# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Conformance runner for the Logic v1 monotonic core (issue #501, Task 7).

This module is the **conformance runner** — it wires :mod:`~.logic_seam` (the
Rust-fed seam containers) and the native ``gmeow_logic`` engine
(``compile_logic`` / ``materialize`` / ``certify`` / ``explain``) into the single
``run()`` function required by the runner contract in
``conformance/logic/runner/README.md``.

The whole compiler — frontend (Turtle → IR), the seven projection back-ends, the
preservation ledger, and the nemo rule extraction — runs in Rust
(``gmeow_logic.compile_logic``, issue #664/#727): the Python compiler duplicate
(the frontend / IR / adapter / projection modules) was deleted in #727.
Reasoning is likewise Rust-authoritative (#651): there is no Python forward-chase
oracle.

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
from typing import cast

from rdflib import ConjunctiveGraph, Graph, URIRef
from rdflib.compare import isomorphic

from gmeow_tools.logic_seam import (
    _ASSERT_RULE_IRI,
    BudgetParams,
    DerivedQuad,
    Explanation,
    ExplanationStep,
    MaterializationResult,
)

_log = logging.getLogger(__name__)

#: The six ``logic:SemanticProfile`` local names (mirrors the Rust
#: ``SemanticProfileId`` enum / the ontology's named individuals).  A case's
#: declared ``semantic_profile`` must be one of these — an unknown localname is a
#: hard failure (no silent fallback).
_VALID_SEMANTIC_PROFILES: frozenset[str] = frozenset(
    {
        "PositiveHornProfile",
        "StratifiedNAFProfile",
        "WellFoundedProfile",
        "StableModelProfile",
        "ProceduralPrologProfile",
        "ProbabilisticProfile",
    }
)

#: The semantic-profile string the native materializer / foundation evaluator
#: stamps on every PositiveHorn run (the committed-golden profile IRI localname).
_POSITIVE_HORN_PROFILE = "PositiveHornProfile"

#: The seven projection target short-names, in canonical order.
_PROJECTION_TARGETS: tuple[str, ...] = (
    "owl-dl",
    "owl-el",
    "datalog",
    "n3",
    "gufo",
    "canonical-rdf12",
    "nemo",
)

#: Map a projection target short-name to its ``compile_logic`` dict key.
_TARGET_TO_KEY: dict[str, str] = {
    "owl-dl": "owl_dl",
    "owl-el": "owl_el",
    "datalog": "datalog",
    "n3": "n3",
    "gufo": "gufo",
    "canonical-rdf12": "canonical_rdf12",
    "nemo": "nemo",
}

#: Which projection targets serialize RDF (re-parsed into an rdflib Graph for the
#: isomorphism diff); the rest are plain text.
_RDF_TARGETS: frozenset[str] = frozenset(
    {"owl-dl", "owl-el", "gufo", "canonical-rdf12"}
)


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
class RunnerProjection:
    """One projection back-end's runner-facing result.

    The compiler runs in Rust (:func:`gmeow_logic.compile_logic`); this is the
    thin container the runner builds from each artifact string for the
    conformance diff.

    Attributes:
        target: Short target name (``"owl-dl"``, ``"datalog"``, …).
        content: The serialized artifact string from the Rust compiler.
        graph: The re-parsed rdflib :class:`~rdflib.Graph` for RDF targets
            (used for the isomorphism diff), or ``None`` for text targets.
    """

    target: str
    content: str
    graph: Graph | None


@dataclass(frozen=True, slots=True)
class ProjectionOutputs:
    """All projection back-end results for one case run.

    Attributes:
        results: The 7 :class:`RunnerProjection` objects (one per back-end).
        report_graph: The preservation report as an rdflib
            :class:`~rdflib.Graph`.
        ledger_json: The preservation ledger as a canonical JSON dict.
            Keys are target names; values are ``{preservation, complexity,
            lossy_drops}`` dicts.  Built by the Rust compiler.
    """

    results: tuple[RunnerProjection, ...]
    report_graph: Graph
    ledger_json: dict[str, dict[str, object]]


@dataclass(frozen=True, slots=True)
class RunnerOutputs:
    """Complete runner output for one conformance case.

    Attributes:
        case_dir: The case directory that was run.
        mode: The engine/fragment mode requested (e.g. ``"native"``).

        materialized: The chase result.  **Populated** for v1 monotonic core.
        materialized_nquads: N-Quads serialization of ``materialized`` (all
            worlds as named graphs, sorted for determinism).  **Populated**.

        projections: All projection back-ends + ledger.  **Populated**.

        explanations: Per-quad explanation skeletons.  **Populated**
            (one :class:`~.logic_seam.Explanation` per quad, in
            ``result.quads`` order).

        verdicts: World-indexed truth verdicts JSON dict.  **Populated** as a
            minimal ``{world_iri: {"quads": n, "status": "consistent"}}``
            mapping.  Full modality/truth verdicts deferred to #503.

        witnesses: Contradiction witnesses.  **Always ``{}``** in v1 monotonic
            core (no negation → no within-world contradiction).  Deferred to
            #503.

        answers: Goal/counterfactual answer sets.  Populated when the case has a
            ``queries/`` directory containing ``*.logic`` files; each entry maps
            the query stem to the ``{"bindings": [...], "status": "..."}`` dict
            returned by :func:`gmeow_logic.query`.  Empty dict when no queries
            are present.  Implements issue #504 backward goals.

        certification: The static profile/decidability verdict for the compiled
            program against the case's declared profile, as the sorted-key
            dict produced by the native ``gmeow_logic.certify`` certifier.
            **Populated** for every case (issue #502, Task 5).

        budget_status: The aggregate budget-governor status copied from
            :attr:`~.logic_seam.MaterializationResult.budget_status` —
            ``"ok"`` when the chase reached fixpoint within budget (or ran
            unbounded), ``"exhausted"`` when a declared ceiling was tripped.
            **Populated** for every case.

        incomplete: ``True`` iff the materialization was a sound partial because
            a budget ceiling was tripped (i.e. ``budget_status == "exhausted"``).
            Mirrors :attr:`~.logic_seam.MaterializationResult.incomplete`.
    """

    case_dir: Path
    mode: str

    # v1 populated outputs
    materialized: MaterializationResult
    materialized_nquads: str
    projections: ProjectionOutputs
    explanations: tuple[Explanation, ...]

    # v1 minimal/stub outputs (documented deferred)
    verdicts: dict[str, dict[str, object]]
    witnesses: dict[str, object]  # always {}
    answers: dict[str, object]  # populated by _resolve_answers (#504)

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
    """Serialize a :class:`~.logic_seam.MaterializationResult` to N-Quads.

    Produces a deterministic N-Quads string with one line per quad, sorted
    lexicographically by (graph, subject, predicate, object) — the canonical
    order used by :mod:`~.logic_seam`.

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


def _build_projection_outputs(compiled: dict[str, object]) -> ProjectionOutputs:
    """Build :class:`ProjectionOutputs` from a ``compile_logic`` result dict.

    The Rust compiler (:func:`gmeow_logic.compile_logic`) already ran every
    projection back-end, the overclaim gate, and the preservation ledger; this
    function only repackages those artifacts for the conformance diff — it
    re-parses the four RDF artifact strings into rdflib Graphs (for the
    isomorphism comparison) and the report Turtle into a Graph, and copies the
    Rust-built ``preservation_ledger`` dict verbatim.

    Args:
        compiled: The dict returned by :func:`gmeow_logic.compile_logic`.

    Returns:
        A :class:`ProjectionOutputs` with all 7 results, the report graph, and
        the JSON ledger.

    Raises:
        RunnerError: If an artifact key is missing or an RDF artifact cannot be
            re-parsed (a Rust↔Python contract violation).
    """
    results: list[RunnerProjection] = []
    for target in _PROJECTION_TARGETS:
        key = _TARGET_TO_KEY[target]
        if key not in compiled:
            raise RunnerError(
                f"compile_logic produced no artifact for target {target!r} "
                f"(missing key {key!r})"
            )
        content = str(compiled[key])
        graph: Graph | None = None
        if target in _RDF_TARGETS:
            graph = Graph()
            try:
                graph.parse(data=content, format="turtle")
            except Exception as exc:
                raise RunnerError(
                    f"compile_logic {target!r} artifact is not parseable Turtle: {exc}"
                ) from exc
        results.append(RunnerProjection(target=target, content=content, graph=graph))

    report_graph = Graph()
    try:
        report_graph.parse(data=str(compiled["report"]), format="turtle")
    except Exception as exc:
        raise RunnerError(
            f"compile_logic projection report is not parseable Turtle: {exc}"
        ) from exc

    # The preservation ledger is built by the Rust compiler; copy it verbatim
    # (deep-copied to plain dicts/lists so the canonical-JSON comparison is stable).
    raw_ledger = compiled.get("preservation_ledger")
    if not isinstance(raw_ledger, dict):
        raise RunnerError(
            "compile_logic did not return a preservation_ledger dict "
            f"(got {type(raw_ledger).__name__})"
        )
    ledger_json: dict[str, dict[str, object]] = {
        str(target): {
            "preservation": row["preservation"],
            "complexity": row["complexity"],
            "lossy_drops": list(row["lossy_drops"]),
        }
        for target, row in raw_ledger.items()
    }

    return ProjectionOutputs(
        results=tuple(results),
        report_graph=report_graph,
        ledger_json=ledger_json,
    )


# --------------------------------------------------------------------------- #
# Explanation builder
# --------------------------------------------------------------------------- #


def _run_explanations(result: MaterializationResult) -> tuple[Explanation, ...]:
    """Produce explanation skeletons for every quad in ``result``.

    The explanation *engine* is native Rust (``gmeow_logic.explain``, issue
    #497): the retired Python oracle (``logic_explain.py``) is gone and there is
    no fallback (no-optionality doctrine — a missing extension is a hard
    failure, mirroring :func:`_materialize_foundation`).  One explanation is
    produced per quad, in ``result.quads`` order; asserted quads (rule_iri ==
    ``logic:assert``) get a trivial depth-0 explanation.

    Args:
        result: The materialization result.

    Returns:
        A tuple of :class:`~.logic_seam.Explanation` objects, one per
        quad, in input order.

    Raises:
        RunnerError: If the ``gmeow_logic`` extension is not installed (hard
            fail), or if the native engine rejects the proof trace.
    """
    if not result.quads:
        return ()

    try:
        import gmeow_logic
    except ImportError as exc:
        raise RunnerError(
            "gmeow_logic native extension is not installed but explanations were "
            "requested — run 'make logic-py' first: "
            f"{exc}"
        ) from exc

    # Build the payload list of dicts from the materialized quads (one per quad,
    # preserving order).  The native engine reads the same seam fields the Python
    # oracle did: graph/subject/predicate/obj/derivation_id/rule_iri/source_quad_ids.
    payload = [
        {
            "graph": quad.graph,
            "subject": quad.subject,
            "predicate": quad.predicate,
            "obj": quad.obj,
            "derivation_id": quad.derivation_id,
            "rule_iri": quad.rule_iri,
            "source_quad_ids": list(quad.source_quad_ids),
        }
        for quad in result.quads
    ]

    try:
        rows = gmeow_logic.explain(payload)
    except (ValueError, RuntimeError) as exc:
        raise RunnerError(f"gmeow_logic.explain failed: {exc}") from exc

    explanations: list[Explanation] = []
    for row in rows:
        steps = tuple(
            ExplanationStep(
                derivation_id=str(step["derivation_id"]),
                rule_iri=str(step["rule_iri"]),
                quad_reifier=str(step["quad_reifier"]),
                subject_iri=str(step["subject_iri"]),
                predicate_iri=str(step["predicate_iri"]),
                obj_n3=str(step["obj_n3"]),
                graph_iri=str(step["graph_iri"]),
                term_iris=tuple(str(t) for t in step["term_iris"]),
                source_step_ids=tuple(str(s) for s in step["source_step_ids"]),
                is_asserted=bool(step["is_asserted"]),
                depth=int(step["depth"]),
            )
            for step in row["step_skeleton"]
        )
        explanations.append(
            Explanation(
                target_derivation_id=str(row["target_derivation_id"]),
                target_quad_reifier=str(row["target_quad_reifier"]),
                world_iri=str(row["world_iri"]),
                step_skeleton=steps,
                cited_iris=frozenset(str(c) for c in row["cited_iris"]),
            )
        )

    return tuple(explanations)


# --------------------------------------------------------------------------- #
# Static certification (native, Rust-authoritative — issue #497)
# --------------------------------------------------------------------------- #


def _certify_native(
    nemo_rules: str,
    declared_profile: str,
    case_label: str,
) -> dict[str, object]:
    """Statically certify ``nemo_rules`` against ``declared_profile`` via Rust.

    The native ``gmeow_logic.certify`` certifier is the sole reasoning authority
    (Principle 17, the "maximally use Rust" doctrine); the Python certifier was
    retired in #651 (parity is now pinned by the conformance ``certification.json``
    goldens + the ``crates/logic/src/certify.rs`` cargo tests).  The engine takes
    the ``% === Rules ===`` section of the nemo projection (the ground-fact axioms
    are not certification inputs — supplied here as the ``nemo_rules`` key of
    :func:`gmeow_logic.compile_logic`) plus the declared profile, and returns the
    verdict dict (``certified``, ``decidability_class``, ``profile_id``, sorted
    ``violations``) — the exact shape the conformance ``certification.json``
    golden compares.

    Args:
        nemo_rules: The ``% === Rules ===`` section of the nemo projection (the
            ``nemo_rules`` key from :func:`gmeow_logic.compile_logic`).
        declared_profile: The semantic-profile localname the case declares.
        case_label: Case name for error messages.

    Returns:
        The certification verdict as a JSON-able dict.

    Raises:
        RunnerError: If the ``gmeow_logic`` extension is not installed (hard
            fail, no Python fallback) or the native certifier raises.
    """
    try:
        import gmeow_logic
    except ImportError as exc:
        raise RunnerError(
            f"Case {case_label}: gmeow_logic native extension is not installed "
            "(certification is Rust-authoritative since #497) — run 'make logic-py'."
        ) from exc

    try:
        verdict = gmeow_logic.certify(nemo_rules, declared_profile)
    except (ValueError, RuntimeError) as exc:
        raise RunnerError(
            f"Case {case_label}: gmeow_logic.certify failed for profile "
            f"{declared_profile}: {exc}"
        ) from exc
    return dict(verdict)


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
    integers, matching :class:`~.logic_seam.BudgetParams`).  When the key
    is absent the chase runs unbounded (``None``) — the #501 default that keeps
    the existing corpus byte-identical.

    Hard-fail (no silent coercion) on a malformed value: a non-object
    ``budget_params``, an unknown key, or a non-integer / non-positive ceiling is
    a :class:`RunnerError`, never a degraded default.

    Args:
        case_dir: The case directory (used only for error messages).
        profile_data: The parsed ``profile.json`` dict.

    Returns:
        A :class:`~.logic_seam.BudgetParams` when ``budget_params`` is
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


def _bare_iri(term_surface: str) -> str:
    """Strip one layer of N3 angle brackets from a term surface.

    The native ``gmeow_logic.materialize`` seam emits the *subject* as a term
    display form (``<iri>`` for a NamedNode), while the explanation engine's reifier
    reconstruction (and the Python :class:`DerivedQuad` contract) expects the BARE
    IRI. A blank-node (``_:b``) or already-bare value passes through unchanged.
    """
    if len(term_surface) >= 2 and term_surface[0] == "<" and term_surface[-1] == ">":
        return term_surface[1:-1]
    return term_surface


def _materialize_native(
    case_dir: Path,
    nemo_rules: str,
    input_graph: ConjunctiveGraph,
    semantic_profile_str: str,
    budget: BudgetParams | None,
) -> MaterializationResult:
    """Materialize the forward chase via the native ``gmeow_logic.materialize``.

    This is the Rust-authoritative default path (issue #651): the Python
    forward-chase oracle (``materialize_program``) is retired. The program's
    ``% === Rules ===`` projection (with ``#[name("…")]`` provenance annotations)
    and the world facts (``input.nq`` as N-Quads) are handed to the engine, which
    routes by ``semantic_profile_str``:

    * PositiveHorn / stratified NAF → the Nemo chase;
    * WellFounded → the native alternating-fixpoint evaluator;
    * StableModel → the native cautious (skeptical) materialization;
    * a declared StratifiedNAF set that fails stratification → asserted-only.

    Provenance is content-addressed and identical to the foundation path, so the
    native ``gmeow_logic.explain`` consumes the rows unchanged.

    Raises:
        RunnerError: If the ``gmeow_logic`` extension is not installed (hard fail,
            no Python fallback), the NEMO projection fails, or the engine raises.
    """
    try:
        import gmeow_logic
    except ImportError as exc:
        raise RunnerError(
            f"Case {case_dir.name}: gmeow_logic native extension is not installed "
            "(materialization is Rust-authoritative since #651) — run 'make logic-py'."
        ) from exc

    input_nq_text = input_graph.serialize(format="nquads")

    try:
        rows = gmeow_logic.materialize(
            nemo_rules,
            input_nq_text,
            budget.max_rule_firings if budget else None,
            budget.max_answers if budget else None,
            budget.time_ms if budget else None,
            semantic_profile_str,
        )
    except (ValueError, RuntimeError) as exc:
        # ValueError = input/rule parse error; RuntimeError = chase/provenance/eval
        # failure. Both must be wrapped so the runner's case-level boundary holds.
        raise RunnerError(
            f"Case {case_dir.name}: gmeow_logic.materialize failed: {exc}"
        ) from exc

    quads: list[DerivedQuad] = [
        DerivedQuad(
            graph=row["graph"],
            subject=_bare_iri(row["subject"]),
            predicate=row["predicate"],
            obj=row["object"],
            graph_component=row["graph_component"],
            derivation_id=row["derivation_id"],
            rule_iri=row["rule_iri"],
            source_quad_ids=list(row["source_quad_ids"]),
            profile=row["profile"],
            budget_status=row["budget_status"],
        )
        for row in rows
    ]

    worlds = frozenset(q.graph for q in quads)
    derived_count = sum(1 for q in quads if q.rule_iri != _ASSERT_RULE_IRI)
    input_count = len(quads) - derived_count
    exhausted = any(q.budget_status == "exhausted" for q in quads)

    return MaterializationResult(
        quads=tuple(quads),
        worlds=worlds,
        # Label the artifact with the profile the run was actually computed under
        # (PositiveHorn / stratified NAF via Nemo, WellFounded / StableModel via the
        # native evaluators) rather than a hardcoded PositiveHorn — the native
        # evaluators apply the declared semantics fully, so the artifact must say so.
        profile=semantic_profile_str,
        # The native evaluators apply the declared semantics fully (well-founded /
        # cautious-stable / stratified NAF) — there is no positive over-approximation,
        # so there is nothing to record as projection loss.
        loss_entries=(),
        input_quad_count=input_count,
        derived_quad_count=derived_count,
        budget_status="exhausted" if exhausted else "ok",
        incomplete=exhausted,
    )


def _resolve_witnesses(
    case_dir: Path,
    nemo_rules: str,
    input_graph: ConjunctiveGraph,
    semantic_profile_str: str,
) -> dict[str, object]:
    """Resolve the stable-model witnesses for a StableModel case (issue #651).

    For every other profile the within-world materialization is single-model, so
    there are no answer-set witnesses and the result is ``{}``. For
    ``StableModelProfile`` the cautious intersection (what ``materialized.nq``
    carries) hides the individual models, so they are surfaced here for the
    ``witnesses.json`` side file: a dict keyed by world IRI, each value a list of
    models, each model a sorted list of atom dicts ``{subject, predicate, object}``.
    """
    if semantic_profile_str != "StableModelProfile":
        return {}
    try:
        import gmeow_logic
    except ImportError as exc:
        raise RunnerError(
            f"Case {case_dir.name}: gmeow_logic native extension is not installed "
            "(stable-model witnesses are Rust-authoritative) — run 'make logic-py'."
        ) from exc
    input_nq_text = input_graph.serialize(format="nquads")
    try:
        return dict(gmeow_logic.stable_models(nemo_rules, input_nq_text))
    except (ValueError, RuntimeError) as exc:
        raise RunnerError(
            f"Case {case_dir.name}: gmeow_logic.stable_models failed: {exc}"
        ) from exc


# --------------------------------------------------------------------------- #
# Backward goal resolver (issue #504)
# --------------------------------------------------------------------------- #


def _resolve_answers(
    case_dir: Path,
    world_nquads: str,
    profile_str: str,
    budget: BudgetParams | None,
) -> dict[str, object]:
    """Resolve every ``queries/*.logic`` backward goal over the materialized EDB.

    Calls the Rust engine (``gmeow_logic.query``) for each ``.logic`` file found
    in ``case_dir/queries/``.  Returns a dict mapping each query stem to the
    ``{"bindings": [...], "status": "..."}`` dict returned by the engine.

    Empty dict is returned when ``case_dir/queries/`` does not exist or contains
    no ``*.logic`` files — no Rust import is performed in that case, so cases
    without queries have zero overhead and remain byte-identical.

    Args:
        case_dir: The conformance case directory.
        world_nquads: The materialized world(s) as a sorted N-Quads string
            (one named graph per world), as produced by
            :func:`_materialize_to_nquads`.
        profile_str: The semantic profile string declared in ``profile.json``
            (e.g. ``"PositiveHornProfile"``).
        budget: The parsed budget params (used for ``max_answers``).  ``None``
            means unbounded.

    Returns:
        A dict ``{query_stem: {"bindings": [...], "status": "..."}}`` for each
        ``.logic`` query file, sorted by stem for determinism.  Empty dict when
        the case has no ``queries/`` directory.

    Raises:
        RunnerError: If ``gmeow_logic`` cannot be imported (extension not built)
            AND the case has queries (no-optionality doctrine); or if any query
            raises a ``ValueError`` from the engine.
    """
    queries_dir = case_dir / "queries"
    if not queries_dir.is_dir():
        return {}

    query_files = sorted(queries_dir.glob("*.logic"))
    if not query_files:
        return {}

    # Only import gmeow_logic when queries are present.  Missing extension when
    # queries exist is a hard failure (no degraded fallback — no-optionality doctrine;
    # the conformance CI always builds the extension).
    try:
        import gmeow_logic
    except ImportError as exc:
        raise RunnerError(
            f"Case {case_dir.name}: gmeow_logic native extension is not installed "
            f"but the case has queries/ — run 'make logic-py' first: {exc}"
        ) from exc

    max_answers: int | None = budget.max_answers if budget is not None else None

    result: dict[str, object] = {}
    for qfile in query_files:
        try:
            qtext = qfile.read_text(encoding="utf-8")
        except OSError as exc:
            raise RunnerError(
                f"Case {case_dir.name}: cannot read query {qfile.name}: {exc}"
            ) from exc
        try:
            answer = gmeow_logic.query(
                world_nquads,
                qtext,
                profile_str,
                None,  # world_iri=None → auto-detect single world
                max_answers,
                None,  # max_steps=None
            )
        except ValueError as exc:
            raise RunnerError(
                f"Case {case_dir.name}: query {qfile.name} failed: {exc}"
            ) from exc
        result[qfile.stem] = answer

    return result


# --------------------------------------------------------------------------- #
# Foundation lowering (issue #636) — native Rust evaluation
# --------------------------------------------------------------------------- #


def _materialize_foundation(
    case_dir: Path,
    input_graph: ConjunctiveGraph,
    profile_data: dict[str, object],
    budget: BudgetParams | None,
) -> MaterializationResult:
    """Evaluate a foundation-lowering case via the native ``gmeow_logic.foundation``.

    The OntoUML-discipline lowering (``logic:violation``), the cross-world rigidity
    closure (``logic:rigidityViolation``) and the anti-rigidity witness policy
    (``logic:dischargeObligation`` / ``logic:witnessRequiredViolation``) are all
    computed by the native Rust evaluator (issue #636).  The retired Python oracle
    (``logic_foundation.py``) is gone — there is no fallback (no-optionality
    doctrine: a missing extension is a hard failure).

    The input world facts (``input.nq``, one named graph per world) are serialized
    to N-Quads and handed to ``gmeow_logic.foundation``; its full-provenance rows
    are mapped one-to-one onto :class:`~.logic_seam.DerivedQuad` records and
    assembled into a :class:`~.logic_seam.MaterializationResult` that every
    downstream consumer (explanations, verdicts, projections, certification) reads
    unchanged.

    Args:
        case_dir: The conformance case directory (used only for error messages).
        input_graph: The parsed world-fact ConjunctiveGraph (from ``input.nq``).
        profile_data: The parsed ``profile.json`` dict; ``anti_rigidity_policy``
            selects the closed witness policy (default ``"witness-obligation"``).
        budget: The parsed budget governor, or ``None`` for unbounded.  The native
            foundation evaluator does not (yet) honour budget ceilings, so a
            non-``None`` value is a hard failure rather than a silently-ignored
            parameter (see Raises).

    Returns:
        A :class:`~.logic_seam.MaterializationResult` over the native rows.

    Raises:
        RunnerError: If the ``gmeow_logic`` extension is not installed (hard fail,
            no Python fallback); if the case declares ``budget_params`` (the native
            foundation path cannot enforce a budget, so it must not fabricate a
            ``budget_status``); or if the native evaluation raises.
    """
    # No-optionality / hard-fail (no silent degradation): the native foundation
    # evaluator runs to full fixpoint and has no budget governor.  A case that
    # declares ``budget_params`` would otherwise receive a fabricated
    # ``budget_status="ok"`` / ``incomplete=False`` artifact that does not reflect
    # any enforced ceiling.  Fail loudly instead of emitting a misleading result.
    if budget is not None:
        raise RunnerError(
            f"Case {case_dir.name}: foundation_lowering cases cannot declare "
            f"budget_params — the native gmeow_logic.foundation evaluator has no "
            f"budget governor and must not fabricate a budget_status. Remove "
            f"budget_params from profile.json (the foundation chase is unbounded)."
        )

    try:
        import gmeow_logic
    except ImportError as exc:
        raise RunnerError(
            f"Case {case_dir.name}: gmeow_logic native extension is not installed "
            f"but the case opts into foundation_lowering — run 'make logic-py' "
            f"first: {exc}"
        ) from exc

    # Serialize the world-fact graph to N-Quads for the native evaluator (one named
    # graph per world; the same surface the Rust foundation loader expects).
    input_nq_text = input_graph.serialize(format="nquads")

    policy = str(profile_data.get("anti_rigidity_policy", "witness-obligation"))
    try:
        rows = gmeow_logic.foundation(input_nq_text, policy)
    except (ValueError, RuntimeError) as exc:
        # gmeow_logic.foundation surfaces input/policy errors as ValueError and
        # evaluator/provenance failures as RuntimeError; both must be wrapped in
        # RunnerError so the runner contract holds (a RuntimeError would otherwise
        # escape the case-level error boundary).
        raise RunnerError(
            f"Case {case_dir.name}: gmeow_logic.foundation failed: {exc}"
        ) from exc

    quads: list[DerivedQuad] = [
        DerivedQuad(
            graph=row["graph"],
            subject=row["subject"],
            predicate=row["predicate"],
            obj=row["obj"],
            # Foundation worlds are flat named graphs: the graph IS the component.
            graph_component=row["graph"],
            derivation_id=row["derivation_id"],
            rule_iri=row["rule_iri"],
            source_quad_ids=list(row["source_quad_ids"]),
            profile=row["profile"],
            budget_status=row["budget_status"],
        )
        for row in rows
    ]

    worlds = frozenset(q.graph for q in quads)
    # Derived = every quad that is not a verbatim asserted input fact.
    derived_count = sum(1 for q in quads if q.rule_iri != _ASSERT_RULE_IRI)
    input_count = len(quads) - derived_count

    return MaterializationResult(
        quads=tuple(quads),
        worlds=worlds,
        # Foundation cases materialize under PositiveHorn semantics (matching the
        # native evaluator's stamped profile and the committed goldens); the
        # declared StratifiedNAF profile is exercised by the static certifier.
        profile=_POSITIVE_HORN_PROFILE,
        loss_entries=(),
        input_quad_count=input_count,
        derived_quad_count=derived_count,
        budget_status="ok",
        incomplete=False,
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

    # Compile the logic: source in Rust (issue #664/#727): the whole frontend →
    # IR → 7-projection + report + ledger + nemo-rules pipeline runs in
    # ``gmeow_logic.compile_logic`` — the Python compiler duplicate was deleted in
    # #727.  Every downstream artifact (projections, ledger, the nemo rule text
    # the reasoning engines consume) is read off this single result dict.
    source_ttl = input_path.read_text(encoding="utf-8")
    try:
        import gmeow_logic
    except ImportError as exc:
        raise RunnerError(
            f"Case {case_dir.name}: gmeow_logic native extension is not installed "
            "(the logic compiler is Rust-authoritative since #664) — "
            "run 'make logic-py'."
        ) from exc
    try:
        compiled = gmeow_logic.compile_logic(source_ttl)
    except (ValueError, RuntimeError) as exc:
        raise RunnerError(
            f"Case {case_dir.name}: gmeow_logic.compile_logic failed: {exc}"
        ) from exc

    for diag in compiled.get("diagnostics", []):
        _log.debug(
            "parse diagnostic [%s] %s: %s",
            diag["severity"],
            diag["code"],
            diag["message"],
        )

    # The ``% === Rules ===`` section of the nemo projection — the reasoning-engine
    # rule surface (materialize / certify / stable_models all consume it).
    nemo_rules = str(compiled["nemo_rules"])

    # Resolve the semantic profile to use for materialization
    semantic_profile_str = str(
        profile_data.get("semantic_profile", "PositiveHornProfile")
    )
    # An unknown localname is a hard failure (no silent fallback): the case author
    # must declare a real semantic profile.
    if semantic_profile_str not in _VALID_SEMANTIC_PROFILES:
        raise RunnerError(
            f"Case {case_dir.name}: unknown semantic_profile "
            f"{semantic_profile_str!r} in profile.json — must be one of "
            f"{sorted(_VALID_SEMANTIC_PROFILES)}"
        )

    # The native engine applies the DECLARED semantics directly (issue #651):
    # PositiveHorn / stratified NAF via the Nemo chase; WellFounded and StableModel
    # via the native non-stratifiable evaluators. There is no PositiveHorn
    # over-approximation any more, so no loss is recorded for a non-PositiveHorn
    # profile — the prior "v1 oracle" warning is retired with the oracle.

    # Static certification against the DECLARED profile (issue #502, Task 5;
    # made Rust-authoritative in #497).  Pure analysis over the projected rules;
    # a non-empty ``violations`` list is surfaced (not raised) so the conformance
    # diff can compare it as a golden artifact.
    certification = _certify_native(nemo_rules, semantic_profile_str, case_dir.name)

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

    # Foundation lowering (issue #503 / #636).  ONLY when a case opts in via
    # ``profile.json`` ``"foundation_lowering": true`` — never auto-gated on
    # stereotype presence, so the existing projections/kind-hierarchy case (which
    # declares logic:Kind/SubKind/Role + a logic:subClassOf chain) stays
    # byte-identical (Corpus-safety, issue #503).  The foundation path is evaluated
    # entirely by the native Rust evaluator ``gmeow_logic.foundation`` (issue #636,
    # Task 2): the Python OntoUML-discipline oracle (``logic_foundation.py``) has
    # been retired — there is no Python fallback (no-optionality doctrine).
    if profile_data.get("foundation_lowering") is True:
        mat_result = _materialize_foundation(
            case_dir, input_graph, profile_data, budget
        )
    else:
        # Rust-authoritative default forward chase (issue #651): the Python oracle
        # (``materialize_program``) is retired. ``gmeow_logic.materialize`` routes by
        # the declared semantic profile — the Nemo chase for PositiveHorn / stratified
        # NAF, and the native well-founded / stable-model evaluators for the
        # non-stratifiable profiles the Nemo chase rejects. Provenance is the same
        # content-addressed reifier graph the native explain / query / counterfactual
        # consumers reconstruct.
        mat_result = _materialize_native(
            case_dir, nemo_rules, input_graph, semantic_profile_str, budget
        )

    # N-Quads serialization
    nquads_str = _materialize_to_nquads(mat_result)

    # Backward goal resolution (issue #504): resolve every queries/*.logic file
    # over the materialized EDB via gmeow_logic.query.  Empty dict for cases
    # without a queries/ directory (no overhead, byte-identical to pre-#504).
    # The query-resolution profile may differ from the materialization profile:
    # a Stratum-C counterfactual case (#505) keeps a valid materialization
    # ``semantic_profile`` (e.g. PositiveHornProfile) and selects its counterfactual
    # revision mode via the optional ``counterfactual_profile`` key (e.g.
    # ``LewisCredulousProfile``), which is passed straight to ``gmeow_logic.query``.
    query_profile = str(
        profile_data.get("counterfactual_profile", semantic_profile_str)
    )
    answers = _resolve_answers(case_dir, nquads_str, query_profile, budget)

    # Projections (built from the Rust compile_logic result — issue #727).  The
    # TypedDict is cast to a plain str-keyed mapping for the dynamic-key reads
    # (per-target artifact + ledger) inside the builder.
    proj_outputs = _build_projection_outputs(cast("dict[str, object]", compiled))

    # Explanations (over whatever quads exist; empty for projection-only cases)
    explanations = _run_explanations(mat_result)

    # Verdicts (v1 minimal)
    verdicts = _build_verdicts(mat_result)

    # Stable-model witnesses (issue #651): the individual answer sets, surfaced for
    # the ``witnesses.json`` side file. ``{}`` for every single-model profile.
    witnesses = _resolve_witnesses(
        case_dir, nemo_rules, input_graph, semantic_profile_str
    )

    return RunnerOutputs(
        case_dir=case_dir,
        mode=mode,
        materialized=mat_result,
        materialized_nquads=nquads_str,
        projections=proj_outputs,
        explanations=explanations,
        verdicts=verdicts,
        witnesses=witnesses,
        answers=answers,
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

    # --- Answers (issue #504 backward goals) ---
    queries_dir = case_dir / "queries"
    answers_dir = expected_root / "answers"
    if queries_dir.is_dir():
        for qfile in sorted(queries_dir.glob("*.logic")):
            stem = qfile.stem
            expected_path = answers_dir / f"{stem}.json"
            actual_answer: object = outputs.answers.get(stem)
            if not expected_path.exists():
                diffs.append(
                    f"[{case_id}] answers/{stem}: golden "
                    f"expected/answers/{stem}.json is missing"
                )
                continue
            try:
                expected_answer = json.loads(expected_path.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError) as exc:
                diffs.append(
                    f"[{case_id}] cannot parse expected answers/{stem}.json: {exc}"
                )
                continue
            if actual_answer is None:
                diffs.append(
                    f"[{case_id}] answers/{stem}: run() produced no answer set"
                )
                continue
            for d in compare_canonical_json(actual_answer, expected_answer):  # type: ignore[arg-type]
                diffs.append(f"[{case_id}] answers/{stem}: {d}")

    return CaseDiffResult(
        case_id=case_id,
        passed=(len(diffs) == 0),
        diffs=diffs,
    )
