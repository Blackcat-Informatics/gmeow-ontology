// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! W2 gate-fatal teeth: a Violation-severity `ProfileOpenValueUseConstraint`
//! finding is gate-fatal through the PRODUCTION intern+gate morphism.
//!
//! The generated `gmeow:ProfileOpenValueUseConstraintProceduralConstraintShape`
//! (a `sh:SPARQLTarget` selecting each profile's `gmeow:profileOpenValue`
//! individuals, firing `SPARQLConstraintComponent` when the value is referenced
//! by no `gmeow:profileDescriptor`) emits at `sh:Violation`. This test is the
//! executable witness for WHY that severity is gate-fatal: it drives a synthetic
//! `ValidationResult` at that severity (`ShaclSeverity::Violation`) through the REAL
//! lowering `gmeow_validate::findings::diag_from_shacl` — the same production surface
//! that interns SHACL results into canonical [`Diag`]s — and shows the interned
//! [`Grade`] is the gate-fatal up-set corner `(Error, DataShapeViolation, Binding)`, so
//! `gmeow_errors::grade::gate` calls it `Fatal`.
//!
//! The verdict is NOT hand-asserted: the three-axis grade is DERIVED by the real
//! `severity_from_shacl` / `DataShapeViolation` / `standpoint_from_shacl` mapping
//! inside `diag_from_shacl`. This is deterministic — it exercises the morphism the
//! open-value guard's fatal severity relies on, independent of the live generated
//! shape's severity.

use gmeow_errors::grade::{FindingCategory, GateVerdict, Grade, Severity, Standpoint, gate};
use gmeow_validate::findings::diag_from_shacl;
use purrdf::shapes::report::{Severity as ShaclSeverity, ValidationResult};
use purrdf::shapes::term::{NamedNode, Term};

const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
const SHACL_SPARQL_CONSTRAINT: &str = "http://www.w3.org/ns/shacl#SPARQLConstraintComponent";

/// A `ProfileOpenValueUse` `ValidationResult` at the given severity — the exact shape
/// of a `gmeow:ProfileOpenValueUseConstraintProceduralConstraintShape` violation: an
/// orphan open-value individual (a `gmeow:SliceQualityDimension` referenced by no
/// `gmeow:profileDescriptor`) as the focus node, NO `sh:path` (the SPARQL constraint's
/// `SELECT $this` carries no result path), `SPARQLConstraintComponent`, sourced at the
/// projected procedural-constraint shape.
fn open_value_use_result(severity: ShaclSeverity) -> ValidationResult {
    ValidationResult {
        focus_node: Term::NamedNode(NamedNode::new_unchecked(format!(
            "{GMEOW_NS}examples/orphanDim"
        ))),
        // A `sh:sparql` node constraint firing `SELECT $this` binds no `sh:resultPath`.
        result_path: None,
        path_structure: None,
        value: None,
        source_constraint_component: NamedNode::new_unchecked(SHACL_SPARQL_CONSTRAINT),
        source_shape: Term::NamedNode(NamedNode::new_unchecked(format!(
            "{GMEOW_NS}ProfileOpenValueUseConstraintProceduralConstraintShape"
        ))),
        severity,
        message: Some(
            "Open value individuals must be referenced by at least one profile \
             descriptor — extensibility-by-construction guard."
                .to_owned(),
        ),
        source_box_roles: Vec::new(),
        path_box_roles: Vec::new(),
        result_box_roles: Vec::new(),
        attributions: vec![],
    }
}

/// A Violation-severity `ProfileOpenValueUse` finding, routed through the production
/// `diag_from_shacl` lowering, grades to the gate-fatal up-set corner and `gate()`
/// calls it `Fatal`. The three-axis witness `(Error, DataShapeViolation, Binding)` is
/// asserted explicitly — each axis is DERIVED by the real lowering, not literal-built.
#[test]
fn violation_open_value_finding_grades_gate_fatal_through_production_lowering() {
    // Drive the PRODUCTION intern+gate path: the real `diag_from_shacl` lowering derives
    // the grade from the ValidationResult (severity_from_shacl → Error, the honest SHACL
    // DataShapeViolation category, standpoint_from_shacl → Binding). No hand-built Grade.
    let diag = diag_from_shacl(&open_value_use_result(ShaclSeverity::Violation));
    let grade = diag.grade();

    // The explicit three-axis witness: the derived grade is exactly the gate-fatal corner.
    assert_eq!(
        grade,
        Grade::new(
            Severity::Error,
            FindingCategory::DataShapeViolation,
            Standpoint::Binding
        ),
        "a Violation-severity ProfileOpenValueUse finding must lower to \
         (Error, DataShapeViolation, Binding)"
    );

    // The gate morphism the whole discharge relies on: this up-set grade is Fatal.
    assert_eq!(
        gate(grade),
        GateVerdict::Fatal,
        "the up-set grade (Error, DataShapeViolation, Binding) must gate Fatal"
    );
}

/// Contrast witness (the PRE-migration state): the SAME open-value finding at
/// `sh:Warning` lowers — through the SAME production `diag_from_shacl` — to a
/// non-Binding grade that does NOT gate Fatal. This is exactly why the
/// Warning→Violation flip is load-bearing: the ONLY axis that changes is severity, and
/// it is what moves the standpoint into the gate-fatal up-set.
#[test]
fn warning_open_value_finding_does_not_gate_fatal_through_production_lowering() {
    let diag = diag_from_shacl(&open_value_use_result(ShaclSeverity::Warning));
    let grade = diag.grade();

    assert_eq!(
        grade,
        Grade::new(
            Severity::Warning,
            FindingCategory::DataShapeViolation,
            Standpoint::Perspectival
        ),
        "a Warning-severity ProfileOpenValueUse finding lowers to a Perspectival \
         (non-Binding) grade"
    );
    assert_ne!(
        gate(grade),
        GateVerdict::Fatal,
        "a Warning (Perspectival) open-value finding must NOT gate Fatal — the \
         pre-migration state"
    );
}
