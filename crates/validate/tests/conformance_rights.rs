// SPDX-License-Identifier: AGPL-3.0-only
// Conformance twins migrated from tests/test_rights.py (#867)
mod conformance_support;
use conformance_support::*;

// ── Tests migrated from tests/test_rights.py ─────────────────────────────────

/// `test_wellformed_rights_fixture_conforms` — every rights relator satisfies
/// the closed-world SHACL shapes: RightsStatement governs one asset, each rule
/// regulates one action, Copyright has work + holder, License names licensor,
/// Trademark has mark + holder (registered, so suppression warning does not fire).
#[test]
fn wellformed_rights_fixture_conforms() {
    let nt = fixture_as_nt("shapes", "rights-wellformed");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "well-formed rights fixture must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_malformed_rights_fixture_is_flagged` — each rights relator trips its
/// closed-world SHACL Violation:
/// - RightsStatement with no statementAbout → "must govern exactly one asset"
/// - Permission with no ruleAction → "must regulate exactly one action"
/// - Copyright with work but no holder → "must have at least one holder"
/// - License with no licensor → "must name at least one licensor"
/// - Trademark with no mark and no holder → "exactly one mark" fires
#[test]
fn malformed_rights_fixture_is_flagged() {
    let nt = fixture_as_nt("shapes", "rights-malformed");
    let report = validate(&nt);
    assert!(!ok(&report), "malformed rights fixture must NOT pass SHACL");
    let errors = violations(&report).join("\n");
    assert!(
        errors.contains("must govern exactly one asset"),
        "expected 'must govern exactly one asset' in violations; got: {errors}"
    );
    assert!(
        errors.contains("must regulate exactly one action"),
        "expected 'must regulate exactly one action' in violations; got: {errors}"
    );
    assert!(
        errors.contains("must have at least one holder"),
        "expected 'must have at least one holder' in violations; got: {errors}"
    );
    assert!(
        errors.contains("must name at least one licensor"),
        "expected 'must name at least one licensor' in violations; got: {errors}"
    );
    assert!(
        errors.contains("exactly one mark"),
        "expected 'exactly one mark' in violations; got: {errors}"
    );
}

/// `test_expired_trademark_warns_but_does_not_fail` — an expired trademark that
/// is not suppressed trips `gmeow:ExpiredTrademarkSuppressionShape` at
/// `sh:Warning` severity (Principle 10: suppression, never erasure). No
/// Violation fires; `ok()` must be true.
#[test]
fn expired_trademark_warns_but_does_not_fail() {
    let nt = fixture_as_nt("shapes", "rights-expired-warning");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "warning-only graph must pass; violations: {:?}",
        violations(&report)
    );
    let ws = warnings(&report);
    assert!(
        ws.iter().any(|w| w.contains("displayable false")),
        "expected a warning containing 'displayable false'; got: {ws:?}"
    );
}
