// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMEOW sync acceptance and candidate recording over synthetic receipt metadata.

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
fn collected_diagnostics_record_receipts_without_replacing_the_final_selector() {
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
        let selector = crate::fixture::publish_stage_fixture_manifest(root.path(), &receipts)
            .unwrap()
            .path;
        let mut finalized: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&selector).unwrap()).unwrap();
        finalized["bundle_import"] = serde_json::json!({"selected": "bundle receipt"});
        finalized["docs"] = serde_json::json!({"selected": "docs receipts"});
        let finalized = serde_json::to_vec(&finalized).unwrap();
        std::fs::write(&selector, &finalized).unwrap();
        run.record_fixture_candidate(root.path()).unwrap();
        assert_eq!(std::fs::read(&selector).unwrap(), finalized);
        let candidate = root
            .path()
            .join(crate::fixture::STAGE_FIXTURE_CANDIDATE_RELATIVE_PATH);
        let bytes = std::fs::read(&candidate).unwrap();
        let expected_root = tempfile::tempdir().unwrap();
        let expected =
            crate::fixture::publish_stage_fixture_manifest(expected_root.path(), &receipts)
                .unwrap();
        assert_eq!(bytes, std::fs::read(expected.path).unwrap());
        assert_eq!(serde_json::to_vec(&run.findings).unwrap(), findings);
        assert_eq!(run.ledger.len(), 1);
        assert_eq!(run.timings.len(), 1);
        assert_eq!(run.timings[0].phase, "fixture-receipt-candidate");
        std::fs::remove_file(candidate).unwrap();
        assert!(
            crate::fixture::reuse_stage_fixture_candidate(root.path())
                .unwrap()
                .is_none(),
            "a finalized runner selector cannot substitute for a missing producer candidate"
        );
        assert_eq!(std::fs::read(selector).unwrap(), finalized);
    }
}

#[test]
fn fatal_diagnostics_drift_and_check_mode_cannot_replace_fixture_records() {
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
        let candidate = root
            .path()
            .join(crate::fixture::STAGE_FIXTURE_CANDIDATE_RELATIVE_PATH);
        std::fs::write(&candidate, b"prior candidate").unwrap();
        run.record_fixture_candidate(root.path()).unwrap();
        assert_eq!(std::fs::read(selector).unwrap(), b"prior selection");
        assert_eq!(std::fs::read(candidate).unwrap(), b"prior candidate");
        assert!(run.timings.is_empty());
    }
}

#[test]
fn accepted_run_with_incomplete_receipts_fails_candidate_recording() {
    // gmeow-test-input: synthetic-only
    let root = tempfile::tempdir().unwrap();
    let mut run = report(RunMode::Update, DiagLedger::new());
    assert!(run.is_clean());
    assert!(run.record_fixture_candidate(root.path()).is_err());
    assert!(
        !root
            .path()
            .join(crate::fixture::STAGE_FIXTURE_MANIFEST_RELATIVE_PATH)
            .exists()
    );
    assert!(
        !root
            .path()
            .join(crate::fixture::STAGE_FIXTURE_CANDIDATE_RELATIVE_PATH)
            .exists()
    );
    assert!(run.timings.is_empty());
}
