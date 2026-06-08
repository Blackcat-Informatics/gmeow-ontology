"""Structural + DL-safety guards for the cross-cutting attestation module.

These tests pin the decisions that keep the attestation layer from quietly
turning the reasoner into a truth machine: signatures prove integrity (not
truth), verification results are observed outcomes (not axioms), and ledger
inclusion proves inclusion (not correctness).
"""

from __future__ import annotations

from pathlib import Path

from rdflib import Graph, Namespace, URIRef
from rdflib.namespace import OWL, RDF, RDFS

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.validate import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"

EX_ATTEST = Namespace("https://blackcatinformatics.ca/gmeow/examples/attestation/")
COVERAGE_FIXTURES = Path(__file__).parent / "fixtures" / "coverage"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# Class structure
# --------------------------------------------------------------------------- #


def test_attestation_is_relator() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "Attestation"),
        RDFS.subClassOf,
        URIRef(GUFO + "Relator"),
    ) in graph


def test_attestation_artifact_is_information_object() -> None:
    graph = _graph()
    for cls in (
        "AttestationArtifact",
        "VerificationResult",
        "TransparencyLogEntry",
        "LedgerTransaction",
        "Block",
    ):
        assert (
            URIRef(GMEOW + cls),
            RDFS.subClassOf,
            URIRef(GMEOW + "InformationObject"),
        ) in graph, f"{cls} should be an InformationObject"


def test_verification_activity_is_activity() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "VerificationActivity"),
        RDFS.subClassOf,
        URIRef(GMEOW + "Activity"),
    ) in graph


def test_ledger_event_is_event() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "LedgerEvent"),
        RDFS.subClassOf,
        URIRef(GMEOW + "Event"),
    ) in graph


def test_blockchain_entities_are_entities() -> None:
    graph = _graph()
    for cls in ("BlockchainNetwork", "SmartContract", "BlockchainAccount"):
        assert (
            URIRef(GMEOW + cls),
            RDFS.subClassOf,
            URIRef(GMEOW + "Entity"),
        ) in graph, f"{cls} should be an Entity"


# --------------------------------------------------------------------------- #
# Value vocabularies are individuals, never subclasses
# --------------------------------------------------------------------------- #


def test_attestation_type_is_value_vocabulary() -> None:
    graph = _graph()
    att_type = URIRef(GMEOW + "AttestationType")
    assert (att_type, RDFS.subClassOf, URIRef(GUFO + "QualityValue")) in graph
    for seed in (
        "attestationTypeSLSAProvenance",
        "attestationTypeVerifiableCredential",
        "attestationTypeBlockchainClaim",
    ):
        assert (URIRef(GMEOW + seed), RDF.type, att_type) in graph
        assert (URIRef(GMEOW + seed), RDF.type, OWL.Class) not in graph


def test_signature_scheme_is_value_vocabulary() -> None:
    graph = _graph()
    sig_scheme = URIRef(GMEOW + "SignatureScheme")
    assert (sig_scheme, RDFS.subClassOf, URIRef(GUFO + "QualityValue")) in graph
    for seed in ("signatureSchemeRSASHA256", "signatureSchemeEd25519"):
        assert (URIRef(GMEOW + seed), RDF.type, sig_scheme) in graph
        assert (URIRef(GMEOW + seed), RDF.type, OWL.Class) not in graph


def test_verification_status_is_value_vocabulary() -> None:
    graph = _graph()
    vs = URIRef(GMEOW + "VerificationStatus")
    assert (vs, RDFS.subClassOf, URIRef(GUFO + "QualityValue")) in graph
    for seed in ("verificationStatusVerified", "verificationStatusRevoked"):
        assert (URIRef(GMEOW + seed), RDF.type, vs) in graph
        assert (URIRef(GMEOW + seed), RDF.type, OWL.Class) not in graph


def test_ledger_finality_status_is_value_vocabulary() -> None:
    graph = _graph()
    fs = URIRef(GMEOW + "LedgerFinalityStatus")
    assert (fs, RDFS.subClassOf, URIRef(GUFO + "QualityValue")) in graph
    for seed in ("finalityStatusFinalized", "finalityStatusOrphaned"):
        assert (URIRef(GMEOW + seed), RDF.type, fs) in graph
        assert (URIRef(GMEOW + seed), RDF.type, OWL.Class) not in graph


# --------------------------------------------------------------------------- #
# Signature generalisation — hasSignature is no longer message-locked
# --------------------------------------------------------------------------- #


def test_has_signature_not_domain_locked_to_message() -> None:
    graph = _graph()
    has_sig = URIRef(GMEOW + "hasSignature")
    msg = URIRef(GMEOW + "Message")
    # The property exists.
    assert (has_sig, RDF.type, OWL.ObjectProperty) in graph
    # It must NOT have rdfs:domain gmeow:Message (that restriction was removed).
    assert (has_sig, RDFS.domain, msg) not in graph, (
        "hasSignature must not be domain-locked to Message"
    )


# --------------------------------------------------------------------------- #
# Certification preserved and documented
# --------------------------------------------------------------------------- #


def test_certification_still_exists_as_relator() -> None:
    graph = _graph()
    cert = URIRef(GMEOW + "Certification")
    assert (cert, RDF.type, OWL.Class) in graph
    assert (cert, RDFS.subClassOf, URIRef(GUFO + "Relator")) in graph
    # Documentation scopeNote should mention attestation.
    assert (cert, RDFS.comment, None) in graph or (cert, RDFS.label, None) in graph


# --------------------------------------------------------------------------- #
# No inferential bridges — signature / verification / ledger do NOT imply truth
# --------------------------------------------------------------------------- #


def test_signature_does_not_imply_truth() -> None:
    """hasSignature must not bridge to observationResult, truth, or trust."""
    graph = _graph()
    has_sig = URIRef(GMEOW + "hasSignature")
    for banned in ("observationResult", "trustor", "trustee", "endorses"):
        banned_node = URIRef(GMEOW + banned)
        assert (has_sig, RDFS.subPropertyOf, banned_node) not in graph
        assert (banned_node, RDFS.subPropertyOf, has_sig) not in graph
        assert (has_sig, OWL.equivalentProperty, banned_node) not in graph


def test_verification_does_not_imply_truth_or_trust() -> None:
    """verificationResult / verifiedBy must not bridge to truth or trust."""
    graph = _graph()
    for prop in ("verificationResult", "verifiedBy"):
        prop_node = URIRef(GMEOW + prop)
        for banned in ("observationResult", "trustor", "trustee", "endorses"):
            banned_node = URIRef(GMEOW + banned)
            assert (prop_node, RDFS.subPropertyOf, banned_node) not in graph
            assert (banned_node, RDFS.subPropertyOf, prop_node) not in graph
            assert (prop_node, OWL.equivalentProperty, banned_node) not in graph


def test_ledger_does_not_imply_real_world_truth() -> None:
    """Ledger properties must not bridge to truth, trust, or quality."""
    graph = _graph()
    for prop in (
        "ledgerInclusionProof",
        "confirmationDepth",
        "finalityStatus",
        "transactionHash",
        "blockHash",
    ):
        prop_node = URIRef(GMEOW + prop)
        for banned in (
            "observationResult",
            "trustor",
            "trustee",
            "endorses",
            "assessedEntity",
        ):
            banned_node = URIRef(GMEOW + banned)
            assert (prop_node, RDFS.subPropertyOf, banned_node) not in graph
            assert (banned_node, RDFS.subPropertyOf, prop_node) not in graph
            assert (prop_node, OWL.equivalentProperty, banned_node) not in graph


# --------------------------------------------------------------------------- #
# No transitive / symmetric property chains on attestation properties
# --------------------------------------------------------------------------- #


def test_attestation_properties_not_transitive_or_symmetric() -> None:
    graph = _graph()
    for prop in (
        "attester",
        "attestedSubject",
        "attestedClaim",
        "hasAttestation",
        "attestationArtifact",
        "verificationActivity",
        "verificationResult",
        "verifiedBy",
        "transparencyLogEntry",
    ):
        prop_node = URIRef(GMEOW + prop)
        assert (prop_node, RDF.type, OWL.TransitiveProperty) not in graph
        assert (prop_node, RDF.type, OWL.SymmetricProperty) not in graph


# --------------------------------------------------------------------------- #
# Standpoint coexistence — contested attestations
# --------------------------------------------------------------------------- #


def test_contested_attestation_coexists() -> None:
    """A contested attestation: one standpoint affirms, another refutes.
    Both claims load, SHACL-pass, and are retained."""
    g = Graph().parse(COVERAGE_FIXTURES / "attestation-vc.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    # The attestation itself exists.
    assert (EX_ATTEST.vcAttestation, RDF.type, URIRef(GMEOW + "Attestation")) in g
    # Both standpoint axioms coexist: affirmation and refutation.
    assert (EX_ATTEST.claimAffirmed, RDF.type, OWL.Axiom) in g
    assert (EX_ATTEST.claimRefuted, RDF.type, OWL.Axiom) in g


# --------------------------------------------------------------------------- #
# Fixture coverage — all 6 scenarios load and SHACL-pass
# --------------------------------------------------------------------------- #

FIXTURES = [
    "attestation-software-release.ttl",
    "attestation-vc.ttl",
    "attestation-email-reuse.ttl",
    "attestation-quality-report.ttl",
    "attestation-blockchain-claim.ttl",
    "attestation-ledger-evidence.ttl",
]


def test_all_fixture_files_load() -> None:
    for name in FIXTURES:
        path = COVERAGE_FIXTURES / name
        assert path.exists(), f"missing fixture {name}"
        g = Graph().parse(path, format="turtle")
        result = run_shacl(g)
        assert result.ok, f"{name} failed SHACL:\n" + "\n".join(result.errors)
