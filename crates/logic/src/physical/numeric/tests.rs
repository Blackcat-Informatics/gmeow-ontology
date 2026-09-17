// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMEOW binding, provenance, world isolation and termination of authored arithmetic.

use super::*;
use crate::physical::NativeOutcome;
use crate::physical::seminaive::joint::JointProgram;
use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm, Fact};
use gmeow_logic_compile::ir::{Formula, LogicProgram, Term};

const DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
fn var(name: &str) -> RcTerm {
    RcTerm::Var(name.to_owned())
}
fn value(lexical: &str) -> RcTerm {
    RcTerm::Literal(RdfLiteral::typed(lexical, DECIMAL))
}
fn call(
    operator: NumericOperator,
    left: RcTerm,
    right: RcTerm,
    result: Option<RcTerm>,
) -> RcNumeric {
    RcNumeric {
        operator,
        left,
        right,
        result,
    }
}
fn literal(lexical: &str) -> TermValue {
    TermValue::Literal {
        lexical_form: lexical.to_owned(),
        datatype: DECIMAL.to_owned(),
        language: None,
        direction: None,
    }
}
fn solution(lexical: &str) -> Solution {
    Solution {
        bindings: vec![("?weight".into(), literal(lexical))],
        source_facts: vec![Fact {
            subject: TermValue::iri("urn:source"),
            predicate: "urn:weight".into(),
            object: literal(lexical),
        }],
    }
}

#[test]
fn numeric_plan_reuses_typed_results_and_keeps_source_support() {
    let plan = Plan::prepare(&[
        call(
            NumericOperator::Multiply,
            var("?weight"),
            value("0.5"),
            Some(var("?product")),
        ),
        call(
            NumericOperator::Add,
            var("?product"),
            value("0.1"),
            Some(var("?result")),
        ),
        call(NumericOperator::Greater, var("?result"), value("0.3"), None),
    ])
    .unwrap();
    let mut input = Vec::with_capacity(8);
    input.extend([solution("0.8"), solution("0.2")]);
    let allocation = input.as_ptr();
    let result = plan.apply_all("urn:axis-rule", input).unwrap();
    assert_eq!(result.as_ptr(), allocation);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].source_facts, solution("0.8").source_facts);
    let TermValue::Literal {
        lexical_form,
        datatype,
        ..
    } = result[0].get("?result").unwrap()
    else {
        panic!("numeric result")
    };
    assert!(
        xsd::numeric_cmp(
            &decode(lexical_form, datatype).unwrap(),
            &decode("0.5", DECIMAL).unwrap()
        )
        .unwrap()
        .is_eq()
    );
    let filter = Plan::prepare(&[call(
        NumericOperator::Add,
        value("0.3"),
        value("0.5"),
        Some(var("?weight")),
    )])
    .unwrap();
    assert!(
        filter
            .apply("urn:bound-result", &mut solution("0.8"))
            .unwrap()
    );
    assert!(
        !filter
            .apply("urn:bound-result", &mut solution("0.2"))
            .unwrap()
    );
}

#[test]
fn undefined_numeric_operation_refuses_the_whole_solution_batch() {
    let plan = Plan::prepare(&[call(
        NumericOperator::Divide,
        value("1"),
        var("?weight"),
        Some(var("?result")),
    )])
    .unwrap();
    let error = plan
        .apply_all("urn:division", vec![solution("0.5"), solution("0")])
        .err()
        .expect("undefined division refuses all candidates");
    assert!(error.message().contains("urn:division"));
    assert!(error.message().contains("incomplete"));
    let invalid = call(
        NumericOperator::Add,
        RcTerm::Literal(RdfLiteral::simple("1")),
        value("1"),
        Some(var("?result")),
    );
    assert!(
        Plan::prepare(&[invalid]).is_err(),
        "constant admission precedes any data join"
    );
}

fn atom(relation: &str, args: Vec<Term>) -> Formula {
    Formula::Atom {
        relation: Term::Iri(relation.into()),
        args,
    }
}
fn variable(name: &str) -> Term {
    Term::Var(name.into())
}
fn program(existential: bool) -> LogicProgram {
    let head = if existential {
        Formula::Exists {
            vars: vec!["record".into()],
            body: Box::new(Formula::And(vec![
                atom("urn:record", vec![variable("s"), variable("record")]),
                atom(
                    "urn:composed-weight",
                    vec![variable("record"), variable("result")],
                ),
            ])),
        }
    } else {
        atom(
            "urn:composed-weight",
            vec![variable("s"), variable("result")],
        )
    };
    LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![Formula::Implies(
        Box::new(Formula::And(vec![
            atom("urn:weight", vec![variable("s"), variable("w")]),
            atom(
                NumericOperator::Add.iri(),
                vec![
                    variable("w"),
                    Term::Literal(RdfLiteral::typed("0.4", DECIMAL)),
                    variable("result"),
                ],
            ),
        ])),
        Box::new(head),
    )])
}

#[test]
fn authored_numeric_formula_executes_in_both_native_rule_shapes() {
    for existential in [false, true] {
        let source = crate::store::WorldStore::new();
        for (world, weight) in [("urn:standpoint:a", "0.6"), ("urn:standpoint:b", "0.2")] {
            source
                .insert_quad_terms(
                    world,
                    TermValue::iri("urn:source"),
                    TermValue::iri("urn:weight"),
                    literal(weight),
                )
                .unwrap();
        }
        let lowered = crate::relational_core::lower_formulas(&program(existential));
        let NativeOutcome::Decided(plan) =
            JointProgram::prepare(&lowered.rules, &lowered.existential_rules).unwrap()
        else {
            panic!("admitted typed arithmetic")
        };
        let NativeOutcome::Decided(result) = plan.materialize(&source, None).unwrap() else {
            panic!("finite arithmetic producer")
        };
        assert_eq!(result.result.status, crate::seam::BudgetStatus::Ok);
        let weights: Vec<_> = result
            .result
            .rows
            .iter()
            .filter(|row| row.predicate == "urn:composed-weight")
            .collect();
        assert_eq!(weights.len(), 2, "one result per standpoint");
        for row in weights {
            let TermValue::Literal {
                lexical_form,
                datatype,
                ..
            } = &row.object
            else {
                panic!("computed value")
            };
            let expected = if row.graph == "urn:standpoint:a" {
                "1"
            } else {
                "0.6"
            };
            assert!(
                xsd::numeric_cmp(
                    &decode(lexical_form, datatype).unwrap(),
                    &decode(expected, DECIMAL).unwrap()
                )
                .unwrap()
                .is_eq()
            );
        }
        assert_eq!(
            result.witness_derivations.len(),
            if existential { 2 } else { 0 }
        );
    }
}

#[test]
fn recursive_numeric_invention_needs_a_position_certificate() {
    let mut rule = EvalRule::positive(
        "urn:recursive-numeric",
        EvalAtom::positive(EvalTerm::var("?s"), "urn:value", EvalTerm::var("?next")),
        vec![EvalAtom::positive(
            EvalTerm::var("?s"),
            "urn:value",
            EvalTerm::var("?value"),
        )],
    );
    rule.numeric = vec![call(
        NumericOperator::Add,
        var("?value"),
        value("1"),
        Some(var("?next")),
    )];
    let NativeOutcome::Decided(plan) = JointProgram::prepare(&[rule.clone()], &[]).unwrap() else {
        panic!("well-formed plan")
    };
    assert!(
        !plan.admission.admits_native(),
        "the numeric result depends on the old value even when that input is absent from the head"
    );
    let source = crate::store::WorldStore::new();
    source
        .insert_quad_terms(
            "urn:world",
            TermValue::iri("urn:s"),
            TermValue::iri("urn:value"),
            literal("1"),
        )
        .unwrap();
    assert!(
        matches!(
            plan.materialize(&source, None).unwrap(),
            NativeOutcome::Unsupported(_)
        ),
        "no bounded observations authorize unbounded numeric recursion"
    );
    let first = crate::physical::plan::canonical_rule_hash(std::slice::from_ref(&rule));
    rule.numeric[0].operator = NumericOperator::Multiply;
    assert_ne!(first, crate::physical::plan::canonical_rule_hash(&[rule]));
}

#[test]
fn public_scalar_materialization_keeps_certification_and_annotation() {
    use crate::annotation::{AnnotationContract, AnnotationFactRef, AnnotationRequest};
    use crate::materialize::{
        MaterializationLimits, materialize_program, materialize_program_annotated,
    };
    use crate::provenance::ZWeightSemiring;
    use gmeow_logic_compile::ir::SemanticProfileId;
    let mut builder = purrdf::RdfDatasetBuilder::new();
    let subject = builder.intern_iri("urn:source");
    let predicate = builder.intern_iri("urn:weight");
    let object = builder.intern_literal(RdfLiteral::typed("0.6", DECIMAL));
    let world = builder.intern_iri("urn:standpoint:a");
    builder.push_quad(subject, predicate, object, Some(world));
    let input = builder.freeze().unwrap();
    let source = program(false);
    let materialized = materialize_program(
        &source,
        &input,
        MaterializationLimits::default(),
        Some(SemanticProfileId::PositiveHorn),
    )
    .unwrap();
    assert!(
        materialized.chase_admission.is_some(),
        "scalar-only production retains its termination proof"
    );
    assert_eq!(
        materialized
            .quads
            .iter()
            .filter(|row| row.predicate == "urn:composed-weight")
            .count(),
        1
    );
    let annotated = materialize_program_annotated(
        &source,
        &input,
        MaterializationLimits::default(),
        Some(SemanticProfileId::PositiveHorn),
        AnnotationRequest::new(
            &ZWeightSemiring,
            &AnnotationContract::exact(),
            |fact: AnnotationFactRef<'_>| (fact.predicate == "urn:weight").then_some(3),
        ),
    )
    .unwrap();
    let row = annotated
        .quads
        .iter()
        .find(|row| row.quad.predicate == "urn:composed-weight")
        .unwrap();
    assert_eq!(row.annotation, 3);
    assert!(row.derivations.iter().any(|derivation| {
        derivation
            .sources
            .iter()
            .any(|fact| fact.predicate == "urn:weight")
    }));
    assert!(annotated.materialization.chase_admission.is_some());

    let mut recursive = source;
    let Formula::Implies(_, head) = &mut recursive.formulas[0] else {
        panic!("formula rule")
    };
    let Formula::Atom { relation, .. } = head.as_mut() else {
        panic!("single head")
    };
    *relation = Term::Iri("urn:weight".into());
    let prepared = crate::program_analysis::prepare_program(&recursive).unwrap();
    let NativeOutcome::Decided(check) = prepared.joint_program().unwrap() else {
        panic!("well-formed numeric plan")
    };
    assert!(
        !check.admission.admits_native(),
        "check admission before an unbudgeted call"
    );
    let mut facts = std::collections::BTreeMap::new();
    facts.insert("urn:standpoint:a".to_owned(), solution("0.6").source_facts);
    let selected = prepared
        .reasoning_input(
            &[],
            &crate::reason::source_existentials::Collector::default()
                .prepare()
                .unwrap(),
            &crate::physical::SelectedDomains::new([]).unwrap(),
            &facts,
            std::sync::Arc::from([]),
            &[],
        )
        .unwrap();
    let NativeOutcome::Decided(specialized) = prepared.reasoning_schema_program(&selected).unwrap()
    else {
        panic!("well-formed specialized plan")
    };
    assert!(
        !specialized.admission.admits_native(),
        "immutable source specialization cannot discard numeric input dependencies"
    );
    assert!(
        materialize_program(
            &recursive,
            &input,
            MaterializationLimits::default(),
            Some(SemanticProfileId::PositiveHorn)
        )
        .is_err(),
        "the public scalar-only route must refuse uncertified unbounded recursion"
    );
}

#[test]
fn invalid_numeric_instructions_fail_before_empty_world_execution() {
    let mut rule = EvalRule::positive(
        "urn:invalid-numeric",
        EvalAtom::positive(EvalTerm::var("?s"), "urn:result", EvalTerm::var("?result")),
        vec![EvalAtom::positive(
            EvalTerm::var("?s"),
            "urn:value",
            EvalTerm::var("?value"),
        )],
    );
    rule.numeric = vec![call(
        NumericOperator::Add,
        var("?missing"),
        value("1"),
        Some(var("?result")),
    )];
    assert!(JointProgram::prepare(&[rule.clone()], &[]).is_err());
    rule.numeric[0].left = RcTerm::Literal(RdfLiteral::simple("1"));
    assert!(JointProgram::prepare(&[rule], &[]).is_err());
}
