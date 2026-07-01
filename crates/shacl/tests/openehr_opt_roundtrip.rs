// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! End-to-end constraints-axis round-trip against the **real** vendored GECCO
//! blood-pressure Operational Template: OPT XML → `OptConstraintIr` → `logic:`
//! `ValidationShapeIr` → SHACL, with the `u∘d=id` section/retraction law and an
//! isomorphism check against the direct-emit oracle it subsumes.

use std::path::PathBuf;

use gmeow_logic_compile::opt_lift::{
    lift_opt_to_validation_shape, recover_opt_from_shape, OptConstraintKind,
};
use gmeow_logic_compile::projections::shapes::project_validation_shape_shacl;
use gmeow_shacl::openehr_opt::{
    lower_magnitude_to_shacl_ttl, read_magnitude_interval, read_opt_quantity_constraint,
};

/// Reads the vendored Blutdruck OPT from disk (`crates/shacl` → repo `validations/`).
fn blutdruck_opt() -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../../validations/openehr-bloodpressure/Blutdruck.opt");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

const SYSTOLIC_NODE: &str = "at0004";
const SHAPE: &str = "https://gmeow.example/openehr/bp/SystolicShape";
const CLASS: &str = "https://gmeow.example/openehr/bp/Systolic";
const MAG: &str = "https://gmeow.example/openehr/bp/magnitude";
const UNITS: &str = "https://gmeow.example/openehr/bp/units";

#[test]
fn real_opt_lifts_to_a_half_open_shacl_shape() {
    let opt = blutdruck_opt();
    let constraint =
        read_opt_quantity_constraint(&opt, SYSTOLIC_NODE, SHAPE, CLASS, MAG, UNITS).unwrap();
    let shape = lift_opt_to_validation_shape(&constraint).unwrap();
    let ttl = project_validation_shape_shacl(&shape);
    // The GECCO systolic magnitude is the half-open interval [0, 1000) mm[Hg].
    assert!(ttl.contains("sh:minInclusive 0"), "{ttl}");
    assert!(ttl.contains("sh:maxExclusive 1000"), "{ttl}");
    assert!(
        !ttl.contains("sh:maxInclusive"),
        "half-open upper must be exclusive: {ttl}"
    );
    assert!(ttl.contains("mm[Hg]"), "units must survive the lift: {ttl}");
}

#[test]
fn real_opt_round_trips_u_after_d_is_identity() {
    let opt = blutdruck_opt();
    let original =
        read_opt_quantity_constraint(&opt, SYSTOLIC_NODE, SHAPE, CLASS, MAG, UNITS).unwrap();
    // d: OPT constraint → logic: validation shape; u: back to the OPT constraint.
    let shape = lift_opt_to_validation_shape(&original).unwrap();
    let recovered = recover_opt_from_shape(&shape).unwrap();
    assert_eq!(
        recovered, original,
        "u∘d must be the identity on the real OPT constraint"
    );
    // Cross-check the recovered interval against the raw magnitude reader.
    let raw = read_magnitude_interval(&opt, SYSTOLIC_NODE).unwrap();
    match recovered.kind {
        OptConstraintKind::Quantity {
            interval, units, ..
        } => {
            assert_eq!(interval.lower, Some(raw.lower));
            assert_eq!(interval.upper, Some(raw.upper));
            assert_eq!(interval.lower_included, raw.lower_included);
            assert_eq!(interval.upper_included, raw.upper_included);
            assert_eq!(units, raw.units);
        }
        other => panic!("expected a Quantity constraint from a quantity node, got {other:?}"),
    }
}

#[test]
fn derived_shacl_matches_the_direct_emit_oracle_on_the_interval() {
    // The derived path (OPT → logic: → SHACL) must agree with the narrow direct-emit
    // reader it subsumes on the interval facets — equivalence before the direct path is
    // retired. Both are read from the SAME real OPT node.
    let opt = blutdruck_opt();
    let raw = read_magnitude_interval(&opt, SYSTOLIC_NODE).unwrap();
    let oracle = lower_magnitude_to_shacl_ttl(&raw, CLASS, MAG, SHAPE);

    let constraint =
        read_opt_quantity_constraint(&opt, SYSTOLIC_NODE, SHAPE, CLASS, MAG, UNITS).unwrap();
    let derived =
        project_validation_shape_shacl(&lift_opt_to_validation_shape(&constraint).unwrap());

    for facet in ["sh:minInclusive 0", "sh:maxExclusive 1000"] {
        assert!(oracle.contains(facet), "oracle missing {facet}: {oracle}");
        assert!(
            derived.contains(facet),
            "derived missing {facet}: {derived}"
        );
    }
}
