// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Frozen-round carry admission and committed witness publication controls.

use super::*;
use crate::native_semantics::SemanticVocabulary;
use crate::physical::store::{RelationStore, SkolemTerm, WitnessContract};
use crate::provenance::{ProofHeight, mint_derivation_id};

const WORLD: &str = "urn:retained:world";
const OTHER: &str = "urn:retained:other";

fn fact(predicate: &str) -> Fact {
    Fact {
        subject: TermValue::iri("urn:retained:subject"),
        predicate: predicate.to_owned(),
        object: TermValue::iri("urn:retained:object"),
    }
}

fn runtime(world: &str, facts: &[Fact]) -> WorldRuntime {
    let family = super::super::families::State::relational(
        world,
        facts,
        SemanticVocabulary::Exact,
        None,
        false,
    )
    .unwrap();
    WorldRuntime::new(facts, SemanticVocabulary::Exact, family).unwrap()
}

fn row(head: &Fact, premise: &Fact, height: u32) -> DerivedRow {
    let source = premise.reifier().unwrap();
    DerivedRow {
        graph: WORLD.to_owned(),
        subject: head.subject.clone(),
        predicate: head.predicate.clone(),
        object: head.object.clone(),
        rule_iri: "urn:retained:rule".to_owned(),
        source_quad_ids: vec![source.clone()],
        derivation_id: mint_derivation_id("urn:retained:rule", &[&source]),
        proof_height: ProofHeight::new(height).unwrap(),
        antecedents: vec![premise.clone()],
        cross_world: None,
    }
}

#[test]
fn carry_candidates_observe_one_frozen_world_and_current_premise_depths() {
    let a = fact("urn:retained:a");
    let b = fact("urn:retained:b");
    let c = fact("urn:retained:c");
    let mut reuse = Reuse {
        by_stratum: BTreeMap::from([
            (0, vec![row(&b, &a, 9), row(&c, &b, 10)]),
            (1, vec![row(&fact("urn:retained:later"), &a, 9)]),
        ]),
        registry: SkolemRegistry::new(),
        introductions: BTreeMap::new(),
        count: 0,
    };
    let runtimes = BTreeMap::from([
        (WORLD.to_owned(), runtime(WORLD, std::slice::from_ref(&a))),
        (OTHER.to_owned(), runtime(OTHER, std::slice::from_ref(&b))),
    ]);
    let mut rounds = BTreeMap::from([
        (WORLD.to_owned(), RoundCandidateBuffer::new()),
        (OTHER.to_owned(), RoundCandidateBuffer::new()),
    ]);
    let eligible = reuse.gather(0, &runtimes, &mut rounds).unwrap();
    assert_eq!(
        eligible,
        BTreeMap::from([(WORLD.to_owned(), BTreeSet::from([b.key()]))])
    );
    let candidates = &rounds[WORLD].entries;
    assert_eq!(candidates.len(), 1);
    let provenance = candidates[0].1.prov.as_ref().unwrap();
    assert_eq!(provenance.proof_height.get(), 1);
    assert_eq!(provenance.sum_src_depth, 0);
    assert_eq!(provenance.source_facts, vec![a.clone()]);
    assert_eq!(provenance.sources, vec![a.reifier().unwrap()]);
    assert!(rounds[OTHER].entries.is_empty());
    assert!(!runtimes[WORLD].store.contains_key(&b.key()));
    assert!(!runtimes[WORLD].store.contains_key(&c.key()));
    assert_eq!(
        reuse.count(),
        0,
        "offering a proof is not a committed reuse"
    );
    assert_eq!(reuse.by_stratum[&0].len(), 2);
    assert_eq!(reuse.by_stratum[&1].len(), 1);
}

#[test]
fn introductions_wait_for_their_world_premises_and_reached_producer() {
    let scope = WitnessContract::native(SemanticVocabulary::Exact).scope(WORLD, [7; 32]);
    let mut prior = SkolemRegistry::new();
    let value = prior.mint(SkolemTerm {
        scope: scope.clone(),
        rule_iri: "urn:retained:mint".to_owned(),
        ordinal: 0,
        frontier: vec![],
    });
    let first = Fact {
        object: value.clone(),
        ..fact("urn:retained:first")
    };
    let second = Fact {
        object: value.clone(),
        ..fact("urn:retained:second")
    };
    let a = fact("urn:retained:a");
    let b = fact("urn:retained:b");
    prior
        .record_head(
            &scope,
            std::slice::from_ref(&value),
            &first,
            std::slice::from_ref(&a),
        )
        .unwrap();
    prior
        .record_head(
            &scope,
            std::slice::from_ref(&value),
            &second,
            std::slice::from_ref(&b),
        )
        .unwrap();
    let mut complete = RelationStore::new();
    for fact in [&first, &second, &a, &b] {
        complete.insert(&fact.predicate, &fact.subject, &fact.object);
    }
    prior.commit_heads(WORLD, &complete);
    let witness = value.as_iri().unwrap();
    let receipt = prior.explain(witness).unwrap();
    assert_eq!(receipt.heads.len(), 2);
    let mut registry = prior.recipes_only();
    let mut reuse = Reuse {
        by_stratum: BTreeMap::new(),
        registry: prior.recipes_only(),
        introductions: BTreeMap::from([(1, vec![receipt])]),
        count: 0,
    };
    // These tiny stores model successive committed snapshots. A statement in
    // another world cannot make the owning world's pending receipt eligible.
    let mut runtimes = BTreeMap::from([
        (
            WORLD.to_owned(),
            runtime(WORLD, &[first.clone(), a.clone(), second.clone()]),
        ),
        (OTHER.to_owned(), runtime(OTHER, std::slice::from_ref(&b))),
    ]);
    reuse
        .install_ready_introductions(0, &runtimes, &mut registry)
        .unwrap();
    assert!(registry.explain(witness).is_none());
    reuse
        .install_ready_introductions(1, &runtimes, &mut registry)
        .unwrap();
    let partial = registry.explain(witness).unwrap();
    assert_eq!(partial.heads.len(), 1);
    assert_eq!(
        partial.heads[0].statement,
        crate::physical::WitnessStatement::from(&first)
    );
    partial.validate().unwrap();
    assert_eq!(reuse.introductions[&1][0].heads.len(), 1);
    runtimes.insert(WORLD.to_owned(), runtime(WORLD, &[first, a, second, b]));
    reuse
        .install_ready_introductions(1, &runtimes, &mut registry)
        .unwrap();
    let complete = registry.explain(witness).unwrap();
    assert_eq!(complete.heads.len(), 2);
    complete.validate().unwrap();
    assert!(reuse.introductions.is_empty());
}
