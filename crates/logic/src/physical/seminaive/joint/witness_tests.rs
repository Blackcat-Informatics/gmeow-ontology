// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native witness ownership and committed-prefix contracts over fabricated facts.

use super::tests::{WORLD, atom, prepare, producer, run};
use super::*;
use crate::physical::{
    MinimumPattern, PropertyAtom, PropertyOperation, PropertyRule, WitnessPosition,
};
use crate::rule_ir::EvalTerm;
use purrdf::TermValue;

fn subject_and_object() -> ExistentialRule {
    producer(
        vec![atom("?x", "urn:seed", "?y")],
        vec![
            atom("?n", "urn:class", "urn:C"),
            atom("?x", "urn:edge", "?n"),
        ],
    )
}

fn source(name: &str) -> crate::native_semantics::ProgramAdmission {
    crate::native_semantics::ProgramAdmission {
        profile: crate::native_semantics::SourceAdmissionProfile::WorldLocalTemplatesV1,
        source_iri: Some(name.to_owned()),
        scopes: Vec::new(),
    }
}

#[test]
fn identical_frontiers_in_distinct_worlds_have_distinct_scoped_witnesses() {
    let result = run(
        &prepare(&[], &[subject_and_object()]),
        &["urn:world:a", "urn:world:b"],
        None,
    );
    assert_eq!(result.witness_derivations.len(), 2);
    let [a, b] = result.witness_derivations.as_slice() else {
        panic!("two worlds");
    };
    assert_ne!(a.witness, b.witness);
    assert_ne!(a.scope.world, b.scope.world);
    assert_eq!(a.scope.contract, b.scope.contract);
    assert_eq!(a.scope.source_rule, b.scope.source_rule);
    assert_eq!(a.frontier, b.frontier);
    for witness in &result.witness_derivations {
        witness.validate().unwrap();
    }
}

#[test]
fn source_rule_content_and_source_contract_both_participate_in_address() {
    let rule = subject_and_object();
    let first = run(
        &prepare(&[], std::slice::from_ref(&rule)).with_witness_source(&source("urn:module:a")),
        &[WORLD],
        None,
    );
    let other_source = run(
        &prepare(&[], std::slice::from_ref(&rule)).with_witness_source(&source("urn:module:b")),
        &[WORLD],
        None,
    );
    let mut changed = rule;
    changed.head[0].object = EvalTerm::named("urn:other-class");
    let other_rule = run(
        &prepare(&[], &[changed]).with_witness_source(&source("urn:module:a")),
        &[WORLD],
        None,
    );
    let a = &first.witness_derivations[0];
    let b = &other_source.witness_derivations[0];
    let c = &other_rule.witness_derivations[0];
    assert_eq!(
        a.rule_iri, c.rule_iri,
        "the unchanged source name is not a rule-content key"
    );
    assert_ne!(a.witness, b.witness);
    assert_ne!(a.scope.contract, b.scope.contract);
    assert_eq!(a.scope.source_rule, b.scope.source_rule);
    assert_ne!(a.witness, c.witness);
    assert_eq!(a.scope.contract, c.scope.contract);
    assert_ne!(a.scope.source_rule, c.scope.source_rule);
    let NativeOutcome::Decided(grounded) = JointProgram::prepare_with_semantics(
        &[],
        &[subject_and_object()],
        &[],
        &BTreeSet::new(),
        crate::native_semantics::SemanticVocabulary::GroundedLogicV1,
    )
    .unwrap() else {
        panic!("selected native profile");
    };
    let grounded = run(
        &grounded.with_witness_source(&source("urn:module:a")),
        &[WORLD],
        None,
    );
    assert_ne!(
        a.scope.contract,
        grounded.witness_derivations[0].scope.contract
    );
    assert_ne!(a.witness, grounded.witness_derivations[0].witness);
}

#[test]
fn reordered_and_unrelated_input_does_not_change_witness_address_or_proof() {
    let program = prepare(&[], &[subject_and_object()]);
    let mut facts = vec![
        Fact {
            subject: TermValue::iri("urn:a"),
            predicate: "urn:seed".to_owned(),
            object: TermValue::iri("urn:b"),
        },
        Fact {
            subject: TermValue::iri("urn:c"),
            predicate: "urn:seed".to_owned(),
            object: TermValue::iri("urn:d"),
        },
    ];
    let execute = |facts: Vec<Fact>| {
        let input = std::collections::BTreeMap::from([(WORLD.to_owned(), facts)]);
        let NativeOutcome::Decided(result) = program.materialize_facts(&input, None).unwrap()
        else {
            panic!("admitted program");
        };
        result.witness_derivations
    };
    let first = execute(facts.clone());
    facts.reverse();
    facts.push(Fact {
        subject: TermValue::iri("urn:unrelated"),
        predicate: "urn:unrelated-predicate".to_owned(),
        object: TermValue::simple_literal("irrelevant"),
    });
    assert_eq!(first, execute(facts));
}

#[test]
fn minting_heads_keep_subject_and_object_positions_and_exclude_later_edges() {
    let consumer = EvalRule::positive(
        "urn:later",
        atom("?x", "urn:later-edge", "?n"),
        vec![atom("?x", "urn:edge", "?n")],
    );
    let result = run(
        &prepare(&[consumer], &[subject_and_object()]),
        &[WORLD],
        None,
    );
    assert!(
        result
            .result
            .rows
            .iter()
            .any(|row| row.predicate == "urn:later-edge")
    );
    let witness = &result.witness_derivations[0];
    assert_eq!(witness.heads.len(), 2);
    let class = witness
        .heads
        .iter()
        .find(|head| head.statement.predicate == "urn:class")
        .unwrap();
    assert_eq!(class.positions, vec![WitnessPosition::Subject]);
    let edge = witness
        .heads
        .iter()
        .find(|head| head.statement.predicate == "urn:edge")
        .unwrap();
    assert_eq!(edge.positions, vec![WitnessPosition::Object]);
    for head in &witness.heads {
        assert_eq!(head.premises.len(), 1);
        assert_eq!(head.premises[0].predicate, "urn:seed");
    }
    witness.validate().unwrap();
}

#[test]
fn bodyless_subject_only_head_uses_the_same_scoped_registry() {
    let rule = producer(Vec::new(), vec![atom("?n", "urn:class", "urn:C")]);
    let result = run(&prepare(&[], &[rule]), &[WORLD], None);
    let witness = &result.witness_derivations[0];
    assert!(witness.frontier.is_empty());
    assert_eq!(witness.heads.len(), 1);
    assert!(
        witness.heads[0].premises.is_empty(),
        "no invented reflexive source premise"
    );
    assert_eq!(witness.heads[0].positions, [WitnessPosition::Subject]);
    witness.validate().unwrap();
}

#[test]
fn zero_and_partial_budget_publish_only_committed_minting_heads() {
    let program = prepare(&[], &[subject_and_object()]);
    let zero = run(&program, &[WORLD], Some(0));
    assert_eq!(zero.result.status, BudgetStatus::Exhausted);
    assert!(zero.witness_derivations.is_empty());
    let partial = run(&program, &[WORLD], Some(1));
    assert_eq!(partial.result.status, BudgetStatus::Exhausted);
    assert_eq!(partial.result.consumed_steps, 1);
    assert_eq!(partial.witness_derivations.len(), 1);
    let witness = &partial.witness_derivations[0];
    assert_eq!(
        witness.heads.len(),
        1,
        "uncommitted conjuncts are not published"
    );
    let head = &witness.heads[0].statement;
    assert!(
        partial
            .result
            .rows
            .iter()
            .any(|row| row.graph == witness.scope.world
                && row.subject == head.subject
                && row.predicate == head.predicate
                && row.object == head.object)
    );
    witness.validate().unwrap();
}

#[test]
fn minimum_and_generic_invention_share_world_contract_and_committed_head_protocol() {
    let minimum = PreparedPropertyRule::new(PropertyRule {
        rule_iri: "urn:minimum".to_owned(),
        head: PropertyAtom([
            EvalTerm::var("?x"),
            EvalTerm::named("urn:minimum-edge"),
            EvalTerm::var("?n"),
        ]),
        body: vec![PropertyAtom([
            EvalTerm::var("?x"),
            EvalTerm::named("urn:seed"),
            EvalTerm::var("?y"),
        ])],
        operation: Some(PropertyOperation::Minimum(MinimumPattern {
            subject: EvalTerm::var("?x"),
            property: EvalTerm::named("urn:minimum-edge"),
            minimum: EvalTerm::ConstLit(TermValue::Literal {
                lexical_form: "1".to_owned(),
                datatype: "http://www.w3.org/2001/XMLSchema#nonNegativeInteger".to_owned(),
                language: None,
                direction: None,
            }),
            class: Some(EvalTerm::named("urn:C")),
            witness: "?n".to_owned(),
        })),
        guards: Vec::new(),
    })
    .unwrap();
    let NativeOutcome::Decided(program) = JointProgram::prepare_with_properties(
        &[],
        &[subject_and_object()],
        &[minimum],
        &BTreeSet::new(),
    )
    .unwrap() else {
        panic!("joint admission");
    };
    let result = run(&program, &["urn:world:a", "urn:world:b"], None);
    assert_eq!(result.witness_derivations.len(), 4);
    let contracts: BTreeSet<_> = result
        .witness_derivations
        .iter()
        .map(|w| w.scope.contract)
        .collect();
    assert_eq!(contracts.len(), 1);
    let sources: BTreeSet<_> = result
        .witness_derivations
        .iter()
        .map(|w| w.scope.source_rule)
        .collect();
    assert_eq!(sources.len(), 2);
    for witness in &result.witness_derivations {
        assert_eq!(witness.heads.len(), 2);
        assert!(
            witness
                .frontier
                .iter()
                .all(|term| term.as_iri() != Some(witness.scope.world.as_str())),
            "world is a scope, never a fabricated frontier argument"
        );
        witness.validate().unwrap();
    }
}

#[test]
fn native_witness_receipt_rejects_missing_heads_wrong_scope_and_tampered_proof() {
    let result = run(&prepare(&[], &[subject_and_object()]), &[WORLD], None);
    let witness = &result.witness_derivations[0];
    let wire = witness.to_wire().unwrap();
    assert_eq!(*witness, WitnessDerivation::from_wire(&wire).unwrap());
    let mut bytes = Vec::new();
    ciborium::into_writer(witness, &mut bytes).unwrap();
    let decoded: WitnessDerivation = ciborium::from_reader(bytes.as_slice()).unwrap();
    assert_eq!(*witness, decoded);
    for mutation in 0..5 {
        let mut changed = witness.clone();
        match mutation {
            0 => changed.heads.clear(),
            1 => changed.scope.world = "urn:foreign-world".to_owned(),
            2 => changed.heads[0].positions.clear(),
            3 => changed.heads[0].derivation_id = "forged".to_owned(),
            _ => changed.rule_iri = "urn:foreign-rule".to_owned(),
        }
        assert!(changed.validate().is_err(), "mutation {mutation}");
    }
    assert!(
        WitnessDerivation::from_wire("{}").is_err(),
        "unversioned receipts are refused"
    );
}

mod retention;
