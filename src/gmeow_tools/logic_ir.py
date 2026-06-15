# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Typed intermediate representation (IR) for the GMEOW Logic compiler (issue #500).

This module is **pure data** — no I/O, no graph parsing, no side effects.  It
defines the frozen dataclass hierarchy that the logic compiler pipeline operates
on:

* :class:`LogicModality` — enum of modality kinds derived from the ``logic:``
  world/modal vocabulary (alethic, epistemic, deontic, doxastic, telic,
  representational, counterfactual).
* :class:`PreservationKind` — enum matching the six ``logic:PreservationKind``
  named individuals verbatim (single source of truth — any change to
  ``module.ttl`` must update this enum).
* :class:`ComplexityClass` — a typed wrapper for the free-text
  ``logic:complexityClass`` values (e.g. ``"PTIME"``, ``"N2EXPTIME"``,
  ``"undecidable"``).
* :class:`SemanticProfileId` — enum of the six ``logic:SemanticProfile``
  named individuals verbatim.
* :class:`LogicAxiom` — a single ``logic:`` axiom with contextual scope
  (standpoint, time, confidence, modality, provenance).
* :class:`LogicRule` — a single rule (head + body axioms) with the same
  contextual scope.
* :class:`LogicProfile` — a declared semantic profile + its complexity class.
* :class:`LogicProgram` — the top-level container for a compiled logic program,
  with a :meth:`LogicProgram.canonical` method that provides a stable,
  order-independent representation suitable for round-trip isomorphism testing.

Canonicalization contract
-------------------------
``LogicProgram`` equality is **content-addressed** and **order-independent**:
two ``LogicProgram`` instances with the same axioms, rules, and profiles but
constructed in a different order compare equal and produce the same
:meth:`~LogicProgram.canonical` output.  Internally this is achieved by
converting all collection fields to **sorted tuples** in
``__post_init__``.  Sorting keys are the ``str()`` representation of each
item.  The ``canonical()`` method returns a ``dict`` of the same sorted
sequences (suitable for JSON serialisation or a hash).  This is the
comparison surface Task 2/3 will use for round-trip isomorphism gating.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import StrEnum
from typing import Any

# --------------------------------------------------------------------------- #
# Enums — single source of truth, local names taken verbatim from module.ttl
# --------------------------------------------------------------------------- #


class SemanticProfileId(StrEnum):
    """The six ``logic:SemanticProfile`` named individuals.

    See LOGIC-SEMANTICS.md §Profiles for the authoritative definition.
    Local names are taken verbatim from ``slices/core/logic/module.ttl`` — any
    change there must be reflected here.  The enum value is the local name
    (without the ``logic:`` prefix) so that it can be round-tripped through
    ``LOGIC_NAMESPACE + value``.
    """

    POSITIVE_HORN = "PositiveHornProfile"
    STRATIFIED_NAF = "StratifiedNAFProfile"
    WELL_FOUNDED = "WellFoundedProfile"
    STABLE_MODEL = "StableModelProfile"
    PROCEDURAL_PROLOG = "ProceduralPrologProfile"
    PROBABILISTIC = "ProbabilisticProfile"


class PreservationKind(StrEnum):
    """The six ``logic:PreservationKind`` named individuals (LOGIC-CONFORMANCE.md).

    Local names taken verbatim from ``slices/core/logic/module.ttl``.
    """

    EXACT = "ExactPreservation"
    SOUND_UNDER = "SoundUnderApproximation"
    COMPLETE_OVER = "CompleteOverApproximation"
    VALIDATION_ONLY = "ValidationOnly"
    INCONSISTENCY_PRESERVING = "InconsistencyPreserving"
    INCONSISTENCY_REFLECTING = "InconsistencyReflecting"


class LogicModality(StrEnum):
    """World/modal kinds from the ``logic:World`` taxonomy.

    Used to annotate contextual scope on axioms and rules.  The values mirror
    the world-type taxonomy mentioned in the ``logic:World`` definition in
    ``module.ttl`` (epistemic, doxastic, telic, deontic, alethic,
    representational, counterfactual).  A ``NONE`` sentinel denotes no modal
    annotation (the default, unmodalized reading).
    """

    NONE = "none"
    ALETHIC = "alethic"
    EPISTEMIC = "epistemic"
    DOXASTIC = "doxastic"
    TELIC = "telic"
    DEONTIC = "deontic"
    REPRESENTATIONAL = "representational"
    COUNTERFACTUAL = "counterfactual"


# --------------------------------------------------------------------------- #
# Supporting dataclasses
# --------------------------------------------------------------------------- #


@dataclass(frozen=True, slots=True)
class ComplexityClass:
    """A typed wrapper for ``logic:complexityClass`` values.

    The value is a free-text string (e.g. ``"PTIME"``, ``"N2EXPTIME"``,
    ``"terminating/PTIME-data"``, ``"undecidable"``) exactly as it appears in
    the ontology.  Frozen so it participates in sets and sorted collections.

    Attributes:
        label: The complexity/decidability class string from the ontology.
    """

    label: str

    def __post_init__(self) -> None:
        """Validate that label is non-empty."""
        if not self.label or not self.label.strip():
            raise ValueError("ComplexityClass.label must be a non-empty string")

    def __str__(self) -> str:
        """Return the complexity class label string."""
        return self.label


# --------------------------------------------------------------------------- #
# Contextual scope — shared by LogicAxiom and LogicRule
# --------------------------------------------------------------------------- #


@dataclass(frozen=True, slots=True)
class ContextualScope:
    """Contextual scope annotations shared by axioms and rules.

    All fields are optional (``None`` = not declared for this item).  When
    present they bind the axiom/rule to a particular world-indexed context.

    Attributes:
        standpoint: IRI string of the standpoint (``logic:World`` individual)
            within which this axiom/rule holds.
        time: An opaque time-expression string (ISO 8601 or EDTF) bounding
            the axiom/rule's validity window.
        confidence: The ``logic:confidence`` value in ``[0, 1]`` — epistemic
            confidence of the asserter.  Normatively distinct from
            ``logic:probability`` (see LOGIC-SEMANTICS.md).
        modality: The modal kind of the axiom/rule world context.
        provenance: IRI string of the provenance source / agent that asserted
            this axiom/rule.
    """

    standpoint: str | None = None
    time: str | None = None
    confidence: float | None = None
    modality: LogicModality = LogicModality.NONE
    provenance: str | None = None

    def __post_init__(self) -> None:
        """Validate numeric ranges."""
        if self.confidence is not None and not (0.0 <= self.confidence <= 1.0):
            raise ValueError(
                f"ContextualScope.confidence must be in [0, 1], got {self.confidence}"
            )


# --------------------------------------------------------------------------- #
# Core IR nodes
# --------------------------------------------------------------------------- #


@dataclass(frozen=True, slots=True)
class LogicAxiom:
    """A single ``logic:`` axiom with contextual scope.

    An axiom is a **ground assertion** in the compiled logic program - a
    subject-predicate-object triple expressed in terms of the ``logic:``
    vocabulary (UFO⁺ sorts, relations, quantitative axes, world terms, and
    preservation polarity).

    Attributes:
        subject: IRI string of the axiom subject.
        predicate: IRI string of the axiom predicate.
        obj: IRI string or literal-string value of the axiom object.
        obj_is_literal: True when ``obj`` is a literal (data value), False
            when it is an IRI.
        scope: Contextual scope for this axiom (standpoint, time, confidence,
            modality, provenance).
    """

    subject: str
    predicate: str
    obj: str
    obj_is_literal: bool = False
    scope: ContextualScope = field(default_factory=ContextualScope)

    def __post_init__(self) -> None:
        """Validate that subject and predicate are non-empty IRI strings."""
        if not self.subject:
            raise ValueError("LogicAxiom.subject must be a non-empty IRI string")
        if not self.predicate:
            raise ValueError("LogicAxiom.predicate must be a non-empty IRI string")

    def _sort_key(self) -> str:
        """Stable sort key for canonical ordering."""
        return (
            f"{self.subject}\x00{self.predicate}\x00{self.obj}\x00{self.obj_is_literal}"
        )


@dataclass(frozen=True, slots=True)
class LogicRule:
    """A single ``logic:`` rule: a head axiom derived from body axioms.

    A rule is a **conditional assertion**: when all ``body`` axioms hold, the
    ``head`` axiom is derived.  Both head and body are expressed as
    :class:`LogicAxiom` instances.  The same contextual scope (standpoint,
    time, confidence, modality, provenance) applies to the rule as a whole.

    The ``body`` tuple is stored in canonical (sorted) order by
    ``__post_init__`` so that two rules with the same head/body but different
    construction order compare equal.

    Attributes:
        head: The derived axiom (consequent).
        body: The condition axioms (antecedents), stored in canonical order.
        scope: Contextual scope for this rule.
    """

    head: LogicAxiom
    body: tuple[LogicAxiom, ...]
    scope: ContextualScope = field(default_factory=ContextualScope)

    def __post_init__(self) -> None:
        """Canonicalize body order and validate non-empty head."""
        # frozen=True means we must use object.__setattr__
        canonical_body = tuple(sorted(self.body, key=lambda a: a._sort_key()))
        object.__setattr__(self, "body", canonical_body)

    def _sort_key(self) -> str:
        """Stable sort key for canonical ordering."""
        body_key = "|".join(a._sort_key() for a in self.body)
        return f"{self.head._sort_key()}\x00{body_key}"


@dataclass(frozen=True, slots=True)
class LogicProfile:
    """A declared semantic profile with its complexity class.

    Every rule set in a ``LogicProgram`` must declare exactly one profile
    (LOGIC-SEMANTICS.md §Semantic profiles).  The ``complexity`` may be
    ``None`` when not explicitly declared (though a complete program should
    always carry one).

    Attributes:
        profile_id: The named ``logic:SemanticProfile`` individual.
        complexity: The ``logic:complexityClass`` value, or ``None`` if not
            declared.
    """

    profile_id: SemanticProfileId
    complexity: ComplexityClass | None = None

    def _sort_key(self) -> str:
        """Stable sort key."""
        compl = str(self.complexity) if self.complexity else ""
        return f"{self.profile_id}\x00{compl}"


# --------------------------------------------------------------------------- #
# Top-level container
# --------------------------------------------------------------------------- #


@dataclass(frozen=True, slots=True)
class LogicProgram:
    """Top-level container for a compiled ``logic:`` program.

    A ``LogicProgram`` aggregates axioms, rules, and profiles derived from a
    ``logic:`` RDF 1.2 source graph.  It is the unit of comparison for the
    round-trip isomorphism gate (Tasks 2/3).

    **Canonicalization contract** — two ``LogicProgram`` instances are
    considered equal when they contain the same axioms, rules, and profiles
    regardless of the order in which they were constructed.  This is achieved
    by storing all three fields as **sorted tuples** in ``__post_init__``
    (sort key = ``str()`` of each element via their ``_sort_key()`` helpers).
    :meth:`canonical` returns a plain ``dict`` of the same sorted sequences
    for serialisation or hashing.

    Attributes:
        axioms: Tuple of :class:`LogicAxiom` instances in canonical order.
        rules: Tuple of :class:`LogicRule` instances in canonical order.
        profiles: Tuple of :class:`LogicProfile` instances in canonical order.
        source_iri: IRI of the source graph or document (optional provenance).
    """

    axioms: tuple[LogicAxiom, ...]
    rules: tuple[LogicRule, ...]
    profiles: tuple[LogicProfile, ...]
    source_iri: str | None = None

    def __post_init__(self) -> None:
        """Canonicalize all collection fields into sorted tuples."""
        object.__setattr__(
            self,
            "axioms",
            tuple(sorted(self.axioms, key=lambda a: a._sort_key())),
        )
        object.__setattr__(
            self,
            "rules",
            tuple(sorted(self.rules, key=lambda r: r._sort_key())),
        )
        object.__setattr__(
            self,
            "profiles",
            tuple(sorted(self.profiles, key=lambda p: p._sort_key())),
        )

    def canonical(self) -> dict[str, Any]:
        """Return a stable, order-independent dict representation.

        The dict is suitable for JSON serialisation or content-hash comparison.
        All sequences are already in canonical (sorted) order due to
        ``__post_init__``; this method exposes them as plain Python structures.

        Returns:
            A ``dict`` with keys ``"axioms"``, ``"rules"``, ``"profiles"``,
            and ``"source_iri"``, each mapping to a list of dicts or a scalar.
        """
        return {
            "axioms": [
                {
                    "subject": a.subject,
                    "predicate": a.predicate,
                    "obj": a.obj,
                    "obj_is_literal": a.obj_is_literal,
                    "scope": {
                        "standpoint": a.scope.standpoint,
                        "time": a.scope.time,
                        "confidence": a.scope.confidence,
                        "modality": str(a.scope.modality),
                        "provenance": a.scope.provenance,
                    },
                }
                for a in self.axioms
            ],
            "rules": [
                {
                    "head": {
                        "subject": r.head.subject,
                        "predicate": r.head.predicate,
                        "obj": r.head.obj,
                        "obj_is_literal": r.head.obj_is_literal,
                    },
                    "body": [
                        {
                            "subject": b.subject,
                            "predicate": b.predicate,
                            "obj": b.obj,
                            "obj_is_literal": b.obj_is_literal,
                        }
                        for b in r.body
                    ],
                    "scope": {
                        "standpoint": r.scope.standpoint,
                        "time": r.scope.time,
                        "confidence": r.scope.confidence,
                        "modality": str(r.scope.modality),
                        "provenance": r.scope.provenance,
                    },
                }
                for r in self.rules
            ],
            "profiles": [
                {
                    "profile_id": str(p.profile_id),
                    "complexity": str(p.complexity) if p.complexity else None,
                }
                for p in self.profiles
            ],
            "source_iri": self.source_iri,
        }
