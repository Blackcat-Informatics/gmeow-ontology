# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Python-authoritative lowering of OntoUML disciplines into ``logic:`` IR rules.

This module is the executable lowering of three of the four OntoUML structural
disciplines that :mod:`gmeow_tools.reasoning_lint` enforces over the merged gUFO
graph (issue #503, Task 2).  Where ``reasoning_lint`` is a *pure-rdflib walk* that
returns prose diagnostics, this module emits :class:`~.logic_ir.LogicRule` IR whose
materialized ``logic:violation`` head facts reproduce — class-for-class — the
*offending sets* the lint would report:

* :func:`exactly_one_stereotype` (``reasoning_lint`` l.128) — a class carries
  0 or >1 stereotype ⇒ ``logic:StereotypeCardinality``.
* :func:`identity_overlap` / **MixIden** (l.149) — a ``Kind`` with a ``Kind``
  proper-ancestor, or a non-``Kind`` sortal not tracing to exactly one ``Kind``
  ⇒ ``logic:MixIden``.
* :func:`anti_rigidity_discipline` / **FreeRole** + **MixRig** (l.180) — an
  anti-rigid sortal with no rigid ancestor ⇒ ``logic:FreeRole``; a rigid sortal
  with an anti-rigid-type ancestor ⇒ ``logic:MixRig``.
* :func:`relator_mediation` / **RelComp** (l.218) — a concrete relator mediating
  fewer than two distinct relata ⇒ ``logic:RelComp``.

The fourth discipline — positive **cross-world rigidity** — is **not** lowerable to
an ordinary in-world Datalog rule, because the GMEOW chase is strictly world-local
(``logic_materialize`` module docstring: "Rules apply within a world; derived quads
stay in that world; no cross-world union").  Rigidity is the world-indexed universal
constraint (LOGIC-SEMANTICS.md §Operational semantics)::

    ∀x, w, w' : exists(x, w) ∧ exists(x, w') ∧ instOf(x, T, w) ∧ rigid(T)
                ⇒ instOf(x, T, w')

so it is implemented here as exactly what the design prescribes — a **bounded
closure pass over the finite materialized world set** (LOGIC-SEMANTICS.md:
"evaluated by closure over the finite materialized world set") — by the pure
post-materialization function :func:`cross_world_rigidity_violations`.  The runner
(:mod:`gmeow_tools.logic_runner`) folds its violation quads back into the
materialized output, gated on the same ``foundation_lowering`` opt-in and active
only when ≥2 worlds materialized (so single-world goldens stay byte-identical).

Lowering vocabulary
-------------------
The lowering is grounded entirely in the standalone ``logic:`` vocabulary declared
in ``slices/core/logic/module.ttl``:

* Stereotypes are ``rdf:type`` puns into the eleven ``logic:`` meta-classes
  (:data:`_META_CLASSES`).
* Subsumption is the world-indexed predicate ``logic:subClassOf``.
* Mediation is ``logic:mediates`` (relator → relatum).
* The derived diagnostic is ``logic:violation`` relating an offending class to a
  closed ``logic:Discipline`` label individual (:data:`_DISCIPLINE_*`).

The lowering builds *derived* helper predicates (transitive subclass closure,
per-stereotype markers, ancestor markers) as ordinary rules, then expresses each
discipline as one or more ``logic:violation`` rules over those helpers.  Absence
("a class has NO stereotype", "no rigid ancestor") is expressed with
negation-as-failure (:attr:`~.logic_ir.LogicAxiom.negated`); "two distinct values"
("two stereotypes", "two mediated relata") is expressed with the inequality body
guard (:attr:`~.logic_ir.LogicRule.distinct_pairs`, issue #503, Task 1).

Stratification
--------------
Every negated predicate (e.g. ``logic:hasSomeStereotype``) is derived purely from
positive helper rules and never depends — transitively — on a ``logic:violation``
head or on its own negation.  The rule set is therefore stratifiable: the negation
only ever crosses a stratum boundary, never closes a cycle, so
:func:`gmeow_tools.logic_certify.certify_program` certifies it under
:attr:`~.logic_ir.SemanticProfileId.STRATIFIED_NAF`.

Purity
------
The emitter is pure — no I/O, no graph parsing, no side effects.  It takes a
:class:`~.logic_ir.LogicProgram` (so a future policy may consult the program's
declared sorts) and returns a deterministic tuple of :class:`~.logic_ir.LogicRule`.
"""

from __future__ import annotations

from rdflib import URIRef

from gmeow_tools.config import LOGIC_NAMESPACE
from gmeow_tools.logic_ir import LogicAxiom, LogicProgram, LogicRule
from gmeow_tools.logic_materialize import (
    DerivedQuad,
    MaterializationResult,
    derivation_id_iri,
    quad_reifier_iri,
)

# --------------------------------------------------------------------------- #
# Namespace constants
# --------------------------------------------------------------------------- #

#: The ``rdf:type`` predicate IRI string, exactly as the IR stores it (matching
#: :data:`gmeow_tools.logic_certify._RDF_TYPE` and the frontend's stereotype
#: extraction).  Stereotype puns are ``?C rdf:type <metaclass>`` atoms.
_RDF_TYPE = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"


def _logic(local: str) -> str:
    """Return the full ``logic:`` IRI for a local name."""
    return LOGIC_NAMESPACE + local


# --------------------------------------------------------------------------- #
# Stereotype vocabulary (the eleven grounded logic: meta-classes)
# --------------------------------------------------------------------------- #

#: Rigid sortals — supply / inherit a principle of identity (lint _RIGID_SORTALS).
_RIGID_SORTALS: tuple[str, ...] = ("Kind", "SubKind")
#: Anti-rigid sortals — classify instances only contingently (lint _ANTI_RIGID_SORTALS).
_ANTI_RIGID_SORTALS: tuple[str, ...] = ("Phase", "Role")
#: Every sortal stereotype (rigid + anti-rigid) — the lint's identity set.
_SORTALS: tuple[str, ...] = _RIGID_SORTALS + _ANTI_RIGID_SORTALS
#: Anti-rigid / semi-rigid types a rigid sortal must never specialize
#: (lint _ANTI_RIGID_TYPES).
_ANTI_RIGID_TYPES: tuple[str, ...] = (
    "Phase",
    "Role",
    "PhaseMixin",
    "RoleMixin",
    "Mixin",
)
#: The full grounded meta-class set, exactly one of which a class must carry.
#: This is the ``logic:`` analogue of ``reasoning_lint._META_CLASSES`` restricted
#: to the eleven stereotypes declared standalone in ``module.ttl`` (the gUFO-only
#: EventType / SituationType / AbstractIndividualType stereotypes are projection
#: artifacts and are not part of the grounded ``logic:`` surface).
_META_CLASSES: tuple[str, ...] = (
    "Kind",
    "SubKind",
    "Phase",
    "Role",
    "Category",
    "Mixin",
    "RoleMixin",
    "PhaseMixin",
    "Relator",
    "Event",
    "Situation",
)

# --------------------------------------------------------------------------- #
# Predicate IRIs (helpers are logic:-namespaced derived predicates)
# --------------------------------------------------------------------------- #

_P_SUBCLASS_OF = _logic("subClassOf")
_P_SUBCLASS_OF_T = _logic("subClassOfT")
_P_MEDIATES = _logic("mediates")
_P_VIOLATION = _logic("violation")

#: The cross-world rigidity diagnostic predicate (issue #503, Task 3).  Emitted by
#: the post-materialization closure pass :func:`cross_world_rigidity_violations`,
#: NOT by an in-world Datalog rule — the constraint is world-spanning and the chase
#: is world-local.  Declared as a closed-vocabulary term in ``module.ttl``.
_P_RIGIDITY_VIOLATION = _logic("rigidityViolation")

#: Marker the schema may carry to declare a type rigid explicitly (honoured in
#: addition to the stereotype-derived path, which is primary).
_P_RIGIDLY_APPLIES_TO = _logic("rigidlyAppliesTo")

#: The ``rdf:type`` predicate IRI (string form, matching :data:`_RDF_TYPE`).
_RDF_TYPE_IRI = _RDF_TYPE

#: Provenance rule IRI stamped on every emitted rigidity-violation quad's seam
#: ``rule_iri`` slot.  Distinct from the in-world ``logic:rule/`` namespace so the
#: cross-world closure pass is identifiable in derivation provenance.
_RIGIDITY_RULE_IRI = _logic("rule/cross-world-rigidity")

#: Derived markers.  Unary markers are reified as ``?C P ?C`` self-edges so the
#: materializer (whose head subject/predicate must be IRIs and whose object may be
#: a bound variable) can carry them.
_P_HAS_META_CLASS = _logic("hasMetaClass")
_P_IS_CLASS = _logic("isClass")
_P_HAS_SOME_STEREOTYPE = _logic("hasSomeStereotype")
_P_HAS_SORTAL = _logic("hasSortalStereotype")
_P_IS_NON_KIND_SORTAL = _logic("isNonKindSortal")
_P_KIND_ANCESTOR = _logic("kindAncestor")
_P_HAS_KIND_ANCESTOR = _logic("hasKindAncestor")
_P_HAS_RIGID_ANCESTOR = _logic("hasRigidAncestor")
_P_HAS_ANTI_RIGID_ANCESTOR = _logic("hasAntiRigidAncestor")
_P_ANTI_RIGID_SORTAL = _logic("antiRigidSortalClass")
_P_RIGID_SORTAL = _logic("rigidSortalClass")
_P_IS_RELATOR = _logic("isRelatorClass")
_P_HAS_LOGIC_SUBCLASS = _logic("hasLogicSubclass")
_P_CONCRETE_RELATOR = _logic("concreteRelator")
_P_HAS_TWO_MEDIATED = _logic("hasTwoMediatedRelata")

# --------------------------------------------------------------------------- #
# Discipline-label individuals (closed set; typos fail closed against module.ttl)
# --------------------------------------------------------------------------- #

_DISCIPLINE_STEREOTYPE_CARDINALITY = _logic("StereotypeCardinality")
_DISCIPLINE_MIXIDEN = _logic("MixIden")
_DISCIPLINE_FREEROLE = _logic("FreeRole")
_DISCIPLINE_MIXRIG = _logic("MixRig")
_DISCIPLINE_RELCOMP = _logic("RelComp")

# --------------------------------------------------------------------------- #
# Atom / rule construction helpers (pure)
# --------------------------------------------------------------------------- #


def _atom(
    subject: str, predicate: str, obj: str, *, negated: bool = False
) -> LogicAxiom:
    """Build a non-literal :class:`~.logic_ir.LogicAxiom` (all terms are IRIs/vars)."""
    return LogicAxiom(
        subject=subject,
        predicate=predicate,
        obj=obj,
        obj_is_literal=False,
        negated=negated,
    )


def _type_atom(
    subject_var: str, metaclass_local: str, *, negated: bool = False
) -> LogicAxiom:
    """Build a stereotype-pun atom ``?C rdf:type logic:<metaclass>``."""
    return _atom(subject_var, _RDF_TYPE, _logic(metaclass_local), negated=negated)


def _violation_rule(
    label_iri: str,
    body: tuple[LogicAxiom, ...],
    *,
    distinct_pairs: tuple[tuple[str, str], ...] = (),
) -> LogicRule:
    """Build a ``?C logic:violation <label>`` rule with the given body and guards.

    The head subject is always the class variable ``?C`` and the head object is the
    closed discipline label IRI, so every offending class is tagged with exactly the
    discipline it violates.
    """
    return LogicRule(
        head=_atom("?C", _P_VIOLATION, label_iri),
        body=body,
        distinct_pairs=distinct_pairs,
    )


# --------------------------------------------------------------------------- #
# Derived helper rules (transitive closure, stereotype + ancestor markers)
# --------------------------------------------------------------------------- #


def _closure_rules() -> tuple[LogicRule, ...]:
    """Transitive ``logic:subClassOf`` closure ``subClassOfT`` (positive recursion).

    ``subClassOfT(?C, ?A) :- subClassOf(?C, ?A)`` and
    ``subClassOfT(?C, ?A) :- subClassOf(?C, ?B), subClassOfT(?B, ?A)`` — a positive
    self-recursive predicate (a single-node SCC with only positive edges, which the
    stratifier certifies).  ``subClassOfT`` is the *proper-ancestor* relation: it
    starts from a direct edge, mirroring ``reasoning_lint._proper_ancestors`` which
    excludes the class itself.
    """
    return (
        LogicRule(
            head=_atom("?C", _P_SUBCLASS_OF_T, "?A"),
            body=(_atom("?C", _P_SUBCLASS_OF, "?A"),),
        ),
        LogicRule(
            head=_atom("?C", _P_SUBCLASS_OF_T, "?A"),
            body=(
                _atom("?C", _P_SUBCLASS_OF, "?B"),
                _atom("?B", _P_SUBCLASS_OF_T, "?A"),
            ),
        ),
    )


def _stereotype_marker_rules() -> tuple[LogicRule, ...]:
    """Per-stereotype ``hasMetaClass`` markers + the ``hasSomeStereotype`` roll-up.

    ``hasMetaClass(?C, logic:<M>) :- ?C rdf:type logic:<M>`` is emitted once per
    grounded meta-class ``M`` (so the materializer, which cannot range a single atom
    over a *set* of classes, sees one ground-object rule per stereotype).
    ``hasSomeStereotype(?C, ?C) :- hasMetaClass(?C, ?M)`` is the "carries at least
    one stereotype" marker the cardinality check negates.
    """
    rules: list[LogicRule] = []
    for metaclass in _META_CLASSES:
        rules.append(
            LogicRule(
                head=_atom("?C", _P_HAS_META_CLASS, _logic(metaclass)),
                body=(_type_atom("?C", metaclass),),
            )
        )
    rules.append(
        LogicRule(
            head=_atom("?C", _P_HAS_SOME_STEREOTYPE, "?C"),
            body=(_atom("?C", _P_HAS_META_CLASS, "?M"),),
        )
    )
    return tuple(rules)


def _class_universe_rules() -> tuple[LogicRule, ...]:
    """Derive ``isClass`` — the set of class-like terms a stereotype is required of.

    A term is a class when it appears as the subject or object of a ``subClassOf``
    edge, or when it carries any stereotype.  This is the ``logic:`` analogue of
    ``reasoning_lint._gmeow_classes`` (which enumerates ``owl:Class`` subjects); in
    the world-indexed fact base the class universe is recovered structurally from the
    subsumption + stereotype facts that mention the term.
    """
    return (
        LogicRule(
            head=_atom("?C", _P_IS_CLASS, "?C"),
            body=(_atom("?C", _P_SUBCLASS_OF, "?X"),),
        ),
        LogicRule(
            head=_atom("?X", _P_IS_CLASS, "?X"),
            body=(_atom("?C", _P_SUBCLASS_OF, "?X"),),
        ),
        LogicRule(
            head=_atom("?C", _P_IS_CLASS, "?C"),
            body=(_atom("?C", _P_HAS_META_CLASS, "?M"),),
        ),
    )


def _ancestor_marker_rules() -> tuple[LogicRule, ...]:
    """Kind-ancestor + rigid/anti-rigid ancestor markers over ``subClassOfT``.

    * ``kindAncestor(?C, ?A) :- subClassOfT(?C, ?A), hasMetaClass(?A, logic:Kind)``
      and ``hasKindAncestor(?C, ?C)`` roll-up.
    * ``hasRigidAncestor(?C, ?C)`` for each rigid sortal ``M`` (Kind, SubKind).
    * ``hasAntiRigidAncestor(?C, ?C)`` for each anti-rigid type ``M``
      (Phase, Role, PhaseMixin, RoleMixin, Mixin).
    """
    rules: list[LogicRule] = [
        LogicRule(
            head=_atom("?C", _P_KIND_ANCESTOR, "?A"),
            body=(
                _atom("?C", _P_SUBCLASS_OF_T, "?A"),
                _atom("?A", _P_HAS_META_CLASS, _logic("Kind")),
            ),
        ),
        LogicRule(
            head=_atom("?C", _P_HAS_KIND_ANCESTOR, "?C"),
            body=(_atom("?C", _P_KIND_ANCESTOR, "?A"),),
        ),
    ]
    for metaclass in _RIGID_SORTALS:
        rules.append(
            LogicRule(
                head=_atom("?C", _P_HAS_RIGID_ANCESTOR, "?C"),
                body=(
                    _atom("?C", _P_SUBCLASS_OF_T, "?A"),
                    _atom("?A", _P_HAS_META_CLASS, _logic(metaclass)),
                ),
            )
        )
    for metaclass in _ANTI_RIGID_TYPES:
        rules.append(
            LogicRule(
                head=_atom("?C", _P_HAS_ANTI_RIGID_ANCESTOR, "?C"),
                body=(
                    _atom("?C", _P_SUBCLASS_OF_T, "?A"),
                    _atom("?A", _P_HAS_META_CLASS, _logic(metaclass)),
                ),
            )
        )
    return tuple(rules)


def _stereotype_class_marker_rules() -> tuple[LogicRule, ...]:
    """Per-class stereotype-family markers (sortal / non-Kind-sortal / rigidity).

    These unify the per-stereotype ``hasMetaClass`` markers into the family
    predicates each discipline rule joins on:

    * ``hasSortalStereotype(?C, ?C)`` for each sortal (Kind/SubKind/Phase/Role).
    * ``isNonKindSortal(?C, ?C)`` from ``hasSortalStereotype(?C, ?C)`` and
      ``NOT hasMetaClass(?C, logic:Kind)`` — the lint's
      ``(stereotypes & sortals) and Kind not in stereotypes`` guard.
    * ``antiRigidSortalClass`` (Phase/Role) and ``rigidSortalClass`` (Kind/SubKind).
    """
    rules: list[LogicRule] = []
    for metaclass in _SORTALS:
        rules.append(
            LogicRule(
                head=_atom("?C", _P_HAS_SORTAL, "?C"),
                body=(_atom("?C", _P_HAS_META_CLASS, _logic(metaclass)),),
            )
        )
    rules.append(
        LogicRule(
            head=_atom("?C", _P_IS_NON_KIND_SORTAL, "?C"),
            body=(
                _atom("?C", _P_HAS_SORTAL, "?C"),
                _atom("?C", _P_HAS_META_CLASS, _logic("Kind"), negated=True),
            ),
        )
    )
    for metaclass in _ANTI_RIGID_SORTALS:
        rules.append(
            LogicRule(
                head=_atom("?C", _P_ANTI_RIGID_SORTAL, "?C"),
                body=(_atom("?C", _P_HAS_META_CLASS, _logic(metaclass)),),
            )
        )
    for metaclass in _RIGID_SORTALS:
        rules.append(
            LogicRule(
                head=_atom("?C", _P_RIGID_SORTAL, "?C"),
                body=(_atom("?C", _P_HAS_META_CLASS, _logic(metaclass)),),
            )
        )
    return tuple(rules)


def _relator_marker_rules() -> tuple[LogicRule, ...]:
    """Relator markers: ``isRelatorClass``, ``hasLogicSubclass``, ``concreteRelator``.

    A class is a relator when it carries the ``Relator`` stereotype OR has a
    ``subClassOfT`` ancestor that does (the lint's ``GUFO.Relator in ancestors``,
    extended to the stereotype pun since the ``logic:`` surface stereotypes Relator
    rather than subclassing it).  It is *concrete* when no other class subclasses it
    (``NOT hasLogicSubclass``), matching the lint's "abstract base defers its
    mediations to subtypes" carve-out.
    """
    return (
        LogicRule(
            head=_atom("?C", _P_IS_RELATOR, "?C"),
            body=(_atom("?C", _P_HAS_META_CLASS, _logic("Relator")),),
        ),
        LogicRule(
            head=_atom("?C", _P_IS_RELATOR, "?C"),
            body=(
                _atom("?C", _P_SUBCLASS_OF_T, "?A"),
                _atom("?A", _P_HAS_META_CLASS, _logic("Relator")),
            ),
        ),
        LogicRule(
            head=_atom("?C", _P_HAS_LOGIC_SUBCLASS, "?C"),
            body=(_atom("?X", _P_SUBCLASS_OF, "?C"),),
        ),
        LogicRule(
            head=_atom("?C", _P_CONCRETE_RELATOR, "?C"),
            body=(
                _atom("?C", _P_IS_RELATOR, "?C"),
                _atom("?C", _P_HAS_LOGIC_SUBCLASS, "?C", negated=True),
            ),
        ),
        LogicRule(
            head=_atom("?C", _P_HAS_TWO_MEDIATED, "?C"),
            body=(
                _atom("?C", _P_MEDIATES, "?R1"),
                _atom("?C", _P_MEDIATES, "?R2"),
            ),
            distinct_pairs=(("?R1", "?R2"),),
        ),
    )


# --------------------------------------------------------------------------- #
# Discipline rules (each emits ?C logic:violation <label>)
# --------------------------------------------------------------------------- #


def _stereotype_cardinality_rules() -> tuple[LogicRule, ...]:
    """``logic:StereotypeCardinality`` — a class with 0 or >1 stereotype.

    Reproduces ``reasoning_lint.exactly_one_stereotype`` (l.128): the *no-stereotype*
    branch is NAF over ``hasSomeStereotype`` (a class that exists in the fact base
    but carries no meta-class pun); the *conflicting-stereotype* branch is two
    distinct ``hasMetaClass`` markers guarded by ``?M1 != ?M2``.
    """
    return (
        # 0 stereotypes: a class with no stereotype pun at all.
        _violation_rule(
            _DISCIPLINE_STEREOTYPE_CARDINALITY,
            body=(
                _atom("?C", _P_IS_CLASS, "?C"),
                _atom("?C", _P_HAS_SOME_STEREOTYPE, "?C", negated=True),
            ),
        ),
        # >1 stereotype: two distinct meta-classes on the same class.
        _violation_rule(
            _DISCIPLINE_STEREOTYPE_CARDINALITY,
            body=(
                _atom("?C", _P_HAS_META_CLASS, "?M1"),
                _atom("?C", _P_HAS_META_CLASS, "?M2"),
            ),
            distinct_pairs=(("?M1", "?M2"),),
        ),
    )


def _identity_overlap_rules() -> tuple[LogicRule, ...]:
    """``logic:MixIden`` — identity-overlap (reasoning_lint.identity_overlap, l.149).

    Three offending shapes:

    * a ``Kind`` with a ``Kind`` proper-ancestor;
    * a non-``Kind`` sortal with TWO distinct ``Kind`` ancestors (``?A1 != ?A2``);
    * a non-``Kind`` sortal with NO ``Kind`` ancestor (NAF over ``hasKindAncestor``).

    Together the latter two reproduce the lint's ``len(kind_ancestors) != 1`` test
    for a non-``Kind`` sortal.
    """
    return (
        # (a) Kind specializing a Kind.
        _violation_rule(
            _DISCIPLINE_MIXIDEN,
            body=(
                _atom("?C", _P_HAS_META_CLASS, _logic("Kind")),
                _atom("?C", _P_KIND_ANCESTOR, "?A"),
            ),
        ),
        # (b.≥2) non-Kind sortal tracing to two distinct Kinds.
        _violation_rule(
            _DISCIPLINE_MIXIDEN,
            body=(
                _atom("?C", _P_IS_NON_KIND_SORTAL, "?C"),
                _atom("?C", _P_KIND_ANCESTOR, "?A1"),
                _atom("?C", _P_KIND_ANCESTOR, "?A2"),
            ),
            distinct_pairs=(("?A1", "?A2"),),
        ),
        # (b.0) non-Kind sortal tracing to no Kind at all.
        _violation_rule(
            _DISCIPLINE_MIXIDEN,
            body=(
                _atom("?C", _P_IS_NON_KIND_SORTAL, "?C"),
                _atom("?C", _P_HAS_KIND_ANCESTOR, "?C", negated=True),
            ),
        ),
    )


def _anti_rigidity_rules() -> tuple[LogicRule, ...]:
    """``logic:FreeRole`` + ``logic:MixRig`` (reasoning_lint.anti_rigidity, l.180).

    * FreeRole: an anti-rigid sortal (Phase/Role) with NO rigid (Kind/SubKind)
      ancestor (NAF over ``hasRigidAncestor``).
    * MixRig: a rigid sortal (Kind/SubKind) with an anti-rigid-type ancestor
      (Phase/Role/PhaseMixin/RoleMixin/Mixin) — positive, so **AC#3** (a Kind/SubKind
      with an anti-rigid Role parent) is caught.
    """
    return (
        _violation_rule(
            _DISCIPLINE_FREEROLE,
            body=(
                _atom("?C", _P_ANTI_RIGID_SORTAL, "?C"),
                _atom("?C", _P_HAS_RIGID_ANCESTOR, "?C", negated=True),
            ),
        ),
        _violation_rule(
            _DISCIPLINE_MIXRIG,
            body=(
                _atom("?C", _P_RIGID_SORTAL, "?C"),
                _atom("?C", _P_HAS_ANTI_RIGID_ANCESTOR, "?C"),
            ),
        ),
    )


def _relator_mediation_rules() -> tuple[LogicRule, ...]:
    """``logic:RelComp`` — a concrete relator mediating <2 distinct relata (l.218).

    A concrete relator (``concreteRelator``) that does NOT have two distinct
    ``logic:mediates`` targets (NAF over ``hasTwoMediatedRelata``) is in violation —
    the ``logic:`` reading of the lint's "a relator must mediate at least two relata"
    (counting distinct ``logic:mediates`` relata rather than the gUFO end-weighting).
    """
    return (
        _violation_rule(
            _DISCIPLINE_RELCOMP,
            body=(
                _atom("?C", _P_CONCRETE_RELATOR, "?C"),
                _atom("?C", _P_HAS_TWO_MEDIATED, "?C", negated=True),
            ),
        ),
    )


# --------------------------------------------------------------------------- #
# Public API
# --------------------------------------------------------------------------- #


def foundation_rules(
    program: LogicProgram, *, policy: str = "witness-obligation"
) -> tuple[LogicRule, ...]:
    """Return the OntoUML-discipline lowering rules for ``program``.

    The returned rules derive ``logic:violation`` facts whose offending-class sets
    reproduce, exactly, the verdicts of :mod:`gmeow_tools.reasoning_lint` for the
    ``exactly_one_stereotype`` / ``identity_overlap`` / ``anti_rigidity_discipline``
    / ``relator_mediation`` disciplines (issue #503, Task 2).  The rule set is
    stratifiable and certifies under
    :attr:`~.logic_ir.SemanticProfileId.STRATIFIED_NAF`.

    The emitter is **pure**: it inspects no I/O and mutates nothing.  ``program`` is
    accepted for forward-compatibility (a future policy may consult its declared
    sorts) but the current lowering is program-independent.

    Args:
        program: The compiled :class:`~.logic_ir.LogicProgram` the rules will be
            injected into (currently unused; reserved for policy-aware lowering).
        policy: Reserved policy selector wired by a later task (issue #503, Task 4).
            Accepted so the signature is stable; the default is the only behaviour
            today and the value is otherwise ignored.

    Returns:
        A deterministic tuple of :class:`~.logic_ir.LogicRule` — the helper-closure
        rules followed by the per-discipline ``logic:violation`` rules.
    """
    # ``program`` / ``policy`` are reserved (Task 4 wires policy); the lowering is
    # program-independent today.  Referenced so static checkers see the use.
    _ = (program, policy)
    return (
        *_closure_rules(),
        *_stereotype_marker_rules(),
        *_class_universe_rules(),
        *_ancestor_marker_rules(),
        *_stereotype_class_marker_rules(),
        *_relator_marker_rules(),
        *_stereotype_cardinality_rules(),
        *_identity_overlap_rules(),
        *_anti_rigidity_rules(),
        *_relator_mediation_rules(),
    )


# --------------------------------------------------------------------------- #
# Cross-world rigidity (issue #503, Task 3) — bounded closure, NOT a Datalog rule
# --------------------------------------------------------------------------- #
#
# The first three disciplines (above) are in-world Datalog: every body atom and
# every derived head fact lives inside a single materialized world, so the chase's
# world-local fixpoint computes them directly.  Positive cross-world rigidity is
# categorically different — it quantifies over PAIRS of worlds:
#
#     ∀x, w, w' : exists(x, w) ∧ exists(x, w') ∧ instOf(x, T, w) ∧ rigid(T)
#                 ⇒ instOf(x, T, w')
#
# No in-world rule can see two worlds at once (``logic_materialize`` keeps derived
# quads inside their origin world and performs no cross-world union), so the design
# (LOGIC-SEMANTICS.md §Operational semantics, §Boundedness) prescribes evaluating it
# "by closure/counting over the world set".  :func:`cross_world_rigidity_violations`
# is that closure: a pure pass over the FINITE materialized multi-world quad set that
# emits one ``logic:rigidityViolation`` per (instance, rigid type, world) where the
# rigid type fails to persist into a world the instance still inhabits.


def _rigid_type_iris(result: MaterializationResult) -> frozenset[str]:
    """Return the rigid-type IRIs over the UNION of all materialized worlds.

    Rigidity of a type is a **schema** property and therefore world-INDEPENDENT
    (LOGIC-SEMANTICS.md: a ``Kind`` is rigid in every world), so the rigid-type set
    is collected from the union across worlds rather than per world.  A type ``T`` is
    rigid iff, in any world, either:

    * ``T rdf:type logic:Kind`` or ``T rdf:type logic:SubKind`` — the
      stereotype-derived rigid-sortal set reused from Task 2
      (:data:`_RIGID_SORTALS`); this is the **primary** path; or
    * ``T logic:rigidlyAppliesTo …`` is asserted — an explicit rigidity marker the
      schema may carry (honoured in addition to the stereotype path).

    The materialized object term is in canonical N3 form (``<iri>`` for an IRI), so
    the rigid-sortal meta-class IRIs are wrapped to ``<…>`` before comparison.

    Args:
        result: The materialized multi-world result to scan.

    Returns:
        The frozenset of rigid-type IRI strings (bare IRIs, no angle brackets).
    """
    rigid_sortal_objs = {f"<{_logic(m)}>" for m in _RIGID_SORTALS}
    rigid: set[str] = set()
    for quad in result.quads:
        if quad.predicate == _RDF_TYPE_IRI and quad.obj in rigid_sortal_objs:
            # ``quad.subject`` is the type ``T`` being stereotyped as a rigid sortal.
            rigid.add(quad.subject)
        elif quad.predicate == _P_RIGIDLY_APPLIES_TO:
            # Explicit ``T logic:rigidlyAppliesTo …`` marker — ``T`` is the subject.
            rigid.add(quad.subject)
    return frozenset(rigid)


def cross_world_rigidity_violations(
    result: MaterializationResult,
) -> tuple[DerivedQuad, ...]:
    """Emit ``logic:rigidityViolation`` quads for cross-world rigidity failures.

    The fourth OntoUML discipline (issue #503, Task 3), implemented as the bounded
    closure the design mandates rather than as an in-world Datalog rule (which cannot
    express it — the chase is world-local).  For the world-indexed universal
    constraint::

        ∀x, w, w' : exists(x, w) ∧ exists(x, w') ∧ instOf(x, T, w) ∧ rigid(T)
                    ⇒ instOf(x, T, w')

    a violation is the witnessed failure of the consequent: an instance ``x`` typed
    by a rigid type ``T`` in some world ``w1`` that still EXISTS in another world
    ``w2`` but is NOT typed ``T`` there.  The emitted quad
    ``(x, logic:rigidityViolation, T)`` is placed **in world ``w2``** — the world
    where rigidity-persistence fails — so the diagnostic surfaces in the world that
    breaks the constraint.

    Semantics over the materialized multi-world quads
    -------------------------------------------------
    * **Rigid type set** — :func:`_rigid_type_iris`, the union-of-worlds schema set
      (stereotype-derived ``logic:Kind``/``logic:SubKind`` primary; explicit
      ``logic:rigidlyAppliesTo`` honoured too).  Schema is world-independent.
    * **instOf(x, T, w)** — a quad ``(x, rdf:type, T)`` in world ``w`` whose object
      ``T`` is in the rigid set.
    * **exists(x, w)** — ``x`` appears as the SUBJECT of any quad in world ``w``.
    * **Violation** — for each instance ``x``, each rigid type ``T``, and each
      ordered world pair ``(w1, w2)`` with ``w1 ≠ w2``: if ``instOf(x, T, w1)`` and
      ``exists(x, w2)`` but NOT ``instOf(x, T, w2)``, emit one
      ``(x, logic:rigidityViolation, T)`` in ``w2``.  De-duplicated to one quad per
      ``(x, T, w2)`` regardless of how many source worlds ``w1`` witness ``T``.

    The pass is **pure**: it reads ``result.quads`` and returns new quads; it mutates
    nothing and performs no I/O.  The runner folds the returned quads into the
    materialized output.  Output is deterministically sorted by
    ``(graph, subject, predicate, obj)`` — the same canonical key
    :mod:`gmeow_tools.logic_materialize` sorts by — so the emission order is stable.

    Seam contract
    -------------
    Each emitted :class:`~.logic_materialize.DerivedQuad` carries the full seam
    contract.  Provenance is content-addressed by the same recipes the chase uses:
    ``derivation_id`` hashes :data:`_RIGIDITY_RULE_IRI` over the reifier of the
    witnessing ``instOf(x, T, w1)`` typing fact, so the cross-world witness is folded
    into the derivation identity deterministically.  ``source_quad_ids`` is left
    **empty**, however: the witness lives in world ``w1`` while the violation lives in
    ``w2``, and the in-world explanation reconstructor
    (:func:`gmeow_tools.logic_explain._reconstruct_derivation_tree`) resolves every
    ``source_quad_ids`` antecedent *within the violation's own world*.  Citing a
    cross-world antecedent there would be unresolvable; a cross-world derivation has
    no in-world antecedent, so the quad is correctly a closure-pass leaf attributed to
    the rigidity rule.  The quad inherits ``result.profile`` and
    ``result.budget_status`` so it is consistent with the rest of the world.

    Args:
        result: The materialized multi-world result (the oracle's output).  A
            single-world result trivially yields no violations (no ordered world
            pair exists).

    Returns:
        A deterministically sorted tuple of ``logic:rigidityViolation``
        :class:`~.logic_materialize.DerivedQuad` records — possibly empty.
    """
    rigid_types = _rigid_type_iris(result)
    if not rigid_types:
        # No rigid type anywhere ⇒ no obligation can be violated.  (Also the common
        # single-world / non-foundation case once the runner gate is accounted for.)
        return ()

    # Index the materialized world set:
    #   * ``subjects_by_world[w]``  — the instances that EXIST in world ``w``.
    #   * ``typings_by_world[w]``   — the set of (x, T) rigid typings in world ``w``.
    subjects_by_world: dict[str, set[str]] = {}
    typings_by_world: dict[str, set[tuple[str, str]]] = {}
    for quad in result.quads:
        subjects_by_world.setdefault(quad.graph, set()).add(quad.subject)
        if quad.predicate == _RDF_TYPE_IRI and quad.obj.startswith("<"):
            type_iri = quad.obj[1:-1]
            if type_iri in rigid_types:
                typings_by_world.setdefault(quad.graph, set()).add(
                    (quad.subject, type_iri)
                )

    worlds = sorted(subjects_by_world)
    if len(worlds) < 2:
        # The constraint is over ORDERED pairs of distinct worlds; a single world
        # admits no such pair, so nothing can fire.
        return ()

    # Closure over the finite world set.  For every (x, T) rigidly typed in a source
    # world ``w1``, check every OTHER world ``w2`` where ``x`` still exists: if ``T``
    # does not persist there, that is a violation, recorded against ``w2``.  We
    # de-duplicate on (x, T, w2) — a single violation per failing target world,
    # independent of how many source worlds witness the rigid typing — and keep the
    # lexicographically smallest source world as the canonical provenance witness.
    violations: dict[tuple[str, str, str], str] = {}
    for w1 in worlds:
        for inst, type_iri in sorted(typings_by_world.get(w1, set())):
            for w2 in worlds:
                if w2 == w1:
                    continue
                if inst not in subjects_by_world.get(w2, set()):
                    continue  # exists(x, w2) is required (conditional constraint)
                if (inst, type_iri) in typings_by_world.get(w2, set()):
                    continue  # rigidity persists into w2 — no violation
                key = (inst, type_iri, w2)
                # First witnessing source world wins (worlds iterated in sorted
                # order, so this is the lexicographically smallest w1).
                violations.setdefault(key, w1)

    out: list[DerivedQuad] = []
    for (inst, type_iri, w2), source_world in violations.items():
        # Provenance: the source-world typing quad ``(inst, rdf:type, type_iri)`` is
        # the cross-world witness that obliges persistence; reify it under the same
        # recipe the chase uses and fold it into ``derivation_id`` so the violation's
        # identity is content-addressed over its witness.  ``source_quad_ids`` is left
        # EMPTY: the witness is in ``source_world`` (== w1) but the violation is in
        # ``w2``, and the in-world explanation reconstructor resolves antecedents
        # within the violation's own world — a cross-world derivation has no in-world
        # antecedent, so the quad is a closure-pass leaf attributed to the rule.
        witness_reifier = quad_reifier_iri(
            URIRef(inst), URIRef(_RDF_TYPE_IRI), URIRef(type_iri)
        )
        deriv_id = derivation_id_iri(_RIGIDITY_RULE_IRI, [witness_reifier])
        # Corpus-safety (issue #503): the violation object is an IRI, so its N3 form
        # is ``<iri>`` — matching the materializer's object canonicalisation exactly.
        out.append(
            DerivedQuad(
                graph=w2,
                subject=inst,
                predicate=_P_RIGIDITY_VIOLATION,
                obj=URIRef(type_iri).n3(),
                graph_component=w2,
                derivation_id=deriv_id,
                rule_iri=_RIGIDITY_RULE_IRI,
                source_quad_ids=[],
                profile=result.profile,
                budget_status=result.budget_status,
            )
        )
        # ``source_world`` (== w1) is folded into the content-addressed witness
        # reifier above; referenced so static checkers see the loop variable's use.
        _ = source_world

    out.sort(key=lambda q: (q.graph, q.subject, q.predicate, q.obj))
    return tuple(out)
