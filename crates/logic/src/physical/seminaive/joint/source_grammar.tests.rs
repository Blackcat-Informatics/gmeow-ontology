// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::rule_ir::EvalAtom;

fn rule() -> ExistentialRule {
    ExistentialRule {
        numeric: Vec::new(),
        rule_iri: "urn:constructor".to_owned(),
        body: vec![EvalAtom::positive(
            EvalTerm::var("x"),
            "urn:seed",
            EvalTerm::var("value"),
        )],
        head: vec![
            EvalAtom::positive(EvalTerm::var("cell"), FIRST, EvalTerm::var("value")),
            EvalAtom::positive(EvalTerm::var("cell"), REST, EvalTerm::named(NIL)),
            EvalAtom::positive(EvalTerm::named("urn:class"), ONE_OF, EvalTerm::var("cell")),
        ],
        distinct: Vec::new(),
        witness_frontier: None,
        witness_policy: WitnessPolicy::FrontierSkolem,
    }
}

#[test]
fn constructor_grammar_checks_closed_paths_and_shared_tails() {
    let mut rule = rule();
    rule.head[1].object = EvalTerm::var("tail");
    rule.head.extend([
        EvalAtom::positive(EvalTerm::var("tail"), FIRST, EvalTerm::var("tail")),
        EvalAtom::positive(EvalTerm::var("tail"), REST, EvalTerm::named(NIL)),
        EvalAtom::positive(
            EvalTerm::named("urn:other-class"),
            ONE_OF,
            EvalTerm::var("tail"),
        ),
    ]);
    assert!(closed_head(&rule, SemanticVocabulary::Exact));
    rule.head[4].object = EvalTerm::var("cell");
    assert!(
        !closed_head(&rule, SemanticVocabulary::Exact),
        "cyclic rest is not a closed constructor"
    );
}

#[test]
fn constructor_grammar_rejects_incomplete_or_ambiguous_cells() {
    for predicate in [FIRST, REST] {
        let mut missing = rule();
        missing.head.retain(|atom| atom.predicate != predicate);
        assert!(!closed_head(&missing, SemanticVocabulary::Exact));
        let mut ambiguous = rule();
        ambiguous.head.push(EvalAtom::positive(
            EvalTerm::var("cell"),
            predicate,
            EvalTerm::named("urn:other"),
        ));
        assert!(!closed_head(&ambiguous, SemanticVocabulary::Exact));
    }
}

#[test]
fn constructor_grammar_cannot_rewrite_a_source_cell_or_open_tail() {
    let mut existing = rule();
    existing.head[0].subject = EvalTerm::var("x");
    assert!(!closed_head(&existing, SemanticVocabulary::Exact));
    let mut open = rule();
    open.head[1].object = EvalTerm::var("x");
    assert!(!closed_head(&open, SemanticVocabulary::Exact));
}

#[test]
fn constructor_grammar_retains_address_frontier_and_policy() {
    let mut rule = rule();
    rule.witness_frontier = Some(Vec::new());
    assert!(
        !closed_head(&rule, SemanticVocabulary::Exact),
        "a reused cell cannot acquire different first values"
    );
    rule.witness_frontier = Some(vec!["value".to_owned()]);
    assert!(closed_head(&rule, SemanticVocabulary::Exact));
    rule.witness_policy = WitnessPolicy::DlAncestorBlocking;
    assert!(!closed_head(&rule, SemanticVocabulary::Exact));
}

#[test]
fn constructor_grammar_does_not_reject_compatible_repeated_heads() {
    let mut rule = rule();
    rule.head.push(rule.head[0].clone());
    assert!(closed_head(&rule, SemanticVocabulary::GroundedLogicV1));
}

#[test]
fn constructor_grammar_checks_the_prepared_allocation_route() {
    let mut rule = rule();
    assert!(
        PreparedChaseRule::new(rule.clone())
            .unwrap()
            .uses_only_frontier_skolems()
    );
    let tuple = vec![
        EvalAtom::positive(
            EvalTerm::var("cell"),
            &crate::provenance::instance_of_iri(),
            EvalTerm::named("urn:relation"),
        ),
        EvalAtom::positive(
            EvalTerm::var("cell"),
            &format!("{}naryArg0", crate::provenance::LOGIC_NAMESPACE),
            EvalTerm::var("value"),
        ),
    ];
    rule.head.extend(tuple.clone());
    assert!(PreparedChaseRule::new(rule.clone()).is_err());
    rule.head = tuple;
    let prepared = PreparedChaseRule::new(rule).unwrap();
    assert_eq!(prepared.rule.witness_policy, WitnessPolicy::FrontierSkolem);
    assert!(!prepared.uses_only_frontier_skolems());
}

#[test]
fn constructor_alias_scan_visits_nested_quote_predicates_and_datatypes() {
    let iri = format!("{}untrusted", crate::facts::SKOLEM_PREFIX);
    let quoted = TermValue::Triple {
        s: TermValue::iri("urn:s").into(),
        p: TermValue::iri(&iri).into(),
        o: TermValue::iri("urn:o").into(),
    };
    assert!(contains_constructor(&quoted));
    let typed = TermValue::Literal {
        lexical_form: "text".to_owned(),
        datatype: iri,
        language: None,
        direction: None,
    };
    assert!(contains_constructor(&typed));
    assert!(!contains_constructor(&TermValue::simple_literal(
        "ordinary"
    )));
}

fn certificate_with_external(external: &[WorldProducerEffect]) -> Option<ConstructorClosedLists> {
    let rule = rule();
    let mut effects =
        super::super::producer_effects(&[], std::slice::from_ref(&rule), &[]).unwrap();
    let statement = |atom: &EvalAtom| {
        [
            atom.subject.clone(),
            EvalTerm::named(&atom.predicate),
            atom.object.clone(),
        ]
    };
    let flows = [crate::physical::effects::value_flow::FlowRule {
        body: rule.body.iter().map(statement).collect(),
        heads: rule.head.iter().map(statement).collect(),
        native_witnesses: rule.existentials(),
        reads: rule.body.iter().map(|atom| Some(statement(atom))).collect(),
        cardinality_guards: Vec::new(),
        list_reads: Vec::new(),
        selected_reads: Vec::new(),
    }];
    let flow = ValueFlow::new(&flows, &effects, SemanticVocabulary::Exact);
    let producers = [Arc::new(PreparedChaseRule::new(rule).unwrap())];
    ConstructorClosedLists::certify(GrammarInput {
        world: "urn:world",
        facts: &[],
        possible: &[],
        external,
        ordinary: 0,
        producers: &producers,
        properties: &[],
        families: &[],
        effects: &mut effects,
        flow: &flow,
    })
}

#[test]
fn constructor_grammar_checks_external_writers_and_world_ownership() {
    let unknown = |world: &str, predicate: Option<&str>| {
        WorldProducerEffect::local(
            world,
            ProducerEffect::new(
                "external".to_owned(),
                vec![StatementPattern::relation(predicate, None)],
                Vec::new(),
            ),
        )
    };
    assert!(certificate_with_external(&[unknown("urn:world", None)]).is_none());
    assert!(certificate_with_external(&[unknown("urn:world", Some(REST))]).is_none());
    assert!(certificate_with_external(&[unknown("urn:world", Some(ONE_OF))]).is_none());
    assert!(certificate_with_external(&[unknown("urn:other", None)]).is_some());
    let proof = certificate_with_external(&[unknown("urn:world", Some("urn:unrelated"))]).unwrap();
    assert!(proof.basis("urn:world", DlConstructFamily::OneOf).is_some());
    assert!(proof.basis("urn:other", DlConstructFamily::OneOf).is_none());
    assert!(
        proof
            .basis("urn:world", DlConstructFamily::UnionOf)
            .is_none()
    );
}
