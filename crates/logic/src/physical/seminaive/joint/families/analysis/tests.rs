// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::physical::{LogicalListCache, RelationStore, SchemaValues, SelectedDomains};
use crate::reason::refute::RefutationPremise;
use crate::reason::refute::native::{NativeEvidenceIndex, NativeSourceTerms};
use crate::rule_ir::FactStore;
use purrdf::TermValue;
use std::sync::Arc;

const WORLD: &str = "urn:family-observation:world";

struct World {
    store: FactStore,
    rel: RelationStore,
    rows: Vec<crate::rule_ir::DerivedRow>,
    evidence: NativeEvidenceIndex,
    ledger: NativeFamilyLedger,
    coverage: SourceCoverageWorld,
    coverage_rows: usize,
    values: SchemaValues,
    lists: LogicalListCache,
    observations: Observations,
}

impl World {
    fn new(facts: &[Fact], allowance: Option<u64>) -> Self {
        let mut store = FactStore::new();
        let mut rel = RelationStore::with_semantics(
            crate::native_semantics::SemanticVocabulary::GroundedLogicV1,
        );
        for fact in facts {
            store.insert(fact.clone());
            rel.insert(&fact.predicate, &fact.subject, &fact.object);
        }
        let sources: Arc<[_]> = facts
            .iter()
            .map(|fact| RefutationPremise {
                subject: fact.subject.clone(),
                predicate: fact.predicate.clone(),
                object: fact.object.clone(),
                graph: None,
            })
            .collect::<Vec<_>>()
            .into();
        let evidence = NativeEvidenceIndex::from_native_facts(
            WORLD.into(),
            [17; 32],
            sources,
            &store,
            &SelectedDomains::new([]).unwrap(),
        )
        .unwrap();
        let mut ledger = NativeFamilyLedger::new(WORLD.into(), None, [17; 32], allowance);
        ledger.source_terms = NativeSourceTerms::NativeFacts;
        Self {
            store,
            rel,
            evidence,
            ledger,
            rows: Vec::new(),
            coverage_rows: 0,
            coverage: SourceCoverageWorld {
                graph: None,
                constructs: Vec::new(),
                admissions: Vec::new(),
            },
            values: SchemaValues::default(),
            lists: LogicalListCache::default(),
            observations: Observations::default(),
        }
    }

    fn evaluate(&mut self, family: NativeRefutationFamily, complete: &BTreeSet<NativeRead>) {
        let input =
            NativeFamilyInput::new(&self.store, &self.rel, &self.rows, &self.evidence, complete)
                .unwrap();
        for fact in &self.store.facts()[self.coverage_rows..] {
            crate::reason::dl::observe_construct(
                fact,
                &input,
                &mut self.ledger,
                &mut self.coverage,
            )
            .unwrap();
        }
        self.coverage_rows = self.store.row_count();
        crate::reason::dl::admit_source_constructs(
            &input,
            &mut self.values,
            &mut self.lists,
            &mut self.ledger,
            &mut self.coverage,
        )
        .unwrap();
        let input = input.with_admissions(&self.coverage);
        self.observations
            .evaluate(
                &[family],
                &input,
                &self.coverage,
                &mut self.values,
                &mut self.lists,
                &mut self.ledger,
            )
            .unwrap();
    }

    fn append(&mut self, fact: Fact) {
        let premise = self.store.facts()[0].clone();
        let source = premise.reifier().unwrap();
        assert!(self.store.insert(fact.clone()).is_some());
        self.rel
            .insert(&fact.predicate, &fact.subject, &fact.object);
        self.rows.push(crate::rule_ir::DerivedRow {
            graph: WORLD.into(),
            subject: fact.subject,
            predicate: fact.predicate,
            object: fact.object,
            rule_iri: "urn:family-observation:derived".into(),
            derivation_id: crate::provenance::mint_derivation_id(
                "urn:family-observation:derived",
                &[&source],
            ),
            source_quad_ids: vec![source],
            antecedents: vec![premise],
            proof_height: crate::provenance::ProofHeight::new(1).unwrap(),
            cross_world: None,
        });
        self.evidence.observe(&self.store, &self.rows, &[]).unwrap();
    }
}

fn fact(subject: &str, predicate: &str, object: TermValue) -> Fact {
    Fact {
        subject: TermValue::iri(subject),
        predicate: predicate.into(),
        object,
    }
}

fn complete() -> BTreeSet<NativeRead> {
    BTreeSet::from([NativeRead {
        marker: None,
        predicate: None,
        kind: NativeReadKind::Completed,
    }])
}

fn prepared(family: NativeRefutationFamily) -> BTreeSet<NativeRead> {
    Observation::new(family)
        .preparation
        .into_iter()
        .chain(crate::reason::dl::source_admission_reads())
        .collect()
}

#[test]
fn completed_observation_preserves_paid_identity_proofs_at_exact_allowance() {
    let family = NativeRefutationFamily::Identity;
    let mut world = World::new(
        &[fact(
            "urn:x",
            &owl("differentFrom"),
            TermValue::iri("urn:x"),
        )],
        Some(1),
    );
    world.evaluate(family, &prepared(family));
    assert_eq!(world.ledger.work.consumed, 1);
    let prior = world.ledger.outcomes[0].clone();
    assert!(matches!(
        prior.completion,
        NativeFamilyCompletion::Awaiting { .. }
    ));
    assert_eq!(prior.conclusions.len(), 1);
    let proofs = world.ledger.proofs.clone();
    world.evaluate(family, &complete());
    assert_eq!(world.ledger.work.consumed, 1);
    assert!(!world.ledger.work.exhausted);
    assert_eq!(
        world.ledger.outcomes[0].completion,
        NativeFamilyCompletion::Complete
    );
    assert_eq!(world.ledger.outcomes[0].conclusions, prior.conclusions);
    assert_eq!(world.ledger.proofs, proofs);
}

#[test]
fn unchanged_obstruction_keeps_its_exact_support_without_recharging() {
    let family = NativeRefutationFamily::Identity;
    let opaque = TermValue::Literal {
        lexical_form: "opaque".into(),
        datatype: "urn:unknown-datatype".into(),
        language: None,
        direction: None,
    };
    let mut world = World::new(&[fact("urn:x", &owl("differentFrom"), opaque)], Some(1));
    world.evaluate(family, &prepared(family));
    let prior = world.ledger.outcomes.clone();
    assert_eq!(prior[0].completion, NativeFamilyCompletion::Obstructed);
    assert!(!prior[0].obstructions.is_empty());
    assert!(
        prior[0]
            .obstructions
            .iter()
            .all(|obstruction| !obstruction.support.is_empty())
    );
    world.evaluate(family, &complete());
    assert_eq!(world.ledger.outcomes, prior);
    assert_eq!(world.ledger.work.consumed, 1);
    assert!(!world.ledger.work.exhausted);
}

#[test]
fn completion_preserves_not_engaged_separately_from_engaged_complete() {
    let family = NativeRefutationFamily::Identity;
    for (facts, expected) in [
        (vec![], NativeFamilyCompletion::NotEngaged),
        (
            vec![fact(
                "urn:p",
                TYPE,
                TermValue::iri(owl("InverseFunctionalProperty")),
            )],
            NativeFamilyCompletion::Complete,
        ),
    ] {
        let mut world = World::new(&facts, None);
        world.evaluate(family, &prepared(family));
        assert!(matches!(
            world.ledger.outcomes[0].completion,
            NativeFamilyCompletion::Awaiting { .. }
        ));
        let consumed = world.ledger.work.consumed;
        world.evaluate(family, &complete());
        assert_eq!(world.ledger.outcomes[0].completion, expected);
        assert_eq!(world.ledger.work.consumed, consumed);
    }
}

#[test]
fn pending_source_preparation_is_not_certified_by_cached_observation() {
    let family = NativeRefutationFamily::HasSelf;
    let mut world = World::new(
        &[
            fact(
                "urn:r",
                &owl("hasSelf"),
                TermValue::Literal {
                    lexical_form: "true".into(),
                    datatype: "http://www.w3.org/2001/XMLSchema#boolean".into(),
                    language: None,
                    direction: None,
                },
            ),
            fact("urn:r", &owl("onProperty"), TermValue::iri("urn:p")),
        ],
        None,
    );
    world.evaluate(family, &BTreeSet::new());
    let prior = world.ledger.outcomes.clone();
    assert!(prior.iter().any(|outcome| matches!(&outcome.completion,
        NativeFamilyCompletion::Awaiting { reads } if reads.iter().any(|read| read.predicate.is_some()))));
    let consumed = world.ledger.work.consumed;
    world.evaluate(family, &BTreeSet::new());
    assert_eq!(world.ledger.outcomes, prior);
    assert_eq!(world.ledger.work.consumed, consumed);
    world.evaluate(family, &complete());
    assert!(
        world.ledger.work.consumed > consumed,
        "newly completed source preparation performs its required work"
    );
    assert!(
        world
            .ledger
            .outcomes
            .iter()
            .all(|outcome| outcome.completion == NativeFamilyCompletion::Complete)
    );
    assert!(
        world
            .coverage
            .admissions
            .iter()
            .all(|admission| admission.completion == NativeFamilyCompletion::Complete)
    );
}

#[test]
fn identity_engagement_and_unrelated_metadata_share_the_declared_read_boundary() {
    let observation = Observation::new(NativeRefutationFamily::Identity);
    for marker in [
        "FunctionalProperty",
        "InverseFunctionalProperty",
        "AllDifferent",
    ] {
        let fact = fact("urn:p", TYPE, TermValue::iri(owl(marker)));
        assert!(observation.reads.iter().any(|read| read.matches_fact(
            &fact,
            crate::native_semantics::SemanticVocabulary::GroundedLogicV1
        )));
    }
    let metadata = fact(
        "urn:assessment",
        "urn:metadata:label",
        TermValue::iri("urn:description"),
    );
    assert!(!observation.reads.iter().any(|read| read.matches_fact(
        &metadata,
        crate::native_semantics::SemanticVocabulary::GroundedLogicV1
    )));
}

#[test]
fn new_relevant_rows_invalidate_but_unrelated_metadata_reuses_the_analysis() {
    let family = NativeRefutationFamily::Identity;
    let mut world = World::new(
        &[fact(
            "urn:x",
            &owl("differentFrom"),
            TermValue::iri("urn:x"),
        )],
        None,
    );
    world.evaluate(family, &prepared(family));
    let prior = world.ledger.outcomes.clone();
    assert_eq!(world.ledger.work.consumed, 1);
    world.append(fact(
        "urn:assessment",
        "urn:metadata:label",
        TermValue::iri("urn:description"),
    ));
    world.evaluate(family, &prepared(family));
    assert_eq!(world.ledger.outcomes, prior);
    assert_eq!(world.ledger.work.consumed, 1);
    world.append(fact(
        "urn:y",
        &owl("differentFrom"),
        TermValue::iri("urn:y"),
    ));
    world.evaluate(family, &complete());
    assert_eq!(world.ledger.work.consumed, 3);
    assert_eq!(world.ledger.outcomes[0].conclusions.len(), 2);
    assert_eq!(
        world.ledger.outcomes[0].completion,
        NativeFamilyCompletion::Complete
    );
    assert!(
        world.ledger.outcomes[0]
            .conclusions
            .iter()
            .all(|conclusion| !conclusion.support.is_empty())
    );
}

#[test]
fn partial_analysis_of_new_facts_keeps_earlier_supported_clashes() {
    let family = NativeRefutationFamily::Identity;
    let mut world = World::new(
        &[fact(
            "urn:z",
            &owl("differentFrom"),
            TermValue::iri("urn:z"),
        )],
        Some(2),
    );
    world.evaluate(family, &prepared(family));
    let prior = world.ledger.outcomes[0].conclusions[0].clone();
    assert_eq!(world.ledger.work.consumed, 1);
    assert!(!world.ledger.work.exhausted);
    // The new lexically earlier obligation consumes the remaining analysis step;
    // the old obligation is now after the cut and cannot be recomputed.
    world.append(fact(
        "urn:a",
        &owl("differentFrom"),
        TermValue::iri("urn:a"),
    ));
    world.evaluate(family, &complete());
    assert_eq!(world.ledger.work.consumed, 2);
    assert!(world.ledger.work.exhausted);
    assert!(!world.ledger.complete());
    let outcome = &world.ledger.outcomes[0];
    assert_eq!(outcome.completion, NativeFamilyCompletion::Exhausted);
    assert_eq!(outcome.conclusions.len(), 2);
    assert!(outcome.conclusions.contains(&prior));
    assert!(
        outcome
            .conclusions
            .iter()
            .any(|clash| clash.subject == TermValue::iri("urn:a"))
    );
    assert!(outcome.bounds.is_empty());
    assert!(outcome.obstructions.is_empty());
    assert!(
        prior
            .support
            .iter()
            .all(|id| world.ledger.proofs.iter().any(|proof| proof.id == *id))
    );
    world.ledger.validate().unwrap();
}
