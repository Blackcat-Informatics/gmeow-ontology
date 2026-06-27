// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Rust↔TTL cross-check (#768, ME2): the typed `logic:ReasoningResult` status
//! enums are the Rust authority; `slices/core/logic/module.ttl` is their lossy
//! projection (Principle 17). This test pins the two together — every enum
//! variant's `module.ttl` local name MUST be declared as a `logic:` individual,
//! so the Rust enums and the ontology surface can never silently diverge.

use gmeow_logic::result::{CompletenessStatus, EvaluationStatus, InformationState, InputStatus};
use gmeow_logic::result_shape::{ColumnBinding, RowCardinality, TermKind};

/// Read the committed logic-slice `module.ttl` (path-independent via the crate
/// manifest dir, so the test works regardless of the run CWD or worktree).
fn module_ttl() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../slices/core/logic/module.ttl"
    );
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

/// Assert each `logic:<local_name>` is declared (the term opens a statement on
/// its own line, so a newline-terminated match is exact and never a prefix hit).
fn assert_declared(ttl: &str, local_names: impl IntoIterator<Item = &'static str>) {
    for name in local_names {
        let needle = format!("\nlogic:{name}\n");
        assert!(
            ttl.contains(&needle),
            "module.ttl is missing the declaration of logic:{name} \
             (the Rust ReasoningResult enum and the ontology surface have drifted)"
        );
    }
}

#[test]
fn every_input_status_individual_is_declared() {
    let ttl = module_ttl();
    assert_declared(&ttl, InputStatus::ALL.iter().map(|v| v.local_name()));
}

#[test]
fn every_evaluation_status_individual_is_declared() {
    let ttl = module_ttl();
    assert_declared(&ttl, EvaluationStatus::ALL.iter().map(|v| v.local_name()));
}

#[test]
fn every_completeness_status_individual_is_declared() {
    let ttl = module_ttl();
    assert_declared(&ttl, CompletenessStatus::ALL.iter().map(|v| v.local_name()));
}

#[test]
fn every_information_state_individual_is_declared() {
    let ttl = module_ttl();
    assert_declared(&ttl, InformationState::ALL.iter().map(|v| v.local_name()));
}

#[test]
fn the_carrier_class_and_value_classes_are_declared() {
    let ttl = module_ttl();
    assert_declared(
        &ttl,
        [
            "ReasoningResult",
            "ResultAssurance",
            "InputStatus",
            "EvaluationStatus",
            "CompletenessStatus",
            "InformationState",
        ],
    );
}

#[test]
fn every_term_kind_individual_is_declared() {
    let ttl = module_ttl();
    assert_declared(&ttl, TermKind::ALL.iter().map(|v| v.local_name()));
}

#[test]
fn every_column_binding_individual_is_declared() {
    let ttl = module_ttl();
    assert_declared(&ttl, ColumnBinding::ALL.iter().map(|v| v.local_name()));
}

#[test]
fn every_row_cardinality_individual_is_declared() {
    let ttl = module_ttl();
    assert_declared(&ttl, RowCardinality::ALL.iter().map(|v| v.local_name()));
}

#[test]
fn the_result_shape_classes_and_value_classes_are_declared() {
    let ttl = module_ttl();
    assert_declared(
        &ttl,
        [
            "ResultShape",
            "ResultColumn",
            "TermKind",
            "ColumnBinding",
            "RowCardinality",
        ],
    );
}
