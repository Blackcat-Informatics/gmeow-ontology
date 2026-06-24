// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_narrative_time.py (#867)
//!
//! Two SHACL-conformance tests from the narrative-time test module are ported
//! here. The remaining tests (TBox membership, dynamic sweep, SPARQL competency
//! query, fixture graph-walk) are retained in Python because they either call
//! `load_merged_graph`, iterate subjects dynamically, or run SPARQL queries.
//!
//! Retained in Python (not migrated):
//!   - `test_narrative_time_frame_is_a_reference_frame`: TBox `(triple) in g` membership.
//!   - `test_axis_vocab_spans_exactly_fabula_and_syuzhet`: dynamic sweep of `g.subjects(...)`.
//!   - `test_frame_properties_are_functional_with_correct_anchors`: TBox membership loop.
//!   - `test_position_is_an_object_with_frame_ordinal_label`: TBox membership checks.
//!   - `test_at_narrative_position_is_domain_free_and_not_functional`: TBox membership.
//!   - `test_flashback_fixture_carries_coexisting_orders`: fixture graph-walk (`g.objects()`).
//!   - `test_competency_narrative_time_axes_query`: SPARQL competency query.

mod conformance_support;
use conformance_support::*;

/// `test_wellformed_narrative_time_fixture_conforms` — the well-formed
/// narrative-time fixture (one creative work, two frames, flashback with
/// coexisting positions) must pass SHACL.
#[test]
fn wellformed_narrative_time_fixture_conforms() {
    let nt = fixture_as_nt("shapes", "narrative-time-wellformed");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "well-formed narrative-time fixture must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_malformed_narrative_time_fixture_is_flagged` — the malformed fixture
/// carries three deliberate violations (no axis, cross-anchor mismatch,
/// frameless bare position) and must produce exactly those SHACL errors.
#[test]
fn malformed_narrative_time_fixture_is_flagged() {
    let nt = fixture_as_nt("shapes", "narrative-time-malformed");
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "malformed narrative-time fixture must fail SHACL"
    );
    let errors = violations(&report).join("\n");
    assert!(
        errors.contains("exactly one gmeow:narrativeTimeAxis"),
        "expected 'exactly one gmeow:narrativeTimeAxis' in errors; got:\n{errors}"
    );
    assert!(
        errors.contains("never the other anchor"),
        "expected 'never the other anchor' in errors; got:\n{errors}"
    );
    assert!(
        errors.contains("exactly one reference frame (gmeow:positionFrame)"),
        "expected 'exactly one reference frame (gmeow:positionFrame)' in errors; got:\n{errors}"
    );
}
