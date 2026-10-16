// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::RdfDatasetBuilder;
use purrdf::entail::{ClaimSubject, DlCompleteness, Question};

/// Reserved vocabulary the fixtures below assert over.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";

const CAT: &str = "http://example.org/Cat";
const DOG: &str = "http://example.org/Dog";
const ANIMAL: &str = "http://example.org/Animal";
const TOM: &str = "http://example.org/tom";

/// A tiny, consistent A-Box: `Cat ⊑ Animal`, `tom a Cat`.
fn consistent_dataset() -> std::sync::Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    let cat = builder.intern_iri(CAT).unwrap();
    let animal = builder.intern_iri(ANIMAL).unwrap();
    let tom = builder.intern_iri(TOM).unwrap();
    let sub = builder.intern_iri(RDFS_SUBCLASSOF).unwrap();
    let ty = builder.intern_iri(RDF_TYPE).unwrap();
    builder.push_quad(cat, sub, animal, None).unwrap();
    builder.push_quad(tom, ty, cat, None).unwrap();
    builder.freeze().expect("freeze the consistent fixture")
}

/// A tiny, INCONSISTENT A-Box: `Cat` and `Dog` are disjoint, yet `tom` is both.
fn inconsistent_dataset() -> std::sync::Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    let cat = builder.intern_iri(CAT).unwrap();
    let dog = builder.intern_iri(DOG).unwrap();
    let tom = builder.intern_iri(TOM).unwrap();
    let disjoint = builder.intern_iri(OWL_DISJOINT_WITH).unwrap();
    let ty = builder.intern_iri(RDF_TYPE).unwrap();
    builder.push_quad(cat, disjoint, dog, None).unwrap();
    builder.push_quad(tom, ty, cat, None).unwrap();
    builder.push_quad(tom, ty, dog, None).unwrap();
    builder.freeze().expect("freeze the inconsistent fixture")
}

#[test]
fn consistency_true_on_a_satisfiable_abox() {
    let dataset = consistent_dataset();
    let reasoner = DlReasoner::new(&dataset).expect("reverse-map the consistent ontology");
    let certified = reasoner.consistency();
    assert_eq!(*certified.answer(), Verdict::True);
    // The whole ontology was read and every search finished: an exact answer.
    assert!(certified.certificate().completeness().is_decided());
    assert_eq!(
        certified.certificate().completeness(),
        DlCompleteness::Decided
    );
    assert!(certified.certificate().boundaries().is_empty());
}

#[test]
fn consistency_false_on_a_disjointness_violation() {
    let dataset = inconsistent_dataset();
    let reasoner = DlReasoner::new(&dataset).expect("reverse-map the inconsistent ontology");
    let certified = reasoner.consistency();
    // Detected, not errored: consistency is the one service that reports an
    // unsatisfiable ontology as a verdict rather than as a refusal.
    assert_eq!(*certified.answer(), Verdict::False);
    assert!(certified.certificate().completeness().is_decided());
}

#[test]
fn entailed_subsumption_is_certified_true() {
    let dataset = consistent_dataset();
    let mut reasoner = DlReasoner::new(&dataset).expect("reverse-map");
    let axiom = DlAxiom::ClassAssertion {
        individual: TermValue::iri(TOM),
        class: TermValue::iri(ANIMAL),
    };
    // `tom a Animal` is not asserted but IS entailed through `Cat ⊑ Animal`.
    let certified = reasoner.entails(&axiom).expect("consistent ontology");
    assert_eq!(*certified.answer(), Verdict::True);
    assert_eq!(
        certified.certificate().completeness(),
        DlCompleteness::Decided
    );
}

#[test]
fn class_satisfiability_on_the_unsatisfiable_ontology_is_an_error() {
    let dataset = inconsistent_dataset();
    let mut reasoner = DlReasoner::new(&dataset).expect("reverse-map");
    // Every class is vacuously unsatisfiable in an ontology with no model, so
    // the service refuses rather than answering, retaining the no-model category.
    let error = reasoner
        .class_satisfiability(&TermValue::iri(CAT))
        .expect_err("an unsatisfiable ontology has no meaningful class answer");
    let failure = error.downcast_ref::<crate::error::DlService>().unwrap();
    assert_eq!(failure.kind, DlServiceFailureKind::NoModel);
    assert!(matches!(failure.cause.as_ref(), EntailError::Unsatisfiable));
    assert!(error.message().contains("unsatisfiable"), "{error}");
}

#[test]
fn a_malformed_role_hierarchy_retains_native_evidence_after_the_input_is_dropped() {
    let dataset = purrdf::parse_dataset(
        b"@prefix owl: <http://www.w3.org/2002/07/owl#> .
          <urn:short-chain> owl:propertyChainAxiom ( <urn:operand> ) .",
        "text/turtle",
        None,
    )
    .unwrap();
    let error = DlReasoner::new(&dataset).expect_err("a one-role chain is malformed");
    drop(dataset);
    assert_eq!(
        gmeow_errors::code::code_str(error.code()),
        "logic.dl-service"
    );
    let failure = error.downcast_ref::<crate::error::DlService>().unwrap();
    assert_eq!(failure.kind, DlServiceFailureKind::Malformed);
    assert!(matches!(
        failure.cause.as_ref(),
        EntailError::RoleHierarchy(_)
    ));
    assert!(
        std::error::Error::source(failure)
            .unwrap()
            .is::<EntailError>()
    );
    let recorded = gmeow_errors::DiagLedger::new()
        .record(error, gmeow_errors::StageId::new("native-dl-consumer"));
    assert_eq!(recorded.root.code, "logic.dl-service");
    assert!(
        recorded
            .root
            .frames
            .iter()
            .any(|frame| frame.message.contains("urn:short-chain"))
    );
}

#[test]
fn native_service_cancellation_is_an_execution_diagnostic_with_its_original_cause() {
    let error = map_entail_err(EntailError::RoleHierarchy(
        purrdf::entail::RoleHierarchyError::Stopped,
    ));
    let failure = error.downcast_ref::<crate::error::DlService>().unwrap();
    assert_eq!(failure.kind, DlServiceFailureKind::Execution);
    assert!(matches!(
        failure.cause.as_ref(),
        EntailError::RoleHierarchy(purrdf::entail::RoleHierarchyError::Stopped)
    ));
    assert_eq!(
        gmeow_errors::code::code_str(error.code()),
        "logic.dl-service"
    );
}

#[test]
fn a_narrowed_step_cap_reports_unknown_rather_than_a_fabricated_verdict() {
    let dataset = consistent_dataset();

    // Under the size-derived ceiling the same ontology is DECIDED exactly: this is
    // the control that makes the exhausted arm below falsifiable — the ontology is
    // trivially consistent, so a non-conclusion can only come from the ceiling, not
    // from the input being genuinely undecidable.
    let decided = DlReasoner::new(&dataset)
        .expect("reverse-map the consistent ontology")
        .consistency();
    assert_eq!(*decided.answer(), Verdict::True);
    assert_eq!(
        decided.certificate().completeness(),
        DlCompleteness::Decided
    );

    // Now narrow the per-decision step cap to one round — the one ceiling this
    // repository can move, and only downward. One round decides nothing, so the
    // hypertableau search must exhaust.
    let starved =
        DlReasoner::with_step_cap(&dataset, 1).expect("reverse-map under a narrowed step cap");
    assert_eq!(starved.step_cap(), 1, "the cap was narrowed to one round");

    let answer = starved.consistency();

    // The honest contract: exhaustion is a NON-CONCLUSION, never a fabricated
    // verdict and never a panic. A boolean service reports `Unknown`, the
    // certificate reports `BudgetExhausted`, and both completeness predicates read
    // `false` — the answer is not presented as if it were decided or exact.
    assert_eq!(
        *answer.answer(),
        Verdict::Unknown,
        "an exhausted search is Unknown, never True/False as if decided"
    );
    assert_ne!(*answer.answer(), Verdict::True);
    assert_ne!(*answer.answer(), Verdict::False);
    assert_eq!(
        answer.certificate().completeness(),
        DlCompleteness::BudgetExhausted
    );
    assert!(!answer.certificate().completeness().is_decided());
}

/// The GMEOW adapter retains the complete native proof/certificate and their source binding.
#[test]
fn recorded_service_result_survives_the_gmeow_boundary_without_evidence_loss() {
    let dataset = consistent_dataset();
    let result = DlReasoner::with_proofs(&dataset)
        .expect("recording service")
        .consistency();
    let proof = result
        .proof()
        .expect("selected recording cannot lose its proof");
    proof
        .covers(&[ClaimSubject::Consistent])
        .expect("reported answer is covered");
    let context = DlReasoner::with_proofs(&dataset)
        .expect("consumer-owned source preparation")
        .proof_context()
        .expect("exact checking context");
    let replay = proof
        .verify(
            &dataset,
            &Question::Consistency,
            Some(result.certificate()),
            &context,
        )
        .expect("the retained proof and complete certificate check together");
    assert_eq!(replay.runs(), replay.replayed());
    assert!(result.certificate().work_budget() > 0);
    assert!(
        proof
            .verify(
                &inconsistent_dataset(),
                &Question::Consistency,
                Some(result.certificate()),
                &context
            )
            .is_err(),
        "a GMEOW result cannot be rebound to different source premises"
    );
}

/// Explicitly unrecorded service selection cannot manufacture a checking context.
#[test]
fn unrecorded_service_keeps_absence_distinct_from_an_empty_proof() {
    let dataset = consistent_dataset();
    let reasoner = DlReasoner::new(&dataset).unwrap();
    assert!(reasoner.consistency().proof().is_none());
    let error = match reasoner.proof_context() {
        Ok(_) => panic!("unrecorded services cannot create a proof context"),
        Err(error) => error,
    };
    let failure = error.downcast_ref::<crate::error::DlService>().unwrap();
    assert_eq!(failure.kind, DlServiceFailureKind::Execution);
    assert!(matches!(
        failure.cause.as_ref(),
        EntailError::ProofsNotRecorded
    ));
}

/// A consumer prepares fresh question terms without executing another service decision.
#[test]
fn native_proof_checker_prepares_the_selected_question_through_the_gmeow_adapter() {
    let dataset = consistent_dataset();
    let class = TermValue::iri("urn:proof-check:fresh-class");
    let question = Question::ClassSatisfiability {
        class: class.clone(),
    };
    let result = DlReasoner::with_proofs(&dataset)
        .unwrap()
        .class_satisfiability(&class)
        .unwrap();
    let proof = result.proof().expect("selected proof recording");
    proof
        .covers(&[ClaimSubject::ClassSatisfiable { class }])
        .unwrap();
    let mut checker = DlReasoner::with_proofs(&dataset).unwrap();
    checker
        .prepare(&question)
        .expect("prepare the exact consumer question");
    let context = checker.proof_context().unwrap();
    proof
        .verify(&dataset, &question, Some(result.certificate()), &context)
        .expect("the GMEOW boundary preserves a checkable question-bound proof");
    assert!(
        proof
            .verify(
                &dataset,
                &Question::Consistency,
                Some(result.certificate()),
                &context
            )
            .is_err(),
        "a different question is not the selected proof task"
    );
}
