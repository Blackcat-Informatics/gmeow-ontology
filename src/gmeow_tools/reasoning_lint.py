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
#: (relators, modes and qualities are all endurants too — but quality *values*
#: are abstract individuals, see _ABSTRACT_STEREOTYPES below).
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
    """Prefixed ``gmeow:`` / ``gufo:`` rendering for messages (full IRI otherwise)."""
    iri = str(term)
    if _is_gmeow_class_iri(term):
        return "gmeow:" + iri[len(NAMESPACE) :]
    if iri.startswith(str(GUFO)):
        return "gufo:" + iri[len(str(GUFO)) :]
    return iri


def exactly_one_stereotype(graph: Graph) -> list[str]:
    """Every GMEOW class must be punned with exactly one gUFO meta-class."""
    problems: list[str] = []
    for cls in _gmeow_classes(graph):
        stereotypes = _stereotypes(graph, cls)
        if not stereotypes:
            problems.append(
                f"{_local(cls)} carries no gUFO meta-class — pun it with exactly one "
                f"of gufo:Kind/SubKind/Role/Phase/Category/Mixin/RoleMixin/PhaseMixin "
                f"(gufo:EventType/SituationType for perdurants, or "
                f"gufo:AbstractIndividualType for abstract individuals)"
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
        if stereotypes & _RIGID_SORTALS:
            # Name the offending ancestor class(es) and their anti-rigid stereotype.
            bad_ancestors = []
            for ancestor in _proper_ancestors(graph, cls):
                bad = _stereotypes(graph, ancestor) & _ANTI_RIGID_TYPES
                if bad:
                    labels = ", ".join(sorted(_local(s) for s in bad))
                    bad_ancestors.append(f"{_local(ancestor)} ({labels})")
            if bad_ancestors:
                names = ", ".join(sorted(bad_ancestors))
                problems.append(
                    f"{_local(cls)} is a rigid sortal (Kind/SubKind) but specializes "
                    f"anti-rigid ancestor(s) {names} — a rigid type cannot inherit "
                    f"contingent instantiation (OntoUML MixRig). See {_CATALOGUE}"
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


def _all_disjoint_member_sets(graph: Graph) -> list[set[URIRef]]:
    """Every ``owl:AllDisjointClasses`` axiom's member set."""
    from rdflib.collection import Collection

    sets: list[set[URIRef]] = []
    for node in graph.subjects(RDF.type, OWL.AllDisjointClasses):
        for head in graph.objects(node, OWL.members):
            sets.append({m for m in Collection(graph, head) if isinstance(m, URIRef)})
    return sets


def coequal_facet_orthogonality(graph: Graph) -> list[str]:
    """Principle 9 by annotation (#281): annotated axes stay orthogonal.

    Every ``gmeow:coequalFacet true`` axis is held orthogonal to every other —
    derived from the annotation set, so a new axis is enforced the moment it
    is declared.

    Per axis: exactly one range, owned by no other axis; never
    ``owl:FunctionalProperty`` (a locked single value invites the sameAs
    collapse Principle 5 forbids). Per pair: no ``rdfs:subPropertyOf`` or
    ``owl:equivalentProperty`` bridge in either direction. Jointly: every axis
    range is a member of one common ``owl:AllDisjointClasses`` axiom.
    """
    coequal = URIRef(NAMESPACE + "coequalFacet")
    axes = sorted(
        s
        for s, o in graph.subject_objects(coequal)
        if isinstance(s, URIRef) and str(o) == "true"
    )
    if not axes:
        return []
    problems: list[str] = []
    ranges: dict[URIRef, URIRef] = {}
    for axis in axes:
        axis_ranges = sorted(
            r for r in graph.objects(axis, RDFS.range) if isinstance(r, URIRef)
        )
        if len(axis_ranges) != 1:
            problems.append(
                f"co-equal facet {_local(axis)} must have exactly one rdfs:range "
                f"(found {len(axis_ranges)}) — each axis owns its own value space"
            )
            continue
        ranges[axis] = axis_ranges[0]
        if (axis, RDF.type, OWL.FunctionalProperty) in graph:
            problems.append(
                f"co-equal facet {_local(axis)} is owl:FunctionalProperty — a "
                f"locked single value contradicts co-equality (P9) and invites "
                f"sameAs collapse (P5)"
            )
    range_owners: dict[URIRef, list[URIRef]] = {}
    for axis, rng in ranges.items():
        range_owners.setdefault(rng, []).append(axis)
    for rng, owners in sorted(range_owners.items()):
        if len(owners) > 1:
            names = ", ".join(_local(a) for a in owners)
            problems.append(
                f"co-equal facets {names} share the range {_local(rng)} — "
                f"axes collapsed into one value space"
            )
    bridges = (RDFS.subPropertyOf, OWL.equivalentProperty)
    for i, a in enumerate(axes):
        for b in axes[i + 1 :]:
            for predicate in bridges:
                if (a, predicate, b) in graph or (b, predicate, a) in graph:
                    problems.append(
                        f"co-equal facets {_local(a)} and {_local(b)} are bridged "
                        f"by {_local(predicate)} — one axis must never be "
                        f"inferred from another"
                    )
    member_sets = _all_disjoint_member_sets(graph)
    range_set = set(ranges.values())
    if len(range_set) > 1 and not any(range_set <= s for s in member_sets):
        names = ", ".join(sorted(_local(r) for r in range_set))
        problems.append(
            f"the co-equal facet ranges ({names}) are not jointly declared in "
            f"one owl:AllDisjointClasses axiom — the orthogonality matrix is "
            f"not ELK-visible"
        )
    return problems


def frame_declaration_completeness(graph: Graph) -> list[str]:
    """Principle 11 by annotation (#283): the "did you forget the frame?" guard.

    Every property declared ``rdfs:subPropertyOf gmeow:hasReferenceFrame``
    points its domain class at a reference frame — so that domain class must
    DECLARE the requirement (``gmeow:requiresFrame`` naming the property),
    which is what the frame-shapes generator turns into SHACL. A
    frame-pointing property whose carrier class is silent means the shape was
    forgotten, the exact failure mode this annotation regime exists to end.
    """
    has_frame = URIRef(NAMESPACE + "hasReferenceFrame")
    requires = URIRef(NAMESPACE + "requiresFrame")
    problems: list[str] = []
    props = sorted(
        p
        for p in graph.subjects(RDFS.subPropertyOf, has_frame)
        if isinstance(p, URIRef)
    )
    for prop in props:
        domains = sorted(
            d for d in graph.objects(prop, RDFS.domain) if isinstance(d, URIRef)
        )
        for domain in domains:
            if (domain, requires, prop) not in graph:
                problems.append(
                    f"{_local(domain)} carries the frame-pointing property "
                    f"{_local(prop)} but declares no gmeow:requiresFrame for "
                    f"it — the frame-relativity shape would be missing (P11)"
                )
    return problems


def reasoning_invariants(graph: Graph) -> list[str]:
    """Run every UFO anti-pattern check; an empty list means the graph is clean."""
    return [
        *exactly_one_stereotype(graph),
        *identity_overlap(graph),
        *anti_rigidity_discipline(graph),
        *relator_mediation(graph),
        *coequal_facet_orthogonality(graph),
        *frame_declaration_completeness(graph),
    ]
