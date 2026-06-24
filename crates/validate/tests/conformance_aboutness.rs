// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_aboutness.py (#867)
//!
//! Each test loads a fixture file from `tests/fixtures/shapes/` and validates
//! it against the whole shapes corpus using the native SHACL engine.

mod conformance_support;
use conformance_support::*;

/// `test_wellformed_aboutness_fixture_conforms` — a carrier can describe one
/// thing while enacting another — both cells valid.
#[test]
fn wellformed_aboutness_fixture_conforms() {
    let nt = fixture_as_nt("shapes", "aboutness-wellformed");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "wellformed aboutness fixture must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_malformed_aboutness_fixture_is_flagged` — hasAboutness must target a
/// vocabulary IRI, never a free literal.
#[test]
fn malformed_aboutness_fixture_is_flagged() {
    let nt = fixture_as_nt("shapes", "aboutness-malformed");
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "malformed aboutness fixture must fail SHACL; violations were empty"
    );
    let msgs = violations(&report);
    let combined = msgs.join("\n");
    assert!(
        combined.contains("not a free literal"),
        "expected 'not a free literal' in violation messages; got: {combined:?}"
    );
}
