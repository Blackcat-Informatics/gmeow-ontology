// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Blob-integrity conformance over the committed `generated/dist/gmeow.gts`.
//!
//! Recreates natively the consumer-archive invariants and generalizes them into
//! the whole-bundle law: no dangling reference, no
//! orphan blob, no hash-integrity mismatch, across EVERY reference predicate and
//! EVERY stored blob (see [`gmeow_pipeline::bundle_blobs::Bundle::integrity_report`]).
//!
//! `validate` cannot depend on `pipeline` (the dependency runs the other way,
//! `pipeline -> validate`), and the bundle-fold API (`bundle_blobs::Bundle`)
//! lives only in `pipeline`, so this conformance is hosted here rather than in
//! `crates/validate/tests/` — still gated by the workspace-wide `make check`.

use gmeow_pipeline::bundle_blobs::Bundle;
use std::sync::OnceLock;

#[path = "support/authenticated_bundle.rs"]
mod authenticated_bundle;

/// Parse the exact producer-selected bundle once for the entire integration binary.
fn committed_bundle() -> &'static Bundle {
    static BUNDLE: OnceLock<Bundle> = OnceLock::new();
    BUNDLE.get_or_init(|| {
        Bundle::from_snapshot(authenticated_bundle::source_bytes())
            .expect("fold authenticated gmeow.gts")
    })
}

fn integrity_report() -> &'static gmeow_pipeline::bundle_blobs::BundleIntegrityReport {
    static REPORT: OnceLock<gmeow_pipeline::bundle_blobs::BundleIntegrityReport> = OnceLock::new();
    REPORT.get_or_init(|| {
        committed_bundle()
            .integrity_report()
            .expect("compute blob-integrity report over authenticated gmeow.gts")
    })
}

/// Mirrors `tests/test_bundle_carries_the_consumer_archives`: every consumer
/// archive the wheel-only `gmeow` binary depends on is folded into the shipped
/// bundle and resolves non-empty. A distinct message per archive names exactly
/// which rep-string drifted if one silently resolves to `{}`.
#[test]
fn consumer_archives_present_and_non_empty() {
    let bundle = committed_bundle();

    assert!(
        !bundle.sssom().unwrap().is_empty(),
        "sssom-archive blob missing from gmeow.gts"
    );
    assert!(
        !bundle.cells().unwrap().is_empty(),
        "cells-archive blob missing from gmeow.gts"
    );
    assert!(
        !bundle.queries().unwrap().is_empty(),
        "queries-archive blob missing from gmeow.gts"
    );
    assert!(
        !bundle.tests().unwrap().is_empty(),
        "tests-archive blob missing from gmeow.gts"
    );
    assert!(
        !bundle.shapes().unwrap().is_empty(),
        "shapes-archive blob missing from gmeow.gts"
    );
    assert!(
        !bundle.axioms().unwrap().is_empty(),
        "axioms-archive blob missing from gmeow.gts"
    );
    assert!(
        !bundle.reasoning().unwrap().is_empty(),
        "reasoning-archive blob missing from gmeow.gts"
    );
}

/// Documentation guides are external projections, so neither their bytes nor
/// content-addressed references belong in the logical bundle.
#[test]
fn guide_blob_references_are_absent() {
    let report = integrity_report();

    let guide_predicate = "https://blackcatinformatics.ca/gmeow/guideBlob";
    assert!(
        !report.referenced.contains_key(guide_predicate),
        "gmeow:guideBlob references must be absent from the logical bundle: {report}"
    );
}

/// The generalized whole-bundle law: no dangling reference, no orphan blob, no
/// hash-integrity mismatch, across every content-addressed reference predicate
/// and every stored blob. On failure, the report's `Display` names the actual
/// offending predicate/digest so a red test is immediately actionable.
#[test]
fn integrity_report_is_clean() {
    let report = integrity_report();

    assert!(
        report.is_clean(),
        "committed gmeow.gts blob DAG is not clean:\n{report}"
    );
}
