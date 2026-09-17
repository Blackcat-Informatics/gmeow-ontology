// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::opt_lift::{lift_opt_to_validation_shape, recover_opt_from_shape};
use std::path::PathBuf;

fn blutdruck() -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../../validations/openehr-bloodpressure/Blutdruck.opt");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

const BASE: &str = "https://gmeow.example/openehr/bp/";

fn test_all_datatypes() -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../../validations/openehr-test-datatypes/TestAllDatatypes.opt");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

const ALL_TYPES_BASE: &str = "https://gmeow.example/openehr/alltypes/";

/// The naming map the production pipeline uses for the vendored Blutdruck OPT: the
/// systolic/diastolic at-codes get meaningful local names, every other recognized at-code
/// falls back to its raw form.
fn bp_naming() -> std::collections::BTreeMap<String, String> {
    std::collections::BTreeMap::from([
        ("at0004".to_string(), "Systolic".to_string()),
        ("at0005".to_string(), "Diastolic".to_string()),
    ])
}

#[test]
fn walker_extracts_every_constraint_family_present_in_the_real_opt() {
    let naming = std::collections::BTreeMap::new();
    let all = read_all_opt_constraints(&blutdruck(), BASE, &naming).expect("walk Blutdruck.opt");
    // The vendored OPT carries C_DV_QUANTITY magnitudes, C_STRING patterns, a C_CODE_PHRASE
    // code list, C_COMPLEX_OBJECT <occurrences> cardinality, and a <term_bindings> block —
    // the walker must surface every one of those families.
    let has = |pred: fn(&OptConstraintKind) -> bool| all.iter().any(|c| pred(&c.kind));
    assert!(
        has(|k| matches!(k, OptConstraintKind::Quantity { .. })),
        "no quantity extracted from {} constraints",
        all.len()
    );
    assert!(
        has(|k| matches!(k, OptConstraintKind::StringPattern { .. })),
        "no string pattern extracted"
    );
    assert!(
        has(|k| matches!(k, OptConstraintKind::ValueSet { .. })),
        "no coded value set extracted"
    );
    assert!(
        has(|k| matches!(k, OptConstraintKind::Cardinality { .. })),
        "no occurrences cardinality extracted"
    );
    assert!(
        has(|k| matches!(k, OptConstraintKind::TerminologyBinding { .. })),
        "no terminology binding extracted"
    );
}

#[test]
fn every_walked_constraint_round_trips_u_after_d_is_identity() {
    // The section/retraction law holds on EVERY constraint the walker reads from real XML.
    let naming = bp_naming();
    let all = read_all_opt_constraints(&blutdruck(), BASE, &naming).expect("walk");
    assert!(
        all.len() >= 3,
        "expected several constraints, got {}",
        all.len()
    );
    for c in &all {
        let shape = lift_opt_to_validation_shape(c).expect("lift");
        let recovered = recover_opt_from_shape(&shape).expect("recover");
        assert_eq!(&recovered, c, "u∘d must be the identity on {c:?}");
    }
}

#[test]
fn walker_is_deterministic_and_hard_fails_on_a_constraint_free_document() {
    let naming = bp_naming();
    let a = read_all_opt_constraints(&blutdruck(), BASE, &naming).unwrap();
    let b = read_all_opt_constraints(&blutdruck(), BASE, &naming).unwrap();
    assert_eq!(a, b, "the walker must be deterministic in document order");
    let empty_naming = std::collections::BTreeMap::new();
    let err = read_all_opt_constraints("<template></template>", BASE, &empty_naming).unwrap_err();
    assert!(err.to_string().contains("no C_DV_QUANTITY"), "got: {err}");
}

#[test]
fn walker_extracts_ordinal_and_datetime_from_the_real_all_types_opt() {
    let naming = std::collections::BTreeMap::new();
    let all = read_all_opt_constraints(&test_all_datatypes(), ALL_TYPES_BASE, &naming)
        .expect("walk TestAllDatatypes.opt");
    let has = |pred: fn(&OptConstraintKind) -> bool| all.iter().any(|c| pred(&c.kind));
    assert!(
        has(|k| matches!(k, OptConstraintKind::Ordinal { .. })),
        "no ordinal value set extracted from {} constraints",
        all.len()
    );
    assert!(
        has(|k| matches!(k, OptConstraintKind::DateTimePattern { .. })),
        "no datetime pattern extracted"
    );
}

#[test]
fn every_all_types_constraint_round_trips_u_after_d_is_identity() {
    let naming = std::collections::BTreeMap::new();
    let all = read_all_opt_constraints(&test_all_datatypes(), ALL_TYPES_BASE, &naming)
        .expect("walk TestAllDatatypes.opt");
    assert!(
        all.len() >= 2,
        "expected several constraints, got {}",
        all.len()
    );
    for c in &all {
        let shape = lift_opt_to_validation_shape(c).expect("lift");
        let recovered = recover_opt_from_shape(&shape).expect("recover");
        assert_eq!(&recovered, c, "u∘d must be the identity on {c:?}");
    }
}

#[test]
fn walker_mints_meaningful_stable_names_from_the_naming_map_not_positional_ones() {
    // Proves the walker mints the SAME shape/target identity the curated production reader
    // used to hand-wire, purely from the enclosing at-code + naming map — never a
    // `Constraint-{idx}` positional name.
    let naming = bp_naming();
    let all = read_all_opt_constraints(&blutdruck(), BASE, &naming).expect("walk");
    let systolic = all
        .iter()
        .find(|c| {
            matches!(&c.kind, OptConstraintKind::Quantity { units, .. } if units == "mm[Hg]")
                && c.target_class == format!("{BASE}Systolic")
        })
        .expect("a Systolic quantity shape");
    assert_eq!(systolic.target_class, format!("{BASE}Systolic"));
    assert_eq!(systolic.shape_iri, format!("{BASE}SystolicShape"));
    assert!(
        !all.iter()
            .any(|c| c.shape_iri.contains("shape-") || c.target_class.contains("Constraint-")),
        "no minted IRI may be positional: {all:?}"
    );
}
