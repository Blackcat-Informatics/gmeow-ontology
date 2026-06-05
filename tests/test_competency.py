"""Tests that the competency and QC SPARQL queries behave as expected.

Phase 3 of the reasoning-depth epic (#35) upgrades the competency harness to run
over a **reasoned (materialized) graph** rather than the asserted one, so the
queries test what GMEOW *entails*, not merely what is written down (CONSTITUTION
Principle 7, verified by construction; Principle 8, reasoning-centric). The merged
graph is closed under OWL 2 RL with ``owlrl`` once and cached — pure-Python and
Docker-free, the same fast lane as ``tests/test_reasoning_entailments.py``.
Entailment is monotonic, so every answer the asserted graph gave is still present;
reasoning only adds. ``test_competency_ancestry_is_answered_only_by_reasoning``
makes the gain explicit: a competency answer absent from the asserted graph yet
present after materialization.
"""

from __future__ import annotations

from functools import lru_cache

import owlrl
from rdflib import Graph, Namespace
from rdflib.query import ResultRow

from gmeow_tools.config import COMPETENCY_DIR, NAMESPACE, ONTOLOGY_DIR, QC_DIR
from gmeow_tools.graph import load_merged_graph

GMEOW = Namespace(NAMESPACE)
EX = Namespace("https://example.org/test/")


@lru_cache(maxsize=1)
def _reasoned_graph() -> Graph:
    """The merged ontology closed under OWL 2 RL (materialized once, cached)."""
    graph = load_merged_graph(include_imports=False)
    owlrl.DeductiveClosure(owlrl.OWLRL_Semantics).expand(graph)
    return graph


def _query_terms(filename: str) -> set[str]:
    query = (COMPETENCY_DIR / filename).read_text(encoding="utf-8")
    terms: set[str] = set()
    for row in _reasoned_graph().query(query):
        assert isinstance(row, ResultRow)
        terms.add(str(row[0]))
    return terms


def test_competency_agents_query() -> None:
    results = _query_terms("agents.rq")
    # Agent and its skeleton subclasses must be returned.
    for term in ("Agent", "Person", "Organization"):
        assert NAMESPACE + term in results


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


def test_competency_ancestry_is_answered_only_by_reasoning() -> None:
    """AC#2: a competency answer ENTAILED, not asserted.

    "Who are a person's ancestors?" is answerable only over the reasoned graph:
    ``hasParent ∘ hasParent ⊑ hasAncestor`` (a property chain, #38). No SPARQL
    property path over the *asserted* edges yields the grandparent hop — only
    materialization does. This contrasts the asserted and reasoned graphs on the
    same A-Box to prove the entailment is absent before reasoning and present
    after.
    """
    abox = (
        (EX.a, GMEOW.hasParent, EX.b),
        (EX.b, GMEOW.hasParent, EX.c),
    )
    asserted = Graph()
    asserted.parse(ONTOLOGY_DIR / "modules" / "genealogy.ttl", format="turtle")
    for triple in abox:
        asserted.add(triple)

    grandparent = (EX.a, GMEOW.hasAncestor, EX.c)
    ask = f"PREFIX gmeow: <{NAMESPACE}> ASK {{ <{EX.a}> gmeow:hasAncestor <{EX.c}> }}"
    # Absent in the asserted graph...
    assert grandparent not in asserted
    assert not bool(asserted.query(ask))

    # ...present once the property chain is materialized.
    reasoned = Graph()
    reasoned += asserted
    owlrl.DeductiveClosure(owlrl.OWLRL_Semantics).expand(reasoned)
    assert grandparent in reasoned
    assert bool(reasoned.query(ask))


def test_qc_missing_definitions_is_empty() -> None:
    # The skeleton is fully annotated, so the QC check returns no offenders. This
    # QC check stays on the ASSERTED graph deliberately: reasoning must not be
    # able to invent a definition (or spuriously type a bnode as a class), so the
    # completeness guard is only meaningful over what is actually authored.
    graph = load_merged_graph(include_imports=False)
    query = (QC_DIR / "missing-definitions.rq").read_text(encoding="utf-8")
    offenders = list(graph.query(query))
    assert offenders == [], f"classes missing definitions: {offenders}"
