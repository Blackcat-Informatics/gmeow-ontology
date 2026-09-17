// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit diagnostic operation controls over tiny native source datasets.

use super::*;
use crate::physical::{DomainProfile, SelectedLogicalWorld, WitnessOrigin};
use crate::reason::refute::NativeProofOrigin;
use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm};

fn input(rows: &[(&str, &str, &str)]) -> PreparedReasoningInput {
    let mut builder = RdfDatasetBuilder::new();
    for &(s, p, o) in rows {
        builder.push_owned_quad(&RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)));
    }
    crate::reason::prepare_reasoning_input(builder.freeze().unwrap().as_ref()).unwrap()
}

/// This synthetic diagnostic explicitly admits every input context as a theory.
fn selection(input: &PreparedReasoningInput) -> SelectedDomains {
    SelectedDomains::new(input.source_contexts().values().map(|graph| {
        SelectedLogicalWorld::new(
            LogicalGraph::from_graph(graph.clone()),
            DomainProfile::NonemptyObjectDomainV1,
            "urn:test:class-diagnostic:source-theory".to_owned(),
            *input.ingress_contract(),
        )
        .unwrap()
    }))
    .unwrap()
}

#[test]
fn universal_empty_domain_conflict_uses_actual_intrinsic_witness_and_exact_source_owner() {
    for (thing, complement) in [
        (
            "http://www.w3.org/2002/07/owl#Thing",
            "http://www.w3.org/2002/07/owl#complementOf",
        ),
        (
            "https://blackcatinformatics.ca/logic/Thing",
            "https://blackcatinformatics.ca/logic/complementOf",
        ),
    ] {
        let source = input(&[(thing, complement, thing)]);
        let domains = selection(&source);
        let outcome = class_diagnostic(source, &domains, None).unwrap();
        let ClassDiagnosticOutcome::Executed { execution } = &outcome else {
            panic!("valid empty-class theory must execute")
        };
        execution.validate().unwrap();
        assert!(execution.has_conflict());
        assert_eq!(execution.selected_domains, domains);
        assert_eq!(execution.witnesses.len(), 1);
        let witness = &execution.witnesses[0];
        assert!(
            matches!(&witness.scope.origin, WitnessOrigin::NonemptyDomain(domain) if domains.worlds().contains(domain))
        );
        assert!(witness.heads.iter().all(|head| head.premises.is_empty()));
        let proof = &execution
            .classes
            .iter()
            .find(|class| !class.contextual_conflicts.is_empty())
            .unwrap()
            .contextual_conflicts[0]
            .proof;
        assert_eq!(
            proof.premises(),
            BTreeSet::from([super::super::RefutationPremise {
                subject: purrdf::TermValue::iri(thing),
                predicate: complement.to_owned(),
                object: purrdf::TermValue::iri(thing),
                graph: None,
            }])
        );
        let ledger = execution
            .proofs
            .iter()
            .find(|ledger| ledger.world == witness.scope.world)
            .unwrap();
        assert!(proof.native_support().iter().any(|id| ledger.proofs.iter().any(|node|
            node.id == *id && matches!(&node.origin, NativeProofOrigin::Intrinsic { witness: owned } if owned == witness))));
        let mut bytes = Vec::new();
        ciborium::into_writer(&outcome, &mut bytes).unwrap();
        let restored: ClassDiagnosticOutcome = ciborium::from_reader(bytes.as_slice()).unwrap();
        assert_eq!(restored, outcome);
        restored.validate().unwrap();
    }
}

#[test]
fn malformed_selected_source_returns_only_admission_before_intrinsic_execution() {
    let source = input(&[(
        "urn:U",
        "http://www.w3.org/2002/07/owl#unionOf",
        "urn:missing-list",
    )]);
    let domains = selection(&source);
    let outcome = class_diagnostic(source, &domains, None).unwrap();
    outcome.validate().unwrap();
    let ClassDiagnosticOutcome::SourceRefused { admission } = outcome else {
        panic!("selected malformed grammar must not execute")
    };
    assert_eq!(
        admission
            .source_worlds
            .values()
            .map(|world| world.assertions)
            .sum::<u64>(),
        1
    );
    assert!(
        admission
            .selected_worlds
            .values()
            .all(|world| world.refusal.is_some())
    );
    assert_eq!(
        admission.refusal_class().unwrap(),
        Some(super::super::ClassSourceRefusal::Invalid)
    );
}

#[test]
fn diagnostic_requires_exact_domain_authority_and_zero_budget_retains_noncompletion() {
    let rows = [(
        "urn:C",
        "http://www.w3.org/2002/07/owl#complementOf",
        "urn:C",
    )];
    assert!(class_diagnostic(input(&rows), &SelectedDomains::new([]).unwrap(), None).is_err());
    let source = input(&rows);
    let domains = selection(&source);
    let ClassDiagnosticOutcome::Executed { execution } =
        class_diagnostic(source, &domains, Some(0)).unwrap()
    else {
        panic!("admitted bounded operation")
    };
    execution.validate().unwrap();
    assert_eq!(execution.frontier.consumed_steps, 0);
    assert!(!execution.has_conflict());
    assert!(execution.witnesses.is_empty());
    assert!(!matches!(execution.status, NativeClosureStatus::Completed));
    assert!(execution.classes.iter().all(|class| !matches!(
        class.completion,
        super::super::NativeFamilyCompletion::Complete
    )));
}
