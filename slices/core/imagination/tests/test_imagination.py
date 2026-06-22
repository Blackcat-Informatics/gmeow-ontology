# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Imagination slice — the as-if attitude spine and content-origin invariants.

These structural assertions guard the load-bearing shape of the imagination
slice, the third attitude flavour of mind (after epistemics' doxastic spine and
inquiry's erotetic spine):

* The two-verb imagination spine (``imagines`` / ``supposes``) is FLAT
  (Principle 4) and OPEN-RANGE (Principle 13) — domain ``gmeow:Agent``, no
  ``rdfs:range``, neither functional, and NO ``rdfs:subPropertyOf`` between them
  or onto the doxastic spine (``believes`` / ``accepts``): imagining and
  supposing are decoupled from belief and acceptance.
* ``gmeow:ContentOrigin`` is a VALUE VOCABULARY (``gufo:AbstractIndividualType``
  ⊑ ``gufo:QualityValue``): its six members are individuals, never subclasses
  (Principle 9, the ``MentalProcessType`` / ``QuestionType`` idiom).
* ``gmeow:contentOrigin`` keeps an OPEN domain (content may be a claim, a
  proposition, or anything) and is non-functional and vantage-indexed
  (Principle 9). ``gmeow:imaginedWorld`` keeps BOTH ends open — the
  ``logic:World`` target is by reference (Principle 5).
* There is NO reality or truth bit (no ``isReal`` / ``isImaginary`` / ``isTrue``);
  reality-monitoring is a ``contentOrigin`` value, never a flag.
* By-reference discipline: the module asserts NO triple in the ``logic:``
  namespace, and the manifest's ``sliceDependsOn`` is ``kernel`` ALONE.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import Graph, URIRef
from gmeow_rdf.compat.rdflib.namespace import OWL, RDF, RDFS

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"
LOGIC = "https://blackcatinformatics.ca/logic/"
SKOS_DEFINITION = URIRef("http://www.w3.org/2004/02/skos/core#definition")
SLICE_IRI = URIRef("https://blackcatinformatics.ca/gmeow/slices/imagination")
SLICE_DEPENDS_ON = URIRef(GMEOW + "sliceDependsOn")
_MODULE = Path(__file__).resolve().parents[1] / "module.ttl"
_MANIFEST = Path(__file__).resolve().parents[1] / "manifest.ttl"

_SPINE = ("imagines", "supposes")
_ORIGIN_INDIVIDUALS = (
    "originPerceived",
    "originRemembered",
    "originBelieved",
    "originImagined",
    "originSupposed",
    "originGenerated",
)

# Every locally-declared term, by name (11 total): the 2 spine verbs, the
# ContentOrigin value class, its 6 value individuals, and the 2 relational
# properties (contentOrigin / imaginedWorld).
_DECLARED_TERMS = (
    *_SPINE,
    "ContentOrigin",
    *_ORIGIN_INDIVIDUALS,
    "contentOrigin",
    "imaginedWorld",
)


def _t(name: str) -> URIRef:
    """A gmeow-namespaced term URI."""
    return URIRef(GMEOW + name)


def _gufo(name: str) -> URIRef:
    """A gufo-namespaced term URI."""
    return URIRef(GUFO + name)


def _graph() -> Graph:
    g = Graph()
    g.parse(_MODULE, format="turtle")
    return g


def test_spine_are_object_properties_with_agent_domain_open_range() -> None:
    """Each spine verb is an owl:ObjectProperty with rdfs:domain gmeow:Agent, an
    OPEN range (no rdfs:range asserted, Principle 13), and is non-functional —
    an agent imagines and supposes many things at once."""
    g = _graph()
    for prop in _SPINE:
        term = _t(prop)
        assert (term, RDF.type, OWL.ObjectProperty) in g
        assert (term, RDFS.domain, _t("Agent")) in g
        assert (term, RDFS.range, None) not in g
        assert (term, RDF.type, OWL.FunctionalProperty) not in g


def test_spine_is_flat_and_decoupled() -> None:
    """The imagination spine is FLAT (Principle 4): neither verb declares an
    rdfs:subPropertyOf — two distinct attitudes, not a hierarchy, and crucially
    NOT sub-properties of the doxastic spine (believes / accepts), since
    imagining and supposing assert neither truth nor pragmatic commitment."""
    g = _graph()
    for prop in _SPINE:
        assert (_t(prop), RDFS.subPropertyOf, None) not in g


def test_content_origin_is_an_abstract_individual_type() -> None:
    """ContentOrigin is an owl:Class, a logic:AbstractIndividualType, and a
    subclass of logic:QualityValue — the value-vocabulary genus shared with
    gmeow:MentalProcessType and gmeow:QuestionType.
    After #694 migration: stereotype namespace is logic: not gufo:."""
    g = _graph()
    co = _t("ContentOrigin")
    assert (co, RDF.type, OWL.Class) in g
    assert (co, RDF.type, URIRef(LOGIC + "AbstractIndividualType")) in g
    assert (co, RDFS.subClassOf, URIRef(LOGIC + "QualityValue")) in g


def test_content_origin_individuals_are_seeded() -> None:
    """The six content-origin values are individuals of gmeow:ContentOrigin (a
    closed-but-open value vocabulary)."""
    g = _graph()
    for indiv in _ORIGIN_INDIVIDUALS:
        assert (_t(indiv), RDF.type, _t("ContentOrigin")) in g


def test_content_origin_individuals_are_not_subclasses() -> None:
    """Value-vocab discipline (Principle 9, no overtyping): each origin value is
    an INDIVIDUAL, never a subclass of gmeow:ContentOrigin or an owl:Class."""
    g = _graph()
    for indiv in _ORIGIN_INDIVIDUALS:
        term = _t(indiv)
        assert (term, RDFS.subClassOf, _t("ContentOrigin")) not in g
        assert (term, RDF.type, OWL.Class) not in g


def test_content_origin_property_open_domain() -> None:
    """contentOrigin is an owl:ObjectProperty ranging over gmeow:ContentOrigin
    with an OPEN domain (no rdfs:domain — content may be a claim, a proposition,
    or anything) and is non-functional and vantage-indexed (Principle 9): an
    agent's and an auditor's attributions coexist."""
    g = _graph()
    co = _t("contentOrigin")
    assert (co, RDF.type, OWL.ObjectProperty) in g
    assert (co, RDFS.range, _t("ContentOrigin")) in g
    assert (co, RDFS.domain, None) not in g
    assert (co, RDF.type, OWL.FunctionalProperty) not in g


def test_imagined_world_open_domain_and_range() -> None:
    """imaginedWorld is an owl:ObjectProperty with BOTH ends OPEN (no rdfs:domain
    and no rdfs:range): the logic:World target is carried by reference
    (Principle 5), and it is non-functional (one rehearsal may branch into
    several worlds)."""
    g = _graph()
    iw = _t("imaginedWorld")
    assert (iw, RDF.type, OWL.ObjectProperty) in g
    assert (iw, RDFS.domain, None) not in g
    assert (iw, RDFS.range, None) not in g
    assert (iw, RDF.type, OWL.FunctionalProperty) not in g


def test_no_reality_or_truth_bit() -> None:
    """No reality or truth bit: reality-monitoring is a gmeow:contentOrigin value
    (Principle 9), so none of isReal / isImaginary / isGenerated / isFake /
    isTrue / isFalse appears in ANY triple position."""
    g = _graph()
    for name in ("isReal", "isImaginary", "isGenerated", "isFake", "isTrue", "isFalse"):
        term = _t(name)
        assert (term, None, None) not in g
        assert (None, term, None) not in g
        assert (None, None, term) not in g


def test_no_new_content_class() -> None:
    """The slice mints NO new content-mode class (the attitude verbs plus the
    originImagined / originSupposed markers carry the as-if): no gmeow:Supposition
    / gmeow:ImaginedContent / gmeow:ImaginativeContent is declared."""
    g = _graph()
    for name in ("Supposition", "ImaginedContent", "ImaginativeContent"):
        assert (_t(name), RDF.type, OWL.Class) not in g


def test_by_reference_no_logic_triples() -> None:
    """By-reference discipline (Principle 5): the module asserts NO triple whose
    subject, predicate, or object lives in the logic: namespace, EXCEPT for the
    stereotype vocabulary introduced by the #694 migration.

    Stereotype triples (logic:Kind, logic:AbstractIndividualType, logic:QualityValue,
    etc.) are identity/rigidity typing, NOT modal-world dependencies — they replace
    the former gufo: stereotype namespace and are explicitly permitted.  logic:World
    and any other non-stereotype logic: terms remain prose-only."""
    # The set of logic: stereotype IRIs that are explicitly allowed after the
    # #694 gufo→logic migration.  Any logic: node NOT in this set is still
    # forbidden (worlds, modal-logic terms, etc. stay prose-only).
    _logic_stereotypes = frozenset(
        URIRef(LOGIC + n)
        for n in (
            "Kind",
            "SubKind",
            "Phase",
            "Role",
            "Category",
            "Mixin",
            "RoleMixin",
            "PhaseMixin",
            "Event",
            "Situation",
            "AbstractIndividualType",
            "Relator",
            "QualityValue",
            "Mode",
            "Disposition",
        )
    )
    g = _graph()
    for s, p, o in g:
        for node in (s, p, o):
            if isinstance(node, URIRef) and str(node).startswith(LOGIC):
                assert node in _logic_stereotypes, (
                    f"non-stereotype logic: triple via {node} — "
                    "only stereotype vocabulary is permitted; "
                    "logic:World and modal-logic terms must stay prose-only"
                )


def test_manifest_depends_only_on_kernel() -> None:
    """Manifest dependency hygiene (no over-declaration): the single asserted
    foreign IRI is gmeow:Agent (domain of the spine), so sliceDependsOn lists
    KERNEL ALONE — mentation / logic / deception / epistemics are consumed by
    reference, never declared."""
    g = Graph()
    g.parse(_MANIFEST, format="turtle")
    deps = set(g.objects(SLICE_IRI, SLICE_DEPENDS_ON))
    assert deps == {URIRef(GMEOW + "slices/kernel")}, f"unexpected deps: {deps}"


def test_every_declared_term_is_annotated() -> None:
    """Annotation-completeness (Principle 8): each of the 11 locally-declared
    terms — the 2 spine verbs, the ContentOrigin class, its 6 value individuals,
    and the 2 properties — carries an rdfs:label, a skos:definition, and
    rdfs:isDefinedBy the imagination slice IRI."""
    g = _graph()
    assert len(_DECLARED_TERMS) == 11
    for name in _DECLARED_TERMS:
        term = _t(name)
        assert (term, RDFS.label, None) in g, f"{name} missing rdfs:label"
        assert (term, SKOS_DEFINITION, None) in g, f"{name} missing skos:definition"
        assert (term, RDFS.isDefinedBy, SLICE_IRI) in g, (
            f"{name} missing rdfs:isDefinedBy slice IRI"
        )
