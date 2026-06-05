"""Tests that the competency and QC SPARQL queries behave as expected."""

from __future__ import annotations

from rdflib.query import ResultRow

from gmeow_tools.config import COMPETENCY_DIR, NAMESPACE, QC_DIR
from gmeow_tools.graph import load_merged_graph


def test_competency_agents_query() -> None:
    graph = load_merged_graph(include_imports=False)
    query = (COMPETENCY_DIR / "agents.rq").read_text(encoding="utf-8")
    results: set[str] = set()
    for row in graph.query(query):
        assert isinstance(row, ResultRow)
        results.add(str(row[0]))
    # Agent and its skeleton subclasses must be returned.
    for term in ("Agent", "Person", "Organization"):
        assert NAMESPACE + term in results


def _query_terms(filename: str) -> set[str]:
    graph = load_merged_graph(include_imports=False)
    query = (COMPETENCY_DIR / filename).read_text(encoding="utf-8")
    terms: set[str] = set()
    for row in graph.query(query):
        assert isinstance(row, ResultRow)
        terms.add(str(row[0]))
    return terms


def test_competency_works_query() -> None:
    terms = _query_terms("works.rq")
    for term in ("CreativeWork", "Article", "Patent", "Dataset", "SoftwareProject"):
        assert NAMESPACE + term in terms


def test_competency_kinship_query() -> None:
    terms = _query_terms("kinship.rq")
    for term in ("hasParent", "hasChild", "hasSpouse", "hasSibling"):
        assert NAMESPACE + term in terms


def test_competency_life_events_query() -> None:
    terms = _query_terms("life-events.rq")
    # A comprehensive genealogy slice models many life-event types.
    for term in ("Birth", "Death", "Marriage", "Burial", "Census", "Adoption"):
        assert NAMESPACE + term in terms
    assert len(terms) >= 25


def test_competency_email_participants_query() -> None:
    terms = _query_terms("email-participants.rq")
    # Every RFC 5322 role property routes through the EmailAddress seam.
    for term in ("from", "sender", "replyTo", "to", "cc", "bcc"):
        assert NAMESPACE + term in terms


def test_competency_message_trust_query() -> None:
    terms = _query_terms("message-trust.rq")
    for term in (
        "CryptographicSignature",
        "DKIMSignature",
        "SMIMESignature",
        "PGPSignature",
    ):
        assert NAMESPACE + term in terms


def test_competency_interpersonal_relationships_query() -> None:
    terms = _query_terms("interpersonal-relationships.rq")
    for term in (
        "InterpersonalRelationship",
        "ProfessionalRelationship",
        "AcquaintanceRelationship",
    ):
        assert NAMESPACE + term in terms


def test_competency_key_schemes_query() -> None:
    terms = _query_terms("key-schemes.rq")
    for term in ("keySchemePGP", "keySchemeX509", "keySchemeSSH", "keySchemeNostr"):
        assert NAMESPACE + term in terms


def test_competency_key_certifications_query() -> None:
    terms = _query_terms("key-certifications.rq")
    for term in ("certifier", "certifiedKey", "certifiedIdentity"):
        assert NAMESPACE + term in terms


def test_competency_trust_assertions_query() -> None:
    terms = _query_terms("trust-assertions.rq")
    for term in ("trustor", "trustee", "trustLevel", "introducerDepth"):
        assert NAMESPACE + term in terms


def test_competency_import_provenance_query() -> None:
    terms = _query_terms("import-provenance.rq")
    for term in ("sourceModifiedAt", "contentDigest", "sourceLocation"):
        assert NAMESPACE + term in terms


def test_competency_temporal_provenance_clocks_query() -> None:
    terms = _query_terms("temporal-provenance-clocks.rq")
    for term in ("validFrom", "validUntil", "assertedAt", "recordedNoLaterThan"):
        assert NAMESPACE + term in terms


def test_competency_location_kinds_query() -> None:
    terms = _query_terms("location-kinds.rq")
    for term in ("Location", "Place", "VirtualLocation", "StorageLocation"):
        assert NAMESPACE + term in terms


def test_competency_place_types_query() -> None:
    terms = _query_terms("place-types.rq")
    for term in (
        "placeTypeCountry",
        "placeTypeCity",
        "placeTypeRoom",
        "placeTypePremises",
    ):
        assert NAMESPACE + term in terms


def test_competency_storage_media_query() -> None:
    terms = _query_terms("storage-media.rq")
    for term in ("storageMediumCloudService", "storageMediumPhysicalDisk"):
        assert NAMESPACE + term in terms


def test_competency_place_properties_query() -> None:
    terms = _query_terms("place-properties.rq")
    for term in ("containedInPlace", "hasCoordinates", "hasGeometry", "placeType"):
        assert NAMESPACE + term in terms


def test_competency_appellation_kinds_query() -> None:
    terms = _query_terms("appellation-kinds.rq")
    for term in (
        "Appellation",
        "PersonName",
        "Filename",
        "PlaceName",
        "OrganizationName",
    ):
        assert NAMESPACE + term in terms


def test_competency_name_part_types_query() -> None:
    terms = _query_terms("name-part-types.rq")
    # Multi-cultural coverage: Western, Spanish double surname, Arabic, mononym.
    for term in (
        "namePartGiven",
        "namePartSurname",
        "namePartPaternalSurname",
        "namePartNisba",
        "namePartMononym",
        "namePartGenerationName",
        "namePartClanName",
        "namePartBirthOrderName",
        "namePartNomen",
    ):
        assert NAMESPACE + term in terms


def test_competency_pronoun_sets_query() -> None:
    terms = _query_terms("pronoun-sets.rq")
    for term in ("pronounSheHer", "pronounHeHim", "pronounTheyThem", "pronounXeXem"):
        assert NAMESPACE + term in terms


def test_competency_language_origins_query() -> None:
    terms = _query_terms("language-origins.rq")
    for term in (
        "originNatural",
        "originAiGenerated",
        "originProgramming",
        "originConstructedEngineered",
    ):
        assert NAMESPACE + term in terms


def test_competency_writing_systems_query() -> None:
    terms = _query_terms("writing-systems.rq")
    for term in (
        "scriptRoleLogographicContent",
        "scriptRoleSyllabicGrammar",
        "scriptRoleLoanword",
        "scriptRoleTransliteration",
    ):
        assert NAMESPACE + term in terms


def test_competency_proficiency_levels_query() -> None:
    terms = _query_terms("proficiency-levels.rq")
    for term in ("cefrA1", "cefrC2", "levelNative", "levelHeritage"):
        assert NAMESPACE + term in terms


def test_qc_missing_definitions_is_empty() -> None:
    # The skeleton is fully annotated, so the QC check returns no offenders.
    graph = load_merged_graph(include_imports=False)
    query = (QC_DIR / "missing-definitions.rq").read_text(encoding="utf-8")
    offenders = list(graph.query(query))
    assert offenders == [], f"classes missing definitions: {offenders}"
