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

#[test]
fn forget_request_requires_roles_and_named_references_without_granting_permission() {
    // Independently authored, tiny input: this never loads or produces the corpus.
    let prefixes = "@prefix logic: <https://blackcatinformatics.ca/logic/> .
        @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
        @prefix ex: <https://example.org/> .";
    let mut source = format!("{prefixes} gmeow:ForgetRequest a logic:Class .");
    for (property, range) in [
        ("requestingAgent", "gmeow:Agent"),
        ("requestedErasureTarget", "logic:Thing"),
        ("requestUnderRight", "logic:Thing"),
    ] {
        source.push_str(&format!(
            "gmeow:ForgetRequest logic:subClassOf [ a logic:Restriction ;
                logic:onProperty gmeow:{property} ; logic:allValuesFrom {range} ] .
            [ a logic:ClosureEntry ; logic:onClass gmeow:ForgetRequest ;
                logic:closureKey \"https://blackcatinformatics.ca/gmeow/{property}\" ;
                logic:closureValue logic:ClosedWorldClosure ] ."
        ));
    }
    for property in ["requestedErasureTarget", "requestUnderRight"] {
        source.push_str(&format!(
            "ex:{property}Kind a logic:PathNodeKindConstraint ;
                logic:onClass gmeow:ForgetRequest ; logic:valuePath gmeow:{property} ;
                logic:nodeKind \"IRI\" ; logic:formalizes gmeow:ForgetRequest ."
        ));
    }
    let product = compile(&source);
    let shape = product
        .shapes
        .iter()
        .find(|shape| {
            shape.ir.target
                == ShapeTarget::Class(
                    "https://blackcatinformatics.ca/gmeow/ForgetRequest".to_owned(),
                )
        })
        .expect("the request has a derived validation shape");
    for property in [
        "requestingAgent",
        "requestedErasureTarget",
        "requestUnderRight",
    ] {
        assert!(shape.ir.properties.iter().any(|path| {
            path.path == format!("https://blackcatinformatics.ca/gmeow/{property}")
                && path.min_count == Some(1)
        }));
    }
    let shape_graph = purrdf::parse_dataset(
        format!("{SH_HEADER}{}", shape.shacl).as_bytes(),
        "text/turtle",
        None,
    )
    .unwrap();
    let shapes = parse_shapes(&shape_graph).unwrap();
    let valid = "ex:request a gmeow:ForgetRequest ;
        gmeow:requestingAgent ex:person ; gmeow:requestedErasureTarget ex:claim ;
        gmeow:requestUnderRight ex:right . ex:person a gmeow:Agent .";
    let report = |text: &str| {
        let data =
            purrdf::parse_dataset(format!("{prefixes} {text}").as_bytes(), "text/turtle", None)
                .unwrap();
        validate_dataset(&data, &shapes).unwrap()
    };
    assert!(report(valid).results.is_empty());
    for missing in [
        "gmeow:requestingAgent ex:person ;",
        "gmeow:requestedErasureTarget ex:claim ;",
        "gmeow:requestUnderRight ex:right",
    ] {
        assert!(!report(&valid.replace(missing, "")).results.is_empty());
    }
    for (from, to) in [
        (
            "gmeow:requestedErasureTarget ex:claim",
            "gmeow:requestedErasureTarget []",
        ),
        (
            "gmeow:requestedErasureTarget ex:claim",
            "gmeow:requestedErasureTarget \"claim\"",
        ),
        (
            "gmeow:requestUnderRight ex:right",
            "gmeow:requestUnderRight \"right\"",
        ),
    ] {
        assert!(!report(&valid.replace(from, to)).results.is_empty());
    }
}
