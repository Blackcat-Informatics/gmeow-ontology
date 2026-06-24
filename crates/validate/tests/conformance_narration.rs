// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_narration.py (#867)
//!
//! Migrated tests (SHACL fixture-based):
//!   - `test_wellformed_narration_fixture_conforms`  → `wellformed_narration_fixture_conforms`
//!   - `test_malformed_narration_fixture_is_flagged` → `malformed_narration_fixture_is_flagged`
//!
//! Retained in Python (not migrated):
//!   - `test_seam_links_specialize_one_ancestor`: pure `_graph()` TBox membership checks.
//!   - `test_orientations_are_not_inverse_axioms`: pure `_graph()` TBox membership checks.
//!   - `test_narration_usage_is_a_reified_relator_with_open_subject`: `_graph()` TBox checks.
//!   - `test_narration_mode_vocab_seeds`: `_graph()` subject iteration.
//!   - `test_no_truth_bridge_from_unreliable_mode`: `_graph()` object iteration.
//!   - `test_fixture_obeys_the_efficiency_budget`: iterates fixture quads, no `run_shacl`.
//!   - `test_competency_cooccurrence_query_over_fixture`: SPARQL SELECT over fixture.

mod conformance_support;
use conformance_support::*;

/// `test_wellformed_narration_fixture_conforms` — the chapter-scale well-formed
/// narration fixture (flat links + one promoted NarrationUsage) must pass SHACL.
#[test]
fn wellformed_narration_fixture_conforms() {
    let nt = fixture_as_nt("shapes", "narration-wellformed");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "well-formed narration fixture must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_malformed_narration_fixture_is_flagged` — the malformed fixture carries
/// three deliberate violations (no mode, no subject, no segment) and must fail
/// SHACL with messages naming all three missing properties.
#[test]
fn malformed_narration_fixture_is_flagged() {
    let nt = fixture_as_nt("shapes", "narration-malformed");
    let report = validate(&nt);
    assert!(!ok(&report), "malformed narration fixture must fail SHACL");
    let v = violations(&report);
    let joined = v.join("\n");
    assert!(
        joined.contains("at least one gmeow:narrationMode"),
        "expected 'at least one gmeow:narrationMode' in violations; got: {joined:?}"
    );
    assert!(
        joined.contains("exactly one gmeow:narrationSubject"),
        "expected 'exactly one gmeow:narrationSubject' in violations; got: {joined:?}"
    );
    assert!(
        joined.contains("exactly one gmeow:narrationSegment"),
        "expected 'exactly one gmeow:narrationSegment' in violations; got: {joined:?}"
    );
}
