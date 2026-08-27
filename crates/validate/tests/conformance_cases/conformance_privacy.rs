// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_privacy.py
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
//!
//! Migrated to native GraphStore/projection twins below (not SHACL):
//!   - `test_no_preferred_or_primary_sensitivity_term`: kernel-module subject sweep.
//!   - `test_odrl_projection_emits_privacy_policy`: `odrl.rq` projection round-trip.
use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;

// ── Tests migrated from tests/test_privacy.py ────────────────────────────────

#[batch_cases]
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

// ── GraphStore / projection twins migrated from tests/test_privacy.py ─────────

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const ODRL: &str = "http://www.w3.org/ns/odrl/2/";
const EX_PRIV: &str = "https://example.org/privacy/";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}
fn odrl(local: &str) -> String {
    format!("{ODRL}{local}")
}
fn ex(local: &str) -> String {
    format!("{EX_PRIV}{local}")
}

/// Twin of `test_no_preferred_or_primary_sensitivity_term` (Principle 9): no
/// `gmeow:` term in the kernel module whose local name (containing no `/`) starts
/// with `primary`/`preferred`. Native re-expression of the Python subject sweep —
/// strip the namespace prefix to recover the local name, exactly as Python sliced
/// `str(s)[len(NAMESPACE):]`, then apply the same case-insensitive prefix test.
#[gmeow_test_batch_macros::batch_test]
fn no_preferred_or_primary_sensitivity_term() {
    let g = GraphStore::parse_ttl_file(&repo_root().join("slices/core/kernel/module.ttl"));
    let offenders = g.primary_or_preferred_terms();
    assert!(
        offenders.is_empty(),
        "primary/preferred sensitivity term leaked: {offenders:?}"
    );
}

/// Twin of `test_odrl_projection_emits_privacy_policy`: the ODRL projection over the
/// privacy coverage fixture emits the policy, its permission (action/target/assignee),
/// and the purpose constraint (leftOperand/operator).
#[gmeow_test_batch_macros::batch_test]
fn odrl_projection_emits_privacy_policy() {
    let g = GraphStore::ontology_plus_ttl_file(
        &repo_root().join("tests/fixtures/coverage/privacy.ttl"),
    );
    let out = g.construct(&[], &read_query("generated/queries/odrl.rq"));
    assert!(out.has(
        Some(&ex("alice-privacy")),
        Some(RDF_TYPE),
        Some(&odrl("Set"))
    ));
    assert!(out.has(
        Some(&ex("alice-privacy")),
        Some(&odrl("permission")),
        Some(&ex("perm-process"))
    ));
    assert!(out.has(
        Some(&ex("perm-process")),
        Some(RDF_TYPE),
        Some(&odrl("Permission"))
    ));
    assert!(out.has(
        Some(&ex("perm-process")),
        Some(&odrl("action")),
        Some(&gm("actionProcessPersonalData"))
    ));
    assert!(out.has(
        Some(&ex("perm-process")),
        Some(&odrl("target")),
        Some(&ex("alice-home"))
    ));
    assert!(out.has(
        Some(&ex("perm-process")),
        Some(&odrl("assignee")),
        Some(&ex("deliveryCo"))
    ));
    // Constraint (purpose = delivery).
    assert!(out.has(
        Some(&ex("perm-process")),
        Some(&odrl("constraint")),
        Some(&ex("purpose-delivery"))
    ));
    assert!(out.has(
        Some(&ex("purpose-delivery")),
        Some(&odrl("leftOperand")),
        Some(&odrl("purpose"))
    ));
    assert!(out.has(
        Some(&ex("purpose-delivery")),
        Some(&odrl("operator")),
        Some(&odrl("eq"))
    ));
}
