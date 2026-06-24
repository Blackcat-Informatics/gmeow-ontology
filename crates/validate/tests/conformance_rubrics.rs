// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance tests for rubrics slice SHACL shapes.
mod conformance_support;
use conformance_support::*;

/// `test_wellformed_rubrics_fixture_conforms` — a well-formed rubrics graph
/// passes SHACL.
#[test]
fn wellformed_rubrics_fixture_conforms() {
    let nt = fixture_as_nt("shapes", "rubrics-wellformed");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "well-formed rubrics fixture must conform; violations: {:?}",
        violations(&report)
    );
}

/// `test_malformed_rubrics_fixture_is_flagged` — a malformed rubrics graph
/// triggers all expected SHACL violation messages.
#[test]
fn malformed_rubrics_fixture_is_flagged() {
    let nt = fixture_as_nt("shapes", "rubrics-malformed");
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "malformed rubrics fixture must produce violations"
    );
    let errors = violations(&report).join("\n");
    assert!(
        errors.contains("reward and penalty poles must be distinct"),
        "missing 'reward and penalty poles must be distinct' in:\n{errors}"
    );
    assert!(
        errors.contains("minimum must be strictly below its maximum"),
        "missing 'minimum must be strictly below its maximum' in:\n{errors}"
    );
    assert!(
        errors.contains("at least one gmeow:anchorMeaning"),
        "missing 'at least one gmeow:anchorMeaning' in:\n{errors}"
    );
    assert!(
        errors.contains("range minimum must not exceed"),
        "missing 'range minimum must not exceed' in:\n{errors}"
    );
    assert!(
        errors.contains("must name exactly one gmeow:rewardPole"),
        "missing 'must name exactly one gmeow:rewardPole' in:\n{errors}"
    );
    assert!(
        errors.contains("binds at most one gmeow:usesScale"),
        "missing 'binds at most one gmeow:usesScale' in:\n{errors}"
    );
    assert!(
        errors.contains("must pin exactly one decimal gmeow:anchorRangeMin"),
        "missing 'must pin exactly one decimal gmeow:anchorRangeMin' in:\n{errors}"
    );
    assert!(
        errors.contains("must lie within the scale"),
        "missing 'must lie within the scale' in:\n{errors}"
    );
    assert!(
        errors.contains("may not redirect to the criterion that anchors it"),
        "missing 'may not redirect to the criterion that anchors it' in:\n{errors}"
    );
    assert!(
        errors.contains("at least one of gmeow:viaSelector"),
        "missing 'at least one of gmeow:viaSelector' in:\n{errors}"
    );
    assert!(
        errors.contains("exactly one gmeow:exemplarPolarity"),
        "missing 'exactly one gmeow:exemplarPolarity' in:\n{errors}"
    );
    assert!(
        errors.contains("a gmeow:assessmentCriterion, a gmeow:assessmentRubric, or both"),
        "missing 'a gmeow:assessmentCriterion, a gmeow:assessmentRubric, or both' in:\n{errors}"
    );
}
