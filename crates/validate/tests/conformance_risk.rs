// SPDX-License-Identifier: AGPL-3.0-only
// Conformance twins migrated from tests/test_risk.py (#867)
mod conformance_support;
use conformance_support::*;

// ── Tests migrated from tests/test_risk.py ───────────────────────────────────

/// `test_wellformed_risk_fixture_conforms` — the well-formed risk fixture
/// passes SHACL validation.
#[test]
fn wellformed_risk_fixture_conforms() {
    let nt = fixture_as_nt("shapes", "risk-wellformed");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "well-formed risk fixture must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_malformed_risk_fixture_is_flagged` — the malformed risk fixture fails
/// SHACL with all expected error messages present.
#[test]
fn malformed_risk_fixture_is_flagged() {
    let nt = fixture_as_nt("shapes", "risk-malformed");
    let report = validate(&nt);
    assert!(!ok(&report), "malformed risk fixture must NOT pass SHACL");
    let errors = violations(&report).join("\n");
    assert!(
        errors.contains("exactly one gmeow:hazardBearer"),
        "expected 'exactly one gmeow:hazardBearer' in errors; got: {errors}"
    );
    assert!(
        errors.contains("at least one feared gmeow:manifestedAsType"),
        "expected 'at least one feared gmeow:manifestedAsType' in errors; got: {errors}"
    );
    assert!(
        errors.contains("antecedent and consequent must be distinct"),
        "expected 'antecedent and consequent must be distinct' in errors; got: {errors}"
    );
    assert!(
        errors.contains("exactly one gmeow:causalModality"),
        "expected 'exactly one gmeow:causalModality' in errors; got: {errors}"
    );
    assert!(
        errors.contains("no causal link may reach itself"),
        "expected 'no causal link may reach itself' in errors; got: {errors}"
    );
    assert!(
        errors.contains("an ungraded cascade is just a story"),
        "expected 'an ungraded cascade is just a story' in errors; got: {errors}"
    );
    assert!(
        errors.contains("at least one gmeow:mitigationMeasure"),
        "expected 'at least one gmeow:mitigationMeasure' in errors; got: {errors}"
    );
    assert!(
        errors.contains("CausalLink (barrier on the chain) or a Hazard"),
        "expected 'CausalLink (barrier on the chain) or a Hazard' in errors; got: {errors}"
    );
}
