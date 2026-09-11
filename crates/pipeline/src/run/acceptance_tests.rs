// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMEOW sync acceptance and selector publication over synthetic receipt metadata.

use super::*;
use crate::cache::{StageKeyContext, stage_key};

fn report(mode: RunMode, ledger: DiagLedger) -> RunReport {
    RunReport {
        mode,
        produced: 0,
        reproduced: 0,
        written: 0,
        skipped_writes: 0,
        removed: 0,
        findings: ledger.findings("gmeow-pipeline"),
        ledger,
        drifted: vec![],
        timings: vec![],
        stage_receipt_root: String::new(),
        stage_timings: vec![],
        stage_phase_timings: vec![],
        level_timings: vec![],
        stage_receipts: vec![],
        output_paths: vec![],
        certification: certify_build_plan(&full_spec()).unwrap(),
    }
}

/// Bind stage declarations only; no stage executes and no product is produced.
fn receipt_metadata() -> Vec<StageReceipt> {
    let spec = full_spec();
    let graph = spec.validate().unwrap();
    bind(&spec, &graph, &default_registry())
        .unwrap()
        .iter()
        .map(|stage| {
            let context = StageKeyContext::new(stage.id(), "metadata-fixture", vec![], vec![]);
            StageReceipt {
                schema_version: context.schema_version,
                action_key: stage_key(&context),
                context,
                stability: stage.stability().iri().into(),
                cache_disposition: stage.cache_policy().iri().into(),
                product_digest: "0".repeat(64),
                product_blob_digest: None,
                product_blob_bytes: 0,
                dataset_quads: 0,
                default_graph: None,
                provenance: None,
                content_store: None,
                graphs: vec![],
                blob_representations: vec![],
                logical_artifacts: vec![],
                typed_handles: vec![],
            }
        })
        .collect()
}

fn ledger_for(grade: Grade) -> DiagLedger {
    let mut ledger = DiagLedger::new();
    ledger.attach(
        Diag::new(register_code("pipeline.drift"), grade, "synthetic finding"),
        StageId::new(PIPELINE_STAGE_ID),
    );
    ledger
}

#[test]
fn collected_diagnostics_preserve_success_and_publish_exact_receipts() {
    // gmeow-test-input: synthetic-only
    let receipts = receipt_metadata();
    for grade in [
        Grade::new(
            Severity::Info,
            FindingCategory::Transient,
            Standpoint::Advisory,
        ),
        Grade::new(
            Severity::Warning,
            FindingCategory::ModelingDisciplineViolation,
            Standpoint::Binding,
        ),
        Grade::new(
            Severity::Error,
            FindingCategory::ModelingDisciplineViolation,
            Standpoint::Advisory,
        ),
        Grade::new(
            Severity::Error,
            FindingCategory::PermittedEpistemicConflict,
            Standpoint::Binding,
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let mut run = report(RunMode::Update, ledger_for(grade));
        run.stage_receipts = receipts.clone();
        let findings = serde_json::to_vec(&run.findings).unwrap();
        assert_eq!(run.findings.len(), 1);
        assert!(run.is_clean(), "{grade:?}");
        run.publish_fixture_selector(root.path()).unwrap();
        let selector = root
            .path()
            .join(crate::fixture::STAGE_FIXTURE_MANIFEST_RELATIVE_PATH);
        let bytes = std::fs::read(selector).unwrap();
        let expected_root = tempfile::tempdir().unwrap();
        let expected =
            crate::fixture::publish_stage_fixture_manifest(expected_root.path(), &receipts)
                .unwrap();
        assert_eq!(bytes, std::fs::read(expected.path).unwrap());
        assert_eq!(serde_json::to_vec(&run.findings).unwrap(), findings);
        assert_eq!(run.ledger.len(), 1);
        assert_eq!(run.timings.len(), 1);
        assert_eq!(run.timings[0].phase, "fixture-selector");
    }
}

#[test]
fn fatal_diagnostics_drift_and_check_mode_cannot_replace_a_selector() {
    // gmeow-test-input: synthetic-only
    let fatal = Grade::new(
        Severity::Error,
        FindingCategory::ModelingDisciplineViolation,
        Standpoint::Binding,
    );
    let mut failed = report(RunMode::Update, ledger_for(fatal));
    // Removing the lossy projection cannot change the authoritative verdict.
    failed.findings.clear();
    assert!(!failed.is_clean());
    let mut drift = report(RunMode::Update, DiagLedger::new());
    drift.drifted.push("generated/example.ttl".into());
    assert!(!drift.is_clean());
    let check = report(RunMode::Check, DiagLedger::new());
    assert!(check.is_clean());
    for mut run in [failed, drift, check] {
        let root = tempfile::tempdir().unwrap();
        let selector = root
            .path()
            .join(crate::fixture::STAGE_FIXTURE_MANIFEST_RELATIVE_PATH);
        std::fs::create_dir_all(selector.parent().unwrap()).unwrap();
        std::fs::write(&selector, b"prior selection").unwrap();
        run.publish_fixture_selector(root.path()).unwrap();
        assert_eq!(std::fs::read(selector).unwrap(), b"prior selection");
        assert!(run.timings.is_empty());
    }
}

#[test]
fn accepted_run_with_incomplete_receipts_fails_publication() {
    // gmeow-test-input: synthetic-only
    let root = tempfile::tempdir().unwrap();
    let mut run = report(RunMode::Update, DiagLedger::new());
    assert!(run.is_clean());
    assert!(run.publish_fixture_selector(root.path()).is_err());
    assert!(
        !root
            .path()
            .join(crate::fixture::STAGE_FIXTURE_MANIFEST_RELATIVE_PATH)
            .exists()
    );
    assert!(run.timings.is_empty());
}
