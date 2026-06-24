"""Structural + DL-safety guards for the names building block.

Asserted-TBox invariants are now in slices/core/names/tests/structural.ttl
(47 declarative saPolarity cells run by the Rust slicetest harness).

RETAINED here (not migratable to scopeModule cells):
  test_place_naming_is_defined_class -- traverses OWL intersection lists
    using rdflib Collection; the equivalentClass body is an anonymous blank
    node that cannot be faithfully expressed as a single ASK without
    re-encoding the blank-node restriction inline.
  test_seeded_pronoun_sets_have_five_forms -- iterates over a 21-item
    _DECLINABLE_PRONOUN_ANCHORS tuple with per-anchor label + five-form
    checks; a dynamic numeric sweep over live ABox data.
  test_pronoun_name_only_value_exists -- checks label presence via
    graph.value() and five-form absence on _NON_SPECIFYING_PRONOUNS;
    ABox / numeric live-data checks.
  test_contested_name_usage_coexists -- run_shacl() on a fixture file;
    ExampleConformance, not a structural TBox assertion.
  test_audience_and_standpoint_are_distinct -- checks gmeow:accordingTo,
    which is defined in a cross-slice module (not home-asserted in names);
    the merged-graph sweep ensures neither direction is accidentally wired.

MIGRATED to single cells (coverage preserved in structural.ttl):
  test_no_primary_or_preferred_name_term_exists + the extended variant --
    both banned-term sweeps collapsed into cell 30 (saNoPreferredOrPrimaryNameTerm)
    covering all 9 unique banned IRIs from both functions.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Namespace, URIRef
from gmeow_rdf.compat.rdflib.collection import Collection

from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_place_naming_is_defined_class() -> None:
    """PlaceNaming reuses the NameUsage relator as a DEFINED class
    (≡ NameUsage ⊓ ∃usageNamed.Place) — Principle 6, no parallel relator. This is
    the first owl:equivalentClass defined class in the ontology; the reasoner
    classifies a place-naming, nothing asserts it (see the entailment QC query)."""
    graph = _graph()
    pn = URIRef(GMEOW + "PlaceNaming")
    assert (pn, RDF.type, OWL.Class) in graph
    assert (pn, RDFS.subClassOf, URIRef(GMEOW + "NameUsage")) in graph
    found = False
    for eq in graph.objects(pn, OWL.equivalentClass):
        inter = graph.value(eq, OWL.intersectionOf)
        if inter is None:
            continue
        members = list(Collection(graph, inter))
        has_nameusage = URIRef(GMEOW + "NameUsage") in members
        has_place_restriction = any(
            (m, OWL.onProperty, URIRef(GMEOW + "usageNamed")) in graph
            and (m, OWL.someValuesFrom, URIRef(GMEOW + "Place")) in graph
            for m in members
        )
        if has_nameusage and has_place_restriction:
            found = True
    assert found, "PlaceNaming ≡ NameUsage ⊓ ∃usageNamed.Place must be defined"


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


# --------------------------------------------------------------------------- #
# Standpoint coexistence — contested name usages + audience/standpoint distinction (#51)
# --------------------------------------------------------------------------- #

EX_NAMES = Namespace("https://blackcatinformatics.ca/gmeow/examples/names/")
COVERAGE_FIXTURES = Path(__file__).parent / "fixtures" / "coverage"


def test_contested_name_usage_coexists() -> None:
    """Two standpoint-indexed NameUsage claims on the same person load, SHACL-pass,
    and are BOTH retained — neither is the ground truth."""
    g = Graph().parse(COVERAGE_FIXTURES / "names-contested.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    names = set(g.objects(EX_NAMES.person, URIRef(GMEOW + "hasName")))
    assert {EX_NAMES.chosenName, EX_NAMES.legalName} <= names


def test_audience_and_standpoint_are_distinct() -> None:
    """usageAudience (social scope) is not bridged to accordingTo (standpoint frame).
    The axes are orthogonal. Retained (not migrated) because gmeow:accordingTo
    is a cross-slice subject -- the merged-graph sweep is authoritative here."""
    g = _graph()
    audience = URIRef(GMEOW + "usageAudience")
    according_to = URIRef(GMEOW + "accordingTo")
    assert (audience, RDFS.subPropertyOf, according_to) not in g
    assert (according_to, RDFS.subPropertyOf, audience) not in g
    assert (audience, OWL.equivalentProperty, according_to) not in g


# Retained (cross-slice): these Appellation subclasses / hasAppellation bridges are
# asserted in their home slices (organization/creative-works/agreements/software/
# documents), not the names module, so a module-scoped cell cannot see them (#867).


def test_appellation_umbrella_and_structural_subclasses() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "Appellation"),
        RDFS.subClassOf,
        URIRef(GMEOW + "InformationObject"),
    ) in graph
    for sub in (
        "PersonName",
        "Filename",
        "PlaceName",
        "OrganizationName",
        "CreativeWorkTitle",
        "AgreementName",
        "SoftwareName",
    ):
        assert (
            URIRef(GMEOW + sub),
            RDFS.subClassOf,
            URIRef(GMEOW + "Appellation"),
        ) in graph


def test_has_title_subproperty_of_hasappellation() -> None:
    """hasTitle is the creative-work-scoped specialization of hasAppellation
    (issue #97), giving CreativeWork multilingual Appellation-based titles."""
    graph = _graph()
    ht = URIRef(GMEOW + "hasTitle")
    assert (ht, RDF.type, OWL.ObjectProperty) in graph
    assert (ht, RDFS.subPropertyOf, URIRef(GMEOW + "hasAppellation")) in graph
    assert (ht, RDFS.domain, URIRef(GMEOW + "CreativeWork")) in graph
    assert (ht, RDFS.range, URIRef(GMEOW + "CreativeWorkTitle")) in graph


def test_has_software_name_subproperty_of_hasappellation() -> None:
    """hasSoftwareName is the software-scoped specialization of hasAppellation
    (issue #97), domain-free so it can attach to both SoftwareProject and
    SoftwareProduct (issue #231)."""
    graph = _graph()
    hsn = URIRef(GMEOW + "hasSoftwareName")
    assert (hsn, RDF.type, OWL.ObjectProperty) in graph
    assert (hsn, RDFS.subPropertyOf, URIRef(GMEOW + "hasAppellation")) in graph
    # Domain is inherited from hasAppellation (Entity), not restricted to
    # SoftwareProject, so both projects and products can bear software names.
    assert (hsn, RDFS.domain, URIRef(GMEOW + "SoftwareProject")) not in graph
    assert (hsn, RDFS.range, URIRef(GMEOW + "SoftwareName")) in graph
