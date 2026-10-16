// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tiny native result controls for the documentation verdict admission boundary.
//! No authored corpus, pipeline execution or alternate reasoner is involved.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use gmeow_docs::model::ConsistencyAssessment;

use gmeow_logic::reason::el::InferredAxiom;
use gmeow_logic::result::{
    CompletenessStatus, ContradictionWitness, EvaluationStatus, InformationState, InputStatus,
    PreservationClaim, ReasoningResult, ResultPayload, ResultProvenance,
};
use gmeow_logic::result_rdf::{GRAPH_REASONING, project_reasoning_dataset};
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfTerm, TermValue};

use super::{reasoning_verdict_from_product, reasoning_verdict_from_reason};
use crate::bundle::{PipelineHandle, bundle_from_artifacts_over};
use crate::node::StageProduct;

const WORLD: &str = "urn:docs-verdict:world";
const LOGIC_SUBCLASS: &str = "https://blackcatinformatics.ca/logic/subClassOf";
const LOGIC_NOTHING: &str = "https://blackcatinformatics.ca/logic/Nothing";
const RDFS_SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const OWL_NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";

/// One explicit closure row, retaining its native term and named world.
fn axiom(subject: &str, predicate: &str, object: &str) -> InferredAxiom {
    InferredAxiom {
        subject: subject.to_owned(),
        predicate: predicate.to_owned(),
        object: TermValue::iri(object),
        world: WORLD.to_owned(),
        is_edb: true,
        rule_name: None,
        premises: Vec::new(),
        modal_evaluation: None,
    }
}

/// A conclusive native consistency result with the supplied tiny closure.
fn result(inferred: Vec<InferredAxiom>) -> ReasoningResult {
    ReasoningResult::new(
        InputStatus::Valid,
        EvaluationStatus::Completed,
        CompletenessStatus::CompleteForFragment,
        PreservationClaim::exact(),
        InformationState::Supported,
        ResultProvenance::native("docs-verdict-control", WORLD),
        ResultPayload::Inferred(inferred),
    )
}

/// Build a genuine governed native summary and pin the complete payload to it.
pub(crate) fn product(result: ReasoningResult) -> StageProduct {
    let summary = project_reasoning_dataset(&result).expect("valid tiny native result");
    product_over(result, &summary)
}

/// The explicit backing parameter also permits malformed-payload controls whose
/// valid original summary is retained while the native result is altered.
fn product_over(result: ReasoningResult, summary: &RdfDataset) -> StageProduct {
    let mut builder = RdfDatasetBuilder::new();
    let graph = builder.intern_iri(GRAPH_REASONING).unwrap();
    builder.declare_named_graph(graph).unwrap();
    for mut row in purrdf::native_quads::flat_rdf_quads(summary) {
        row.graph_name = Some(RdfTerm::iri(GRAPH_REASONING));
        builder.push_owned_quad(&row).unwrap();
    }
    let mut bundle = bundle_from_artifacts_over(
        builder.freeze().expect("tiny reasoning summary"),
        BTreeMap::new(),
        purrdf::provenance::DatasetProvenance::new(),
    );
    let pinned = bundle.graph_digest(GRAPH_REASONING).unwrap();
    bundle
        .pin_handle(
            GRAPH_REASONING,
            PipelineHandle::Reasoning(Arc::new(result)),
            pinned,
        )
        .expect("native graph pin");
    StageProduct::from_bundle("stage-reason", Arc::new(bundle))
}

/// Every control enters through the same upstream ownership key as production.
fn upstream(product: StageProduct) -> BTreeMap<String, StageProduct> {
    BTreeMap::from([("stage-reason".to_owned(), product)])
}

/// Empty classes are documented from the native closure without declaring an
/// otherwise satisfiable ontology inconsistent or depending on byte artifacts.
#[test]
fn native_empty_classes_preserve_consistency_and_both_grounded_spellings() {
    // gmeow-test-input: synthetic-only
    let product = product(result(vec![
        axiom(
            "urn:docs-verdict:canonical-empty",
            LOGIC_SUBCLASS,
            LOGIC_NOTHING,
        ),
        axiom(
            "urn:docs-verdict:projected-empty",
            RDFS_SUBCLASS,
            OWL_NOTHING,
        ),
        axiom(
            "urn:docs-verdict:canonical-empty",
            LOGIC_SUBCLASS,
            LOGIC_NOTHING,
        ),
        axiom(LOGIC_NOTHING, LOGIC_SUBCLASS, LOGIC_NOTHING),
        axiom(OWL_NOTHING, RDFS_SUBCLASS, OWL_NOTHING),
        axiom(
            "urn:docs-verdict:mentions-bottom",
            "urn:docs-verdict:mentions",
            LOGIC_NOTHING,
        ),
    ]));
    assert!(
        product.artifacts().is_empty(),
        "the consumer needs no closure byte artifact"
    );
    let verdict =
        reasoning_verdict_from_reason(&upstream(product)).expect("decided native verdict");
    assert_eq!(verdict.consistency, ConsistencyAssessment::Consistent);
    assert_eq!(
        verdict.unsatisfiable,
        BTreeSet::from([
            "urn:docs-verdict:canonical-empty".to_owned(),
            "urn:docs-verdict:projected-empty".to_owned(),
        ])
    );
}

/// The original documentation reader's inconsistency contract now follows
/// the typed contradiction witness, while preserving its empty-class inventory.
#[test]
fn witnessed_native_inconsistency_remains_distinct_from_empty_classes() {
    // gmeow-test-input: synthetic-only
    let mut native = result(vec![
        axiom("urn:docs-verdict:empty", LOGIC_SUBCLASS, LOGIC_NOTHING),
        axiom(
            "urn:docs-verdict:individual",
            "https://blackcatinformatics.ca/logic/instanceOf",
            LOGIC_NOTHING,
        ),
    ]);
    native.information = InformationState::Both;
    native
        .provenance
        .contradiction_witnesses
        .push(ContradictionWitness {
            individual: "urn:docs-verdict:individual".to_owned(),
            world: WORLD.to_owned(),
            premises: vec![(
                "urn:docs-verdict:individual".to_owned(),
                "https://blackcatinformatics.ca/logic/instanceOf".to_owned(),
                LOGIC_NOTHING.to_owned(),
            )],
        });
    let verdict = reasoning_verdict_from_reason(&upstream(product(native.clone())))
        .expect("witnessed contradiction");
    assert_eq!(verdict.consistency, ConsistencyAssessment::Inconsistent);
    assert_eq!(
        verdict.unsatisfiable,
        BTreeSet::from(["urn:docs-verdict:empty".to_owned()])
    );
    native.completeness = CompletenessStatus::Incomplete;
    native.preservation = PreservationClaim::for_unsupported(["urn:docs-verdict:unsupported-rule"]);
    assert_eq!(
        reasoning_verdict_from_reason(&upstream(product(native)))
            .unwrap()
            .consistency,
        ConsistencyAssessment::Inconsistent,
        "a completed run's witnessed contradiction remains decisive with unrelated unsupported residue"
    );
}

/// A native consistency reader may not interpret another payload or handle arm
/// as an empty closure, even when its summary is pinned correctly.
#[test]
fn missing_reason_product_handle_and_wrong_payloads_refuse() {
    // gmeow-test-input: synthetic-only
    assert!(reasoning_verdict_from_reason(&BTreeMap::new()).is_err());
    let mut no_handle = product(result(Vec::new()));
    let _ = Arc::make_mut(&mut no_handle.bundle).detach_handle(GRAPH_REASONING);
    assert!(
        reasoning_verdict_from_reason(&upstream(no_handle))
            .unwrap_err()
            .message()
            .contains("missing pinned")
    );
    let mut wrong_payload = result(Vec::new());
    wrong_payload.payload = ResultPayload::Empty;
    assert!(
        reasoning_verdict_from_reason(&upstream(product(wrong_payload)))
            .unwrap_err()
            .message()
            .contains("Inferred payload")
    );
    let mut wrong_arm = product(result(Vec::new()));
    let bundle = Arc::make_mut(&mut wrong_arm.bundle);
    let pin = bundle.graph_digest(GRAPH_REASONING).unwrap();
    bundle
        .pin_handle(
            GRAPH_REASONING,
            PipelineHandle::Logic(Arc::new(gmeow_logic_compile::ir::LogicProgram::new(
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
            ))),
            pin,
        )
        .unwrap();
    assert!(
        reasoning_verdict_from_reason(&upstream(wrong_arm))
            .unwrap_err()
            .message()
            .contains("Reasoning handle arm")
    );
}

/// Undecided observations can be published, but never become positive consistency
/// or class satisfiability claims. Invalid native input remains a hard failure.
#[test]
fn nonconclusive_or_undecided_native_results_never_render_consistent() {
    // gmeow-test-input: synthetic-only
    for information in [
        InformationState::Undetermined,
        InformationState::Neither,
        InformationState::Opposed,
        InformationState::NotEvaluated,
    ] {
        let mut native = result(Vec::new());
        native.information = information;
        assert_eq!(
            reasoning_verdict_from_reason(&upstream(product(native)))
                .unwrap()
                .consistency,
            ConsistencyAssessment::Undetermined,
        );
    }
    for completeness in [CompletenessStatus::Incomplete, CompletenessStatus::Unknown] {
        let mut native = result(Vec::new());
        native.completeness = completeness;
        assert_eq!(
            reasoning_verdict_from_reason(&upstream(product(native)))
                .unwrap()
                .consistency,
            ConsistencyAssessment::Undetermined
        );
    }
    for evaluation in [
        EvaluationStatus::BudgetExhausted,
        EvaluationStatus::Unsupported,
    ] {
        let mut native = result(Vec::new());
        native.evaluation = evaluation;
        native.completeness = CompletenessStatus::Incomplete;
        assert_eq!(
            reasoning_verdict_from_reason(&upstream(product(native)))
                .unwrap()
                .consistency,
            ConsistencyAssessment::Undetermined,
        );
    }
    let mut invalid = result(Vec::new());
    invalid.input = InputStatus::Invalid;
    assert!(reasoning_verdict_from_reason(&upstream(product(invalid))).is_err());
    let mut dropped = result(Vec::new());
    dropped.preservation =
        PreservationClaim::for_unsupported(["urn:docs-verdict:unsupported-rule"]);
    assert_eq!(
        reasoning_verdict_from_reason(&upstream(product(dropped)))
            .unwrap()
            .consistency,
        ConsistencyAssessment::Undetermined,
    );
    // CompleteForFragment independently certifies conclusiveness under the
    // native result contract, even if a broader computation exhausted its budget.
    let mut complete_fragment = result(Vec::new());
    complete_fragment.evaluation = EvaluationStatus::BudgetExhausted;
    assert_eq!(
        reasoning_verdict_from_reason(&upstream(product(complete_fragment)))
            .unwrap()
            .consistency,
        ConsistencyAssessment::Consistent,
    );
}

/// Publication preserves the exact refusal and known empty classes while the
/// native gate's own consistency predicate continues to reject the same result.
#[test]
fn unsupported_native_observation_keeps_all_axes_and_gate_refusal() {
    let mut native = result(vec![axiom("urn:empty", LOGIC_SUBCLASS, LOGIC_NOTHING)]);
    native.evaluation = EvaluationStatus::Unsupported;
    native.completeness = CompletenessStatus::Incomplete;
    native.information = InformationState::Undetermined;
    native.preservation =
        PreservationClaim::for_unsupported(["nominal-set-equality", "class-equivalence"]);
    assert!(
        !native.is_decided_consistent(),
        "the reason/verify gate still rejects this result"
    );
    let verdict = reasoning_verdict_from_reason(&upstream(product(native.clone()))).unwrap();
    assert_eq!(verdict.consistency, ConsistencyAssessment::Undetermined);
    assert_eq!(verdict.input, native.input.wire());
    assert_eq!(verdict.evaluation, native.evaluation.wire());
    assert_eq!(verdict.completeness, native.completeness.wire());
    assert_eq!(verdict.information, native.information.wire());
    assert_eq!(
        verdict.unsupported_constructs,
        native.preservation.unsupported_constructs
    );
    assert_eq!(verdict.unsatisfiable, BTreeSet::from(["urn:empty".into()]));
    let mut term = gmeow_docs::model::DocTerm {
        iri: "urn:empty".into(),
        category: gmeow_docs::model::DocTermCategory::Class,
        ..Default::default()
    };
    assert_eq!(verdict.term_status(&term), "unsatisfiable");
    term.iri = "urn:other".into();
    assert_eq!(verdict.term_status(&term), "undetermined");
    assert!(
        !native.is_decided_consistent(),
        "observing docs cannot change native admission"
    );
}

/// Result invariants remain mandatory after identity admission: a bare glut is
/// invalid even if someone publishes its native bytes under a real graph pin.
#[test]
fn invalid_native_contradiction_without_evidence_refuses() {
    // gmeow-test-input: synthetic-only
    let mut native = result(Vec::new());
    let summary = project_reasoning_dataset(&native).unwrap();
    native.information = InformationState::Both;
    let error =
        reasoning_verdict_from_reason(&upstream(product_over(native, &summary))).unwrap_err();
    assert!(
        error.message().contains("information=both requires"),
        "{error}"
    );
}

/// Native payload fields omitted by the RDF summary still belong to the full
/// published commitment; replacing them under the same graph pin must refuse.
#[test]
fn changed_native_payload_identity_refuses() {
    // gmeow-test-input: synthetic-only
    let original = result(vec![axiom(
        "urn:docs-verdict:a",
        "urn:docs-verdict:p",
        "urn:docs-verdict:b",
    )]);
    let mut changed = original.clone();
    let ResultPayload::Inferred(rows) = &mut changed.payload else {
        panic!("the native control has an inferred payload")
    };
    rows[0].object = TermValue::iri("urn:docs-verdict:substituted");
    let mut substituted = product(original);
    let bundle = Arc::make_mut(&mut substituted.bundle);
    let pin = bundle.graph_digest(GRAPH_REASONING).unwrap();
    bundle
        .pin_handle(
            GRAPH_REASONING,
            PipelineHandle::Reasoning(Arc::new(changed)),
            pin,
        )
        .unwrap();
    assert!(
        reasoning_verdict_from_reason(&upstream(substituted))
            .unwrap_err()
            .message()
            .contains("identity changed")
    );
}

/// A missing graph declaration or released/wrong-stage product cannot satisfy
/// the native input contract, regardless of any stored digest or empty pin.
#[test]
fn absent_graph_released_carrier_and_wrong_producer_refuse() {
    // gmeow-test-input: synthetic-only
    let mut bundle = crate::bundle::bundle_from_artifacts(
        BTreeMap::new(),
        purrdf::provenance::DatasetProvenance::new(),
    );
    let pin = bundle.graph_digest(GRAPH_REASONING).unwrap();
    bundle
        .pin_handle(
            GRAPH_REASONING,
            PipelineHandle::Reasoning(Arc::new(result(Vec::new()))),
            pin,
        )
        .unwrap();
    let absent = StageProduct::from_bundle("stage-reason", Arc::new(bundle));
    assert!(
        reasoning_verdict_from_reason(&upstream(absent))
            .unwrap_err()
            .message()
            .contains("graph pin")
    );
    let released = product(result(Vec::new())).into_carrier_released().unwrap();
    assert!(reasoning_verdict_from_reason(&upstream(released)).is_err());
    let mut wrong_stage = product(result(Vec::new()));
    wrong_stage.stage_id = "another-producer".to_owned();
    assert!(reasoning_verdict_from_reason(&upstream(wrong_stage)).is_err());
}

/// The post-DAG docs measurement consumes the already retained snapshot's
/// identical native result after the original reasoning carrier is released.
#[test]
fn retained_snapshot_owns_the_post_dag_documentation_verdict() {
    // gmeow-test-input: synthetic-only
    let reason = product(result(vec![axiom(
        "urn:docs-verdict:snapshot-empty",
        LOGIC_SUBCLASS,
        LOGIC_NOTHING,
    )]));
    let snapshot = StageProduct::from_bundle("stage-snapshot", Arc::clone(reason.bundle()));
    let released = reason.into_carrier_released().unwrap();
    assert!(reasoning_verdict_from_reason(&upstream(released)).is_err());
    let verdict = reasoning_verdict_from_product(&snapshot, "stage-snapshot")
        .expect("retained snapshot owner");
    assert_eq!(verdict.consistency, ConsistencyAssessment::Consistent);
    assert_eq!(
        verdict.unsatisfiable,
        BTreeSet::from(["urn:docs-verdict:snapshot-empty".to_owned()])
    );
    assert!(
        reasoning_verdict_from_product(&snapshot, "stage-reason").is_err(),
        "the caller must name its actual selected owner, never probe a fallback"
    );
}
