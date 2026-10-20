// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::RdfDatasetBuilder;
use purrdf::entail::{ClaimSubject, DlCompleteness, Question};

#[test]
fn packed_view_retains_borrowed_proof_context_and_session() {
    let dataset = consistent_dataset();
    let bytes = purrdf::PackBuilder::build_bytes(&dataset).unwrap();
    let view = purrdf::PackView::from_bytes(&bytes).unwrap();
    let mut reasoner = DlReasoner::with_proofs(&view).unwrap();
    reasoner.prepare(&Question::Consistency).unwrap();
    let result = reasoner.consistency();
    assert_eq!(*result.answer(), Verdict::True);
    {
        let context = reasoner.borrowed_proof_context().unwrap();
        result
            .proof()
            .unwrap()
            .verify(
                &view,
                &Question::Consistency,
                Some(result.certificate()),
                &context,
            )
            .unwrap();
    }
    assert_eq!(*reasoner.consistency().answer(), Verdict::True);
    assert_eq!(
        *DlReasoner::new(&view).unwrap().consistency().answer(),
        Verdict::True
    );
    assert_eq!(DlReasoner::with_step_cap(&view, 1).unwrap().step_cap(), 1);
}

#[test]
fn shared_view_read_failure_preserves_original_cause_in_every_constructor() {
    use std::sync::Arc;
    let source = RefusingView {
        dataset: consistent_dataset(),
        failed: std::cell::Cell::new(false),
        cause: Arc::new(std::io::Error::other("caller source read failed")),
    };
    let cause = Arc::clone(&source.cause);
    let results: Vec<_> = (0..4)
        .map(|mode| {
            source.failed.set(false);
            match mode {
                0 => DlReasoner::new(&source),
                1 => DlReasoner::with_proofs(&source),
                2 => DlReasoner::with_step_cap(&source, 1),
                _ => DlReasoner::with_proofs_and_stop(
                    &source,
                    purrdf::entail::reasoner::SchemaPreparationBudget::default(),
                    Arc::new(CallerStop::default()),
                ),
            }
        })
        .collect();
    drop(source);
    for result in results {
        let diagnostic = result.unwrap_err();
        let failure = diagnostic
            .downcast_ref::<crate::error::DlService>()
            .unwrap();
        assert_eq!(failure.kind, DlServiceFailureKind::Execution);
        let native = failure
            .cause
            .downcast_ref::<EntailError<Arc<std::io::Error>>>()
            .unwrap();
        let EntailError::SourceRead(original) = native else {
            panic!("the source read failure must remain native and typed");
        };
        assert!(Arc::ptr_eq(original, &cause));
        assert!(std::error::Error::source(native).is_some());
    }
}

struct RefusingView {
    dataset: std::sync::Arc<RdfDataset>,
    failed: std::cell::Cell<bool>,
    cause: std::sync::Arc<std::io::Error>,
}

impl purrdf::DatasetView for RefusingView {
    type Id = purrdf::TermId;
    type ReadError = std::sync::Arc<std::io::Error>;
    type TermGuard<'a> = purrdf::TermRef<'a>;
    type ProbePlan = ();

    fn read_error(&self) -> Option<Self::ReadError> {
        self.failed.get().then(|| self.cause.clone())
    }

    fn quads(&self) -> impl Iterator<Item = purrdf::QuadIds> + '_ {
        self.dataset
            .quads()
            .take(1)
            .inspect(|_| self.failed.set(true))
    }

    fn resolve(&self, id: Self::Id) -> std::result::Result<Self::TermGuard<'_>, Self::ReadError> {
        Ok(self.dataset.as_ref().resolve(id))
    }

    fn term_id_by_value(
        &self,
        value: &TermValue,
    ) -> std::result::Result<Option<Self::Id>, Self::ReadError> {
        Ok(self.dataset.as_ref().term_id_by_value(value))
    }

    fn capabilities(&self) -> purrdf::RdfStoreCapabilities {
        self.dataset.capabilities()
    }

    fn probe_plan(&self, _: bool, _: bool, _: bool, _: purrdf::GraphMatch) {}

    fn quads_for_pattern_with_plan(
        &self,
        (): &Self::ProbePlan,
        s: Option<Self::Id>,
        p: Option<Self::Id>,
        o: Option<Self::Id>,
        g: purrdf::GraphMatch,
    ) -> impl Iterator<Item = purrdf::QuadIds> + '_ {
        self.quads_for_pattern(s, p, o, g)
    }

    fn term_count(&self) -> u64 {
        purrdf::DatasetView::term_count(&self.dataset)
    }
}

#[derive(Debug, Default)]
struct CallerStop(std::sync::atomic::AtomicBool);

impl purrdf::StopSignal for CallerStop {
    fn stopped(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Recording retains the caller's signal and the complete native proof/certificate.
#[test]
fn recorded_cancellation_preserves_completed_proofs_and_later_stop_receipts() {
    use purrdf::entail::reasoner::SchemaPreparationBudget;
    use std::sync::Arc;

    let dataset = consistent_dataset();
    let stop = Arc::new(CallerStop::default());
    let mut reasoner = DlReasoner::with_proofs_and_stop(
        &dataset,
        SchemaPreparationBudget::default(),
        stop.clone(),
    )
    .unwrap();
    reasoner.prepare(&Question::Consistency).unwrap();
    let completed = reasoner.consistency();
    assert_eq!(*completed.answer(), Verdict::True);
    assert!(completed.certificate().completeness().is_decided());
    stop.0.store(true, std::sync::atomic::Ordering::Relaxed);
    let stopped = reasoner.consistency();
    assert_eq!(*stopped.answer(), Verdict::Unknown);
    assert!(!stopped.certificate().completeness().is_decided());
    assert_eq!(
        stopped.proof().unwrap().receipt().unwrap().cause(),
        purrdf::entail::StopCause::CallerStop
    );
    let context = reasoner.proof_context().unwrap();
    for result in [completed, stopped] {
        result
            .proof()
            .unwrap()
            .verify(
                &dataset,
                &Question::Consistency,
                Some(result.certificate()),
                &context,
            )
            .unwrap();
    }
}

/// Construction cancellation keeps its native cause after releasing the source.
#[test]
fn recorded_preparation_cancellation_remains_a_typed_execution_diagnostic() {
    use purrdf::entail::reasoner::SchemaPreparationBudget;
    use std::sync::Arc;

    let dataset = consistent_dataset();
    let stop = CallerStop(std::sync::atomic::AtomicBool::new(true));
    let error = DlReasoner::with_proofs_and_stop(
        &dataset,
        SchemaPreparationBudget::default(),
        Arc::new(stop),
    )
    .unwrap_err();
    drop(dataset);
    let failure = error.downcast_ref::<crate::error::DlService>().unwrap();
    assert_eq!(failure.kind, DlServiceFailureKind::Execution);
    assert!(matches!(
        failure.cause.downcast_ref::<EntailError>().unwrap(),
        EntailError::Stopped
    ));
    assert_eq!(
        gmeow_errors::code::code_str(error.code()),
        "logic.dl-service"
    );
}

/// A selected native preparation ceiling survives the diagnostic facade unchanged.
#[test]
fn recorded_cancellation_retains_the_selected_preparation_budget() {
    use purrdf::entail::reasoner::SchemaPreparationBudget;
    use std::sync::Arc;

    let dataset = purrdf::parse_dataset(
        b"@prefix owl: <http://www.w3.org/2002/07/owl#> .
          @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
          @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
          <urn:Cat> rdfs:subClassOf [ owl:onProperty <urn:parent> ;
              owl:onClass <urn:Animal> ; owl:minQualifiedCardinality \"1\"^^xsd:nonNegativeInteger ] .
          <urn:tom> a <urn:Cat> .",
        "text/turtle",
        None,
    )
    .unwrap();
    let budget = SchemaPreparationBudget {
        work: Some(0),
        bytes: None,
    };
    let native = Reasoner::with_proofs_and_preparation_budget(&dataset, budget)
        .unwrap()
        .consistency();
    let adapted =
        DlReasoner::with_proofs_and_stop(&dataset, budget, Arc::new(CallerStop::default()))
            .unwrap()
            .consistency();
    assert_eq!(*adapted.answer(), Verdict::Unknown);
    assert_eq!(
        adapted.certificate().completeness(),
        DlCompleteness::BudgetExhausted
    );
    assert_eq!(adapted.certificate(), native.certificate());
    assert_eq!(adapted.proof(), native.proof());
}

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

/// No-model refusal retains its semantic category, upstream cause and canonical failure class.
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
    assert!(matches!(
        failure.cause.downcast_ref::<EntailError>().unwrap(),
        EntailError::Unsatisfiable
    ));
    assert!(error.message().contains("unsatisfiable"), "{error}");
    assert_eq!(
        error.inner().failure_class.as_deref(),
        Some("https://blackcatinformatics.ca/logic/DirectSemanticsServiceRefusal")
    );
}

/// The hierarchy error survives input release and reaches the diagnostic ledger.
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
        failure.cause.downcast_ref::<EntailError>().unwrap(),
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
    assert_eq!(
        recorded.root.failure_class.as_deref(),
        Some("https://blackcatinformatics.ca/logic/DirectSemanticsServiceRefusal")
    );
    assert!(
        recorded
            .root
            .frames
            .iter()
            .any(|frame| frame.message.contains("urn:short-chain"))
    );
}

/// Class-restriction refusals retain source spelling through the actual GMEOW ledger.
#[test]
fn a_class_role_refusal_keeps_its_source_in_recorded_and_unrecorded_services() {
    for (recording, controlled) in [(false, false), (true, false), (true, true)] {
        let dataset = purrdf::parse_dataset(
            b"@prefix owl: <http://www.w3.org/2002/07/owl#> .
              @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
              <urn:refused-class-role> a owl:AnnotationProperty .
              <urn:restricted-class> rdfs:subClassOf [
                  a owl:Restriction ; owl:onProperty <urn:refused-class-role> ;
                  owl:someValuesFrom <urn:filler-class>
              ] .",
            "text/turtle",
            None,
        )
        .unwrap();
        let error = if controlled {
            DlReasoner::with_proofs_and_stop(
                &dataset,
                purrdf::entail::reasoner::SchemaPreparationBudget::default(),
                std::sync::Arc::new(CallerStop::default()),
            )
        } else if recording {
            DlReasoner::with_proofs(&dataset)
        } else {
            DlReasoner::new(&dataset)
        }
        .expect_err("an annotation property cannot be an object restriction role");
        drop(dataset);
        let failure = error.downcast_ref::<crate::error::DlService>().unwrap();
        assert_eq!(failure.kind, DlServiceFailureKind::Malformed);
        let EntailError::RoleHierarchy(role) = failure.cause.downcast_ref::<EntailError>().unwrap()
        else {
            panic!("the adapter must preserve the original native role refusal");
        };
        assert!(matches!(
            role.classification(),
            purrdf::entail::RoleHierarchyError::InvalidProperty { .. }
        ));
        assert!(role.presentation().is_some());
        let recorded = gmeow_errors::DiagLedger::new()
            .record(error, gmeow_errors::StageId::new("native-dl-consumer"));
        assert_eq!(recorded.root.code, "logic.dl-service");
        assert!(
            recorded
                .root
                .frames
                .iter()
                .any(|frame| { frame.message.contains("urn:refused-class-role") })
        );
    }
}

#[test]
fn native_service_cancellation_is_an_execution_diagnostic_with_its_original_cause() {
    let error = map_entail_err(EntailError::<std::convert::Infallible>::RoleHierarchy(
        purrdf::entail::RoleHierarchyError::Stopped,
    ));
    let failure = error.downcast_ref::<crate::error::DlService>().unwrap();
    assert_eq!(failure.kind, DlServiceFailureKind::Execution);
    assert!(matches!(
        failure.cause.downcast_ref::<EntailError>().unwrap(),
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
    let borrowed_error = match reasoner.borrowed_proof_context() {
        Ok(_) => panic!("unrecorded services cannot borrow a proof context"),
        Err(error) => error,
    };
    let borrowed_failure = borrowed_error
        .downcast_ref::<crate::error::DlService>()
        .unwrap();
    assert_eq!(borrowed_failure.kind, DlServiceFailureKind::Execution);
    assert!(matches!(
        borrowed_failure
            .cause
            .downcast_ref::<EntailError>()
            .unwrap(),
        EntailError::ProofsNotRecorded
    ));
    let error = match reasoner.proof_context() {
        Ok(_) => panic!("unrecorded services cannot create a proof context"),
        Err(error) => error,
    };
    let failure = error.downcast_ref::<crate::error::DlService>().unwrap();
    assert_eq!(failure.kind, DlServiceFailureKind::Execution);
    assert!(matches!(
        failure.cause.downcast_ref::<EntailError>().unwrap(),
        EntailError::ProofsNotRecorded
    ));
}

/// Canonical text restrictions retain their native proofs and counterproofs after projection.
#[test]
fn canonical_language_tagged_data_reaches_the_native_recorded_service() {
    for (value, expected) in [
        (r#""A selected constraint"@en"#, Verdict::True),
        (r#""A selected constraint""#, Verdict::False),
    ] {
        let source = purrdf::parse_dataset(
            format!(
                r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
           @prefix ex: <urn:constraint-data:> .
           @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
           ex:text a logic:DatatypeProperty .
           ex:Case a logic:Class ; logic:subClassOf [
               a logic:Restriction ; logic:onProperty ex:text ;
               logic:allValuesFrom rdf:langString
           ] .
           ex:case a ex:Case ; ex:text {value} ."#
            )
            .as_bytes(),
            "text/turtle",
            None,
        )
        .unwrap();
        let projection =
            gmeow_logic_compile::projections::reader_view::with_owl_rdfs_projection(&source)
                .unwrap();
        assert!(projection.quads().any(|quad| {
            matches!(
                projection.resolve(quad.s),
                purrdf::TermRef::Iri("urn:constraint-data:text")
            ) && matches!(
                projection.resolve(quad.p),
                purrdf::TermRef::Iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#type")
            ) && matches!(
                projection.resolve(quad.o),
                purrdf::TermRef::Iri("http://www.w3.org/2002/07/owl#DatatypeProperty")
            )
        }));
        let mut reasoner = DlReasoner::with_proofs(&projection)
            .expect("the canonical data role must not become an invalid object role");
        reasoner.prepare(&Question::Consistency).unwrap();
        let result = reasoner.consistency();
        assert_eq!(*result.answer(), expected);
        assert!(result.certificate().completeness().is_decided());
        let context = reasoner.proof_context().unwrap();
        result
            .proof()
            .expect("selected proof recording")
            .verify(
                &projection,
                &Question::Consistency,
                Some(result.certificate()),
                &context,
            )
            .unwrap();
    }
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
