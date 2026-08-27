// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_disclosure.py
//!
//! Covers the closed-world SHACL shapes for the disclosure control facility
//! leak detection, well-formed conformance, and conditional-disclosure
//! warning tolerance.
//!
//! Retained in Python (not migrated):
//!   - `test_projection_context_class_structure`: calls `_graph()` /
//!     `load_merged_graph` for TBox membership checks — pure OWL, not SHACL.
//!   - `test_disclosure_policy_class_structure`: same reason.
//!   - `test_eligible_for_consumer_property_structure`: same reason.
//!   - `test_has_disclosure_policy_property_structure`: same reason.
//!   - `test_projection_context_seeds_declared`: iterates subjects dynamically
//!     via `_graph()` — dynamic sweep, not portable.
//!   - `test_disclosure_policy_seeds_declared`: same reason.
//!   - `test_disclosure_orthogonal_to_other_axes`: iterates axes via
//!     `combinations()` — dynamic sweep.
//!   - `test_disclosure_orthogonal_to_granularity`: `_graph()` membership.
//!   - `test_no_preferred_or_primary_disclosure_term`: disk-iterates
//!     `module_path("kernel")` subjects dynamically.
//!   - `test_project_when_in_sparql_query`: reads a `.rq` file from disk and
//!     checks string content — not a SHACL conformance test.
//!   - `test_public_candidates_query_runnable`: SPARQL SELECT against
//!     `_projection_source()` + a competency `.rq` file.
//!   - `test_privacy_leaks_query_runnable`: same pattern.

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;

#[batch_cases]
#[case::leak_fixture_is_flagged(
    Case::file("shapes", "disclosure-leak")
        .fails()
        .violations(&["policyNeverPublic"])
)]
#[case::wellformed_disclosure_fixture_conforms(Case::file("shapes", "disclosure-wellformed"))]
#[case::conditional_disclosure_warns_but_does_not_fail(
    Case::file("shapes", "disclosure-conditional-warning")
        .warnings(&["sourceIndependenceIndependent"])
)]
fn disclosure(#[case] case: Case) {
    case.run();
}

// ── SPARQL / GraphStore twins migrated from tests/test_disclosure.py ───────────

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const EX_DISCLOSURE: &str = "https://example.org/disclosure/";
const SCHEMA: &str = "https://schema.org/";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

fn ex_disc(local: &str) -> String {
    format!("{EX_DISCLOSURE}{local}")
}

fn schema(local: &str) -> String {
    format!("{SCHEMA}{local}")
}

/// Twin of `test_no_preferred_or_primary_disclosure_term`: no `gmeow:` term in the
/// kernel module whose local name (no `/`) starts with `primary`/`preferred`.
/// Native re-expression of the Python subject sweep via a DISTINCT-subject SELECT.
#[gmeow_test_batch_macros::batch_test]
fn no_preferred_or_primary_disclosure_term() {
    let g = GraphStore::parse_ttl_file(&repo_root().join("slices/core/kernel/module.ttl"));
    let (_vars, rows) = g.select(&[], "SELECT DISTINCT ?s WHERE { ?s ?p ?o }");
    let mut offenders = Vec::new();
    for row in &rows {
        let Some(Some(term)) = row.first() else {
            continue;
        };
        let Some(iri) = term.as_iri() else {
            continue;
        };
        if let Some(local) = iri.strip_prefix(GMEOW) {
            let lower = local.to_lowercase();
            if !local.contains('/')
                && (lower.starts_with("primary") || lower.starts_with("preferred"))
            {
                offenders.push(iri.to_owned());
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "primary/preferred disclosure term leaked: {offenders:?}"
    );
}

/// Twin of `test_project_when_in_sparql_query`: re-expressed as native semantics.
/// The generated `schema-org.rq` projection carries the projectWhen guard
/// (`FILTER EXISTS { ?ent gmeow:eligibleForConsumer gmeow:consumerPublicSite }`)
/// on the description branch. Running the CONSTRUCT over the disclosure fixture,
/// the public-eligible entity's description projects while the non-eligible one is
/// dropped — the guard's behaviour, asserted as semantics rather than source text.
#[gmeow_test_batch_macros::batch_test]
fn project_when_gates_description_on_public_eligibility() {
    let g = GraphStore::parse_ttl_file(&repo_root().join("tests/fixtures/coverage/disclosure.ttl"));
    let query = read_query("generated/queries/schema-org.rq");
    let projected = g.construct(&[], &query);
    // Alice IS eligible for consumerPublicSite → her description projects.
    assert!(
        projected.contains_triple(
            &iri(&ex_disc("alice")),
            &iri(&schema("description")),
            &lit("Alice Public"),
        ),
        "public-eligible entity's description must project"
    );
    // Bob is NOT public-eligible → the projectWhen FILTER EXISTS guard drops him.
    assert!(
        !projected.contains_triple(
            &iri(&ex_disc("bob")),
            &iri(&schema("description")),
            &lit("Bob Internal"),
        ),
        "non-eligible entity's description must be gated out"
    );
}

/// Twin of `test_public_candidates_query_runnable`: `public-candidates.rq`, run
/// with `?consumer` pre-bound to consumerPublicSite (initBindings) over the
/// ontology+fixture, returns the public-safe alice (FILTER NOT EXISTS displayable
/// false, FILTER ?policy NOT IN sensitive policies).
#[gmeow_test_batch_macros::batch_test]
fn public_candidates_query_runnable() {
    QueryCase::new(
        "disclosure/public-candidates",
        &[Feature::FilterNotExists, Feature::InitBindings],
    )
    .over_ontology_plus("tests/fixtures/coverage/disclosure.ttl")
    .query_file("public-candidates.rq")
    .bind("consumer", iri(&gm("consumerPublicSite")))
    .select_contains_rows(vec![vec![
        iri(&ex_disc("alice")),
        iri(&gm("policyPublicSafe")),
    ]])
    .run();
}

/// Twin of `test_privacy_leaks_query_runnable`: `privacy-leaks.rq` (FILTER ?policy
/// IN neverPublic/internalOnly, FILTER ?consumer IN the public allowlist) finds
/// the leak — secretPlace is neverPublic yet eligible for consumerWikidata.
#[gmeow_test_batch_macros::batch_test]
fn privacy_leaks_query_runnable() {
    QueryCase::new("disclosure/privacy-leaks", &[])
        .over_ontology_plus("tests/fixtures/coverage/disclosure.ttl")
        .query_file("privacy-leaks.rq")
        .select_contains_rows(vec![vec![
            iri(&ex_disc("secretPlace")),
            iri(&gm("policyNeverPublic")),
            iri(&gm("consumerWikidata")),
        ]])
        .run();
}
