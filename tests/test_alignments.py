"""Tests for the canonical-term alignments (the superset mechanism)."""

from __future__ import annotations

from rdflib import Graph, URIRef
from rdflib.namespace import OWL, SKOS

from gmeow_tools.config import ALIGNMENT_TARGETS, LinkPolicy
from gmeow_tools.mappings import build_alignment_graph, load_mappings

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    return build_alignment_graph(load_mappings())


def test_person_unifies_across_vocabularies() -> None:
    graph = _graph()
    person = URIRef(GMEOW + "Person")
    equivalents = {str(o) for o in graph.objects(person, OWL.equivalentClass)}
    assert "http://xmlns.com/foaf/0.1/Person" in equivalents
    assert "https://schema.org/Person" in equivalents
    assert "http://www.w3.org/2000/10/swap/pim/gedcom#Individual" in equivalents


def test_software_project_aligned_to_doap() -> None:
    """SoftwareProject is now under Project (not Work) so DOAP's conflation is
    a lossy closeMatch, not equivalence (Principle 6 — greenfield de-conflation)."""
    graph = _graph()
    assert (
        URIRef(GMEOW + "SoftwareProject"),
        SKOS.closeMatch,
        URIRef("http://usefulinc.com/ns/doap#Project"),
    ) in graph


def test_kinship_property_alignment() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "hasParent"),
        OWL.equivalentProperty,
        URIRef("http://purl.org/vocab/relationship/childOf"),
    ) in graph


def test_event_types_aligned_to_bio() -> None:
    # Event types are now value individuals (#41) → value↔class skos:closeMatch.
    graph = _graph()
    assert (
        URIRef(GMEOW + "eventTypeBirth"),
        SKOS.closeMatch,
        URIRef("http://purl.org/vocab/bio/0.1/Birth"),
    ) in graph


def test_parentchild_relationship_typed() -> None:
    # The reified parent-child relationship aligns to the GEDCOM X type.
    from rdflib.namespace import SKOS

    graph = _graph()
    assert (
        URIRef(GMEOW + "BiologicalParentChild"),
        SKOS.closeMatch,
        URIRef("http://gedcomx.org/BiologicalParent"),
    ) in graph


def test_email_message_equivalent_to_schema() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "EmailMessage"),
        OWL.equivalentClass,
        URIRef("https://schema.org/EmailMessage"),
    ) in graph


def test_email_participants_aligned_to_schema() -> None:
    from rdflib.namespace import SKOS

    graph = _graph()
    # The author/recipient role properties close-match their schema.org peers.
    assert (
        URIRef(GMEOW + "from"),
        SKOS.closeMatch,
        URIRef("https://schema.org/sender"),
    ) in graph
    assert (
        URIRef(GMEOW + "to"),
        SKOS.closeMatch,
        URIRef("https://schema.org/toRecipient"),
    ) in graph


def test_trust_aligned_to_wot_schema() -> None:
    from rdflib.namespace import SKOS

    graph = _graph()
    assert (
        URIRef(GMEOW + "CryptographicKey"),
        SKOS.closeMatch,
        URIRef("http://xmlns.com/wot/0.1/PubKey"),
    ) in graph
    assert (
        URIRef(GMEOW + "fingerprint"),
        SKOS.closeMatch,
        URIRef("http://xmlns.com/wot/0.1/fingerprint"),
    ) in graph


def test_relationships_aligned_to_rel_vocab() -> None:
    from rdflib.namespace import SKOS

    graph = _graph()
    assert (
        URIRef(GMEOW + "hasMet"),
        SKOS.closeMatch,
        URIRef("http://purl.org/vocab/relationship/hasMet"),
    ) in graph


def test_wot_is_reference_only() -> None:
    # The WOT schema's license is unknown → fails safe to reference-only (linked,
    # never imported).
    assert ALIGNMENT_TARGETS["wot"].policy is LinkPolicy.REFERENCE_ONLY


def test_import_provenance_aligned_to_prov_and_dcterms() -> None:
    from rdflib.namespace import SKOS

    graph = _graph()
    assert (
        URIRef(GMEOW + "sourceModifiedAt"),
        SKOS.closeMatch,
        URIRef("http://purl.org/dc/terms/modified"),
    ) in graph
    assert (
        URIRef(GMEOW + "assertedAt"),
        SKOS.closeMatch,
        URIRef("http://www.w3.org/ns/prov#generatedAtTime"),
    ) in graph


def test_location_aligned_across_geo_vocabularies() -> None:
    from rdflib.namespace import SKOS

    graph = _graph()
    assert (
        URIRef(GMEOW + "VirtualLocation"),
        SKOS.closeMatch,
        URIRef("https://schema.org/VirtualLocation"),
    ) in graph
    assert (
        URIRef(GMEOW + "Geometry"),
        SKOS.closeMatch,
        URIRef("http://www.opengis.net/ont/geosparql#Geometry"),
    ) in graph
    # Containment round-trips with every geo vocab's "within"/parent relation.
    contained = URIRef(GMEOW + "containedInPlace")
    assert (
        contained,
        SKOS.closeMatch,
        URIRef("http://www.opengis.net/ont/geosparql#sfWithin"),
    ) in graph
    assert (
        contained,
        SKOS.closeMatch,
        URIRef("http://www.wikidata.org/prop/direct/P131"),
    ) in graph
    # Inverse containment aligned to GeoSPARQL sfContains (#101).
    contains = URIRef(GMEOW + "containsPlace")
    assert (
        contains,
        SKOS.closeMatch,
        URIRef("http://www.opengis.net/ont/geosparql#sfContains"),
    ) in graph
    # Address component equivalence + WGS84 coordinate alignment.
    assert (
        URIRef(GMEOW + "streetAddress"),
        OWL.equivalentProperty,
        URIRef("https://schema.org/streetAddress"),
    ) in graph
    assert (
        URIRef(GMEOW + "latitude"),
        SKOS.closeMatch,
        URIRef("http://www.w3.org/2003/01/geo/wgs84_pos#lat"),
    ) in graph
    assert (
        URIRef(GMEOW + "timezone"),
        SKOS.closeMatch,
        URIRef("http://www.w3.org/2006/vcard/ns#tz"),
    ) in graph


def test_universal_mereology_aligned_across_foundational_and_web_vocabularies() -> None:
    graph = _graph()
    part = URIRef(GMEOW + "partOf")
    has_part = URIRef(GMEOW + "hasPart")
    expected = {
        (part, SKOS.closeMatch, URIRef("http://purl.obolibrary.org/obo/BFO_0000050")),
        (
            has_part,
            SKOS.closeMatch,
            URIRef("http://purl.obolibrary.org/obo/BFO_0000051"),
        ),
        (part, SKOS.closeMatch, URIRef("http://purl.org/nemo/gufo#isComponentOf")),
        (
            part,
            SKOS.closeMatch,
            URIRef("http://purl.org/nemo/gufo#TemporaryParthoodSituation"),
        ),
        (part, SKOS.closeMatch, URIRef("https://schema.org/isPartOf")),
        (has_part, SKOS.closeMatch, URIRef("https://schema.org/hasPart")),
        (part, SKOS.closeMatch, URIRef("http://purl.org/dc/terms/isPartOf")),
        (has_part, SKOS.closeMatch, URIRef("http://purl.org/dc/terms/hasPart")),
        (
            part,
            SKOS.closeMatch,
            URIRef("http://www.cidoc-crm.org/cidoc-crm/P46i_forms_part_of"),
        ),
        (
            has_part,
            SKOS.closeMatch,
            URIRef("http://www.cidoc-crm.org/cidoc-crm/P46_is_composed_of"),
        ),
    }
    for triple in expected:
        assert triple in graph


def test_geo_authority_targets_are_import_ok() -> None:
    # Getty TGN/GVP (ODC-BY) and WGS84 (W3C) are link-and-copy-permitted.
    for key in ("tgn", "wgs84", "gvp"):
        assert ALIGNMENT_TARGETS[key].policy is LinkPolicy.IMPORT_OK


def test_schema_is_reference_only() -> None:
    # schema.org (CC-BY-SA) may be linked but never imported.
    assert ALIGNMENT_TARGETS["schema"].policy is LinkPolicy.REFERENCE_ONLY


def test_qb_alignments_present() -> None:
    from rdflib.namespace import SKOS

    graph = _graph()
    assert (
        URIRef(GMEOW + "SpatialAggregation"),
        SKOS.closeMatch,
        URIRef("http://purl.org/linked-data/cube#Observation"),
    ) in graph
    assert (
        URIRef(GMEOW + "Dataset"),
        SKOS.closeMatch,
        URIRef("http://purl.org/linked-data/cube#DataSet"),
    ) in graph
    assert (
        URIRef(GMEOW + "AggregationFunction"),
        SKOS.closeMatch,
        URIRef("http://purl.org/linked-data/cube#MeasureProperty"),
    ) in graph
    # Place is intentionally NOT aligned to qb:DimensionProperty because Place is
    # an object class (spatial features) whereas qb:DimensionProperty is a
    # metaclass (class of properties). See mapping-dsl/equivalences/aggregation.ttl.


def test_all_mappings_expand() -> None:
    # No CURIE in any mapping row fails to expand (would raise MappingError).
    graph = _graph()
    assert len(graph) >= 80


def test_quantity_aligned_to_qudt_quantity_value() -> None:
    """gmeow:Quantity maps to qudt:QuantityValue (#77)."""
    from rdflib.namespace import SKOS

    graph = _graph()
    assert (
        URIRef(GMEOW + "Quantity"),
        SKOS.closeMatch,
        URIRef("http://qudt.org/schema/qudt/QuantityValue"),
    ) in graph


def test_quantity_value_aligned_to_qudt() -> None:
    """gmeow:quantityValue maps to qudt:quantityValue (#77)."""
    from rdflib.namespace import SKOS

    graph = _graph()
    assert (
        URIRef(GMEOW + "quantityValue"),
        SKOS.closeMatch,
        URIRef("http://qudt.org/schema/qudt/quantityValue"),
    ) in graph


def test_quantity_uncertainty_aligned_to_qudt() -> None:
    """gmeow:quantityUncertainty maps to qudt:standardUncertainty (#77)."""
    from rdflib.namespace import SKOS

    graph = _graph()
    assert (
        URIRef(GMEOW + "quantityUncertainty"),
        SKOS.closeMatch,
        URIRef("http://qudt.org/schema/qudt/standardUncertainty"),
    ) in graph


def test_deception_event_type_aligned_to_wikidata() -> None:
    """gmeow:eventTypeDeception maps to wd:Q170028 (#213)."""
    from rdflib.namespace import SKOS

    graph = _graph()
    assert (
        URIRef(GMEOW + "eventTypeDeception"),
        SKOS.closeMatch,
        URIRef("http://www.wikidata.org/entity/Q170028"),
    ) in graph


def test_claim_review_aligned_to_attestation() -> None:
    """schema:ClaimReview maps to gmeow:Attestation as lossy projection (#213)."""
    from rdflib.namespace import SKOS

    graph = _graph()
    assert (
        URIRef("https://schema.org/ClaimReview"),
        SKOS.relatedMatch,
        URIRef(GMEOW + "Attestation"),
    ) in graph


def test_rating_aligned_to_verification_result() -> None:
    """schema:Rating maps to gmeow:VerificationResult (#213)."""
    from rdflib.namespace import SKOS

    graph = _graph()
    assert (
        URIRef("https://schema.org/Rating"),
        SKOS.closeMatch,
        URIRef(GMEOW + "VerificationResult"),
    ) in graph


def test_bullshit_modality_aligned_to_crminf() -> None:
    """gmeow:bullshit extends CRMinf I6_Belief_Value (#213)."""
    from rdflib.namespace import SKOS

    graph = _graph()
    assert (
        URIRef(GMEOW + "bullshit"),
        SKOS.relatedMatch,
        URIRef("http://www.ics.forth.gr/isl/CRMinf/I6_Belief_Value"),
    ) in graph
