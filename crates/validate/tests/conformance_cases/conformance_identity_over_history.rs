// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_identity_over_history.py
//!
//! Identity over immutable history (the `.mailmap` model): a contributor
//! transition keeps both old and new identities co-equal; the old identity is
//! suppressed (`displayable false`), not deleted; an AI agent is a first-class
//! `SoftwareAgent` whose authorship claim carries statement-level confidence and
//! self-assertion metadata.
//!
//! Python-fn → Rust-fn mapping:
//! - `test_contributor_transition_preserves_both_identities` →
//!   [`contributor_transition_preserves_both_identities`]
//!   (merged ontology + fixture, via `GraphStore::ontology_plus_ttl_file`).
//! - `test_ai_author_is_software_agent_with_statement_metadata` →
//!   [`ai_author_is_software_agent_with_statement_metadata`]
//!   (fixture membership + the compiled statement `owl:Axiom` reifier walk
//!   over `generated/statements/gmeow-statements.owl.ttl`).
//! - `test_suppressed_identity_passes_shacl` →
//!   [`suppressed_identity_passes_shacl`] (fixture-only `validate`, the
//!   twin of Python `run_shacl`).
//!
//! - `test_mailmap_projection_emits_canonical_and_suppressed_lines` →
//!   [`mailmap_projection_emits_canonical_and_suppressed_lines`] — runs the
//!   committed `mailmap.rq` CONSTRUCT in-process over the fixture and asserts the
//!   canonical + suppressed lines (the end-to-end projection OUTPUT, no Python).

use crate::conformance_support::*;
use purrdf::slice::rdf_query::Object;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const EX: &str = "https://blackcatinformatics.ca/gmeow/examples/";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const OWL_SAME_AS: &str = "http://www.w3.org/2002/07/owl#sameAs";

/// Repo-relative path to the issue-234 fixture.
const FIXTURE_REL: &str = "tests/fixtures/coverage/identity-over-history.ttl";
fn gmeow(local: &str) -> String {
    format!("{GMEOW}{local}")
}

fn ex(local: &str) -> String {
    format!("{EX}{local}")
}

/// Twin of `test_contributor_transition_preserves_both_identities`.
///
/// Eve and Evan coexist; the historical `AuthorIdentity` is not erased, and the
/// two are co-equal (no `owl:sameAs`), Principle 9.
#[gmeow_test_batch_macros::batch_test]
fn contributor_transition_preserves_both_identities() {
    let g = GraphStore::ontology_plus_ttl_file(&repo_root().join(FIXTURE_REL));
    assert!(g.has(
        Some(&ex("evanIdentity")),
        Some(RDF_TYPE),
        Some(&gmeow("AuthorIdentity"))
    ));
    assert!(g.has_literal(
        &ex("evanIdentity"),
        &gmeow("displayable"),
        "false",
        XSD_BOOLEAN
    ));
    assert!(g.has_literal(&ex("eveName"), &gmeow("displayable"), "true", XSD_BOOLEAN));
    assert!(g.has(
        Some(&ex("transitionCommit")),
        Some(&gmeow("commitAuthorIdentity")),
        Some(&ex("evanIdentity"))
    ));
    assert!(g.has(
        Some(&ex("transitionCommit")),
        Some(&gmeow("authoredBy")),
        Some(&ex("eve"))
    ));
    // Principle 9: co-equal, not merged.
    assert!(!g.has(
        Some(&ex("eve")),
        Some(OWL_SAME_AS),
        Some(&ex("evanIdentity"))
    ));
}

/// Twin of `test_ai_author_is_software_agent_with_statement_metadata`.
///
/// GitHub-Copilot-Bot is a `SoftwareAgent`; the `authoredBy` claim on the AI
/// commit is reified in the compiled statements as an `owl:Axiom` carrying
/// exactly one `confidence` (0.9) and one `selfAsserted` (true).
#[gmeow_test_batch_macros::batch_test]
fn ai_author_is_software_agent_with_statement_metadata() {
    // Fixture membership (merged ontology + fixture).
    let fixture = GraphStore::ontology_plus_ttl_file(&repo_root().join(FIXTURE_REL));
    assert!(fixture.has(
        Some(&ex("copilot")),
        Some(RDF_TYPE),
        Some(&gmeow("SoftwareAgent"))
    ));
    assert!(fixture.has(
        Some(&ex("aiCommit")),
        Some(&gmeow("authoredBy")),
        Some(&ex("copilot"))
    ));

    // The compiled statement OWL carries the reified annotation axiom.
    let statements =
        GraphStore::parse_ttl(&authenticated_corpus_text("validate-statements-owl.ttl"));

    // Find the owl:Axiom reifying <aiCommit> gmeow:authoredBy <copilot>.
    let ai_commit = ex("aiCommit");
    let authored_by = gmeow("authoredBy");
    let copilot = ex("copilot");
    let axiom = statements
        .subjects_of_type_h(OWL_AXIOM)
        .into_iter()
        .find(|ax| {
            let sources = statements.objects_h(ax, OWL_ANNOTATED_SOURCE);
            let props = statements.objects_h(ax, OWL_ANNOTATED_PROPERTY);
            let targets = statements.objects_h(ax, OWL_ANNOTATED_TARGET);
            sources
                .iter()
                .any(|o| matches!(o, Object::Named(iri) if *iri == ai_commit))
                && props
                    .iter()
                    .any(|o| matches!(o, Object::Named(iri) if *iri == authored_by))
                && targets
                    .iter()
                    .any(|o| matches!(o, Object::Named(iri) if *iri == copilot))
        })
        .expect("OWL axiom for AI authorship not found in compiled statements");

    // Exactly one confidence, == 0.9.
    let confidences = statements.objects_h(&axiom, &gmeow("confidence"));
    assert_eq!(
        confidences.len(),
        1,
        "expected one confidence, got {confidences:?}"
    );
    match &confidences[0] {
        Object::Literal { value, .. } => {
            let parsed: f64 = value.parse().expect("confidence is numeric");
            assert!(
                (parsed - 0.9).abs() < 1e-9,
                "expected confidence 0.9, got {value}"
            );
        }
        other => panic!("expected a literal confidence, got {other:?}"),
    }

    // Exactly one selfAsserted, == true.
    let self_asserted = statements.objects_h(&axiom, &gmeow("selfAsserted"));
    assert_eq!(
        self_asserted.len(),
        1,
        "expected one selfAsserted, got {self_asserted:?}"
    );
    match &self_asserted[0] {
        Object::Literal {
            value, datatype, ..
        } => {
            assert_eq!(value, "true", "selfAsserted must be true");
            assert_eq!(datatype, XSD_BOOLEAN, "selfAsserted must be xsd:boolean");
        }
        other => panic!("expected a literal selfAsserted, got {other:?}"),
    }
}

/// Twin of `test_suppressed_identity_passes_shacl`.
///
/// A suppressed contributor identity is retained and SHACL-valid (fixture-only
/// `validate`, the twin of Python `run_shacl`).
#[gmeow_test_batch_macros::batch_test]
fn suppressed_identity_passes_shacl() {
    let path = repo_root().join(FIXTURE_REL);
    let nt = ttl_file_to_nt(&path);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "identity-over-history fixture must SHACL-pass; violations: {:?}",
        violations(&report)
    );

    let g = GraphStore::parse_ttl_file(&path);
    assert!(g.has(
        Some(&ex("evanIdentity")),
        Some(RDF_TYPE),
        Some(&gmeow("AuthorIdentity"))
    ));
    assert!(g.has_literal(
        &ex("evanIdentity"),
        &gmeow("displayable"),
        "false",
        XSD_BOOLEAN
    ));
}

/// Twin of `test_mailmap_projection_emits_canonical_and_suppressed_lines`.
///
/// Runs the committed `.mailmap` projection query (`generated/queries/mailmap.rq`,
/// a SPARQL CONSTRUCT) in-process over the issue-234 fixture and asserts the
/// projection OUTPUT: (a) the canonical mailmap line survives for the displayable
/// identity, and (b) the suppressed historical `AuthorIdentity` yields a
/// canonical→old-bytes mapping line (Principle 10 — the old bytes are preserved,
/// not erased). This restores the end-to-end mailmap output assertion natively.
#[gmeow_test_batch_macros::batch_test]
fn mailmap_projection_emits_canonical_and_suppressed_lines() {
    let src = GraphStore::parse_ttl_file(&repo_root().join(FIXTURE_REL));
    let query = read_query("mailmap.rq");
    let projected = src.construct(&[], &query);

    // (a) The canonical mailmap line for the displayable current identity.
    assert!(
        projected.has_literal(
            &ex("eve"),
            &gmeow("mailmapEntry"),
            "Eve <eve@example.com>",
            XSD_STRING
        ),
        "canonical mailmap line for the displayable identity must be projected"
    );

    // (b) The suppressed old identity maps its historical bytes to the canonical
    // line: CONCAT(canonical, " ", oldBytes).
    assert!(
        projected.has_literal(
            &ex("evanIdentity"),
            &gmeow("projectedMailmapMapping"),
            "Eve <eve@example.com> Evan <evan@example.com>",
            XSD_STRING
        ),
        "suppressed old→canonical mailmap mapping line must be projected"
    );
}
