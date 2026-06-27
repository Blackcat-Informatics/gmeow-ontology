// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_organization.py (#867)
//!
//! Each test loads a fixture file, converts it to N-Triples, and validates
//! against the whole shapes corpus.
//!
//! Retained in Python (not migrated):
//!   - `test_contested_membership_coexists`: mixes SHACL check with `g.objects()`
//!     graph content assertions not expressible in Rust without a query API.
//!   - `test_contested_succession_coexists`: same fixture, same graph-content pattern.
//!   - `test_withdrawn_recognition_suppressed_not_deleted`: checks `(triple) in g`
//!     graph membership after SHACL — requires graph query.
//!   - `test_post_seat_independent_of_holder`: `result.ok` + `g.objects()` + set check.
//!   - `test_post_successive_holders`: `result.ok` + `g.subjects()` set check.
//!   - `test_site_location`: `result.ok` + `g.objects()` + `in g` membership checks.
//!   - `test_change_event_entailments`: `result.ok` + `g.objects()` set checks.
//!   - `test_wellformed_legal_identifier_passes`: requires `g.remove()` graph mutation
//!     before validation — not expressible as a pure fixture test.
//!   - `test_no_preferred_or_primary_org_term`: pure TBox sweep over `_graph()`.
//!   - `test_change_event_type_values_exist`: cross-slice `_graph()` check; docstring
//!     marks RETAIN ("narrowing to scopeModule would miss cross-slice violations").

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

// ── Tests migrated from tests/test_organization.py ───────────────────────────

#[rstest]
#[case::membership_fills_post_org_mismatch_warns(
    Case::file("coverage", "organization-posts")
        .warnings(&["fills a Post whose organization differs"])
)]
#[case::legal_identifier_requires_scheme(
    Case::file("coverage", "organization-legal-identity")
        .fails()
        .violations(&["must declare a gmeow:identifierScheme"])
)]
fn organization(#[case] case: Case) {
    case.run();
}
