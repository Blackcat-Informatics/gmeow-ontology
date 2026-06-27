// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_interior.py (#867)

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

#[rstest]
#[case::wellformed_interior_fixture_conforms(Case::file("shapes", "interior-wellformed"))]
#[case::malformed_interior_fixture_is_flagged(
    Case::file("shapes", "interior-malformed")
        .fails()
        .violations(&[
            "exactly one gmeow:samplePosition",
            "exactly one gmeow:sampleState",
            "protagonist-of-WHAT is half the claim",
            "an unnameable recurring unit is a tag",
            "rides the narration seam into a ContentSegment",
            "exactly one gmeow:emotionBearer",
            "at least one gmeow:emotionType",
            "must read SOMETHING",
            "half a reading is no reading",
        ])
)]
fn interior(#[case] case: Case) {
    case.run();
}
