"""UFO/OntoUML anti-pattern checks over the meta-grounded ontology.

Every GMEOW class records two orthogonal gUFO facets: its **nature** — what an
instance *is* — via ``rdfs:subClassOf`` into the gUFO individual taxonomy
(``gufo:FunctionalComplex``, ``gufo:Relator``, ``gufo:Event`` …); and its
**stereotype** — the type's identity/rigidity status — via ``rdf:type`` into the
gUFO ``gufo:EndurantType`` / ``gufo:EventType`` / ``gufo:SituationType`` taxonomy
(OWL 2 punning; see ``imports/gufo.ttl`` ~line 1427). This module checks the
structural discipline the stereotype facet licenses, exactly the role
:mod:`gmeow_tools.statement_lint` plays for the statement layer and
:mod:`gmeow_tools.projection_lint` for the projection stack — pure rdflib, no Docker.

The checks (each cites the OntoUML anti-pattern it guards; catalogue:
https://ontouml.readthedocs.io/en/latest/anti-patterns/):

* :func:`exactly_one_stereotype` — every GMEOW class carries exactly one gUFO
  meta-class (the precondition for every check below).
* :func:`identity_overlap` — **MixIden**: a sortal inherits identity from exactly
  one ``gufo:Kind``; no ``Kind`` specializes another ``Kind`` (every endurant
  instantiates one and only one kind).
* :func:`anti_rigidity_discipline` — **MixRig / FreeRole**: an anti-rigid sortal
  (``Role`` / ``Phase``) specializes a rigid sortal, and no rigid type specializes
  an anti-rigid one (a rigid type cannot inherit contingent instantiation).
* :func:`relator_mediation` — **RelComp**: a concrete ``gufo:Relator`` mediates at
  least two relata, so phase 2 (#38) can hang ``someValuesFrom`` mediation axioms
  on a relator that actually connects things.
"""

from __future__ import annotations

from rdflib import RDF, RDFS, Graph, URIRef
from rdflib.namespace import OWL, Namespace

from gmeow_tools.config import NAMESPACE, PREFIXES

GUFO = Namespace(PREFIXES["gufo"])

#: OntoUML anti-pattern catalogue, cited in messages so failures self-document.
_CATALOGUE = "https://ontouml.readthedocs.io/en/latest/anti-patterns/"

#: gUFO ``EndurantType`` stereotypes, by sortality and rigidity, for endurants
#: (relators, modes, qualities and quality values are all endurants too).
_ENDURANT_STEREOTYPES: frozenset[URIRef] = frozenset(
    {
        GUFO.Kind,
        GUFO.SubKind,
        GUFO.Phase,
        GUFO.Role,
        GUFO.Category,
        GUFO.Mixin,
        GUFO.RoleMixin,
        GUFO.PhaseMixin,
    }
)
#: gUFO stereotypes for perdurants (no sortality/rigidity split in gUFO).
_PERDURANT_STEREOTYPES: frozenset[URIRef] = frozenset(
    {GUFO.EventType, GUFO.SituationType}
)
#: gUFO stereotype for types whose instances are abstract individuals — value
#: spaces (``gufo:QualityValue``) and temporal regions. ``gufo:Kind`` is wrong for
#: these: a Kind is an ``EndurantType`` (its instances are endurants), whereas a
#: value/interval is a ``gufo:AbstractIndividual``, ``owl:disjointWith`` the concrete.
_ABSTRACT_STEREOTYPES: frozenset[URIRef] = frozenset({GUFO.AbstractIndividualType})
#: The full set of acceptable stereotypes — exactly one per class.
_META_CLASSES: frozenset[URIRef] = (
    _ENDURANT_STEREOTYPES | _PERDURANT_STEREOTYPES | _ABSTRACT_STEREOTYPES
)
#: Rigid sortals supply/inherit a principle of identity.
_RIGID_SORTALS: frozenset[URIRef] = frozenset({GUFO.Kind, GUFO.SubKind})
#: Anti-rigid sortals classify their instances only contingently.
_ANTI_RIGID_SORTALS: frozenset[URIRef] = frozenset({GUFO.Phase, GUFO.Role})
#: Anti-rigid / semi-rigid types a rigid sortal must never specialize.
_ANTI_RIGID_TYPES: frozenset[URIRef] = frozenset(
    {GUFO.Phase, GUFO.Role, GUFO.PhaseMixin, GUFO.RoleMixin, GUFO.Mixin}
)

#: A relator's validity-period scope is not one of its mediated relata.
_TIME_INTERVAL = URIRef(NAMESPACE + "TimeInterval")


def _is_gmeow_class_iri(term: URIRef) -> bool:
    """Whether an IRI is a bare GMEOW vocabulary term (not an instance sub-path)."""
    iri = str(term)
    return iri.startswith(NAMESPACE) and "/" not in iri[len(NAMESPACE) :]


def _gmeow_classes(graph: Graph) -> list[URIRef]:
    """The GMEOW-namespaced ``owl:Class`` vocabulary terms, sorted for stable output."""
    return sorted(
        (
            cls
            for cls in graph.subjects(RDF.type, OWL.Class)
            if isinstance(cls, URIRef) and _is_gmeow_class_iri(cls)
        ),
        key=str,
    )


def _proper_ancestors(graph: Graph, cls: URIRef) -> set[URIRef]:
    """Transitive ``rdfs:subClassOf`` super-classes of ``cls``, excluding itself."""
    return {
        a
        for a in graph.transitive_objects(cls, RDFS.subClassOf)
        if isinstance(a, URIRef) and a != cls
    }


def _stereotypes(graph: Graph, cls: URIRef) -> set[URIRef]:
    """The gUFO meta-classes ``cls`` is punned as, via ``rdf:type``."""
    return {
        t
        for t in graph.objects(cls, RDF.type)
        if isinstance(t, URIRef) and t in _META_CLASSES
    }


def _local(term: URIRef) -> str:
    """Short ``gmeow:Name`` rendering for messages."""
    return (
        "gmeow:" + str(term)[len(NAMESPACE) :]
        if _is_gmeow_class_iri(term)
        else str(term)
    )


def exactly_one_stereotype(graph: Graph) -> list[str]:
    """Every GMEOW class must be punned with exactly one gUFO meta-class."""
    problems: list[str] = []
    for cls in _gmeow_classes(graph):
        stereotypes = _stereotypes(graph, cls)
        if not stereotypes:
            problems.append(
                f"{_local(cls)} carries no gUFO meta-class — pun it with exactly one "
                f"of gufo:Kind/SubKind/Role/Phase/Category/Mixin/RoleMixin/PhaseMixin "
                f"(or gufo:EventType/SituationType for perdurants)"
            )
        elif len(stereotypes) > 1:
            names = ", ".join(sorted(_local(s) for s in stereotypes))
            problems.append(
                f"{_local(cls)} carries conflicting gUFO meta-classes ({names}) — "
                f"a class has exactly one stereotype"
            )
    return problems


def identity_overlap(graph: Graph) -> list[str]:
    """MixIden: a sortal inherits identity from exactly one Kind; no Kind ⊑ Kind."""
    problems: list[str] = []
    for cls in _gmeow_classes(graph):
        stereotypes = _stereotypes(graph, cls)
        ancestors = _proper_ancestors(graph, cls)
        kind_ancestors = sorted(
            (a for a in ancestors if GUFO.Kind in graph.objects(a, RDF.type)), key=str
        )
        if GUFO.Kind in stereotypes and kind_ancestors:
            names = ", ".join(_local(a) for a in kind_ancestors)
            problems.append(
                f"{_local(cls)} is a gufo:Kind but specializes gufo:Kind(s) {names} — "
                f"identity conflict (OntoUML MixIden: every endurant instantiates "
                f"exactly one Kind). See {_CATALOGUE}"
            )
        # A non-Kind sortal must trace to exactly one Kind (OntoUML MixIden).
        if (
            (stereotypes & (_RIGID_SORTALS | _ANTI_RIGID_SORTALS))
            and GUFO.Kind not in stereotypes
            and len(kind_ancestors) != 1
        ):
            names = ", ".join(_local(a) for a in kind_ancestors) or "none"
            problems.append(
                f"{_local(cls)} is a sortal but specializes {len(kind_ancestors)} "
                f"gufo:Kind(s) ({names}) — a sortal inherits identity from exactly "
                f"one Kind (OntoUML MixIden). See {_CATALOGUE}"
            )
    return problems


def anti_rigidity_discipline(graph: Graph) -> list[str]:
    """MixRig / FreeRole: anti-rigid sortals need a rigid super; rigid avoid them."""
    problems: list[str] = []
    for cls in _gmeow_classes(graph):
        stereotypes = _stereotypes(graph, cls)
        ancestor_stereotypes: set[URIRef] = set()
        for ancestor in _proper_ancestors(graph, cls):
            ancestor_stereotypes |= {
                t
                for t in graph.objects(ancestor, RDF.type)
                if isinstance(t, URIRef) and t in _META_CLASSES
            }
        if (stereotypes & _ANTI_RIGID_SORTALS) and not (
            ancestor_stereotypes & _RIGID_SORTALS
        ):
            problems.append(
                f"{_local(cls)} is an anti-rigid sortal (Role/Phase) but specializes "
                f"no rigid sortal — nowhere to inherit a principle of identity "
                f"(OntoUML FreeRole). See {_CATALOGUE}"
            )
        if (stereotypes & _RIGID_SORTALS) and (
            ancestor_stereotypes & _ANTI_RIGID_TYPES
        ):
            bad = ", ".join(
                sorted(_local(s) for s in ancestor_stereotypes & _ANTI_RIGID_TYPES)
            )
            problems.append(
                f"{_local(cls)} is a rigid sortal (Kind/SubKind) but specializes the "
                f"anti-rigid type ({bad}) — a rigid type cannot inherit contingent "
                f"instantiation (OntoUML MixRig). See {_CATALOGUE}"
            )
    return problems


def relator_mediation(graph: Graph) -> list[str]:
    """RelComp: every concrete gufo:Relator mediates at least two relata.

    A relator is concrete when no other GMEOW class specializes it (abstract relator
    bases legitimately defer their mediations to subtypes). Mediation is counted in
    *ends*: each GMEOW object property incident to the relator (or a relator ancestor)
    contributes 1 end if functional, 2 if not — matching OntoUML mediation cardinality,
    where one non-functional property can connect two or more relata. The validity
    link to gmeow:TimeInterval is excluded; it scopes the relator, it is not a relatum.
    """
    gmeow_object_properties = [
        p
        for p in graph.subjects(RDF.type, OWL.ObjectProperty)
        if isinstance(p, URIRef) and str(p).startswith(NAMESPACE)
    ]
    problems: list[str] = []
    for cls in _gmeow_classes(graph):
        if GUFO.Relator not in _proper_ancestors(graph, cls):
            continue
        has_gmeow_subclass = any(
            isinstance(sub, URIRef) and sub != cls and str(sub).startswith(NAMESPACE)
            for sub in graph.subjects(RDFS.subClassOf, cls)
        )
        if has_gmeow_subclass:
            continue  # abstract base — its concrete subtypes carry the mediations
        relator_terms = {cls} | {
            a for a in _proper_ancestors(graph, cls) if str(a).startswith(NAMESPACE)
        }
        ends = 0
        for prop in gmeow_object_properties:
            domains = set(graph.objects(prop, RDFS.domain))
            ranges = set(graph.objects(prop, RDFS.range))
            relata = (ranges if domains & relator_terms else set()) | (
                domains if ranges & relator_terms else set()
            )
            relata -= relator_terms | {_TIME_INTERVAL}
            if not relata:
                continue
            ends += 1 if (prop, RDF.type, OWL.FunctionalProperty) in graph else 2
        if ends < 2:
            problems.append(
                f"{_local(cls)} is a concrete gufo:Relator mediating only {ends} "
                f"end(s) — a relator must mediate at least two (OntoUML RelComp). "
                f"See {_CATALOGUE}"
            )
    return problems


def reasoning_invariants(graph: Graph) -> list[str]:
    """Run every UFO anti-pattern check; an empty list means the graph is clean."""
    return [
        *exactly_one_stereotype(graph),
        *identity_overlap(graph),
        *anti_rigidity_discipline(graph),
        *relator_mediation(graph),
    ]
