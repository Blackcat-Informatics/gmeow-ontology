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
use rstest::rstest;

// ── Tests migrated from tests/test_privacy.py ────────────────────────────────

#[rstest]
#[case::wellformed_privacy_fixture_conforms(Case::file("shapes", "privacy-wellformed"))]
#[case::malformed_privacy_fixture_is_flagged(
    Case::file("shapes", "privacy-malformed")
        .fails()
        .violations(&[
            "must govern exactly one asset",
            "must regulate exactly one action",
        ])
        .warnings(&[
            "should name exactly one data subject",
            "at least one data controller",
        ])
)]
#[case::sensitive_value_warns_but_does_not_fail(
    Case::file("shapes", "privacy-sensitive-warning")
        .warnings(&["sensitivitySensitivePersonal"])
)]
fn privacy(#[case] case: Case) {
    case.run();
}
