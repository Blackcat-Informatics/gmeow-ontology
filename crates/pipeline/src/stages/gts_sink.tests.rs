// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn terminal_product_commits_full_ingestion_evidence_beside_the_exact_gts() {
    let graph = "https://blackcatinformatics.ca/gmeow/graph/test-empty-terminal-role";
    let mut native = purrdf::RdfDatasetBuilder::new();
    let role = native.intern_iri(graph);
    native.declare_named_graph(role);
    let source = native.freeze().expect("selected empty role");
    // gmeow-test-input: synthetic-only
    let emission = gmeow_gts_profile::view_to_gmeow_gts(&source).expect("tiny terminal emission");
    let expected = emission.ingestion.clone();
    let output = snapshot_output(
        "stage-gts-sink",
        crate::stages::carrier::SerializedCarrierSnapshot {
            emission,
            pass_one_receipt: b"synthetic pass-one receipt".to_vec(),
            timings: Vec::new(),
        },
    )
    .expect("terminal publication");
    let bytes = output.product.artifact(GTS_PATH).expect("selected GTS");
    let companion = output
        .product
        .artifact(INGESTION_RECEIPT_PATH)
        .expect("mandatory selected ingestion receipt");
    let report = gmeow_gts_profile::read_ingestion_receipt(bytes, companion)
        .expect("same output commitment");
    assert_eq!(report.ingestion, expected);
    assert_eq!(report.ingestion.declarations_omitted, vec![graph]);
    assert!(report.source_receipts.is_empty());
    assert_eq!(
        output.product.artifact(PASS_ONE_RECEIPT_PATH).unwrap(),
        b"synthetic pass-one receipt"
    );
    let mut changed = bytes.to_vec();
    changed.push(0);
    assert!(gmeow_gts_profile::read_ingestion_receipt(&changed, companion).is_err());
}

#[test]
fn sink_declares_only_the_lanes_it_reads_as_carrier_inputs() {
    let sink = GtsSinkStage::new();
    assert_eq!(
        sink.carrier_consumes(),
        [
            "stage-archive-blobs",
            "stage-medium-dictionaries",
            "stage-snapshot",
        ]
    );
    for artifact_only in [
        "stage-source-load",
        "stage-compile-logic",
        "stage-mappings",
        "stage-reason",
        "stage-validate",
        "stage-verify-attestation",
        "stage-conformance",
    ] {
        assert!(
            sink.consumes().iter().any(|id| id == artifact_only),
            "{artifact_only} remains a declared DAG dependency"
        );
        assert!(
            !sink.carrier_consumes().iter().any(|id| id == artifact_only),
            "{artifact_only} supplies committed bytes, not a live carrier"
        );
    }
}
