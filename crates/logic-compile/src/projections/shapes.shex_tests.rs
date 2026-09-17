// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::ir::ShaclNodeKind;

fn prop(path: &str, comps: Vec<ConstraintComponent>) -> PropertyConstraintIr {
    PropertyConstraintIr::new(path, None, None, None, comps).unwrap()
}

fn shape(iri: &str, class: &str, props: Vec<PropertyConstraintIr>) -> ValidationShapeIr {
    ValidationShapeIr::new(iri, ShapeTarget::Class(class.to_owned()), props, None).unwrap()
}

#[test]
fn quantity_interval_projects_shex_numeric_facets() {
    let s = shape(
        "https://ex/S",
        "https://ex/C",
        vec![prop(
            "https://ex/magnitude",
            vec![
                ConstraintComponent::NumericRange {
                    min: Some(0.0),
                    max: Some(1000.0),
                    min_inclusive: true,
                    max_inclusive: false,
                },
                ConstraintComponent::Datatype("http://www.w3.org/2001/XMLSchema#decimal".into()),
            ],
        )],
    );
    let shex = project_validation_shape_shex(&s);
    assert!(shex.contains("MININCLUSIVE 0"), "{shex}");
    assert!(shex.contains("MAXEXCLUSIVE 1000"), "{shex}");
    assert!(
        shex.contains("<http://www.w3.org/2001/XMLSchema#decimal>"),
        "{shex}"
    );
}

#[test]
fn value_set_projects_shex_value_set() {
    let s = shape(
        "https://ex/S",
        "https://ex/C",
        vec![prop(
            "https://ex/code",
            vec![ConstraintComponent::In(vec![
                ShapeValue::Iri("https://ex/at0004".into()),
                ShapeValue::Iri("https://ex/at0005".into()),
            ])],
        )],
    );
    let shex = project_validation_shape_shex(&s);
    assert!(
        shex.contains("[<https://ex/at0004> <https://ex/at0005>]"),
        "{shex}"
    );
}

#[test]
fn cardinality_maps_to_shex_suffix() {
    assert_eq!(shex_cardinality(Some(0), Some(1)), " ?");
    assert_eq!(shex_cardinality(Some(1), None), " +");
    assert_eq!(shex_cardinality(Some(0), None), " *");
    assert_eq!(shex_cardinality(Some(1), Some(1)), "");
    assert_eq!(shex_cardinality(Some(2), Some(4)), " {2,4}");
}

#[test]
fn node_kind_and_pattern_project_to_shex() {
    let s = shape(
        "https://ex/S",
        "https://ex/C",
        vec![prop(
            "https://ex/name",
            vec![
                ConstraintComponent::NodeKindShacl(ShaclNodeKind::Literal),
                ConstraintComponent::Pattern {
                    regex: "^[A-Z]+$".into(),
                    flags: None,
                },
            ],
        )],
    );
    let shex = project_validation_shape_shex(&s);
    assert!(
        shex.contains("LITERAL") || shex.contains("/^[A-Z]+$/"),
        "{shex}"
    );
    // The pattern residue is inherited from SHACL (regex dialect).
    assert!(
        shex_residue(&s).iter().any(|r| r.contains("regex-dialect")),
        "{:?}",
        shex_residue(&s)
    );
}

#[test]
fn shex_regex_escapes_the_slash_delimiter() {
    // A `/` inside the pattern must be escaped as `\/`, else it prematurely closes the
    // ShExC `/…/` regex literal and corrupts the shape.
    let s = shape(
        "https://ex/S",
        "https://ex/C",
        vec![prop(
            "https://ex/path",
            vec![ConstraintComponent::Pattern {
                regex: "^a/b$".into(),
                flags: None,
            }],
        )],
    );
    let shex = project_validation_shape_shex(&s);
    assert!(
        shex.contains("/^a\\/b$/"),
        "the `/` delimiter must be escaped: {shex}"
    );
}

#[test]
fn shex_residue_is_strictly_larger_than_shacl_for_datetime() {
    let s = shape(
        "https://ex/S",
        "https://ex/C",
        vec![prop(
            "https://ex/when",
            vec![ConstraintComponent::DateTimeRange {
                min: Some("2020-01-01T00:00:00Z".into()),
                max: None,
                min_inclusive: true,
                max_inclusive: false,
            }],
        )],
    );
    // SHACL Core expresses the datetime range (no residue); ShEx cannot (residue).
    assert!(
        shacl_residue(&s).is_empty(),
        "shacl: {:?}",
        shacl_residue(&s)
    );
    assert!(
        shex_residue(&s)
            .iter()
            .any(|r| r.contains("datetime range")),
        "shex: {:?}",
        shex_residue(&s)
    );
}

#[test]
fn reifier_and_value_keyed_target_are_shex_residue() {
    let property = PropertyConstraintIr::new("https://ex/p", None, None, None, vec![])
        .unwrap()
        .with_reifier(Some("https://ex/R".into()), true)
        .unwrap();
    let s = ValidationShapeIr::new(
        "https://ex/S",
        ShapeTarget::ValueKeyed {
            predicate: "https://ex/kind".into(),
            value: "https://ex/Bp".into(),
        },
        vec![property],
        None,
    )
    .unwrap();
    let r = shex_residue(&s);
    assert!(r.iter().any(|x| x.contains("value-keyed target")), "{r:?}");
    assert!(r.iter().any(|x| x.contains("reifier")), "{r:?}");
}

#[test]
fn has_value_projects_shex_singleton_value_set() {
    let s = shape(
        "https://ex/S",
        "https://ex/C",
        vec![prop(
            "https://ex/p",
            vec![ConstraintComponent::HasValue(ShapeValue::Iri(
                "https://ex/v".into(),
            ))],
        )],
    );
    let shex = project_validation_shape_shex(&s);
    assert!(shex.contains("[<https://ex/v>]"), "{shex}");
}

#[test]
fn qualified_count_and_negation_are_shex_residue() {
    let s = shape(
        "https://ex/S",
        "https://ex/C",
        vec![prop(
            "https://ex/p",
            vec![
                ConstraintComponent::QualifiedValueShape {
                    shape: vec![ConstraintComponent::Class("https://ex/Q".into())],
                    min: Some(1),
                    max: None,
                },
                ConstraintComponent::Not(Box::new(ConstraintComponent::Class(
                    "https://ex/D".into(),
                ))),
            ],
        )],
    );
    let r = shex_residue(&s);
    assert!(
        r.iter().any(|x| x.contains("qualified value-shape count")),
        "{r:?}"
    );
    assert!(r.iter().any(|x| x.contains("negated constraint")), "{r:?}");
}

#[test]
fn residue_classification_is_pinned_and_nested_lossy_is_never_silently_dropped() {
    // Guards the exhaustive residue classifiers (shacl_component_residue / shex_residue): a
    // lossy component is flagged, a faithful one is not, and — critically — a lossy component
    // NESTED inside a `sh:not` is still flagged. Before the classifiers were made exhaustive
    // and recursive, the trailing `_ => {}` catch-all dropped a `Not(Pattern)` in silence,
    // defeating the loss ledger's "carried and flagged, never dropped" contract.
    let faithful = shape(
        "https://ex/faithful",
        "https://ex/C",
        vec![prop(
            "https://ex/p",
            vec![ConstraintComponent::Datatype(
                "http://www.w3.org/2001/XMLSchema#string".into(),
            )],
        )],
    );
    assert!(
        shacl_residue(&faithful).is_empty(),
        "a faithful sh:datatype must carry no SHACL residue: {:?}",
        shacl_residue(&faithful)
    );

    let lossy = shape(
        "https://ex/lossy",
        "https://ex/C",
        vec![prop(
            "https://ex/p",
            vec![ConstraintComponent::Pattern {
                regex: "^A".into(),
                flags: None,
            }],
        )],
    );
    assert!(
        shacl_residue(&lossy)
            .iter()
            .any(|x| x.contains("sh:pattern")),
        "a top-level sh:pattern must be flagged: {:?}",
        shacl_residue(&lossy)
    );

    // The regression the exhaustiveness+recursion fix closes: a lossy component inside `sh:not`.
    let nested = shape(
        "https://ex/nested",
        "https://ex/C",
        vec![prop(
            "https://ex/p",
            vec![ConstraintComponent::Not(Box::new(
                ConstraintComponent::Pattern {
                    regex: "^A".into(),
                    flags: None,
                },
            ))],
        )],
    );
    assert!(
        shacl_residue(&nested)
            .iter()
            .any(|x| x.contains("sh:pattern")),
        "a Pattern nested inside sh:not must NOT be silently dropped: {:?}",
        shacl_residue(&nested)
    );
}

#[test]
fn domain_target_node_component_is_shex_residue() {
    let s = ValidationShapeIr::new(
        "https://ex/p-domain-shape",
        ShapeTarget::SubjectsOf("https://ex/p".into()),
        vec![],
        None,
    )
    .unwrap()
    .with_node_components(vec![ConstraintComponent::Class("https://ex/C".into())])
    .unwrap();
    let shex = project_validation_shape_shex(&s);
    assert!(shex.contains("# targetSubjectsOf <https://ex/p>"), "{shex}");
    assert!(
        shex_residue(&s)
            .iter()
            .any(|x| x.contains("focus-node-level")),
        "{:?}",
        shex_residue(&s)
    );
}

#[test]
fn empty_program_yields_empty_shex_document() {
    let prog = LogicProgram::new(vec![], vec![], vec![], None);
    assert_eq!(project_validation_shapes_shex(&prog), "");
}

#[test]
fn facet_only_shex_omits_the_any_node_shapeatom() {
    // A property whose only ShEx-expressible constraint is a facet (here a
    // pattern) has no datatype/node-kind base. The facets alone ARE the ShExC
    // node constraint (`xsFacet+`); a leading `.` (the any-node shapeAtom)
    // would be a second, juxtaposed shapeAtom and the document would not parse.
    let s = shape(
        "https://ex/S",
        "https://ex/C",
        vec![prop(
            "https://ex/path",
            vec![ConstraintComponent::Pattern {
                regex: ".*".into(),
                flags: None,
            }],
        )],
    );
    let shex = project_validation_shape_shex(&s);
    assert!(
        !shex.contains(". /"),
        "a facet-only constraint must not be prefixed with the `.` any shapeAtom: {shex}"
    );
    assert!(shex.contains("/.*/"), "{shex}");
}
