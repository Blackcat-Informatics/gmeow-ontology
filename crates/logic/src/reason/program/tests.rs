// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Selected program reasoning over synthetic native source statements.

use super::*;
use crate::physical::SelectedDomains;
use crate::reason::{reason_program, reason_program_budgeted};
use gmeow_logic_compile::ir::{Formula, LogicProgram, Term};
use purrdf::{RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};

const EDGE: &str = "urn:program:edge";
const READY: &str = "urn:program:ready";
const DONE: &str = "urn:program:done";
const TUPLE: &str = "urn:program:tuple";
const X: &str = "urn:program:x";

fn atom(predicate: &str, args: &[&str]) -> Formula {
    Formula::atom(
        Term::iri(predicate).unwrap(),
        args.iter().map(|name| Term::var(*name).unwrap()).collect(),
    )
    .unwrap()
}

fn law(body: Formula, head: Formula) -> Formula {
    Formula::Forall {
        vars: vec!["x".to_owned(), "y".to_owned()],
        body: Box::new(Formula::Implies(Box::new(body), Box::new(head))),
    }
}

fn program() -> LogicProgram {
    LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![
        law(atom(EDGE, &["x", "y"]), atom(READY, &["x", "y"])),
        law(atom(READY, &["x", "y"]), atom(TUPLE, &["x", "y", "x"])),
        law(atom(TUPLE, &["x", "y", "x"]), atom(DONE, &["x", "y"])),
    ])
}

fn input() -> std::sync::Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    builder.push_owned_quad(&RdfQuad::new(
        RdfTerm::iri(X),
        EDGE,
        RdfTerm::Literal(RdfLiteral::simple("a literal value")),
    ));
    builder.freeze().unwrap()
}

#[test]
fn default_graph_literal_flows_through_joint_formula_heads_with_real_premises() {
    let result = reason_program(
        &program(),
        prepare_reasoning_input(&input()).unwrap(),
        &SelectedDomains::new([]).unwrap(),
    )
    .unwrap();
    let done = result
        .inferred()
        .iter()
        .find(|row| row.predicate == DONE)
        .unwrap();
    assert_eq!(done.subject, X);
    assert_eq!(
        done.object,
        purrdf::TermValue::simple_literal("a literal value")
    );
    assert_eq!(done.world, crate::reason::rl::DEFAULT_WORLD);
    assert_eq!(
        done.premises.len(),
        4,
        "the nary body has four actual tuple premises"
    );
    assert!(
        result
            .native_execution()
            .unwrap()
            .chase_certificates
            .iter()
            .any(|certificate| certificate.world == done.world)
    );
}

#[test]
fn native_annotations_keep_literal_facets_through_formula_feedback() {
    let mut builder = RdfDatasetBuilder::new();
    let reifier = builder.intern_iri(X);
    let subject = builder.intern_iri("urn:statement:subject");
    let predicate = builder.intern_iri("urn:statement:predicate");
    let object = builder.intern_iri("urn:statement:object");
    let triple = builder.intern_triple(subject, predicate, object);
    builder.push_reifier(reifier, triple);
    let edge = builder.intern_iri(EDGE);
    for literal in [
        RdfLiteral::language_tagged("value", "ar"),
        RdfLiteral {
            lexical_form: "value".into(),
            datatype: None,
            language: Some("ar".into()),
            direction: Some(purrdf::RdfTextDirection::Rtl),
        },
    ] {
        let value = builder.intern_literal(literal);
        builder.push_annotation(reifier, edge, value);
    }
    let result = reason_program(
        &program(),
        prepare_reasoning_input(&builder.freeze().unwrap()).unwrap(),
        &SelectedDomains::new([]).unwrap(),
    )
    .unwrap();
    let done = result
        .inferred()
        .iter()
        .filter(|row| row.predicate == DONE)
        .map(|row| crate::provenance::term_display(&row.object))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        done,
        std::collections::BTreeSet::from([
            "\"value\"@ar".to_owned(),
            "\"value\"@ar--rtl".to_owned()
        ])
    );
}

#[test]
fn formula_producer_and_consumer_share_one_budget() {
    let (full, status, total) = reason_program_budgeted(
        &program(),
        prepare_reasoning_input(&input()).unwrap(),
        &SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(status, crate::seam::BudgetStatus::Ok);
    assert!(full.inferred().iter().any(|row| row.predicate == DONE));
    assert_eq!(total, 6);
    let (partial, status, consumed) = reason_program_budgeted(
        &program(),
        prepare_reasoning_input(&input()).unwrap(),
        &SelectedDomains::new([]).unwrap(),
        Some(3),
    )
    .unwrap();
    assert_eq!(status, crate::seam::BudgetStatus::Exhausted);
    assert_eq!(consumed, 3);
    assert!(!partial.inferred().iter().any(|row| row.predicate == DONE));
    assert!(
        !partial
            .native_execution()
            .unwrap()
            .chase_certificates
            .is_empty(),
        "a partial run retains its admission"
    );
}

#[test]
fn equal_subjects_in_distinct_worlds_do_not_share_rule_premises() {
    let mut builder = RdfDatasetBuilder::new();
    builder.push_owned_quad(
        &RdfQuad::new(RdfTerm::iri(X), EDGE, RdfTerm::iri("urn:first"))
            .in_graph(RdfTerm::iri("urn:world:first")),
    );
    builder.push_owned_quad(
        &RdfQuad::new(RdfTerm::iri(X), "urn:unrelated", RdfTerm::iri("urn:second"))
            .in_graph(RdfTerm::iri("urn:world:second")),
    );
    let result = reason_program(
        &program(),
        prepare_reasoning_input(&builder.freeze().unwrap()).unwrap(),
        &SelectedDomains::new([]).unwrap(),
    )
    .unwrap();
    let done: Vec<_> = result
        .inferred()
        .iter()
        .filter(|row| row.predicate == DONE)
        .collect();
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].world, "urn:world:first");
}

#[test]
fn graph_key_collision_is_rejected_instead_of_merging_worlds() {
    let mut builder = RdfDatasetBuilder::new();
    builder.push_owned_quad(
        &RdfQuad::new(RdfTerm::iri(X), EDGE, RdfTerm::iri("urn:y"))
            .in_graph(RdfTerm::iri(crate::reason::rl::DEFAULT_WORLD)),
    );
    let error = input_facts(&builder.freeze().unwrap()).unwrap_err();
    assert!(error.message().contains("distinct source graphs"));
}

// The schema rule is authored as an ordinary formula. Its newly derived domain
// must fire a native property join before the consuming formula is evaluated.
fn domain_program() -> LogicProgram {
    let domain = Formula::atom(
        Term::iri("http://www.w3.org/2000/01/rdf-schema#domain").unwrap(),
        vec![
            Term::iri(EDGE).unwrap(),
            Term::iri("urn:program:class").unwrap(),
        ],
    )
    .unwrap();
    LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![
        law(atom(EDGE, &["x", "y"]), domain),
        law(
            atom(
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                &["x", "y"],
            ),
            atom(DONE, &["x", "y"]),
        ),
    ])
}

#[test]
fn derived_schema_and_authored_consumers_share_one_fixed_point() {
    let input = input();
    let prepared = crate::program_analysis::prepare_program(&domain_program()).unwrap();
    let closure = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&input).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(closure.status, crate::seam::BudgetStatus::Ok);
    let class = closure
        .inferred
        .iter()
        .find(|row| {
            row.predicate == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type" && row.subject == X
        })
        .unwrap();
    assert_eq!(class.rule_name.as_deref(), Some("dl:domain"));
    assert_eq!(
        class.premises,
        vec![
            (
                EDGE.to_owned(),
                "http://www.w3.org/2000/01/rdf-schema#domain".to_owned(),
                "<urn:program:class>".to_owned()
            ),
            (
                X.to_owned(),
                EDGE.to_owned(),
                "\"a literal value\"".to_owned()
            ),
        ]
    );
    assert!(closure.inferred.iter().any(|row| row.subject == X
        && row.predicate == DONE
        && crate::provenance::term_display(&row.object) == "<urn:program:class>"));
    assert_eq!(closure.frontier.consumed_steps, 3);
    assert!(
        !closure.certificates.is_empty(),
        "the native schema fragment retains its admission"
    );
}

#[test]
fn shared_schema_budget_cuts_before_the_authored_consumer() {
    let prepared = crate::program_analysis::prepare_program(&domain_program()).unwrap();
    let closure = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&input()).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        Some(2),
    )
    .unwrap();
    assert_eq!(closure.status, crate::seam::BudgetStatus::Exhausted);
    assert_eq!(closure.frontier.consumed_steps, 2);
    assert!(
        closure
            .inferred
            .iter()
            .any(|row| row.rule_name.as_deref() == Some("dl:domain"))
    );
    assert!(!closure.inferred.iter().any(|row| row.predicate == DONE));
}

#[test]
fn native_has_value_preserves_literal_facet_and_wakes_authored_rules() {
    let mut builder = RdfDatasetBuilder::new();
    for quad in [
        RdfQuad::new(
            RdfTerm::iri("urn:restriction"),
            "http://www.w3.org/2002/07/owl#onProperty",
            RdfTerm::iri(EDGE),
        ),
        RdfQuad::new(
            RdfTerm::iri("urn:restriction"),
            "http://www.w3.org/2002/07/owl#hasValue",
            RdfTerm::literal(RdfLiteral::language_tagged("valeur", "fr")),
        ),
        RdfQuad::new(
            RdfTerm::iri(X),
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            RdfTerm::iri("urn:restriction"),
        ),
    ] {
        builder.push_owned_quad(&quad);
    }
    let program = LogicProgram::new(vec![], vec![], vec![], None)
        .with_formulas(vec![law(atom(EDGE, &["x", "y"]), atom(DONE, &["x", "y"]))]);
    let prepared = crate::program_analysis::prepare_program(&program).unwrap();
    let closure = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&builder.freeze().unwrap()).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(closure.status, crate::seam::BudgetStatus::Ok);
    let done = closure
        .inferred
        .iter()
        .find(|row| row.predicate == DONE)
        .unwrap();
    assert_eq!(done.object, purrdf::TermValue::lang_literal("valeur", "fr"));
    let edge = closure
        .inferred
        .iter()
        .find(|row| row.predicate == EDGE)
        .unwrap();
    assert_eq!(edge.rule_name.as_deref(), Some("dl:hasValue-assertion"));
    assert_eq!(
        edge.premises.len(),
        3,
        "restriction, selected value and membership all support the consequence"
    );
}

#[test]
fn schema_propagation_keeps_worlds_separate() {
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![law(
        atom(
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            &["x", "y"],
        ),
        atom(DONE, &["x", "y"]),
    )]);
    let mut builder = RdfDatasetBuilder::new();
    builder.push_owned_quad(
        &RdfQuad::new(
            RdfTerm::iri(EDGE),
            "http://www.w3.org/2000/01/rdf-schema#domain",
            RdfTerm::iri("urn:class"),
        )
        .in_graph(RdfTerm::iri("urn:schema-world")),
    );
    builder.push_owned_quad(
        &RdfQuad::new(RdfTerm::iri(X), EDGE, RdfTerm::iri("urn:y"))
            .in_graph(RdfTerm::iri("urn:data-world")),
    );
    let prepared = crate::program_analysis::prepare_program(&program).unwrap();
    let closure = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&builder.freeze().unwrap()).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    assert!(!closure.inferred.iter().any(|row| row.predicate == DONE));
}

#[test]
fn schema_dependency_cycle_through_negation_is_not_admitted_as_stratified() {
    use crate::physical::{JointProgram, NativeOutcome};
    use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm};
    let mut absent_type = EvalAtom::positive(
        EvalTerm::var("?x"),
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
        EvalTerm::named("urn:class"),
    );
    absent_type.negated = true;
    let rule = EvalRule::positive(
        "urn:derive-domain-from-absence",
        EvalAtom::positive(
            EvalTerm::named(EDGE),
            "http://www.w3.org/2000/01/rdf-schema#domain",
            EvalTerm::named("urn:class"),
        ),
        vec![
            EvalAtom::positive(EvalTerm::var("?x"), EDGE, EvalTerm::var("?y")),
            absent_type,
        ],
    );
    let domain = super::super::schema::laws()
        .iter()
        .find(|law| law.source.rule_iri == "dl:domain")
        .unwrap();
    let outcome = JointProgram::prepare_with_properties(
        &[rule],
        &[],
        std::slice::from_ref(domain),
        &std::collections::BTreeSet::from([EDGE.to_owned()]),
    )
    .unwrap();
    assert!(matches!(
        outcome,
        NativeOutcome::Unsupported(crate::physical::UnsupportedKind::NonStratifiable)
    ));
}

#[test]
fn active_schema_and_invention_share_a_joint_termination_certificate() {
    let mut builder = RdfDatasetBuilder::new();
    builder.push_owned_quad(&RdfQuad::new(RdfTerm::iri(X), EDGE, RdfTerm::iri("urn:y")));
    builder.push_owned_quad(&RdfQuad::new(
        RdfTerm::iri(EDGE),
        "http://www.w3.org/2000/01/rdf-schema#domain",
        RdfTerm::iri("urn:class"),
    ));
    let input = builder.freeze().unwrap();
    let prepared = crate::program_analysis::prepare_program(&program()).unwrap();
    let full = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&input).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(full.status, crate::seam::BudgetStatus::Ok);
    assert!(full.inferred.iter().any(|row| row.predicate == DONE));
    assert!(!full.certificates.is_empty());
    assert!(full.certificates.iter().all(|certificate| {
        certificate.admission.admits_native()
            && certificate
                .admission
                .to_finding()
                .message
                .contains("joint statement value-flow")
    }));
    let bounded = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&input).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        Some(100),
    )
    .unwrap();
    assert_eq!(bounded.status, crate::seam::BudgetStatus::Ok);
    assert_eq!(bounded.inferred, full.inferred);
    assert_eq!(bounded.witnesses, full.witnesses);
    assert_eq!(bounded.certificates, full.certificates);
}

#[test]
fn schema_plans_reuse_analyses_without_reusing_world_data() {
    let prepared = crate::program_analysis::prepare_program(&domain_program()).unwrap();
    let original = prepare_reasoning_input(&input()).unwrap();
    let first_world = &original.facts;
    let sources = crate::reason::source_existentials::Collector::default()
        .prepare()
        .unwrap();
    let domains = crate::physical::SelectedDomains::new([]).unwrap();
    let laws = applicable_schema_laws(&prepared, first_world, &sources, &domains, &[], &[]);
    let selected = prepared
        .reasoning_input(
            &laws,
            &sources,
            &domains,
            first_world,
            std::sync::Arc::from([]),
            &[],
        )
        .unwrap();
    let NativeOutcome::Decided(first) = prepared.reasoning_schema_program(&selected).unwrap()
    else {
        panic!("finite schema program")
    };
    let again = prepared
        .reasoning_input(
            &laws,
            &sources,
            &domains,
            first_world,
            std::sync::Arc::from([]),
            &[],
        )
        .unwrap();
    let NativeOutcome::Decided(second) = prepared.reasoning_schema_program(&again).unwrap() else {
        panic!("cached finite schema program")
    };
    assert!(Arc::ptr_eq(&first, &second));
    let mut other_values = first_world.clone();
    other_values
        .get_mut(crate::reason::rl::DEFAULT_WORLD)
        .unwrap()
        .push(Fact {
            subject: TermValue::iri(X),
            predicate: EDGE.to_owned(),
            object: TermValue::iri(DONE),
        });
    let changed = prepared
        .reasoning_input(
            &laws,
            &sources,
            &domains,
            &other_values,
            std::sync::Arc::from([]),
            &[],
        )
        .unwrap();
    assert_ne!(
        selected.identity(),
        changed.identity(),
        "value changes can change the producer graph without adding predicates"
    );
    let NativeOutcome::Decided(other) = prepared.reasoning_schema_program(&changed).unwrap() else {
        panic!("changed finite schema program")
    };
    assert!(!Arc::ptr_eq(&first, &other));
    let changed_origins = BTreeMap::from([(
        crate::reason::rl::DEFAULT_WORLD.to_owned(),
        other_values[crate::reason::rl::DEFAULT_WORLD]
            .iter()
            .map(|fact| super::super::refute::RefutationPremise {
                subject: fact.subject.clone(),
                predicate: fact.predicate.clone(),
                object: fact.object.clone(),
                graph: None,
            })
            .collect::<Vec<_>>()
            .into(),
    )]);
    let changed_binding = changed
        .bind_native(
            &changed_origins,
            &original.graphs,
            Some(crate::modal::native::NativeModalProgram::prepare(&changed_origins).unwrap()),
            Some(Arc::new(
                crate::contextual::native::NativeContextualProgram::prepare(&changed_origins)
                    .unwrap(),
            )),
        )
        .unwrap();
    let mut governor = crate::physical::StepGovernor::new(None);
    let mut registry = crate::physical::SkolemRegistry::new();
    let error = first
        .materialize_input_governed(&changed, changed_binding, &mut governor, &mut registry)
        .err()
        .unwrap();
    assert!(
        error
            .message()
            .contains("schedule does not admit this native input summary")
    );
    assert_eq!(
        governor.consumed, 0,
        "stale schedule must fail before execution"
    );
    assert!(
        first.materialize_facts(first_world, None).is_err(),
        "unbound ingress cannot evade source admission"
    );
    let second_world = BTreeMap::from([(
        "urn:other:world".to_owned(),
        first_world[crate::reason::rl::DEFAULT_WORLD].clone(),
    )]);
    let renamed = prepared
        .reasoning_input(
            &laws,
            &sources,
            &domains,
            &second_world,
            std::sync::Arc::from([]),
            &[],
        )
        .unwrap();
    assert_eq!(
        selected.identity(),
        renamed.identity(),
        "world-local planning may share a complete value abstraction"
    );
    let graph = TermValue::iri("urn:other:world");
    let renamed_graphs = BTreeMap::from([("urn:other:world".to_owned(), Some(graph.clone()))]);
    let renamed_origins = BTreeMap::from([(
        "urn:other:world".to_owned(),
        original.occurrences[crate::reason::rl::DEFAULT_WORLD]
            .iter()
            .cloned()
            .map(|mut source| {
                source.graph = Some(graph.clone());
                source
            })
            .collect::<Vec<_>>()
            .into(),
    )]);
    let binding = renamed
        .bind_native(
            &renamed_origins,
            &renamed_graphs,
            Some(crate::modal::native::NativeModalProgram::prepare(&renamed_origins).unwrap()),
            Some(Arc::new(
                crate::contextual::native::NativeContextualProgram::prepare(&renamed_origins)
                    .unwrap(),
            )),
        )
        .unwrap();
    let NativeOutcome::Decided(result) = first
        .materialize_input_governed(&renamed, binding, &mut governor, &mut registry)
        .unwrap()
    else {
        panic!("reused native execution")
    };
    assert!(
        result
            .result
            .rows
            .iter()
            .all(|row| row.graph == "urn:other:world")
    );
    assert!(result.result.rows.iter().any(|row| row.predicate == DONE));
}
