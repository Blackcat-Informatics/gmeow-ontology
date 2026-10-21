// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Independent tiny owner contracts exercise production wiring and proof;
//! authenticated repository owner acceptance is a separate producer-bound lane.

use super::*;

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
        for (i, (role, class)) in roles.iter().enumerate() {
            let next = if i + 1 == roles.len() {
                "rdf:nil".into()
            } else {
                format!("<urn:{prefix}:list:{}>", i + 1)
            };
            text.push_str(&format!("<urn:{prefix}:list:{i}> rdf:first <urn:{prefix}:parameter:{i}> ; rdf:rest {next} .\n<urn:{prefix}:parameter:{i}> a l:CallParameter ; l:parameterPredicate g:{role} ; l:parameterValueType g:{class} ; l:parameterRequired true ; m:hasMathematicalSymbol <urn:{prefix}:symbol:{i}> .\n<urn:{prefix}:symbol:{i}> a m:MathematicalSymbol .\n<urn:{prefix}:leaf:{i}> a m:SymbolReference ; m:hasMathematicalSymbol <urn:{prefix}:symbol:{i}> .\n"));
        }
        text.push_str(&format!("<urn:{prefix}:out:list> rdf:first <urn:{prefix}:output> ; rdf:rest rdf:nil .\n<urn:{prefix}:output> a l:CallResult ; l:parameterPredicate g:observationResult ; l:parameterValueType g:{output} .\n"));
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
fn finance_evaluation_requires_every_selected_procedural_product() {
    for identity in PROCEDURAL_PRODUCTS {
        let mut incomplete = shapes();
        incomplete.node_shapes.retain(|shape| {
            !matches!(&shape.id,
            purrdf::shapes::term::Term::NamedNode(iri) if iri.as_str() == *identity)
        });
        assert!(require_shapes(&incomplete).is_err(), "missing {identity}");
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
        run(&input(false), &owner(), "synthetic-owner-v1")
            .unwrap()
            .quantity
    );
    assert_ne!(
        original.quantity,
        run(&input(false), &owner(), "synthetic-owner-v2")
            .unwrap()
            .quantity
    );
    assert_ne!(
        original.quantity,
        run(
            &input(false).replace("1000.00", "1000.000"),
            &owner(),
            "synthetic-owner-v1"
        )
        .unwrap()
        .quantity
    );
    assert_ne!(
        original.quantity,
        run(
            &input(false).replace("urn:alice", "urn:other-vantage"),
            &owner(),
            "synthetic-owner-v1"
        )
        .unwrap()
        .quantity
    );
}
