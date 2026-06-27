// SPDX-License-Identifier: AGPL-3.0-only
// Conformance twins migrated from tests/test_rights.py (#867)

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

// ── Tests migrated from tests/test_rights.py ─────────────────────────────────

#[rstest]
#[case::wellformed_rights_fixture_conforms(Case::file("shapes", "rights-wellformed"))]
#[case::malformed_rights_fixture_is_flagged(
    Case::file("shapes", "rights-malformed")
        .fails()
        .violations(&[
            "must govern exactly one asset",
            "must regulate exactly one action",
            "must have at least one holder",
            "must name at least one licensor",
            "exactly one mark",
        ])
)]
#[case::expired_trademark_warns_but_does_not_fail(
    Case::file("shapes", "rights-expired-warning")
        .warnings(&["displayable false"])
)]
fn rights(#[case] case: Case) {
    case.run();
}
