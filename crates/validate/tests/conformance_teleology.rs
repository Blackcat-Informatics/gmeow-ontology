// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_teleology.py (#867)
//!
//! Each test loads a fixture file from `tests/fixtures/shapes/` and validates
//! it against the whole shapes corpus using `validate()`.
//!
//! Retained in Python (not migrated):
//!   - `test_intrinsic_modes_are_grounded`: `(triple) in g` membership test
//!     on `_graph()` (cross-slice subject — gmeow:MentalMoment defined in
//!     the mentation slice, not the teleology module).
//!   - `test_no_preferred_or_primary_goal_terms`: dynamic whole-graph sweep
//!     over `g.subjects()`; scoping to the teleology module would narrow the
//!     live-set intent.
//!   - `test_competency_teleology_modes_query`: reads an external `.rq` file
//!     and asserts SPARQL SELECT result sets — not portable to SHACL engine.

mod conformance_support;
use conformance_support::*;

/// `test_wellformed_teleology_fixture_conforms` — a well-formed teleology
/// fixture passes SHACL without violations.
#[test]
fn wellformed_teleology_fixture_conforms() {
    let nt = fixture_as_nt("shapes", "teleology-wellformed");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "well-formed teleology fixture must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_malformed_teleology_fixture_is_flagged` — a malformed teleology
/// fixture must produce violations for exactly the known constraint messages.
#[test]
fn malformed_teleology_fixture_is_flagged() {
    let nt = fixture_as_nt("shapes", "teleology-malformed");
    let report = validate(&nt);
    assert!(!ok(&report), "malformed teleology fixture must fail SHACL");
    let errs = violations(&report).join("\n");
    assert!(
        errs.contains("exactly one gmeow:intentBearer"),
        "expected 'exactly one gmeow:intentBearer' in violations; got: {errs}"
    );
    assert!(
        errs.contains("distinct from its committed agent"),
        "expected 'distinct from its committed agent' in violations; got: {errs}"
    );
    assert!(
        errs.contains("never its own counter-goal"),
        "expected 'never its own counter-goal' in violations; got: {errs}"
    );
    assert!(
        errs.contains("exactly one gmeow:tenureAgent"),
        "expected 'exactly one gmeow:tenureAgent' in violations; got: {errs}"
    );
}
