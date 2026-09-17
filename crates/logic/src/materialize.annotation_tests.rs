// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::annotation::{
    AnnotationFactRef, AnnotationLineageContract, AnnotationQueryClass, AnnotationRequest,
};
use crate::provenance::{ASSERT_RULE_IRI, ZWeightSemiring};
use gmeow_logic_compile::ir::{
    ContextualScope, Formula, LogicAxiom, LogicProgram, LogicRule, SemanticProfileId, Term,
};
use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm};

const WORLD: &str = "https://example.org/world";
const EDGE: &str = "https://example.org/edge";
const REACH: &str = "https://example.org/reach";
const X: &str = "https://example.org/x";
const Y: &str = "https://example.org/y";

fn dataset() -> std::sync::Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    builder.push_owned_quad(
        &RdfQuad::new(RdfTerm::iri(X), EDGE, RdfTerm::iri(Y)).in_graph(RdfTerm::iri(WORLD)),
    );
    builder.freeze().expect("valid annotation test dataset")
}

fn binary_program() -> LogicProgram {
    let head = LogicAxiom::new(
        "?x",
        REACH,
        gmeow_logic_compile::ir::AtomicTerm::resource("?y"),
        false,
        ContextualScope::default(),
    )
    .unwrap();
    let body = LogicAxiom::new(
        "?x",
        EDGE,
        gmeow_logic_compile::ir::AtomicTerm::resource("?y"),
        false,
        ContextualScope::default(),
    )
    .unwrap();
    let scope = ContextualScope {
        provenance: Some("https://example.org/rule/reach".to_owned()),
        ..ContextualScope::default()
    };
    LogicProgram::new(
        vec![],
        vec![LogicRule::new(head, vec![body], vec![], scope)],
        vec![],
        None,
    )
}

#[test]
fn annotated_nonmonotone_profiles_fold_selected_lineage_without_a_second_solve() {
    let input = dataset();
    for (profile, expected) in [
        (
            SemanticProfileId::WellFounded,
            AnnotationQueryClass::WellFounded,
        ),
        (
            SemanticProfileId::StableModel,
            AnnotationQueryClass::StableModel,
        ),
    ] {
        let contract = crate::annotation::AnnotationContract::exact();
        let annotated = materialize_program_annotated(
            &binary_program(),
            input.as_ref(),
            MaterializationLimits::default(),
            Some(profile),
            AnnotationRequest::new(
                &ZWeightSemiring,
                &contract,
                |fact: AnnotationFactRef<'_>| (fact.predicate == EDGE).then_some(2),
            ),
        )
        .expect("annotated non-monotone materialization");
        assert_eq!(annotated.certification.query_class, expected);
        assert_eq!(
            annotated.certification.lineage_contract,
            AnnotationLineageContract::SelectedPhysicalDerivation
        );
        assert_eq!(annotated.materialization.nonmonotone_solve_runs.len(), 1);
        let derived = annotated
            .quads
            .iter()
            .find(|row| row.quad.predicate == REACH)
            .expect("selected solver proof derives reach");
        assert_eq!(derived.annotation, 2);
        assert!(derived.derivations.iter().any(|d| d.annotation == 2));
    }
}

#[test]
fn annotated_existential_head_carries_body_product_onto_every_invented_tuple_row() {
    let rel = "https://example.org/rel";
    let constant = "https://example.org/constant";
    let body = Formula::atom(
        Term::iri(REACH).unwrap(),
        vec![Term::var("x").unwrap(), Term::var("y").unwrap()],
    )
    .unwrap();
    let head = Formula::atom(
        Term::iri(rel).unwrap(),
        vec![
            Term::var("x").unwrap(),
            Term::var("y").unwrap(),
            Term::iri(constant).unwrap(),
        ],
    )
    .unwrap();
    let formula = Formula::Forall {
        vars: vec!["x".to_owned(), "y".to_owned()],
        body: Box::new(Formula::Implies(Box::new(body), Box::new(head))),
    };
    let program = binary_program().with_formulas(vec![formula]);
    let contract = crate::annotation::AnnotationContract::exact();
    let annotated = materialize_program_annotated(
        &program,
        dataset().as_ref(),
        MaterializationLimits::default(),
        Some(SemanticProfileId::PositiveHorn),
        AnnotationRequest::new(
            &ZWeightSemiring,
            &contract,
            |fact: AnnotationFactRef<'_>| (fact.predicate == EDGE).then_some(3),
        ),
    )
    .expect("annotated existential materialization");

    assert_eq!(
        annotated.certification.query_class,
        AnnotationQueryClass::ExistentialChase
    );
    assert_eq!(
        annotated.certification.lineage_contract,
        AnnotationLineageContract::SelectedPhysicalDerivation
    );
    assert!(annotated.materialization.chase_admission.is_some());
    let reach = annotated
        .quads
        .iter()
        .find(|row| row.quad.predicate == REACH)
        .expect("joint closure derives reach");
    assert_eq!(reach.annotation, 3);
    assert!(reach.derivations.iter().any(|derivation| {
        derivation.rule_iri != ASSERT_RULE_IRI
            && derivation
                .sources
                .iter()
                .any(|source| source.predicate == EDGE)
    }));
    let invented = annotated
        .quads
        .iter()
        .filter(|row| row.quad.rule_iri != ASSERT_RULE_IRI && row.quad.predicate != REACH)
        .collect::<Vec<_>>();
    assert_eq!(invented.len(), 4, "instanceOf plus three positional rows");
    assert!(invented.iter().all(|row| row.annotation == 3));
    assert!(invented.iter().all(|row| {
        row.derivations.iter().any(|derivation| {
            derivation.annotation == 3
                && derivation
                    .sources
                    .iter()
                    .any(|source| source.predicate == REACH)
        })
    }));
}
fn feedback_program() -> LogicProgram {
    let tuple = || {
        Formula::atom(
            Term::iri("urn:materialize:tuple").unwrap(),
            vec![
                Term::var("x").unwrap(),
                Term::var("y").unwrap(),
                Term::iri(X).unwrap(),
            ],
        )
        .unwrap()
    };
    let implication = |body, head| Formula::Forall {
        vars: vec!["x".to_owned(), "y".to_owned()],
        body: Box::new(Formula::Implies(Box::new(body), Box::new(head))),
    };
    binary_program().with_formulas(vec![
        implication(
            Formula::atom(
                Term::iri(REACH).unwrap(),
                vec![Term::var("x").unwrap(), Term::var("y").unwrap()],
            )
            .unwrap(),
            tuple(),
        ),
        implication(
            tuple(),
            Formula::atom(
                Term::iri("urn:materialize:done").unwrap(),
                vec![Term::var("x").unwrap(), Term::var("y").unwrap()],
            )
            .unwrap(),
        ),
    ])
}

#[test]
fn public_materializers_share_joint_feedback_and_budgeted_lineage() {
    let program = feedback_program();
    let input = dataset();
    let full = materialize_program(
        &program,
        input.as_ref(),
        MaterializationLimits::default(),
        None,
    )
    .unwrap();
    assert!(
        full.quads
            .iter()
            .any(|quad| quad.predicate == "urn:materialize:done")
    );
    assert_eq!(full.frontier.consumed_steps, 6);
    let contract = crate::annotation::AnnotationContract::exact();
    let annotated = materialize_program_annotated(
        &program,
        input.as_ref(),
        MaterializationLimits::default(),
        None,
        AnnotationRequest::new(
            &ZWeightSemiring,
            &contract,
            |fact: AnnotationFactRef<'_>| (fact.predicate == EDGE).then_some(3),
        ),
    )
    .unwrap();
    let done = annotated
        .quads
        .iter()
        .find(|row| row.quad.predicate == "urn:materialize:done")
        .unwrap();
    assert_eq!(
        done.annotation, 81,
        "four selected tuple premises each carry weight 3"
    );
    assert_eq!(full.frontier, annotated.materialization.frontier);
    assert!(
        done.derivations
            .iter()
            .any(|derivation| derivation.sources.len() == 4)
    );
    let limits = MaterializationLimits { max_steps: Some(3) };
    let partial = materialize_program(&program, input.as_ref(), limits, None).unwrap();
    assert_eq!(partial.frontier.consumed_steps, 3);
    assert!(
        !partial
            .quads
            .iter()
            .any(|quad| quad.predicate == "urn:materialize:done")
    );
    assert!(
        partial
            .quads
            .iter()
            .any(|quad| quad.budget_status == BudgetStatus::Exhausted)
    );
    let annotated = materialize_program_annotated(
        &program,
        input.as_ref(),
        limits,
        None,
        AnnotationRequest::new(&ZWeightSemiring, &contract, |_: AnnotationFactRef<'_>| {
            Some(1)
        }),
    )
    .unwrap();
    assert_eq!(partial.frontier, annotated.materialization.frontier);
    assert!(
        annotated
            .quads
            .iter()
            .all(|row| row.quad.budget_status == BudgetStatus::Exhausted
                || row.quad.predicate == EDGE)
    );
}

#[test]
fn structured_existential_materialization_retains_witness_recipes() {
    let atom = |predicate, object| {
        StructuredAtom::new(
            StructuredTerm::var("?x"),
            predicate,
            StructuredTerm::var(object),
        )
    };
    let rules = [StructuredExistentialRule {
        rule_iri: "urn:materialize:invent".to_owned(),
        body: vec![atom(EDGE, "?y")],
        head: vec![atom(REACH, "?witness")],
        distinct: Vec::new(),
        witness_frontier: None,
    }];
    let result =
        materialize_existential_rules(dataset().as_ref(), &rules, MaterializationLimits::default())
            .unwrap();
    assert_eq!(result.witness_derivations.len(), 1);
    let recipe = &result.witness_derivations[0];
    assert_eq!(recipe.frontier, vec![TermValue::iri(X)]);
    assert_eq!(recipe.rule_iri, "urn:materialize:invent");
    assert!(
        result
            .quads
            .iter()
            .any(|quad| quad.predicate == REACH && quad.object == TermValue::iri(&recipe.witness))
    );
}
