// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tiny native-store controls for retained class evidence and completion.

use super::*;
use crate::physical::{RelationStore, SelectedDomains};
use crate::reason::refute::native::NativeEvidenceIndex;
use crate::reason::refute::{NativeProofId, RefutationPremise, RefutationProof};
use crate::rule_ir::FactStore;
use std::sync::Arc;

const WORLD: &str = "urn:class-native:world";
const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const COMPLEMENT: &str = "http://www.w3.org/2002/07/owl#complementOf";
const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const UNION: &str = "http://www.w3.org/2002/07/owl#unionOf";
const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

struct Frame {
    store: FactStore,
    rel: RelationStore,
    evidence: NativeEvidenceIndex,
    sources: Arc<[RefutationPremise]>,
}

impl Frame {
    /// These explicit original rows are the complete synthetic source contract.
    fn new(rows: &[(&str, &str, &str)]) -> Self {
        let sources: Arc<[_]> = rows
            .iter()
            .map(|(s, p, o)| RefutationPremise {
                subject: TermValue::iri(*s),
                predicate: (*p).to_owned(),
                object: TermValue::iri(*o),
                graph: Some(TermValue::iri(WORLD)),
            })
            .collect::<Vec<_>>()
            .into();
        let mut store = FactStore::new();
        let mut rel = RelationStore::with_semantics(
            crate::native_semantics::SemanticVocabulary::GroundedLogicV1,
        );
        for source in sources.iter() {
            let fact = source_fact(source);
            if store.insert(fact.clone()).is_some() {
                rel.insert(&fact.predicate, &fact.subject, &fact.object);
            }
        }
        let evidence = NativeEvidenceIndex::new(
            WORLD.to_owned(),
            Some(TermValue::iri(WORLD)),
            [31; 32],
            sources.clone(),
            &store,
            &SelectedDomains::new([]).unwrap(),
        )
        .unwrap();
        Self {
            store,
            rel,
            evidence,
            sources,
        }
    }

    fn analysis(&self) -> PreparedClassAnalysis {
        PreparedClassAnalysis::from_native_sources(
            WORLD,
            Some(&TermValue::iri(WORLD)),
            &self.sources,
        )
        .unwrap()
    }

    fn observe(
        &self,
        analysis: &mut PreparedClassAnalysis,
        ledger: &mut NativeFamilyLedger,
        complete: bool,
    ) -> ClassExecutionOutcome {
        let completed = if complete {
            BTreeSet::from([NativeRead {
                predicate: None,
                marker: None,
                kind: NativeReadKind::Completed,
            }])
        } else {
            BTreeSet::new()
        };
        let input = NativeFamilyInput::new(&self.store, &self.rel, &[], &self.evidence, &completed)
            .unwrap();
        analysis.evaluate(&input, ledger).unwrap()
    }

    fn ledger(&self, allowance: Option<u64>) -> NativeFamilyLedger {
        NativeFamilyLedger::new(
            WORLD.to_owned(),
            Some(TermValue::iri(WORLD)),
            [31; 32],
            allowance,
        )
    }
}

fn complement() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("urn:x", TYPE, "urn:C"),
        ("urn:x", TYPE, "urn:notC"),
        ("urn:notC", COMPLEMENT, "urn:C"),
    ]
}

#[test]
fn completed_class_conflict_retains_exact_native_support_and_reuses_the_current_result() {
    let frame = Frame::new(&complement());
    let mut analysis = frame.analysis();
    let mut ledger = frame.ledger(None);
    let first = frame.observe(&mut analysis, &mut ledger, true);
    assert_eq!(first.completion, NativeFamilyCompletion::Complete);
    assert_eq!(first.contextual_conflicts.len(), 1);
    assert_eq!(first.local_conclusions().len(), 1);
    assert_eq!(
        first.local_conclusions()[0].subject,
        TermValue::iri("urn:x")
    );
    assert!(
        first.local_conclusions()[0].committed.is_none(),
        "only the shared governor commits a head"
    );
    let proof = &first.contextual_conflicts[0].proof;
    assert_eq!(proof.premises(), frame.sources.iter().cloned().collect());
    assert_eq!(
        ledger.source_leaves(&proof.native_support()).unwrap(),
        frame
            .sources
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    );
    ledger.validate().unwrap();
    first.validate(&ledger).unwrap();
    let work = ledger.work.consumed;
    let proofs = ledger.proofs.clone();
    assert_eq!(frame.observe(&mut analysis, &mut ledger, true), first);
    assert_eq!(ledger.work.consumed, work);
    assert_eq!(ledger.proofs, proofs);
    let mut bytes = Vec::new();
    ciborium::into_writer(&first, &mut bytes).unwrap();
    let decoded: ClassExecutionOutcome = ciborium::from_reader(bytes.as_slice()).unwrap();
    assert_eq!(decoded, first);
    decoded.validate(&ledger).unwrap();
}

#[test]
fn all_connected_model_obstructions_survive_a_sibling_closing_proof() {
    let mut rows = complement();
    let on_property = "http://www.w3.org/2002/07/owl#onProperty";
    let all_values = "http://www.w3.org/2002/07/owl#allValuesFrom";
    rows.extend([
        ("urn:C", on_property, "urn:p"),
        ("urn:C", all_values, "urn:D"),
        ("urn:unrelated", on_property, "urn:other-property"),
    ]);
    let frame = Frame::new(&rows);
    let mut ledger = frame.ledger(None);
    let result = frame.observe(&mut frame.analysis(), &mut ledger, true);
    assert_eq!(result.completion, NativeFamilyCompletion::Obstructed);
    assert_eq!(result.contextual_conflicts.len(), 1);
    assert_eq!(result.obstructions.len(), 2);
    let sources: BTreeSet<_> = result
        .obstructions
        .iter()
        .flat_map(|row| ledger.source_leaves(&row.support).unwrap())
        .collect();
    assert_eq!(
        sources,
        frame
            .sources
            .iter()
            .filter(|row| row.subject == TermValue::iri("urn:C")
                && [on_property, all_values].contains(&row.predicate.as_str()))
            .cloned()
            .collect()
    );
    result.validate(&ledger).unwrap();
}

#[test]
fn pure_other_family_and_unowned_list_fields_do_not_select_class_completion() {
    let rows = [
        ("urn:C", SUBCLASS, "urn:R"),
        ("urn:R", "http://www.w3.org/2002/07/owl#onProperty", "urn:p"),
        (NIL, FIRST, "urn:ordinary-data"),
    ];
    let frame = Frame::new(&rows);
    let mut ledger = frame.ledger(None);
    let mut analysis = frame.analysis();
    assert!(analysis.admission().outside_selection());
    let result = frame.observe(&mut analysis, &mut ledger, true);
    assert_eq!(result.completion, NativeFamilyCompletion::NotEngaged);
    assert!(result.obstructions.is_empty());
    assert!(result.contextual_conflicts.is_empty());
}

#[test]
fn source_refusal_and_zero_budget_cannot_publish_unexamined_conflicts() {
    let mut rows = complement();
    rows.extend([("urn:U", UNION, "urn:missing")]);
    let frame = Frame::new(&rows);
    let mut analysis = frame.analysis();
    let mut ledger = frame.ledger(None);
    assert!(
        analysis.admission().selected_worlds[WORLD]
            .refusal
            .is_some()
    );
    let refusal = frame.observe(&mut analysis, &mut ledger, true);
    assert_eq!(refusal.completion, NativeFamilyCompletion::Obstructed);
    assert!(refusal.contextual_conflicts.is_empty());
    assert!(ledger.proofs.is_empty());
    assert_eq!(ledger.work.consumed, 0);
    let frame = Frame::new(&complement());
    let mut ledger = frame.ledger(Some(0));
    let bounded = frame.observe(&mut frame.analysis(), &mut ledger, true);
    assert_eq!(bounded.completion, NativeFamilyCompletion::Exhausted);
    assert!(bounded.contextual_conflicts.is_empty());
    assert_eq!(ledger.work.consumed, 0);
}

#[test]
fn schema_writers_must_finish_before_class_search_and_world_contracts_cannot_alias() {
    let frame = Frame::new(&complement());
    let mut ledger = frame.ledger(None);
    let mut analysis = frame.analysis();
    let awaiting = frame.observe(&mut analysis, &mut ledger, false);
    assert!(
        matches!(awaiting.completion, NativeFamilyCompletion::Awaiting { ref reads } if !reads.is_empty())
    );
    assert!(awaiting.contextual_conflicts.is_empty());
    assert!(ledger.proofs.is_empty());
    let complete = frame.observe(&mut analysis, &mut ledger, true);
    let mut changed = complete.clone();
    changed.input_contract = [0; 32];
    assert!(changed.validate(&ledger).is_err());
    let mut changed = complete.clone();
    changed.graph = None;
    assert!(changed.validate(&ledger).is_err());
    let mut changed = complete;
    changed.completion = NativeFamilyCompletion::Blocked { reads: vec![] };
    assert!(changed.validate(&ledger).is_err());
}

#[test]
fn exhaustive_branches_cannot_lose_an_alternative_or_its_native_proof_reference() {
    let rows = [
        ("urn:x", TYPE, "urn:U"),
        ("urn:U", UNION, "urn:h"),
        ("urn:h", FIRST, "urn:A"),
        ("urn:h", REST, "urn:t"),
        ("urn:t", FIRST, "urn:B"),
        ("urn:t", REST, NIL),
        ("urn:A", SUBCLASS, "urn:C"),
        ("urn:A", SUBCLASS, "urn:notC"),
        ("urn:notC", COMPLEMENT, "urn:C"),
        ("urn:B", SUBCLASS, "urn:D"),
        ("urn:B", SUBCLASS, "urn:notD"),
        ("urn:notD", COMPLEMENT, "urn:D"),
    ];
    let frame = Frame::new(&rows);
    let mut ledger = frame.ledger(None);
    let result = frame.observe(&mut frame.analysis(), &mut ledger, true);
    result.validate(&ledger).unwrap();
    let RefutationProof::Cases {
        branches,
        alternatives,
        ..
    } = &result.contextual_conflicts[0].proof
    else {
        panic!("two exhaustive native alternatives")
    };
    assert_eq!(branches.len(), 2);
    assert_eq!(alternatives.len(), 2);
    let mut missing = result.clone();
    let RefutationProof::Cases { branches, .. } = &mut missing.contextual_conflicts[0].proof else {
        unreachable!()
    };
    branches.pop();
    assert!(missing.validate(&ledger).is_err());
    let mut altered = result;
    let RefutationProof::Cases { support, .. } = &mut altered.contextual_conflicts[0].proof else {
        unreachable!()
    };
    *support = vec![NativeProofId([0; 32])];
    assert!(altered.validate(&ledger).is_err());
}

#[test]
fn uninhabited_recursive_definition_remains_an_explicit_model_obstruction() {
    let frame = Frame::new(&[("urn:C", COMPLEMENT, "urn:C")]);
    let mut ledger = frame.ledger(None);
    let outcome = frame.observe(&mut frame.analysis(), &mut ledger, true);
    assert_eq!(outcome.completion, NativeFamilyCompletion::Obstructed);
    assert!(!outcome.obstructions.is_empty());
    assert!(outcome.contextual_conflicts.is_empty());
    assert_eq!(
        outcome
            .obstructions
            .iter()
            .flat_map(|o| ledger.source_leaves(&o.support).unwrap())
            .collect::<BTreeSet<_>>(),
        frame.sources.iter().cloned().collect()
    );
}
