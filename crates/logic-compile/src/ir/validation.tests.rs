// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn failure_class_is_unique_annotation_metadata() {
    let bare = ValidationShapeIr::new(
        "https://ex/Shape",
        ShapeTarget::Class("https://ex/C".into()),
        vec![],
        None,
    )
    .unwrap();
    let keyed = bare
        .clone()
        .with_failure_class("https://ex/Failure")
        .unwrap();
    assert_eq!(bare.content_key(), keyed.content_key());
    let err = keyed
        .with_failure_class("https://ex/OtherFailure")
        .unwrap_err();
    assert!(err.message().contains("duplicate"));
}

#[test]
fn value_set_members_are_order_independent() {
    let members = |a: &str, b: &str| {
        PropertyConstraintIr::new(
            "https://ex/p",
            None,
            None,
            None,
            vec![ConstraintComponent::In(vec![
                ShapeValue::Iri(a.to_owned()),
                ShapeValue::Iri(b.to_owned()),
            ])],
        )
        .unwrap()
    };
    let ab = members("https://ex/a", "https://ex/b");
    let ba = members("https://ex/b", "https://ex/a");
    assert_eq!(
        ab.content_key(),
        ba.content_key(),
        "value-set member order must not affect identity"
    );
    assert_eq!(ab, ba, "normalized components must be structurally equal");
}

#[test]
fn value_set_literal_with_datatype_and_lang_is_rejected() {
    let err = PropertyConstraintIr::new(
        "https://ex/p",
        None,
        None,
        None,
        vec![ConstraintComponent::In(vec![ShapeValue::Literal(
            purrdf::RdfLiteral {
                lexical_form: "x".into(),
                datatype: Some("https://ex/dt".into()),
                language: Some("en".into()),
                direction: None,
            },
        )])],
    )
    .unwrap_err();
    assert!(
        err.message().contains("literal datatype does not match"),
        "got: {err}"
    );
}

#[test]
fn value_keyed_target_key_is_unambiguous() {
    // The classic delimiter collision: `"a=b" + "c"` and `"a" + "b=c"` must fold to DISTINCT
    // keys, never the same `a=b=c`.
    let x = ShapeTarget::ValueKeyed {
        predicate: "a=b".into(),
        value: "c".into(),
    };
    let y = ShapeTarget::ValueKeyed {
        predicate: "a".into(),
        value: "b=c".into(),
    };
    assert_ne!(
        x.content_key(),
        y.content_key(),
        "distinct value-keyed targets must not share a content key"
    );
}

#[test]
fn default_presentation_fields_leave_property_key_byte_identical() {
    // The type-migration / new-field hazard the empty-attach test does NOT catch: a plain
    // property shape (no inverse path, no severity, no message) must fold to the SAME bytes
    // the key produced before those fields existed — i.e. the tail markers must be ABSENT.
    let p = PropertyConstraintIr::new(
        "https://ex/p",
        Some(1),
        Some(1),
        Some(ConstraintProvenance::OwlRestriction),
        vec![ConstraintComponent::Class("https://ex/C".into())],
    )
    .unwrap();
    let key = p.content_key();
    assert!(
        !key.contains("inverse="),
        "default key must not carry inverse: {key}"
    );
    assert!(
        !key.contains("sev="),
        "default key must not carry severity: {key}"
    );
    assert!(
        !key.contains("msg="),
        "default key must not carry message: {key}"
    );
    // And the presentation setters DO perturb the key (falsifiable).
    assert_ne!(p.content_key(), p.clone().inverted().content_key());
    assert_ne!(
        p.content_key(),
        p.clone()
            .with_severity(ShaclSeverity::Warning)
            .content_key()
    );
}

#[test]
fn default_node_components_and_label_leave_shape_key_byte_identical() {
    // A shape with no node_components and no label must fold to a key with NO NODECOMPS/label
    // tail — the guarantee that the historical shape corpus's content-addressed key cannot
    // drift now that these fields exist.
    let shape = ValidationShapeIr::new(
        "https://ex/S-shape",
        ShapeTarget::Class("https://ex/S".into()),
        vec![
            PropertyConstraintIr::new(
                "https://ex/p",
                None,
                None,
                None,
                vec![ConstraintComponent::Class("https://ex/C".into())],
            )
            .unwrap(),
        ],
        None,
    )
    .unwrap();
    let key = shape.content_key();
    assert!(
        key.ends_with(&format!("PROPS={}", {
            key_list(
                shape
                    .properties
                    .iter()
                    .map(PropertyConstraintIr::content_key),
            )
        })),
        "a plain shape's key must end at PROPS with no NODECOMPS/label tail: {key}"
    );
    assert!(!key.contains("NODECOMPS="));
    assert!(!key.contains("label="));
    // Attaching a node component / label DOES perturb the key.
    let with_nc = shape
        .clone()
        .with_node_components(vec![ConstraintComponent::Class("https://ex/D".into())])
        .unwrap();
    assert_ne!(shape.content_key(), with_nc.content_key());
    assert!(with_nc.content_key().contains("NODECOMPS="));
}

#[test]
fn new_components_round_trip_content_key_and_order_independence() {
    // HasValue, QualifiedValueShape, and Not all fold deterministically, and a qualified
    // value shape's inner components are order-independent.
    let mk = |inner_a: &str, inner_b: &str| {
        PropertyConstraintIr::new(
            "https://ex/p",
            None,
            None,
            None,
            vec![
                ConstraintComponent::HasValue(ShapeValue::Iri("https://ex/v".into())),
                ConstraintComponent::QualifiedValueShape {
                    shape: vec![
                        ConstraintComponent::Class(inner_a.into()),
                        ConstraintComponent::NodeKindShacl(ShaclNodeKind::Iri),
                        ConstraintComponent::Datatype(inner_b.into()),
                    ],
                    min: Some(1),
                    max: None,
                },
                ConstraintComponent::Not(Box::new(ConstraintComponent::Class(
                    "https://ex/Disjoint".into(),
                ))),
            ],
        )
        .unwrap()
    };
    // Inner shape supplied in two different orders → identical key (order-independence).
    let x = mk("https://ex/A", "https://ex/dt");
    let mut reordered = PropertyConstraintIr::new(
        "https://ex/p",
        None,
        None,
        None,
        vec![
            ConstraintComponent::Not(Box::new(ConstraintComponent::Class(
                "https://ex/Disjoint".into(),
            ))),
            ConstraintComponent::QualifiedValueShape {
                shape: vec![
                    ConstraintComponent::Datatype("https://ex/dt".into()),
                    ConstraintComponent::NodeKindShacl(ShaclNodeKind::Iri),
                    ConstraintComponent::Class("https://ex/A".into()),
                ],
                min: Some(1),
                max: None,
            },
            ConstraintComponent::HasValue(ShapeValue::Iri("https://ex/v".into())),
        ],
    )
    .unwrap();
    // sanity: normalization made reordered structurally equal to x
    reordered.message = None;
    assert_eq!(x.content_key(), reordered.content_key());
    assert_eq!(x, reordered);
}

#[test]
fn has_value_literal_with_datatype_and_lang_is_rejected() {
    let err = PropertyConstraintIr::new(
        "https://ex/p",
        None,
        None,
        None,
        vec![ConstraintComponent::HasValue(ShapeValue::Literal(
            purrdf::RdfLiteral {
                lexical_form: "x".into(),
                datatype: Some("https://ex/dt".into()),
                language: Some("en".into()),
                direction: None,
            },
        ))],
    )
    .unwrap_err();
    assert!(
        err.message().contains("literal datatype does not match"),
        "got: {err}"
    );
}

#[test]
fn new_targets_and_qualified_shape_are_lossy_transparent() {
    // A Not wrapping a lossy inner is lossy; a QualifiedValueShape over lossy inner is lossy.
    let not_pattern = ConstraintComponent::Not(Box::new(ConstraintComponent::Pattern {
        regex: "a".into(),
        flags: None,
    }));
    assert!(not_pattern.is_lossy());
    let qvs_clean = ConstraintComponent::QualifiedValueShape {
        shape: vec![ConstraintComponent::Class("https://ex/C".into())],
        min: Some(1),
        max: None,
    };
    assert!(!qvs_clean.is_lossy());
    // Subjects-of / objects-of targets validate their predicate.
    assert!(
        ValidationShapeIr::new(
            "https://ex/s",
            ShapeTarget::SubjectsOf("  ".into()),
            vec![],
            None,
        )
        .is_err()
    );
}

#[test]
fn or_and_xone_branches_are_order_independent_and_lossy_transparent() {
    // Branch supply order must not affect identity, and a lossy branch makes the whole
    // disjunction lossy (recursion through Or/Xone).
    let mk = |a: &str, b: &str| {
        PropertyConstraintIr::new(
            "https://ex/p",
            None,
            None,
            None,
            vec![ConstraintComponent::Or(vec![
                ConstraintComponent::Class(a.into()),
                ConstraintComponent::Class(b.into()),
            ])],
        )
        .unwrap()
    };
    assert_eq!(
        mk("https://ex/A", "https://ex/B").content_key(),
        mk("https://ex/B", "https://ex/A").content_key(),
        "Or branch order must not affect identity"
    );
    let clean = ConstraintComponent::Xone(vec![ConstraintComponent::Class("https://ex/A".into())]);
    assert!(!clean.is_lossy());
    let lossy = ConstraintComponent::Or(vec![ConstraintComponent::Pattern {
        regex: "^a".into(),
        flags: None,
    }]);
    assert!(
        lossy.is_lossy(),
        "a Pattern branch makes the disjunction lossy"
    );
}

#[test]
fn language_and_terminology_codes_are_sorted_at_construction() {
    let p = PropertyConstraintIr::new(
        "https://ex/p",
        None,
        None,
        None,
        vec![ConstraintComponent::LanguageIn(vec![
            "fr".into(),
            "en".into(),
            "de".into(),
        ])],
    )
    .unwrap();
    match &p.components[0] {
        ConstraintComponent::LanguageIn(langs) => assert_eq!(langs, &["de", "en", "fr"]),
        other => panic!("expected LanguageIn, got {other:?}"),
    }
}
