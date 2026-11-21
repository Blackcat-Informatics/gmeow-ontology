// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const OUTPUT: &str = "generated/logic/sample.nt";
const ORPHAN: &str = "generated/logic/unexpected.nt";

fn synthetic_bundle(object: &str) -> Vec<u8> {
    let trig = format!(
        r#"
        @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
        @prefix owl: <http://www.w3.org/2002/07/owl#> .
        @prefix dcterms: <http://purl.org/dc/terms/> .
        <https://blackcatinformatics.ca/gmeow> a owl:Ontology ;
            dcterms:title "GMEOW" ; owl:versionInfo "test" .
        gmeow:pipeline-build gmeow:expectsGeneratedOutput "{OUTPUT}" .
        gmeow:r1 a gmeow:FanoutExtraction ;
            gmeow:extractsPath "{OUTPUT}" ; gmeow:extractsMatch "exact" ;
            gmeow:extractsGraphFamily "rdf-fanout" ; gmeow:extractsForm "ntriples" .
        <https://blackcatinformatics.ca/gmeow/graph/fanout/logic/sample.nt> {{
            <urn:subject> <urn:predicate> <{object}> .
        }}
        "#
    );
    let dataset = purrdf::parse_dataset(trig.as_bytes(), "application/trig", None)
        .expect("parse synthetic fanout graph");
    let mut builder = purrdf::gts_compose::SnapshotBuilder::new();
    builder
        .add_dataset(dataset.as_ref())
        .expect("admit synthetic graph");
    // gmeow-test-input: synthetic-only
    gmeow_gts_profile::emit_gmeow_gts(
        builder,
        Vec::new(),
        Vec::new(),
        None,
        &gmeow_gts_profile::baseline_medium_plan(),
    )
    .expect("emit synthetic bundle")
    .bytes
}

/// The update sweep checks disk outputs and never certifies a replaced bundle
/// using the previous bundle's reconstruction.
#[test]
fn update_sweep_follows_the_current_bundle() {
    let work = tempfile::tempdir().expect("create isolated fanout root");
    let root = work.path();
    let bundle = root.join(GTS_PATH);
    std::fs::create_dir_all(root.join("generated/dist")).expect("create terminal directory");
    std::fs::write(&bundle, synthetic_bundle("urn:original")).expect("write initial bundle");

    let (report, fanout) = fanout_for_update(root, 1).expect("write original projection");
    assert_eq!(report.produced, 1);
    assert!(
        fanout
            .sweep_materialized_tree(root)
            .expect("sweep original")
            .is_clean()
    );

    let (_, fanout) = fanout_for_update(root, 1).expect("retain original projection");
    std::fs::write(root.join(OUTPUT), b"drift").expect("change materialized file");
    let drift = fanout
        .sweep_materialized_tree(root)
        .expect("sweep modified tree");
    assert_eq!(drift.mismatch, [OUTPUT]);

    let (_, fanout) = fanout_for_update(root, 1).expect("restore original projection");
    std::fs::rename(root.join(OUTPUT), root.join(ORPHAN)).expect("move materialized file");
    let drift = fanout
        .sweep_materialized_tree(root)
        .expect("sweep incomplete tree");
    assert_eq!(drift.missing, [OUTPUT]);
    assert_eq!(drift.orphan, [ORPHAN]);

    let (_, fanout) = fanout_for_update(root, 1).expect("restore original projection");
    std::fs::write(&bundle, synthetic_bundle("urn:replacement")).expect("replace bundle");
    let drift = fanout
        .sweep_materialized_tree(root)
        .expect("sweep replacement");
    assert_eq!(drift.mismatch, [OUTPUT]);

    let (_, fanout) = fanout_for_update(root, 1).expect("write replacement projection");
    std::fs::rename(&bundle, work.path().join("retired.gts")).expect("move the published bundle");
    let error = fanout
        .sweep_materialized_tree(root)
        .expect_err("an unreadable bundle must fail the sweep");
    assert!(
        error
            .downcast_ref::<crate::error::StageFailed>()
            .is_some_and(|failure| failure.cause.is_some())
    );
}
