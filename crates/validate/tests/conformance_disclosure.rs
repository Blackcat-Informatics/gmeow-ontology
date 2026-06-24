// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_disclosure.py (#867)
//!
//! Covers the closed-world SHACL shapes for the disclosure control facility
//! (#225): leak detection, well-formed conformance, and conditional-disclosure
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

mod conformance_support;
use conformance_support::*;

/// `test_leak_fixture_is_flagged` — a Place carrying `policyNeverPublic` AND
/// `eligibleForConsumer consumerWikidata` (a public consumer) must trigger a
/// `DisclosureLeakShape` Violation.
#[test]
fn leak_fixture_is_flagged() {
    let nt = fixture_as_nt("shapes", "disclosure-leak");
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "disclosure-leak fixture must produce a Violation; got none"
    );
    let msgs = violations(&report);
    let joined = msgs.join("\n");
    assert!(
        joined.contains("policyNeverPublic"),
        "violation message must mention policyNeverPublic; got: {joined:?}"
    );
}

/// `test_wellformed_disclosure_fixture_conforms` — a public-safe name eligible
/// for a public consumer must pass SHACL with no violations.
#[test]
fn wellformed_disclosure_fixture_conforms() {
    let nt = fixture_as_nt("shapes", "disclosure-wellformed");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "disclosure-wellformed fixture must conform; violations: {:?}",
        violations(&report)
    );
}

/// `test_conditional_disclosure_warns_but_does_not_fail` — a fact carrying
/// `policyPublicOnlyWithIndependentSource` with no supporting independent
/// citation must produce a Warning (not a Violation).  `ok()` must be true
/// and at least one warning must mention `sourceIndependenceIndependent`.
#[test]
fn conditional_disclosure_warns_but_does_not_fail() {
    let nt = fixture_as_nt("shapes", "disclosure-conditional-warning");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "warning-only graph must pass (ok = no violations); errors: {:?}",
        violations(&report)
    );
    let warns = warnings(&report);
    assert!(
        warns
            .iter()
            .any(|w| w.contains("sourceIndependenceIndependent")),
        "at least one warning must mention sourceIndependenceIndependent; got: {warns:?}"
    );
}
