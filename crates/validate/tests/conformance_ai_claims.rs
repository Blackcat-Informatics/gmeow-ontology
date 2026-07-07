// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_ai_claims.py
//!
//! Migrated test:
//!   - `test_normative_fixture_validates_against_the_full_graph` — MERGED mode:
//!     the Python test calls `load_merged_graph(include_imports=False)`, adds the
//!     `ai-normative.ttl` fixture, then runs `run_shacl(g)`.  The Rust twin uses
//!     `validate_with_ontology` which combines `base_ontology_nt()` with the
//!     fixture triples, identical semantics.
//!
//! Retained in Python (not migrated):
//!   - All tombstone absence tests (`test_no_parallel_claim_construct_exists`,
//!     etc.) — these iterate `load_merged_graph` checking `(s, None, None) not in g`
//!     patterns; no `run_shacl` call.
//!   - Seam membership tests (`test_memory_is_a_role_on_the_universal_claim_construct`,
//!     etc.) — vocabulary structural checks, no `run_shacl` call.
//!   - `test_assessment_seam_is_the_norms_extensions` — parses the fixture and
//!     checks specific triples; no `run_shacl` call.

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

#[rstest]
#[case::normative_fixture_validates_against_the_full_graph(
    Case::file("coverage", "ai-normative")
        .with_ontology()
)]
fn ai_claims(#[case] case: Case) {
    case.run();
}

// ── GraphStore twins migrated from tests/test_ai_claims.py ────────────────────

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDFS_IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";
const EX_AI_NORMATIVE: &str = "https://blackcatinformatics.ca/gmeow/examples/ai-normative/";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

fn ex(local: &str) -> String {
    format!("{EX_AI_NORMATIVE}{local}")
}

/// Twin of `test_no_parallel_claim_construct_exists`: gmeow:Observation IS the
/// universal claim construct — the earlier Claim/GeneratedClaim/ExtractedClaim
/// classes (and claimText) must never return as a subject.
#[test]
fn no_parallel_claim_construct_exists() {
    let g = GraphStore::ontology();
    for tombstone in ["Claim", "GeneratedClaim", "ExtractedClaim", "claimText"] {
        assert!(
            !g.has(Some(&gm(tombstone)), None, None),
            "parallel claim construct returned: {tombstone}"
        );
    }
}

/// Twin of `test_no_parallel_evaluation_construct_exists`: evaluation is the norms
/// extension's Assessment (judge-as-vantage), never a parallel metric construct.
#[test]
fn no_parallel_evaluation_construct_exists() {
    let g = GraphStore::ontology();
    for tombstone in [
        "MetricObservation",
        "EvaluationRun",
        "EvaluationMetric",
        "metricValue",
        "observesMetric",
        "scoresSubject",
    ] {
        assert!(
            !g.has(Some(&gm(tombstone)), None, None),
            "parallel evaluation construct returned: {tombstone}"
        );
    }
}

/// Twin of `test_no_duplicate_provenance_properties`: outputs hang off the EXISTING
/// wasGeneratedBy — no forward duplicates.
#[test]
fn no_duplicate_provenance_properties() {
    let g = GraphStore::ontology();
    for tombstone in ["producedOutput", "builtBy", "extractionMethod"] {
        assert!(
            !g.has(Some(&gm(tombstone)), None, None),
            "duplicate provenance property returned: {tombstone}"
        );
    }
}

/// Twin of `test_no_winner_machinery_anywhere`: contradictions surface; nothing
/// ranks them (P9).
#[test]
fn no_winner_machinery_anywhere() {
    let g = GraphStore::ontology();
    for banned in ["resolvedBy", "winningClaim", "reviewRating"] {
        assert!(
            !g.has(Some(&gm(banned)), None, None),
            "winner machinery returned: {banned}"
        );
    }
}

/// Twin of `test_no_new_identity_axes_were_minted`: the AI layer carries WHO SAID,
/// never WHO IS. Each of the ai / graphrag slices defines at least one term, and
/// none of those terms carries a gmeow:coequalFacet triple (no identity axis).
#[test]
fn no_new_identity_axes_were_minted() {
    let g = GraphStore::ontology();
    let coequal = gm("coequalFacet");
    for slice_iri in ["slices/ai", "slices/graphrag"] {
        let terms = g.subjects(RDFS_IS_DEFINED_BY, &gm(slice_iri));
        assert!(!terms.is_empty(), "no terms defined by {slice_iri}");
        for term in &terms {
            assert!(
                !g.has(Some(term), Some(&coequal), None),
                "{slice_iri} term {term} minted an identity axis (coequalFacet)"
            );
        }
    }
}

/// Twin of `test_assessment_seam_is_the_norms_extensions`: the fixture's evaluator
/// is an Assessment — the judge is just a vantage.
#[test]
fn assessment_seam_is_the_norms_extensions() {
    let g =
        GraphStore::parse_ttl_file(&repo_root().join("tests/fixtures/coverage/ai-normative.ttl"));
    assert!(
        g.has(
            Some(&ex("assessment-1")),
            Some(RDF_TYPE),
            Some(&gm("Assessment"))
        ),
        "assessment-1 must be a gmeow:Assessment"
    );
    assert!(
        g.has(
            Some(&ex("assessment-1")),
            Some(&gm("vantage")),
            Some(&ex("judge"))
        ),
        "assessment-1 vantage must be the judge"
    );
}
