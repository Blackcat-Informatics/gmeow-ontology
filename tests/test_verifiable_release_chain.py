"""Structural + fixture + competency tests for the verifiable release chain (#233).

Wires the software module to the attestation/trust infrastructure:
BuildActivity, Builder, SLSA level, and the full MeowGraph v1.0.0 fixture.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import Graph, Literal, Namespace
from gmeow_rdf.compat.rdflib.namespace import OWL, RDF, RDFS
from gmeow_rdf.compat.rdflib.query import ResultRow

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
EX = Namespace("https://example.org/verifiable-release/")
FIXTURE = Path(__file__).parent / "fixtures" / "verifiable-release-chain.ttl"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _fixture() -> Graph:
    return Graph().parse(FIXTURE, format="turtle")


def _combined() -> Graph:
    return _graph() + _fixture()


# --------------------------------------------------------------------------- #
# Structural guards
# --------------------------------------------------------------------------- #


def test_build_activity_is_activity() -> None:
    g = _graph()
    assert (GM.BuildActivity, RDF.type, OWL.Class) in g
    assert (GM.BuildActivity, RDFS.subClassOf, GM.Activity) in g


def test_builder_is_software_agent() -> None:
    g = _graph()
    assert (GM.Builder, RDF.type, OWL.Class) in g
    assert (GM.Builder, RDFS.subClassOf, GM.SoftwareAgent) in g


def test_build_properties_exist() -> None:
    g = _graph()
    assert (GM.buildSource, RDFS.domain, GM.BuildActivity) in g
    assert (GM.buildOutput, RDFS.domain, GM.BuildActivity) in g
    assert (GM.buildOutput, RDFS.range, GM.Distribution) in g
    assert (GM.buildConfigUri, RDFS.domain, GM.BuildActivity) in g
    assert (GM.hasSLSALevel, RDFS.domain, GM.Attestation) in g
    assert (GM.hasSLSALevel, RDFS.range, GM.SLSALevel) in g


def test_release_doi_property_exists() -> None:
    g = _graph()
    assert (GM.releaseDoi, RDFS.domain, GM.Release) in g
    assert (GM.releaseDoi, RDFS.range, RDFS.Literal) in g


def test_build_event_type_seeded() -> None:
    g = _graph()
    assert (GM.eventTypeBuild, RDF.type, GM.EventType) in g


# --------------------------------------------------------------------------- #
# Fixture + SHACL
# --------------------------------------------------------------------------- #


# --------------------------------------------------------------------------- #
# Fixture chain assertions
# --------------------------------------------------------------------------- #


def test_fixture_signed_commit() -> None:
    g = _fixture()
    assert (EX.releaseCommit, RDF.type, GM.Commit) in g
    assert (EX.releaseCommit, GM.hasSignature, EX.commitSignature) in g
    assert (EX.commitSignature, GM.signedBy, EX.alice) in g
    assert (EX.commitSignature, GM.signingKey, EX.aliceEd25519) in g


def test_fixture_signed_tag() -> None:
    g = _fixture()
    assert (EX.tagV1_0_0, RDF.type, GM.Tag) in g
    assert (EX.tagV1_0_0, GM.pointsToCommit, EX.releaseCommit) in g
    assert (EX.tagV1_0_0, GM.hasSignature, EX.tagSignature) in g
    assert (EX.tagSignature, GM.signedBy, EX.alice) in g


def test_fixture_release_with_doi() -> None:
    g = _fixture()
    assert (EX.v1_0_0, RDF.type, GM.Release) in g
    assert (EX.v1_0_0, GM.releaseTag, EX.tagV1_0_0) in g
    assert (
        EX.v1_0_0,
        GM.releaseDoi,
        Literal("10.5281/zenodo.1234567"),
    ) in g


def test_fixture_build_activity() -> None:
    g = _fixture()
    assert (EX.buildV1_0_0, RDF.type, GM.BuildActivity) in g
    assert (EX.buildV1_0_0, GM.buildSource, EX.releaseCommit) in g
    assert (EX.buildV1_0_0, GM.buildOutput, EX.distTarball) in g
    assert (
        EX.buildV1_0_0,
        GM.buildConfigUri,
        Literal(
            "https://github.com/example/meowgraph/blob/v1.0.0/.github/workflows/release.yml"
        ),
    ) in g
    assert (EX.buildV1_0_0, GM.eventType, GM.eventTypeBuild) in g
    assert (EX.githubActions, RDF.type, GM.Builder) in g


def test_fixture_slsa_attestation() -> None:
    g = _fixture()
    assert (EX.slsaAttestation, RDF.type, GM.Attestation) in g
    assert (
        EX.slsaAttestation,
        GM.attestationType,
        GM.attestationTypeSLSAProvenance,
    ) in g
    assert (EX.slsaAttestation, GM.hasSLSALevel, GM.slsaLevel3) in g
    assert (EX.slsaAttestation, GM.attestedSubject, EX.distTarball) in g
    assert (EX.slsaAttestation, GM.attestationArtifact, EX.slsaArtifact) in g


def test_fixture_cosign_signature() -> None:
    g = _fixture()
    assert (EX.distTarball, GM.hasSignature, EX.cosignSignature) in g
    assert (EX.cosignSignature, GM.signedBy, EX.alice) in g
    assert (
        EX.cosignSignature,
        GM.signatureAlgorithm,
        Literal("ed25519"),
    ) in g
    assert (EX.cosignSignature, GM.signingKey, EX.aliceEd25519) in g


def test_fixture_rekor_entry() -> None:
    g = _fixture()
    assert (EX.slsaAttestation, GM.transparencyLogEntry, EX.rekorEntry) in g
    assert (EX.rekorEntry, RDF.type, GM.TransparencyLogEntry) in g
    assert (
        EX.rekorEntry,
        GM.logEntryUrl,
        Literal("https://rekor.sigstore.dev/api/v1/log/entries/24296fb24b8ad77a…"),
    ) in g


def test_fixture_swhid_on_commit() -> None:
    g = _fixture()
    vals = list(g.objects(EX.releaseCommit, GM.contentDigest))
    assert any("swh:" in str(v) for v in vals)


# --------------------------------------------------------------------------- #
# Competency queries (inline SPARQL over the combined graph)
# --------------------------------------------------------------------------- #


def test_query_key_that_signed_commit() -> None:
    """Which key signed the commit that the release tag points to?"""
    g = _combined()
    query = """
        SELECT ?key
        WHERE {
            ?release a <https://blackcatinformatics.ca/gmeow/Release> ;
                     <https://blackcatinformatics.ca/gmeow/releaseTag> ?tag .
            ?tag <https://blackcatinformatics.ca/gmeow/pointsToCommit> ?commit .
            ?commit <https://blackcatinformatics.ca/gmeow/hasSignature> ?sig .
            ?sig <https://blackcatinformatics.ca/gmeow/signingKey> ?key .
        }
    """
    rows = list(g.query(query, initBindings={"release": EX.v1_0_0}))
    assert rows
    assert any(
        str(row[0]) == str(EX.aliceEd25519)
        for row in rows
        if isinstance(row, ResultRow)
    )


def test_query_build_that_produced_artifact() -> None:
    """Which build produced the artifact with this SWHID?"""
    g = _combined()
    query = """
        SELECT ?build
        WHERE {
            ?commit <https://blackcatinformatics.ca/gmeow/contentDigest> ?swhid .
            ?build a <https://blackcatinformatics.ca/gmeow/BuildActivity> ;
                   <https://blackcatinformatics.ca/gmeow/buildSource> ?commit ;
                   <https://blackcatinformatics.ca/gmeow/buildOutput> ?dist .
        }
    """
    rows = list(
        g.query(
            query,
            initBindings={
                "swhid": Literal("swh:1:rev:0123456789abcdef0123456789abcdef01234567")
            },
        )
    )
    assert rows
    assert any(
        str(row[0]) == str(EX.buildV1_0_0) for row in rows if isinstance(row, ResultRow)
    )


def test_query_rekor_entry_for_attestation() -> None:
    """Is there a Rekor entry for the attestation of this release?"""
    g = _combined()
    query = """
        ASK {
            ?release a <https://blackcatinformatics.ca/gmeow/Release> ;
                     <https://blackcatinformatics.ca/gmeow/releaseTag> ?tag .
            ?tag <https://blackcatinformatics.ca/gmeow/pointsToCommit> ?commit .
            ?build a <https://blackcatinformatics.ca/gmeow/BuildActivity> ;
                   <https://blackcatinformatics.ca/gmeow/buildSource> ?commit ;
                   <https://blackcatinformatics.ca/gmeow/buildOutput> ?dist .
            ?attestation a <https://blackcatinformatics.ca/gmeow/Attestation> ;
                         <https://blackcatinformatics.ca/gmeow/attestedSubject>
                           ?dist ;
                         <https://blackcatinformatics.ca/gmeow/transparencyLogEntry>
                           ?rekor .
            ?rekor a <https://blackcatinformatics.ca/gmeow/TransparencyLogEntry> .
        }
    """
    result = g.query(query, initBindings={"release": EX.v1_0_0})
    assert bool(result)
