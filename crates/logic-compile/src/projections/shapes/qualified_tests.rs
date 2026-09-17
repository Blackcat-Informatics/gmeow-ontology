// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMEOW's canonical restrictions must project to independent, well-formed counts.

use super::*;
use purrdf::shapes::engine::{parse_shapes, validate_dataset};

const PREFIXES: &str = "@prefix ex: <https://example.org/> .\n";

fn qualified(class: &str, min: Option<u32>, max: Option<u32>) -> ConstraintComponent {
    ConstraintComponent::QualifiedValueShape {
        shape: vec![ConstraintComponent::Class(format!(
            "https://example.org/{class}"
        ))],
        min,
        max,
    }
}

fn project(components: Vec<ConstraintComponent>) -> String {
    let property = PropertyConstraintIr::new("https://example.org/p", None, None, None, components)
        .unwrap()
        .with_message("the qualified requirement must hold")
        .unwrap();
    let shape = ValidationShapeIr::new(
        "https://example.org/shape",
        ShapeTarget::Class("https://example.org/Cell".into()),
        vec![property],
        None,
    )
    .unwrap()
    .with_failure_class("https://example.org/MissingEvidence")
    .unwrap();
    format!("{SHACL_PREFIXES}{}", project_validation_shape_shacl(&shape))
}

fn check(shapes: &str, data: &str, component: Option<&str>) {
    let shapes = parse_shapes(shapes, None).expect("GMEOW projection is valid SHACL");
    let data = format!("{PREFIXES}ex:focus a ex:Cell .\n{data}");
    let dataset = purrdf::parse_dataset(data.as_bytes(), "text/turtle", None).unwrap();
    let report = validate_dataset(&dataset, &shapes).unwrap();
    assert_eq!(report.conforms, component.is_none(), "{report:#?}");
    if let Some(component) = component {
        assert_eq!(report.results.len(), 1, "one failing bound, one finding");
        assert!(
            report.results[0]
                .source_constraint_component
                .as_str()
                .ends_with(component),
            "{report:#?}"
        );
        assert_eq!(
            report.results[0].message.as_deref(),
            Some("the qualified requirement must hold")
        );
    }
}

#[test]
fn separate_canonical_bounds_share_one_qualified_value_shape() {
    let shapes = project(vec![
        qualified("Required", None, Some(1)),
        qualified("Required", Some(1), None),
        qualified("Required", Some(0), Some(3)),
    ]);
    assert_eq!(shapes.matches("sh:qualifiedValueShape").count(), 1);
    assert_eq!(shapes.matches("enforcesFailureClass").count(), 2);
    check(&shapes, "", Some("QualifiedMinCountConstraintComponent"));
    check(&shapes, "ex:focus ex:p ex:a . ex:a a ex:Required .", None);
    check(
        &shapes,
        "ex:focus ex:p ex:a, ex:b . ex:a a ex:Required . ex:b a ex:Required .",
        Some("QualifiedMaxCountConstraintComponent"),
    );
    check(
        &shapes,
        "ex:focus ex:p ex:a, ex:b . ex:a a ex:Other . ex:b a ex:Other .",
        Some("QualifiedMinCountConstraintComponent"),
    );
}

#[test]
fn different_qualifying_classes_keep_independent_counts_and_metadata() {
    let shapes = project(vec![
        qualified("First", Some(1), Some(1)),
        qualified("Second", Some(1), Some(1)),
    ]);
    assert_eq!(shapes.matches("sh:property").count(), 2);
    assert_eq!(shapes.matches("enforcesFailureClass").count(), 3);
    assert_eq!(shapes.matches("sh:message").count(), 2);
    check(
        &shapes,
        "ex:focus ex:p ex:a, ex:b . ex:a a ex:First . ex:b a ex:Second .",
        None,
    );
    for class in ["First", "Second"] {
        check(
            &shapes,
            &format!("ex:focus ex:p ex:a . ex:a a ex:{class} ."),
            Some("QualifiedMinCountConstraintComponent"),
        );
    }
    check(
        &shapes,
        "ex:focus ex:p ex:a, ex:b, ex:c . ex:a a ex:First . ex:b a ex:First . ex:c a ex:Second .",
        Some("QualifiedMaxCountConstraintComponent"),
    );
}

#[test]
fn inconsistent_qualified_bounds_remain_unsatisfiable() {
    let shapes = project(vec![
        qualified("Required", Some(2), None),
        qualified("Required", None, Some(1)),
    ]);
    check(
        &shapes,
        "ex:focus ex:p ex:a . ex:a a ex:Required .",
        Some("QualifiedMinCountConstraintComponent"),
    );
    check(
        &shapes,
        "ex:focus ex:p ex:a, ex:b . ex:a a ex:Required . ex:b a ex:Required .",
        Some("QualifiedMaxCountConstraintComponent"),
    );
}

#[test]
fn canonical_minimum_and_maximum_project_to_an_executable_single_count_domain() {
    let prefixes = "@prefix g: <https://blackcatinformatics.ca/gmeow/> .\n\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n";
    let source = format!(
        "{prefixes}g:Cell a logic:Class ; logic:subClassOf\n\
         [ a logic:Restriction ; logic:onProperty g:p ; logic:onClass g:Value ; logic:minQualifiedCardinality 1 ],\n\
         [ a logic:Restriction ; logic:onProperty g:p ; logic:onClass g:Value ; logic:maxQualifiedCardinality 1 ] ."
    );
    let dataset = purrdf::parse_dataset(source.as_bytes(), "text/turtle", None).unwrap();
    let shapes = crate::frontend::derive_validation_shapes(&dataset).unwrap();
    let cell = shapes
        .iter()
        .find(
            |shape| matches!(&shape.target, ShapeTarget::Class(class) if class.ends_with("/Cell")),
        )
        .expect("canonical restrictions derive a Cell validation shape");
    let before = cell.clone();
    let projected = format!("{SHACL_PREFIXES}{}", project_validation_shape_shacl(cell));
    assert_eq!(
        cell, &before,
        "projection never rewrites canonical restrictions"
    );
    assert_eq!(projected.matches("sh:qualifiedValueShape").count(), 1);
    let executable = parse_shapes(&projected, None).unwrap();
    for (data, conforms) in [
        ("g:focus a g:Cell .", false),
        ("g:focus a g:Cell ; g:p g:a . g:a a g:Value .", true),
        (
            "g:focus a g:Cell ; g:p g:a, g:b . g:a a g:Value . g:b a g:Value .",
            false,
        ),
    ] {
        let source = format!("{prefixes}{data}");
        let dataset = purrdf::parse_dataset(source.as_bytes(), "text/turtle", None).unwrap();
        let report = validate_dataset(&dataset, &executable).unwrap();
        assert_eq!(report.conforms, conforms, "{data}: {report:#?}");
    }
}
