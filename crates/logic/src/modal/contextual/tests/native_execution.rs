// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Synthetic GMEOW producer contracts: contextual receipts belong to the shared
//! native run and preserve attribution after its actual writers complete.

use super::*;
use crate::reason::refute::NativeProofOrigin;

#[test]
fn one_contextual_receipt_owns_every_metadata_head_and_rejects_tampering() {
    let dataset = parse(&format!("{SOURCE}\n{QUERY}"));
    let input = crate::reason::prepare_reasoning_input(dataset.as_ref()).unwrap();
    let domains = evidence_domains(&input);
    let result = crate::reason::reason_all(input, &domains).unwrap();
    let execution = result.provenance.native_execution.as_ref().unwrap();
    let owner = execution
        .families
        .iter()
        .find(|ledger| ledger.world == crate::result_rdf::GRAPH_REASONING)
        .unwrap();
    let [receipt] = owner.contextual_receipts.as_slice() else {
        panic!("one selected request must have one shared native receipt");
    };
    let heads = owner
        .proofs
        .iter()
        .filter_map(|proof| match proof.origin {
            NativeProofOrigin::Contextual { receipt: identity } => {
                assert_eq!(identity, receipt.id);
                Some(proof.statement.clone())
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    assert!(
        heads.len() > 10,
        "the complete typed assessment is published"
    );
    assert_eq!(heads, receipt.statements.iter().cloned().collect());
    assert!(
        receipt
            .supports
            .iter()
            .any(|support| support.world == "urn:example:w")
    );
    assert!(
        receipt
            .supports
            .iter()
            .any(|support| support.world == crate::reason::rl::DEFAULT_WORLD)
    );
    execution.validate_structure().unwrap();
    assert_preexisting_publication_retains_assessment(receipt);

    let mut truncated = receipt.clone();
    truncated.statements.pop();
    assert!(truncated.validate().is_err());
    let mut foreign = execution.as_ref().clone();
    let ledger = foreign
        .families
        .iter_mut()
        .find(|ledger| ledger.world == crate::result_rdf::GRAPH_REASONING)
        .unwrap();
    ledger.contextual_receipts[0].supports[0].world = "urn:example:unselected".into();
    assert!(foreign.validate_structure().is_err());
}

fn assert_preexisting_publication_retains_assessment(
    receipt: &crate::contextual::native::NativeContextualReceipt,
) {
    use crate::physical::RelationStore;
    use crate::reason::refute::RefutationPremise;
    use crate::reason::refute::native::{
        NativeEvidenceIndex, NativeFamilyInput, NativeFamilyLedger,
    };
    use crate::rule_ir::{Fact, FactStore};

    let world = crate::result_rdf::GRAPH_REASONING;
    let graph = Some(TermValue::iri(world));
    let mut store = FactStore::new();
    let mut relations =
        RelationStore::with_semantics(crate::native_semantics::SemanticVocabulary::Exact);
    let originals = receipt
        .statements
        .iter()
        .map(|statement| {
            let fact = Fact {
                subject: statement.subject.clone(),
                predicate: statement.predicate.clone(),
                object: statement.object.clone(),
            };
            relations.insert(&fact.predicate, &fact.subject, &fact.object);
            store.insert(fact);
            RefutationPremise {
                subject: statement.subject.clone(),
                predicate: statement.predicate.clone(),
                object: statement.object.clone(),
                graph: graph.clone(),
            }
        })
        .collect::<Vec<_>>();
    // This operation only authenticates a publication; it invokes no intrinsic
    // domain law and therefore explicitly selects no such law.
    let domains = crate::physical::SelectedDomains::new([]).unwrap();
    let evidence = NativeEvidenceIndex::new(
        world.into(),
        graph.clone(),
        receipt.input_contract,
        originals.into(),
        &store,
        &domains,
    )
    .unwrap();
    let completed = BTreeSet::new();
    let input = NativeFamilyInput::new(&store, &relations, &[], &evidence, &completed).unwrap();
    let mut ledger = NativeFamilyLedger::new(world.into(), graph, receipt.input_contract, None);
    let receipt = std::sync::Arc::new(receipt.clone());
    input.retain_contextual(&receipt, &mut ledger).unwrap();
    input.retain_contextual(&receipt, &mut ledger).unwrap();
    assert_eq!(ledger.contextual_receipts.len(), 1);
    assert_eq!(ledger.proofs.len(), receipt.statements.len());
    assert!(
        ledger
            .proofs
            .iter()
            .all(|proof| matches!(proof.origin, NativeProofOrigin::Asserted { .. }))
    );
    ledger.validate().unwrap();
    ledger.proofs.pop();
    assert!(
        ledger.validate().is_err(),
        "even a duplicate publication must retain proof for every metadata head"
    );
}

#[test]
fn native_contextual_status_waits_for_its_committed_writer() {
    let source = SOURCE.replace(
        "gmeow:standpointSupportStatus gmeow:supportSupported",
        "ex:reportedStatus gmeow:supportSupported",
    );
    let rule = "ex:w { ex:reportedStatus <http://www.w3.org/2000/01/rdf-schema#subPropertyOf> gmeow:standpointSupportStatus . }";
    let dataset = parse(&format!("{source}\n{rule}\n{QUERY}"));
    let input = crate::reason::prepare_reasoning_input(dataset.as_ref())
        .expect("source grammar does not require a future attribution head");
    let domains = evidence_domains(&input);
    let result = crate::reason::reason_all(input, &domains).unwrap();
    assert!(result.inferred().iter().any(|row| row.predicate
        == format!("{LOGIC_NAMESPACE}resultInformation")
        && row.object.as_iri() == Some(InformationState::Supported.iri().as_str())));
    let execution = result.provenance.native_execution.as_ref().unwrap();
    let receipt = &execution
        .families
        .iter()
        .find(|ledger| ledger.world == crate::result_rdf::GRAPH_REASONING)
        .unwrap()
        .contextual_receipts[0];
    let status = receipt
        .premises
        .iter()
        .position(|(world, statement)| {
            world == "urn:example:w"
                && statement.subject.as_iri() == Some("urn:example:yes")
                && statement.predicate == format!("{GMEOW_NS}standpointSupportStatus")
        })
        .unwrap();
    let support = &receipt.supports[status];
    let ledger = execution
        .families
        .iter()
        .find(|ledger| ledger.world == support.world)
        .unwrap();
    let proof = ledger
        .proofs
        .iter()
        .find(|proof| proof.id == support.proof)
        .unwrap();
    assert!(matches!(proof.origin, NativeProofOrigin::Derived { .. }));
    execution.validate_structure().unwrap();
}

#[test]
fn a_zero_shared_allowance_publishes_one_complete_unvisited_assessment() {
    let dataset = parse(&format!("{SOURCE}\n{QUERY}"));
    let input = crate::reason::prepare_reasoning_input(dataset.as_ref()).unwrap();
    let domains = evidence_domains(&input);
    let result = crate::reason::reason_all_budgeted(
        input,
        &domains,
        &crate::query_ir::Budget {
            max_steps: Some(0),
            max_answers: None,
        },
    )
    .unwrap();
    assert_eq!(result.provenance.consumed_budget.consumed, 0);
    let execution = result.provenance.native_execution.as_ref().unwrap();
    let ledger = execution
        .families
        .iter()
        .find(|ledger| ledger.world == crate::result_rdf::GRAPH_REASONING)
        .unwrap();
    let [receipt] = ledger.contextual_receipts.as_slice() else {
        panic!("an exhausted request still publishes exactly one complete stop record");
    };
    assert!(
        !receipt.premises.is_empty(),
        "the admitted request source remains authenticated"
    );
    assert!(
        receipt
            .premises
            .iter()
            .all(|(world, _)| world == crate::reason::rl::DEFAULT_WORLD),
        "zero allowance must not inspect even already-committed attribution: {:?}",
        receipt.premises
    );
    assert!(
        receipt
            .supports
            .iter()
            .all(|support| support.world == crate::reason::rl::DEFAULT_WORLD),
        "an unvisited assessment cannot claim evidence-world support"
    );
    let published = result
        .inferred()
        .iter()
        .filter(|row| {
            !row.is_edb
                && row.world == crate::result_rdf::GRAPH_REASONING
                && row.rule_name.as_deref() == Some(RULE_IRI)
        })
        .map(|row| crate::physical::WitnessStatement {
            subject: TermValue::iri(&row.subject),
            predicate: row.predicate.clone(),
            object: row.object.clone(),
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(published, receipt.statements.iter().cloned().collect());
    assert!(published.iter().any(|row| row.predicate
        == format!("{LOGIC_NAMESPACE}resultEvaluation")
        && row.object.as_iri() == Some(EvaluationStatus::BudgetExhausted.iri().as_str())));
    assert!(published.iter().any(|row| row.predicate
        == format!("{LOGIC_NAMESPACE}resultInformation")
        && row.object.as_iri() == Some(InformationState::Undetermined.iri().as_str())));
    assert!(
        !published
            .iter()
            .any(|row| row.predicate == format!("{LOGIC_NAMESPACE}resultProof"))
    );
    execution.validate_structure().unwrap();
}

#[test]
fn derived_attribution_receipts_keep_their_committed_source_world() {
    let source = SOURCE
        .replace("gmeow:accordingTo", "ex:reportedOwner")
        .replace("gmeow:standpointSupportStatus", "ex:reportedStatus");
    let rules = r#"
        ex:w {
            ex:reportedOwner <http://www.w3.org/2000/01/rdf-schema#subPropertyOf> gmeow:accordingTo .
            ex:reportedStatus <http://www.w3.org/2000/01/rdf-schema#subPropertyOf> gmeow:standpointSupportStatus .
        }
    "#;
    let dataset = parse(&format!("{source}\n{rules}\n{QUERY}"));
    let input = crate::reason::prepare_reasoning_input(dataset.as_ref()).unwrap();
    let domains = evidence_domains(&input);
    let result = crate::reason::reason_all(input, &domains).unwrap();
    let execution = result.provenance.native_execution.as_ref().unwrap();
    let receipt = &execution
        .families
        .iter()
        .find(|ledger| ledger.world == crate::result_rdf::GRAPH_REASONING)
        .unwrap()
        .contextual_receipts[0];
    let owner = execution
        .families
        .iter()
        .find(|ledger| ledger.world == "urn:example:w")
        .unwrap();
    let mut evidence_ids = BTreeSet::new();
    for predicate in [
        format!("{GMEOW_NS}accordingTo"),
        format!("{GMEOW_NS}standpointSupportStatus"),
    ] {
        let index = receipt
            .premises
            .iter()
            .position(|(world, statement)| {
                world == "urn:example:w"
                    && statement.subject.as_iri() == Some("urn:example:yes")
                    && statement.predicate == predicate
            })
            .expect("derived attribution participates in the selected assessment");
        let support = &receipt.supports[index];
        assert_eq!(support.world, owner.world);
        let proof = owner
            .proofs
            .iter()
            .find(|proof| proof.id == support.proof)
            .unwrap();
        assert!(matches!(proof.origin, NativeProofOrigin::Derived { .. }));
        let evidence = receipt.statements.iter().find(|statement| {
            statement.predicate == "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies"
                && matches!(&statement.object, TermValue::Triple { s, p, .. }
                    if s.as_iri() == Some("urn:example:yes") && p.as_iri() == Some(predicate.as_str()))
        }).expect("the selected contextual proof retains the exact native attribution receipt");
        evidence_ids.insert(evidence.subject.clone());
        assert!(
            receipt
                .statements
                .iter()
                .any(|statement| statement.subject == evidence.subject
                    && statement.predicate == format!("{GMEOW_NS}inWorld")
                    && statement.object.as_iri() == Some("urn:example:w"))
        );
        assert!(
            !receipt
                .statements
                .iter()
                .any(|statement| statement.subject == evidence.subject
                    && statement.predicate == format!("{GMEOW_NS}inWorld")
                    && statement.object.as_iri() != Some("urn:example:w"))
        );
    }
    assert_eq!(
        evidence_ids.len(),
        2,
        "owner and status retain distinct receipts"
    );
    execution.validate_structure().unwrap();
}

#[test]
fn unrelated_existing_reasoning_graph_does_not_become_contextual_source_grammar() {
    let graph = crate::result_rdf::GRAPH_REASONING;
    let existing = format!(
        r#"
        <{graph}> {{
            ex:oldResult a logic:ReasoningResult;
                logic:journalBoundary logic:OpenJournalBoundary .
            ex:oldEvidence rdf:reifies <<( ex:oldItem ex:oldPredicate ex:oldValue )>> .
        }}
    "#
    );
    let dataset = parse(&format!("{SOURCE}\n{QUERY}\n{existing}"));
    let input = crate::reason::prepare_reasoning_input(dataset.as_ref()).unwrap();
    let domains = evidence_domains(&input);
    let result = crate::reason::reason_all(input, &domains)
        .expect("unselected output metadata cannot mutate the selected request grammar");
    assert!(
        result.inferred().iter().any(|row| row.is_edb
            && row.world == graph
            && row.subject == "urn:example:oldResult"
            && row.predicate == format!("{LOGIC_NAMESPACE}journalBoundary")
            && row.object.as_iri()
                == Some("https://blackcatinformatics.ca/logic/OpenJournalBoundary")),
        "existing output metadata remains an authenticated original assertion"
    );
    let execution = result.provenance.native_execution.as_ref().unwrap();
    let ledger = execution
        .families
        .iter()
        .find(|ledger| ledger.world == graph)
        .unwrap();
    let [receipt] = ledger.contextual_receipts.as_slice() else {
        panic!("the original request must still publish its complete assessment");
    };
    assert!(
        receipt
            .statements
            .iter()
            .any(
                |statement| statement.predicate == format!("{LOGIC_NAMESPACE}resultInformation")
                    && statement.object.as_iri()
                        == Some(InformationState::Supported.iri().as_str())
            )
    );
    assert!(
        receipt.premises.iter().all(|(world, _)| world != graph),
        "unselected previous output is not request or evidence input"
    );
    execution.validate_structure().unwrap();
}

#[test]
fn a_request_writer_in_an_admitted_empty_world_is_rejected_before_execution() {
    use crate::physical::{DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld};
    use gmeow_logic_compile::ir::{
        AtomicTerm, ContextualScope, LogicAxiom, LogicProgram, LogicRule,
    };

    let dataset = purrdf::RdfDatasetBuilder::new().freeze().unwrap();
    let input = crate::reason::prepare_reasoning_input(dataset.as_ref()).unwrap();
    let world = "urn:example:admitted-empty-world";
    let domains = SelectedDomains::new([SelectedLogicalWorld::new(
        LogicalGraph::Named(TermValue::iri(world)),
        DomainProfile::NonemptyObjectDomainV1,
        "urn:test:contextual:empty-world-root-admission".into(),
        *input.ingress_contract(),
    )
    .unwrap()])
    .unwrap();
    let rule = LogicRule::new(
        LogicAxiom::ground(
            "?x",
            RDF_TYPE,
            AtomicTerm::Iri(format!("{LOGIC_NAMESPACE}ContextualEvaluationRequest")),
        )
        .unwrap(),
        vec![
            LogicAxiom::ground(
                "?x",
                format!("{LOGIC_NAMESPACE}instanceOf"),
                AtomicTerm::Iri(format!("{LOGIC_NAMESPACE}Thing")),
            )
            .unwrap(),
        ],
        vec![],
        ContextualScope::default(),
    );
    let program = LogicProgram::new(vec![], vec![rule], vec![], None);
    let error = crate::reason::reason_program(&program, input, &domains)
        .expect_err("a selected empty world must not permit a late first request");
    let refusal = error
        .downcast_ref::<crate::error::NativeCoverage>()
        .expect("typed immutable contextual source refusal");
    assert_eq!(refusal.profile, "native-immutable-contextual-source-v1");
    assert_eq!(refusal.world, world);
}
