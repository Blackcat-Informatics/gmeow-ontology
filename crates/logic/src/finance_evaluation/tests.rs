// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Independent tiny owner contracts exercise production wiring and proof;
//! authenticated repository owner acceptance is a separate producer-bound lane.

use super::*;
use purrdf::shapes::{
    expression::NodeExpr,
    shapes::{ConstraintAnnotation, ModelOwner},
    term::{Literal as ShapeLiteral, NamedNode, Term as ShapeTerm},
};

fn list_text(members: Vec<String>, cell: impl FnMut(usize) -> String) -> String {
    let vocab = purrdf_core::ListVocab {
        first: "rdf:first".to_owned(),
        rest: "rdf:rest".to_owned(),
        nil: "rdf:nil".to_owned(),
    };
    let mut text = String::new();
    purrdf_core::build_rdf_list(members, &vocab, cell, |s, p, o| {
        text.push_str(&format!("{s} {p} {o} .\n"));
    });
    text
}

fn owner() -> String {
    let mut text = String::from(
        r#"
@prefix g: <https://blackcatinformatics.ca/gmeow/> .
@prefix m: <https://blackcatinformatics.ca/math/> .
@prefix l: <https://blackcatinformatics.ca/logic/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
g:MonetaryAmount l:subClassOf m:Quantity .
g:FinancialPeriodRate l:subClassOf m:Quantity .
g:FinancialPeriodCount l:subClassOf m:Quantity .
g:FinancialReturnRatio l:subClassOf m:Quantity .
g:SimpleInterestCalculation l:subClassOf g:Measurement .
g:HoldingPeriodReturnCalculation l:subClassOf g:Measurement .
g:Measurement l:subClassOf g:Observation .
g:referenceFrameUSD a g:ReferenceFrame ; g:frameKind g:frameKindScalar ; g:frameRealm g:frameRealmCurrency ; g:dimensionCount 1 ; g:determinacyModel g:determinacyCrisp ; g:hasAxis g:axisScalar ; g:requiresHost false .
g:referenceFrameEUR a g:ReferenceFrame ; g:frameKind g:frameKindScalar ; g:frameRealm g:frameRealmCurrency ; g:dimensionCount 1 ; g:determinacyModel g:determinacyCrisp ; g:hasAxis g:axisScalar ; g:requiresHost false .
"#,
    );
    for (method, prefix, roles, output) in [
        (
            "methodSimpleInterest",
            "simple",
            vec![
                ("principalAmount", "MonetaryAmount"),
                ("interestRate", "FinancialPeriodRate"),
                ("interestPeriodCount", "FinancialPeriodCount"),
            ],
            "MonetaryAmount",
        ),
        (
            "methodHoldingPeriodReturn",
            "return",
            vec![
                ("openingAmount", "MonetaryAmount"),
                ("closingAmount", "MonetaryAmount"),
            ],
            "FinancialReturnRatio",
        ),
    ] {
        text.push_str(&format!("g:{method} a g:ObservationMethod, l:CallableSignature ; l:inputParameters <urn:{prefix}:list:0> ; l:outputParameters <urn:{prefix}:out:list> ; g:calculationExpression <urn:{prefix}:recipe> .\n"));
        text.push_str(&list_text(
            (0..roles.len())
                .map(|i| format!("<urn:{prefix}:parameter:{i}>"))
                .collect(),
            |i| format!("<urn:{prefix}:list:{i}>"),
        ));
        for (i, (role, class)) in roles.iter().enumerate() {
            text.push_str(&format!("<urn:{prefix}:parameter:{i}> a l:CallParameter ; l:parameterPredicate g:{role} ; l:parameterValueType g:{class} ; l:parameterRequired true ; m:hasMathematicalSymbol <urn:{prefix}:symbol:{i}> .\n<urn:{prefix}:symbol:{i}> a m:MathematicalSymbol .\n<urn:{prefix}:leaf:{i}> a m:SymbolReference ; m:hasMathematicalSymbol <urn:{prefix}:symbol:{i}> .\n"));
        }
        text.push_str(&list_text(vec![format!("<urn:{prefix}:output>")], |_| {
            format!("<urn:{prefix}:out:list>")
        }));
        text.push_str(&format!("<urn:{prefix}:output> a l:CallResult ; l:parameterPredicate g:observationResult ; l:parameterValueType g:{output} .\n"));
    }
    for (node, op, left, right) in [
        (
            "simple:product",
            "Multiplication",
            "simple:leaf:0",
            "simple:leaf:1",
        ),
        (
            "simple:recipe",
            "Multiplication",
            "simple:product",
            "simple:leaf:2",
        ),
        (
            "return:difference",
            "Subtraction",
            "return:leaf:1",
            "return:leaf:0",
        ),
        (
            "return:recipe",
            "Division",
            "return:difference",
            "return:leaf:0",
        ),
    ] {
        text.push_str(&format!("<urn:{node}> a m:ApplicationExpression ; m:operator m:{op} ; m:argumentSlot <urn:{node}:0>, <urn:{node}:1> .\n<urn:{node}:0> a m:ArgumentSlot ; m:slotIndex 0 ; m:slotExpression <urn:{left}> .\n<urn:{node}:1> a m:ArgumentSlot ; m:slotIndex 1 ; m:slotExpression <urn:{right}> .\n"));
    }
    text
}

fn input(return_case: bool) -> String {
    let roles = if return_case {
        "a g:HoldingPeriodReturnCalculation ; g:observationMethod g:methodHoldingPeriodReturn ; g:calculationExpression <urn:return:recipe> ; g:openingAmount <urn:opening> ; g:closingAmount <urn:closing>"
    } else {
        "a g:SimpleInterestCalculation ; g:observationMethod g:methodSimpleInterest ; g:calculationExpression <urn:simple:recipe> ; g:principalAmount <urn:opening> ; g:interestRate <urn:rate> ; g:interestPeriodCount <urn:periods>"
    };
    format!(
        r#"
@prefix g: <https://blackcatinformatics.ca/gmeow/> .
@prefix m: <https://blackcatinformatics.ca/math/> .
<urn:calculation> {roles} ; g:vantage <urn:alice> ; g:observedFeature <urn:account> ; g:calculationPeriodBasis <urn:basis> .
<urn:alice> a g:Entity . <urn:account> a g:Entity .
<urn:opening> a g:MonetaryAmount ; m:quantityValue 1000.00 ; m:hasDimension m:dimensionless ; g:currency g:referenceFrameUSD .
<urn:closing> a g:MonetaryAmount ; m:quantityValue 1200.00 ; m:hasDimension m:dimensionless ; g:currency g:referenceFrameUSD .
<urn:rate> a g:FinancialPeriodRate ; m:quantityValue 0.05 ; m:hasDimension m:dimensionless ; g:hasReferenceFrame <urn:basis> .
<urn:periods> a g:FinancialPeriodCount ; m:quantityValue 2.0 ; m:hasDimension m:dimensionless ; g:hasReferenceFrame <urn:basis> .
<urn:basis> a g:ReferenceFrame ; g:frameKind g:frameKindScalar ; g:frameRealm g:frameRealmTemporal ; g:dimensionCount 1 ; g:determinacyModel g:determinacyCrisp ; g:hasAxis g:axisScalar ; g:requiresHost false .
"#
    )
}

fn shapes() -> Shapes {
    let mut text = String::from("@prefix sh: <http://www.w3.org/ns/shacl#> .\n");
    for class in [
        format!("{M}Quantity"),
        format!("{M}ApplicationExpression"),
        format!("{M}ArgumentSlot"),
        format!("{M}SymbolReference"),
        format!("{G}MonetaryAmount"),
        format!("{G}ReferenceFrame"),
        format!("{G}Measurement"),
        format!("{G}Observation"),
        format!("{G}SimpleInterestCalculation"),
        format!("{G}HoldingPeriodReturnCalculation"),
        format!("{G}FinancialPeriodRate"),
        format!("{G}FinancialPeriodCount"),
        format!("{G}FinancialReturnRatio"),
    ] {
        text.push_str(&format!(
            "<{class}/synthetic-shape> a sh:NodeShape ; sh:targetClass <{class}> .\n"
        ));
    }
    // Independent wiring controls: these tiny products deliberately do not claim
    // equivalence to repository laws. Authenticated products are tested separately.
    for identity in PROCEDURAL_PRODUCTS {
        text.push_str(&format!("<{identity}> a sh:NodeShape ; sh:targetClass <urn:synthetic:Absent> ; sh:sparql [ sh:select \"SELECT $this WHERE {{ FILTER(false) }}\" ] .\n"));
    }
    purrdf::shapes::engine::parse_shapes(&text, None).unwrap()
}

fn run(source: &str, owner_text: &str, identity: &str) -> gmeow_errors::Result<Evaluation> {
    // gmeow-test-input: synthetic-only
    let source = purrdf::parse_dataset(source.as_bytes(), "text/turtle", None).unwrap();
    let owner = purrdf::parse_dataset(owner_text.as_bytes(), "text/turtle", None).unwrap();
    evaluate(
        &source,
        FinanceOwner {
            dataset: &owner,
            shapes: &shapes(),
            identity,
        },
        "urn:calculation",
    )
}

#[test]
fn finance_evaluation_callable_lists_use_strict_collection_and_named_member_contracts() {
    let valid = owner();
    let read = |text: &str| {
        let dataset = purrdf::parse_dataset(text.as_bytes(), "text/turtle", None).unwrap();
        let records = Records::read(&dataset, true).unwrap();
        signature(&records, format!("{G}methodSimpleInterest"))
    };
    let complete = read(&valid).unwrap();
    assert_eq!(
        complete
            .parameters
            .iter()
            .map(|(role, _, _)| role.as_str())
            .collect::<Vec<_>>(),
        [
            format!("{G}principalAmount"),
            format!("{G}interestRate"),
            format!("{G}interestPeriodCount")
        ]
    );
    for (name, from, to) in [
        (
            "cycle",
            "<urn:simple:list:2> rdf:rest rdf:nil",
            "<urn:simple:list:2> rdf:rest <urn:simple:list:0>",
        ),
        (
            "missing first",
            "<urn:simple:list:0> rdf:first <urn:simple:parameter:0> .",
            "",
        ),
        (
            "missing rest",
            "<urn:simple:list:0> rdf:rest <urn:simple:list:1> .",
            "",
        ),
        (
            "duplicate first",
            "<urn:simple:list:0> rdf:first <urn:simple:parameter:0>",
            "<urn:simple:list:0> rdf:first <urn:simple:parameter:0>, <urn:simple:parameter:1>",
        ),
        (
            "duplicate rest",
            "<urn:simple:list:0> rdf:rest <urn:simple:list:1>",
            "<urn:simple:list:0> rdf:rest <urn:simple:list:1>, rdf:nil",
        ),
        (
            "literal member",
            "<urn:simple:list:0> rdf:first <urn:simple:parameter:0>",
            "<urn:simple:list:0> rdf:first 'not a parameter'",
        ),
    ] {
        assert!(valid.contains(from), "{name}");
        let error = read(&valid.replace(from, to)).err().expect(name);
        assert!(error.to_string().contains(PROFILE), "{name}: {error}");
    }
    assert!(
        read(&format!(
            "{valid} rdf:nil rdf:first <urn:simple:parameter:0> ."
        ))
        .is_err()
    );
}

#[test]
fn finance_evaluation_mixed_named_and_anonymous_selected_list_edges_still_refuse() {
    let owner = format!(
        "{} <urn:simple:list:0> rdf:first [ a l:CallParameter ] .",
        owner()
    );
    assert!(run(&input(false), &owner, "ambiguous-selected-list-owner").is_err());
}

#[test]
fn finance_evaluation_owner_import_retains_list_cells_members_and_override_checks() {
    let dataset = purrdf::parse_dataset(owner().as_bytes(), "text/turtle", None).unwrap();
    let trusted = Records::read(&dataset, true).unwrap();
    let mut imported = Records::default();
    imported
        .admit_owner(&trusted, [format!("{G}methodSimpleInterest")])
        .unwrap();
    let premises = imported.premises();
    for subject in [
        "urn:simple:list:0",
        "urn:simple:list:1",
        "urn:simple:list:2",
        "urn:simple:out:list",
        "urn:simple:parameter:0",
        "urn:simple:parameter:1",
        "urn:simple:parameter:2",
        "urn:simple:output",
    ] {
        let records: Vec<_> = trusted
            .0
            .iter()
            .filter(|((s, _), _)| s == subject)
            .collect();
        assert!(!records.is_empty(), "owned record {subject}");
        for ((_, predicate), values) in records {
            assert_eq!(imported.values(subject, predicate), values.as_slice());
            for value in values {
                assert!(
                    premises.contains(&Formula::Atom {
                        relation: Term::Iri(predicate.clone()),
                        args: vec![Term::Iri(subject.into()), value.clone()],
                    }),
                    "source support {subject} {predicate}"
                );
            }
        }
    }
    for sabotage in [
        "<urn:simple:list:1> <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil> .",
        "<urn:simple:parameter:1> a <urn:wrong-parameter-type> .",
        "<http://www.w3.org/1999/02/22-rdf-syntax-ns#nil> <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil> .",
    ] {
        let source = purrdf::parse_dataset(sabotage.as_bytes(), "text/turtle", None).unwrap();
        let mut changed = Records::read(&source, false).unwrap();
        assert!(
            changed
                .admit_owner(&trusted, [format!("{G}methodSimpleInterest")])
                .is_err(),
            "{sabotage}"
        );
    }
}

#[test]
fn finance_evaluation_requires_every_selected_procedural_product() {
    require_shapes(&shapes()).expect("the complete selected procedural profile is admitted");
    for identity in PROCEDURAL_PRODUCTS {
        let mut incomplete = shapes();
        let before = incomplete.node_shapes.len();
        incomplete.node_shapes.retain(|shape| {
            !matches!(&shape.id,
            purrdf::shapes::term::Term::NamedNode(iri) if iri.as_str() == *identity)
        });
        assert_eq!(
            incomplete.node_shapes.len() + 1,
            before,
            "remove {identity}"
        );
        assert!(require_shapes(&incomplete).is_err(), "missing {identity}");
    }

    #[derive(Debug)]
    enum UnavailableProduct {
        Body,
        ShapeGuard,
        SparqlGuard,
    }
    for unavailable in [
        UnavailableProduct::Body,
        UnavailableProduct::ShapeGuard,
        UnavailableProduct::SparqlGuard,
    ] {
        let mut changed = shapes();
        let position = changed
            .node_shapes
            .iter()
            .position(|shape| {
                matches!(&shape.id, ShapeTerm::NamedNode(iri) if iri.as_str() == PROCEDURAL_PRODUCTS[0])
            })
            .unwrap();
        let root = &changed.node_shapes[position];
        let id = root.id.clone();
        let mut definition = root
            .owner
            .as_ref()
            .unwrap()
            .definition(&id)
            .unwrap()
            .clone();
        let guard = NodeExpr::Constant(ShapeTerm::Literal(ShapeLiteral::new_typed_literal(
            "true",
            NamedNode::from("http://www.w3.org/2001/XMLSchema#boolean"),
        )));
        match unavailable {
            UnavailableProduct::Body => definition.constraints.clear(),
            UnavailableProduct::ShapeGuard => definition.deactivated_when.push(guard),
            UnavailableProduct::SparqlGuard => {
                let index = definition
                    .constraints
                    .iter()
                    .position(|constraint| matches!(constraint, Constraint::Sparql { .. }))
                    .unwrap();
                definition
                    .constraint_annotations
                    .push(ConstraintAnnotation {
                        constraint: AnnotatedConstraint::Constraint(index),
                        severity: None,
                        messages: Vec::new(),
                        deactivated_when: vec![guard],
                    });
            }
        }
        let owner = ModelOwner::from_definitions(vec![definition]).unwrap();
        changed.node_shapes[position] = owner.root(&id).unwrap();
        // The untouched catalog still contains the valid original declaration;
        // admission must follow the actual exported root's owner instead.
        assert!(require_shapes(&changed).is_err(), "{unavailable:?}");
    }
}

#[test]
fn finance_evaluation_rejects_anonymous_required_owner_records() {
    let owner = owner().replace(
        "l:inputParameters <urn:simple:list:0>",
        "l:inputParameters [ rdf:first <urn:simple:parameter:0> ; rdf:rest rdf:nil ]",
    );
    let error = run(&input(false), &owner, "synthetic-anonymous-owner")
        .err()
        .expect("anonymous owner must be refused");
    assert!(
        error
            .to_string()
            .contains("unsupported anonymous or quoted")
    );
}

#[test]
fn finance_evaluation_contract_projection_retains_named_class_proof_not_anonymous_restrictions() {
    let owner = format!(
        "{} g:MonetaryAmount l:subClassOf [ a l:Restriction ; l:onProperty m:quantityValue ; l:allValuesFrom <http://www.w3.org/2001/XMLSchema#decimal> ] .",
        owner()
    );
    let evaluation = run(
        &input(false),
        &owner,
        "synthetic-owner-with-declarative-restriction",
    )
    .unwrap();
    magnitude(&evaluation, "100");
    assert!(
        crate::result_rdf::project_reasoning_result(&evaluation.result)
            .unwrap()
            .contains("subClassOf")
    );
    // This explicitly bounded synthetic test claims arithmetic proof only;
    // actual declarative restrictions are exercised by producer-owned shapes.
}

fn magnitude(evaluation: &Evaluation, expected: &str) {
    let value = evaluation
        .result
        .inferred()
        .iter()
        .find(|row| {
            !row.is_edb
                && row.subject == evaluation.quantity
                && row.predicate == format!("{M}quantityValue")
        })
        .unwrap();
    let TermValue::Literal {
        lexical_form,
        datatype,
        ..
    } = &value.object
    else {
        panic!("numeric output")
    };
    let actual = purrdf::xsd::parse_by_iri(lexical_form, datatype)
        .unwrap()
        .unwrap();
    let expected = purrdf::xsd::parse_by_iri(expected, "http://www.w3.org/2001/XMLSchema#decimal")
        .unwrap()
        .unwrap();
    assert!(
        purrdf::xsd::numeric_cmp(&actual, &expected)
            .unwrap()
            .is_eq()
    );
}

#[test]
fn finance_evaluation_derives_money_and_return_with_owned_quantity_typing_proof() {
    for (is_return, expected, class, frame_predicate) in [
        (false, "100", "MonetaryAmount", "currency"),
        (true, "0.2", "FinancialReturnRatio", "hasReferenceFrame"),
    ] {
        let evaluation = run(&input(is_return), &owner(), "synthetic-owner-v1").unwrap();
        magnitude(&evaluation, expected);
        assert!(evaluation.result.inferred().iter().any(|row| !row.is_edb
            && row.subject == "urn:opening"
            && row.predicate == TYPE
            && row.object == TermValue::iri(format!("{M}Quantity"))));
        assert!(evaluation.result.inferred().iter().any(|row| !row.is_edb
            && row.subject == evaluation.quantity
            && row.predicate == TYPE
            && row.object == TermValue::iri(format!("{G}{class}"))));
        assert!(evaluation.result.inferred().iter().any(|row| !row.is_edb
            && row.subject == evaluation.quantity
            && row.predicate == format!("{G}{frame_predicate}")));
        let proof = crate::result_rdf::project_reasoning_result(&evaluation.result).unwrap();
        let prefix = if is_return { "return" } else { "simple" };
        for suffix in [
            "list:0",
            "list:1",
            "out:list",
            "parameter:0",
            "parameter:1",
            "output",
        ] {
            assert!(
                proof.contains(&format!("urn:{prefix}:{suffix}")),
                "list source support {suffix}"
            );
        }
        for witness in [
            "urn:opening",
            "subClassOf",
            "parameterPredicate",
            "hasMathematicalSymbol",
            "calculationPeriodBasis",
            "observationResult",
        ] {
            assert!(
                proof.contains(witness),
                "missing native source witness {witness}"
            );
        }
        evaluation.result.validate_native_closure().unwrap();
    }
}

#[test]
fn finance_evaluation_refuses_missing_or_wrong_owned_quantity_axiom() {
    for invalid in [
        owner().replace("g:MonetaryAmount l:subClassOf m:Quantity .", ""),
        owner().replace(
            "g:MonetaryAmount l:subClassOf m:Quantity .",
            "g:MonetaryAmount l:subClassOf g:Entity .",
        ),
    ] {
        assert!(run(&input(false), &invalid, "synthetic-owner-v1").is_err());
    }
}

#[test]
fn finance_evaluation_refuses_conflicting_frames_roles_recipes_and_inexact_results() {
    let interest = input(false);
    let returns = input(true);
    for invalid in [
        interest.replace("g:interestRate <urn:rate>", "g:interestRate <urn:rate>, <urn:other>"),
        interest.replace("g:interestRate <urn:rate>", "g:interestRate <urn:rate> ; g:closingAmount <urn:closing>"),
        interest.replace("g:vantage <urn:alice>", "g:vantage <urn:alice>, <urn:bob>"),
        interest.replace("g:calculationPeriodBasis <urn:basis>", "g:calculationPeriodBasis <urn:basis> ; g:observationResult <urn:answer>"),
        interest.replace("m:hasDimension m:dimensionless", "m:hasDimension m:lengthDimension"),
        interest.replace("m:quantityValue 0.05", "m:quantityValue 5e-2"),
        interest.replace("g:hasReferenceFrame <urn:basis>", "g:hasReferenceFrame <urn:wrong-basis>"),
        returns.replace("m:quantityValue 1200.00 ; m:hasDimension m:dimensionless ; g:currency g:referenceFrameUSD", "m:quantityValue 1200.00 ; m:hasDimension m:dimensionless ; g:currency g:referenceFrameEUR"),
        returns.replace("1000.00", "0.00"),
        returns.replace("1000.00", "3.00").replace("1200.00", "4.00"),
        format!("{interest}\n<urn:opening> g:monetaryValue 1000.00 ."),
        format!("{interest}\n<urn:simple:recipe> m:operator m:Addition ."),
    ] { assert!(run(&invalid, &owner(), "synthetic-owner-v1").is_err(), "must reject {invalid}"); }
}

#[test]
fn finance_evaluation_identity_binds_owner_context_and_exact_input_values() {
    let original = run(&input(false), &owner(), "synthetic-owner-v1").unwrap();
    assert_eq!(
        original.quantity,
        "urn:gmeow:math-evaluation:quantity:f6fa5a0bcf7a15ad031d453b2b3e8fa5badb7a21860cd619db5ce6e066366978"
    );
    assert_eq!(
        original.quantity,
        run(&input(false), &owner(), "synthetic-owner-v1")
            .unwrap()
            .quantity
    );
    let changed_owner = run(&input(false), &owner(), "synthetic-owner-v2").unwrap();
    assert_ne!(original.quantity, changed_owner.quantity);
    assert_eq!(
        changed_owner.quantity,
        "urn:gmeow:math-evaluation:quantity:7f4811e2f16666c0e03f9e87d9df4b2ca4dae8398269263da0ed02438cd33569"
    );
    let changed_precision = run(
        &input(false).replace("1000.00", "1000.000"),
        &owner(),
        "synthetic-owner-v1",
    )
    .unwrap();
    assert_ne!(original.quantity, changed_precision.quantity);
    assert_eq!(
        changed_precision.quantity,
        "urn:gmeow:math-evaluation:quantity:cddb8223a8c451dda86db9fe6dc0aac52b45ac5522022dd381bbc56acb0fff06"
    );
    let changed_vantage = run(
        &input(false).replace("urn:alice", "urn:other-vantage"),
        &owner(),
        "synthetic-owner-v1",
    )
    .unwrap();
    assert_ne!(original.quantity, changed_vantage.quantity);
    assert_eq!(
        changed_vantage.quantity,
        "urn:gmeow:math-evaluation:quantity:753408bcffb1d8ac75c2f9edb49f3bcd742418bebbbdb41a03ac518fc8c099e6"
    );
}
