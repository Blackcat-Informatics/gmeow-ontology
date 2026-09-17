// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native effect admission and completion, independent of corpus production.

use super::*;
use crate::query_ir::{QBuiltin, QTerm};

const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const INSTANCE: &str = "https://blackcatinformatics.ca/logic/instanceOf";
const DIMENSIONLESS: &str = "https://blackcatinformatics.ca/math/Dimensionless";

fn typing(predicate: &str, class: &str) -> EvalRule {
    EvalRule::positive(
        "urn:typing",
        EvalAtom::positive(
            EvalTerm::named("urn:dimension"),
            predicate,
            EvalTerm::named(class),
        ),
        Vec::new(),
    )
}

fn dimension_read(mut rule: EvalRule) -> EvalRule {
    rule.builtins.push(QBuiltin::DimEqual {
        d1: QTerm::Const("<urn:dimension>".to_owned()),
        d2: QTerm::Const("<https://blackcatinformatics.ca/math/lengthDimension>".to_owned()),
    });
    rule
}

#[test]
fn completion_waits_for_the_last_writer_of_each_interpreted_predicate() {
    for predicate in [TYPE, INSTANCE] {
        let effects = [
            ProducerEffect::rule(&typing(predicate, DIMENSIONLESS)),
            ProducerEffect::rule(&dimension_read(typing(predicate, "urn:FailureClass"))),
        ];
        let result = schedule(
            &effects,
            SemanticVocabulary::GroundedLogicV1,
            &BTreeSet::new(),
        )
        .unwrap();
        assert_eq!(result.strata, [0, 1]);
        assert_eq!(result.completed[TYPE], 1);
        assert_eq!(result.completed[INSTANCE], 1);
    }
}

#[test]
fn dimension_class_feedback_refuses_both_operator_spellings() {
    for predicate in [TYPE, INSTANCE] {
        let effect = ProducerEffect::rule(&dimension_read(typing(predicate, DIMENSIONLESS)));
        assert!(
            schedule(
                &[effect],
                SemanticVocabulary::GroundedLogicV1,
                &BTreeSet::new()
            )
            .is_err()
        );
    }
    // An exact instanceOf writer cannot modify the builtin's exact RDF type input.
    let effect = ProducerEffect::rule(&dimension_read(typing(INSTANCE, DIMENSIONLESS)));
    assert!(schedule(&[effect], SemanticVocabulary::Exact, &BTreeSet::new()).is_ok());
}

#[test]
fn marker_aliases_apply_only_to_the_read_operator_role() {
    let canonical = "https://blackcatinformatics.ca/logic/Thing";
    let projected = "http://www.w3.org/2002/07/owl#Thing";
    let read = StatementPattern::atom(&typing(TYPE, canonical).head);
    let write = StatementPattern::atom(&typing(INSTANCE, projected).head);
    assert!(read.reads_write(&write, SemanticVocabulary::GroundedLogicV1));
    assert!(!read.reads_write(&write, SemanticVocabulary::Exact));
    let mut data_read = read.clone();
    let mut data_write = write.clone();
    data_read.predicate = Some("urn:data".to_owned());
    data_write.predicate = Some("urn:data".to_owned());
    assert!(!data_read.reads_write(&data_write, SemanticVocabulary::GroundedLogicV1));
    data_read.subject = data_read.object.take();
    data_write.subject = data_write.object.take();
    data_read.predicate = Some(TYPE.to_owned());
    data_write.predicate = Some(INSTANCE.to_owned());
    assert!(!data_read.reads_write(&data_write, SemanticVocabulary::GroundedLogicV1));
}

#[test]
fn native_constants_distinguish_blank_scope_literals_and_nested_statements() {
    let values = [
        TermValue::Blank {
            label: "x".to_owned(),
            scope: purrdf::BlankScope(1),
        },
        TermValue::Blank {
            label: "x".to_owned(),
            scope: purrdf::BlankScope(2),
        },
        TermValue::simple_literal("<urn:x>"),
        TermValue::iri("urn:x"),
    ];
    for (i, read) in values.iter().enumerate() {
        for (j, write) in values.iter().enumerate() {
            let pattern = |value: &TermValue| StatementPattern {
                subject: None,
                predicate: Some(TYPE.to_owned()),
                object: Some(value.clone()),
                ranges: None,
            };
            assert_eq!(
                pattern(read).reads_write(&pattern(write), SemanticVocabulary::GroundedLogicV1),
                i == j
            );
            let quote = |value: &TermValue| TermValue::Triple {
                s: Box::new(TermValue::iri("urn:claim")),
                p: Box::new(TermValue::iri(TYPE)),
                o: Box::new(value.clone()),
            };
            assert_eq!(
                pattern(&quote(read))
                    .reads_write(&pattern(&quote(write)), SemanticVocabulary::GroundedLogicV1),
                i == j
            );
        }
    }
}

#[test]
fn dynamic_writes_delay_known_completion_and_cannot_hide_strict_cycles() {
    let mut effect = ProducerEffect::rule(&typing(TYPE, "urn:FailureClass"));
    effect.reads.push((
        StatementPattern::relation(Some(TYPE), Some(DIMENSIONLESS)),
        ReadDependency::Completed,
    ));
    effect.writes[0].predicate = None;
    let result = schedule(
        &[effect],
        SemanticVocabulary::Exact,
        &BTreeSet::from(["urn:source".to_owned()]),
    )
    .unwrap();
    assert_eq!(result.dynamic_completion, Some(1));
    assert_eq!(result.completed["urn:source"], 1);
    let mut effect = ProducerEffect::rule(&dimension_read(typing(TYPE, DIMENSIONLESS)));
    effect.writes[0].predicate = None;
    assert!(schedule(&[effect], SemanticVocabulary::Exact, &BTreeSet::new()).is_err());
    // An arbitrary operator with even an unrelated class-valued object could
    // instead write a structural math cell; do not erase those completed reads.
    let mut effect = ProducerEffect::rule(&dimension_read(typing(TYPE, "urn:FailureClass")));
    effect.writes[0].predicate = None;
    assert!(schedule(&[effect], SemanticVocabulary::Exact, &BTreeSet::new()).is_err());
}

#[test]
fn completion_only_dynamic_receipts_do_not_claim_statement_writes() {
    let receipt = ProducerEffect::new("urn:admission".to_owned(), Vec::new(), Vec::new())
        .completes(vec![StatementPattern::relation(None, None)]);
    let reader = ProducerEffect::new(
        "urn:consumer".to_owned(),
        vec![StatementPattern::relation(Some("urn:result"), None)],
        vec![(
            StatementPattern::relation(Some(TYPE), None),
            ReadDependency::Completed,
        )],
    )
    .requiring_source_admission();
    let result = schedule(
        &[receipt, reader],
        SemanticVocabulary::Exact,
        &BTreeSet::from([TYPE.to_owned()]),
    )
    .unwrap();
    assert_eq!(result.strata, [0, 1]);
    assert_eq!(result.dynamic_completion, None);
    assert!(!result.completed.contains_key(TYPE));
    assert_eq!(result.completed["urn:result"], 1);
}

#[test]
fn ranged_grammar_observation_ignores_disjoint_dynamic_data_writer() {
    use super::value_flow::{FlowRule, ValueFlow};

    let selector = "urn:selector";
    let grammar = "urn:fixed-grammar";
    let body = [
        EvalTerm::var("owner"),
        EvalTerm::named(selector),
        EvalTerm::var("predicate"),
    ];
    let head = [
        EvalTerm::var("owner"),
        EvalTerm::var("predicate"),
        EvalTerm::named("urn:value"),
    ];
    let effect = ProducerEffect::new(
        "urn:dynamic-data-writer".to_owned(),
        vec![StatementPattern::statement(&head)],
        vec![(StatementPattern::statement(&body), ReadDependency::Positive)],
    );
    let flow = ValueFlow::with_observations(
        &[FlowRule {
            body: vec![body.clone()],
            heads: vec![head],
            native_witnesses: Vec::new(),
            reads: vec![Some(body)],
        }],
        std::slice::from_ref(&effect),
        SemanticVocabulary::Exact,
        &[StatementPattern::relation(Some(grammar), None)],
    );
    let input = [crate::rule_ir::Fact {
        subject: TermValue::iri("urn:owner"),
        predicate: selector.to_owned(),
        object: TermValue::iri("urn:data-predicate"),
    }];
    let effects = flow.refine(std::slice::from_ref(&effect), &flow.summarize(input.iter()));
    assert!(effects[0].writes[0].predicate.is_none());
    let observation = flow.ranged_pattern(StatementPattern::relation(Some(grammar), None));
    let result = schedule_observed(
        &effects,
        SemanticVocabulary::Exact,
        &BTreeSet::new(),
        &[observation],
    )
    .unwrap();
    assert_eq!(result.observed_completion, [None]);
}

#[test]
fn multi_head_producer_waits_as_one_effect_without_coupling_unrelated_writers() {
    let mut first = ProducerEffect::rule(&typing(TYPE, DIMENSIONLESS));
    first
        .writes
        .push(StatementPattern::relation(Some("urn:side-effect"), None));
    let last = ProducerEffect::rule(&dimension_read(typing(TYPE, "urn:FailureClass")));
    let result = schedule(&[first, last], SemanticVocabulary::Exact, &BTreeSet::new()).unwrap();
    assert_eq!(result.strata, [0, 1]);
    assert_eq!(result.completed["urn:side-effect"], 0);
    assert_eq!(result.completed[TYPE], 1);
}
