// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_standpoint.py (#867)

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

// ── Migrated from tests/test_standpoint.py ───────────────────────────────────

#[rstest]
#[case::coexistence_fixture_conforms(Case::file("shapes", "standpoint-coexistence"))]
#[case::preferred_claim_is_flagged(
    Case::file("shapes", "standpoint-preferred-violation")
        .fails()
        .violations(&["preferred/primary"])
)]
#[case::withdrawn_standpoint_warning_does_not_fail(
    Case::file("shapes", "standpoint-withdrawn-warning")
        .warnings(&["displayable false"])
)]
#[case::variety_coexistence_fixture_conforms(Case::file("shapes", "variety-coexistence"))]
#[case::etymology_coexistence_fixture_conforms(Case::file("shapes", "etymology-coexistence"))]
fn standpoint(#[case] case: Case) {
    case.run();
}
