"""Tests that the competency and QC SPARQL queries behave as expected.

Phase 3 of the reasoning-depth epic (#35) upgrades the competency harness to run
over a **reasoned (materialized) graph** rather than the asserted one, so the
queries test what GMEOW *entails*, not merely what is written down (CONSTITUTION
Principle 7, verified by construction; Principle 8, reasoning-centric). The merged
graph is closed under OWL 2 RL with the native ``gmeow_logic`` RL engine
(``gmeow_tools.native_rl.native_rl_closure``) once and cached — Java/Docker-free,
the same native primary lane as ``tests/test_reasoning_entailments.py``. The
legacy ``owlrl`` baseline now lives only in the classic-cross-check lane as the
agreement oracle (issue #666).
Entailment is monotonic, so every answer the asserted graph gave is still present;
reasoning only adds. ``test_competency_ancestry_is_answered_only_by_reasoning``
makes the gain explicit: a competency answer absent from the asserted graph yet
present after materialization.
"""

from __future__ import annotations

from datetime import UTC, datetime, timedelta
from functools import lru_cache

from gmeow_rdf.compat.rdflib import RDF, Graph, Literal, Namespace
from gmeow_rdf.compat.rdflib.namespace import XSD
from gmeow_rdf.compat.rdflib.query import ResultRow

from gmeow_tools.config import COMPETENCY_DIR, NAMESPACE, QC_DIR
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.native_rl_rdflib import native_rl_closure
from gmeow_tools.slices import module_path

GMEOW = Namespace(NAMESPACE)
EX = Namespace("https://example.org/test/")


@lru_cache(maxsize=1)
def _reasoned_graph() -> Graph:
    """The merged ontology closed under OWL 2 RL (materialized once, cached)."""
    graph = load_merged_graph(include_imports=False)
    native_rl_closure(graph)
    return graph


def _query_terms(filename: str) -> set[str]:
    query = (COMPETENCY_DIR / filename).read_text(encoding="utf-8")
    terms: set[str] = set()
    for row in _reasoned_graph().query(query):
        assert isinstance(row, ResultRow)
        terms.add(str(row[0]))
    return terms


def _query_terms_on_graph(filename: str, graph: Graph) -> set[str]:
    """Run a competency query against a specific graph (for inline-data tests)."""
    query = (COMPETENCY_DIR / filename).read_text(encoding="utf-8")
    terms: set[str] = set()
    for row in graph.query(query):
        assert isinstance(row, ResultRow)
        terms.add(str(row[0]))
    return terms


# test_competency_agents_query was migrated to the declarative test-DSL and now
# runs in the native Rust slice-test harness as ex:cqAgentKinds in
# slices/core/epistemics/tests/competency.ttl (full 6-row enumeration, exact).
# See dsl/tests/MIGRATION-LEDGER.md (#784).


def test_competency_works_query() -> None:
    terms = _query_terms("works.rq")
    for term in ("CreativeWork", "Article", "Patent", "Dataset", "SoftwareProduct"):
        assert NAMESPACE + term in terms


def test_competency_rights_query() -> None:
    terms = _query_terms("rights.rq")
    # The abstract rule and the ODRL deontic trio (rights facility, #21).
    for term in ("Rule", "Permission", "Prohibition", "Duty"):
        assert NAMESPACE + term in terms


def test_competency_kinship_query() -> None:
    terms = _query_terms("kinship.rq")
    for term in ("hasParent", "hasChild", "hasSpouse", "hasSibling"):
        assert NAMESPACE + term in terms


def test_competency_life_events_query() -> None:
    terms = _query_terms("life-events.rq")
    # Life-event kinds are now eventType VALUE individuals (#41), not subclasses.
    for term in (
        "eventTypeBirth",
        "eventTypeDeath",
        "eventTypeMarriage",
        "eventTypeBurial",
        "eventTypeCensus",
        "eventTypeAdoption",
    ):
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


def test_competency_place_namings_query() -> None:
    terms = _query_terms("place-namings.rq")
    # PlaceNaming is the defined place-scoped subclass of NameUsage (issue #105).
    assert NAMESPACE + "PlaceNaming" in terms


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

    "Who are a person's ancestors?" is answered by the ``gmeow:hasAncestor``
    relation, which is derived by the property chain ``hasParent ∘ hasParent ⊑
    hasAncestor`` (#38). The ``hasAncestor`` triple is authored *nowhere* in the
    A-Box — it only appears once the chain is materialized. (One could of course
    walk ``hasParent+`` as a path, but the competency answer relation is
    ``hasAncestor``, and that triple is entailed, not asserted.) This contrasts
    the asserted and reasoned graphs on the same A-Box to prove the entailment is
    absent before reasoning and present after.
    """
    abox = (
        (EX.a, GMEOW.hasParent, EX.b),
        (EX.b, GMEOW.hasParent, EX.c),
    )
    asserted = Graph()
    asserted.parse(module_path("genealogy"), format="turtle")
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
    native_rl_closure(reasoned)
    assert grandparent in reasoned
    assert bool(reasoned.query(ask))


def test_place_naming_is_entailed_not_asserted() -> None:
    """The PlaceNaming DEFINED class (≡ NameUsage ⊓ ∃usageNamed.Place, issue #105).

    A name-usage that names a gmeow:Place is CLASSIFIED as a gmeow:PlaceNaming by
    the reasoner — the type is entailed, authored nowhere (Principle 6: place-naming
    reuses the NameUsage relator instead of minting a parallel one; Principle 8:
    reasoning-centric). This is the first owl:equivalentClass defined class in the
    ontology; the test contrasts the asserted and reasoned graphs on the same A-Box
    to prove the classification is absent before reasoning and present after.
    """
    asserted = load_merged_graph(include_imports=False)
    asserted.add((EX.usage, RDF.type, GMEOW.NameUsage))
    asserted.add((EX.usage, GMEOW.usageNamed, EX.place))
    asserted.add((EX.place, RDF.type, GMEOW.Place))
    asserted.add((EX.usage, GMEOW.usageAppellation, EX.toponym))
    asserted.add((EX.toponym, RDF.type, GMEOW.PlaceName))

    classified = (EX.usage, RDF.type, GMEOW.PlaceNaming)
    # Absent in the asserted graph (nothing types it a PlaceNaming)...
    assert classified not in asserted

    # ...present once the equivalentClass definition is materialized.
    reasoned = Graph()
    reasoned += asserted
    native_rl_closure(reasoned)
    assert classified in reasoned
    # And a name-usage that does NOT name a Place is NOT classified as a PlaceNaming.
    asserted.add((EX.personUsage, RDF.type, GMEOW.NameUsage))
    asserted.add((EX.personUsage, GMEOW.usageNamed, EX.person))
    asserted.add((EX.person, RDF.type, GMEOW.Person))
    other = Graph()
    other += asserted
    native_rl_closure(other)
    assert (EX.personUsage, RDF.type, GMEOW.PlaceNaming) not in other


def test_qc_missing_definitions_is_empty() -> None:
    # The skeleton is fully annotated, so the QC check returns no offenders. This
    # QC check stays on the ASSERTED graph deliberately: reasoning must not be
    # able to invent a definition (or spuriously type a bnode as a class), so the
    # completeness guard is only meaningful over what is actually authored.
    graph = load_merged_graph(include_imports=False)
    query = (QC_DIR / "missing-definitions.rq").read_text(encoding="utf-8")
    offenders = list(graph.query(query))
    assert offenders == [], f"classes missing definitions: {offenders}"


def test_competency_citation_intents_query() -> None:
    terms = _query_terms("citation-intents.rq")
    for term in (
        "intentCitesAsDataSource",
        "intentUsesMethodIn",
        "intentExtends",
        "intentIsInspiredBy",
        "intentConformsTo",
        "intentDerivedFrom",
        "intentDocuments",
        "intentSupports",
        "intentDisagreesWith",
        "intentBridgedByReference",
    ):
        assert NAMESPACE + term in terms
    assert len(terms) >= 10


# test_competency_contribution_roles_query was migrated to the declarative
# test-DSL and now runs in the native Rust slice-test harness as
# ex:cqContributionRoles in slices/core/epistemics/tests/competency.ttl (full
# 48-row enumeration, exact). See dsl/tests/MIGRATION-LEDGER.md (#784).


def test_competency_evidence_query() -> None:
    terms = _query_terms("evidence.rq")
    for term in (
        "EvidenceClass",
        "hasEvidenceClass",
        "sourceIndependence",
        "sourceTier",
        "coverageDepth",
        "supportsNotability",
        "evidenceVERIFIED",
        "evidenceSELF",
        "evidenceANECDOTAL",
        "evidenceRUMOR",
        "evidenceLegalFiling",
        "sourceIndependenceIndependent",
        "sourceIndependenceSelfOrIssuerOriginated",
        "sourceTierPrimary",
        "sourceTierSecondary",
        "sourceTierTertiary",
        "coverageDepthSignificantCoverage",
        "coverageDepthPassingMention",
        "coverageDepthRoutineFiling",
    ):
        assert NAMESPACE + term in terms


def test_competency_notability_eligible_query() -> None:
    terms = _query_terms("notability-eligible.rq")
    for term in (
        "sourceIndependenceIndependent",
        "sourceTierSecondary",
        "coverageDepthSignificantCoverage",
        "supportsNotability",
    ):
        assert NAMESPACE + term in terms


def test_competency_deception_types_query() -> None:
    terms = _query_terms("deception-types.rq")
    for term in (
        "eventTypeDeception",
        "eventTypeLie",
        "eventTypePaltering",
        "eventTypeOmission",
        "eventTypeDistortion",
        "eventTypeBullshit",
        "eventTypeSelfDeception",
        "ClaimVeridicality",
        "veridicalityUntrue",
        "veridicalityLicensedFalsehood",
    ):
        assert NAMESPACE + term in terms
    assert len(terms) >= 10


def test_competency_deception_roles_query() -> None:
    terms = _query_terms("deception-roles.rq")
    for term in (
        "roleDeceiver",
        "roleDeceived",
        "roleBeneficiaryOfDeception",
        "roleDupe",
        "roleSpinDoctor",
    ):
        assert NAMESPACE + term in terms
    assert len(terms) >= 5


def test_competency_deception_lie_query() -> None:
    """Lie = held refuted + projected unequivocal."""
    g = Graph()
    g.add((EX.event1, RDF.type, GMEOW.Event))
    g.add((EX.event1, GMEOW.eventType, GMEOW.eventTypeLie))
    g.add((EX.event1, GMEOW.heldStandpoint, EX.claimA))
    g.add((EX.event1, GMEOW.projectedStandpoint, EX.claimB))
    g.add((EX.claimA, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.claimB, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.claimA, GMEOW.claimModality, GMEOW.refuted))
    g.add((EX.claimB, GMEOW.claimModality, GMEOW.unequivocal))
    terms = _query_terms_on_graph("deception-lie.rq", g)
    assert str(EX.event1) in terms


def test_competency_deception_omission_query() -> None:
    """Omission = held present, no projected."""
    g = Graph()
    g.add((EX.event1, RDF.type, GMEOW.Event))
    g.add((EX.event1, GMEOW.eventType, GMEOW.eventTypeOmission))
    g.add((EX.event1, GMEOW.heldStandpoint, EX.claimA))
    g.add((EX.claimA, RDF.type, GMEOW.StandpointClaim))
    terms = _query_terms_on_graph("deception-omission.rq", g)
    assert str(EX.event1) in terms


def test_competency_deception_paltering_query() -> None:
    """Paltering = projected implicates false P', held refutes P'."""
    g = Graph()
    g.add((EX.event1, RDF.type, GMEOW.Event))
    g.add((EX.event1, GMEOW.eventType, GMEOW.eventTypePaltering))
    g.add((EX.event1, GMEOW.implicates, EX.propositionPprime))
    g.add((EX.event1, GMEOW.heldStandpoint, EX.claimA))
    g.add((EX.event1, GMEOW.projectedStandpoint, EX.claimB))
    g.add((EX.claimA, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.claimB, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.claimA, GMEOW.claimModality, GMEOW.refuted))
    terms = _query_terms_on_graph("deception-paltering.rq", g)
    assert str(EX.event1) in terms


def test_competency_deception_bullshit_query() -> None:
    """Bullshit = held modality = bullshit, projected unequivocal."""
    g = Graph()
    g.add((EX.event1, RDF.type, GMEOW.Event))
    g.add((EX.event1, GMEOW.eventType, GMEOW.eventTypeBullshit))
    g.add((EX.event1, GMEOW.heldStandpoint, EX.claimA))
    g.add((EX.event1, GMEOW.projectedStandpoint, EX.claimB))
    g.add((EX.claimA, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.claimB, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.claimA, GMEOW.claimModality, GMEOW.bullshit))
    g.add((EX.claimB, GMEOW.claimModality, GMEOW.unequivocal))
    terms = _query_terms_on_graph("deception-bullshit.rq", g)
    assert str(EX.event1) in terms


def test_competency_deception_distortion_query() -> None:
    """Distortion = held probable + projected unequivocal."""
    g = Graph()
    g.add((EX.event1, RDF.type, GMEOW.Event))
    g.add((EX.event1, GMEOW.eventType, GMEOW.eventTypeDistortion))
    g.add((EX.event1, GMEOW.heldStandpoint, EX.claimA))
    g.add((EX.event1, GMEOW.projectedStandpoint, EX.claimB))
    g.add((EX.claimA, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.claimB, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.claimA, GMEOW.claimModality, GMEOW.probable))
    g.add((EX.claimB, GMEOW.claimModality, GMEOW.unequivocal))
    terms = _query_terms_on_graph("deception-distortion.rq", g)
    assert str(EX.event1) in terms


def test_competency_deception_licensed_falsehood_query() -> None:
    """Negative test: fiction claims must NOT be returned as lies."""
    g = Graph()
    g.add((EX.fictionClaim, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.fictionClaim, GMEOW.accordingTo, EX.narrativeFrame))
    g.add(
        (EX.fictionClaim, GMEOW.claimVeridicality, GMEOW.veridicalityLicensedFalsehood)
    )
    terms = _query_terms_on_graph("deception-licensed-falsehood.rq", g)
    assert len(terms) == 0


def test_competency_deception_self_deception_query() -> None:
    """Self-deception = event with eventTypeSelfDeception and a participant."""
    g = Graph()
    g.add((EX.event1, RDF.type, GMEOW.Event))
    g.add((EX.event1, GMEOW.eventType, GMEOW.eventTypeSelfDeception))
    g.add((EX.event1, GMEOW.hasParticipant, EX.agent1))
    terms = _query_terms_on_graph("deception-self-deception.rq", g)
    assert str(EX.event1) in terms


def test_competency_deception_fabrication_query() -> None:
    """Fabrication = held refuted + projected unequivocal + failed verification."""
    g = Graph()
    g.add((EX.event1, RDF.type, GMEOW.Event))
    g.add((EX.event1, GMEOW.eventType, GMEOW.eventTypeFabrication))
    g.add((EX.event1, GMEOW.heldStandpoint, EX.claimA))
    g.add((EX.event1, GMEOW.projectedStandpoint, EX.claimB))
    g.add((EX.claimA, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.claimB, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.claimA, GMEOW.claimModality, GMEOW.refuted))
    g.add((EX.claimB, GMEOW.claimModality, GMEOW.unequivocal))
    # Distinctive machinery: fabricated work + failed verification result.
    g.add((EX.work1, RDF.type, GMEOW.CreativeWork))
    g.add((EX.event1, GMEOW.implicates, EX.work1))
    g.add((EX.verification1, RDF.type, GMEOW.VerificationResult))
    g.add(
        (EX.verification1, GMEOW.hasVerificationStatus, GMEOW.verificationStatusFailed)
    )
    terms = _query_terms_on_graph("deception-fabrication.rq", g)
    assert str(EX.event1) in terms


def test_competency_deception_forgery_query() -> None:
    """Forgery = held refuted + projected unequivocal + counterpart + failed sig."""
    g = Graph()
    g.add((EX.event1, RDF.type, GMEOW.Event))
    g.add((EX.event1, GMEOW.eventType, GMEOW.eventTypeForgery))
    g.add((EX.event1, GMEOW.heldStandpoint, EX.claimA))
    g.add((EX.event1, GMEOW.projectedStandpoint, EX.claimB))
    g.add((EX.claimA, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.claimB, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.claimA, GMEOW.claimModality, GMEOW.refuted))
    g.add((EX.claimB, GMEOW.claimModality, GMEOW.unequivocal))
    # Distinctive machinery: forged work with counterpartOf + failed signature.
    g.add((EX.forgedWork, RDF.type, GMEOW.CreativeWork))
    g.add((EX.genuineWork, RDF.type, GMEOW.CreativeWork))
    g.add((EX.forgedWork, GMEOW.counterpartOf, EX.genuineWork))
    g.add((EX.event1, GMEOW.implicates, EX.forgedWork))
    g.add((EX.signature1, RDF.type, GMEOW.CryptographicSignature))
    g.add((EX.signature1, GMEOW.signatureOf, EX.forgedWork))
    g.add((EX.signature1, GMEOW.hasVerificationStatus, GMEOW.verificationStatusFailed))
    terms = _query_terms_on_graph("deception-forgery.rq", g)
    assert str(EX.event1) in terms


def test_competency_deception_impersonation_query() -> None:
    """Impersonation = held refuted + projected unequivocal + mismatched facet."""
    g = Graph()
    g.add((EX.event1, RDF.type, GMEOW.Event))
    g.add((EX.event1, GMEOW.eventType, GMEOW.eventTypeImpersonation))
    g.add((EX.event1, GMEOW.heldStandpoint, EX.claimA))
    g.add((EX.event1, GMEOW.projectedStandpoint, EX.claimB))
    g.add((EX.claimA, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.claimB, RDF.type, GMEOW.StandpointClaim))
    g.add((EX.claimA, GMEOW.claimModality, GMEOW.refuted))
    g.add((EX.claimB, GMEOW.claimModality, GMEOW.unequivocal))
    # Distinctive machinery: identity facet whose subject differs from deceiver.
    g.add((EX.facet1, RDF.type, GMEOW.IdentityFacet))
    g.add((EX.facet1, GMEOW.observedFeature, EX.event1))
    g.add((EX.facet1, GMEOW.facetSubject, EX.victim))
    terms = _query_terms_on_graph("deception-impersonation.rq", g)
    assert str(EX.event1) in terms


def test_competency_myths_query() -> None:
    terms = _query_terms("myths.rq")
    for term in (
        "Myth",
        "hasMythTelling",
        "mythFrame",
        "propagatesFrom",
        "wasDerivedFrom",
    ):
        assert NAMESPACE + term in terms
    assert len(terms) >= 5


def test_competency_procedures_query() -> None:
    terms = _query_terms("procedures.rq")
    for term in (
        "Procedure",
        "ProcedureStep",
        "Execution",
        "ControlFlow",
        "DataFlow",
    ):
        assert NAMESPACE + term in terms


def test_competency_ingestion_executions_query() -> None:
    # The query looks for Executions of ingestion procedures that produced
    # Observations but no Events. In the asserted ontology there are no
    # Execution individuals, so the query returns an empty set — this is
    # correct behaviour for the TBox-only graph.
    terms = _query_terms("ingestion-executions.rq")
    # Empty result is valid for the TBox; we just verify the query parses and runs.
    assert terms == set()


def test_competency_research_inquiries_query() -> None:
    # The query looks for open research inquiries. In the asserted ontology
    # there are no open inquiry instances, so the query returns an empty set.
    terms = _query_terms("research-inquiries.rq")
    assert terms == set()


# --------------------------------------------------------------------------- #
# Issue #263 — Expertise depth competency queries
# --------------------------------------------------------------------------- #


def test_competency_expertise_expert_python_query() -> None:
    """Expert-level proficiency query returns the agent."""
    g = Graph()
    g.add((EX.agent1, RDF.type, GMEOW.Agent))
    g.add((EX.python, RDF.type, GMEOW.Skill))
    g.add((EX.prof1, RDF.type, GMEOW.SkillProficiency))
    g.add((EX.prof1, GMEOW.skillProficiencyAgent, EX.agent1))
    g.add((EX.prof1, GMEOW.skillProficiencyOf, EX.python))
    g.add((EX.prof1, GMEOW.skillProficiencyLevel, GMEOW.dreyfusExpert))
    terms = _query_terms_on_graph("expertise-expert-python.rq", g)
    assert str(EX.agent1) in terms


def test_competency_expertise_expiring_credentials_query() -> None:
    """Expiring-credentials query returns credentials with a future expiry."""
    g = Graph()
    g.add((EX.cred1, RDF.type, GMEOW.Credential))
    g.add((EX.cred1, GMEOW.credentialIssuer, EX.amazon))
    g.add((EX.amazon, RDF.type, GMEOW.Organization))
    # Use a timezone-aware future date so rdflib can compare with NOW().
    expires_soon = datetime.now(UTC) + timedelta(days=180)
    expires_str = expires_soon.isoformat().replace("+00:00", "Z")
    g.add(
        (
            EX.cred1,
            GMEOW.validUntil,
            Literal(expires_str, datatype=XSD.dateTime),
        )
    )
    terms = _query_terms_on_graph("expertise-expiring-credentials.rq", g)
    assert str(EX.cred1) in terms


def test_competency_expertise_endorsed_vs_self_asserted_query() -> None:
    """Endorsed-vs-self-asserted query classifies proficiencies correctly."""
    g = Graph()
    g.add((EX.agent1, RDF.type, GMEOW.Agent))
    g.add((EX.peer, RDF.type, GMEOW.Agent))
    g.add((EX.python, RDF.type, GMEOW.Skill))

    # Self-asserted proficiency (no attestation)
    g.add((EX.profSelf, RDF.type, GMEOW.SkillProficiency))
    g.add((EX.profSelf, GMEOW.skillProficiencyAgent, EX.agent1))
    g.add((EX.profSelf, GMEOW.skillProficiencyOf, EX.python))
    g.add((EX.profSelf, GMEOW.skillProficiencyLevel, GMEOW.assessedCompetent))

    # Endorsed proficiency (third-party attestation)
    g.add((EX.profEndorsed, RDF.type, GMEOW.SkillProficiency))
    g.add((EX.profEndorsed, GMEOW.skillProficiencyAgent, EX.agent1))
    g.add((EX.profEndorsed, GMEOW.skillProficiencyOf, EX.python))
    g.add((EX.profEndorsed, GMEOW.skillProficiencyLevel, GMEOW.dreyfusExpert))
    g.add((EX.att1, RDF.type, GMEOW.Attestation))
    g.add((EX.att1, GMEOW.attestedSubject, EX.profEndorsed))
    g.add((EX.att1, GMEOW.attester, EX.peer))

    # Self-attested proficiency with self-attestation (should stay self-asserted)
    g.add((EX.profSelfAttested, RDF.type, GMEOW.SkillProficiency))
    g.add((EX.profSelfAttested, GMEOW.skillProficiencyAgent, EX.agent1))
    g.add((EX.profSelfAttested, GMEOW.skillProficiencyOf, EX.python))
    g.add((EX.att2, RDF.type, GMEOW.Attestation))
    g.add((EX.att2, GMEOW.attestedSubject, EX.profSelfAttested))
    g.add((EX.att2, GMEOW.attester, EX.agent1))

    terms = _query_terms_on_graph("expertise-endorsed-vs-self-asserted.rq", g)
    assert str(EX.profSelf) in terms
    assert str(EX.profEndorsed) in terms
    assert str(EX.profSelfAttested) in terms


def test_competency_expertise_employment_credentials_query() -> None:
    """Employment-credentials query links employment to certifying credentials."""
    g = Graph()
    g.add((EX.emp1, RDF.type, GMEOW.Employment))
    g.add((EX.sweOcc, RDF.type, GMEOW.Occupation))
    g.add((EX.emp1, GMEOW.employmentOccupation, EX.sweOcc))
    g.add((EX.cred1, RDF.type, GMEOW.Credential))
    g.add((EX.cred1, GMEOW.credentialFor, EX.sweOcc))
    g.add((EX.cred1, GMEOW.credentialIssuer, EX.org1))
    g.add((EX.org1, RDF.type, GMEOW.Organization))
    terms = _query_terms_on_graph("expertise-employment-credentials.rq", g)
    assert str(EX.emp1) in terms
