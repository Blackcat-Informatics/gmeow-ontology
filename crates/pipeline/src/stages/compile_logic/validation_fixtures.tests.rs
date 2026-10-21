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
fn result_contract_checks_presence_and_every_value_without_cross_record_leaks() {
    // Independently authored synthetic law: no repository module or corpus is loaded.
    let text = r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
        @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
        @prefix ex: <urn:result-contract:> .
        @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
        ex:contract a logic:Constraint ; gmeow:enforcesFailureClass ex:Failure ; logic:integrity [ a logic:Formula ; logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable "this" ], [ logic:termIndex 1 ; logic:termVariable "result" ] ; logic:forall [ a logic:Formula ; logic:antecedent [ a logic:Formula ; logic:and [ a logic:Formula ; logic:relation rdf:type ; logic:argument [ logic:termIndex 0 ; logic:termVariable "this" ], [ logic:termIndex 1 ; logic:termIri ex:Report ] ],
[ a logic:Formula ; logic:relation ex:result ; logic:argument [ logic:termIndex 0 ; logic:termVariable "this" ], [ logic:termIndex 1 ; logic:termVariable "result" ] ] ] ; logic:consequent [ a logic:Formula ; logic:and [ a logic:Formula ; logic:antecedent [ a logic:Formula ; logic:and [ a logic:Formula ; logic:relation rdf:type ; logic:argument [ logic:termIndex 0 ; logic:termVariable "this" ], [ logic:termIndex 1 ; logic:termIri ex:Report ] ],
[ a logic:Formula ; logic:relation ex:result ; logic:argument [ logic:termIndex 0 ; logic:termVariable "this" ], [ logic:termIndex 1 ; logic:termVariable "result" ] ] ] ; logic:consequent [ a logic:Formula ; logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable "value" ] ; logic:exists [ a logic:Formula ; logic:relation ex:value ; logic:argument [ logic:termIndex 0 ; logic:termVariable "result" ], [ logic:termIndex 1 ; logic:termVariable "value" ] ] ] ],
[ a logic:Formula ; logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable "valueTyped" ] ; logic:forall [ a logic:Formula ; logic:antecedent [ a logic:Formula ; logic:and [ a logic:Formula ; logic:and [ a logic:Formula ; logic:relation rdf:type ; logic:argument [ logic:termIndex 0 ; logic:termVariable "this" ], [ logic:termIndex 1 ; logic:termIri ex:Report ] ],
[ a logic:Formula ; logic:relation ex:result ; logic:argument [ logic:termIndex 0 ; logic:termVariable "this" ], [ logic:termIndex 1 ; logic:termVariable "result" ] ] ],
[ a logic:Formula ; logic:relation ex:value ; logic:argument [ logic:termIndex 0 ; logic:termVariable "result" ], [ logic:termIndex 1 ; logic:termVariable "valueTyped" ] ] ] ; logic:consequent [ a logic:Formula ; logic:relation rdf:type ; logic:argument [ logic:termIndex 0 ; logic:termVariable "valueTyped" ], [ logic:termIndex 1 ; logic:termIri ex:Value ] ] ] ] , [ a logic:Formula ; logic:antecedent [ a logic:Formula ; logic:and
 [ a logic:Formula ; logic:relation rdf:type ; logic:argument [ logic:termIndex 0 ; logic:termVariable "this" ], [ logic:termIndex 1 ; logic:termIri ex:Report ] ],
 [ a logic:Formula ; logic:relation ex:result ; logic:argument [ logic:termIndex 0 ; logic:termVariable "this" ], [ logic:termIndex 1 ; logic:termVariable "result" ] ] ] ; logic:consequent [ a logic:Formula ; logic:or
 [ a logic:Formula ; logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable "target" ] ; logic:exists [ a logic:Formula ; logic:relation ex:target ; logic:argument [ logic:termIndex 0 ; logic:termVariable "result" ], [ logic:termIndex 1 ; logic:termVariable "target" ] ] ],
 [ a logic:Formula ; logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable "parameter" ] ; logic:exists [ a logic:Formula ; logic:relation ex:parameter ; logic:argument [ logic:termIndex 0 ; logic:termVariable "result" ], [ logic:termIndex 1 ; logic:termVariable "parameter" ] ] ] ] ] ] ] ] ."#;
    let product = compile(text);
    let dataset = purrdf::parse_dataset(text.as_bytes(), "text/turtle", None).unwrap();
    let prepared = PreparedLogicSource::new(&dataset).unwrap();
    let (constraints, diagnostics) = prepared.constraints();
    assert!(diagnostics.is_empty());
    let program =
        LogicProgram::new(Vec::new(), Vec::new(), Vec::new(), None).with_constraints(constraints);
    let (rules, residue) = gmeow_logic_compile::relational_core::lower_violation_constraints_to_rc(
        &program,
        &Default::default(),
    );
    assert!(residue.is_empty(), "{residue:?}");
    assert_eq!(rules.len(), 3);
    assert!(product.diagnostics.is_empty(), "{:?}", product.diagnostics);
    assert!(
        product
            .procedural_shacl
            .contains("contractProceduralConstraintShape")
    );
    let shapes = parse_shapes(&product.procedural_shacl, None).unwrap();
    for (name, body, valid) in [
        ("typed value", "ex:result ex:value ex:good .", true),
        (
            "alternative target",
            "ex:result ex:value ex:good . ex:result ex:parameter ex:goal .",
            true,
        ),
        ("missing value", "", false),
        ("missing target", "ex:result ex:value ex:good .", false),
        ("untyped value", "ex:result ex:value ex:bad .", false),
        (
            "valid plus malformed extra",
            "ex:result ex:value ex:good, ex:bad .",
            false,
        ),
        ("literal value", "ex:result ex:value 1 .", false),
        (
            "neighbor cannot supply missing value",
            "ex:other a ex:Report ; ex:result ex:otherResult . ex:otherResult ex:value ex:good .",
            false,
        ),
        (
            "one good result cannot conceal another",
            "ex:result ex:value ex:good . ex:report ex:result ex:emptyResult .",
            false,
        ),
    ] {
        let target = if name == "alternative target" || name == "missing target" {
            ""
        } else {
            "ex:result ex:target ex:goal ."
        };
        let input = format!(
            "@prefix ex: <urn:result-contract:> . ex:good a ex:Value . ex:report a ex:Report ; ex:result ex:result . {target} {body}"
        );
        let data = purrdf::parse_dataset(input.as_bytes(), "text/turtle", None).unwrap();
        let report = validate_dataset(&data, &shapes).unwrap();
        assert_eq!(report.results.is_empty(), valid, "{name}: {report:?}");
        if !valid {
            assert!(
                report
                    .results
                    .iter()
                    .any(|finding| finding.focus_node.to_string().contains("report")),
                "{name}: {report:?}"
            );
        }
    }
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
    let shapes = parse_shapes(&format!("{SH_HEADER}{}", shape.shacl), None).unwrap();
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
    assert!(
        report(&valid.replace(
            "gmeow:requestingAgent ex:person",
            "gmeow:requestingAgent [ a gmeow:Agent ]",
        ))
        .results
        .is_empty(),
        "the requester may be an anonymous typed agent"
    );
    for missing in [
        "gmeow:requestingAgent ex:person ;",
        "gmeow:requestedErasureTarget ex:claim ;",
        "gmeow:requestUnderRight ex:right",
    ] {
        assert!(!report(&valid.replace(missing, "")).results.is_empty());
    }
    for (from, to) in [
        (
            "gmeow:requestingAgent ex:person",
            "gmeow:requestingAgent \"requester\"",
        ),
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
        (
            "gmeow:requestUnderRight ex:right",
            "gmeow:requestUnderRight []",
        ),
    ] {
        assert!(!report(&valid.replace(from, to)).results.is_empty());
    }
}
