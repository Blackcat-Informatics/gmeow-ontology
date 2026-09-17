// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native rule binding, witness and retraction contracts over synthetic GMEOW rules.

use purrdf::{BlankScope, RdfTextDirection, TermValue};

use super::chase::{ExistentialRule, WitnessPolicy, chase_world_explained};
use super::incremental::SignedFact;
use super::incremental_grounding::IncrementalGroundProgram;
use super::plan::Parsed;
use super::seminaive::{NativeOutcome, materialize_native};
use crate::rule_ir::{
    EvalAtom, EvalRule, EvalTerm, Fact, FactStore, least_model_of_reduct, world_edb_facts,
};

const WORLD: &str = "urn:bindings:world";

fn values() -> Vec<TermValue> {
    let blank = |scope| TermValue::Blank {
        label: "source".to_owned(),
        scope: BlankScope(scope),
    };
    let directional = TermValue::Literal {
        lexical_form: "quoted \"claim\"\n<urn:value>".to_owned(),
        datatype: "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString".to_owned(),
        language: Some("ar".to_owned()),
        direction: Some(RdfTextDirection::Rtl),
    };
    vec![
        blank(17),
        blank(19),
        TermValue::simple_literal("<urn:value>"),
        directional.clone(),
        TermValue::Triple {
            s: Box::new(blank(17)),
            p: Box::new(TermValue::iri("urn:quotes")),
            o: Box::new(TermValue::Triple {
                s: Box::new(blank(19)),
                p: Box::new(TermValue::iri("urn:states")),
                o: Box::new(directional),
            }),
        },
    ]
}

fn atom(subject: &str, predicate: &str, object: &str) -> EvalAtom {
    EvalAtom::positive(EvalTerm::var(subject), predicate, EvalTerm::var(object))
}

fn facts() -> Vec<Fact> {
    values()
        .into_iter()
        .map(|object| Fact {
            subject: TermValue::iri("urn:claim"),
            predicate: "urn:source".to_owned(),
            object,
        })
        .collect()
}

fn copy_rule() -> EvalRule {
    EvalRule::positive(
        "urn:copy",
        atom("?s", "urn:copy", "?o"),
        vec![atom("?s", "urn:source", "?o")],
    )
}

#[test]
fn forward_and_reduct_keep_native_objects_worlds_and_exact_premises() {
    let store = crate::store::WorldStore::new();
    for fact in facts() {
        store
            .insert_quad_terms(
                WORLD,
                fact.subject,
                TermValue::iri(fact.predicate),
                fact.object,
            )
            .unwrap();
    }
    store.insert_quad(
        "urn:other-world",
        "urn:foreign",
        "urn:source",
        "urn:foreign",
    );
    let rules = [copy_rule()];
    let executable = Parsed::uncached(&rules)
        .stratify()
        .unwrap()
        .plan()
        .into_executable();
    let NativeOutcome::Decided(result) = materialize_native(&store, &executable, None).unwrap()
    else {
        panic!("the finite copy rule must be admitted");
    };
    let mut edb = FactStore::new();
    for fact in world_edb_facts(&store, WORLD).unwrap() {
        edb.insert(fact);
    }
    let reduct = least_model_of_reduct(&edb, &rules, &FactStore::new()).unwrap();
    for rows in [&result.rows, &reduct.derivations] {
        let selected: Vec<_> = rows
            .iter()
            .filter(|row| row.predicate == "urn:copy" && row.subject == TermValue::iri("urn:claim"))
            .collect();
        assert_eq!(selected.len(), values().len());
        for expected in facts() {
            let row = selected
                .iter()
                .find(|row| row.object == expected.object)
                .unwrap();
            assert_eq!(row.antecedents.len(), 1);
            assert_eq!(row.antecedents[0].object, expected.object);
            assert_eq!(row.source_quad_ids, [expected.reifier().unwrap()]);
        }
    }
    assert!(
        result
            .rows
            .iter()
            .filter(|row| row.subject == TermValue::iri("urn:claim"))
            .all(|row| row.graph == WORLD)
    );
}

#[test]
fn indexed_join_and_negation_distinguish_native_source_terms() {
    let store = crate::store::WorldStore::new();
    let values = values();
    for object in &values {
        for predicate in ["urn:source", "urn:corroborates"] {
            store
                .insert_quad_terms(
                    WORLD,
                    TermValue::iri("urn:claim"),
                    TermValue::iri(predicate),
                    object.clone(),
                )
                .unwrap();
        }
    }
    store
        .insert_quad_terms(
            WORLD,
            TermValue::iri("urn:claim"),
            TermValue::iri("urn:rejected"),
            values[0].clone(),
        )
        .unwrap();
    let mut rejected = atom("?s", "urn:rejected", "?o");
    rejected.negated = true;
    let rules = [EvalRule::positive(
        "urn:corroborated-copy",
        atom("?s", "urn:copy", "?o"),
        vec![
            atom("?s", "urn:source", "?o"),
            atom("?s", "urn:corroborates", "?o"),
            rejected,
        ],
    )];
    let executable = Parsed::uncached(&rules)
        .stratify()
        .unwrap()
        .plan()
        .into_executable();
    let NativeOutcome::Decided(result) = materialize_native(&store, &executable, None).unwrap()
    else {
        panic!("the stratified join must be admitted");
    };
    let copied: Vec<_> = result
        .rows
        .iter()
        .filter(|row| row.predicate == "urn:copy")
        .collect();
    assert_eq!(copied.len(), values.len() - 1);
    assert!(!copied.iter().any(|row| row.object == values[0]));
    for value in &values[1..] {
        let row = copied.iter().find(|row| &row.object == value).unwrap();
        assert_eq!(row.antecedents.len(), 2);
        assert_eq!(row.antecedents[0].predicate, "urn:source");
        assert_eq!(row.antecedents[1].predicate, "urn:corroborates");
        assert!(row.antecedents.iter().all(|fact| &fact.object == value));
    }
}

#[test]
fn incremental_grounding_preserves_native_constants_across_retraction() {
    let rules = [copy_rule()];
    let mut program = IncrementalGroundProgram::new("native-bindings", facts(), &rules).unwrap();
    for expected in values() {
        assert!(
            program
                .snapshot()
                .rules
                .iter()
                .any(|rule| { rule.head.object == EvalTerm::ConstLit(expected.clone()) })
        );
    }
    let removed = facts().remove(0);
    let delta = program
        .apply([SignedFact {
            fact: removed.clone(),
            weight: -1,
        }])
        .unwrap();
    assert_eq!(delta.rule_changes.len(), 1);
    assert_eq!(delta.rule_changes[0].weight, -1);
    assert_eq!(
        delta.rule_changes[0].rule.head.object,
        EvalTerm::ConstLit(removed.object.clone())
    );
    assert!(
        !program
            .snapshot()
            .rules
            .iter()
            .any(|rule| { rule.head.object == EvalTerm::ConstLit(removed.object.clone()) })
    );
    assert!(
        program
            .snapshot()
            .rules
            .iter()
            .any(|rule| { rule.head.object == EvalTerm::ConstLit(values().remove(1)) }),
        "the same blank label in another scope remains live"
    );
    program.check_scratch_parity().unwrap();
    program
        .apply([SignedFact {
            fact: removed,
            weight: 1,
        }])
        .unwrap();
    program.check_scratch_parity().unwrap();
    assert_eq!(program.snapshot().rules.len(), values().len());
}

#[test]
fn existential_witnesses_retain_native_frontiers_and_source_evidence() {
    let rule = ExistentialRule {
        numeric: Vec::new(),
        rule_iri: "urn:witness-rule".to_owned(),
        body: vec![atom("?s", "urn:source", "?o")],
        head: vec![atom("?w", "urn:witness-value", "?o")],
        distinct: Vec::new(),
        witness_frontier: None,
        witness_policy: WitnessPolicy::FrontierSkolem,
    };
    let (result, registry) = chase_world_explained(WORLD, &facts(), &[rule], None).unwrap();
    let NativeOutcome::Decided(result) = result else {
        panic!("the acyclic existential rule must be admitted");
    };
    let derived: Vec<_> = result
        .rows
        .iter()
        .filter(|row| row.predicate == "urn:witness-value")
        .collect();
    assert_eq!(derived.len(), values().len());
    let mut witnesses = std::collections::BTreeSet::new();
    for source in facts() {
        let row = derived
            .iter()
            .find(|row| row.object == source.object)
            .unwrap();
        let witness = row.subject.as_iri().unwrap();
        assert!(witnesses.insert(witness));
        let explanation = registry.explain(witness).unwrap();
        assert_eq!(
            explanation.frontier.as_slice(),
            std::slice::from_ref(&source.object)
        );
        assert_eq!(row.antecedents.len(), 1);
        assert_eq!(row.antecedents[0].object, source.object);
        assert_eq!(row.source_quad_ids, [source.reifier().unwrap()]);
        assert_eq!(row.graph, WORLD);
    }
}
