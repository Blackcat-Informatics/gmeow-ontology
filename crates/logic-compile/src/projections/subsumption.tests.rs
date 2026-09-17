// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::ir::{
    ConstraintComponent, ConstraintProvenance, PropertyConstraintIr, ShaclSeverity, ShapeTarget,
    ValidationShapeIr,
};

/// A one-property class-targeted shape over the given path/cardinality/components.
fn shape(
    iri: &str,
    target_class: &str,
    path: &str,
    min: Option<u32>,
    max: Option<u32>,
    prov: Option<ConstraintProvenance>,
    components: Vec<ConstraintComponent>,
) -> ValidationShapeIr {
    ValidationShapeIr::new(
        iri,
        ShapeTarget::Class(target_class.to_owned()),
        vec![PropertyConstraintIr::new(path, min, max, prov, components).unwrap()],
        None,
    )
    .unwrap()
}

#[test]
fn equivalent_is_reflexive_and_symmetric() {
    let s = shape(
        "https://ex/S",
        "https://ex/C",
        "https://ex/p",
        Some(1),
        Some(1),
        Some(ConstraintProvenance::OwlRestriction),
        vec![ConstraintComponent::Class("https://ex/D".into())],
    );
    assert!(equivalent(&s, &s), "equivalence must be reflexive");
    let t = shape(
        "https://ex/T",
        "https://ex/C",
        "https://ex/p",
        Some(1),
        Some(1),
        Some(ConstraintProvenance::OwlRestriction),
        vec![ConstraintComponent::Class("https://ex/D".into())],
    );
    assert_eq!(equivalent(&s, &t), equivalent(&t, &s), "must be symmetric");
    assert!(equivalent(&s, &t));
}

#[test]
fn presentation_and_provenance_are_projected_out() {
    // Two shapes differing ONLY in iri / severity / message / cardinality_provenance are
    // enforcement-equivalent (presentation and ledger polarity never change findings).
    let base_prop = |prov| {
        PropertyConstraintIr::new(
            "https://ex/p",
            Some(1),
            Some(1),
            Some(prov),
            vec![ConstraintComponent::Class("https://ex/D".into())],
        )
        .unwrap()
    };
    let a = ValidationShapeIr::new(
        "https://ex/A",
        ShapeTarget::Class("https://ex/C".into()),
        vec![base_prop(ConstraintProvenance::OwlRestriction)],
        None,
    )
    .unwrap();
    let b = ValidationShapeIr::new(
        "https://ex/B-different-iri",
        ShapeTarget::Class("https://ex/C".into()),
        vec![
            base_prop(ConstraintProvenance::OptNative)
                .with_severity(ShaclSeverity::Warning)
                .with_message("distinct message")
                .unwrap(),
        ],
        None,
    )
    .unwrap();
    assert_ne!(
        a.content_key(),
        b.content_key(),
        "the IDENTITY keys must differ (presentation/provenance/iri all differ)"
    );
    assert!(
        equivalent(&a, &b),
        "enforcement must ignore iri/severity/message/provenance"
    );
    // Equivalence must imply mutual subsumption.
    assert!(subsumes(&a, &b) && subsumes(&b, &a));
}

#[test]
fn equivalent_false_when_component_or_cardinality_differs() {
    let a = shape(
        "https://ex/A",
        "https://ex/C",
        "https://ex/p",
        Some(1),
        Some(1),
        Some(ConstraintProvenance::OwlRestriction),
        vec![ConstraintComponent::Class("https://ex/D".into())],
    );
    // Different component value.
    let b = shape(
        "https://ex/A",
        "https://ex/C",
        "https://ex/p",
        Some(1),
        Some(1),
        Some(ConstraintProvenance::OwlRestriction),
        vec![ConstraintComponent::Class("https://ex/Other".into())],
    );
    assert!(!equivalent(&a, &b), "differing component ⇒ not equivalent");
    // Different cardinality.
    let c = shape(
        "https://ex/A",
        "https://ex/C",
        "https://ex/p",
        Some(0),
        None,
        Some(ConstraintProvenance::OwlRestriction),
        vec![ConstraintComponent::Class("https://ex/D".into())],
    );
    assert!(
        !equivalent(&a, &c),
        "differing cardinality ⇒ not equivalent"
    );
}

#[test]
fn extra_component_strictly_subsumes() {
    // `strong` carries an EXTRA component the `weak` shape lacks — same path/cardinality.
    let weak = shape(
        "https://ex/W",
        "https://ex/C",
        "https://ex/p",
        Some(1),
        Some(1),
        Some(ConstraintProvenance::OwlRestriction),
        vec![ConstraintComponent::Class("https://ex/D".into())],
    );
    let strong = shape(
        "https://ex/S",
        "https://ex/C",
        "https://ex/p",
        Some(1),
        Some(1),
        Some(ConstraintProvenance::OwlRestriction),
        vec![
            ConstraintComponent::Class("https://ex/D".into()),
            ConstraintComponent::MinLength(3),
        ],
    );
    assert!(subsumes(&strong, &weak), "extra component ⇒ strong ⊑ weak");
    assert!(
        !subsumes(&weak, &strong),
        "the weaker shape does NOT subsume the stronger (strict)"
    );
}

#[test]
fn cardinality_interval_containment() {
    // min 1..=1 ⊆ 0..=unbounded on the same path+components.
    let weak = shape(
        "https://ex/W",
        "https://ex/C",
        "https://ex/p",
        Some(0),
        None,
        Some(ConstraintProvenance::OwlRestriction),
        vec![ConstraintComponent::Class("https://ex/D".into())],
    );
    let strong = shape(
        "https://ex/S",
        "https://ex/C",
        "https://ex/p",
        Some(1),
        Some(1),
        Some(ConstraintProvenance::OwlRestriction),
        vec![ConstraintComponent::Class("https://ex/D".into())],
    );
    assert!(subsumes(&strong, &weak), "[1,1] ⊆ [0,∞] ⇒ strong ⊑ weak");
    assert!(!subsumes(&weak, &strong), "[0,∞] ⊄ [1,1] ⇒ not the reverse");
}

#[test]
fn equivalence_implies_mutual_subsumption() {
    let a = shape(
        "https://ex/A",
        "https://ex/C",
        "https://ex/p",
        Some(2),
        Some(5),
        Some(ConstraintProvenance::OptNative),
        vec![
            ConstraintComponent::Class("https://ex/D".into()),
            ConstraintComponent::MinLength(1),
        ],
    );
    // Same enforcement, components supplied in a different order + different iri.
    let b = shape(
        "https://ex/B",
        "https://ex/C",
        "https://ex/p",
        Some(2),
        Some(5),
        Some(ConstraintProvenance::OptNative),
        vec![
            ConstraintComponent::MinLength(1),
            ConstraintComponent::Class("https://ex/D".into()),
        ],
    );
    assert!(equivalent(&a, &b));
    assert!(
        subsumes(&a, &b) && subsumes(&b, &a),
        "equivalent ⇒ mutual subsumption"
    );
}

#[test]
fn reifier_strengthening_and_incomparable_shapes() {
    let prop = |reifier: Option<String>, required: bool| {
        let p = PropertyConstraintIr::new(
            "https://ex/p",
            Some(1),
            Some(1),
            Some(ConstraintProvenance::OwlRestriction),
            vec![ConstraintComponent::Class("https://ex/D".into())],
        )
        .unwrap();
        if reifier.is_some() || required {
            p.with_reifier(reifier, required).unwrap()
        } else {
            p
        }
    };
    let mk = |iri: &str, reifier: Option<String>, required: bool| {
        ValidationShapeIr::new(
            iri,
            ShapeTarget::Class("https://ex/C".into()),
            vec![prop(reifier, required)],
            None,
        )
        .unwrap()
    };
    // reification_required=true subsumes false.
    let req = mk("https://ex/Req", None, true);
    let plain = mk("https://ex/Plain", None, false);
    assert!(subsumes(&req, &plain), "reifreq true ⊒ false");
    assert!(!subsumes(&plain, &req), "false ⋢ true");
    // Two DIFFERENT reifier_shape IRIs are not comparable in either direction.
    let ra = mk("https://ex/Ra", Some("https://ex/ShapeA".into()), false);
    let rb = mk("https://ex/Rb", Some("https://ex/ShapeB".into()), false);
    assert!(
        !subsumes(&ra, &rb) && !subsumes(&rb, &ra),
        "distinct reifier shapes are incomparable"
    );
    // A present reifier shape subsumes an absent one.
    assert!(subsumes(&ra, &plain), "reifier_shape present ⊒ absent");
    assert!(!subsumes(&plain, &ra), "absent reifier_shape ⋢ present");
}

#[test]
fn residue_normal_form_equals_shacl_residue_for_a_lossy_shape() {
    let s = shape(
        "https://ex/Lossy",
        "https://ex/C",
        "https://ex/p",
        None,
        None,
        None,
        vec![ConstraintComponent::Pattern {
            regex: "^[A-Z]+$".into(),
            flags: None,
        }],
    );
    let normal = residue_normal_form(&s);
    assert_eq!(normal, super::super::shapes::shacl_residue(&s));
    assert!(
        !normal.is_empty(),
        "a Pattern component must produce regex-dialect residue"
    );
}
