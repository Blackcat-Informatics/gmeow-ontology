"""Pure-Python invariants for the statement-metadata layer.

These checks guard the canonical statement DSL before either downcast is written,
the same role :mod:`gmeow_tools.projection_lint` plays for the mapping compiler.
They are deliberately Docker-free (pure rdflib) so they run in the non-Docker CI
job; the Jena-backed OWL↔RDF 1.2 round-trip isomorphism (the reasoning-lossless
proof) lives in :mod:`gmeow_tools.rdf12`, gated on Jena.

* :func:`annotation_property_soundness` — every ``gmeow:annProperty`` is an
  ``owl:AnnotationProperty`` in the ontology (so the generated ``owl:Axiom``
  downcast stays OWL 2 DL-clean), and ``gmeow:confidence`` values lie in [0, 1].
* :func:`base_triple_groundedness` — every quoted predicate is a declared property,
  and any ``gmeow:``-namespace subject/object is a declared term (a typo can't
  fabricate a reified statement about a nonexistent term).
"""

from __future__ import annotations

from rdflib import RDF, RDFS, XSD, Graph, Literal, URIRef
from rdflib.namespace import OWL, Namespace

from gmeow_tools.config import NAMESPACE, PREFIXES
from gmeow_tools.statement_dsl import StatementDsl

GM = Namespace(PREFIXES["gmeow"])

#: rdf:type values that count as "a declared property".
_PROPERTY_TYPES: frozenset[URIRef] = frozenset(
    {
        OWL.ObjectProperty,
        OWL.DatatypeProperty,
        OWL.AnnotationProperty,
        RDF.Property,
    }
)

#: The OWL 2 datatype map (the only datatypes legal in a base-triple literal that
#: is a logical data-property assertion). Notably EXCLUDES xsd:date, xsd:time,
#: xsd:gYear*, xsd:duration — using one in the reasoned core is a DL violation.
_RDF = Namespace(PREFIXES["rdf"])
_OWL2_DL_DATATYPES: frozenset[URIRef] = frozenset(
    {
        RDFS.Literal,
        _RDF.PlainLiteral,
        _RDF.XMLLiteral,
        _RDF.langString,
        OWL.real,
        OWL.rational,
        XSD.string,
        XSD.normalizedString,
        XSD.token,
        XSD.language,
        XSD.Name,
        XSD.NCName,
        XSD.NMTOKEN,
        XSD.decimal,
        XSD.integer,
        XSD.nonNegativeInteger,
        XSD.nonPositiveInteger,
        XSD.positiveInteger,
        XSD.negativeInteger,
        XSD.long,
        XSD.int,
        XSD.short,
        XSD.byte,
        XSD.unsignedLong,
        XSD.unsignedInt,
        XSD.unsignedShort,
        XSD.unsignedByte,
        XSD.double,
        XSD.float,
        XSD.boolean,
        XSD.hexBinary,
        XSD.base64Binary,
        XSD.anyURI,
        XSD.dateTime,
        XSD.dateTimeStamp,
    }
)


def _is_gmeow_vocab_term(term: URIRef) -> bool:
    """Whether a term is a GMEOW *vocabulary* term (NAMESPACE + bare local name).

    Instance/example IRIs live under sub-paths (``…/gmeow/examples/…``,
    ``…/gmeow/reifier/…``) and are *not* vocabulary terms — only a bare local name
    directly under the namespace is checked for declaration (so a typo in a class
    or value individual is caught, but instance data is not flagged).
    """
    iri = str(term)
    return iri.startswith(NAMESPACE) and "/" not in iri[len(NAMESPACE) :]


def _is_declared(onto: Graph, term: URIRef) -> bool:
    """Whether a term is the subject of any triple in the ontology."""
    return (term, None, None) in onto


def annotation_property_soundness(dsl: StatementDsl, onto: Graph) -> list[str]:
    """Every annProperty must be an owl:AnnotationProperty; confidence ∈ [0, 1]."""
    problems: list[str] = []
    for cell in dsl.cells:
        for ann in cell.annotations:
            if (ann.prop, RDF.type, OWL.AnnotationProperty) not in onto:
                problems.append(
                    f"{cell.iri}: annotation property {ann.prop} is not an "
                    "owl:AnnotationProperty in the ontology — the OWL downcast "
                    "would not be OWL 2 DL-clean"
                )
            if ann.prop == GM.confidence:
                problems.extend(_confidence_problem(cell.iri, ann.value))
    return problems


def _confidence_problem(cell: URIRef, value: URIRef | Literal) -> list[str]:
    if not isinstance(value, Literal):
        return [f"{cell}: gmeow:confidence value must be a literal, got {value!r}"]
    try:
        number = float(value)
    except (TypeError, ValueError):
        return [f"{cell}: gmeow:confidence value {value!r} is not numeric"]
    if not 0.0 <= number <= 1.0:
        return [f"{cell}: gmeow:confidence {number} is outside [0, 1]"]
    return []


def base_triple_groundedness(dsl: StatementDsl, onto: Graph) -> list[str]:
    """Quoted predicates must be declared; gmeow: subjects/objects must exist."""
    problems: list[str] = []
    for cell in dsl.cells:
        t = cell.triple
        types = set(onto.objects(t.predicate, RDF.type))
        if not types & _PROPERTY_TYPES:
            problems.append(
                f"{cell.iri}: quoted predicate {t.predicate} is not a declared "
                "GMEOW property"
            )
        for role, term in (("qSubject", t.subject), ("qObject", t.obj)):
            if (
                isinstance(term, URIRef)
                and _is_gmeow_vocab_term(term)
                and not _is_declared(onto, term)
            ):
                problems.append(
                    f"{cell.iri}: {role} {term} is a gmeow: vocabulary term "
                    "but is not declared in the ontology (typo?)"
                )
    return problems


def base_triple_dl_datatypes(dsl: StatementDsl) -> list[str]:
    """A literal base-triple object must use an OWL 2 datatype (the reasoner sees it).

    The base triple is a logical assertion the reasoner consumes via the OWL
    downcast, so a non-OWL-2 datatype (``xsd:date``, ``xsd:time``, ``xsd:gYear``,
    ``xsd:duration`` …) would break OWL 2 DL. Annotation values are exempt — they
    live outside the logical core — so this checks only ``qObjectLiteral``.
    """
    problems: list[str] = []
    for cell in dsl.cells:
        obj = cell.triple.obj
        if (
            isinstance(obj, Literal)
            and obj.datatype is not None
            and obj.datatype not in _OWL2_DL_DATATYPES
        ):
            problems.append(
                f"{cell.iri}: quoted-object literal datatype {obj.datatype} is "
                "not an OWL 2 datatype — the reasoned OWL downcast would not be "
                "OWL 2 DL (use xsd:dateTime, xsd:string, …)"
            )
    return problems


def statement_invariants(dsl: StatementDsl, onto: Graph) -> list[str]:
    """Run every pure-Python statement invariant; empty list means clean."""
    return [
        *annotation_property_soundness(dsl, onto),
        *base_triple_groundedness(dsl, onto),
        *base_triple_dl_datatypes(dsl),
    ]
