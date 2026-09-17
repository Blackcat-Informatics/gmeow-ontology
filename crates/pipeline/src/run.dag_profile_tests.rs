// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::{
    CanonicalTargetOutcome, RunMode, certify_build_plan, full_spec, reconcile_canonical_target,
    write_artifact,
};
use crate::canonical_abstract::ProjectedAbstract;
use gmeow_logic::dag_profile::{DagCertification, certify_acyclic};
use gmeow_logic::result::{
    CompletenessStatus, EvaluationStatus, InformationState, PreservationClaim,
};

#[test]
fn complete_build_plan_binds_before_any_stage_executes() {
    let spec = full_spec();
    let graph = spec.validate().expect("complete valid stage graph");
    let stages = crate::loader::bind(&spec, &graph, &crate::registry::default_registry())
        .expect("every declared stage agrees with its production implementation");
    assert_eq!(stages.len(), spec.stages.len());
}

/// The dogfooded build plan (`gmeow:pipeline-build`, a `logic:Plan`) is
/// certified under the DAG-workflow profile (`logic:DagWorkflowResource`): its
/// producer → consumer dataflow closure is acyclic, so the shared certifier
/// returns `Certified` and the typed result is complete-for-fragment. This is
/// the executable witness of "gmeow:Pipeline certified under the profile".
#[test]
fn build_dag_certifies_under_the_dag_workflow_profile() {
    let spec = full_spec();
    // Edges in producer → consumer orientation, matching the canonical
    // logic:dataflowConsumes (consumer -> producer) the executor inverts.
    let edges: Vec<(String, String)> = spec
        .stages
        .iter()
        .flat_map(|s| {
            s.consumes
                .iter()
                .map(move |dep| (dep.clone(), s.id.clone()))
        })
        .collect();
    let cert = certify_acyclic(edges.iter().map(|(a, b)| (a.as_str(), b.as_str())));
    assert_eq!(
        cert,
        DagCertification::Certified,
        "the build DAG must certify acyclic under logic:DagWorkflowResource; witness: {:?}",
        cert.witness()
    );
    assert_eq!(
        cert.result_status(),
        (
            EvaluationStatus::Completed,
            CompletenessStatus::CompleteForFragment
        ),
        "an acyclic build plan reports complete-for-fragment under the DAG profile"
    );
}

/// The build-pipeline executor hand-off: the build run's typed `ReasoningResult` certification
/// surface. `certify_build_plan` is the EXACT wiring `run_full` folds into the
/// returned `RunReport.certification` — it certifies the real `full_spec()`
/// plan via the shared certifier and lowers the verdict to the typed result a
/// consumer reads, so this asserts the run's certification field WITHOUT a full
/// (off-budget) build. The real build is always acyclic, so the verdict is the
/// certified `Completed` / `CompleteForFragment` result.
#[test]
fn build_run_surfaces_a_certified_typed_reasoning_result() {
    let spec = full_spec();
    let cert = certify_build_plan(&spec).expect("the build plan certifies (no cycle)");
    assert_eq!(
        cert.evaluation,
        EvaluationStatus::Completed,
        "the certified build plan's typed result is evaluation=completed"
    );
    assert_eq!(
        cert.completeness,
        CompletenessStatus::CompleteForFragment,
        "the certified build plan's typed result is complete-for-fragment"
    );
    assert_eq!(cert.information, InformationState::Supported);
    // A certified verdict carries an exact (loss-free) preservation claim and no
    // cycle witness — agreeing with the RDF emit_dag_certification mapping.
    assert_eq!(cert.preservation, PreservationClaim::exact());
    assert!(cert.preservation.unsupported_constructs.is_empty());
    assert!(cert.validate().is_ok());
}

#[test]
fn write_artifact_skips_unchanged_bytes_and_rewrites_drift() {
    let dir = tempfile::tempdir().expect("tempdir");

    assert!(
        write_artifact(dir.path(), "generated/sample.txt", b"v1").expect("initial write"),
        "missing file should be written"
    );
    assert_eq!(
        std::fs::read(dir.path().join("generated/sample.txt")).expect("read initial"),
        b"v1"
    );

    assert!(
        !write_artifact(dir.path(), "generated/sample.txt", b"v1").expect("same bytes"),
        "identical bytes should be left untouched"
    );

    assert!(
        write_artifact(dir.path(), "generated/sample.txt", b"v2").expect("changed bytes"),
        "changed bytes should be rewritten"
    );
    assert_eq!(
        std::fs::read(dir.path().join("generated/sample.txt")).expect("read changed"),
        b"v2"
    );
}

#[test]
/// Update and check classify fixed points consistently while check remains read-only.
fn canonical_target_reconciliation_counts_fixed_points_and_reports_check_drift() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = ProjectedAbstract {
        path: "CITATION.cff",
        bytes: b"abstract: canonical\n".to_vec(),
    };
    std::fs::write(dir.path().join(target.path), &target.bytes).expect("seed fixed point");

    assert_eq!(
        reconcile_canonical_target(dir.path(), RunMode::Update, &target)
            .expect("update fixed point"),
        CanonicalTargetOutcome::Reproduced { written: false }
    );
    assert_eq!(
        reconcile_canonical_target(dir.path(), RunMode::Check, &target).expect("check fixed point"),
        CanonicalTargetOutcome::Reproduced { written: false }
    );

    std::fs::write(dir.path().join(target.path), b"abstract: drifted\n").expect("seed drift");
    assert_eq!(
        reconcile_canonical_target(dir.path(), RunMode::Check, &target).expect("check drift"),
        CanonicalTargetOutcome::Drifted
    );
    assert_eq!(
        reconcile_canonical_target(dir.path(), RunMode::Update, &target).expect("repair drift"),
        CanonicalTargetOutcome::Reproduced { written: true }
    );
}
