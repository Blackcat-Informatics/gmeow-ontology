// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMEOW list-law feedback and evidence over synthetic native inputs.

use super::*;
use gmeow_logic_compile::ir::{Formula, LogicProgram, Term};
use purrdf::{RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};
use std::collections::BTreeSet;

const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
const ONE_OF: &str = "http://www.w3.org/2002/07/owl#oneOf";
const CHAIN: &str = "http://www.w3.org/2002/07/owl#propertyChainAxiom";
const DIFFERENT: &str = "http://www.w3.org/2002/07/owl#differentFrom";

fn fact(s: &str, p: &str, o: &str) -> RdfQuad {
    RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o))
}

fn source(quads: Vec<RdfQuad>) -> std::sync::Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for quad in quads {
        builder.push_owned_quad(&quad);
    }
    builder.freeze().unwrap()
}

fn atom(s: &str, p: &str, o: &str) -> Formula {
    let term = |value: &str| match value.strip_prefix('?') {
        Some(name) => Term::var(name).unwrap(),
        None => Term::iri(value).unwrap(),
    };
    Formula::atom(Term::iri(p).unwrap(), vec![term(s), term(o)]).unwrap()
}

fn program(rules: Vec<(Formula, Formula)>) -> LogicProgram {
    LogicProgram::new(vec![], vec![], vec![], None).with_formulas(
        rules
            .into_iter()
            .map(|(body, head)| Formula::Forall {
                vars: vec!["x".to_owned(), "y".to_owned()],
                body: Box::new(Formula::Implies(Box::new(body), Box::new(head))),
            })
            .collect(),
    )
}

fn member_consumer() -> LogicProgram {
    program(vec![(
        atom("?x", TYPE, "urn:class"),
        atom("?x", "urn:done", "urn:class"),
    )])
}

#[test]
fn derived_list_cells_wake_nominal_membership_and_its_consumer() {
    let p = program(vec![
        (atom("?x", "urn:seed", "?y"), atom("urn:list", FIRST, "?x")),
        (atom("urn:list", FIRST, "?x"), atom("urn:list", REST, NIL)),
        (
            atom("?x", TYPE, "urn:class"),
            atom("?x", "urn:done", "urn:class"),
        ),
    ]);
    let prepared = crate::program_analysis::prepare_program(&p).unwrap();
    let data = source(vec![
        fact("urn:class", ONE_OF, "urn:list"),
        fact("urn:x", "urn:seed", "urn:y"),
    ]);
    let full = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&data).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    assert!(
        full.inferred
            .iter()
            .any(|row| row.subject == "urn:x" && row.predicate == "urn:done")
    );
    let member = full
        .inferred
        .iter()
        .find(|row| row.rule_name.as_deref() == Some("dl:oneOf-member"))
        .unwrap();
    assert_eq!(
        member.premises,
        vec![
            (
                "urn:class".to_owned(),
                ONE_OF.to_owned(),
                "<urn:list>".to_owned()
            ),
            (
                "urn:list".to_owned(),
                FIRST.to_owned(),
                "<urn:x>".to_owned()
            ),
            ("urn:list".to_owned(), REST.to_owned(), format!("<{NIL}>")),
        ]
    );
    let cut = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&data).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        Some(2),
    )
    .unwrap();
    assert_eq!(cut.status, crate::seam::BudgetStatus::Exhausted);
    assert!(!cut.inferred.iter().any(|row| row.predicate == "urn:done"));
}

#[test]
fn a_shared_list_tail_is_an_independent_logical_reference() {
    let data = source(vec![
        fact("urn:outer", FIRST, "urn:outside"),
        fact("urn:outer", REST, "urn:tail"),
        fact("urn:tail", FIRST, "urn:x"),
        fact("urn:tail", REST, NIL),
        fact("urn:class", ONE_OF, "urn:tail"),
    ]);
    let prepared = crate::program_analysis::prepare_program(&member_consumer()).unwrap();
    let result = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&data).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    let done: Vec<_> = result
        .inferred
        .iter()
        .filter(|row| row.predicate == "urn:done")
        .map(|row| row.subject.as_str())
        .collect();
    assert_eq!(done, ["urn:x"]);
}

#[test]
fn incomplete_or_ambiguous_nominals_cannot_certify_a_complete_run() {
    let prepared = crate::program_analysis::prepare_program(&member_consumer()).unwrap();
    for (tail, extra, expected) in [
        ("urn:missing", None, "incomplete selected logical list"),
        ("urn:list", None, "cyclic RDF list"),
        (NIL, Some(fact("urn:list", FIRST, "urn:y")), "multiple"),
        (
            NIL,
            Some(fact("urn:list", REST, "urn:other-tail")),
            "multiple",
        ),
    ] {
        let mut quads = vec![
            fact("urn:class", ONE_OF, "urn:list"),
            fact("urn:list", FIRST, "urn:x"),
            fact("urn:list", REST, tail),
        ];
        quads.extend(extra);
        let error = execute(
            &prepared,
            crate::reason::program::prepare_reasoning_input(&source(quads)).unwrap(),
            &crate::physical::SelectedDomains::new([]).unwrap(),
            None,
        )
        .err()
        .expect("malformed nominal must refuse closure");
        assert!(error.message().contains(expected), "{error}");
    }
}

#[test]
fn property_chain_uses_derived_edges_and_retains_literal_and_full_path_evidence() {
    let p = program(vec![
        (atom("?x", "urn:seed", "?y"), atom("?x", "urn:p3", "?y")),
        (
            atom("?x", "urn:composed", "?y"),
            atom("?x", "urn:done", "?y"),
        ),
    ]);
    let data = source(vec![
        fact("urn:composed", CHAIN, "urn:l0"),
        fact("urn:l0", FIRST, "urn:p1"),
        fact("urn:l0", REST, "urn:l1"),
        fact("urn:l1", FIRST, "urn:p2"),
        fact("urn:l1", REST, "urn:l2"),
        fact("urn:l2", FIRST, "urn:p3"),
        fact("urn:l2", REST, NIL),
        fact("urn:x", "urn:p1", "urn:m1"),
        fact("urn:m1", "urn:p2", "urn:m2"),
        RdfQuad::new(
            RdfTerm::iri("urn:m2"),
            "urn:seed",
            RdfTerm::Literal(RdfLiteral::language_tagged("valeur", "fr")),
        ),
    ]);
    let prepared = crate::program_analysis::prepare_program(&p).unwrap();
    let result = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&data).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    let done = result
        .inferred
        .iter()
        .find(|row| row.predicate == "urn:done")
        .unwrap();
    assert_eq!(done.subject, "urn:x");
    assert_eq!(done.object, purrdf::TermValue::lang_literal("valeur", "fr"));
    let composed = result
        .inferred
        .iter()
        .find(|row| row.rule_name.as_deref() == Some("dl:property-chain"))
        .unwrap();
    assert_eq!(composed.premises.len(), 10);
    assert_eq!(composed.premises[0].1, CHAIN);
    assert_eq!(composed.premises[6].1, REST);
    assert_eq!(composed.premises[7].1, "urn:p1");
    assert_eq!(composed.premises[8].1, "urn:p2");
    assert_eq!(composed.premises[9].1, "urn:p3");
}

#[test]
fn repeated_member_occurrences_preserve_all_different_counterevidence() {
    let p = program(vec![(
        atom("?x", DIFFERENT, "?y"),
        atom("?x", "urn:done", "?y"),
    )]);
    let data = source(vec![
        fact(
            "urn:axiom",
            TYPE,
            "http://www.w3.org/2002/07/owl#AllDifferent",
        ),
        fact(
            "urn:axiom",
            "http://www.w3.org/2002/07/owl#members",
            "urn:l0",
        ),
        fact("urn:l0", FIRST, "urn:x"),
        fact("urn:l0", REST, "urn:l1"),
        fact("urn:l1", FIRST, "urn:x"),
        fact("urn:l1", REST, NIL),
    ]);
    let prepared = crate::program_analysis::prepare_program(&p).unwrap();
    let result = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&data).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    assert!(result.inferred.iter().any(|row| row.subject == "urn:x"
        && row.predicate == "urn:done"
        && row.object.as_iri() == Some("urn:x")));
    let distinct = result
        .inferred
        .iter()
        .find(|row| row.rule_name.as_deref() == Some("dl:allDifferent-pairwise"))
        .unwrap();
    assert_eq!(distinct.premises.len(), 6);
}

#[test]
fn list_cache_and_membership_are_local_to_their_source_world() {
    let mut quads = Vec::new();
    for (world, member) in [("urn:world:a", "urn:x"), ("urn:world:b", "urn:y")] {
        quads.extend(
            [
                fact("urn:class", ONE_OF, "urn:list"),
                fact("urn:list", FIRST, member),
                fact("urn:list", REST, NIL),
            ]
            .map(|quad| quad.in_graph(RdfTerm::iri(world))),
        );
    }
    let prepared = crate::program_analysis::prepare_program(&member_consumer()).unwrap();
    let result = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&source(quads)).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    let done: std::collections::BTreeSet<_> = result
        .inferred
        .iter()
        .filter(|row| row.predicate == "urn:done")
        .map(|row| (row.world.as_str(), row.subject.as_str()))
        .collect();
    assert_eq!(
        done,
        std::collections::BTreeSet::from([("urn:world:a", "urn:x"), ("urn:world:b", "urn:y")])
    );
}

#[test]
fn list_membership_feedback_cannot_hide_an_existential_cycle() {
    let head = Formula::Exists {
        vars: vec!["z".to_owned()],
        body: Box::new(Formula::And(vec![
            atom("?x", "urn:witness", "?z"),
            atom("?z", FIRST, "?z"),
            atom("?z", REST, NIL),
            atom("urn:class", ONE_OF, "?z"),
        ])),
    };
    let p = program(vec![
        (atom("?x", "urn:seed", "?y"), head),
        (
            atom("?x", TYPE, "urn:class"),
            atom("?x", "urn:seed", "urn:unit"),
        ),
    ]);
    let prepared = crate::program_analysis::prepare_program(&p).unwrap();
    let data = source(vec![fact("urn:x", "urn:seed", "urn:unit")]);
    assert!(prepared.preservation.unsupported_constructs.is_empty());
    assert_eq!(
        prepared.existential_rules.len(),
        1,
        "the authored producer must reach admission"
    );
    let error = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&data).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .err()
    .expect("list-member value flow must participate in the joint proof");
    assert!(
        error.message().contains("NonTerminatingExistential"),
        "{error}"
    );
    let bounded = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&data).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        Some(32),
    )
    .unwrap();
    assert_eq!(bounded.status, crate::seam::BudgetStatus::Exhausted);
    assert!(bounded.witnesses.len() > 1);
    assert!(
        bounded
            .certificates
            .iter()
            .all(|certificate| !certificate.admission.admits_native())
    );
}

#[test]
fn single_binary_existential_uses_one_witness_per_frontier_binding() {
    let p = program(vec![(
        atom("?x", "urn:seed", "?y"),
        Formula::Exists {
            vars: vec!["z".to_owned()],
            body: Box::new(atom("?x", "urn:witness", "?z")),
        },
    )]);
    let prepared = crate::program_analysis::prepare_program(&p).unwrap();
    assert!(prepared.preservation.unsupported_constructs.is_empty());
    assert_eq!(prepared.existential_rules.len(), 1);
    assert!(prepared.rules.is_empty());
    let data = source(vec![
        fact("urn:x", "urn:seed", "urn:unit"),
        fact("urn:x", "urn:seed", "urn:other"),
        fact("urn:y", "urn:seed", "urn:unit"),
    ]);
    let result = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&data).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    let links: Vec<_> = result
        .inferred
        .iter()
        .filter(|row| row.predicate == "urn:witness")
        .collect();
    assert_eq!(
        links.len(),
        2,
        "body-only bindings cannot invent extra witnesses"
    );
    assert_ne!(
        links[0].object, links[1].object,
        "distinct frontier bindings have distinct witnesses"
    );
    assert_eq!(result.witnesses.len(), 2);
    assert!(
        result
            .certificates
            .iter()
            .all(|certificate| certificate.admission.admits_native())
    );
}

const INTERSECTION: &str = "http://www.w3.org/2002/07/owl#intersectionOf";
const NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";

fn two_member_list() -> Vec<RdfQuad> {
    vec![
        fact("urn:list", FIRST, "urn:a"),
        fact("urn:list", REST, "urn:tail"),
        fact("urn:tail", FIRST, "urn:b"),
        fact("urn:tail", REST, NIL),
    ]
}

#[test]
fn intersection_membership_shares_late_type_feedback_and_complete_premises() {
    let p = program(vec![
        (atom("?x", "urn:seed", "?y"), atom("?x", TYPE, "urn:b")),
        (
            atom("?x", TYPE, "urn:class"),
            atom("?x", "urn:done", "urn:class"),
        ),
    ]);
    let mut quads = two_member_list();
    quads.extend([
        fact("urn:class", INTERSECTION, "urn:list"),
        fact("urn:x", TYPE, "urn:a"),
        fact("urn:x", "urn:seed", "urn:y"),
    ]);
    let data = source(quads);
    let prepared = crate::program_analysis::prepare_program(&p).unwrap();
    let result = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&data).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    assert!(
        result
            .inferred
            .iter()
            .any(|row| row.subject == "urn:x" && row.predicate == "urn:done")
    );
    let membership = result
        .inferred
        .iter()
        .find(|row| {
            row.subject == "urn:x" && row.rule_name.as_deref() == Some("dl:intersection-membership")
        })
        .unwrap();
    for (s, p, o) in [
        ("urn:list", FIRST, "urn:a"),
        ("urn:list", REST, "urn:tail"),
        ("urn:tail", FIRST, "urn:b"),
        ("urn:tail", REST, NIL),
        ("urn:x", TYPE, "urn:a"),
        ("urn:x", TYPE, "urn:b"),
    ] {
        assert!(
            membership
                .premises
                .contains(&(s.to_owned(), p.to_owned(), format!("<{o}>")))
        );
    }
    let cut = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&data).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        Some(1),
    )
    .unwrap();
    assert_eq!(cut.status, crate::seam::BudgetStatus::Exhausted);
    assert!(!cut.inferred.iter().any(|row| row.predicate == "urn:done"));
}

#[test]
fn intersection_guard_finishes_before_negated_consumers() {
    let p = program(vec![
        (atom("?x", "urn:seed", "?y"), atom("?x", TYPE, "urn:b")),
        (
            Formula::And(vec![
                atom("?x", "urn:seed", "?y"),
                Formula::Not(Box::new(atom("?x", TYPE, "urn:class"))),
            ]),
            atom("?x", "urn:absent", "urn:class"),
        ),
    ]);
    let mut quads = two_member_list();
    quads.extend([
        fact("urn:class", INTERSECTION, "urn:list"),
        fact("urn:x", TYPE, "urn:a"),
        fact("urn:x", "urn:seed", "urn:y"),
    ]);
    let prepared = crate::program_analysis::prepare_program(&p).unwrap();
    let result = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&source(quads)).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    assert!(
        !result
            .inferred
            .iter()
            .any(|row| row.predicate == "urn:absent")
    );
    assert!(result.inferred.iter().any(|row| row.subject == "urn:x"
        && row.predicate == TYPE
        && row.object.as_iri() == Some("urn:class")));
}

#[test]
fn intersection_index_wakes_when_its_first_member_type_arrives_late() {
    let p = program(vec![
        (atom("?x", "urn:seed", "?y"), atom("?x", TYPE, "urn:a")),
        (
            atom("?x", TYPE, "urn:class"),
            atom("?x", "urn:done", "urn:class"),
        ),
    ]);
    let mut quads = two_member_list();
    quads.extend([
        fact("urn:class", INTERSECTION, "urn:list"),
        fact("urn:x", TYPE, "urn:b"),
        fact("urn:x", "urn:seed", "urn:y"),
        // Unrelated types cannot become premises of the conjunction proof.
        fact("urn:x", TYPE, "urn:0-unrelated"),
        fact("urn:outsider", TYPE, "urn:0-unrelated"),
    ]);
    let prepared = crate::program_analysis::prepare_program(&p).unwrap();
    let result = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&source(quads)).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    assert!(
        result
            .inferred
            .iter()
            .any(|row| { row.subject == "urn:x" && row.predicate == "urn:done" })
    );
    assert!(
        !result
            .inferred
            .iter()
            .any(|row| { row.subject == "urn:outsider" && row.predicate == "urn:done" })
    );
    let proof = result
        .inferred
        .iter()
        .find(|row| {
            row.subject == "urn:x" && row.rule_name.as_deref() == Some("dl:intersection-membership")
        })
        .unwrap();
    let expected: BTreeSet<_> = [
        ("urn:class", INTERSECTION, "urn:list"),
        ("urn:list", FIRST, "urn:a"),
        ("urn:list", REST, "urn:tail"),
        ("urn:tail", FIRST, "urn:b"),
        ("urn:tail", REST, NIL),
        ("urn:x", TYPE, "urn:a"),
        ("urn:x", TYPE, "urn:b"),
    ]
    .map(|(s, p, o)| (s.to_owned(), p.to_owned(), format!("<{o}>")))
    .into();
    assert_eq!(
        proof.premises.iter().cloned().collect::<BTreeSet<_>>(),
        expected
    );
}

#[test]
fn nominal_closure_requires_every_explicit_difference_without_unique_names() {
    let p = program(vec![(
        atom("?x", TYPE, NOTHING),
        atom("?x", "urn:clash-consumed", "urn:class"),
    )]);
    let prepared = crate::program_analysis::prepare_program(&p).unwrap();
    for complete in [false, true] {
        let mut quads = two_member_list();
        quads.extend([
            fact("urn:class", ONE_OF, "urn:list"),
            fact("urn:x", TYPE, "urn:class"),
            fact("urn:a", DIFFERENT, "urn:x"),
        ]);
        if complete {
            quads.push(fact("urn:b", DIFFERENT, "urn:x"));
        }
        let result = execute(
            &prepared,
            crate::reason::program::prepare_reasoning_input(&source(quads)).unwrap(),
            &crate::physical::SelectedDomains::new([]).unwrap(),
            None,
        )
        .unwrap();
        assert_eq!(
            result
                .inferred
                .iter()
                .any(|row| row.subject == "urn:x" && row.predicate == "urn:clash-consumed"),
            complete
        );
        if complete {
            let clash = result
                .inferred
                .iter()
                .find(|row| {
                    row.subject == "urn:x"
                        && row.rule_name.as_deref() == Some("dl:oneOf-closure-clash")
                })
                .unwrap();
            for member in ["urn:a", "urn:b"] {
                assert!(
                    clash.premises.contains(&(
                        member.to_owned(),
                        DIFFERENT.to_owned(),
                        "<urn:x>".to_owned()
                    )),
                    "retain the actual reverse-oriented premise"
                );
            }
            assert!(clash.premises.contains(&(
                "urn:tail".to_owned(),
                REST.to_owned(),
                format!("<{NIL}>")
            )));
        }
    }
}

#[test]
fn late_distinctness_wakes_nominal_closure_and_its_rule_consumer() {
    let p = program(vec![
        (atom("?x", "urn:seed", "?y"), atom("?x", DIFFERENT, "urn:b")),
        (
            atom("?x", TYPE, NOTHING),
            atom("?x", "urn:done", "urn:class"),
        ),
    ]);
    let mut quads = two_member_list();
    quads.extend([
        fact("urn:class", ONE_OF, "urn:list"),
        fact("urn:x", TYPE, "urn:class"),
        fact("urn:x", DIFFERENT, "urn:a"),
        fact("urn:x", "urn:seed", "urn:y"),
    ]);
    let prepared = crate::program_analysis::prepare_program(&p).unwrap();
    let result = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&source(quads)).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    assert!(
        result
            .inferred
            .iter()
            .any(|row| row.subject == "urn:x" && row.predicate == "urn:done")
    );
}

#[test]
fn universal_list_guards_never_join_types_across_worlds() {
    let mut quads: Vec<_> = two_member_list()
        .into_iter()
        .chain([
            fact("urn:class", INTERSECTION, "urn:list"),
            fact("urn:x", TYPE, "urn:a"),
        ])
        .map(|quad| quad.in_graph(RdfTerm::iri("urn:world:one")))
        .collect();
    quads.push(fact("urn:x", TYPE, "urn:b").in_graph(RdfTerm::iri("urn:world:two")));
    let prepared = crate::program_analysis::prepare_program(&member_consumer()).unwrap();
    let result = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&source(quads)).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    assert!(
        !result
            .inferred
            .iter()
            .any(|row| row.predicate == "urn:done")
    );
}

#[test]
fn empty_nominal_guard_remains_in_joint_invention_admission() {
    let p = program(vec![(
        atom("?x", TYPE, NOTHING),
        Formula::Exists {
            vars: vec!["z".to_owned()],
            body: Box::new(Formula::And(vec![
                atom("?x", "urn:next", "?z"),
                atom("?z", TYPE, "urn:class"),
            ])),
        },
    )]);
    let data = source(vec![
        fact("urn:class", ONE_OF, NIL),
        fact("urn:x", TYPE, "urn:class"),
    ]);
    let prepared = crate::program_analysis::prepare_program(&p).unwrap();
    assert!(
        execute(
            &prepared,
            crate::reason::program::prepare_reasoning_input(&data).unwrap(),
            &crate::physical::SelectedDomains::new([]).unwrap(),
            None
        )
        .is_err(),
        "dropping an empty universal guard must not hide an infinite producer cycle"
    );
    let cut = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&data).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        Some(8),
    )
    .unwrap();
    assert_eq!(cut.status, crate::seam::BudgetStatus::Exhausted);
}

#[test]
fn empty_intersection_admits_active_resource_terms_without_inventing_literal_subjects() {
    let data = source(vec![
        fact("urn:class", INTERSECTION, NIL),
        fact("urn:x", "urn:relation", "urn:y"),
        RdfQuad::new(
            RdfTerm::iri("urn:x"),
            "urn:label",
            RdfTerm::Literal(RdfLiteral::simple("literal")),
        ),
    ]);
    let prepared = crate::program_analysis::prepare_program(&member_consumer()).unwrap();
    let result = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&data).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    for subject in ["urn:x", "urn:y", "urn:relation"] {
        assert!(
            result
                .inferred
                .iter()
                .any(|row| row.subject == subject && row.predicate == "urn:done"),
            "missing empty-intersection member {subject}"
        );
    }
}
