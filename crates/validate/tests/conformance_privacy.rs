// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_privacy.py (#867)
//!
//! Migrated: the three `run_shacl`-based fixture tests.
//!
//! Retained in Python (not migrated):
//!   - `test_sensitivity_level_class_structure`: pure `_graph()` TBox triple-membership check.
//!   - `test_has_sensitivity_property_structure`: pure `_graph()` TBox check.
//!   - `test_value_vocab_spans_five_seeds`: iterates `load_merged_graph` subjects dynamically.
//!   - `test_privacy_roles_declared`: pure `_graph()` TBox triple-membership check.
//!   - `test_privacy_notice_is_information_object`: pure `_graph()` TBox check.
//!   - `test_has_privacy_notice_is_domain_free`: pure `_graph()` TBox check.
//!   - `test_action_process_personal_data_is_rights_action`: pure `_graph()` TBox check.
//!   - `test_sensitivity_orthogonal_to_other_axes`: `_graph()` + `combinations` iteration.
//!   - `test_sensitivity_orthogonal_to_granularity`: pure `_graph()` TBox check.
//!   - `test_no_preferred_or_primary_sensitivity_term`: parses `module_path("kernel")` + iterates subjects.
//!   - `test_odrl_projection_emits_privacy_policy`: uses `project_graph` (projection, not SHACL).
mod conformance_support;
use conformance_support::*;

// ── Tests migrated from tests/test_privacy.py ────────────────────────────────

/// `test_wellformed_privacy_fixture_conforms` — the well-formed privacy fixture
/// must pass SHACL with no violations.
#[test]
fn wellformed_privacy_fixture_conforms() {
    let nt = fixture_as_nt("shapes", "privacy-wellformed");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "well-formed privacy fixture must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_malformed_privacy_fixture_is_flagged` — the malformed privacy fixture
/// must produce violation-level errors for missing asset/action constraints and
/// warning-level results for missing consent metadata.
#[test]
fn malformed_privacy_fixture_is_flagged() {
    let nt = fixture_as_nt("shapes", "privacy-malformed");
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "malformed privacy fixture must fail SHACL; violations were empty"
    );
    let errs = violations(&report).join("\n");
    assert!(
        errs.contains("must govern exactly one asset"),
        "expected 'must govern exactly one asset' in violations; got: {errs}"
    );
    assert!(
        errs.contains("must regulate exactly one action"),
        "expected 'must regulate exactly one action' in violations; got: {errs}"
    );
    // Consent well-formedness is Warning severity (incomplete metadata is allowed).
    let warns = warnings(&report).join("\n");
    assert!(
        warns.contains("should name exactly one data subject"),
        "expected 'should name exactly one data subject' in warnings; got: {warns}"
    );
    assert!(
        warns.contains("at least one data controller"),
        "expected 'at least one data controller' in warnings; got: {warns}"
    );
}

/// `test_sensitive_value_warns_but_does_not_fail` — a graph that uses
/// `gmeow:sensitivitySensitivePersonal` must pass (no violations) but emit
/// at least one warning mentioning `sensitivitySensitivePersonal`.
#[test]
fn sensitive_value_warns_but_does_not_fail() {
    let nt = fixture_as_nt("shapes", "privacy-sensitive-warning");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "warning-only privacy graph must pass SHACL; violations: {:?}",
        violations(&report)
    );
    let warns = warnings(&report);
    assert!(
        warns
            .iter()
            .any(|w| w.contains("sensitivitySensitivePersonal")),
        "expected a warning mentioning 'sensitivitySensitivePersonal'; got: {warns:?}"
    );
}
