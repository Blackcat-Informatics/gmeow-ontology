// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_standpoint.py (#867)

mod conformance_support;
use conformance_support::*;

// ── Migrated from tests/test_standpoint.py ───────────────────────────────────

/// `test_coexistence_fixture_conforms` — Contradictory standpoint-indexed claims
/// COEXIST with no violation (the centerpiece).  Loads the
/// `tests/fixtures/shapes/standpoint-coexistence.ttl` fixture directly.
#[test]
fn coexistence_fixture_conforms() {
    let nt = fixture_as_nt("shapes", "standpoint-coexistence");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "coexistence fixture must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_preferred_claim_is_flagged` — A claim that tries to crown a single
/// winner via `gmeow:primaryStandpoint` must trigger `sh:Violation`
/// (Principle 9: no single slot to win).  Loads
/// `tests/fixtures/shapes/standpoint-preferred-violation.ttl`.
#[test]
fn preferred_claim_is_flagged() {
    let nt = fixture_as_nt("shapes", "standpoint-preferred-violation");
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "preferred/primary claim must produce a SHACL violation"
    );
    let msgs = violations(&report);
    assert!(
        msgs.iter().any(|m| m.contains("preferred/primary")),
        "violation message must mention preferred/primary; got: {:?}",
        msgs
    );
}

/// `test_withdrawn_standpoint_warning_does_not_fail` — A withdrawn
/// (closed-interval) tenure without `gmeow:displayable false` warns but does
/// NOT hard-fail (Principle 10 — suppression, never erasure).  Loads
/// `tests/fixtures/shapes/standpoint-withdrawn-warning.ttl`.
#[test]
fn withdrawn_standpoint_warning_does_not_fail() {
    let nt = fixture_as_nt("shapes", "standpoint-withdrawn-warning");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "warning-only graph must pass SHACL; violations: {:?}",
        violations(&report)
    );
    let warns = warnings(&report);
    assert!(
        warns.iter().any(|w| w.contains("displayable false")),
        "expected a warning mentioning 'displayable false'; got: {:?}",
        warns
    );
}

/// `test_variety_coexistence_fixture_conforms` — Contradictory `varietyKind`
/// assertions COEXIST with no violation (Principle 9).  Loads
/// `tests/fixtures/shapes/variety-coexistence.ttl`.
#[test]
fn variety_coexistence_fixture_conforms() {
    let nt = fixture_as_nt("shapes", "variety-coexistence");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "variety-coexistence fixture must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_etymology_coexistence_fixture_conforms` — Contradictory
/// `derivationKind` assertions COEXIST with no violation (Principle 9).  Loads
/// `tests/fixtures/shapes/etymology-coexistence.ttl`.
#[test]
fn etymology_coexistence_fixture_conforms() {
    let nt = fixture_as_nt("shapes", "etymology-coexistence");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "etymology-coexistence fixture must pass SHACL; violations: {:?}",
        violations(&report)
    );
}
