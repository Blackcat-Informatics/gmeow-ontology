// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! F6 — the sharp ADL→FOL fidelity test: the half-open magnitude interval.
//!
//! The Blutdruck OPT's systolic `C_DV_QUANTITY` (property `openehr::pressure`, units
//! `mm[Hg]`) constrains the magnitude with `lower_included=true`, **`upper_included=false`**
//! — a half-open `[lo, hi)`. The concrete bounds, read verbatim from the vendored
//! `validations/openehr-bloodpressure/Blutdruck.opt` (the systolic `<magnitude>` block),
//! are `lower=0`, `upper=1000`: the interval is `[0, 1000)` mm[Hg].
//!
//! Lowering that ADL constraint to `logic:`/SHACL must regenerate the boundary inclusivity
//! EXACTLY: `lower_included=true → sh:minInclusive`, `upper_included=false → sh:maxExclusive`
//! (NEVER `sh:maxInclusive`). This is the sharp test that `u ∘ d = id` holds on the
//! *constraint* and not merely the data — an off-by-one on the open boundary would silently
//! admit `value == hi`. The two halves below check both the structural lowering (the parsed
//! shape carries MaxExclusive, never MaxInclusive) and the enforced semantics (`value == hi`
//! violates; `value == lo` and `value < hi` conform).

use gmeow_shacl::engine::{parse_shapes, validate_graphs};
use gmeow_shacl::model::sh::MAX_EXCLUSIVE_CONSTRAINT_COMPONENT;
use gmeow_shacl::shapes::Constraint;

/// The systolic constraint lowered from the OPT half-open `[0, 1000)` mm[Hg]:
/// `sh:minInclusive 0` (lower_included) + `sh:maxExclusive 1000` (upper EXCLUDED).
const SYSTOLIC_SHAPES_TTL: &str = r#"
@prefix sh:    <http://www.w3.org/ns/shacl#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://gmeow.example/openehr/bp/> .

ex:SystolicMeasurementShape a sh:NodeShape ;
    sh:targetClass gmeow:SystolicMeasurement ;
    sh:property [
        sh:path gmeow:quantityValue ;
        sh:minInclusive 0 ;
        sh:maxExclusive 1000 ;
    ] .
"#;

fn data_node(local: &str, value: &str) -> String {
    format!(
        "<https://gmeow.example/openehr/bp/{local}> \
         <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
         <https://blackcatinformatics.ca/gmeow/SystolicMeasurement> .\n\
         <https://gmeow.example/openehr/bp/{local}> \
         <https://blackcatinformatics.ca/gmeow/quantityValue> \
         \"{value}\"^^<http://www.w3.org/2001/XMLSchema#decimal> .\n"
    )
}

#[test]
fn half_open_lowers_to_max_exclusive_never_max_inclusive() {
    let shapes = parse_shapes(SYSTOLIC_SHAPES_TTL).expect("parse systolic shapes");
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
    // value == hi (1000) must VIOLATE under maxExclusive — the boundary that distinguishes
    // the half-open [lo, hi) from a closed [lo, hi]. value == lo (0) and value < hi conform.
    let data = format!(
        "{}{}{}",
        data_node("atLowerBound", "0"), // == lo: minInclusive admits it
        data_node("belowUpper", "999"), // < hi: inside the interval
        data_node("atUpperBound", "1000"), // == hi: maxExclusive rejects it
    );
    let report = validate_graphs(&data, SYSTOLIC_SHAPES_TTL).expect("validate");

    assert!(
        !report.conforms,
        "value == hi must make the graph non-conformant"
    );

    assert_eq!(
        report.results.len(),
        1,
        "exactly one violation (value == hi); lo and below-hi must pass"
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
