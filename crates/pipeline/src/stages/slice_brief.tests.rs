// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// The stage attaches EXACTLY the authoring-briefs graph, carrying real
/// `gmeow:AuthoringPacket` triples — the proof the packet corpus reaches the carrier
/// (and thence `gmeow.gts`), not merely a test.
#[test]
fn run_attaches_the_authoring_briefs_graph() {
    let root = repo_root();
    let product = crate::fixture::stage_fixture(
        &root,
        std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(1),
        "stage-slice-brief",
    )
    .expect("exact slice-brief fixture")
    .outcome
    .product;
    let dataset = product.dataset();
    let projected = dataset.project_named_graph(GRAPH_AUTHORING_BRIEFS);
    assert!(
        projected.quad_count() > 0,
        "graph/authoring-briefs must carry the assembled packet corpus"
    );
    // The corpus must carry the packet type (proof the packets, not stray triples).
    let n = count_packets(&projected);
    assert!(
        n > 0,
        "graph/authoring-briefs must carry real gmeow:AuthoringPacket individuals, got {n}"
    );
}

fn count_packets(ds: &RdfDataset) -> usize {
    graph::instances_of(ds, &graph::g("AuthoringPacket")).len()
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// A missing directory is a legitimate "absent" input to [`collect_ext`]:
/// `Ok(())`, nothing collected — never an error.
#[test]
fn collect_ext_absent_directory_is_ok_and_empty() {
    let temp = tempfile::tempdir().expect("tempdir");
    let dir = temp.path().join("does-not-exist");
    assert!(!dir.exists(), "precondition: {dir:?} must not exist");

    let mut out = Vec::new();
    let result = collect_ext(&dir, "ttl", &mut out);

    assert!(
        result.is_ok(),
        "a NotFound read_dir must be treated as absent (Ok), got {result:?}"
    );
    assert!(
        out.is_empty(),
        "an absent directory must collect zero paths, got {out:?}"
    );
}

/// A `read_dir` failure that is NOT `NotFound` (here: the parent path
/// component is a plain file, so the OS refuses with `NotADirectory`/`ENOTDIR`)
/// MUST propagate as an `Err` from [`collect_ext`], never be laundered into
/// "no mappings / no translations here". Deterministic — does not depend on
/// running as non-root (unlike a permission-bits test, which root would bypass).
#[test]
fn collect_ext_non_directory_parent_errors() {
    let temp = tempfile::tempdir().expect("tempdir");
    let marker_file = temp.path().join("marker");
    std::fs::write(&marker_file, b"not a directory").expect("write marker file");

    // `marker_file` is a plain file, so `marker_file/mappings` cannot be a
    // directory: `read_dir` must fail with something other than `NotFound`.
    let bogus_dir = marker_file.join("mappings");
    let mut out = Vec::new();
    let result = collect_ext(&bogus_dir, "ttl", &mut out);

    assert!(
        result.is_err(),
        "a non-NotFound read_dir error must propagate as Err, got {result:?}"
    );
    assert!(
        out.is_empty(),
        "no paths must be collected on the error path, got {out:?}"
    );
}

/// The stage's ONE dependency set is the four generated-shape producers, so the
/// scheduler orders it AFTER they materialize `generated/shapes/` (the ordering that
/// stops a no-consumes leaf from cold-failing the union enumeration) and keys its
/// generated union members on their product digests rather than a `generated/` disk
/// read (the stale-disk-fold class).
#[test]
fn new_consumes_the_generated_shape_producers() {
    let stage = SliceBriefStage::new();
    assert_eq!(
        stage.consumes(),
        crate::stages::shape_union_fresh::producer_consumes().as_slice(),
        "slice-brief must consume exactly the generated-shape producers for the fresh union"
    );
}

/// The cache key declares NO `generated/` path: the shape union's generated members
/// are product-sourced off the consumed producers (their digests are the change
/// basis), so a `generated/shapes/*.ttl` file NEVER appears in `input_files` — a
/// `generated/` path there is itself the stale-disk-fold bug class. Proven on a temp
/// root (no real slices, a single on-disk generated member so `shape_files` succeeds),
/// so it needs no materialized repo.
#[test]
fn input_files_declares_no_generated_path() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    // The AUTHORED half `shape_files` requires: an authored shapes/ member and at
    // least one generated/shapes/ member on disk (its fail-closed non-empty gate).
    std::fs::create_dir_all(root.join("shapes")).unwrap();
    std::fs::write(root.join("shapes/authored.ttl"), b"# authored shape\n").unwrap();
    std::fs::create_dir_all(root.join("generated/shapes")).unwrap();
    std::fs::write(
        root.join("generated/shapes/validation-shapes.ttl"),
        b"# generated shape\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("slices")).unwrap();

    let files = SliceBriefStage::new()
        .input_files(root)
        .expect("input_files enumerates the authored cache-key basis");
    assert!(
        !files.is_empty(),
        "the authored shape members must still be declared, got {files:?}"
    );
    for f in &files {
        let rel = f.strip_prefix(root).unwrap_or(f.as_path());
        assert!(
            !rel.starts_with("generated"),
            "no generated/ path may enter the cache key (stale-disk-fold class): {}",
            f.display()
        );
    }
    // The authored generated-shapes member on disk is deliberately NOT declared.
    assert!(
        files
            .iter()
            .all(|f| !f.ends_with("generated/shapes/validation-shapes.ttl")),
        "the on-disk generated union member must be product-sourced, never a cache-key input"
    );
}
