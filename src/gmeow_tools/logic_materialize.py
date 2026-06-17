# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
r"""Python oracle forward materializer for the Logic v1 monotonic core.

This module is **Principle 7's oracle**: a semi-naive Horn/Datalog forward
chase to fixpoint that defines the authoritative materialisation semantics.
The Rust engine (Task 4) must reproduce every IRI it produces byte-for-byte.
Get it RIGHT; everything downstream gates against it.

Design contract
---------------
* **World-indexed (CRITICAL).**  The chase runs per named-graph world.  Rules
  apply *within* a world; derived quads stay in that world.  No cross-world
  union or implicit merge.  Contested facts in different worlds coexist without
  collapse.
* **Determinism.**  Every derived quad carries the full seam contract (seam
  data contract, LOGIC-RUNTIME.md): ``graph``, ``(S, P, O, G)``,
  ``derivation_id``, ``rule_iri``, ``source_quad_ids``, ``profile``,
  ``budget_status``.  All IDs are content-addressed (SHA-1, sorted for order
  independence).
* **Blank-node Skolemization.**  Input blank nodes are Skolemized to
  deterministic IRIs *before* hashing or materializing so that derived facts
  contain only IRIs/literals.
* **No-occurrence gate (Stratum B).**  After the chase, every world is checked
  for token ``gufo:Event`` instances.  Any world that contains one raises
  :class:`NoOccurrenceViolationError` — the invariant is that the
  risk/teleology/norms fixture entails zero Event instances.
* **Loss-ledger hooks.**  Constructs narrowed during the chase are recorded as
  :class:`LossEntry` items for Task 5 to aggregate into the projection report.

Implementation notes
--------------------
*Input graph:*  an rdflib :class:`~rdflib.ConjunctiveGraph` (named
graphs = worlds).  N-Quads strings are accepted via :func:`parse_nquads`.

*IR rules:*  the ``body`` atoms use :class:`~.logic_ir.LogicAxiom` with
``subject``, ``predicate``, ``obj``, ``obj_is_literal`` fields.  For the
monotonic core (v1 PositiveHornProfile) we treat each axiom as a ground or
single-variable atom.  Variable atoms have a ``subject`` or ``obj`` starting
with ``?`` (Datalog convention).

*Reifier recipe:*  reuses :func:`~.statement_dsl.mint_reifier` exactly:
``sha1(s.n3() + " " + p.n3() + " " + o.n3()).hexdigest()`` under
``{NAMESPACE}reifier/``.

*Derivation ID recipe:*
``sha1(rule_iri + "\n" + "\n".join(sorted(source_reifier_iris))).hexdigest()``
under ``{NAMESPACE}derivation/``.  Sorted sources → order-independent.

*Term-canonicalization:*  ``.n3()`` on rdflib terms already produces the
canonical form required by the Rust mirror:
- IRI: ``<iri>``
- language literal: ``"lex"@lang`` (rdflib lowercases the lang subtag)
- typed literal: ``"lex"^^<dt>`` (xsd:string/rdf:langString elided by rdflib)
- No numeric normalization (lexical form preserved verbatim).
"""

from __future__ import annotations

import io
import logging
import time
from dataclasses import dataclass, field
from hashlib import sha1
from typing import NamedTuple

from rdflib import RDF, ConjunctiveGraph, Graph, Literal, URIRef
from rdflib.term import BNode, Node

from gmeow_tools.config import NAMESPACE, PREFIXES
from gmeow_tools.logic_ir import (
    LogicAxiom,
    LogicProgram,
    LogicRule,
    PreservationKind,
    SemanticProfileId,
)
from gmeow_tools.statement_dsl import QuotedTriple, mint_reifier

_log = logging.getLogger(__name__)

# --------------------------------------------------------------------------- #
# Namespace constants
# --------------------------------------------------------------------------- #

_GUFO_EVENT = URIRef("http://purl.org/nemo/gufo#Event")
_RDFS_SUB_CLASS_OF = URIRef("http://www.w3.org/2000/01/rdf-schema#subClassOf")
_DERIVATION_PREFIX = f"{NAMESPACE}derivation/"
_SKOLEM_PREFIX = f"{NAMESPACE}skolem/"

# Profile IRI used in output records — resolved from SemanticProfileId
_LOGIC_NS = PREFIXES["logic"]


# --------------------------------------------------------------------------- #
# Exceptions
# --------------------------------------------------------------------------- #


class NoOccurrenceViolationError(Exception):
    """Raised when a world contains a token typed as gufo:Event or a subclass.

    Stratum B invariant: the no-occurrence gate forbids token Event instances.
    Type-level use of gufo:Event (as a class) is permitted; only
    rdf:type assertions to gufo:Event (or a subclass) on a non-class individual
    are violations.
    """

    def __init__(self, world_iri: str, token_iri: str, event_type: str) -> None:
        """Initialize with world, token, and event-type information.

        Args:
            world_iri: The world IRI where the violation was detected.
            token_iri: The IRI of the token instance typed as an Event.
            event_type: The IRI of the gufo:Event subclass that was violated.
        """
        self.world_iri = world_iri
        self.token_iri = token_iri
        self.event_type = event_type
        super().__init__(
            f"No-occurrence gate violation in world <{world_iri}>: "
            f"token <{token_iri}> is typed as gufo:Event subclass <{event_type}>"
        )


class MaterializationError(Exception):
    """Raised for malformed input that prevents materialization."""


# --------------------------------------------------------------------------- #
# Supporting types
# --------------------------------------------------------------------------- #


# --------------------------------------------------------------------------- #
# Budget governor (issue #502, Task 3)
# --------------------------------------------------------------------------- #

#: Canonical budget-status spellings.  These MUST stay byte-identical to the
#: Rust ``BudgetStatus::as_str()`` mapping (crates/logic/src/seam.rs):
#: ``Ok -> "ok"``, ``Partial -> "partial"``, ``Exhausted -> "exhausted"``.
_BUDGET_OK = "ok"
_BUDGET_EXHAUSTED = "exhausted"


@dataclass(frozen=True, slots=True)
class BudgetParams:
    """Declared runtime ceilings for the forward chase.

    A ceiling of ``None`` means *unbounded* for that dimension; ``None``
    everywhere (the default for every field) means the chase runs to full
    fixpoint with no governance whatsoever.

    The all-``None`` (unbounded) default is **deliberate, not an oversight**:
    it is a documented default-off posture chosen so that the existing #501
    materialisation corpus stays byte-identical.  A caller must *opt in* to
    governance by passing an explicit ceiling; until then the oracle behaves
    exactly as it did before #502 (every quad ``budget_status="ok"``,
    :attr:`MaterializationResult.incomplete` ``False``).

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


@dataclass(slots=True)
class BudgetState:
    """Mutable per-run tracker enforcing the :class:`BudgetParams` ceilings.

    One instance is created per chase run and threaded through the round loop.
    The honesty invariant is *structural*: when a ceiling is hit the chase
    stops mid-loop and the already-emitted quads are tagged with
    :meth:`status_str`.  No quad is ever fabricated and no exception is raised
    on exhaustion — the kept set is always a sound subset of the full fixpoint.

    The ``"partial"`` Rust spelling is intentionally **not** produced here: the
    Python oracle has no mid-round partial-closure state, so a run is either
    fully within budget (``"ok"``) or it stopped early (``"exhausted"``).
    """

    params: BudgetParams
    firings: int = 0
    answers: int = 0
    exhausted: bool = False
    #: Set only when the WALL-CLOCK ceiling trips.  Unlike the deterministic
    #: count ceilings, a time cut must stop the chase immediately (runaway
    #: protection), so it is tracked separately from :attr:`exhausted`.
    time_exhausted: bool = False
    reason: str | None = None
    _start: float = field(default_factory=time.monotonic)

    def note_firing(self) -> None:
        """Record one rule firing; flag exhaustion if the firing ceiling is met.

        Called *after* a derived quad is appended.  Reaching the
        ``max_rule_firings`` ceiling marks the run exhausted (so the result is
        tagged incomplete and post-hoc truncated), but it **does not stop the
        chase**: the count ceilings let the chase reach full fixpoint and then
        truncate to a canonical-sort prefix of the *complete* derivation set.
        This makes the kept set a deterministic, **evaluation-order-independent**
        function of (program, input, budget) — the only contract under which the
        Python oracle and the Rust engine can keep the *same* truncated quad set
        (Principle 7).  Only :meth:`check_time` stops the chase early (runaway
        protection); a wall-clock cut is inherently nondeterministic and is never
        used in committed conformance fixtures.
        """
        self.firings += 1
        ceiling = self.params.max_rule_firings
        if ceiling is not None and self.firings >= ceiling and not self.exhausted:
            self.exhausted = True
            self.reason = f"max_rule_firings={ceiling} reached"

    def note_answers(self, count: int) -> None:
        """Record the current derived-answer count; flag exhaustion if exceeded.

        Like :meth:`note_firing`, reaching ``max_answers`` marks the run
        exhausted for the incompleteness tag + post-hoc truncation, but does not
        stop the chase — see that method for the engine-parity rationale.

        Args:
            count: The running number of derived answers (quads) emitted.
        """
        self.answers = count
        ceiling = self.params.max_answers
        if ceiling is not None and self.answers >= ceiling and not self.exhausted:
            self.exhausted = True
            self.reason = f"max_answers={ceiling} reached"

    def check_time(self) -> None:
        """Flag exhaustion if the elapsed wall-clock exceeds the time ceiling."""
        ceiling = self.params.time_ms
        if ceiling is None or self.time_exhausted:
            return
        elapsed_ms = (time.monotonic() - self._start) * 1000.0
        if elapsed_ms >= ceiling:
            self.time_exhausted = True
            self.exhausted = True
            self.reason = f"time_ms={ceiling} exceeded"

    def should_stop_chase(self) -> bool:
        """Return True iff the chase must stop NOW (wall-clock ceiling only).

        The deterministic count ceilings (``max_rule_firings`` / ``max_answers``)
        do **not** stop the chase — they let it reach fixpoint so the post-hoc
        truncation is a canonical prefix of the *complete* derivation set
        (engine-parity contract).  Only a wall-clock cut forces an early stop.
        """
        return self.time_exhausted

    def is_exhausted(self) -> bool:
        """Return True iff any ceiling has tripped.

        Accessor used in the chase loop guards so the exhaustion check is opaque
        to the type-narrower (the flag is mutated by :meth:`note_firing` etc.
        across method-call boundaries, which static narrowing of the bare
        attribute would otherwise miss).
        """
        return self.exhausted

    def status_str(self) -> str:
        """Return the canonical budget status for quads emitted by this run.

        Returns ``"exhausted"`` once any ceiling has tripped, otherwise
        ``"ok"``.  ``"partial"`` is never returned (see class docstring).
        """
        return _BUDGET_EXHAUSTED if self.exhausted else _BUDGET_OK


@dataclass(frozen=True, slots=True)
class LossEntry:
    """A record of a construct narrowed during the chase.

    Used by Task 5 to aggregate the projection loss ledger.

    Attributes:
        construct: The IRI or description of the construct that was narrowed.
        reason: Human-readable explanation of why it was narrowed.
        preservation_kind: The :class:`~.logic_ir.PreservationKind` that applies.
    """

    construct: str
    reason: str
    preservation_kind: PreservationKind


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
        loss_entries: Constructs narrowed during the chase (for Task 5).
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
# The explanation *engine* is native Rust (``gmeow_logic.explain``); the Python
# oracle module (``logic_explain.py``) is retired.  These two containers are the
# pure IR/data the runner maps the native rows onto — they live here alongside
# DerivedQuad/MaterializationResult (the other materialization IR), so any
# consumer can import them without depending on the retired module.


class ExplanationStep(NamedTuple):
    """One node in the derivation tree, in the explanation skeleton.

    Field-compatible with the retired ``logic_explain.ExplanationStep``.  The
    conformance gate compares only the cited-IRI/rule-IRI skeleton, so the
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

    Field-compatible with the retired ``logic_explain.Explanation`` (minus the
    ``as_markdown`` prose machinery, which the conformance gate never compared).
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


# --------------------------------------------------------------------------- #
# Skolemization
# --------------------------------------------------------------------------- #


def _skolem_iri(bnode: BNode) -> URIRef:
    """Deterministically Skolemize a blank node to a stable IRI.

    The hash covers the blank-node identifier string so that the same BNode
    (within a parse session) always maps to the same IRI.  Cross-session
    stability requires that the blank-node identifier itself is stable (which
    rdflib guarantees for parsed blank nodes using the source string).
    """
    digest = sha1(str(bnode).encode("utf-8")).hexdigest()
    return URIRef(f"{_SKOLEM_PREFIX}{digest}")


def _skolemize(term: Node) -> URIRef | Literal:
    """Skolemize a blank node to an IRI; pass through IRIs and Literals.

    Args:
        term: Any rdflib node.

    Returns:
        A :class:`~rdflib.URIRef` or :class:`~rdflib.Literal`.

    Raises:
        MaterializationError: If the term is not a URIRef, Literal, or BNode.
    """
    if isinstance(term, URIRef):
        return term
    if isinstance(term, Literal):
        return term
    if isinstance(term, BNode):
        return _skolem_iri(term)
    raise MaterializationError(f"Unexpected RDF term type: {type(term)!r} for {term!r}")


# --------------------------------------------------------------------------- #
# Content-addressing helpers
# --------------------------------------------------------------------------- #


def quad_reifier_iri(
    s: URIRef | Literal,
    p: URIRef | Literal,
    o: URIRef | Literal,
) -> str:
    """Return the reifier IRI for a quad's (S, P, O) using the canonical recipe.

    Reuses :func:`~.statement_dsl.mint_reifier` exactly — the same SHA-1 hash
    over N3-canonical ``"s.n3() p.n3() o.n3()"`` under ``{NAMESPACE}reifier/``.
    This is the single source of truth for the reifier recipe; the Rust mirror
    must reproduce it byte-for-byte.

    Args:
        s: Skolemized subject.
        p: Skolemized predicate (always URIRef in practice).
        o: Skolemized object (URIRef or Literal).

    Returns:
        The reifier IRI as a string.
    """
    # mint_reifier requires URIRef subject and predicate — both are guaranteed
    # by the caller (Skolemization strips BNodes; predicates are always URIs).
    if not isinstance(s, URIRef):
        raise MaterializationError(
            f"quad_reifier_iri: subject must be URIRef after Skolemization, got {s!r}"
        )
    if not isinstance(p, URIRef):
        raise MaterializationError(
            f"quad_reifier_iri: predicate must be URIRef, got {p!r}"
        )
    qt = QuotedTriple(subject=s, predicate=p, obj=o)
    return str(mint_reifier(qt))


def derivation_id_iri(rule_iri: str, source_reifier_iris: list[str]) -> str:
    r"""Compute the derivation IRI for a rule firing.

    The hash covers ``rule_iri + "\n" + "\n".join(sorted(source_reifier_iris))``
    so the result is order-independent w.r.t. the antecedent quads.  This is
    the canonical recipe the Rust engine must mirror byte-for-byte.

    Args:
        rule_iri: The IRI of the fired rule.
        source_reifier_iris: The reifier IRIs of the consumed antecedent quads.

    Returns:
        The derivation IRI as a string under ``{NAMESPACE}derivation/``.
    """
    payload = rule_iri + "\n" + "\n".join(sorted(source_reifier_iris))
    digest = sha1(payload.encode("utf-8")).hexdigest()
    return f"{_DERIVATION_PREFIX}{digest}"


# --------------------------------------------------------------------------- #
# Profile resolution
# --------------------------------------------------------------------------- #

#: Sentinel rule IRI for asserted (input) facts.
_ASSERT_RULE_IRI = f"{_LOGIC_NS}assert"


def _resolve_profile(program: LogicProgram) -> str:
    """Return the profile IRI declared in the program.

    The v1 oracle supports only :attr:`~.logic_ir.SemanticProfileId.POSITIVE_HORN`;
    anything else is recorded as a loss entry (the caller handles this).

    Args:
        program: The compiled logic program.

    Returns:
        The profile IRI string, or the PositiveHorn IRI if no profile declared.
    """
    if not program.profiles:
        return _LOGIC_NS + SemanticProfileId.POSITIVE_HORN
    # Use the first declared profile (canonical order from LogicProgram)
    return _LOGIC_NS + str(program.profiles[0].profile_id)


# --------------------------------------------------------------------------- #
# N-Quads input parser
# --------------------------------------------------------------------------- #


def parse_nquads(nquads_text: str) -> ConjunctiveGraph:
    """Parse an N-Quads string into a :class:`~rdflib.ConjunctiveGraph`.

    Empty or whitespace-only input returns an empty ConjunctiveGraph without
    raising (the empty-case oracle parity contract).

    Args:
        nquads_text: N-Quads encoded string.

    Returns:
        A ConjunctiveGraph with all parsed quads.

    Raises:
        MaterializationError: If the N-Quads text is malformed.
    """
    cg: ConjunctiveGraph = ConjunctiveGraph()
    stripped = nquads_text.strip()
    if not stripped:
        return cg
    try:
        cg.parse(io.StringIO(nquads_text), format="nquads")
    except Exception as exc:
        raise MaterializationError(f"Failed to parse N-Quads input: {exc}") from exc
    return cg


# --------------------------------------------------------------------------- #
# Datalog variable matching (positive Horn fragment only)
# --------------------------------------------------------------------------- #


def _is_var(term_str: str) -> bool:
    """Return True if the term string is a Datalog variable (starts with ``?``)."""
    return term_str.startswith("?")


def _match_atom(
    axiom_subj: str,
    axiom_pred: str,
    axiom_obj: str,
    axiom_obj_is_literal: bool,
    fact_s: URIRef | Literal,
    fact_p: URIRef | Literal,
    fact_o: URIRef | Literal,
) -> dict[str, URIRef | Literal] | None:
    """Try to match a rule body atom against a ground fact.

    Returns a variable-binding dict on success, or None on failure.  Only
    IRIs and literals are matched; after Skolemization there are no blank nodes
    in the fact base.

    Args:
        axiom_subj: Rule atom subject (IRI string or ``?var``).
        axiom_pred: Rule atom predicate (IRI string or ``?var``).
        axiom_obj: Rule atom object (IRI string, literal string, or ``?var``).
        axiom_obj_is_literal: Whether the axiom object is a literal.
        fact_s: Ground fact subject.
        fact_p: Ground fact predicate.
        fact_o: Ground fact object.

    Returns:
        Binding dict (possibly empty) mapping variable names to ground terms,
        or None if the atom does not match.
    """
    bindings: dict[str, URIRef | Literal] = {}

    # -- subject --
    if _is_var(axiom_subj):
        if not isinstance(fact_s, URIRef):
            return None  # subject must bind to an IRI
        bindings[axiom_subj] = fact_s
    else:
        if str(fact_s) != axiom_subj:
            return None

    # -- predicate --
    if _is_var(axiom_pred):
        if not isinstance(fact_p, URIRef):
            return None
        # Intra-atom repeated variable: if this var already bound (e.g. from
        # the subject slot), the new value must agree with the existing binding.
        if axiom_pred in bindings and bindings[axiom_pred] != fact_p:
            return None
        bindings[axiom_pred] = fact_p
    else:
        if str(fact_p) != axiom_pred:
            return None

    # -- object --
    if _is_var(axiom_obj):
        # Intra-atom repeated variable: if this var is already bound (e.g. ?x
        # appears in subject and object slots, as in `?x :p ?x`), the object
        # value must equal the existing binding; otherwise this fact does not
        # match the atom.
        if axiom_obj in bindings and bindings[axiom_obj] != fact_o:
            return None
        bindings[axiom_obj] = fact_o
    else:
        # Ground match: compare canonical string representation
        if axiom_obj_is_literal:
            # For literal atoms: match the string form of the literal
            if not isinstance(fact_o, Literal):
                return None
            if str(fact_o) != axiom_obj:
                return None
        else:
            if str(fact_o) != axiom_obj:
                return None

    return bindings


def _bindings_satisfy_distinct(
    distinct_pairs: tuple[tuple[str, str], ...],
    bindings: dict[str, URIRef | Literal],
) -> bool:
    """Return True iff every inequality guard holds for ``bindings`` (issue #503).

    Each pair ``(?A, ?B)`` is an inequality body guard: the rule fires only when
    ``?A`` and ``?B`` bind to *distinct* ground values.  Both variables MUST be
    bound by the positive body (a guard over an unbound variable is a malformed
    rule); an unbound member raises :class:`MaterializationError` rather than
    being silently treated as satisfied.

    Args:
        distinct_pairs: The rule's canonicalised inequality guards.
        bindings: The variable→ground-term binding assembled from the positive
            body join.

    Returns:
        ``True`` if every pair binds to two non-equal terms (so the head may be
        derived), ``False`` if any pair binds to equal terms (binding rejected).

    Raises:
        MaterializationError: If a guard variable is not bound by the body.
    """
    for var_a, var_b in distinct_pairs:
        if var_a not in bindings or var_b not in bindings:
            raise MaterializationError(
                f"Inequality guard variable {(var_a, var_b)!r} is unbound after "
                "body matching. Both variables of a logic:distinctBody guard "
                "must appear in a positive body atom."
            )
        # Compare canonical N3 forms so the inequality matches the engine's
        # term identity (same canonicalisation used throughout the chase).
        if bindings[var_a].n3() == bindings[var_b].n3():
            return False
    return True


def _merge_bindings(
    b1: dict[str, URIRef | Literal],
    b2: dict[str, URIRef | Literal],
) -> dict[str, URIRef | Literal] | None:
    """Merge two binding dicts; return None if they conflict."""
    merged = dict(b1)
    for var, val in b2.items():
        if var in merged:
            if merged[var] != val:
                return None
        else:
            merged[var] = val
    return merged


def _apply_bindings(
    term_str: str,
    is_literal: bool,
    bindings: dict[str, URIRef | Literal],
) -> URIRef | Literal:
    """Apply variable bindings to a term string.

    Args:
        term_str: IRI string, literal string, or ``?var``.
        is_literal: Whether the term is a literal in the IR.
        bindings: Variable bindings from body matching.

    Returns:
        The ground URIRef or Literal.

    Raises:
        MaterializationError: If a variable in the head is unbound.
    """
    if _is_var(term_str):
        if term_str not in bindings:
            raise MaterializationError(
                f"Head variable {term_str!r} is unbound after body matching. "
                "Check that all head variables appear in the rule body."
            )
        return bindings[term_str]
    if is_literal:
        return Literal(term_str)
    return URIRef(term_str)


# --------------------------------------------------------------------------- #
# World extraction from ConjunctiveGraph
# --------------------------------------------------------------------------- #


def _extract_worlds(
    cg: ConjunctiveGraph,
) -> dict[str, list[tuple[URIRef | Literal, URIRef | Literal, URIRef | Literal]]]:
    """Extract per-world Skolemized fact lists from the ConjunctiveGraph.

    Args:
        cg: The input ConjunctiveGraph (named graphs = worlds).

    Returns:
        A dict mapping world IRI string → list of Skolemized (S, P, O) tuples.
    """
    worlds: dict[
        str, list[tuple[URIRef | Literal, URIRef | Literal, URIRef | Literal]]
    ] = {}
    for ctx in cg.contexts():
        graph_id = ctx.identifier
        if not isinstance(graph_id, URIRef):
            # Skip default graph (BNode identifier) — worlds must be named
            continue
        world_iri = str(graph_id)
        facts: list[tuple[URIRef | Literal, URIRef | Literal, URIRef | Literal]] = []
        for s, p, o in ctx:
            sk_s = _skolemize(s)
            sk_p = _skolemize(p)
            sk_o = _skolemize(o)
            facts.append((sk_s, sk_p, sk_o))
        worlds[world_iri] = facts
    return worlds


# --------------------------------------------------------------------------- #
# No-occurrence gate
# --------------------------------------------------------------------------- #


def _collect_event_subclasses(onto_graph: Graph) -> frozenset[URIRef]:
    """Collect all classes that are rdfs:subClassOf* gufo:Event.

    Uses a simple BFS/iterative expansion over the rdfs:subClassOf relation
    present in the given graph.

    Args:
        onto_graph: An rdflib Graph containing rdfs:subClassOf triples.

    Returns:
        Frozenset of URIRefs that are subclasses of gufo:Event (including itself).
    """
    event_classes: set[URIRef] = {_GUFO_EVENT}
    frontier: set[URIRef] = {_GUFO_EVENT}
    while frontier:
        new_frontier: set[URIRef] = set()
        for cls in frontier:
            for sub, _, _ in onto_graph.triples((None, _RDFS_SUB_CLASS_OF, cls)):
                if isinstance(sub, URIRef) and sub not in event_classes:
                    event_classes.add(sub)
                    new_frontier.add(sub)
        frontier = new_frontier
    return frozenset(event_classes)


def _assert_no_occurrence(
    world_iri: str,
    facts: list[tuple[URIRef | Literal, URIRef | Literal, URIRef | Literal]],
    event_subclasses: frozenset[URIRef],
) -> None:
    """Raise NoOccurrenceViolationError if any token is typed as gufo:Event/subclass.

    A *token* is a subject that carries an rdf:type assertion to an Event class.
    Type-level references (where the subject IS a class) are allowed.

    Args:
        world_iri: The world IRI for error messages.
        facts: The Skolemized (S, P, O) facts in this world (input + derived).
        event_subclasses: The set of gufo:Event subclasses to check against.

    Raises:
        NoOccurrenceViolationError: If any fact typed a token as gufo:Event/subclass.
    """
    rdf_type = URIRef(str(RDF.type))
    for s, p, o in facts:
        if str(p) != str(rdf_type):
            continue
        if not isinstance(o, URIRef):
            continue
        if o not in event_subclasses:
            continue
        # s is typed as an Event class; s itself must NOT be a class
        # (type-level usage: s rdfs:subClassOf gufo:Event is ok;
        #  but s rdf:type gufo:Event where s is an instance is forbidden)
        # We treat all URIRefs as potential tokens; OWL-style punning would
        # require an onto-graph, which we don't have here. The gate checks for
        # the pattern the runtime enforces: any rdf:type → gufo:Event triple.
        if isinstance(s, URIRef):
            raise NoOccurrenceViolationError(
                world_iri=world_iri,
                token_iri=str(s),
                event_type=str(o),
            )


# --------------------------------------------------------------------------- #
# Semi-naive forward chase
# --------------------------------------------------------------------------- #


def _build_asserted_quad(
    world_iri: str,
    s: URIRef | Literal,
    p: URIRef | Literal,
    o: URIRef | Literal,
    profile_iri: str,
    budget_status: str,
) -> DerivedQuad:
    """Build a DerivedQuad record for an asserted (input) fact.

    The derivation_id for an asserted fact uses the assert sentinel rule IRI
    with the single source reifier (the quad itself hashes as its own source).

    Args:
        world_iri: The world this fact belongs to.
        s: Skolemized subject.
        p: Skolemized predicate.
        o: Skolemized object.
        profile_iri: The profile IRI for this run.
        budget_status: Canonical budget status to stamp on the quad.

    Returns:
        A :class:`DerivedQuad` with ``rule_iri = logic:assert``.
    """
    if not isinstance(s, URIRef):
        raise MaterializationError(
            f"Asserted quad subject must be URIRef after Skolemization, got {s!r}"
        )
    if not isinstance(p, URIRef):
        raise MaterializationError(f"Asserted quad predicate must be URIRef, got {p!r}")
    reifier = quad_reifier_iri(s, p, o)
    deriv_id = derivation_id_iri(_ASSERT_RULE_IRI, [reifier])
    return DerivedQuad(
        graph=world_iri,
        subject=str(s),
        predicate=str(p),
        obj=o.n3(),
        graph_component=world_iri,
        derivation_id=deriv_id,
        rule_iri=_ASSERT_RULE_IRI,
        source_quad_ids=[reifier],
        profile=profile_iri,
        budget_status=budget_status,
    )


def _stratified_rule_groups(
    program: LogicProgram, enable_naf: bool
) -> tuple[tuple[LogicRule, ...], ...]:
    """Partition ``program.rules`` into ordered strata for the chase (issue #503).

    With ``enable_naf=False`` the result is a single stratum equal to
    ``program.rules`` (so the chase is byte-identical to the pre-#503
    single-fixpoint behaviour — Corpus-safety).

    With ``enable_naf=True`` the rules are layered using the same predicate
    dependency-graph stratification the certifier uses
    (:func:`gmeow_tools.logic_certify.stratify`): each rule is assigned to the
    stratum of its head predicate key, and strata are emitted low-to-high.  Chasing
    a lower stratum to fixpoint before a higher one guarantees a ``logic:negatedBody``
    atom is only checked once the predicate it negates is settled (sound stratified
    NAF).  A rule whose head predicate key is absent from the layering (no
    inter-rule dependency) is placed in the lowest stratum.

    Args:
        program: The compiled logic program.
        enable_naf: Whether NAF (and hence stratification) is active.

    Returns:
        A tuple of strata, each a tuple of :class:`~.logic_ir.LogicRule`, ordered
        low-to-high.
    """
    if not enable_naf:
        return (tuple(program.rules),)

    # Local import avoids a module-level cycle (logic_certify imports nothing from
    # this module, but keeping the import local mirrors the other deferred imports).
    from gmeow_tools.logic_certify import (
        PredicateDepGraph,
        _predicate_key,
        stratify,
    )

    graph = PredicateDepGraph.from_program(program)
    result = stratify(graph)
    if not result.is_stratified:
        # A non-stratifiable program has no perfect model; the foundation rules are
        # certified stratified, so this only fires on a malformed augmented program.
        raise MaterializationError(
            "NAF chase requested for a non-stratifiable rule set "
            f"(offending cycle {result.offending_cycle}). Stratified NAF requires "
            "negation to cross only stratum boundaries, never a recursive cycle."
        )

    # Map each predicate key to its stratum index (low-to-high).
    key_to_stratum: dict[str, int] = {}
    for idx, layer in enumerate(result.strata):
        for key in layer:
            key_to_stratum[key] = idx

    buckets: dict[int, list[LogicRule]] = {idx: [] for idx in range(len(result.strata))}
    # Rules with no inter-rule dependency edge never enter the dep graph layering;
    # park them in stratum 0 (they are positive base rules).
    fallback = 0
    for rule in program.rules:
        stratum = key_to_stratum.get(_predicate_key(rule.head), fallback)
        buckets.setdefault(stratum, []).append(rule)

    return tuple(tuple(buckets[idx]) for idx in sorted(buckets))


def _chase_world(
    world_iri: str,
    initial_facts: list[tuple[URIRef | Literal, URIRef | Literal, URIRef | Literal]],
    program: LogicProgram,
    profile_iri: str,
    budget: BudgetParams | None = None,
    enable_naf: bool = False,
) -> tuple[
    list[DerivedQuad],
    list[tuple[URIRef | Literal, URIRef | Literal, URIRef | Literal]],
    list[LossEntry],
    BudgetState,
]:
    """Run the forward chase in one world to fixpoint (or until budget exhausts).

    Implements semi-naive evaluation: we track the 'delta' (newly derived facts)
    and in each round only attempt to fire rules where at least one body atom
    matches a delta fact.  For the v1 monotonic Horn profile this terminates
    when no new facts can be derived.

    Budget governance (issue #502)
    ------------------------------
    When ``budget`` declares a ceiling, a :class:`BudgetState` enforces it.  On
    exhaustion the chase STOPS mid-loop and breaks out of the round loop — it
    never raises and never fabricates a quad.  Every quad already emitted is
    tagged with the run's :meth:`BudgetState.status_str`, so the kept set is a
    sound subset of the full fixpoint.  Under a ``max_answers`` cap the derived
    quads are deterministically truncated to the canonical-sort prefix so the
    cap is honoured exactly and reproducibly.

    With ``budget=None`` (or an all-unbounded :class:`BudgetParams`) the run is
    byte-identical to the pre-#502 behaviour: every quad ``budget_status="ok"``.

    Args:
        world_iri: The world IRI (for provenance records).
        initial_facts: The Skolemized asserted (S, P, O) facts for this world.
        program: The compiled logic program (provides rules).
        profile_iri: The profile IRI for seam metadata.
        budget: Optional runtime ceilings; ``None`` means unbounded.
        enable_naf: When True (issue #503), evaluate ``logic:negatedBody`` atoms as
            stratified negation-as-failure: the rules are partitioned into ordered
            strata (:func:`_stratified_rule_groups`) and each stratum is chased to
            fixpoint before the next, so a negated atom is only checked once its
            stratum is settled.  When False (the byte-stable default for every
            pre-#503 case) the whole rule set is a single positive stratum and
            negated atoms are treated as ordinary positive joins — the
            materialisation is byte-identical to the prior behaviour.

    Returns:
        A 4-tuple:
        - list of all DerivedQuad records (asserted + derived),
        - list of all (S, P, O) facts after closure (for the no-occurrence gate),
        - list of LossEntry records for non-Horn constructs,
        - the :class:`BudgetState` for this run (carries exhaustion status).
    """
    budget_state = BudgetState(params=budget if budget is not None else BudgetParams())
    loss_entries: list[LossEntry] = []

    # Warn on non-POSITIVE_HORN profile (loss in v1)
    for prof in program.profiles:
        if prof.profile_id != SemanticProfileId.POSITIVE_HORN:
            loss_entries.append(
                LossEntry(
                    construct=f"{_LOGIC_NS}{prof.profile_id}",
                    reason=(
                        f"v1 oracle supports only PositiveHornProfile; "
                        f"{prof.profile_id} semantics not applied"
                    ),
                    preservation_kind=PreservationKind.SOUND_UNDER,
                )
            )

    # Indexed fact store: (s_str, p_str, o_str) → (sk_s, sk_p, sk_o)
    # The string key is the canonical identity for deduplication.
    fact_index: dict[
        tuple[str, str, str],
        tuple[URIRef | Literal, URIRef | Literal, URIRef | Literal],
    ] = {}
    for s, p, o in initial_facts:
        key = (s.n3(), p.n3(), o.n3())
        fact_index[key] = (s, p, o)

    # Build asserted DerivedQuad records
    all_quads: list[DerivedQuad] = []
    for s, p, o in initial_facts:
        if isinstance(s, URIRef) and isinstance(p, URIRef):
            all_quads.append(
                _build_asserted_quad(
                    world_iri, s, p, o, profile_iri, budget_state.status_str()
                )
            )
        else:
            # Subjects that are still non-URI after Skolemization (shouldn't
            # happen, but hard-fail rather than silently skip)
            raise MaterializationError(
                f"Non-URI subject {s!r} in world {world_iri!r} after Skolemization"
            )

    # Semi-naive: delta starts as all initial facts
    delta: list[tuple[URIRef | Literal, URIRef | Literal, URIRef | Literal]] = list(
        initial_facts
    )

    # Collect derived DerivedQuad records separately (appended after input)
    derived_quads: list[DerivedQuad] = []

    # Stratified evaluation (issue #503): when enable_naf is True the rule set is
    # split into ordered strata so a negated atom is only checked once its stratum
    # is at fixpoint.  When False the whole set is ONE stratum — byte-identical to
    # the pre-#503 single-fixpoint chase.  Each stratum is chased to fixpoint and
    # then the delta is reset to ALL current facts so the next stratum re-derives
    # against everything settled below it.
    rule_strata = _stratified_rule_groups(program, enable_naf)

    for stratum_rules in rule_strata:
        # Corpus-safety (issue #503): the non-NAF path has exactly one stratum
        # equal to ``program.rules``, so this outer loop runs once and the inner
        # chase is byte-identical to the prior behaviour.
        delta = list(fact_index.values())

        # We iterate rules: for each rule, try to join body atoms against current
        # facts (using the delta for the semi-naive optimization).
        for _round in range(10_000):  # hard cap (should terminate much sooner)
            # Budget gate: re-check wall-clock at the top of every round.  Only a
            # WALL-CLOCK cut stops the chase early (runaway protection); the
            # deterministic count ceilings let the chase reach fixpoint so the
            # post-hoc truncation is a canonical prefix of the COMPLETE derivation
            # set — the evaluation-order-independent contract required for oracle ≡
            # engine parity (Principle 7).
            budget_state.check_time()
            if budget_state.should_stop_chase():
                break

            new_delta: list[
                tuple[URIRef | Literal, URIRef | Literal, URIRef | Literal]
            ] = []

            for rule in stratum_rules:
                if budget_state.should_stop_chase():
                    break
                if not rule.body:
                    # Zero-body rules (facts-as-rules): emit head unconditionally
                    head = rule.head
                    head_s = _apply_bindings(head.subject, False, {})
                    head_p = _apply_bindings(head.predicate, False, {})
                    head_o = _apply_bindings(head.obj, head.obj_is_literal, {})
                    if not isinstance(head_s, URIRef) or not isinstance(head_p, URIRef):
                        continue
                    key = (head_s.n3(), head_p.n3(), head_o.n3())
                    if key in fact_index:
                        continue
                    rule_iri = str(
                        rule.scope.provenance or f"{_LOGIC_NS}rule/anonymous"
                    )
                    deriv_id = derivation_id_iri(rule_iri, [])
                    fact_index[key] = (head_s, head_p, head_o)
                    new_delta.append((head_s, head_p, head_o))
                    derived_quads.append(
                        DerivedQuad(
                            graph=world_iri,
                            subject=str(head_s),
                            predicate=str(head_p),
                            obj=head_o.n3(),
                            graph_component=world_iri,
                            derivation_id=deriv_id,
                            rule_iri=rule_iri,
                            source_quad_ids=[],
                            profile=profile_iri,
                            budget_status=budget_state.status_str(),
                        )
                    )
                    budget_state.note_firing()
                    budget_state.note_answers(len(derived_quads))
                    budget_state.check_time()
                    if budget_state.should_stop_chase():
                        break
                    continue

                # For rules with body: join all atoms against the current fact
                # base.  Semi-naive: at least one body atom must match a delta fact.
                rule_iri = str(rule.scope.provenance or f"{_LOGIC_NS}rule/anonymous")

                # Enumerate all binding combinations via recursive join
                # (for v1 we have simple Datalog; body size is small).  Under
                # enable_naf the join evaluates negated body atoms as NAF literals;
                # otherwise they are treated positively (byte-stable default).
                binding_sets = _join_body_atoms(
                    rule.body,
                    fact_index,
                    delta,
                    enable_naf,
                )

                for bindings, source_keys in binding_sets:
                    # Inequality body guards (issue #503): reject any candidate
                    # binding where a distinct pair resolves both variables to the
                    # same value.  The head is derived only for bindings where every
                    # guard is genuinely unequal.  An unbound guard variable is a
                    # malformed rule and raises (handled defensively, consistent with
                    # the head-grounding error path below).  Rules with no guard
                    # (``distinct_pairs == ()`` — every pre-#503 rule) never enter
                    # this branch and are byte-identical to the prior chase.
                    if rule.distinct_pairs and not _bindings_satisfy_distinct(
                        rule.distinct_pairs, bindings
                    ):
                        continue

                    # Ground the head
                    try:
                        head_s = _apply_bindings(rule.head.subject, False, bindings)
                        head_p = _apply_bindings(rule.head.predicate, False, bindings)
                        head_o = _apply_bindings(
                            rule.head.obj, rule.head.obj_is_literal, bindings
                        )
                    except MaterializationError:
                        # Unbound head variable — record as loss and skip
                        loss_entries.append(
                            LossEntry(
                                construct=rule_iri,
                                reason="Head variable unbound after body matching",
                                preservation_kind=PreservationKind.SOUND_UNDER,
                            )
                        )
                        continue

                    if not isinstance(head_s, URIRef) or not isinstance(head_p, URIRef):
                        # Non-IRI head subject/predicate — skip (Datalog constraint)
                        continue

                    key = (head_s.n3(), head_p.n3(), head_o.n3())
                    if key in fact_index:
                        continue

                    # Compute provenance
                    source_reifiers = [
                        quad_reifier_iri(
                            fact_index[sk_key][0]
                            if isinstance(fact_index[sk_key][0], URIRef)
                            else URIRef(str(fact_index[sk_key][0])),
                            fact_index[sk_key][1]
                            if isinstance(fact_index[sk_key][1], URIRef)
                            else URIRef(str(fact_index[sk_key][1])),
                            fact_index[sk_key][2],
                        )
                        for sk_key in source_keys
                        if isinstance(fact_index[sk_key][0], URIRef)
                        and isinstance(fact_index[sk_key][1], URIRef)
                    ]
                    deriv_id = derivation_id_iri(rule_iri, source_reifiers)

                    fact_index[key] = (head_s, head_p, head_o)
                    new_delta.append((head_s, head_p, head_o))
                    derived_quads.append(
                        DerivedQuad(
                            graph=world_iri,
                            subject=str(head_s),
                            predicate=str(head_p),
                            obj=head_o.n3(),
                            graph_component=world_iri,
                            derivation_id=deriv_id,
                            rule_iri=rule_iri,
                            source_quad_ids=source_reifiers,
                            profile=profile_iri,
                            budget_status=budget_state.status_str(),
                        )
                    )
                    budget_state.note_firing()
                    budget_state.note_answers(len(derived_quads))
                    budget_state.check_time()
                    if budget_state.should_stop_chase():
                        break

            if budget_state.should_stop_chase():
                break  # wall-clock cut — stop the chase (sound partial result)
            if not new_delta:
                break  # fixpoint reached
            delta = new_delta
        else:
            # Iteration cap hit — should not happen for finite positive programs
            raise MaterializationError(
                f"Chase did not reach fixpoint in world {world_iri!r} after "
                "10,000 rounds. Check for non-terminating rules."
            )

        if budget_state.should_stop_chase():
            break  # propagate a wall-clock cut out of the stratum loop too

    # Deterministic truncation under the DERIVED-quad count ceilings.  Because
    # the chase ran to FULL fixpoint (only a wall-clock cut stops it early), the
    # derived set here is COMPLETE, so truncating it to the canonical-sort PREFIX
    # yields an evaluation-order-independent kept set: the SAME quads the Rust
    # engine keeps (it likewise runs to fixpoint then truncates), giving oracle ≡
    # engine parity (Principle 7).  Both ``max_rule_firings`` and ``max_answers``
    # bound the count of DERIVED (IDB) quads; the effective cap is the minimum of
    # the declared ceilings.  Asserted EDB facts live in ``all_quads`` and are
    # never truncated by a derivation budget.  The canonical key
    # ``(graph, subject, predicate, obj)`` is byte-identical to the engine's
    # ``budget_sort_key`` (within this fixed world ``graph`` is constant, so the
    # prefix is stable either way).
    derived_caps = [
        c
        for c in (
            budget_state.params.max_rule_firings,
            budget_state.params.max_answers,
        )
        if c is not None
    ]
    if derived_caps:
        derived_cap = min(derived_caps)
        if len(derived_quads) > derived_cap:
            derived_quads.sort(key=lambda q: (q.graph, q.subject, q.predicate, q.obj))
            derived_quads = derived_quads[:derived_cap]
            # The full fixpoint exceeded the cap ⇒ exhausted, regardless of which
            # ceiling tripped during the chase (note_firing/note_answers already
            # set it, but a max_answers cap smaller than a single round's output
            # is reasserted here for the rebuilt-to-fixpoint path).
            if not budget_state.exhausted:
                budget_state.exhausted = True
                budget_state.reason = f"derived cap={derived_cap} exceeded at fixpoint"

    # Final status stamp: once a run is exhausted EVERY quad it emitted
    # (asserted + derived) carries "exhausted", so the result is unambiguously
    # marked incomplete.  When not exhausted this is a no-op ("ok" everywhere).
    final_status = budget_state.status_str()
    if budget_state.is_exhausted():
        all_quads = [q._replace(budget_status=final_status) for q in all_quads]
        derived_quads = [q._replace(budget_status=final_status) for q in derived_quads]

    all_quads.extend(derived_quads)
    all_facts = list(fact_index.values())
    return all_quads, all_facts, loss_entries, budget_state


def _ground_negated_atom_key(
    atom: LogicAxiom,
    bindings: dict[str, URIRef | Literal],
) -> tuple[str, str, str] | None:
    """Ground a negated atom to a canonical ``fact_index`` key, or ``None``.

    Returns the ``(s, p, o)`` n3-string key when *every* term grounds to either a
    bound variable's value or a constant IRI — enabling an O(1) membership test in
    :func:`_atom_is_satisfied` instead of a full scan.  Returns ``None`` (so the
    caller falls back to the scan) when any term is an unbound variable (a
    non-DL-safe atom) or a constant *literal* object: :func:`_match_atom` compares
    literal constants by string form, not by the datatyped ``.n3()`` the key uses,
    so a literal-constant lookup is not exact and must scan.

    Bound-variable values originate from the fact base, so their ``.n3()`` is
    exactly the keyed form; constant IRIs key canonically as ``<iri>``.  This makes
    the fast path byte-equivalent to the scan for every atom it accepts.
    """

    def _ground(term: str, is_literal: bool) -> str | None:
        if _is_var(term):
            val = bindings.get(term)
            return None if val is None else val.n3()
        if is_literal:
            return None
        return URIRef(term).n3()

    s = _ground(atom.subject, False)
    if s is None:
        return None
    p = _ground(atom.predicate, False)
    if p is None:
        return None
    o = _ground(atom.obj, atom.obj_is_literal)
    if o is None:
        return None
    return (s, p, o)


def _atom_is_satisfied(
    atom: LogicAxiom,
    bindings: dict[str, URIRef | Literal],
    fact_index: dict[
        tuple[str, str, str],
        tuple[URIRef | Literal, URIRef | Literal, URIRef | Literal],
    ],
) -> bool:
    """Whether a (negated) atom has at least one match in ``fact_index`` (issue #503).

    Used for negation-as-failure: the negated atom ``NOT p(?x, ?y)`` is *satisfied as
    a negative literal* when this returns ``False`` (no matching fact).  The atom is
    grounded by the current positive-join ``bindings`` first, then matched against the
    fact base.  Stratified evaluation (see :func:`_chase_world`) guarantees the
    negated predicate's stratum is already at fixpoint, so a missing match is a sound
    "fails", never a premature one.

    Args:
        atom: The negated body :class:`~.logic_ir.LogicAxiom`.
        bindings: The variable bindings assembled by the positive body join.
        fact_index: The current world fact store.

    Returns:
        ``True`` iff some fact matches the bound atom (so NAF would block the rule).
    """
    # Fast path (issue #503 review): when the atom grounds fully to bound /
    # constant-IRI terms, an O(1) key-membership test replaces the O(N) scan over
    # every candidate binding.  Falls through to the scan for partially-bound
    # (non-DL-safe) atoms or constant-literal objects — see
    # :func:`_ground_negated_atom_key`.
    ground_key = _ground_negated_atom_key(atom, bindings)
    if ground_key is not None:
        return ground_key in fact_index

    atom_s = bindings.get(atom.subject)
    atom_p = bindings.get(atom.predicate)
    atom_o = bindings.get(atom.obj)
    for (fs, fp, fo), (sk_s, sk_p, sk_o) in fact_index.items():
        if atom_s is not None and fs != atom_s.n3():
            continue
        if atom_p is not None and fp != atom_p.n3():
            continue
        if atom_o is not None and fo != atom_o.n3():
            continue
        if (
            _match_atom(
                atom.subject,
                atom.predicate,
                atom.obj,
                atom.obj_is_literal,
                sk_s,
                sk_p,
                sk_o,
            )
            is not None
        ):
            return True
    return False


def _join_body_atoms(
    body: tuple[object, ...],
    fact_index: dict[
        tuple[str, str, str],
        tuple[URIRef | Literal, URIRef | Literal, URIRef | Literal],
    ],
    delta: list[tuple[URIRef | Literal, URIRef | Literal, URIRef | Literal]],
    enable_naf: bool = False,
) -> list[tuple[dict[str, URIRef | Literal], list[tuple[str, str, str]]]]:
    """Join all body atoms against the fact base (semi-naive).

    For v1 positive Horn: all atoms are positive; the join is a nested-loop
    over the fact base.  At least one atom must match a delta fact (the
    semi-naive condition).

    Negation-as-failure (issue #503, ``enable_naf=True``)
    ----------------------------------------------------
    When ``enable_naf`` is True the body is split into *positive* atoms (joined as
    usual, contributing the semi-naive delta condition + provenance) and *negated*
    atoms (``atom.negated``), which are NOT joined and contribute no source quad.
    After the positive join, every candidate binding is dropped if any of its
    grounded negated atoms still matches a fact (:func:`_atom_is_satisfied`).  This
    is sound only under stratified evaluation — the caller chases lower strata to
    fixpoint before any rule whose negated atom references them fires.

    With ``enable_naf=False`` (the default — every pre-#503 call) negated atoms are
    treated as ordinary positive joins, exactly as before, so the materialisation of
    the existing corpus is byte-identical.

    Args:
        body: The body axioms (LogicAxiom instances from the rule).
        fact_index: The current world fact store.
        delta: Newly derived facts from the previous round.
        enable_naf: When True, evaluate ``atom.negated`` body atoms as
            negation-as-failure literals; when False, treat them positively
            (the byte-stable pre-#503 default).

    Returns:
        A list of (bindings, source_keys) pairs — one per full join result.
        ``source_keys`` are the fact_index keys of the matched facts.
    """
    delta_set: set[tuple[str, str, str]] = {
        (s.n3(), p.n3(), o.n3()) for s, p, o in delta
    }

    # Under NAF the join walks only positive atoms; negated atoms are applied as a
    # post-join filter.  Without NAF the whole body is joined (byte-stable default).
    if enable_naf:
        positive_body: tuple[object, ...] = tuple(
            a for a in body if not (isinstance(a, LogicAxiom) and a.negated)
        )
        negated_body: tuple[LogicAxiom, ...] = tuple(
            a for a in body if isinstance(a, LogicAxiom) and a.negated
        )
    else:
        positive_body = body
        negated_body = ()

    # Start with a single empty binding + no sources
    solutions: list[tuple[dict[str, URIRef | Literal], list[tuple[str, str, str]]]] = [
        ({}, [])
    ]

    for atom in positive_body:
        if not isinstance(atom, LogicAxiom):
            raise MaterializationError(f"Body element is not a LogicAxiom: {atom!r}")
        next_solutions: list[
            tuple[dict[str, URIRef | Literal], list[tuple[str, str, str]]]
        ] = []
        for bindings, sources in solutions:
            # Ground the atom's subject/predicate/object using current bindings
            atom_s = bindings.get(atom.subject, None)
            atom_p = bindings.get(atom.predicate, None)
            atom_o = bindings.get(atom.obj, None)

            for (fs, fp, fo), (sk_s, sk_p, sk_o) in fact_index.items():
                # Quick filter for bound terms
                if atom_s is not None and fs != atom_s.n3():
                    continue
                if atom_p is not None and fp != atom_p.n3():
                    continue
                if atom_o is not None and fo != atom_o.n3():
                    continue
                m = _match_atom(
                    atom.subject,
                    atom.predicate,
                    atom.obj,
                    atom.obj_is_literal,
                    sk_s,
                    sk_p,
                    sk_o,
                )
                if m is None:
                    continue
                merged = _merge_bindings(bindings, m)
                if merged is None:
                    continue
                next_solutions.append((merged, [*sources, (fs, fp, fo)]))

        solutions = next_solutions
        if not solutions:
            break

    # Negation-as-failure filter (issue #503): drop any binding whose grounded
    # negated atoms still match a fact.  Only applied when enable_naf is True; the
    # negated_body tuple is empty otherwise, so this is a no-op for the byte-stable
    # default path.
    if negated_body:
        solutions = [
            (bindings, sources)
            for bindings, sources in solutions
            if not any(
                _atom_is_satisfied(neg, bindings, fact_index) for neg in negated_body
            )
        ]

    # Semi-naive filter: at least one source key must be in the delta.  A rule with
    # an all-negated body (no positive atom) has no source key and so could never
    # pass this filter; such rules are malformed under DL-safety (every variable must
    # be positively bound), so they are correctly never fired here.
    return [
        (bindings, sources)
        for bindings, sources in solutions
        if any(sk in delta_set for sk in sources)
    ]


# --------------------------------------------------------------------------- #
# Public API
# --------------------------------------------------------------------------- #


def materialize_program(
    program: LogicProgram,
    input_graph: ConjunctiveGraph,
    profile: SemanticProfileId = SemanticProfileId.POSITIVE_HORN,
    budget: BudgetParams | None = None,
    enable_naf: bool = False,
) -> MaterializationResult:
    """Run the forward Horn chase to fixpoint over the input ConjunctiveGraph.

    This is the **Python oracle** — the authoritative executable spec that the
    Rust engine (Task 4) must match byte-for-byte (Principle 7).

    World-indexed semantics
    -----------------------
    The chase runs *per named-graph world*.  Rules apply within a world; derived
    facts stay in that world.  There is no implicit cross-world union.
    Contested facts in different worlds coexist without collapse.

    Seam data contract
    ------------------
    Every output :class:`DerivedQuad` carries the full contract from
    LOGIC-RUNTIME.md §The seam data contract:
    ``graph``, ``(S, P, O, G)``, ``derivation_id``, ``rule_iri``,
    ``source_quad_ids``, ``profile``, ``budget_status``.

    No-occurrence gate (Stratum B)
    ------------------------------
    After the chase, every world is tested for token gufo:Event instances.
    A violation raises :class:`NoOccurrenceViolationError` immediately.

    Args:
        program: The compiled :class:`~.logic_ir.LogicProgram` (provides rules).
        input_graph: The facts as a :class:`~rdflib.ConjunctiveGraph` (named
            graphs = worlds).  Use :func:`parse_nquads` to convert N-Quads.
        profile: The semantic profile to use.  v1 oracle supports only
            :attr:`~.logic_ir.SemanticProfileId.POSITIVE_HORN`; other profiles
            are recorded as loss entries and skipped.
        budget: Optional runtime ceilings (issue #502).  ``None`` (the default)
            means unbounded — the chase runs to full fixpoint and the result is
            byte-identical to the pre-#502 behaviour (every quad
            ``budget_status="ok"``, ``incomplete=False``).  When a ceiling is
            passed and tripped, the result is a SOUND partial: kept quads carry
            ``budget_status="exhausted"`` and :attr:`MaterializationResult.incomplete`
            is ``True`` — never a false answer.
        enable_naf: When True (issue #503), evaluate ``logic:negatedBody`` atoms as
            stratified negation-as-failure (see :func:`_chase_world`).  When False
            (the default — every pre-#503 caller) the materialisation is
            byte-identical to the prior behaviour: negated atoms are treated as
            ordinary positive joins.  Only the gated foundation-lowering path
            (:mod:`gmeow_tools.logic_runner`) passes ``True``.

    Returns:
        A :class:`MaterializationResult` with all quads, worlds, profile,
        loss entries, counts, and the aggregate budget status / incompleteness
        marker.

    Raises:
        NoOccurrenceViolationError: If any world contains a token gufo:Event instance.
        MaterializationError: If the input is malformed.
    """
    profile_iri = _LOGIC_NS + str(profile)

    # Build a minimal onto-graph from the input for subClassOf closure
    # (only needed for the no-occurrence gate; uses the combined input facts)
    onto_graph = Graph()
    for ctx in input_graph.contexts():
        for s, p, o in ctx:
            valid_s = isinstance(s, URIRef | BNode)
            valid_p = isinstance(p, URIRef)
            valid_o = isinstance(o, URIRef | BNode | Literal)
            if valid_s and valid_p and valid_o:
                onto_graph.add((s, p, o))
    event_subclasses = _collect_event_subclasses(onto_graph)

    # Collect all worlds from the ConjunctiveGraph
    world_facts = _extract_worlds(input_graph)

    all_quads: list[DerivedQuad] = []
    all_loss_entries: list[LossEntry] = []
    all_worlds: set[str] = set(world_facts.keys())
    input_quad_count = 0
    derived_quad_count = 0

    any_exhausted = False
    for world_iri, facts in sorted(world_facts.items()):
        input_quad_count += len(facts)
        world_quads, closed_facts, loss, world_budget = _chase_world(
            world_iri, facts, program, profile_iri, budget, enable_naf
        )
        derived_quad_count += len(world_quads) - len(facts)
        all_quads.extend(world_quads)
        all_loss_entries.extend(loss)
        if world_budget.exhausted:
            any_exhausted = True

        # No-occurrence gate (Stratum B)
        _assert_no_occurrence(world_iri, closed_facts, event_subclasses)

    # Sort output deterministically: world IRI, then S/P/O N3-lex order
    all_quads.sort(key=lambda q: (q.graph, q.subject, q.predicate, q.obj))

    # Aggregate budget status: worst-of across all worlds (AC-B incompleteness
    # marker).  "exhausted" if ANY world tripped a ceiling, else "ok".
    aggregate_status = _BUDGET_EXHAUSTED if any_exhausted else _BUDGET_OK

    return MaterializationResult(
        quads=tuple(all_quads),
        worlds=frozenset(all_worlds),
        profile=profile_iri,
        loss_entries=tuple(all_loss_entries),
        input_quad_count=input_quad_count,
        derived_quad_count=derived_quad_count,
        budget_status=aggregate_status,
        incomplete=any_exhausted,
    )
