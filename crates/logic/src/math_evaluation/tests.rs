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

fn reference_prepare(
    references: &mut std::str::Lines<'_>,
    dataset: &RdfDataset,
    root: &str,
    bindings: &BTreeMap<String, String>,
    context: &EvaluationContext,
) -> gmeow_errors::Result<PreparedEvaluation> {
    use purrdf_lex::json::record::FromJson;

    assert_eq!(references.next(), Some("BEGIN_MATH_RESULT_REFERENCE"));
    let preimage = references
        .next()
        .expect("each original packet contains its complete preimage")
        .strip_prefix("MATH_RESULT_PREIMAGE=")
        .expect("the original packet labels the preimage");
    let quantity = references
        .next()
        .expect("each original packet contains the actual quantity identity")
        .strip_prefix("MATH_RESULT_QUANTITY=")
        .expect("the original packet labels the quantity identity");
    let program = references
        .next()
        .expect("each original packet contains the actual program identity")
        .strip_prefix("MATH_RESULT_PROGRAM=")
        .expect("the original packet labels the program identity");
    assert_eq!(references.next(), Some("END_MATH_RESULT_REFERENCE"));

    let decoded =
        purrdf_lex::json::read(preimage).expect("the complete original identity preimage is JSON");
    let fields = decoded
        .as_array()
        .expect("an identity preimage is a tuple array");
    assert!(matches!(fields.len(), 5 | 8));
    assert_eq!(
        String::from_json(&fields[0]).expect("the profile is text"),
        PROFILE
    );
    let decoded_root = String::from_json(&fields[1]).expect("the selected root is text");
    let decoded_bindings = BTreeMap::<String, String>::from_json(&fields[2])
        .expect("the original bindings contain complete string pairs");
    let premises = Vec::<Formula>::from_json(&fields[3])
        .expect("the original lowering premises are complete formulas");
    let calls = Vec::<Formula>::from_json(&fields[4])
        .expect("the original arithmetic calls are complete formulas");
    let decoded_context = if fields.len() == 8 {
        EvaluationContext {
            identity: String::from_json(&fields[5]).expect("the caller identity is text"),
            premises: Vec::<Formula>::from_json(&fields[6])
                .expect("the caller premises retain complete formula and literal metadata"),
            quantity_classes: BTreeSet::<String>::from_json(&fields[7])
                .expect("the caller quantity classes form an ordered string set"),
        }
    } else {
        EvaluationContext::default()
    };
    assert_eq!(
        encode_identity(
            &decoded_root,
            &decoded_bindings,
            &premises,
            &calls,
            &decoded_context
        )
        .expect("the actual native identity encoder accepts the original typed fields"),
        preimage.as_bytes(),
        "every original compact preimage remains byte-identical"
    );
    let prepared = prepare(dataset, root, bindings, context)?;
    assert_eq!(
        prepared.quantity, quantity,
        "actual preparation retains the original quantity identity"
    );
    assert_eq!(
        prepared.program.source_iri.as_deref(),
        Some(program),
        "actual preparation retains the original program identity"
    );
    Ok(prepared)
}

fn reference_evaluate(
    references: &mut std::str::Lines<'_>,
    dataset: &RdfDataset,
    bindings: &BTreeMap<String, String>,
) -> gmeow_errors::Result<Evaluation> {
    execute_prepared(
        dataset,
        reference_prepare(
            references,
            dataset,
            "urn:root",
            bindings,
            &EvaluationContext::default(),
        )?,
    )
}

fn reference_run(
    references: &mut std::str::Lines<'_>,
    text: &str,
) -> gmeow_errors::Result<Evaluation> {
    // gmeow-test-input: synthetic-only
    let dataset = purrdf::parse_dataset(text.as_bytes(), "text/turtle", None)
        .expect("the independent arithmetic identity fixture is valid Turtle");
    reference_evaluate(references, &dataset, &bindings())
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
    let mut references = include_str!("fixtures/result-identities-v1.txt").lines();
    let text = source("Addition", "1", "2");
    let original = reference_run(&mut references, &text).unwrap();
    assert_eq!(
        original.quantity,
        reference_run(&mut references, &text).unwrap().quantity
    );
    // Statement order cannot alter the selected operation identity.
    let reordered = text.replace(
        "math:argumentSlot <urn:slot0>, <urn:slot1>",
        "math:argumentSlot <urn:slot1>, <urn:slot0>",
    );
    assert_eq!(
        original.quantity,
        reference_run(&mut references, &reordered).unwrap().quantity
    );
    for changed in [
        source("Multiplication", "1", "2"),
        source("Addition", "1.0", "2"),
        source("Addition", "2", "2"),
    ] {
        assert_ne!(
            original.quantity,
            reference_run(&mut references, &changed).unwrap().quantity
        );
    }
    let dataset = purrdf::parse_dataset(text.as_bytes(), "text/turtle", None).unwrap();
    let mut swapped = bindings();
    swapped.insert("urn:symbol:left".into(), "urn:quantity:right".into());
    swapped.insert("urn:symbol:right".into(), "urn:quantity:left".into());
    assert_ne!(
        original.quantity,
        reference_evaluate(&mut references, &dataset, &swapped)
            .unwrap()
            .quantity
    );

    let context_identity = "authored caller\0\n雪😀";
    let context_premises = vec![Formula::Atom {
        relation: named("urn:authored:label"),
        args: vec![
            named("urn:quantity:left"),
            Term::Literal(RdfLiteral {
                lexical_form: "context \"label\"\n雪".into(),
                datatype: Some("http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString".into()),
                language: Some("ar".into()),
                direction: Some(purrdf::RdfTextDirection::Rtl),
            }),
        ],
    }];
    let context_classes = BTreeSet::from([
        "urn:authored:QuantitySubclassZ".to_owned(),
        "urn:authored:QuantitySubclassA".to_owned(),
    ]);
    let mut contextual_quantities = BTreeSet::new();
    for (label, context) in [
        ("standalone", EvaluationContext::default()),
        (
            "identity-only",
            EvaluationContext {
                identity: context_identity.into(),
                ..EvaluationContext::default()
            },
        ),
        (
            "premises-only",
            EvaluationContext {
                premises: context_premises.clone(),
                ..EvaluationContext::default()
            },
        ),
        (
            "classes-only",
            EvaluationContext {
                quantity_classes: context_classes.clone(),
                ..EvaluationContext::default()
            },
        ),
        (
            "complete-context",
            EvaluationContext {
                identity: context_identity.into(),
                premises: context_premises,
                quantity_classes: context_classes,
            },
        ),
    ] {
        assert_eq!(
            references.next(),
            Some(format!("MATH_RESULT_CONTEXT_LABEL={label}").as_str())
        );
        let prepared =
            reference_prepare(&mut references, &dataset, "urn:root", &bindings(), &context).expect(
                "the independently authored identity sample prepares without executing proof",
            );
        if label == "standalone" {
            assert_eq!(prepared.quantity, original.quantity);
        } else {
            assert_ne!(prepared.quantity, original.quantity, "{label}");
        }
        assert!(
            contextual_quantities.insert(prepared.quantity),
            "every selected context has a distinct preparation identity: {label}"
        );
    }
    assert!(
        references.next().is_none(),
        "all twelve complete original packets were consumed"
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
