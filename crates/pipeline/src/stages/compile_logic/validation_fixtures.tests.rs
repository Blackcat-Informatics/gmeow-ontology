// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic_compile::ir::{ConstraintComponent, ShapeTarget};

fn compile(text: &str) -> ModuleProduct {
    let dataset = purrdf::parse_dataset(text.as_bytes(), "text/turtle", None).unwrap();
    let source = PreparedLogicSource::new(&dataset).unwrap();
    compile_module(&source).unwrap()
}

#[test]
fn typed_shape_and_emitted_surface_survive_the_producer_record() {
    let product = compile(
        r#"
            @prefix logic: <https://blackcatinformatics.ca/logic/> .
            @prefix ex: <https://blackcatinformatics.ca/gmeow/testing/> .
            ex:C a logic:Class ; logic:subClassOf [
                a logic:Restriction ; logic:onProperty ex:p ; logic:allValuesFrom ex:D
            ] .
            ex:D a logic:Class . ex:p a logic:ObjectProperty .
        "#,
    );
    let wire = serde_json::to_vec(&product).unwrap();
    let restored: ModuleProduct = serde_json::from_slice(&wire).unwrap();
    let shape = restored
        .shapes
        .iter()
        .find(|shape| {
            shape.ir.target
                == ShapeTarget::Class("https://blackcatinformatics.ca/gmeow/testing/C".to_owned())
        })
        .expect("the source class has its produced validation shape");
    assert!(shape.ir.properties.iter().any(|property| {
            property.path == "https://blackcatinformatics.ca/gmeow/testing/p" && property.components.iter().any(|component| {
                matches!(component, ConstraintComponent::Class(class) if class == "https://blackcatinformatics.ca/gmeow/testing/D")
            })
        }));
    assert!(
        shape
            .shacl
            .contains("<https://blackcatinformatics.ca/gmeow/testing/p>")
    );
    assert!(
        shape
            .shacl
            .contains("<https://blackcatinformatics.ca/gmeow/testing/D>")
    );
}

#[test]
fn malformed_constraints_remain_visible_in_the_record() {
    let product = compile(
        r#"
            @prefix logic: <https://blackcatinformatics.ca/logic/> .
            <urn:broken> a logic:Constraint ; logic:integrity <urn:missing-formula> .
        "#,
    );
    let wire = serde_json::to_vec(&product).unwrap();
    let restored: ModuleProduct = serde_json::from_slice(&wire).unwrap();
    assert!(
        restored
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == "MALFORMED_CONSTRAINT" }),
        "a failed constraint cannot become a silently empty shape product"
    );
}
