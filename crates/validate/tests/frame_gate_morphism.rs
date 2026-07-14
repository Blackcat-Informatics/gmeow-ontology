// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! W1 gate-fatal teeth: a Violation-severity frame MinCount finding is gate-fatal
//! through the PRODUCTION intern+gate morphism.
//!
//! The generated `gmeow:ExpressionFrameRequirementShape` (`sh:path
//! gmeow:hasReferenceFrame`, `sh:minCount 1`, `MinCountConstraintComponent`) emits at
//! `sh:Violation`. This test is the executable witness for WHY that severity is
//! gate-fatal: it drives a synthetic `ValidationResult` at that severity
//! (`ShaclSeverity::Violation`)
//! through the REAL lowering `gmeow_validate::findings::diag_from_shacl` — the same
//! production surface that interns SHACL results into canonical [`Diag`]s — and shows
//! the interned [`Grade`] is the gate-fatal up-set corner `(Error, DataShapeViolation,
//! Binding)`, so `gmeow_errors::grade::gate` calls it `Fatal`.
//!
//! The verdict is NOT hand-asserted: the three-axis grade is DERIVED by the real
//! `severity_from_shacl` / `DataShapeViolation` / `standpoint_from_shacl` mapping inside
//! `diag_from_shacl`. This is deterministic — it exercises the morphism the frame
//! guard's fatal severity relies on, independent of the live generated shape's
//! severity.

use gmeow_errors::grade::{FindingCategory, GateVerdict, Grade, Severity, Standpoint, gate};
use gmeow_validate::findings::diag_from_shacl;
use purrdf::shapes::report::{Severity as ShaclSeverity, ValidationResult};
use purrdf::shapes::term::{NamedNode, Term};

const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
const SHACL_MIN_COUNT: &str = "http://www.w3.org/ns/shacl#MinCountConstraintComponent";

/// A frame MinCount `ValidationResult` at the given severity — the exact shape of a
/// `gmeow:ExpressionFrameRequirementShape` violation: a frameless `gmeow:Expression`
/// focus node, `sh:path gmeow:hasReferenceFrame`, `MinCountConstraintComponent`,
/// sourced at `gmeow:ExpressionFrameRequirementShape`.
fn frame_min_count_result(severity: ShaclSeverity) -> ValidationResult {
    ValidationResult {
        focus_node: Term::NamedNode(NamedNode::new_unchecked(format!("{GMEOW_NS}examples/x"))),
        result_path: Some(Term::NamedNode(NamedNode::new_unchecked(format!(
            "{GMEOW_NS}hasReferenceFrame"
        )))),
        path_structure: None,
        value: None,
        source_constraint_component: NamedNode::new_unchecked(SHACL_MIN_COUNT),
        source_shape: Term::NamedNode(NamedNode::new_unchecked(format!(
            "{GMEOW_NS}ExpressionFrameRequirementShape"
        ))),
        severity,
        message: Some(
            "A Expression must carry at least one reference frame (gmeow:hasReferenceFrame)."
                .to_owned(),
        ),
        source_box_roles: Vec::new(),
        path_box_roles: Vec::new(),
        result_box_roles: Vec::new(),
        attributions: vec![],
    }
}

/// A Violation-severity frame MinCount finding, routed through the production
/// `diag_from_shacl` lowering, grades to the gate-fatal up-set corner and `gate()`
/// calls it `Fatal`. The three-axis witness `(Error, DataShapeViolation, Binding)` is
/// asserted explicitly — each axis is DERIVED by the real lowering, not literal-built.
#[test]
fn violation_frame_finding_grades_gate_fatal_through_production_lowering() {
    // Drive the PRODUCTION intern+gate path: the real `diag_from_shacl` lowering derives
    // the grade from the ValidationResult (severity_from_shacl → Error, the honest SHACL
    // DataShapeViolation category, standpoint_from_shacl → Binding). No hand-built Grade.
    let diag = diag_from_shacl(&frame_min_count_result(ShaclSeverity::Violation));
    let grade = diag.grade();

    // The explicit three-axis witness: the derived grade is exactly the gate-fatal corner.
    assert_eq!(
        grade,
        Grade::new(
            Severity::Error,
            FindingCategory::DataShapeViolation,
            Standpoint::Binding
        ),
        "a Violation-severity frame MinCount finding must lower to (Error, DataShapeViolation, Binding)"
    );

    // The gate morphism the whole discharge relies on: this up-set grade is Fatal.
    assert_eq!(
        gate(grade),
        GateVerdict::Fatal,
        "the up-set grade (Error, DataShapeViolation, Binding) must gate Fatal"
    );
}

/// Contrast witness (the PRE-migration state): the SAME frame finding at `sh:Warning`
/// lowers — through the SAME production `diag_from_shacl` — to a non-Binding grade that
/// does NOT gate Fatal. This is exactly why the Warning→Violation flip is load-bearing:
/// the ONLY axis that changes is severity, and it is what moves the standpoint into the
/// gate-fatal up-set.
#[test]
fn warning_frame_finding_does_not_gate_fatal_through_production_lowering() {
    let diag = diag_from_shacl(&frame_min_count_result(ShaclSeverity::Warning));
    let grade = diag.grade();

    assert_eq!(
        grade,
        Grade::new(
            Severity::Warning,
            FindingCategory::DataShapeViolation,
            Standpoint::Perspectival
        ),
        "a Warning-severity frame MinCount finding lowers to a Perspectival (non-Binding) grade"
    );
    assert_ne!(
        gate(grade),
        GateVerdict::Fatal,
        "a Warning (Perspectival) frame finding must NOT gate Fatal — the pre-migration state"
    );
}
