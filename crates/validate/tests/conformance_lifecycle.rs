// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_lifecycle.py (#867)
mod conformance_support;
use conformance_support::*;

// ── Tests migrated from tests/test_lifecycle.py ───────────────────────────────

/// `test_wellformed_entity_existence_conforms` — a fully well-formed
/// EntityExistence (entity + interval) must pass SHACL.
///
/// Mirrors Python: `run_shacl(_fixture("entity-existence-wellformed"))`.
#[test]
fn wellformed_entity_existence_conforms() {
    let nt = fixture_as_nt("shapes", "entity-existence-wellformed");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "well-formed EntityExistence must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_malformed_entity_existence_is_flagged` — an EntityExistence missing
/// its entity and/or interval must be flagged with violations mentioning
/// `existenceEntity` and `duringInterval`.
///
/// Mirrors Python: `run_shacl(_fixture("entity-existence-malformed"))`.
#[test]
fn malformed_entity_existence_is_flagged() {
    let nt = fixture_as_nt("shapes", "entity-existence-malformed");
    let report = validate(&nt);
    assert!(!ok(&report), "malformed EntityExistence must fail SHACL");
    let joined = violations(&report).join("\n");
    assert!(
        joined.contains("existenceEntity") && joined.contains("duringInterval"),
        "violations must mention existenceEntity and duringInterval; got: {joined}"
    );
}
