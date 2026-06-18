# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""OWL/gUFO adapter: normalize legacy ``owl:*`` / ``gufo:`` source into IR.

This module is the **adapter phase** of the #500 logic compiler
(LOGIC-MIGRATION.md §Adapter phases).  It accepts legacy RDF source that
uses ``owl:*`` structural vocabulary and/or ``gufo:`` stereotypes and
normalizes it into the same typed :class:`~.logic_ir.LogicProgram` IR that
the ``logic:`` front-end parser (:mod:`gmeow_tools.logic_frontend`) produces.

The adapter enables the **round-trip isomorphism gate**: a construct authored
in ``logic:`` and an equivalent construct in ``owl:*``/``gufo:`` form must
normalize to identical IR.  The gate is enforced by
:func:`assert_ir_isomorphic`.

Adapter phase contract
----------------------
* **Fail-soft on unrecognised constructs**: constructs that do not map cleanly
  (e.g. a blank-node restriction, an anonymous axiom, or an unmapped ``owl:``
  annotation property) emit a named :class:`~.logic_frontend.Diagnostic` and
  are skipped.  Nothing is silently dropped (ETHOS fail-fast principle).
* **Raise on fundamentally unparsable input**: empty graph or unreadable file
  raises :class:`~.logic_frontend.LogicParseError` — the same contract as the
  front-end parser.
* **Never duplicate**: the ``logic:`` front-end parser is reused via import
  for the logic: side of the round-trip; this module handles only owl/gufo.

Mapping rules
-------------
The following table governs normalization.  The ``logic:`` target sorts and
relations are the authoritative names from
``slices/core/logic/design/LOGIC-SEMANTICS.md`` (the foundational categories
section).

**gUFO stereotype → logic: sort** (``rdf:type`` on a named class):

+-------------------------+--------------------+
| gUFO IRI                | logic: sort        |
+=========================+====================+
| gufo:Kind               | logic:Kind         |
+-------------------------+--------------------+
| gufo:SubKind            | logic:SubKind      |
+-------------------------+--------------------+
| gufo:Phase              | logic:Phase        |
+-------------------------+--------------------+
| gufo:Role               | logic:Role         |
+-------------------------+--------------------+
| gufo:Category           | logic:Category     |
+-------------------------+--------------------+
| gufo:Mixin              | logic:Mixin        |
+-------------------------+--------------------+
| gufo:RoleMixin          | logic:RoleMixin    |
+-------------------------+--------------------+
| gufo:PhaseMixin         | logic:PhaseMixin   |
+-------------------------+--------------------+
| gufo:Relator            | logic:Relator      |
+-------------------------+--------------------+
| gufo:EventType          | logic:Event        |
+-------------------------+--------------------+
| gufo:SituationType      | logic:Situation    |
+-------------------------+--------------------+

**OWL structural construct → logic: predicate** (emitted as ``LogicAxiom``):

+--------------------------------+------------------------------+
| OWL predicate                  | logic: predicate             |
+================================+==============================+
| rdfs:subClassOf                | logic:subClassOf             |
+--------------------------------+------------------------------+
| owl:equivalentClass            | logic:equivalentClass        |
+--------------------------------+------------------------------+
| owl:disjointWith               | logic:disjointWith           |
+--------------------------------+------------------------------+
| rdfs:subPropertyOf             | logic:subPropertyOf          |
+--------------------------------+------------------------------+
| owl:equivalentProperty         | logic:equivalentProperty     |
+--------------------------------+------------------------------+
| owl:inverseOf                  | logic:inverseOf              |
+--------------------------------+------------------------------+
| rdfs:domain                    | logic:domain                 |
+--------------------------------+------------------------------+
| rdfs:range                     | logic:range                  |
+--------------------------------+------------------------------+
| owl:TransitiveProperty         | logic:transitiveProperty     |
+--------------------------------+------------------------------+
| owl:SymmetricProperty          | logic:symmetricProperty      |
+--------------------------------+------------------------------+
| owl:FunctionalProperty         | logic:functionalProperty     |
+--------------------------------+------------------------------+
| owl:InverseFunctionalProperty  | logic:inverseFunctionalProp  |
+--------------------------------+------------------------------+

Constructs not in the table (blank-node restrictions,
``owl:intersectionOf``, etc.) emit an ``UNMAPPED_OWL_CONSTRUCT`` diagnostic
and are skipped.

Round-trip isomorphism gate
---------------------------
:func:`assert_ir_isomorphic` compares two
:class:`~.logic_ir.LogicProgram` instances by their
:meth:`~.logic_ir.LogicProgram.canonical` form (content-addressed,
order-independent).  On mismatch it raises :class:`IRIsomorphismError`
with a directional diff::

    A has, B lacks:  <item>
    B has, A lacks:  <item>

mirroring ``statement_compile.assert_lossless`` in style.

Dependencies: rdflib, gmeow_tools.config, gmeow_tools.logic_ir,
gmeow_tools.logic_frontend.  No I/O side effects other than reading the
graph.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass
from pathlib import Path

from rdflib import RDF, Graph, Literal, Namespace, URIRef
from rdflib.namespace import OWL, RDFS

from gmeow_tools.config import LOGIC_NAMESPACE, PREFIXES
from gmeow_tools.logic_frontend import (
    WARNING,
    Diagnostic,
    LogicParseError,
)
from gmeow_tools.logic_ir import (
    ContextualScope,
    LogicAxiom,
    LogicProgram,
)

_log = logging.getLogger(__name__)

LOGIC = Namespace(LOGIC_NAMESPACE)
GUFO = Namespace(PREFIXES["gufo"])

# --------------------------------------------------------------------------- #
# gUFO stereotype → logic: sort  (rdf:type assertions)
# --------------------------------------------------------------------------- #

#: Each entry maps a gUFO stereotype IRI to its ``logic:`` sort local name.
#: When a named class is typed with the gUFO IRI the adapter emits an
#: ``rdf:type logic:<sort>`` LogicAxiom for that class.
_GUFO_TO_LOGIC_SORT: dict[URIRef, str] = {
    GUFO.Kind: "Kind",
    GUFO.SubKind: "SubKind",
    GUFO.Phase: "Phase",
    GUFO.Role: "Role",
    GUFO.Category: "Category",
    GUFO.Mixin: "Mixin",
    GUFO.RoleMixin: "RoleMixin",
    GUFO.PhaseMixin: "PhaseMixin",
    GUFO.Relator: "Relator",
    GUFO.EventType: "Event",
    GUFO.SituationType: "Situation",
}

# --------------------------------------------------------------------------- #
# gUFO class IRI → logic: term  (the machine-checkable correspondence floor)
# --------------------------------------------------------------------------- #


class _Superseded:
    """Sentinel marking a gUFO class that ``gmeow:logic`` deliberately replaces.

    The five gUFO *temporary-situation reifiers* (quality-value attribution,
    temporary constitution / instantiation / parthood / relationship) are a
    workaround for OWL 2 DL's inability to attach a time interval to an edge.
    ``gmeow:logic`` supersedes that whole reification pattern with
    :class:`logic:Fluent` + RDF-1.2 edge properties (a time-scoped statement is
    a first-class edge, not a reified blank node).  There is therefore **no**
    faithful 1:1 logic: term for these classes — they map to this sentinel so
    the coverage gate can assert "covered by supersession" rather than "missing".
    """

    __slots__ = ()

    def __repr__(self) -> str:  # pragma: no cover - debug aid only
        return "SUPERSEDED"


#: Module-level singleton sentinel (see :class:`_Superseded`).
SUPERSEDED: _Superseded = _Superseded()

#: Authoritative, COMPREHENSIVE ``gUFO class IRI → logic: term`` correspondence.
#:
#: This dict is the machine-checkable **"gmeow:logic ⊇ gUFO floor"**
#: correspondence: every one of the 49 ``owl:Class`` declarations in
#: ``imports/gufo.ttl`` appears here exactly once, mapped to either
#:
#: * the IRI string of the corresponding ``logic:`` term (the faithful
#:   down-projection target — gUFO is a VALIDATION_ONLY lossy projection of
#:   ``gmeow:logic``), or
#: * :data:`SUPERSEDED` for the five temporary-situation reifiers that
#:   ``gmeow:logic`` replaces with :class:`logic:Fluent` + RDF-1.2 edge
#:   properties (see :class:`_Superseded`).
#:
#: It is consumed by the coverage gate ``tests/test_logic_gufo_superset.py``
#: (#663 Task 4), which asserts that the gUFO floor is wholly covered by the
#: richer ``gmeow:logic`` spine.  It is a strict **superset** of
#: :data:`_GUFO_TO_LOGIC_SORT` (the 11 stereotype rows): the stereotype rows
#: reappear here with the same targets, plus the remaining 38 structural /
#: foundational / higher-order categories.
#:
#: Mapping notes for classes without a same-named ``logic:`` term:
#:   * ``gufo:EventType`` → ``logic:Event`` and ``gufo:SituationType`` →
#:     ``logic:Situation`` (consistent with the stereotype map: gUFO punning
#:     reifies the type, gmeow:logic keeps the perdurant/situation sort).
#:   * ``gufo:IntrinsicMode`` / ``gufo:ExtrinsicMode`` → ``logic:Mode``.
#:   * ``gufo:IntrinsicAspect`` / ``gufo:ExtrinsicAspect`` → ``logic:Aspect``.
#:   * ``gufo:QualityValue`` → ``logic:QualityValue``; the *quality space* it
#:     ranges over is ``logic:QualitySpace`` (a gmeow:logic enrichment with no
#:     gUFO class of its own, hence not a key here).
_GUFO_CLASS_TO_LOGIC: dict[URIRef, str | _Superseded] = {
    # --- Top of the individual taxonomy ---
    GUFO.Individual: LOGIC_NAMESPACE + "Individual",
    GUFO.ConcreteIndividual: LOGIC_NAMESPACE + "ConcreteIndividual",
    GUFO.AbstractIndividual: LOGIC_NAMESPACE + "AbstractIndividual",
    # --- Endurants / perdurants / situations (concrete-individual spine) ---
    GUFO.Endurant: LOGIC_NAMESPACE + "Endurant",
    GUFO.Event: LOGIC_NAMESPACE + "Event",
    GUFO.Situation: LOGIC_NAMESPACE + "Situation",
    GUFO.Participation: LOGIC_NAMESPACE + "Participation",
    # --- Endurant subkinds: objects vs aspects ---
    GUFO.Object: LOGIC_NAMESPACE + "Object",
    GUFO.Aspect: LOGIC_NAMESPACE + "Aspect",
    GUFO.IntrinsicAspect: LOGIC_NAMESPACE + "Aspect",
    GUFO.ExtrinsicAspect: LOGIC_NAMESPACE + "Aspect",
    GUFO.IntrinsicMode: LOGIC_NAMESPACE + "Mode",
    GUFO.ExtrinsicMode: LOGIC_NAMESPACE + "Mode",
    GUFO.Quality: LOGIC_NAMESPACE + "Quality",
    GUFO.QualityValue: LOGIC_NAMESPACE + "QualityValue",
    GUFO.Relator: LOGIC_NAMESPACE + "Relator",
    # --- Object aggregation kinds ---
    GUFO.Collection: LOGIC_NAMESPACE + "Collection",
    GUFO.FixedCollection: LOGIC_NAMESPACE + "FixedCollection",
    GUFO.VariableCollection: LOGIC_NAMESPACE + "VariableCollection",
    GUFO.Quantity: LOGIC_NAMESPACE + "Quantity",
    GUFO.FunctionalComplex: LOGIC_NAMESPACE + "FunctionalComplex",
    # --- Type level (higher-order) ---
    GUFO.Type: LOGIC_NAMESPACE + "Type",
    GUFO.EndurantType: LOGIC_NAMESPACE + "EndurantType",
    GUFO.RelationshipType: LOGIC_NAMESPACE + "RelationshipType",
    GUFO.MaterialRelationshipType: LOGIC_NAMESPACE + "MaterialRelationshipType",
    GUFO.ComparativeRelationshipType: LOGIC_NAMESPACE + "ComparativeRelationshipType",
    GUFO.AbstractIndividualType: LOGIC_NAMESPACE + "AbstractIndividualType",
    GUFO.ConcreteIndividualType: LOGIC_NAMESPACE + "ConcreteIndividualType",
    GUFO.EventType: LOGIC_NAMESPACE + "Event",
    GUFO.SituationType: LOGIC_NAMESPACE + "Situation",
    # --- Endurant-type meta axes (sortality / rigidity) ---
    GUFO.Sortal: LOGIC_NAMESPACE + "Sortal",
    GUFO.NonSortal: LOGIC_NAMESPACE + "NonSortal",
    GUFO.RigidType: LOGIC_NAMESPACE + "RigidType",
    GUFO.AntiRigidType: LOGIC_NAMESPACE + "AntiRigidType",
    GUFO.SemiRigidType: LOGIC_NAMESPACE + "SemiRigidType",
    GUFO.NonRigidType: LOGIC_NAMESPACE + "NonRigidType",
    # --- The 11 OntoUML stereotypes (superset of _GUFO_TO_LOGIC_SORT) ---
    GUFO.Kind: LOGIC_NAMESPACE + "Kind",
    GUFO.SubKind: LOGIC_NAMESPACE + "SubKind",
    GUFO.Phase: LOGIC_NAMESPACE + "Phase",
    GUFO.Role: LOGIC_NAMESPACE + "Role",
    GUFO.Category: LOGIC_NAMESPACE + "Category",
    GUFO.Mixin: LOGIC_NAMESPACE + "Mixin",
    GUFO.RoleMixin: LOGIC_NAMESPACE + "RoleMixin",
    GUFO.PhaseMixin: LOGIC_NAMESPACE + "PhaseMixin",
    # --- Superseded temporary-situation reifiers (logic:Fluent + RDF-1.2) ---
    GUFO.QualityValueAttributionSituation: SUPERSEDED,
    GUFO.TemporaryConstitutionSituation: SUPERSEDED,
    GUFO.TemporaryInstantiationSituation: SUPERSEDED,
    GUFO.TemporaryParthoodSituation: SUPERSEDED,
    GUFO.TemporaryRelationshipSituation: SUPERSEDED,
}

# --------------------------------------------------------------------------- #
# OWL structural predicate → logic: predicate  (structural axioms)
# --------------------------------------------------------------------------- #

#: Each entry maps an OWL/RDFS predicate IRI to the ``logic:`` predicate IRI
#: that is its canonical equivalent in the logic: vocabulary.
_OWL_PRED_TO_LOGIC: dict[URIRef, URIRef] = {
    RDFS.subClassOf: LOGIC.subClassOf,
    OWL.equivalentClass: LOGIC.equivalentClass,
    OWL.disjointWith: LOGIC.disjointWith,
    RDFS.subPropertyOf: LOGIC.subPropertyOf,
    OWL.equivalentProperty: LOGIC.equivalentProperty,
    OWL.inverseOf: LOGIC.inverseOf,
    RDFS.domain: LOGIC.domain,
    RDFS.range: LOGIC.range,
}

#: OWL property-characteristic types that are used as ``rdf:type`` objects on
#: property nodes; they map to corresponding ``logic:`` type IRIs.
_OWL_CHARACTERISTIC_TO_LOGIC: dict[URIRef, URIRef] = {
    OWL.TransitiveProperty: LOGIC.transitiveProperty,
    OWL.SymmetricProperty: LOGIC.symmetricProperty,
    OWL.FunctionalProperty: LOGIC.functionalProperty,
    OWL.InverseFunctionalProperty: LOGIC.inverseFunctionalProperty,
}

# RDFS meta-annotation predicates — carry no structural logic payload
_RDFS_SKIP_PREDS: frozenset[URIRef] = frozenset(
    {
        URIRef("http://www.w3.org/2000/01/rdf-schema#label"),
        URIRef("http://www.w3.org/2000/01/rdf-schema#comment"),
        URIRef("http://www.w3.org/2000/01/rdf-schema#seeAlso"),
        URIRef("http://www.w3.org/2000/01/rdf-schema#isDefinedBy"),
        OWL.versionIRI,
        OWL.versionInfo,
        OWL.imports,
        OWL.deprecated,
    }
)


# --------------------------------------------------------------------------- #
# IR isomorphism gate
# --------------------------------------------------------------------------- #


class IRIsomorphismError(Exception):
    """Raised by :func:`assert_ir_isomorphic` when two programs differ."""


def assert_ir_isomorphic(prog_a: LogicProgram, prog_b: LogicProgram) -> None:
    """Assert that two LogicProgram instances are canonically equal.

    Comparison is done via :meth:`~.logic_ir.LogicProgram.canonical`, which
    provides a stable, order-independent representation.  On mismatch the
    error message lists the directional diff — what is in A-not-B and in
    B-not-A — mirroring the style of ``statement_compile.assert_lossless``.

    Args:
        prog_a: The first program (e.g. parsed from ``logic:`` source).
        prog_b: The second program (normalized from ``owl:*``/``gufo:``).

    Raises:
        IRIsomorphismError: When the two programs differ, with a diff.
    """
    if prog_a == prog_b:
        return

    can_a = prog_a.canonical()
    can_b = prog_b.canonical()

    # Build set representations for diff
    axioms_a = {_axiom_key(a) for a in can_a["axioms"]}
    axioms_b = {_axiom_key(a) for a in can_b["axioms"]}
    rules_a = {_rule_key(r) for r in can_a["rules"]}
    rules_b = {_rule_key(r) for r in can_b["rules"]}
    profiles_a = {_profile_key(p) for p in can_a["profiles"]}
    profiles_b = {_profile_key(p) for p in can_b["profiles"]}

    lines: list[str] = []
    for item in sorted(axioms_a - axioms_b):
        lines.append(f"A has, B lacks (axiom):  {item}")
    for item in sorted(axioms_b - axioms_a):
        lines.append(f"B has, A lacks (axiom):  {item}")
    for item in sorted(rules_a - rules_b):
        lines.append(f"A has, B lacks (rule):   {item}")
    for item in sorted(rules_b - rules_a):
        lines.append(f"B has, A lacks (rule):   {item}")
    for item in sorted(profiles_a - profiles_b):
        lines.append(f"A has, B lacks (profile): {item}")
    for item in sorted(profiles_b - profiles_a):
        lines.append(f"B has, A lacks (profile): {item}")

    if not lines:
        # The canonical dicts differ in source_iri or some other field
        if can_a.get("source_iri") != can_b.get("source_iri"):
            lines.append(
                "source_iri differs: "
                f"A={can_a['source_iri']!r}  B={can_b['source_iri']!r}"
            )
        else:
            lines.append("programs differ (canonical() mismatch — check nested scope)")

    raise IRIsomorphismError(
        "IR isomorphism gate FAILED — programs do not normalize identically:\n  "
        + "\n  ".join(lines)
    )


def _axiom_key(a: dict) -> str:  # type: ignore[type-arg]
    """Stable string key for an axiom dict from canonical()."""
    return f"{a['subject']}\x00{a['predicate']}\x00{a['obj']}\x00{a['obj_is_literal']}"


def _rule_key(r: dict) -> str:  # type: ignore[type-arg]
    """Stable string key for a rule dict from canonical()."""
    head = r["head"]
    head_key = f"{head['subject']}\x00{head['predicate']}\x00{head['obj']}"
    body_key = "|".join(
        sorted(f"{b['subject']}\x00{b['predicate']}\x00{b['obj']}" for b in r["body"])
    )
    base = f"{head_key}\x00{body_key}"
    # Corpus-safety (issue #503): fold the inequality guards into the diff key
    # ONLY when present, so a difference in distinct-pairs is surfaced in the
    # isomorphism diff while pre-#503 rules (no ``"distinct"`` key in the
    # canonical dict) keep their exact pre-existing diff key string.
    distinct = r.get("distinct")
    if distinct:
        base += "\x00" + "|".join(f"{a}\x00{b}" for a, b in distinct)
    return base


def _profile_key(p: dict) -> str:  # type: ignore[type-arg]
    """Stable string key for a profile dict from canonical()."""
    return f"{p['profile_id']}\x00{p['complexity'] or ''}"


# --------------------------------------------------------------------------- #
# Internal helpers
# --------------------------------------------------------------------------- #


def _bind_legacy_prefixes(graph: Graph) -> None:
    """Bind GMEOW prefixes onto the graph for consistent IRI resolution."""
    for prefix, iri in PREFIXES.items():
        graph.bind(prefix, iri, override=False)


def _is_blank(node: object) -> bool:
    """Return True when node is a blank node (not a named IRI)."""
    from rdflib import BNode

    return isinstance(node, BNode)


def _is_complex_restriction(graph: Graph, node: object) -> bool:
    """Return True when node is a blank-node OWL restriction (unmappable)."""
    from rdflib import BNode

    if not isinstance(node, BNode):
        return False
    # A blank node typed owl:Restriction or connected via owl:someValuesFrom.
    restriction_preds = {
        OWL.someValuesFrom,
        OWL.allValuesFrom,
        OWL.hasValue,
        OWL.onProperty,
        OWL.minCardinality,
        OWL.maxCardinality,
        OWL.cardinality,
        OWL.onClass,
        OWL.onDataRange,
    }
    return any((node, pred, None) in graph for pred in restriction_preds)


# --------------------------------------------------------------------------- #
# Axiom extraction from OWL/gUFO source
# --------------------------------------------------------------------------- #


@dataclass(frozen=True, slots=True)
class _MappedAxiom:
    """Internal carrier before converting to LogicAxiom."""

    subject: str
    predicate: str
    obj: str
    obj_is_literal: bool = False


def _extract_gufo_sort_axioms(
    graph: Graph,
    diagnostics: list[Diagnostic],
) -> list[_MappedAxiom]:
    """Collect gUFO stereotype ``rdf:type`` triples → ``logic:`` sort axioms."""
    result: list[_MappedAxiom] = []
    for gufo_iri, logic_local in _GUFO_TO_LOGIC_SORT.items():
        logic_type_iri = LOGIC_NAMESPACE + logic_local
        for subject in graph.subjects(RDF.type, gufo_iri):
            if _is_blank(subject):
                diagnostics.append(
                    Diagnostic(
                        severity=WARNING,
                        code="BLANK_NODE_GUFO_SORT",
                        message=(
                            f"Blank-node subject typed {gufo_iri!s} cannot be"
                            " normalized to a logic: sort; skipped"
                        ),
                        subject=None,
                    )
                )
                continue
            result.append(
                _MappedAxiom(
                    subject=str(subject),
                    predicate=str(RDF.type),
                    obj=logic_type_iri,
                )
            )
    return result


def _extract_owl_char_axioms(
    graph: Graph,
    diagnostics: list[Diagnostic],
) -> list[_MappedAxiom]:
    """Normalize OWL property-characteristic ``rdf:type`` triples.

    e.g. ``ex:p rdf:type owl:TransitiveProperty`` →
    ``LogicAxiom(ex:p, rdf:type, logic:transitiveProperty)``.
    """
    result: list[_MappedAxiom] = []
    for owl_char, logic_type in _OWL_CHARACTERISTIC_TO_LOGIC.items():
        for subject in graph.subjects(RDF.type, owl_char):
            if _is_blank(subject):
                diagnostics.append(
                    Diagnostic(
                        severity=WARNING,
                        code="BLANK_NODE_OWL_CHAR",
                        message=(
                            f"Blank-node subject typed {owl_char!s} cannot be "
                            "normalized; skipped"
                        ),
                        subject=None,
                    )
                )
                continue
            result.append(
                _MappedAxiom(
                    subject=str(subject),
                    predicate=str(RDF.type),
                    obj=str(logic_type),
                )
            )
    return result


def _extract_owl_structural_axioms(
    graph: Graph,
    diagnostics: list[Diagnostic],
) -> list[_MappedAxiom]:
    """Normalize OWL/RDFS structural predicates to ``logic:`` predicates.

    Iterates over predicates in ``_OWL_PRED_TO_LOGIC``.  Blank-node objects
    (anonymous restrictions) emit ``UNMAPPED_OWL_CONSTRUCT`` and are skipped.
    """
    result: list[_MappedAxiom] = []
    for owl_pred, logic_pred in _OWL_PRED_TO_LOGIC.items():
        for s, _, o in graph.triples((None, owl_pred, None)):
            if _is_blank(s):
                # Anonymous subject — skip silently (blank reification helper)
                continue
            if _is_blank(o):
                # Anonymous object — likely a blank-node restriction.
                if _is_complex_restriction(graph, o):
                    diagnostics.append(
                        Diagnostic(
                            severity=WARNING,
                            code="UNMAPPED_OWL_CONSTRUCT",
                            message=(
                                f"{str(s)!r} {str(owl_pred)!r}"
                                " [blank-node restriction]: OWL restrictions"
                                " cannot be normalized to logic: axioms; skipped"
                            ),
                            subject=str(s),
                        )
                    )
                else:
                    diagnostics.append(
                        Diagnostic(
                            severity=WARNING,
                            code="UNMAPPED_OWL_CONSTRUCT",
                            message=(
                                f"{str(s)!r} {str(owl_pred)!r} [blank node]:"
                                " anonymous blank-node object cannot be"
                                " normalized; skipped"
                            ),
                            subject=str(s),
                        )
                    )
                continue
            obj_is_literal = isinstance(o, Literal)
            result.append(
                _MappedAxiom(
                    subject=str(s),
                    predicate=str(logic_pred),
                    obj=str(o),
                    obj_is_literal=obj_is_literal,
                )
            )
    return result


def _extract_unmapped_owl_triples(
    graph: Graph,
    diagnostics: list[Diagnostic],
) -> None:
    """Emit diagnostics for OWL predicates that are not in the mapping table.

    Only fires for named-IRI subjects that have OWL-namespace predicates
    outside the known mapping (i.e., not already handled above and not in
    skip lists).
    """
    owl_ns = str(OWL)
    for s, p, _o in graph:
        p_str = str(p)
        if not p_str.startswith(owl_ns):
            continue
        # Already handled above or is a skip predicate
        if p in _OWL_PRED_TO_LOGIC or p in _RDFS_SKIP_PREDS:
            continue
        # rdf:type with owl:* object — handled by characteristic / skip lists
        if p == RDF.type:
            continue
        if _is_blank(s):
            continue
        diagnostics.append(
            Diagnostic(
                severity=WARNING,
                code="UNMAPPED_OWL_CONSTRUCT",
                message=(
                    f"OWL predicate {p_str!r} on {str(s)!r}"
                    " has no logic: equivalent; skipped"
                ),
                subject=str(s),
            )
        )


# --------------------------------------------------------------------------- #
# Public API
# --------------------------------------------------------------------------- #


def adapt_legacy_source(
    graph_or_path: Graph | Path | str,
    *,
    source_iri: str | None = None,
) -> tuple[LogicProgram, list[Diagnostic]]:
    """Parse legacy ``owl:*`` / ``gufo:`` RDF source into a LogicProgram.

    Accepts the same argument forms as
    :func:`~gmeow_tools.logic_frontend.parse_logic_source` and normalizes
    the legacy vocabulary into the same
    :class:`~.logic_ir.LogicProgram` IR.

    The adapter handles:

    * **gUFO stereotype assignments** (``rdf:type gufo:Kind`` etc.) →
      ``rdf:type logic:Kind`` axioms.
    * **OWL property characteristics** (``owl:TransitiveProperty`` etc.)
      → ``rdf:type logic:transitiveProperty`` axioms.
    * **OWL/RDFS structural predicates** (``rdfs:subClassOf``,
      ``owl:equivalentClass``, ``rdfs:domain``, ``rdfs:range``,
      ``owl:inverseOf``, etc.) → their ``logic:`` equivalents.

    Constructs that do not map cleanly emit
    :class:`~.logic_frontend.Diagnostic` warnings and are skipped
    (fail-soft, never silent).

    Args:
        graph_or_path: An rdflib :class:`~rdflib.Graph`, or a
            :class:`~pathlib.Path` / string path to a Turtle file.
        source_iri: Optional IRI to record as the program's provenance.
            When ``None`` and ``graph_or_path`` is a path, the file URI
            is used.

    Returns:
        A ``(LogicProgram, diagnostics)`` pair.

    Raises:
        LogicParseError: When the input is empty, unreadable, or
            unparsable (e.g. a file that does not exist).
    """
    diagnostics: list[Diagnostic] = []

    # ---- Graph loading ----
    if isinstance(graph_or_path, Graph):
        graph = graph_or_path
    else:
        path = Path(graph_or_path)
        if not path.exists():
            raise LogicParseError(f"Source file does not exist: {path}")
        if source_iri is None:
            source_iri = path.as_uri()
        graph = Graph()
        try:
            graph.parse(str(path), format="turtle")
        except Exception as exc:
            raise LogicParseError(f"Failed to parse {path}: {exc}") from exc

    _bind_legacy_prefixes(graph)

    if len(graph) == 0:
        raise LogicParseError(
            "Source graph is empty — nothing to adapt.  "
            "Pass a non-empty graph or a Turtle file with owl:* / gufo: triples."
        )

    # ---- Extraction / normalization ----
    mapped: list[_MappedAxiom] = []
    mapped.extend(_extract_gufo_sort_axioms(graph, diagnostics))
    mapped.extend(_extract_owl_char_axioms(graph, diagnostics))
    mapped.extend(_extract_owl_structural_axioms(graph, diagnostics))
    _extract_unmapped_owl_triples(graph, diagnostics)

    # ---- Build LogicAxiom instances (dedup) ----
    axiom_set: set[LogicAxiom] = set()
    for m in mapped:
        try:
            axiom = LogicAxiom(
                subject=m.subject,
                predicate=m.predicate,
                obj=m.obj,
                obj_is_literal=m.obj_is_literal,
                scope=ContextualScope(),
            )
        except ValueError as exc:
            diagnostics.append(
                Diagnostic(
                    severity=WARNING,
                    code="MALFORMED_ADAPTED_AXIOM",
                    message=str(exc),
                    subject=m.subject,
                )
            )
            continue
        axiom_set.add(axiom)

    program = LogicProgram(
        axioms=tuple(axiom_set),
        rules=(),  # OWL/gUFO has no rule surface; rules are logic:-only
        profiles=(),  # OWL/gUFO carries no logic:SemanticProfile declarations
        source_iri=source_iri,
    )

    _log.debug(
        "adapt_legacy_source: %d axioms, %d diagnostics from %d source triples",
        len(program.axioms),
        len(diagnostics),
        len(graph),
    )

    return program, diagnostics
