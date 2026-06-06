"""Structural + DL-safety guards for the names building block.

Pins the universal Appellation hierarchy, the reified NameUsage relator and its
functional roles, the value-vs-subclass decisions (name-part kinds, purposes,
registers, pronoun sets and honorifics are value vocabularies), the
claim-vs-reality filename pattern, and the inclusivity / anti-colonial
invariants: pronouns and honorifics are never tied to sex, and there is NO
preferred/primary/canonical-name term (co-equality is structural).
"""

from __future__ import annotations

from rdflib import OWL, RDF, RDFS, Graph, URIRef
from rdflib.namespace import XSD

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_appellation_umbrella_and_structural_subclasses() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "Appellation"),
        RDFS.subClassOf,
        URIRef(GMEOW + "InformationObject"),
    ) in graph
    for sub in ("PersonName", "Filename", "PlaceName", "OrganizationName"):
        assert (
            URIRef(GMEOW + sub),
            RDFS.subClassOf,
            URIRef(GMEOW + "Appellation"),
        ) in graph


def test_name_usage_is_a_relator() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "NameUsage"),
        RDFS.subClassOf,
        URIRef(GUFO + "Relator"),
    ) in graph


def test_name_usage_roles_functionality() -> None:
    graph = _graph()
    # Constitutive roles are functional (one usage = one named/appellation/scope).
    for prop in (
        "usageNamed",
        "usageAppellation",
        "usageRelationshipScope",
        "usageRegister",
    ):
        node = URIRef(GMEOW + prop)
        assert (node, RDF.type, OWL.ObjectProperty) in graph
        assert (node, RDF.type, OWL.FunctionalProperty) in graph
    # Namer and audience are shared/perspectival — NOT functional.
    for prop in ("usageNamer", "usageAudience"):
        node = URIRef(GMEOW + prop)
        assert (node, RDF.type, OWL.ObjectProperty) in graph
        assert (node, RDF.type, OWL.FunctionalProperty) not in graph


def test_hasname_subproperty_of_hasappellation() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "hasName"),
        RDFS.subPropertyOf,
        URIRef(GMEOW + "hasAppellation"),
    ) in graph
    # And hasName was re-homed from genealogy with PersonName as its range.
    assert (
        URIRef(GMEOW + "hasName"),
        RDFS.range,
        URIRef(GMEOW + "PersonName"),
    ) in graph


def test_name_part_kinds_are_values_not_subclasses() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "NamePartType"),
        RDFS.subClassOf,
        URIRef(GUFO + "QualityValue"),
    ) in graph
    part_type = URIRef(GMEOW + "namePartType")
    assert (part_type, RDF.type, OWL.ObjectProperty) in graph
    # Functional: a single part has exactly one kind.
    assert (part_type, RDF.type, OWL.FunctionalProperty) in graph
    # Multi-cultural value individuals exist...
    for ind in (
        "namePartGiven",
        "namePartSurname",
        "namePartPatronymic",
        "namePartNisba",
        "namePartMononym",
        "namePartPaternalSurname",
        "namePartExtension",
    ):
        assert (
            URIRef(GMEOW + ind),
            RDF.type,
            URIRef(GMEOW + "NamePartType"),
        ) in graph
    # ...and the rejected per-kind subclasses must NOT exist as classes.
    for rejected in ("GivenName", "Surname", "Patronymic", "Mononym"):
        assert (URIRef(GMEOW + rejected), RDF.type, OWL.Class) not in graph


def test_purpose_register_honorific_pronoun_are_value_vocabularies() -> None:
    graph = _graph()
    for vocab, sample in (
        ("NamePurpose", ("namePurposeLegal", "namePurposeChosen")),
        ("NameRegister", ("registerFormal", "registerIntimate")),
        ("Honorific", ("honorificMx", "honorificDr", "honorificSan")),
        (
            "PronounSet",
            (
                "pronounSheHer",
                "pronounTheyThem",
                "pronounXeXem",
                "pronounFaeFaer",
                "pronounZeZir",
            ),
        ),
    ):
        # PronounSet is a STRUCTURED information artifact (five pronoun forms), so it
        # stays an InformationObject; the flat value vocabularies are abstract value
        # spaces (gufo:QualityValue).
        if vocab == "PronounSet":
            parent = URIRef(GMEOW + "InformationObject")
        else:
            parent = URIRef(GUFO + "QualityValue")
        assert (URIRef(GMEOW + vocab), RDFS.subClassOf, parent) in graph
        for ind in sample:
            assert (URIRef(GMEOW + ind), RDF.type, URIRef(GMEOW + vocab)) in graph


def test_custom_pronoun_set_has_five_forms() -> None:
    graph = _graph()
    for form in (
        "pronounSubject",
        "pronounObject",
        "pronounPossessiveDeterminer",
        "pronounPossessive",
        "pronounReflexive",
    ):
        node = URIRef(GMEOW + form)
        assert (node, RDF.type, OWL.DatatypeProperty) in graph
        assert (node, RDF.type, OWL.FunctionalProperty) in graph
    # A named set fills ALL five forms (they/them).
    they = URIRef(GMEOW + "pronounTheyThem")
    for form in (
        "pronounSubject",
        "pronounObject",
        "pronounPossessiveDeterminer",
        "pronounPossessive",
        "pronounReflexive",
    ):
        assert graph.value(they, URIRef(GMEOW + form)) is not None


#: The maximal source-cited anchor inventory of stably-declinable English pronoun
#: sets (declensions verified against the pronouns.page structured database). Each
#: MUST carry all five functional forms; the seed list is anchors, not a fence.
_DECLINABLE_PRONOUN_ANCHORS = (
    "pronounSheHer",
    "pronounHeHim",
    "pronounTheyThem",
    "pronounItIts",
    "pronounXeXem",
    "pronounZeHir",
    "pronounEyEm",
    "pronounEEm",
    "pronounZeZir",
    "pronounFaeFaer",
    "pronounAeAer",
    "pronounVeVer",
    "pronounViVir",
    "pronounPerPer",
    "pronounNeNem",
    "pronounThonThon",
    "pronounCoCos",
    "pronounHuHum",
    "pronounKiKin",
    "pronounZheZher",
    "pronounOneOne",
)

#: Non-specifying values — they assert a stance, not a declension, so they carry no
#: five forms by design (mirrors the existing pronounAny / pronounAsk anchors).
_NON_SPECIFYING_PRONOUNS = ("pronounAny", "pronounAsk", "pronounNameOnly")


def test_seeded_pronoun_sets_have_five_forms() -> None:
    """Every declinable anchor is a PronounSet filling ALL five forms (maximal,
    source-cited coverage — issue #46)."""
    graph = _graph()
    forms = (
        "pronounSubject",
        "pronounObject",
        "pronounPossessiveDeterminer",
        "pronounPossessive",
        "pronounReflexive",
    )
    pronoun_set = URIRef(GMEOW + "PronounSet")
    for anchor in _DECLINABLE_PRONOUN_ANCHORS:
        node = URIRef(GMEOW + anchor)
        assert (node, RDF.type, pronoun_set) in graph, f"{anchor} is not a PronounSet"
        assert graph.value(node, RDFS.label) is not None, f"{anchor} lacks a label"
        for form in forms:
            assert graph.value(node, URIRef(GMEOW + form)) is not None, (
                f"{anchor} is missing {form}"
            )


def test_pronoun_name_only_value_exists() -> None:
    """An explicit no-pronouns / name-only value exists, distinct from any/ask and
    carrying no five forms by design (issue #46, acceptance 2)."""
    graph = _graph()
    name_only = URIRef(GMEOW + "pronounNameOnly")
    assert (name_only, RDF.type, URIRef(GMEOW + "PronounSet")) in graph
    assert graph.value(name_only, RDFS.label) is not None
    # Distinct individuals — not collapsed onto pronounAny / pronounAsk.
    assert name_only != URIRef(GMEOW + "pronounAny")
    assert name_only != URIRef(GMEOW + "pronounAsk")
    # No declined forms (it asserts the ABSENCE of a pronoun set).
    for value in _NON_SPECIFYING_PRONOUNS:
        node = URIRef(GMEOW + value)
        assert (node, RDF.type, URIRef(GMEOW + "PronounSet")) in graph
        assert graph.value(node, URIRef(GMEOW + "pronounSubject")) is None


def test_pronouns_and_honorifics_are_address_value_facets() -> None:
    """Pronouns/honorifics are forms of ADDRESS over their own value vocabularies.

    The cross-axis independence (address is never inferred from gender/sex/
    orientation) is asserted in test_identity_orthogonality.py; the removed
    gmeow:sex literal is replaced by the gender + sexuality modules.
    """
    graph = _graph()
    for facet in ("hasPronounSet", "honorific"):
        node = URIRef(GMEOW + facet)
        # The facet ranges over its own value vocabulary, not a sex/gender value.
        ranges = set(graph.objects(node, RDFS.range))
        expected = "PronounSet" if facet == "hasPronounSet" else "Honorific"
        assert URIRef(GMEOW + expected) in ranges


def test_no_primary_or_preferred_name_term_exists() -> None:
    """Anti-colonial tenet: co-equality is structural — no primary-name marker."""
    graph = _graph()
    property_types = (OWL.ObjectProperty, OWL.DatatypeProperty, OWL.AnnotationProperty)
    for banned in (
        "preferredForDisplay",
        "primaryName",
        "canonicalName",
        "isPrimaryName",
        "preferredName",
    ):
        node = URIRef(GMEOW + banned)
        for pt in property_types:
            assert (node, RDF.type, pt) not in graph, f"{banned} must not be defined"
        assert (node, RDF.type, OWL.Class) not in graph


def test_displayable_is_the_only_display_control() -> None:
    graph = _graph()
    displayable = URIRef(GMEOW + "displayable")
    assert (displayable, RDF.type, OWL.DatatypeProperty) in graph
    # NOT functional: multi-source true/false claims must coexist rather than
    # force a global OWL inconsistency (the repo's conflicts-coexist stance).
    assert (displayable, RDF.type, OWL.FunctionalProperty) not in graph
    assert (displayable, RDFS.range, XSD.boolean) in graph


def test_filename_claim_vs_reality_no_contradiction() -> None:
    graph = _graph()
    for prop, domain in (
        ("claimedMediaType", "Filename"),
        ("detectedMediaType", "InformationObject"),
    ):
        node = URIRef(GMEOW + prop)
        assert (node, RDF.type, OWL.DatatypeProperty) in graph
        # Non-functional: a mismatch is coexisting claims, not an inconsistency.
        assert (node, RDF.type, OWL.FunctionalProperty) not in graph
        assert (node, RDFS.domain, URIRef(GMEOW + domain)) in graph
    # No disjointness wired between the two claim properties.
    assert (
        URIRef(GMEOW + "claimedMediaType"),
        OWL.propertyDisjointWith,
        URIRef(GMEOW + "detectedMediaType"),
    ) not in graph


def test_no_flat_name_part_properties() -> None:
    """Greenfield: name components are ALWAYS the typed gmeow:NamePart — the flat
    literal duplicates (givenName/familyName/givenNamePart/surnamePart) and the
    deprecated nameType are removed; a 'First Last' rendering is a projection."""
    graph = _graph()
    for removed in (
        "givenName",
        "familyName",
        "givenNamePart",
        "surnamePart",
        "nameType",
    ):
        node = URIRef(GMEOW + removed)
        assert (node, RDF.type, OWL.DatatypeProperty) not in graph
        assert (node, RDF.type, OWL.ObjectProperty) not in graph


def test_personname_no_longer_double_defined_in_genealogy() -> None:
    # PersonName moved to names.ttl; it must be declared exactly once (no dup).
    graph = _graph()
    person_name = URIRef(GMEOW + "PersonName")
    assert (person_name, RDF.type, OWL.Class) in graph
    parents = set(graph.objects(person_name, RDFS.subClassOf))
    # Re-homed under Appellation, NOT under InformationObject directly anymore.
    assert URIRef(GMEOW + "Appellation") in parents
    assert URIRef(GMEOW + "InformationObject") not in parents
