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
use rstest::rstest;

#[rstest]
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
