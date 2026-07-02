// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The sharp ADL→FOL fidelity test: the half-open magnitude interval, read from the vendored
//! OPT itself rather than transcribed by hand, and lowered through the **canonical derived
//! path** — OPT → `OptConstraintIr` → `logic:ValidationShapeIr` → SHACL — NOT a direct emit.
//!
//! This test drives the constraints axis end to end against the actual
//! `validations/openehr-bloodpressure/Blutdruck.opt`: it parses the systolic (`at0004`) and
//! diastolic (`at0005`) `C_DV_QUANTITY` magnitude intervals out of the OPT's XML, lifts the
//! systolic constraint to a `logic:` validation shape, projects that shape to SHACL, and
//! validates data against the *generated* shape — not a hand-copied constant. If the OPT ever
//! changes its boundary inclusivity or bounds, this test reads the new values and
//! (dis)proves the invariant against them.
//!
//! The Blutdruck OPT constrains both magnitudes with `lower_included=true`,
//! **`upper_included=false`** — a half-open `[0, 1000)` mm[Hg]. Lowering that ADL constraint
//! to SHACL must regenerate the boundary inclusivity EXACTLY: `lower_included=true →
//! sh:minInclusive`, `upper_included=false → sh:maxExclusive` (NEVER `sh:maxInclusive`). This
//! is the sharp check that `u ∘ d = id` holds on the *constraint* and not merely the data —
//! an off-by-one on the open boundary would silently admit `value == hi`.

use std::path::PathBuf;

use gmeow_logic_compile::opt_lift::lift_opt_to_validation_shape;
use gmeow_logic_compile::projections::shapes::project_validation_shape_shacl;
use gmeow_shacl::engine::{parse_shapes, validate_graphs};
use gmeow_shacl::model::sh::MAX_EXCLUSIVE_CONSTRAINT_COMPONENT;
use gmeow_shacl::openehr_opt::{read_magnitude_interval, read_opt_quantity_constraint};
use gmeow_shacl::shapes::Constraint;

const CLASS: &str = "https://blackcatinformatics.ca/gmeow/SystolicMeasurement";
const VALUE_PATH: &str = "https://blackcatinformatics.ca/gmeow/quantityValue";
const UNIT_PATH: &str = "https://blackcatinformatics.ca/gmeow/quantityUnit";
const SHAPE: &str = "https://gmeow.example/openehr/bp/SystolicMeasurementShape";

/// Reads the vendored Blutdruck OPT from disk (`crates/shacl` → repo `validations/`).
fn read_blutdruck_opt() -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../../validations/openehr-bloodpressure/Blutdruck.opt");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

/// The systolic `at0004` constraint lifted through the canonical path to SHACL Turtle.
fn systolic_shape_ttl(opt: &str) -> String {
    let constraint =
        read_opt_quantity_constraint(opt, "at0004", SHAPE, CLASS, VALUE_PATH, UNIT_PATH)
            .expect("read+package systolic C_DV_QUANTITY");
    let shape = lift_opt_to_validation_shape(&constraint).expect("lift systolic to logic:");
    // project_validation_shape_shacl emits a single shape without the prefix header (the
    // multi-shape document adds it); prepend it so the SHACL validator can parse `sh:`/`xsd:`.
    format!(
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n{}",
        project_validation_shape_shacl(&shape)
    )
}

/// A data node carrying a magnitude value AND the required `mm[Hg]` unit (so the units
/// property shape is satisfied and only the magnitude boundary can violate).
fn data_node(local: &str, value: &str) -> String {
    format!(
        "<https://gmeow.example/openehr/bp/{local}> \
         <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <{CLASS}> .\n\
         <https://gmeow.example/openehr/bp/{local}> \
         <{VALUE_PATH}> \"{value}\"^^<http://www.w3.org/2001/XMLSchema#decimal> .\n\
         <https://gmeow.example/openehr/bp/{local}> \
         <{UNIT_PATH}> \"mm[Hg]\"^^<http://www.w3.org/2001/XMLSchema#string> .\n"
    )
}

#[test]
fn systolic_and_diastolic_intervals_read_from_opt_match() {
    let opt = read_blutdruck_opt();

    let systolic = read_magnitude_interval(&opt, "at0004").expect("read systolic magnitude");
    assert!(
        systolic.lower_included,
        "systolic lower_included, read {systolic:?}"
    );
    assert!(
        !systolic.upper_included,
        "systolic upper_included half-open, read {systolic:?}"
    );
    assert_eq!(
        systolic.lower, 0.0,
        "systolic lower bound, read {systolic:?}"
    );
    assert_eq!(
        systolic.upper, 1000.0,
        "systolic upper bound, read {systolic:?}"
    );
    assert_eq!(
        systolic.units, "mm[Hg]",
        "systolic units, read {systolic:?}"
    );

    let diastolic = read_magnitude_interval(&opt, "at0005").expect("read diastolic magnitude");
    assert!(
        diastolic.lower_included,
        "diastolic lower_included, read {diastolic:?}"
    );
    assert!(
        !diastolic.upper_included,
        "diastolic upper_included, read {diastolic:?}"
    );
    assert_eq!(
        diastolic, systolic,
        "diastolic must match the systolic [0, 1000) mm[Hg] interval"
    );
}

#[test]
fn half_open_lowers_to_max_exclusive_never_max_inclusive() {
    let opt = read_blutdruck_opt();
    let shapes_ttl = systolic_shape_ttl(&opt);

    let shapes = parse_shapes(&shapes_ttl).expect("parse systolic shapes");
    let constraints: Vec<&Constraint> = shapes
        .node_shapes
        .iter()
        .flat_map(|n| n.property_shapes.iter())
        .flat_map(|p| p.constraints.iter())
        .collect();

    let has_min_inclusive = constraints
        .iter()
        .any(|c| matches!(c, Constraint::MinInclusive(_)));
    let has_max_exclusive = constraints
        .iter()
        .any(|c| matches!(c, Constraint::MaxExclusive(_)));
    let has_max_inclusive = constraints
        .iter()
        .any(|c| matches!(c, Constraint::MaxInclusive(_)));

    assert!(
        has_min_inclusive,
        "lower_included=true must lower to sh:minInclusive"
    );
    assert!(
        has_max_exclusive,
        "upper_included=false must lower to sh:maxExclusive"
    );
    assert!(
        !has_max_inclusive,
        "upper_included=false must NOT regenerate sh:maxInclusive (the off-by-one ADL leak)"
    );
}

#[test]
fn value_equal_to_open_upper_bound_is_rejected() {
    // value == hi (1000) must VIOLATE under maxExclusive — the boundary distinguishing the
    // half-open [lo, hi) from a closed [lo, hi]. value == lo (0) and value < hi conform (each
    // data node carries the required mm[Hg] unit so only the magnitude boundary can violate).
    let opt = read_blutdruck_opt();
    let shapes_ttl = systolic_shape_ttl(&opt);

    let data = format!(
        "{}{}{}",
        data_node("atLowerBound", "0"),
        data_node("belowUpper", "999"),
        data_node("atUpperBound", "1000"),
    );
    let report = validate_graphs(&data, &shapes_ttl).expect("validate");

    assert!(
        !report.conforms,
        "value == hi must make the graph non-conformant"
    );
    assert_eq!(
        report.results.len(),
        1,
        "exactly one violation (value == hi); lo and below-hi with units must pass — got {:?}",
        report.results
    );
    let v = &report.results[0];
    assert_eq!(
        v.source_constraint_component.as_str(),
        MAX_EXCLUSIVE_CONSTRAINT_COMPONENT,
        "the violation must be a MaxExclusiveConstraintComponent"
    );
    assert!(
        v.focus_node.to_string().contains("atUpperBound"),
        "the violating focus must be the value-at-hi node, got {:?}",
        v.focus_node
    );
}
