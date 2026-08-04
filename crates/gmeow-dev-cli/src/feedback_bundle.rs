// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Build, read, and verify the self-describing diagnostics feedback bundle.
//!
//! `gmeow-dev feedback` always emits `dist/gmeow-feedback.gts`: a self-contained
//! GTS bundle whose snapshot graph IS the `gmeow:` RDF projection of the findings
//! (SPARQL-queryable), with the SARIF 2.1.0 and flat-JSON projections riding as
//! content-addressed blob frames. The snapshot content id is stamped into the
//! report metadata so the bundle is a verifiable self-attestation.
//!
//! This module is the Rust twin of the retired Python
//! `src/gmeow_tools/feedback_bundle.py` + `src/gmeow_tools/gts_producer.py`.

use std::collections::BTreeMap;

use ciborium::value::Value as CborValue;
use gmeow_errors::{Report, render};
use purrdf::gts::dataset_from_gts_graph;
use purrdf::gts::reader::read;
use purrdf::gts_compose::{BlobRow, SnapshotBuilder};
use purrdf::parse_dataset;
use serde_json::Value;

/// Blob representation label for the embedded SARIF 2.1.0 projection.
pub const REP_SARIF: &str = "gmeow:report/sarif";
/// Blob representation label for the embedded flat-JSON findings projection.
pub const REP_FINDINGS: &str = "gmeow:report/findings";
/// Report-metadata key carrying the snapshot self-attestation content id.
pub const META_SNAPSHOT_ID: &str = "snapshotContentId";

/// Embed `report` into a self-describing feedback `.gts` bundle.
///
/// The snapshot graph is the findings RDF; SARIF and flat JSON ride as blobs.
/// The snapshot content id is stamped into the report metadata before the
/// JSON/SARIF projections are rendered, so the embedded report attests to the
/// bundle it lives in.
pub fn build_feedback_bundle(report: &Report) -> gmeow_errors::Result<Vec<u8>> {
    let mut builder = SnapshotBuilder::new();
    let nquads = render::to_gmeow_rdf(report);
    if !nquads.trim().is_empty() {
        let dataset = parse_dataset(nquads.as_bytes(), "application/n-quads", None)
            .map_err(|e| crate::error::feedback(format!("parse findings RDF: {e}")))?;
        builder
            .add_dataset(&dataset)
            .map_err(|e| crate::error::feedback(format!("add findings dataset: {e}")))?;
    }

    let snapshot_id = builder.snapshot_content_id();

    let mut stamped = report.clone();
    stamped
        .metadata
        .insert(META_SNAPSHOT_ID.to_owned(), Value::String(snapshot_id));

    let sarif = render::to_sarif(&stamped)
        .map_err(|e| crate::error::feedback(format!("sarif render: {e}")))?;
    let flat = render::to_json(&stamped)
        .map_err(|e| crate::error::feedback(format!("json render: {e}")))?;

    gmeow_gts_profile::emit_gmeow_gts(
        &builder,
        Vec::new(),
        vec![
            BlobRow {
                data: sarif.into_bytes(),
                media_type: "application/sarif+json".to_owned(),
                rep: REP_SARIF.to_owned(),
            },
            BlobRow {
                data: flat.into_bytes(),
                media_type: "application/json".to_owned(),
                rep: REP_FINDINGS.to_owned(),
            },
        ],
        None,
        None,
        None,
    )
    .map_err(|e| crate::error::feedback(format!("emit feedback bundle: {e}")))
}

/// Map each embedded report blob's `rep` to its decoded payload.
pub fn read_report_blobs(
    graph: &mut purrdf::gts::model::Graph,
) -> gmeow_errors::Result<BTreeMap<String, Vec<u8>>> {
    let decoded = graph
        .decoded_blobs()
        .map_err(|e| crate::error::feedback(format!("decode blobs: {e}")))?;
    let by_digest: BTreeMap<&str, &[u8]> = decoded
        .iter()
        .map(|(digest, bytes)| (digest.as_str(), bytes.as_slice()))
        .collect();

    let mut out = BTreeMap::new();
    for (digest, meta) in &graph.blob_meta {
        if let Some(rep) = meta_text(meta, "rep")
            && let Some(&bytes) = by_digest.get(digest.as_str())
        {
            out.insert(rep, bytes.to_vec());
        }
    }
    Ok(out)
}

/// True when the embedded report attests to this bundle's snapshot.
///
/// Re-derives the snapshot content id from the folded findings graph and checks
/// it equals the `snapshotContentId` the embedded flat-JSON report recorded.
///
/// The bundle is untrusted input: a corrupt or tampered bundle (unreadable bytes,
/// malformed JSON, a non-mapping payload, an unparsable graph) is simply *not a
/// valid self-attestation* and returns `False` rather than raising.
pub fn verify_feedback_bundle(bundle: &[u8]) -> bool {
    let mut graph = read(bundle, true, None);

    let Some(findings_digest) = graph.blob_meta.iter().find_map(|(digest, meta)| {
        meta_text(meta, "rep")
            .filter(|rep| rep == REP_FINDINGS)
            .map(|_| digest.clone())
    }) else {
        return false;
    };

    let Ok(Some(flat)) = graph.blob_bytes_cloned(&findings_digest) else {
        return false;
    };

    let Ok(payload) = serde_json::from_slice::<Value>(&flat) else {
        return false;
    };

    let Some(metadata) = payload.get("metadata").and_then(Value::as_object) else {
        return false;
    };

    let Some(stamped) = metadata.get(META_SNAPSHOT_ID).and_then(Value::as_str) else {
        return false;
    };

    let Ok(dataset) = dataset_from_gts_graph(&graph) else {
        return false;
    };

    let mut builder = SnapshotBuilder::new();
    if builder.add_dataset(dataset.as_ref()).is_err() {
        return false;
    }

    builder.snapshot_content_id() == stamped
}

/// Extract a text value from a CBOR map's public metadata.
fn meta_text(meta: &CborValue, key: &str) -> Option<String> {
    let CborValue::Map(entries) = meta else {
        return None;
    };
    entries.iter().find_map(|(k, v)| match (k, v) {
        (CborValue::Text(k), CborValue::Text(v)) if k == key => Some(v.clone()),
        _ => None,
    })
}
