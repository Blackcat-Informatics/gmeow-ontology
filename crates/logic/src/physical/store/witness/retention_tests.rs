// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Synthetic proof-ledger controls independent of any repository corpus.

use super::*;
use crate::provenance::{ASSERT_RULE_IRI, ProofHeight};

fn fact(subject: &str, predicate: &str, object: &str) -> Fact {
    Fact {
        subject: TermValue::iri(subject),
        predicate: predicate.to_owned(),
        object: TermValue::iri(object),
    }
}

fn store(facts: &[&Fact]) -> RelationStore {
    let mut store = RelationStore::new();
    for fact in facts {
        store.insert(&fact.predicate, &fact.subject, &fact.object);
    }
    store
}

fn row(world: &str, fact: &Fact, rule: &str, premises: &[Fact]) -> DerivedRow {
    let source_quad_ids = premises
        .iter()
        .map(|fact| fact.reifier().unwrap())
        .collect::<Vec<_>>();
    let ids = source_quad_ids
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    DerivedRow {
        cross_world: None,
        graph: world.to_owned(),
        subject: fact.subject.clone(),
        predicate: fact.predicate.clone(),
        object: fact.object.clone(),
        rule_iri: rule.to_owned(),
        derivation_id: crate::provenance::mint_derivation_id(rule, &ids),
        source_quad_ids,
        proof_height: ProofHeight::new(u32::from(rule != ASSERT_RULE_IRI)).unwrap(),
        antecedents: premises.to_vec(),
    }
}

fn recipe(world: &str) -> SkolemTerm {
    SkolemTerm {
        scope: WitnessContract::native(
            crate::native_semantics::SemanticVocabulary::GroundedLogicV1,
        )
        .scope(world, [7; 32]),
        rule_iri: "urn:mint".to_owned(),
        ordinal: 0,
        frontier: vec![TermValue::iri("urn:x")],
    }
}

fn mint(registry: &mut SkolemRegistry, world: &str, premises: &[Fact]) -> (String, Fact) {
    let recipe = recipe(world);
    let witness = registry.mint(recipe.clone());
    let head = Fact {
        subject: TermValue::iri("urn:x"),
        predicate: "urn:edge".to_owned(),
        object: witness.clone(),
    };
    registry
        .record_head(&recipe.scope, &[witness.clone()], &head, premises)
        .unwrap();
    (witness.as_iri().unwrap().to_owned(), head)
}

#[test]
fn competing_winner_preserves_both_introductions_and_the_surviving_alternative() {
    let world = "urn:world";
    let a = fact("urn:x", "urn:premise", "urn:a");
    let b = fact("urn:x", "urn:premise", "urn:b");
    let mut registry = SkolemRegistry::new();
    let (witness, head) = mint(&mut registry, world, std::slice::from_ref(&a));
    mint(&mut registry, world, std::slice::from_ref(&b));
    // A competing ordinary proof won the tuple's deterministic tie-break.
    let winner = row(world, &head, "urn:ordinary", std::slice::from_ref(&b));
    registry.commit_heads(world, &store(&[&head, &a, &b]));
    registry.retain_committed_facts(&[
        winner.clone(),
        row(world, &a, ASSERT_RULE_IRI, &[]),
        row(world, &b, ASSERT_RULE_IRI, &[]),
    ]);
    let introduction = registry.explain(&witness).unwrap();
    assert_eq!(introduction.heads.len(), 2);
    introduction.validate().unwrap();
    assert!(
        introduction
            .heads
            .iter()
            .all(|head| head.derivation_id != winner.derivation_id)
    );

    registry.retain_committed_facts(&[winner, row(world, &b, ASSERT_RULE_IRI, &[])]);
    let surviving = registry.explain(&witness).unwrap();
    assert_eq!(surviving.heads.len(), 1);
    assert_eq!(
        surviving.heads[0].premises,
        vec![WitnessStatement::from(&b)]
    );
    surviving.validate().unwrap();
}

#[test]
fn one_world_commit_cannot_erase_another_worlds_pending_introduction() {
    let mut registry = SkolemRegistry::new();
    let premise = fact("urn:x", "urn:premise", "urn:a");
    let (a, _) = mint(&mut registry, "urn:a", std::slice::from_ref(&premise));
    let (b, head_b) = mint(&mut registry, "urn:b", std::slice::from_ref(&premise));
    registry.commit_heads("urn:a", &RelationStore::new());
    assert!(registry.explain(&a).is_none());
    registry.commit_heads("urn:b", &store(&[&head_b, &premise]));
    registry.explain(&b).unwrap().validate().unwrap();
    assert!(registry.pending.is_empty());
}

#[test]
fn later_introductions_for_an_existing_head_keep_ordered_premises_without_new_rows() {
    let world = "urn:world";
    let a = fact("urn:x", "urn:premise", "urn:a");
    let b = fact("urn:x", "urn:premise", "urn:b");
    let mut registry = SkolemRegistry::new();
    let (witness, head) = mint(&mut registry, world, std::slice::from_ref(&a));
    let committed = store(&[&head, &a, &b]);
    registry.commit_heads(world, &committed);
    assert_eq!(registry.explain(&witness).unwrap().heads.len(), 1);

    // The old head never enters a later candidate buffer, but both firings
    // still provide independent ordered introduction evidence.
    mint(&mut registry, world, &[a.clone(), b.clone()]);
    mint(&mut registry, world, &[b.clone(), a.clone()]);
    mint(&mut registry, world, &[a.clone(), b.clone()]);
    registry.commit_heads(world, &committed);
    let receipt = registry.explain(&witness).unwrap();
    assert_eq!(receipt.heads.len(), 3);
    let expected: BTreeSet<_> = [
        vec![WitnessStatement::from(&a)],
        vec![WitnessStatement::from(&a), WitnessStatement::from(&b)],
        vec![WitnessStatement::from(&b), WitnessStatement::from(&a)],
    ]
    .into_iter()
    .collect();
    assert_eq!(
        receipt
            .heads
            .iter()
            .map(|head| head.premises.clone())
            .collect::<BTreeSet<_>>(),
        expected
    );
    assert_eq!(
        receipt
            .heads
            .iter()
            .map(|head| &head.derivation_id)
            .collect::<BTreeSet<_>>()
            .len(),
        2,
        "the public derivation id is order-independent; the full receipt keeps premise order"
    );
    receipt.validate().unwrap();
    assert!(registry.pending.is_empty());

    // Re-observation changes neither the store nor the published receipt.
    registry.commit_heads(world, &committed);
    assert_eq!(registry.explain(&witness), Some(receipt));
}

#[test]
fn committed_cut_requires_the_exact_head_and_every_native_premise() {
    let world = "urn:world";
    let a = fact("urn:x", "urn:premise", "urn:a");
    let b = Fact {
        subject: TermValue::iri("urn:x"),
        predicate: "urn:label".to_owned(),
        object: TermValue::Literal {
            lexical_form: "same-text".to_owned(),
            datatype: "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString".to_owned(),
            language: Some("en".to_owned()),
            direction: None,
        },
    };
    let mut other_language = b.clone();
    if let TermValue::Literal { language, .. } = &mut other_language.object {
        *language = Some("fr".to_owned());
    }
    let mut registry = SkolemRegistry::new();
    let (witness, head) = mint(&mut registry, world, std::slice::from_ref(&a));
    mint(&mut registry, world, &[a.clone(), b.clone()]);
    let mut uncommitted_head = head.clone();
    uncommitted_head.predicate = "urn:outside-cut".to_owned();
    registry
        .record_head(
            &recipe(world).scope,
            &[TermValue::iri(&witness)],
            &uncommitted_head,
            std::slice::from_ref(&a),
        )
        .unwrap();

    registry.commit_heads(world, &store(&[&head, &a, &other_language]));
    let receipt = registry.explain(&witness).unwrap();
    assert_eq!(receipt.heads.len(), 1);
    assert_eq!(receipt.heads[0].statement, WitnessStatement::from(&head));
    assert_eq!(receipt.heads[0].premises, vec![WitnessStatement::from(&a)]);
    assert!(registry.pending.is_empty());

    // Publishing the missing premise later cannot resurrect a discarded firing.
    registry.commit_heads(world, &store(&[&head, &a, &b, &uncommitted_head]));
    assert_eq!(registry.explain(&witness), Some(receipt));
}

#[test]
fn retained_introductions_wait_for_the_producer_and_every_committed_premise() {
    let world = "urn:world";
    let premise = fact("urn:x", "urn:premise", "urn:a");
    let mut registry = SkolemRegistry::new();
    let (witness, head) = mint(&mut registry, world, std::slice::from_ref(&premise));
    registry.commit_heads(world, &store(&[&head, &premise]));
    let producers = BTreeMap::from([((world.to_owned(), "urn:mint".to_owned()), 3)]);
    let facts = BTreeMap::from([(
        world.to_owned(),
        BTreeMap::from([(premise.key(), 4), (head.key(), 2)]),
    )]);
    let derived = BTreeSet::from([(world.to_owned(), head.key())]);
    let mut scheduled = registry.retained_introductions(&producers, &facts, &derived);
    assert_eq!(scheduled.keys().copied().collect::<Vec<_>>(), vec![4]);
    assert!(
        registry
            .retained_introductions(&BTreeMap::new(), &facts, &derived)
            .is_empty()
    );
    assert!(
        registry
            .retained_introductions(&producers, &facts, &BTreeSet::new())
            .is_empty()
    );

    // A budget cut before rank 4 can expose allocation recipes but no receipt.
    let mut restored = registry.recipes_only();
    assert!(restored.recipe(&witness).is_some());
    assert!(restored.explain(&witness).is_none());
    for rank in 0..4 {
        restored
            .install_introductions(&scheduled.remove(&rank).unwrap_or_default(), |_, _| true)
            .unwrap();
        assert!(restored.explain(&witness).is_none());
    }
    let introductions = scheduled.remove(&4).unwrap();
    assert!(
        restored
            .install_introductions(&introductions, |_, key| key != &premise.key())
            .is_err()
    );
    assert!(
        restored.explain(&witness).is_none(),
        "a refused receipt has no partial publication"
    );
    restored
        .install_introductions(&introductions, |owner, key| {
            owner == world && facts[world].contains_key(key)
        })
        .unwrap();
    assert_eq!(restored.explain(&witness), registry.explain(&witness));

    let mut wrong_scope = introductions;
    wrong_scope[0].scope.world = "urn:foreign-world".to_owned();
    assert!(
        registry
            .recipes_only()
            .install_introductions(&wrong_scope, |_, _| true)
            .is_err()
    );
}
