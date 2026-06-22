# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
r"""Rust-fed seam data containers for the Logic runtime.

Rust/Python boundary (issue #651 / #727)
----------------------------------------
Rust (``gmeow_logic``) is the **whole logic engine**: the compiler
(``compile_logic`` — frontend, IR, the seven projections, the preservation
ledger) AND the reasoning authority (``materialize`` / ``certify`` / ``explain``
/ ``query`` / ``foundation`` / ``stable_models``).  Python keeps only:

* :mod:`gmeow_tools.logic_seam` (this module) — the Rust-fed dataclass
  containers built from the native ``gmeow_logic`` result dicts.

The Python compiler duplicate (the frontend / IR / adapter / projection
modules) was deleted in #727, the Python forward-chase oracle / certifier were
retired in #651, and the Python conformance runner (``logic_runner``) was retired
in #785 — the logic conformance gate is now the native Rust ``gmeow-conformance``
datatest harness (``crates/conformance``), which drives the same ``gmeow_logic``
cores directly and diffs against the committed goldens under cargo-nextest.
Parity with the Rust engine is guaranteed by content-addressed derivation-graph
goldens (#641) plus that native conformance suite, not by a Python
re-implementation.

What lives here
---------------
These are the pure IR/data the runner maps the native rows onto — they carry
no chase, join, skolemize, or certification logic, only the seam-contract
field shapes (LOGIC-RUNTIME.md §The seam data contract):

* :class:`DerivedQuad` — one materialized quad + full seam metadata;
* :class:`MaterializationResult` — the aggregate materialization output;
* :class:`BudgetParams` — declared runtime ceilings;
* :class:`LossEntry` — a construct narrowed during projection;
* :class:`ExplanationStep` / :class:`Explanation` — the cited-IRI derivation
  skeleton (the native ``gmeow_logic.explain`` surface);
* :class:`MaterializationError` — malformed-input signal;
* :data:`_ASSERT_RULE_IRI` — the ``logic:assert`` sentinel rule IRI stamped on
  asserted (input) facts.

Determinism note: all content-addressed IDs (``derivation_id``,
``source_quad_ids``) are minted by the Rust engine
(``gmeow_logic.materialize`` / ``mint_reifier`` / ``mint_derivation_id``); the
recipe is no longer reproduced in Python.
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum
from typing import NamedTuple

from gmeow_tools.config import PREFIXES

# --------------------------------------------------------------------------- #
# Preservation kind (the six logic:PreservationKind named individuals)
# --------------------------------------------------------------------------- #


class PreservationKind(StrEnum):
    """The six ``logic:PreservationKind`` named individuals (LOGIC-CONFORMANCE.md).

    Local names taken verbatim from ``slices/core/logic/module.ttl`` and mirrored
    by the Rust ``PreservationKind`` enum.  Lives here (the runtime seam) since the
    Python compiler IR was deleted in #727; it annotates :class:`LossEntry`.
    """

    EXACT = "ExactPreservation"
    SOUND_UNDER = "SoundUnderApproximation"
    COMPLETE_OVER = "CompleteOverApproximation"
    VALIDATION_ONLY = "ValidationOnly"
    INCONSISTENCY_PRESERVING = "InconsistencyPreserving"
    INCONSISTENCY_REFLECTING = "InconsistencyReflecting"


# --------------------------------------------------------------------------- #
# Namespace constants
# --------------------------------------------------------------------------- #

#: Logic namespace used to mint the asserted-fact sentinel rule IRI.
_LOGIC_NS = PREFIXES["logic"]

#: Sentinel rule IRI for asserted (input) facts.  Mirrors the Rust engine's
#: ``logic:assert`` stamp on every quad that came straight from the input
#: (``rule_iri == logic:assert`` ⇒ no antecedent rule fired).
_ASSERT_RULE_IRI = f"{_LOGIC_NS}assert"


# --------------------------------------------------------------------------- #
# Exceptions
# --------------------------------------------------------------------------- #


class MaterializationError(Exception):
    """Raised for malformed input that prevents materialization."""


# --------------------------------------------------------------------------- #
# Budget params (issue #502)
# --------------------------------------------------------------------------- #


@dataclass(frozen=True, slots=True)
class BudgetParams:
    """Declared runtime ceilings for the forward chase.

    A ceiling of ``None`` means *unbounded* for that dimension; ``None``
    everywhere (the default for every field) means the chase runs to full
    fixpoint with no governance whatsoever.

    The all-``None`` (unbounded) default is **deliberate, not an oversight**:
    it is a documented default-off posture chosen so that the existing #501
    materialisation corpus stays byte-identical.  A caller must *opt in* to
    governance by passing an explicit ceiling.

    Non-``None`` ceilings must be positive :class:`int` values (``bool``
    is rejected as a type error; zero or negative values are rejected as a
    value error).

    Determinism note
    ----------------
    :attr:`max_rule_firings` and :attr:`max_answers` are deterministic gates
    (they count discrete, reproducible events) and are the ceilings used in
    committed conformance fixtures.  :attr:`time_ms` is inherently
    nondeterministic (wall-clock dependent); it still produces a *sound* partial
    result on exhaustion, but per the gate-health doctrine it must NOT be used
    in committed conformance fixtures.

    Attributes:
        time_ms: Wall-clock ceiling in milliseconds, or ``None`` for unbounded.
        max_rule_firings: Maximum number of rule firings (derived quads), or
            ``None`` for unbounded.
        max_answers: Maximum number of derived answers (quads) to keep, or
            ``None`` for unbounded.
    """

    time_ms: int | None = None
    max_rule_firings: int | None = None
    max_answers: int | None = None

    def __post_init__(self) -> None:
        """Validate that each non-None ceiling is a positive int (not bool).

        ``None`` is always accepted (unbounded — the documented default-off
        posture).  A ``bool`` value is rejected via :exc:`TypeError` because
        ``bool`` is a subclass of ``int`` and ``True``/``False`` ceilings are
        nonsensical.  A zero or negative ceiling is rejected via
        :exc:`ValueError`.
        """
        for name, v in (
            ("time_ms", self.time_ms),
            ("max_rule_firings", self.max_rule_firings),
            ("max_answers", self.max_answers),
        ):
            if v is None:
                continue
            if isinstance(v, bool) or not isinstance(v, int):
                raise TypeError(
                    f"{name} must be an int or None, got {type(v).__name__}"
                )
            if v <= 0:
                raise ValueError(f"{name} must be a positive int or None, got {v}")

    def is_unbounded(self) -> bool:
        """Return True iff every ceiling is ``None`` (no governance at all)."""
        return (
            self.time_ms is None
            and self.max_rule_firings is None
            and self.max_answers is None
        )


# --------------------------------------------------------------------------- #
# Loss ledger
# --------------------------------------------------------------------------- #


@dataclass(frozen=True, slots=True)
class LossEntry:
    """A record of a construct narrowed during projection.

    Used to aggregate the projection loss ledger.

    Attributes:
        construct: The IRI or description of the construct that was narrowed.
        reason: Human-readable explanation of why it was narrowed.
        preservation_kind: The :class:`PreservationKind` that applies.
    """

    construct: str
    reason: str
    preservation_kind: PreservationKind


# --------------------------------------------------------------------------- #
# Materialization seam containers
# --------------------------------------------------------------------------- #


class DerivedQuad(NamedTuple):
    """One materialized quad with its full seam-contract metadata.

    All fields map directly to the seam data contract in LOGIC-RUNTIME.md
    §The seam data contract.

    Attributes:
        graph: IRI of the named-graph world this quad belongs to.
        subject: IRI string of the quad subject.
        predicate: IRI string of the quad predicate.
        obj: Canonical N3 representation of the quad object.
        graph_component: Same as ``graph`` (included for seam parity; future
            extensions may split graph vs. component IRI).
        derivation_id: Stable IRI for this derivation step.
        rule_iri: IRI of the rule that produced this quad (or the
            ``assert:`` sentinel for input facts).
        source_quad_ids: Reifier IRIs of the antecedent quads.
        profile: IRI of the semantic/decidability profile in force.
        budget_status: ``"ok"`` for a run that reached fixpoint within budget;
            ``"exhausted"`` for every quad emitted by a run that hit a
            :class:`BudgetParams` ceiling (issue #502).  Canonical spellings
            mirror the Rust ``BudgetStatus`` enum.
    """

    graph: str
    subject: str
    predicate: str
    obj: str
    graph_component: str
    derivation_id: str
    rule_iri: str
    source_quad_ids: list[str]
    profile: str
    budget_status: str


@dataclass(frozen=True, slots=True)
class MaterializationResult:
    """The result of a full forward-chase materialization.

    Attributes:
        quads: All materialized quads (input facts + derived facts) in
            deterministic order (world IRI, then S/P/O N3-lex order).
        worlds: The set of world IRIs present in the output.
        profile: The semantic profile IRI used for this run.
        loss_entries: Constructs narrowed during the chase.
        input_quad_count: Number of input (asserted) quads.
        derived_quad_count: Number of freshly derived quads (not in input).
        budget_status: The worst-of budget status across all worlds —
            ``"exhausted"`` if any world hit a :class:`BudgetParams` ceiling,
            else ``"ok"`` (issue #502, AC-B incompleteness marker).
        incomplete: ``True`` iff :attr:`budget_status` is ``"exhausted"`` — the
            explicit incompleteness marker required by AC-B.  When ``True`` the
            ``quads`` are a *sound subset* of the full fixpoint, never a false
            answer.
    """

    quads: tuple[DerivedQuad, ...]
    worlds: frozenset[str]
    profile: str
    loss_entries: tuple[LossEntry, ...]
    input_quad_count: int
    derived_quad_count: int
    budget_status: str
    incomplete: bool


# --------------------------------------------------------------------------- #
# Explanation IR (issue #497)
# --------------------------------------------------------------------------- #
#
# The explanation *engine* is native Rust (``gmeow_logic.explain``).  These two
# containers are the pure IR/data the runner maps the native rows onto.


class ExplanationStep(NamedTuple):
    """One node in the derivation tree, in the explanation skeleton.

    The conformance gate compares only the cited-IRI/rule-IRI skeleton, so the
    runner populates the subset of fields the native engine returns
    (``derivation_id``, ``rule_iri``, ``term_iris``); the remaining fields carry
    deterministic defaults so existing constructors keep working.

    Attributes:
        derivation_id: Stable IRI for this derivation step.
        rule_iri: IRI of the rule that produced this quad (or the assert
            sentinel for asserted facts).
        quad_reifier: Reifier IRI for the (S, P, O) triple.
        subject_iri: IRI string of the quad subject.
        predicate_iri: IRI string of the quad predicate.
        obj_n3: N3 representation of the quad object.
        graph_iri: World (named graph) IRI.
        term_iris: Sorted tuple of term IRIs cited at this step.
        source_step_ids: Derivation IDs of the antecedent steps.
        is_asserted: True if this quad was an input fact (rule_iri == assert).
        depth: Depth in the derivation tree (0 = the target quad).
    """

    derivation_id: str
    rule_iri: str
    quad_reifier: str = ""
    subject_iri: str = ""
    predicate_iri: str = ""
    obj_n3: str = ""
    graph_iri: str = ""
    term_iris: tuple[str, ...] = ()
    source_step_ids: tuple[str, ...] = ()
    is_asserted: bool = False
    depth: int = 0


class Explanation(NamedTuple):
    """The full explanation skeleton for a single derived (or asserted) quad.

    The native ``gmeow_logic.explain`` produces the cited-IRI skeleton; the
    runner maps each returned dict onto this container.

    Attributes:
        target_derivation_id: The derivation_id of the quad being explained.
        target_quad_reifier: The reifier IRI of the target quad's (S, P, O).
            This is the key the conformance runner matches explanations by.
        world_iri: The named graph (world) the target quad lives in.
        step_skeleton: Ordered :class:`ExplanationStep` sequence (DFS order).
        cited_iris: The complete cited-IRI set — the conformance surface.
        prose_lines: Retained for backwards-compatible construction only; the
            native engine never populates it (defaults to empty).
    """

    target_derivation_id: str
    target_quad_reifier: str
    world_iri: str
    step_skeleton: tuple[ExplanationStep, ...]
    cited_iris: frozenset[str]
    prose_lines: tuple[str, ...] = ()
