// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_verifiable_release_chain.py.
//!
//! All structural, fixture-membership, and SPARQL competency assertions are
//! migrated here using the native graph-query helper.

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;
use purrdf::TermValue;
use std::collections::BTreeSet;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const EX: &str = "https://example.org/verifiable-release/";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const RDFS_LITERAL: &str = "http://www.w3.org/2000/01/rdf-schema#Literal";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

const FIXTURE: &str = "tests/fixtures/verifiable-release-chain.ttl";

// ── SHACL conformance case ────────────────────────────────────────────────────

#[batch_cases]
#[case::fixture_loads_and_shacl_passes(Case::repo_path(FIXTURE))]
fn verifiable_release_chain(#[case] case: Case) {
    case.run();
}

// ── Structural guards ─────────────────────────────────────────────────────────

#[gmeow_test_batch_macros::batch_test]
fn build_activity_is_activity() {
    let g = GraphStore::ontology();
    assert!(g.has(
        Some(&format!("{GMEOW}BuildActivity")),
        Some(RDF_TYPE),
        Some(OWL_CLASS)
    ));
    assert!(g.has(
        Some(&format!("{GMEOW}BuildActivity")),
        Some(RDFS_SUBCLASS_OF),
        Some(&format!("{GMEOW}Activity")),
    ));
}

#[gmeow_test_batch_macros::batch_test]
fn builder_is_software_agent() {
    let g = GraphStore::ontology();
    assert!(g.has(
        Some(&format!("{GMEOW}Builder")),
        Some(RDF_TYPE),
        Some(OWL_CLASS)
    ));
    assert!(g.has(
        Some(&format!("{GMEOW}Builder")),
        Some(RDFS_SUBCLASS_OF),
        Some(&format!("{GMEOW}SoftwareAgent")),
    ));
}

#[gmeow_test_batch_macros::batch_test]
fn build_properties_exist() {
    let g = GraphStore::ontology();
    assert!(g.has(
        Some(&format!("{GMEOW}buildSource")),
        Some(RDFS_DOMAIN),
        Some(&format!("{GMEOW}BuildActivity")),
    ));
    assert!(g.has(
        Some(&format!("{GMEOW}buildOutput")),
        Some(RDFS_DOMAIN),
        Some(&format!("{GMEOW}BuildActivity")),
    ));
    assert!(g.has(
        Some(&format!("{GMEOW}buildOutput")),
        Some(RDFS_RANGE),
        Some(&format!("{GMEOW}Distribution")),
    ));
    assert!(g.has(
        Some(&format!("{GMEOW}buildConfigUri")),
        Some(RDFS_DOMAIN),
        Some(&format!("{GMEOW}BuildActivity")),
    ));
    assert!(g.has(
        Some(&format!("{GMEOW}hasSLSALevel")),
        Some(RDFS_DOMAIN),
        Some(&format!("{GMEOW}Attestation")),
    ));
    assert!(g.has(
        Some(&format!("{GMEOW}hasSLSALevel")),
        Some(RDFS_RANGE),
        Some(&format!("{GMEOW}SLSALevel")),
    ));
}

#[gmeow_test_batch_macros::batch_test]
fn release_doi_property_exists() {
    let g = GraphStore::ontology();
    assert!(g.has(
        Some(&format!("{GMEOW}releaseDoi")),
        Some(RDFS_DOMAIN),
        Some(&format!("{GMEOW}Release")),
    ));
    assert!(g.has(
        Some(&format!("{GMEOW}releaseDoi")),
        Some(RDFS_RANGE),
        Some(RDFS_LITERAL),
    ));
}

#[gmeow_test_batch_macros::batch_test]
fn build_event_type_seeded() {
    let g = GraphStore::ontology();
    assert!(g.has(
        Some(&format!("{GMEOW}eventTypeBuild")),
        Some(RDF_TYPE),
        Some(&format!("{GMEOW}EventType")),
    ));
}

// ── Fixture chain assertions ──────────────────────────────────────────────────

fn fixture_store() -> GraphStore {
    GraphStore::parse_ttl_file(&repo_root().join(FIXTURE))
}

#[gmeow_test_batch_macros::batch_test]
fn fixture_signed_commit() {
    let g = fixture_store();
    assert!(g.has(
        Some(&format!("{EX}releaseCommit")),
        Some(RDF_TYPE),
        Some(&format!("{GMEOW}Commit")),
    ));
    assert!(g.has(
        Some(&format!("{EX}releaseCommit")),
        Some(&format!("{GMEOW}hasSignature")),
        Some(&format!("{EX}commitSignature")),
    ));
    assert!(g.has(
        Some(&format!("{EX}commitSignature")),
        Some(&format!("{GMEOW}signedBy")),
        Some(&format!("{EX}alice")),
    ));
    assert!(g.has(
        Some(&format!("{EX}commitSignature")),
        Some(&format!("{GMEOW}signingKey")),
        Some(&format!("{EX}aliceEd25519")),
    ));
}

#[gmeow_test_batch_macros::batch_test]
fn fixture_signed_tag() {
    let g = fixture_store();
    assert!(g.has(
        Some(&format!("{EX}tagV1_0_0")),
        Some(RDF_TYPE),
        Some(&format!("{GMEOW}Tag")),
    ));
    assert!(g.has(
        Some(&format!("{EX}tagV1_0_0")),
        Some(&format!("{GMEOW}pointsToCommit")),
        Some(&format!("{EX}releaseCommit")),
    ));
    assert!(g.has(
        Some(&format!("{EX}tagV1_0_0")),
        Some(&format!("{GMEOW}hasSignature")),
        Some(&format!("{EX}tagSignature")),
    ));
    assert!(g.has(
        Some(&format!("{EX}tagSignature")),
        Some(&format!("{GMEOW}signedBy")),
        Some(&format!("{EX}alice")),
    ));
}

#[gmeow_test_batch_macros::batch_test]
fn fixture_release_with_doi() {
    let g = fixture_store();
    assert!(g.has(
        Some(&format!("{EX}v1_0_0")),
        Some(RDF_TYPE),
        Some(&format!("{GMEOW}Release")),
    ));
    assert!(g.has(
        Some(&format!("{EX}v1_0_0")),
        Some(&format!("{GMEOW}releaseTag")),
        Some(&format!("{EX}tagV1_0_0")),
    ));
    assert!(g.has_literal(
        &format!("{EX}v1_0_0"),
        &format!("{GMEOW}releaseDoi"),
        "10.5281/zenodo.1234567",
        "http://www.w3.org/2001/XMLSchema#string",
    ));
}

#[gmeow_test_batch_macros::batch_test]
fn fixture_build_activity() {
    let g = fixture_store();
    assert!(g.has(
        Some(&format!("{EX}buildV1_0_0")),
        Some(RDF_TYPE),
        Some(&format!("{GMEOW}BuildActivity")),
    ));
    assert!(g.has(
        Some(&format!("{EX}buildV1_0_0")),
        Some(&format!("{GMEOW}buildSource")),
        Some(&format!("{EX}releaseCommit")),
    ));
    assert!(g.has(
        Some(&format!("{EX}buildV1_0_0")),
        Some(&format!("{GMEOW}buildOutput")),
        Some(&format!("{EX}distTarball")),
    ));
    assert!(g.has_literal(
        &format!("{EX}buildV1_0_0"),
        &format!("{GMEOW}buildConfigUri"),
        "https://github.com/example/meowgraph/blob/v1.0.0/.github/workflows/release.yml",
        "http://www.w3.org/2001/XMLSchema#string",
    ));
    assert!(g.has(
        Some(&format!("{EX}buildV1_0_0")),
        Some(&format!("{GMEOW}eventType")),
        Some(&format!("{GMEOW}eventTypeBuild")),
    ));
    assert!(g.has(
        Some(&format!("{EX}githubActions")),
        Some(RDF_TYPE),
        Some(&format!("{GMEOW}Builder")),
    ));
}

#[gmeow_test_batch_macros::batch_test]
fn fixture_slsa_attestation() {
    let g = fixture_store();
    assert!(g.has(
        Some(&format!("{EX}slsaAttestation")),
        Some(RDF_TYPE),
        Some(&format!("{GMEOW}Attestation")),
    ));
    assert!(g.has(
        Some(&format!("{EX}slsaAttestation")),
        Some(&format!("{GMEOW}attestationType")),
        Some(&format!("{GMEOW}attestationTypeSLSAProvenance")),
    ));
    assert!(g.has(
        Some(&format!("{EX}slsaAttestation")),
        Some(&format!("{GMEOW}hasSLSALevel")),
        Some(&format!("{GMEOW}slsaLevel3")),
    ));
    assert!(g.has(
        Some(&format!("{EX}slsaAttestation")),
        Some(&format!("{GMEOW}attestedSubject")),
        Some(&format!("{EX}distTarball")),
    ));
    assert!(g.has(
        Some(&format!("{EX}slsaAttestation")),
        Some(&format!("{GMEOW}attestationArtifact")),
        Some(&format!("{EX}slsaArtifact")),
    ));
}

#[gmeow_test_batch_macros::batch_test]
fn fixture_cosign_signature() {
    let g = fixture_store();
    assert!(g.has(
        Some(&format!("{EX}distTarball")),
        Some(&format!("{GMEOW}hasSignature")),
        Some(&format!("{EX}cosignSignature")),
    ));
    assert!(g.has(
        Some(&format!("{EX}cosignSignature")),
        Some(&format!("{GMEOW}signedBy")),
        Some(&format!("{EX}alice")),
    ));
    assert!(g.has_literal(
        &format!("{EX}cosignSignature"),
        &format!("{GMEOW}signatureAlgorithm"),
        "ed25519",
        "http://www.w3.org/2001/XMLSchema#string",
    ));
    assert!(g.has(
        Some(&format!("{EX}cosignSignature")),
        Some(&format!("{GMEOW}signingKey")),
        Some(&format!("{EX}aliceEd25519")),
    ));
}

#[gmeow_test_batch_macros::batch_test]
fn fixture_rekor_entry() {
    let g = fixture_store();
    assert!(g.has(
        Some(&format!("{EX}slsaAttestation")),
        Some(&format!("{GMEOW}transparencyLogEntry")),
        Some(&format!("{EX}rekorEntry")),
    ));
    assert!(g.has(
        Some(&format!("{EX}rekorEntry")),
        Some(RDF_TYPE),
        Some(&format!("{GMEOW}TransparencyLogEntry")),
    ));
    assert!(g.has_literal(
        &format!("{EX}rekorEntry"),
        &format!("{GMEOW}logEntryUrl"),
        "https://rekor.sigstore.dev/api/v1/log/entries/24296fb24b8ad77a…",
        "http://www.w3.org/2001/XMLSchema#string",
    ));
}

#[gmeow_test_batch_macros::batch_test]
fn fixture_swhid_on_commit() {
    let g = fixture_store();
    // contentDigest values are literals, so objects() returns nothing; collect
    // the literal lexical forms via SPARQL SELECT.
    let (vars, rows) = g.select(
        &[],
        &format!(
            "PREFIX ex: <{EX}>\n\
         PREFIX gmeow: <{GMEOW}>\n\
         SELECT ?d WHERE {{ ex:releaseCommit gmeow:contentDigest ?d }}"
        ),
    );
    let idx = vars.iter().position(|v| v == "d").expect("?d projected");
    let values: Vec<String> = rows
        .into_iter()
        .filter_map(|r| match &r[idx] {
            Some(TermValue::Literal { lexical_form, .. }) => Some(lexical_form.clone()),
            _ => None,
        })
        .collect();
    assert!(
        values.iter().any(|v| v.contains("swh:")),
        "expected an SWHID content digest, got {values:?}"
    );
}

// ── Competency queries over the combined ontology + fixture graph ─────────────

fn combined_store() -> GraphStore {
    GraphStore::ontology_plus_ttl_file(&repo_root().join(FIXTURE))
}

#[gmeow_test_batch_macros::batch_test]
fn query_key_that_signed_commit() {
    let g = combined_store();
    let (vars, rows) = g.select(
        &[],
        &format!(
            "PREFIX gmeow: <{GMEOW}>\n\
         SELECT ?key WHERE {{\n\
             ?release a gmeow:Release ;\n\
                      gmeow:releaseTag ?tag .\n\
             ?tag gmeow:pointsToCommit ?commit .\n\
             ?commit gmeow:hasSignature ?sig .\n\
             ?sig gmeow:signingKey ?key .\n\
         }}"
        ),
    );
    let idx = vars
        .iter()
        .position(|v| v == "key")
        .expect("?key projected");
    let keys: BTreeSet<String> = rows
        .into_iter()
        .filter_map(|r| match &r[idx] {
            Some(TermValue::Iri(iri)) => Some(iri.clone()),
            _ => None,
        })
        .collect();
    assert!(
        keys.contains(&format!("{EX}aliceEd25519")),
        "expected alice's signing key among {keys:?}"
    );
}

#[gmeow_test_batch_macros::batch_test]
fn query_build_that_produced_artifact() {
    let g = combined_store();
    let (vars, rows) = g.select(&[], &format!(
        "PREFIX gmeow: <{GMEOW}>\n\
         SELECT ?build WHERE {{\n\
             ?commit gmeow:contentDigest \"swh:1:rev:0123456789abcdef0123456789abcdef01234567\"^^<http://www.w3.org/2001/XMLSchema#string> .\n\
             ?build a gmeow:BuildActivity ;\n\
                    gmeow:buildSource ?commit ;\n\
                    gmeow:buildOutput ?dist .\n\
         }}"
    ));
    let idx = vars
        .iter()
        .position(|v| v == "build")
        .expect("?build projected");
    let builds: BTreeSet<String> = rows
        .into_iter()
        .filter_map(|r| match &r[idx] {
            Some(TermValue::Iri(iri)) => Some(iri.clone()),
            _ => None,
        })
        .collect();
    assert!(
        builds.contains(&format!("{EX}buildV1_0_0")),
        "expected buildV1_0_0 among {builds:?}"
    );
}

#[gmeow_test_batch_macros::batch_test]
fn query_rekor_entry_for_attestation() {
    let g = combined_store();
    assert!(g.ask(
        &[],
        &format!(
            "PREFIX ex: <{EX}>\n\
         PREFIX gmeow: <{GMEOW}>\n\
         ASK {{\n\
             ex:v1_0_0 a gmeow:Release ;\n\
                       gmeow:releaseTag ?tag .\n\
             ?tag gmeow:pointsToCommit ?commit .\n\
             ?build a gmeow:BuildActivity ;\n\
                    gmeow:buildSource ?commit ;\n\
                    gmeow:buildOutput ?dist .\n\
             ?attestation a gmeow:Attestation ;\n\
                          gmeow:attestedSubject ?dist ;\n\
                          gmeow:transparencyLogEntry ?rekor .\n\
             ?rekor a gmeow:TransparencyLogEntry .\n\
         }}"
        )
    ));
}
