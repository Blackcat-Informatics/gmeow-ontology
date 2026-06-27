// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance tests for rubrics slice SHACL shapes.

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

#[rstest]
#[case::wellformed_rubrics_fixture_conforms(Case::file("shapes", "rubrics-wellformed"))]
#[case::malformed_rubrics_fixture_is_flagged(
    Case::file("shapes", "rubrics-malformed")
        .fails()
        .violations(&[
            "reward and penalty poles must be distinct",
            "minimum must be strictly below its maximum",
            "at least one gmeow:anchorMeaning",
            "range minimum must not exceed",
            "must name exactly one gmeow:rewardPole",
            "binds at most one gmeow:usesScale",
            "must pin exactly one decimal gmeow:anchorRangeMin",
            "must lie within the scale",
            "may not redirect to the criterion that anchors it",
            "at least one of gmeow:viaSelector",
            "exactly one gmeow:exemplarPolarity",
            "a gmeow:assessmentCriterion, a gmeow:assessmentRubric, or both",
        ])
)]
fn rubrics(#[case] case: Case) {
    case.run();
}
