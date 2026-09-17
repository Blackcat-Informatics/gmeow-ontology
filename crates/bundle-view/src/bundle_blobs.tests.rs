// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use std::path::PathBuf;
use std::sync::OnceLock;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root canonicalizes")
}

/// Exact producer-selected snapshot bytes. Missing or mismatched identity is
/// terminal; this test helper never generates or imports the corpus.
fn committed_snapshot() -> &'static [u8] {
    static SNAPSHOT: OnceLock<Vec<u8>> = OnceLock::new();
    SNAPSHOT
        .get_or_init(|| {
            gmeow_bundle_import::load_authenticated_source_bytes(&repo_root())
                .expect("load authenticated bundle bytes without rebuilding them")
        })
        .as_slice()
}

fn committed_bundle() -> &'static Bundle {
    static BUNDLE: OnceLock<Bundle> = OnceLock::new();
    BUNDLE.get_or_init(|| {
        Bundle::from_snapshot(committed_snapshot()).expect("fold committed gmeow.gts")
    })
}

fn committed_integrity_report() -> &'static BundleIntegrityReport {
    static REPORT: OnceLock<BundleIntegrityReport> = OnceLock::new();
    REPORT.get_or_init(|| {
        committed_bundle()
            .integrity_report()
            .expect("integrity report")
    })
}

/// Mirrors `tests/test_bundle_blob_integrity.py`: the wheel-mode consumer
/// archives are folded into gmeow.gts as blobs and resolve non-empty. This
/// pins Rust↔producer rep-string agreement — a drifted label would silently
/// resolve to `{}` and ship the bundle without that surface.
fn bundle_carries_the_consumer_archives() {
    let bundle = committed_bundle();
    assert!(
        !bundle.sssom().unwrap().is_empty(),
        "mappings-archive blob missing from gmeow.gts"
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
    let models = bundle.models_python().unwrap();
    assert!(
        !models.is_empty(),
        "models-python blob missing from gmeow.gts"
    );
    assert!(
        models.contains_key("gmeow_models/__init__.py"),
        "models-python blob is missing the package __init__.py member"
    );
}

// (The former `span_table_rep_labels_agree` drift-pin test is gone: the producer
// side now RE-EXPORTS `REP_SPAN_TABLE`/`REP_DIAG_NODES` from this module
// (`gmeow_pipeline::stages::carrier`), so producer and reader are one constant and the label
// cannot drift structurally — a runtime assert_eq of a const against itself guards
// nothing.)

/// Presentation projections are a hard negative contract for the committed
/// logical bundle: they are regenerated externally by `make check-sync SYNC_MODE=update SYNC_OUTPUTS=docs`.
fn documentation_projections_are_absent() {
    let bundle = committed_bundle();
    assert!(
        bundle.ontology_docs().unwrap().is_empty(),
        "ontology-docs must be absent"
    );
    assert!(
        bundle.docs_book().unwrap().is_empty(),
        "docs-book must be absent"
    );
    assert!(
        bundle.docs_print().unwrap().is_empty(),
        "docs-print must be absent"
    );
    assert!(
        bundle.okf().unwrap().is_empty(),
        "okf-export must be absent"
    );
    // NOT `yaml-ld-archive`. It used to be listed here as a documentation
    // projection, which was only true because its writer was `#[cfg(test)]`. It is
    // now a PRODUCTION frame folded by `stage-archive-blobs` (the claim corpus's
    // JSON-LD-star / YAML-LD-star surface), so
    // "absent" is no longer its contract. Its PRESENCE is asserted against a real
    // DAG emission in `tests/medium_bundle.rs`
    // (`the_claim_reps_are_real_frames_primed_by_claims`) rather than against this
    // git-ignored local bundle, which a stale worktree copy would answer wrongly in
    // either direction.

    let schemas = bundle.schemas().unwrap();
    assert!(
        schemas.contains_key("gmeow.schema.json"),
        "schemas-archive carries the JSON Schema"
    );
    assert!(
        bundle.schema().unwrap().is_some(),
        "schema() resolves the JSON Schema payload"
    );
}

fn merged_ttl_reconstructs_non_empty_ntriples() {
    let bundle = committed_bundle();
    let base = bundle.merged_ttl(false).unwrap();
    assert!(!base.is_empty(), "merged N-Triples are non-empty");
    assert!(base.ends_with(b"\n"), "merged N-Triples end with a newline");
    // The guideBlob reference triples are filtered out of the merged graph —
    // i.e. no triple carries `gmeow:guideBlob` in PREDICATE position (the
    // property's own definitional triples, where it is a subject, survive, so
    // a bare substring check would be wrong).
    let text = String::from_utf8(base.clone()).expect("merged graph is UTF-8");
    let guide_predicate = format!(" <{}guideBlob> ", gmeow_ns::GMEOW_NS);
    assert!(
        !text.lines().any(|line| line.contains(&guide_predicate)),
        "guideBlob reference triples are filtered from the merged graph"
    );
    // Including imports never drops assertions.
    let with_imports = bundle.merged_ttl(true).unwrap();
    assert!(with_imports.len() >= base.len());
}

fn absent_rep_resolves_to_empty_not_error() {
    let bundle = committed_bundle();
    assert!(
        bundle.archive("no-such-rep-archive").unwrap().is_empty(),
        "an unknown rep resolves to an empty archive (wheel-only contract)"
    );
    assert!(bundle.blob_by_rep("no-such-rep").unwrap().is_none());
}

fn malformed_snapshot_is_a_hard_error() {
    assert!(Bundle::from_snapshot(b"not a valid gts snapshot").is_err());
}

fn is_slice_mapping_matches_five_segment_ttl() {
    assert!(is_slice_mapping("slices/core/inhabitation/mappings/x.ttl"));
    assert!(!is_slice_mapping("dsl/mappings/equivalences/x.ttl"));
    assert!(!is_slice_mapping("slices/core/inhabitation/shapes.ttl"));
    assert!(!is_slice_mapping("slices/a/b/mappings/x.rq"));
}

/// Regression pin for the scoping decision in [`Bundle::integrity_report`]:
/// `gmeow:contentDigest` and `gmeow:definitionDigest` are domain-free
/// content-hash/fingerprint predicates over the committed bundle (thousands
/// of `gmeow:definitionDigest` triples alone), and legitimately carry
/// `blake3:`-shaped literals with NO promise of a matching bundle blob —
/// only properties whose local name ends in `Blob` are a dereference
/// contract. Scanning by literal shape instead of by name previously
/// flagged this committed bundle's real, correct data as thousands of
/// false "dangling references".
fn integrity_report_does_not_flag_fingerprint_predicates_as_references() {
    let report = committed_integrity_report();
    assert!(
        !report
            .referenced
            .contains_key(&format!("{GMEOW_NS}contentDigest")),
        "gmeow:contentDigest is a content-hash identity predicate, not a blob-store \
             reference — it must not appear in the referenced-digest map"
    );
    assert!(
        !report
            .referenced
            .contains_key(&format!("{GMEOW_NS}definitionDigest")),
        "gmeow:definitionDigest is a term-definition fingerprint, not a blob-store \
             reference — it must not appear in the referenced-digest map"
    );
}

/// Regression pin for the orphan-detection scoping: a stored blob that
/// carries a producer-declared `rep` label (e.g. the SHACL findings/SARIF
/// reports, the compiled shape surfaces) is a legitimate, intentionally
/// shipped blob even when this module exposes no dedicated typed accessor
/// for that particular rep, and even when no graph predicate references
/// its digest by name. Only a blob with NEITHER a reference NOR a
/// declared rep is a genuine orphan.
fn integrity_report_does_not_flag_rep_labeled_blobs_as_orphans() {
    let report = committed_integrity_report();
    let graph = committed_bundle().view.graph();
    for orphan in &report.orphan_blobs {
        let has_rep = graph
            .blob_meta
            .iter()
            .any(|(d, meta)| d == orphan && blob_meta_rep(meta).is_some());
        assert!(
            !has_rep,
            "blob {orphan} carries a producer-declared rep label and must not be \
                 flagged as an orphan"
        );
    }
}

// -- Teeth: each of the three integrity-law failure modes genuinely trips
// `is_clean() == false`, proven on tiny SYNTHETIC snapshots (never the real
// 48 MB committed bundle) so the fixtures stay fixture-scale and on-gate. --

/// Ingest RDF `text` into a fresh [`purrdf::gts_compose::SnapshotBuilder`]
/// (mirrors `gmeow_pipeline::stages::release`'s test builder) — the same
/// single-exit ingestion (`parse_dataset` → `add_dataset`) those fixtures use
/// to author a synthetic snapshot without touching the committed bundle.
fn builder_from(text: &str, media_type: &str) -> purrdf::gts_compose::SnapshotBuilder {
    let dataset = purrdf::parse_dataset(text.as_bytes(), media_type, None).expect("parse fixture");
    let mut b = purrdf::gts_compose::SnapshotBuilder::new();
    b.add_dataset(&dataset).expect("add_dataset");
    b
}

/// TEETH (dangling reference): a `*Blob`-suffixed predicate triple pointing
/// at a digest with NO matching stored blob must trip `is_clean() == false`
/// and land in `report.dangling`. Built via the public `gts_compose` builder
/// surface (`builder_from` + [`purrdf::gts_compose::emit_gts`]) with an empty
/// blob list — this failure mode needs no low-level writer access, since the
/// dangling-ness lives entirely in the graph, not the blob store.
fn integrity_report_flags_a_dangling_blob_reference() {
    use purrdf::gts_compose::{DEFAULT_RSYNCABLE_THRESHOLD, emit_gts};

    let dangling_digest = format!("blake3:{}", "0".repeat(64));
    let nq = format!("<https://e/s> <https://e/testBlob> \"{dangling_digest}\" .\n");
    let b = builder_from(&nq, purrdf::NativeRdfFormat::NTriples.media_type());
    // gmeow-test-input: synthetic-only
    let snapshot = emit_gts(
        &b,
        "dist",
        None,
        Vec::new(),
        Vec::new(),
        None,
        None,
        None,
        DEFAULT_RSYNCABLE_THRESHOLD,
        &purrdf::gts_compose::MediumPlan::dist_default(None),
    )
    .expect("emit synthetic snapshot with no stored blobs");

    let bundle = Bundle::from_snapshot(&snapshot).expect("fold synthetic snapshot");
    let report = bundle.integrity_report().expect("integrity report");
    assert!(
        !report.is_clean(),
        "a *Blob predicate pointing at an unstored digest must trip is_clean() == false"
    );
    let missing = report
        .dangling
        .get("https://e/testBlob")
        .expect("the testBlob predicate is present in the dangling map");
    assert_eq!(missing, &vec![dangling_digest]);
    assert!(
        report.orphan_blobs.is_empty(),
        "no blobs are stored at all, so there can be no orphan"
    );
    assert!(
        report.hash_mismatches.is_empty(),
        "no blobs are stored at all, so there can be no hash mismatch"
    );
}

/// A hand-authored `dist` snapshot: one `blob` frame whose `pub` metadata is
/// supplied VERBATIM (bypassing [`purrdf::gts_compose::BlobRow`]) plus a plain
/// `snapshot` frame from `builder`. This is the shared low-level construction
/// both [`integrity_report_flags_an_orphan_blob`] and
/// [`integrity_report_flags_a_hash_mismatch`] need.
///
/// The argument rests on two upstream INVARIANTS, stated as invariants rather
/// than pinned to a version and a line number — a line pin in a moving
/// dependency is a comment that will silently lie:
///
/// 1. `purrdf::gts_compose::emit_gts` derives every blob frame's `pub.digest`
///    from the blob's OWN bytes (`digest_string(&blob.data)`) and stamps
///    `pub.rep` from `BlobRow::rep`. So neither an orphan (rep-less) nor a
///    hash-mismatched (wrong-digest) blob is constructible through `BlobRow` —
///    every `BlobRow`-authored blob is, by construction, correctly keyed and
///    rep-labeled.
/// 2. The reader takes a `pub.digest`-bearing frame's DECLARED digest as the
///    blob's store key verbatim (normalizing only the `blake3:` prefix form);
///    it never recomputes the hash from the frame's `d` bytes.
///
/// The lower-level
/// [`purrdf::gts::writer::Writer::add_frame_with_options`] (the same call
/// `emit_gts` itself makes) takes an arbitrary
/// `pub_meta` CBOR value, so it is the genuine producer-side seam: a producer
/// that (a) forgets to stamp a `rep` on a blob it stores, or (b) declares a
/// `pub.digest` that does not match its own bytes, is a real bug class this
/// module's integrity law exists to catch — not a bytes-corruption hack.
///
/// Invariant 2 above is what makes this construction reach the law under test:
/// the reader's `pub_digest` accepts the declared text (or 32 raw bytes) as the
/// store key without recomputing it from `d`, while frame processing separately
/// recomputes each frame's OWN self-hash (`"id"`) over the frame's actual bytes
/// (INCLUDING this same `pub_meta` + `"d"`). So a hand-authored frame with a
/// deliberately-wrong
/// declared digest is still a fully self-consistent, chain-valid frame — the
/// frame self-hash law and this crate's blob-keying law check two different
/// things (frame authenticity vs. declared-key-vs-bytes agreement) — which is
/// why this construction, unlike raw-byte tampering of the committed bundle,
/// is never intercepted upstream before `integrity_report()` runs.
fn hand_authored_blob_snapshot(data: Vec<u8>, pub_meta: ciborium::value::Value) -> Vec<u8> {
    use purrdf::gts::writer::{FrameOptions, Writer};

    let nq = "<https://e/s> <https://e/p> <https://e/o> .\n"; // no *Blob predicate at all
    let b = builder_from(nq, purrdf::NativeRdfFormat::NTriples.media_type());

    let mut writer = Writer::new("dist");
    writer
        .add_frame_with_options(
            "blob",
            FrameOptions {
                raw: Some(data),
                pub_meta: Some(pub_meta),
                ..Default::default()
            },
        )
        .expect("add hand-authored blob frame");
    writer
        .add_frame_with_options(
            "snapshot",
            FrameOptions {
                payload: Some(b.snapshot_payload()),
                ..Default::default()
            },
        )
        .expect("add snapshot frame");
    writer.into_bytes()
}

/// TEETH (orphan blob): a stored blob referenced by no `*Blob` predicate AND
/// carrying no producer-declared `rep` label must trip `is_clean() == false`
/// and land in `report.orphan_blobs`. The digest IS correctly keyed (so this
/// test isolates orphan-ness from the hash-mismatch law below).
fn integrity_report_flags_an_orphan_blob() {
    use ciborium::value::Value;
    use purrdf::gts::writer::digest_string;

    let data = b"{\"orphan\":true}".to_vec();
    let digest = digest_string(&data);
    let pub_meta = Value::Map(vec![
        ("digest".into(), Value::Text(digest.clone())),
        ("mt".into(), Value::Text("application/json".to_string())),
        // Deliberately no "rep" entry: nobody declared why this blob exists.
    ]);
    let snapshot = hand_authored_blob_snapshot(data, pub_meta);

    let bundle = Bundle::from_snapshot(&snapshot).expect("fold hand-authored snapshot");
    let report = bundle.integrity_report().expect("integrity report");
    assert!(
        !report.is_clean(),
        "an unreferenced, rep-less stored blob must trip is_clean() == false"
    );
    assert_eq!(report.orphan_blobs, vec![digest]);
    assert!(
        report.hash_mismatches.is_empty(),
        "the orphan blob's digest is correctly keyed; only orphan-ness should fire"
    );
    assert!(report.dangling.values().all(Vec::is_empty));
}

/// TEETH (hash mismatch): a stored blob whose declared `pub.digest` does NOT
/// equal `blake3(decoded bytes)` must trip `is_clean() == false` and land in
/// `report.hash_mismatches`. The blob DOES carry a `rep` (so this test
/// isolates the mismatch from the orphan law above — a rep-labeled blob must
/// never also be flagged as an orphan, per the existing regression pin).
fn integrity_report_flags_a_hash_mismatch() {
    use ciborium::value::Value;
    use purrdf::gts::writer::digest_string;

    let data = b"{\"real\":\"bytes\"}".to_vec();
    let real_digest = digest_string(&data);
    let bogus_digest = format!("blake3:{}", "1".repeat(64));
    assert_ne!(
        real_digest, bogus_digest,
        "sanity: the declared digest is genuinely wrong"
    );
    let pub_meta = Value::Map(vec![
        ("digest".into(), Value::Text(bogus_digest.clone())),
        ("mt".into(), Value::Text("application/json".to_string())),
        ("rep".into(), Value::Text("mismatched-report".to_string())),
    ]);
    let snapshot = hand_authored_blob_snapshot(data, pub_meta);

    let bundle = Bundle::from_snapshot(&snapshot).expect("fold hand-authored snapshot");
    let report = bundle.integrity_report().expect("integrity report");
    assert!(
        !report.is_clean(),
        "a declared digest that != blake3(decoded bytes) must trip is_clean() == false"
    );
    assert_eq!(report.hash_mismatches, vec![(bogus_digest, real_digest)]);
    assert!(
        report.orphan_blobs.is_empty(),
        "the blob carries a rep, so it must not ALSO be flagged as an orphan"
    );
    assert!(report.dangling.values().all(Vec::is_empty));
}

/// One process owns the bundle-reader contract table. This makes the parsed
/// committed bundle and its whole-store integrity walk true shared intermediates
/// instead of repeating both in a fresh nextest process for every assertion.
#[test]
fn bundle_reader_contracts_share_one_authenticated_parse() {
    let cases: &[(&str, fn())] = &[
        (
            "bundle_carries_the_consumer_archives",
            bundle_carries_the_consumer_archives,
        ),
        (
            "documentation_projections_are_absent",
            documentation_projections_are_absent,
        ),
        (
            "merged_ttl_reconstructs_non_empty_ntriples",
            merged_ttl_reconstructs_non_empty_ntriples,
        ),
        (
            "absent_rep_resolves_to_empty_not_error",
            absent_rep_resolves_to_empty_not_error,
        ),
        (
            "malformed_snapshot_is_a_hard_error",
            malformed_snapshot_is_a_hard_error,
        ),
        (
            "is_slice_mapping_matches_five_segment_ttl",
            is_slice_mapping_matches_five_segment_ttl,
        ),
        (
            "integrity_report_does_not_flag_fingerprint_predicates_as_references",
            integrity_report_does_not_flag_fingerprint_predicates_as_references,
        ),
        (
            "integrity_report_does_not_flag_rep_labeled_blobs_as_orphans",
            integrity_report_does_not_flag_rep_labeled_blobs_as_orphans,
        ),
        (
            "integrity_report_flags_a_dangling_blob_reference",
            integrity_report_flags_a_dangling_blob_reference,
        ),
        (
            "integrity_report_flags_an_orphan_blob",
            integrity_report_flags_an_orphan_blob,
        ),
        (
            "integrity_report_flags_a_hash_mismatch",
            integrity_report_flags_a_hash_mismatch,
        ),
    ];
    let mut failures = Vec::new();
    for (name, case) in cases {
        if let Err(payload) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(*case)) {
            let detail = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| {
                    payload
                        .downcast_ref::<&str>()
                        .map(|text| (*text).to_string())
                })
                .unwrap_or_else(|| "non-string panic".to_string());
            failures.push(format!("{name}: {detail}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} bundle-reader contract(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
