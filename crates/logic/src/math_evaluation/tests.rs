// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn source(operator: &str, left: &str, right: &str) -> String {
    format!(
        r#"
@prefix math: <https://blackcatinformatics.ca/math/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
<urn:root> a math:ApplicationExpression ; math:operator math:{operator} ;
math:argumentSlot <urn:slot0>, <urn:slot1> .
<urn:slot0> a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression <urn:left> .
<urn:slot1> a math:ArgumentSlot ; math:slotIndex 1 ; math:slotExpression <urn:right> .
<urn:left> a math:SymbolReference ; math:hasMathematicalSymbol <urn:symbol:left> .
<urn:right> a math:SymbolReference ; math:hasMathematicalSymbol <urn:symbol:right> .
<urn:symbol:left> a math:MathematicalSymbol .
<urn:symbol:right> a math:MathematicalSymbol .
<urn:quantity:left> a math:Quantity ; math:hasDimension math:dimensionless ; math:quantityValue {left} .
<urn:quantity:right> a math:Quantity ; math:hasDimension math:dimensionless ; math:quantityValue {right} .
"#
    )
}
fn bindings() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("urn:symbol:left".into(), "urn:quantity:left".into()),
        ("urn:symbol:right".into(), "urn:quantity:right".into()),
    ])
}
fn run(text: &str) -> gmeow_errors::Result<Evaluation> {
    // gmeow-test-input: synthetic-only
    let dataset = purrdf::parse_dataset(text.as_bytes(), "text/turtle", None).unwrap();
    evaluate(&dataset, "urn:root", &bindings())
}
fn value(evaluation: &Evaluation) -> String {
    let row = evaluation
        .result
        .inferred()
        .iter()
        .find(|row| {
            !row.is_edb
                && row.subject == evaluation.quantity
                && row.predicate == iri("quantityValue")
        })
        .unwrap();
    let purrdf::TermValue::Literal { lexical_form, .. } = &row.object else {
        panic!("numeric result")
    };
    lexical_form.clone()
}

#[test]
fn math_evaluation_executes_four_operations_with_native_source_proofs() {
    for (operator, left, right, expected) in [
        ("Addition", "0.1", "0.2", "0.3"),
        ("Subtraction", "0.3", "0.1", "0.2"),
        ("Multiplication", "0.2", "0.5", "0.1"),
        ("Division", "7", "2", "3.5"),
        (
            "Addition",
            "170141183460469231731687303715884105728",
            "1",
            "170141183460469231731687303715884105729",
        ),
    ] {
        let evaluation = run(&source(operator, left, right)).unwrap();
        assert_eq!(value(&evaluation), expected);
        evaluation.result.validate_native_closure().unwrap();
        let projection = crate::result_rdf::project_reasoning_result(&evaluation.result).unwrap();
        assert!(projection.contains("resultDerivedAxiom"));
        assert!(
            projection.contains("urn:slot0"),
            "AST source support belongs in the normal proof"
        );
        assert!(
            projection.contains("urn:quantity:left"),
            "bound source quantity belongs in the normal proof"
        );
        assert!(projection.contains(&evaluation.quantity));
        assert!(
            !evaluation
                .result
                .inferred()
                .iter()
                .any(|row| !row.is_edb && row.predicate.ends_with("currency"))
        );
    }
}

#[test]
fn math_evaluation_identity_retains_bindings_ast_values_and_literal_metadata() {
    let text = source("Addition", "1", "2");
    let original = run(&text).unwrap();
    assert_eq!(
        original.quantity,
        "urn:gmeow:math-evaluation:quantity:f6c1a63288008cbfe41d592b6708b251efb159a71fa7a45613fdf45254174680"
    );
    assert_eq!(original.quantity, run(&text).unwrap().quantity);
    // Statement order cannot alter the selected operation identity.
    let reordered = text.replace(
        "math:argumentSlot <urn:slot0>, <urn:slot1>",
        "math:argumentSlot <urn:slot1>, <urn:slot0>",
    );
    assert_eq!(original.quantity, run(&reordered).unwrap().quantity);
    for (changed, expected) in [
        (
            source("Multiplication", "1", "2"),
            "urn:gmeow:math-evaluation:quantity:5071ffd974f5ec9c96e4fb1fa9180213a2d523b5169853202c30004ae9fa8c82",
        ),
        (
            source("Addition", "1.0", "2"),
            "urn:gmeow:math-evaluation:quantity:5f5483c973cdbdc92913ea68002390df7d7ea23e861f035d01f464aaa9981dfe",
        ),
        (
            source("Addition", "2", "2"),
            "urn:gmeow:math-evaluation:quantity:2bfaeeaa2d3860845a6c78152312a3fcc2f45a0c4ebaa7e2349ed8304dff9fbe",
        ),
    ] {
        let evaluation = run(&changed).unwrap();
        assert_ne!(original.quantity, evaluation.quantity);
        assert_eq!(evaluation.quantity, expected);
    }
    let dataset = purrdf::parse_dataset(text.as_bytes(), "text/turtle", None).unwrap();
    let mut swapped = bindings();
    swapped.insert("urn:symbol:left".into(), "urn:quantity:right".into());
    swapped.insert("urn:symbol:right".into(), "urn:quantity:left".into());
    let evaluation = evaluate(&dataset, "urn:root", &swapped).unwrap();
    assert_ne!(original.quantity, evaluation.quantity);
    assert_eq!(
        evaluation.quantity,
        "urn:gmeow:math-evaluation:quantity:962f550b011a54de208e210e29e8e21344cfe136ed1504c8798a7634f7d2df8e"
    );
}

#[test]
fn math_evaluation_rejects_malformed_or_unsupported_selected_operations() {
    let valid = source("Division", "1", "2");
    for invalid in [
        source("Division", "1", "0"),
        source("Division", "1", "3"),
        source("Exponentiation", "2", "3"),
        source("Addition", "1e0", "2"),
        valid.replace("math:slotIndex 0", "math:slotIndex \"0\"^^xsd:string"),
        valid.replace("math:slotIndex 0", "math:slotIndex \"0\"^^xsd:decimal"),
        valid.replace("math:slotIndex 1", "math:slotIndex 0"),
        valid.replace("math:slotIndex 1", "math:slotIndex 2"),
        valid.replace("math:quantityValue 1", "math:quantityValue 1, 2"),
        valid.replace(
            "math:hasDimension math:dimensionless",
            "math:hasDimension math:lengthDimension",
        ),
        valid.replace(
            "math:quantityValue 1",
            "math:quantityValue \"1\"^^xsd:string",
        ),
        valid.replace(
            "math:slotExpression <urn:left>",
            "math:slotExpression <urn:root>",
        ),
    ] {
        assert!(run(&invalid).is_err(), "must refuse: {invalid}");
    }
    let dataset = purrdf::parse_dataset(valid.as_bytes(), "text/turtle", None).unwrap();
    assert!(evaluate(&dataset, "urn:root", &BTreeMap::new()).is_err());
    let mut extra = bindings();
    extra.insert("urn:unused".into(), "urn:quantity:left".into());
    assert!(evaluate(&dataset, "urn:root", &extra).is_err());
}

#[test]
fn math_evaluation_never_reuses_an_asserted_result_identity() {
    let text = source("Addition", "1", "2");
    let evaluation = run(&text).unwrap();
    let with_asserted_result = format!(
        "{text}\n<{}> <urn:note> <urn:already-present> .",
        evaluation.quantity
    );
    assert!(run(&with_asserted_result).is_err());
}

#[test]
fn math_evaluation_rejects_conflicting_concrete_ast_forms_but_keeps_base_types() {
    let valid = source("Addition", "1", "2");
    // With only the left binding, the previous symbol-first evaluator accepted
    // this root despite the structural owner reading its application branch.
    let ambiguous = format!(
        "{valid}\n<urn:root> a math:SymbolReference ; math:hasMathematicalSymbol <urn:symbol:left> ."
    );
    let dataset = purrdf::parse_dataset(ambiguous.as_bytes(), "text/turtle", None).unwrap();
    let only_left = BTreeMap::from([("urn:symbol:left".into(), "urn:quantity:left".into())]);
    let error = evaluate(&dataset, "urn:root", &only_left)
        .err()
        .expect("ambiguous node must be rejected before branch selection");
    assert!(error.message().contains("unambiguous concrete AST form"));
    for kind in ["BindingExpression", "VariableExpression", "NumberLiteral"] {
        let mixed = format!("{valid}\n<urn:root> a math:{kind} .");
        let error = run(&mixed)
            .err()
            .expect("supported and unsupported concrete forms conflict");
        assert!(error.message().contains("unambiguous concrete AST form"));
    }
    let with_bases = format!(
        "{valid}\n<urn:root> a math:MathematicalExpression .\n<urn:left> a math:MathematicalExpression ."
    );
    assert_eq!(value(&run(&with_bases).unwrap()), "3");
}

#[test]
fn math_evaluation_keeps_valid_native_subclass_consequences_and_their_proof() {
    let text = format!(
        "{}\nmath:Quantity <http://www.w3.org/2000/01/rdf-schema#subClassOf> <urn:ExtraQuantityType> .",
        source("Addition", "1", "2")
    );
    let evaluation = run(&text).unwrap();
    assert_eq!(value(&evaluation), "3");
    assert!(evaluation.result.inferred().iter().any(|row| {
        !row.is_edb
            && row.subject == evaluation.quantity
            && row.predicate == TYPE
            && row.object == purrdf::TermValue::iri("urn:ExtraQuantityType")
    }));
    assert!(
        evaluation
            .result
            .inferred()
            .iter()
            .filter(|row| !row.is_edb && row.subject == evaluation.quantity)
            .count()
            > 3
    );
    let proof = crate::result_rdf::project_reasoning_result(&evaluation.result).unwrap();
    assert!(proof.contains("urn:ExtraQuantityType"));
    assert!(proof.contains("subClassOf"));
    evaluation.result.validate_native_closure().unwrap();
}
