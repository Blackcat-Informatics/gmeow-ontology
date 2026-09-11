// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Synthetic native producer verdicts at CLI and feedback publication boundaries.

use super::{pipeline_feedback, record_gate_verdict};
use gmeow_errors::{
    Diag, DiagLedger, FindingCategory, Grade, Severity, StageId, Standpoint, register_code,
};
use gmeow_pipeline::run::{RunMode, RunReport};

fn native_report(grade: Grade) -> RunReport {
    let mut ledger = DiagLedger::new();
    ledger.attach(
        Diag::new(
            register_code("gmeow-dev.sync.drift"),
            grade,
            "synthetic producer finding",
        ),
        StageId::new("stage-pipeline-reconcile"),
    );
    RunReport {
        mode: RunMode::Update,
        produced: 0,
        reproduced: 0,
        written: 0,
        skipped_writes: 0,
        removed: 0,
        findings: ledger.findings("pipeline"),
        ledger,
        drifted: vec![],
        timings: vec![],
        stage_receipt_root: String::new(),
        stage_timings: vec![],
        stage_phase_timings: vec![],
        level_timings: vec![],
        stage_receipts: vec![],
        output_paths: vec![],
        certification:
            gmeow_logic::dag_profile::certify_acyclic(std::iter::empty::<(&str, &str)>())
                .into_reasoning_result("urn:synthetic:build-contract", "urn:synthetic:build-world"),
    }
}

#[test]
fn cli_and_feedback_preserve_the_native_verdict_and_every_finding() {
    let reporter = crate::dev_common::reporter_for(gmeow_cli_core::ConsoleMode::Text);
    for (grade, expected) in [
        (
            Grade::new(
                Severity::Info,
                FindingCategory::Transient,
                Standpoint::Advisory,
            ),
            true,
        ),
        (
            Grade::new(
                Severity::Warning,
                FindingCategory::ModelingDisciplineViolation,
                Standpoint::Binding,
            ),
            true,
        ),
        (
            Grade::new(
                Severity::Error,
                FindingCategory::ModelingDisciplineViolation,
                Standpoint::Advisory,
            ),
            true,
        ),
        (
            Grade::new(
                Severity::Error,
                FindingCategory::PermittedEpistemicConflict,
                Standpoint::Binding,
            ),
            true,
        ),
        (
            Grade::new(
                Severity::Error,
                FindingCategory::ModelingDisciplineViolation,
                Standpoint::Binding,
            ),
            false,
        ),
    ] {
        let run = native_report(grade);
        let original = serde_json::to_vec(&run.findings).unwrap();
        assert_eq!(
            crate::dev_sync::accept_pipeline_report(reporter.as_ref(), &run, false).is_ok(),
            expected
        );
        let (mut report, accepted) = pipeline_feedback(run);
        assert_eq!(accepted, expected, "{grade:?}");
        assert_eq!(serde_json::to_vec(&report.findings).unwrap(), original);
        record_gate_verdict(&mut report, "gate_verdict", accepted);
        let restored: gmeow_errors::Report =
            serde_json::from_slice(&serde_json::to_vec(&report).unwrap()).unwrap();
        let expected_verdict = if expected {
            gmeow_errors::GateVerdict::Collected
        } else {
            gmeow_errors::GateVerdict::Fatal
        };
        for key in ["pipeline_gate_verdict", "gate_verdict"] {
            assert_eq!(
                serde_json::from_value::<gmeow_errors::GateVerdict>(restored.metadata[key].clone())
                    .unwrap(),
                expected_verdict
            );
        }
    }
}

#[test]
fn drift_and_fatal_ledger_cannot_be_hidden_by_the_feedback_projection() {
    let mut fatal = native_report(Grade::new(
        Severity::Error,
        FindingCategory::ModelingDisciplineViolation,
        Standpoint::Binding,
    ));
    fatal.findings.clear();
    assert!(!pipeline_feedback(fatal).1);
    let mut drift = native_report(Grade::new(
        Severity::Info,
        FindingCategory::Transient,
        Standpoint::Advisory,
    ));
    drift.drifted.push("generated/synthetic.ttl".into());
    let (report, accepted) = pipeline_feedback(drift);
    assert!(!accepted);
    assert_eq!(report.findings.len(), 2);
    assert_eq!(report.findings[0].code, "generator.drift");
    assert_eq!(report.findings[0].standpoint, Some(Standpoint::Binding));
    assert_eq!(report.findings[1].severity, Severity::Info);
}

#[test]
fn failed_selected_surface_retains_the_successful_pipeline_verdict() {
    let run = native_report(Grade::new(
        Severity::Info,
        FindingCategory::Transient,
        Standpoint::Advisory,
    ));
    let (mut report, accepted) = pipeline_feedback(run);
    assert!(accepted);
    record_gate_verdict(&mut report, "gate_verdict", false);
    let restored: gmeow_errors::Report =
        serde_json::from_slice(&serde_json::to_vec(&report).unwrap()).unwrap();
    assert_eq!(
        restored.metadata["pipeline_gate_verdict"],
        serde_json::json!(gmeow_errors::GateVerdict::Collected)
    );
    assert_eq!(
        restored.metadata["gate_verdict"],
        serde_json::json!(gmeow_errors::GateVerdict::Fatal)
    );
    assert_eq!(restored.findings.len(), 1);
}
