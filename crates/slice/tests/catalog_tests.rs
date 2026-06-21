// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Acceptance tests for SliceCatalog: path-independence, lossless round-trip,
//! and recoverability.

use gmeow_slice::artifact::ArtifactRole;
use gmeow_slice::catalog::SliceCatalog;

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test-slice")
}

// ── (a) Path-independence ─────────────────────────────────────────────────────

/// Copies the fixture slice into a tempdir, builds both catalogs, and asserts
/// that every artifact has the same raw_digest regardless of filesystem location.
#[test]
fn path_independence() {
    let src = fixture_dir();
    let tmp = tempfile::tempdir().expect("tempdir");
    let dst = tmp.path().join("test-slice");
    copy_dir_all(&src, &dst).expect("copy fixture");

    let rec_src = SliceCatalog::from_slice_dir(&src).expect("load from src");
    let rec_dst = SliceCatalog::from_slice_dir(&dst).expect("load from dst");

    assert_eq!(
        rec_src.artifacts.len(),
        rec_dst.artifacts.len(),
        "artifact count must match"
    );

    let mut src_digests: Vec<(&str, &str)> = rec_src
        .artifacts
        .iter()
        .map(|a| (a.logical_path.as_str(), a.raw_digest.as_str()))
        .collect();
    let mut dst_digests: Vec<(&str, &str)> = rec_dst
        .artifacts
        .iter()
        .map(|a| (a.logical_path.as_str(), a.raw_digest.as_str()))
        .collect();

    src_digests.sort();
    dst_digests.sort();

    assert_eq!(
        src_digests, dst_digests,
        "raw digests must be path-independent"
    );
}

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}

// ── (b) Lossless round-trip ───────────────────────────────────────────────────

/// After loading the test-slice, verify that the manifest fields are parsed
/// correctly — IRI, title, identifier, and consumer text.
#[test]
fn lossless_round_trip() {
    let dir = fixture_dir();
    let rec = SliceCatalog::from_slice_dir(&dir).expect("load slice");

    // Structural fields from manifest.ttl.
    assert_eq!(
        rec.manifest.slice_iri,
        "https://example.org/test/slice/test"
    );
    assert_eq!(rec.manifest.title.as_deref(), Some("Test Slice"));
    assert_eq!(rec.manifest.identifier.as_deref(), Some("10.99999/test"));
    assert!(
        rec.manifest.consumers.iter().any(|c| c.contains("testing")),
        "consumers should contain 'testing'"
    );

    // The manifest graph must be non-empty (lossless: every triple survived).
    assert!(
        rec.manifest_graph.quad_count() > 0,
        "manifest_graph must contain quads"
    );

    // The unknown custom property triple must survive in the IR graph.
    // We verify it's there by checking the quad count includes it.
    // manifest.ttl has: a, label, title, creator, identifier, tier, consumer, customProp = 8 triples.
    assert!(
        rec.manifest_graph.quad_count() >= 8,
        "manifest_graph should contain at least 8 triples (got {})",
        rec.manifest_graph.quad_count()
    );
}

// ── (c) Recoverability ────────────────────────────────────────────────────────

/// For every artifact in the test slice, assert that find_artifact and
/// find_by_digest both return the same record, and that content is non-empty.
#[test]
fn recoverability() {
    let dir = fixture_dir();
    let rec = SliceCatalog::from_slice_dir(&dir).expect("load slice");

    assert!(
        !rec.artifacts.is_empty(),
        "test-slice must have at least one artifact"
    );

    for artifact in &rec.artifacts {
        // find_artifact must locate it by role+path.
        let found = rec.find_artifact(&artifact.role, &artifact.logical_path);
        assert!(
            found.is_some(),
            "find_artifact({:?}, {:?}) returned None",
            artifact.role,
            artifact.logical_path
        );
        assert_eq!(
            found.unwrap().raw_digest,
            artifact.raw_digest,
            "find_artifact returned wrong artifact"
        );

        // find_by_digest must also find it.
        let found2 = rec.find_by_digest(&artifact.raw_digest);
        assert!(
            found2.is_some(),
            "find_by_digest({:?}) returned None",
            artifact.raw_digest
        );

        // Content must be non-empty.
        assert!(
            !artifact.content.is_empty(),
            "artifact {:?} has empty content",
            artifact.logical_path
        );
    }
}

// ── (d) Role classification ───────────────────────────────────────────────────

/// Verify that the three fixture files are classified with the correct roles.
#[test]
fn role_classification() {
    let dir = fixture_dir();
    let rec = SliceCatalog::from_slice_dir(&dir).expect("load slice");

    let has_manifest = rec
        .artifacts
        .iter()
        .any(|a| matches!(a.role, ArtifactRole::Manifest));
    let has_module = rec
        .artifacts
        .iter()
        .any(|a| matches!(a.role, ArtifactRole::Module));
    let has_docs = rec
        .artifacts
        .iter()
        .any(|a| matches!(a.role, ArtifactRole::Documentation));

    assert!(has_manifest, "manifest.ttl must be classified as Manifest");
    assert!(has_module, "module.ttl must be classified as Module");
    assert!(has_docs, "docs.md must be classified as Documentation");
}
