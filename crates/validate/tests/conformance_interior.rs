// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_interior.py (#867)
mod conformance_support;
use conformance_support::*;

/// `test_wellformed_interior_fixture_conforms` — the well-formed interior
/// fixture passes SHACL validation.
#[test]
fn wellformed_interior_fixture_conforms() {
    let nt = fixture_as_nt("shapes", "interior-wellformed");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "interior-wellformed fixture must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_malformed_interior_fixture_is_flagged` — the malformed interior
/// fixture fails SHACL with the expected violation messages.
#[test]
fn malformed_interior_fixture_is_flagged() {
    let nt = fixture_as_nt("shapes", "interior-malformed");
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "interior-malformed fixture must fail SHACL; violations were empty"
    );
    let errors = violations(&report).join("\n");
    assert!(
        errors.contains("exactly one gmeow:samplePosition"),
        "expected 'exactly one gmeow:samplePosition' in violations; got: {errors}"
    );
    assert!(
        errors.contains("exactly one gmeow:sampleState"),
        "expected 'exactly one gmeow:sampleState' in violations; got: {errors}"
    );
    assert!(
        errors.contains("protagonist-of-WHAT is half the claim"),
        "expected 'protagonist-of-WHAT is half the claim' in violations; got: {errors}"
    );
    assert!(
        errors.contains("an unnameable recurring unit is a tag"),
        "expected 'an unnameable recurring unit is a tag' in violations; got: {errors}"
    );
    assert!(
        errors.contains("rides the narration seam into a ContentSegment"),
        "expected 'rides the narration seam into a ContentSegment' in violations; got: {errors}"
    );
    assert!(
        errors.contains("exactly one gmeow:emotionBearer"),
        "expected 'exactly one gmeow:emotionBearer' in violations; got: {errors}"
    );
    assert!(
        errors.contains("at least one gmeow:emotionType"),
        "expected 'at least one gmeow:emotionType' in violations; got: {errors}"
    );
    assert!(
        errors.contains("must read SOMETHING"),
        "expected 'must read SOMETHING' in violations; got: {errors}"
    );
    assert!(
        errors.contains("half a reading is no reading"),
        "expected 'half a reading is no reading' in violations; got: {errors}"
    );
}
