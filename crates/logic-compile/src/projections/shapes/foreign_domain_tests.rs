// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Derived shapes are named only in the authoring namespaces.

use super::*;
use crate::frontend::{derive_validation_shapes, is_authoring_namespace};
use purrdf::shapes::engine::{parse_shapes, validate_dataset};

const PREFIXES: &str = "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
     @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
     @prefix g: <https://blackcatinformatics.ca/gmeow/> .\n\
     @prefix ex: <https://example.org/> .\n";

fn dataset(body: &str) -> std::sync::Arc<purrdf::RdfDataset> {
    purrdf::parse_dataset(format!("{PREFIXES}{body}").as_bytes(), "text/turtle", None).unwrap()
}

/// Protects: a functional or inverse-functional property whose domain or range class lies
/// outside the authoring namespaces derives no shape named in a foreign namespace, and its
/// at-most-one rule is still enforced on every subject through the property-scoped shape.
#[test]
fn functional_property_with_a_foreign_domain_derives_no_foreign_shape() {
    let shapes = derive_validation_shapes(&dataset(
        "g:run logic:domain rdf:Statement .\n\
         [] a logic:PropertyCharacteristicAssertion ; logic:characterizes g:run ; \
             logic:characteristicSort logic:functionalProperty .\n\
         g:token logic:range rdf:Statement .\n\
         [] a logic:PropertyCharacteristicAssertion ; logic:characterizes g:token ; \
             logic:characteristicSort logic:inverseFunctionalProperty .\n",
    ))
    .unwrap();
    let foreign: Vec<&str> = shapes
        .iter()
        .map(|shape| shape.iri.as_str())
        .filter(|iri| !is_authoring_namespace(iri))
        .collect();
    assert_eq!(foreign, Vec::<&str>::new());

    let lowered: Vec<String> = shapes.iter().map(project_validation_shape_shacl).collect();
    let plan = parse_shapes(&format!("{SHACL_PREFIXES}{}", lowered.join("\n")), None)
        .expect("the projection is valid SHACL");
    let report = validate_dataset(
        &dataset(
            "ex:one a rdf:Statement ; g:run 1 .\n\
             ex:two a rdf:Statement ; g:run 1 , 2 .\n",
        ),
        &plan,
    )
    .unwrap();
    let findings: Vec<(String, &str)> = report
        .results
        .iter()
        .map(|finding| {
            (
                finding.focus_node.to_string(),
                finding.source_constraint_component.as_str(),
            )
        })
        .collect();
    assert_eq!(
        findings,
        [(
            "<https://example.org/two>".to_owned(),
            "http://www.w3.org/ns/shacl#MaxCountConstraintComponent"
        )]
    );
}
