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
        .with_messages(vec![purrdf::RdfLiteral {
            lexical_form: "the qualified requirement must hold".into(),
            datatype: None,
            language: None,
            direction: None,
        }])
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

/// Check projected property validation and retain the owning message, path and failure class.
fn check(shapes: &str, data: &str, component: Option<&str>) {
    let document = purrdf::shapes::text_ingest::parse_turtle_document(shapes, None).unwrap();
    let shapes =
        purrdf::shapes::shapes::from_dataset_with_prefixes(&document.dataset, &document.prefixes)
            .expect("GMEOW projection is valid SHACL");
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
            report.results[0]
                .messages
                .iter()
                .map(purrdf::shapes::term::Literal::value)
                .collect::<Vec<_>>(),
            ["the qualified requirement must hold"]
        );
        let result = &report.results[0];
        assert_eq!(
            result.result_path.as_ref().unwrap().to_rdf_term(),
            purrdf::RdfTerm::Iri("https://example.org/p".into())
        );
        assert!(
            purrdf::native_quads::flat_rdf_quads(&document.dataset).any(|quad| {
                quad.subject == result.source_shape.to_rdf_term()
                    && quad.predicate == "https://blackcatinformatics.ca/gmeow/enforcesFailureClass"
                    && quad.object
                        == purrdf::RdfTerm::Iri("https://example.org/MissingEvidence".into())
            })
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

/// Contradictory bounds and datatype intersections must retain their original count domain.
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

    // Values satisfying only one datatype must never be counted as satisfying both.
    for (min, max, component) in [
        (Some(1), None, Some("QualifiedMinCountConstraintComponent")),
        (None, Some(0), None),
    ] {
        let shapes = project(vec![ConstraintComponent::QualifiedValueShape {
            shape: vec![
                ConstraintComponent::Datatype("http://www.w3.org/2001/XMLSchema#string".into()),
                ConstraintComponent::Datatype("http://www.w3.org/2001/XMLSchema#integer".into()),
            ],
            min,
            max,
        }]);
        for value in ["\"text\"", "7"] {
            check(&shapes, &format!("ex:focus ex:p {value} ."), component);
        }
    }
}

/// Execute authored restrictions through derivation, target admission and conjunctive validation.
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

    // Same-path datatype restrictions remain an intersection after the real derive/merge path.
    let source = format!(
        "{prefixes}g:Cell a logic:Class ; g:enforcesFailureClass g:ConflictingDatatypes ;\n\
         logic:subClassOf\n\
         [ a logic:Restriction ; logic:onProperty g:p ; logic:allValuesFrom xsd:string ],\n\
         [ a logic:Restriction ; logic:onProperty g:p ; logic:allValuesFrom xsd:integer ] ."
    );
    let dataset = purrdf::parse_dataset(source.as_bytes(), "text/turtle", None).unwrap();
    let shapes = crate::frontend::derive_validation_shapes(&dataset).unwrap();
    let cell = shapes
        .iter()
        .find(|shape| !shape.properties.is_empty())
        .unwrap();
    let projected = format!("{SHACL_PREFIXES}{}", project_validation_shape_shacl(cell));
    let document = purrdf::shapes::text_ingest::parse_turtle_document(&projected, None).unwrap();
    let executable =
        purrdf::shapes::shapes::from_dataset_with_prefixes(&document.dataset, &document.prefixes)
            .unwrap();
    for value in [None, Some("\"text\""), Some("7")] {
        let edge = value.map_or_else(String::new, |value| format!("g:focus g:p {value} ."));
        let source = format!("{prefixes}g:focus a g:Cell . {edge}");
        let dataset = purrdf::parse_dataset(source.as_bytes(), "text/turtle", None).unwrap();
        let report = validate_dataset(&dataset, &executable).unwrap();
        assert_eq!(report.conforms, value.is_none(), "{report:#?}");
        if value.is_some() {
            assert_eq!(report.results.len(), 1);
            let result = &report.results[0];
            assert!(
                result
                    .source_constraint_component
                    .as_str()
                    .ends_with("AndConstraintComponent")
            );
            assert_eq!(
                result.result_path.as_ref().unwrap().to_rdf_term(),
                purrdf::RdfTerm::Iri("https://blackcatinformatics.ca/gmeow/p".into())
            );
            assert!(
                purrdf::native_quads::flat_rdf_quads(&document.dataset).any(|quad| {
                    quad.subject == result.source_shape.to_rdf_term()
                        && quad.predicate
                            == "https://blackcatinformatics.ca/gmeow/enforcesFailureClass"
                        && quad.object
                            == purrdf::RdfTerm::Iri(
                                "https://blackcatinformatics.ca/gmeow/ConflictingDatatypes".into(),
                            )
                })
            );
        }
    }

    let range = ValidationShapeIr::new(
        "https://example.org/range",
        ShapeTarget::ObjectsOf("https://example.org/p".into()),
        vec![],
        None,
    )
    .unwrap()
    .with_node_components(cell.properties[0].components.clone())
    .unwrap();
    let projected = format!("{SHACL_PREFIXES}{}", project_validation_shape_shacl(&range));
    let executable = parse_shapes(&projected, None).unwrap();
    for value in ["\"text\"", "7"] {
        let source = format!("{PREFIXES}ex:focus ex:p {value} .");
        let dataset = purrdf::parse_dataset(source.as_bytes(), "text/turtle", None).unwrap();
        let report = validate_dataset(&dataset, &executable).unwrap();
        assert!(!report.conforms);
        assert_eq!(report.results.len(), 1);
        let result = &report.results[0];
        assert_eq!(
            result.source_shape.to_rdf_term(),
            purrdf::RdfTerm::Iri("https://example.org/range".into())
        );
        assert!(
            result
                .source_constraint_component
                .as_str()
                .ends_with("AndConstraintComponent")
        );
        assert!(result.result_path.is_none());
    }
}
