// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Selected requests survive upstream stops without touching unfinished evidence.

use super::*;
use crate::contextual::native::{
    NativeContextualAnalysisStop, NativeContextualProgram, NativeContextualReceipt,
    NativeContextualUpstreamStop,
};
use crate::physical::{RelationStore, SelectedDomains, StepGovernor};
use crate::reason::refute::RefutationPremise;
use crate::reason::refute::native::{
    NativeAnalysisUsage, NativeClosureStatus, NativeEvidenceIndex, NativeFamilyObstruction,
    NativeObstructionKind, NativeSourceTerms,
};
use crate::result::{BudgetLimit, BudgetUsage, EvaluationStatus, InformationState};
use crate::rule_ir::FactStore;
use purrdf::{DatasetView, TermValue};
use std::sync::Arc;

const SOURCE_WORLD: &str = crate::reason::rl::DEFAULT_WORLD;
const OUTPUT_WORLD: &str = crate::result_rdf::GRAPH_REASONING;
const CONTRACT: [u8; 32] = [29; 32];

struct World {
    store: FactStore,
    rel: RelationStore,
    evidence: NativeEvidenceIndex,
    ledger: NativeFamilyLedger,
    complete: BTreeSet<NativeRead>,
}

impl World {
    fn new(world: &str, sources: Arc<[RefutationPremise]>) -> Self {
        let mut store = FactStore::new();
        let mut rel = RelationStore::new();
        for source in sources.iter() {
            let fact = Fact {
                subject: source.subject.clone(),
                predicate: source.predicate.clone(),
                object: source.object.clone(),
            };
            rel.insert(&fact.predicate, &fact.subject, &fact.object);
            store.insert(fact);
        }
        // This fixture authenticates original statements only; it selects no
        // intrinsic domain law and never executes a logical closure.
        let domains = SelectedDomains::new([]).unwrap();
        let evidence = NativeEvidenceIndex::from_native_facts(
            world.into(),
            CONTRACT,
            sources,
            &store,
            &domains,
        )
        .unwrap();
        let mut ledger = NativeFamilyLedger::new(world.into(), None, CONTRACT, None);
        ledger.source_terms = NativeSourceTerms::NativeFacts;
        Self {
            store,
            rel,
            evidence,
            ledger,
            complete: BTreeSet::new(),
        }
    }

    fn snapshot(&mut self) -> NativeWorldSnapshot<'_> {
        NativeWorldSnapshot {
            input: NativeFamilyInput::new(
                &self.store,
                &self.rel,
                &[],
                &self.evidence,
                &self.complete,
            )
            .unwrap(),
            ledger: &mut self.ledger,
        }
    }
}

fn selected_requests() -> (NativeContextualProgram, Arc<[RefutationPremise]>) {
    let dataset = purrdf::parse_dataset(br#"
        @prefix logic: <https://blackcatinformatics.ca/logic/> .
        @prefix ex: <urn:example:> .
        ex:c a logic:AttributedContext ; logic:contextWorld ex:unfinished ;
          logic:contextStandpoint ex:s ; logic:evidenceClosure logic:ClosedWorldClosure .
        ex:first a logic:ContextualEvaluationRequest ; logic:queryFormula ex:q ; logic:queryContext ex:c .
        ex:second a logic:ContextualEvaluationRequest ; logic:queryFormula ex:q ; logic:queryContext ex:c .
        ex:q a logic:Formula ; logic:relation ex:p ; logic:argument ex:a, ex:b .
        ex:a logic:termIndex 0 ; logic:termIri ex:subject .
        ex:b logic:termIndex 1 ; logic:termIri ex:object .
    "#, "text/turtle", None).unwrap();
    let sources: Arc<[_]> = dataset
        .quads()
        .map(|quad| RefutationPremise {
            subject: crate::reason::dataset::native(dataset.as_ref(), quad.s),
            predicate: crate::reason::dataset::native(dataset.as_ref(), quad.p)
                .as_iri()
                .unwrap()
                .to_owned(),
            object: crate::reason::dataset::native(dataset.as_ref(), quad.o),
            graph: None,
        })
        .collect::<Vec<_>>()
        .into();
    let program = NativeContextualProgram::prepare(&BTreeMap::from([(
        SOURCE_WORLD.to_owned(),
        Arc::clone(&sources),
    )]))
    .unwrap();
    (program, sources)
}

fn has_iri(receipt: &NativeContextualReceipt, property: &str, iri: &str) -> bool {
    receipt.statements.iter().any(|statement| {
        statement.predicate == format!("https://blackcatinformatics.ca/logic/{property}")
            && statement.object.as_iri() == Some(iri)
    })
}

#[test]
fn every_upstream_stop_preserves_unvisited_requests_and_actual_inference_allowance() {
    let reads = vec![NativeRead {
        predicate: Some("urn:example:unfinished-role".into()),
        marker: None,
        kind: NativeReadKind::Completed,
    }];
    let usage = NativeAnalysisUsage {
        allowance: Some(3),
        consumed: 2,
        exhausted: true,
    };
    let depth_usage = NativeAnalysisUsage {
        exhausted: false,
        ..usage
    };
    let depth_obstruction = NativeFamilyObstruction {
        kind: NativeObstructionKind::ResourceLimit,
        detail: "case depth reached the selected resource bound".into(),
        support: Vec::new(),
    };
    let cases = [
        (
            NativeContextualUpstreamStop::InferenceExhausted,
            NativeClosureStatus::Exhausted,
            EvaluationStatus::BudgetExhausted,
            InformationState::Undetermined,
            Some(BudgetLimit::Inference),
        ),
        (
            NativeContextualUpstreamStop::AnalysisExhausted {
                analyses: vec![NativeContextualAnalysisStop {
                    world: SOURCE_WORLD.into(),
                    graph: None,
                    usage,
                    class_resource_obstructions: Vec::new(),
                }],
            },
            NativeClosureStatus::Exhausted,
            EvaluationStatus::BudgetExhausted,
            InformationState::Undetermined,
            None,
        ),
        (
            NativeContextualUpstreamStop::AnalysisExhausted {
                analyses: vec![NativeContextualAnalysisStop {
                    world: SOURCE_WORLD.into(),
                    graph: None,
                    usage: depth_usage,
                    class_resource_obstructions: vec![depth_obstruction],
                }],
            },
            NativeClosureStatus::Exhausted,
            EvaluationStatus::BudgetExhausted,
            InformationState::Undetermined,
            None,
        ),
        (
            NativeContextualUpstreamStop::Blocked {
                reads: reads.clone(),
            },
            NativeClosureStatus::Blocked { reads },
            EvaluationStatus::Unsupported,
            InformationState::NotEvaluated,
            None,
        ),
    ];
    let mut contract_sets = BTreeSet::new();
    for (stop, status, evaluation, information, limit) in cases {
        let (program, sources) = selected_requests();
        let source_count = sources.len();
        let mut source = World::new(SOURCE_WORLD, sources);
        source.ledger.work = match &stop {
            NativeContextualUpstreamStop::AnalysisExhausted { analyses } => analyses[0].usage,
            _ => usage,
        };
        let classes = match &stop {
            NativeContextualUpstreamStop::AnalysisExhausted { analyses }
                if !analyses[0].class_resource_obstructions.is_empty() =>
            {
                vec![crate::reason::refute::ClassExecutionOutcome {
                    world: SOURCE_WORLD.into(),
                    graph: None,
                    input_contract: CONTRACT,
                    completion: NativeFamilyCompletion::Exhausted,
                    contextual_conflicts: Vec::new(),
                    obstructions: analyses[0].class_resource_obstructions.clone(),
                }]
            }
            _ => Vec::new(),
        };
        let mut output = World::new(OUTPUT_WORLD, Arc::from([]));
        let mut governor = StepGovernor::new(Some(17));
        governor.consumed = 4;
        let mut state = program.start();
        // Deliberately omit ex:unfinished. Any attempted evidence binding fails
        // instead of accidentally using a completed empty evidence extension.
        let mut worlds = BTreeMap::from([
            (SOURCE_WORLD, source.snapshot()),
            (OUTPUT_WORLD, output.snapshot()),
        ]);
        let batch = program
            .finalize_remaining(&mut state, &mut worlds, &governor, stop.clone())
            .unwrap();
        assert_eq!(governor.consumed, 4);
        assert_eq!(governor.remaining(), Some(13));
        assert_eq!(batch.receipts.len(), 2);
        assert_eq!(
            batch.candidates.len(),
            batch
                .receipts
                .iter()
                .map(|receipt| receipt.statements.len())
                .sum::<usize>()
        );
        assert!(
            program
                .finalize_remaining(&mut state, &mut worlds, &governor, stop.clone())
                .unwrap()
                .receipts
                .is_empty()
        );
        // No selected work remains: a cause collected for an unrelated family
        // cannot invent a failure in this completed contextual operation.
        assert!(
            program
                .finalize_remaining(
                    &mut state,
                    &mut worlds,
                    &governor,
                    NativeContextualUpstreamStop::AnalysisExhausted {
                        analyses: Vec::new()
                    },
                )
                .unwrap()
                .receipts
                .is_empty()
        );
        drop(worlds);
        let families = [source.ledger, output.ledger];
        for receipt in batch.receipts {
            receipt.validate().unwrap();
            assert_eq!(receipt.judgment.upstream_stop(), Some(&stop));
            assert_eq!(receipt.premises.len(), source_count);
            assert!(
                receipt
                    .supports
                    .iter()
                    .all(|support| support.world == SOURCE_WORLD)
            );
            assert!(has_iri(&receipt, "resultEvaluation", &evaluation.iri()));
            assert!(has_iri(&receipt, "resultInformation", &information.iri()));
            assert!(receipt.statements.iter().any(|statement| {
                statement.predicate.ends_with("/resultBudgetAllowance")
                    && matches!(&statement.object, TermValue::Literal { lexical_form, .. } if lexical_form == "13")
            }));
            assert!(!receipt.statements.iter().any(|statement| {
                statement.predicate.ends_with("/resultProof")
                    || statement.predicate.ends_with("/resultCounterproof")
                    || statement.predicate.ends_with("/observedTemporalPrefix")
            }));
            receipt
                .judgment
                .validate_upstream(&status, &families, &classes)
                .unwrap();
            assert!(
                receipt
                    .judgment
                    .validate_upstream(&NativeClosureStatus::Completed, &families, &classes)
                    .is_err()
            );
            match &stop {
                NativeContextualUpstreamStop::AnalysisExhausted { .. } => {
                    let mut changed = families.clone();
                    changed[0].work.consumed += 1;
                    assert!(
                        receipt
                            .judgment
                            .validate_upstream(&status, &changed, &classes)
                            .is_err()
                    );
                    if !classes.is_empty() {
                        assert!(
                            receipt
                                .judgment
                                .validate_upstream(&status, &families, &[])
                                .is_err()
                        );
                        let mut changed = classes.clone();
                        changed[0].obstructions[0].detail.push_str(" changed");
                        assert!(
                            receipt
                                .judgment
                                .validate_upstream(&status, &families, &changed)
                                .is_err()
                        );
                    }
                    // Final admission may exhaust another world; the receipt
                    // authenticates its triggering subset rather than inventing
                    // a later cause for the already finalized request.
                    let mut later = families.clone();
                    later[1].work.exhausted = true;
                    receipt
                        .judgment
                        .validate_upstream(&status, &later, &classes)
                        .unwrap();
                }
                NativeContextualUpstreamStop::Blocked { reads } => {
                    let mut changed = reads.clone();
                    changed[0].predicate = Some("urn:example:different-role".into());
                    assert!(
                        receipt
                            .judgment
                            .validate_upstream(
                                &NativeClosureStatus::Blocked { reads: changed },
                                &families,
                                &classes
                            )
                            .is_err()
                    );
                }
                NativeContextualUpstreamStop::InferenceExhausted => {}
            }
            let budget = BudgetUsage {
                consumed: 4,
                allowance: Some(17),
                limit,
            };
            receipt
                .judgment
                .validate_upstream_allowance(&budget)
                .unwrap();
            assert!(
                receipt
                    .judgment
                    .validate_upstream_allowance(&BudgetUsage {
                        consumed: 5,
                        ..budget
                    })
                    .is_err()
            );
            let encoded = serde_json::to_vec(receipt.as_ref()).unwrap();
            let decoded: NativeContextualReceipt = serde_json::from_slice(&encoded).unwrap();
            assert_eq!(&decoded, receipt.as_ref());
            decoded.validate().unwrap();
            if let Some(statement) = receipt
                .statements
                .iter()
                .find(|statement| statement.predicate.ends_with("/resultContractHash"))
            {
                contract_sets.insert(format!("{:?}", statement.object));
            }
        }
    }
    assert_eq!(
        contract_sets.len(),
        4,
        "the exact upstream cause belongs to the result contract"
    );
}

#[test]
fn an_unselected_contextual_operation_needs_no_upstream_stop_record() {
    let program = NativeContextualProgram::prepare(&BTreeMap::new()).unwrap();
    let batch = program
        .finalize_remaining(
            &mut program.start(),
            &mut BTreeMap::new(),
            &StepGovernor::new(Some(13)),
            NativeContextualUpstreamStop::AnalysisExhausted {
                analyses: Vec::new(),
            },
        )
        .unwrap();
    assert!(batch.receipts.is_empty());
    assert!(batch.candidates.is_empty());
}
