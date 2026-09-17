// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_slice_quality::gate::axis_floor_monotonicity;
use std::process::Command;

const NS: &str = "https://blackcatinformatics.ca/gmeow/";

/// A structurally-complete minimal rubric module (a two-rung ladder, one axis, one
/// threshold) — the CENTRALIZED authority the base reconstruction reads from the
/// rubric slice.
fn rubric_module() -> String {
    format!(
        r#"@prefix gmeow: <{NS}> .
gmeow:tierRegistered a gmeow:QualityTier ; gmeow:tierRank 0 .
gmeow:tierGrounded a gmeow:QualityTier ; gmeow:tierRank 1 .
gmeow:axisGmn1Coverage a gmeow:QualityAxis ;
    gmeow:axisProducer "gmn1_coverage_axis" ;
    gmeow:axisDimension gmeow:dimGmn ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ;
    gmeow:axisThreshold gmeow:thrGmn .
gmeow:thrGmn a gmeow:AxisThreshold ;
    gmeow:thresholdTier gmeow:tierRegistered ;
    gmeow:thresholdFloor 0.0 .
"#
    )
}

/// A DEMO (non-rubric) slice `module.ttl` authoring one `gmeow:AxisFloorCommitment`
/// against the demo slice on `axisGmn1Coverage` at `floor`.
fn demo_module(floor: &str) -> String {
    format!(
        r#"@prefix gmeow: <{NS}> .
gmeow:afc-demo a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceDemo ;
    gmeow:floorAxis gmeow:axisGmn1Coverage ;
    gmeow:floorValue {floor} .
"#
    )
}

/// A git repo fixture whose tree lives in an owned temp directory: dropping the
/// fixture drops the [`tempfile::TempDir`], which removes the tree — on success,
/// on early return, and on panic alike.
struct GitFixture {
    _tmp: tempfile::TempDir,
    root: std::path::PathBuf,
}

/// Run a git command in `root`, isolated from user/system config (never signs).
fn git(root: &std::path::Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(root)
        .env("LC_ALL", "C")
        .env("HOME", root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Build a git repo fixture holding the rubric slice + a demo slice authoring a
/// non-rubric floor at `base_floor`, commit it (the merge base), and return the
/// fixture and the base commit SHA. The caller then rewrites the working tree.
fn fixture_with_base_floor(base_floor: &str) -> (GitFixture, String) {
    let tmp = tempfile::Builder::new()
        .prefix("gmeow-basemono-")
        .tempdir()
        .expect("create temp dir");
    let root = tmp.path().to_path_buf();
    let fx = GitFixture {
        _tmp: tmp,
        root: root.clone(),
    };

    let rubric_dir = root.join("slices/core/slice-quality-rubric");
    let demo_dir = root.join("slices/demo/demo");
    std::fs::create_dir_all(&rubric_dir).unwrap();
    std::fs::create_dir_all(&demo_dir).unwrap();
    std::fs::write(rubric_dir.join("manifest.ttl"), "# rubric slice\n").unwrap();
    std::fs::write(rubric_dir.join("module.ttl"), rubric_module()).unwrap();
    std::fs::write(demo_dir.join("manifest.ttl"), "# demo slice\n").unwrap();
    std::fs::write(demo_dir.join("module.ttl"), demo_module(base_floor)).unwrap();

    git(&root, &["init", "-q"]);
    git(&root, &["config", "user.email", "test@example.com"]);
    git(&root, &["config", "user.name", "Test"]);
    git(&root, &["config", "commit.gpgsign", "false"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "base"]);
    let out = Command::new("git")
        .current_dir(&root)
        .env("HOME", &root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git rev-parse runs");
    assert!(out.status.success(), "git rev-parse failed");
    let base = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (fx, base)
}

fn demo_key() -> (String, String) {
    (format!("{NS}sliceDemo"), "axisGmn1Coverage".to_owned())
}

#[test]
fn base_reconstruction_sees_a_non_rubric_floor_and_reds_a_lowering() {
    // Base commit authors a non-rubric floor at 0.90; working tree lowers it to 0.50.
    let (fx, base) = fixture_with_base_floor("0.9");
    std::fs::write(
        fx.root.join("slices/demo/demo/module.ttl"),
        demo_module("0.5"),
    )
    .unwrap();

    // The base rubric reconstructed over ALL slices' module.ttl at base MUST carry
    // the demo slice's 0.90 floor — proving the multi-slice git-show base
    // reconstruction sees floors authored OUTSIDE the rubric module (the whole point
    // of the widening; the single-file base read never saw it).
    let base_rubric = base_rubric_at(&fx.root, &base)
        .expect("base reconstruction succeeds")
        .expect("rubric module present at base");
    let base_axis = axis_floors_from_rubric(&base_rubric).unwrap();
    assert_eq!(
        base_axis.get(&demo_key()).copied(),
        Some(0.9),
        "base reconstruction must see the non-rubric slice's floor"
    );

    // The working set, through the real segregated loader, carries 0.50.
    let work_rubric = gmeow_slice_quality::load_repo_rubric(&fx.root).unwrap();
    let work_axis = axis_floors_from_rubric(&work_rubric).unwrap();
    assert_eq!(work_axis.get(&demo_key()).copied(), Some(0.5));

    // The monotonicity comparator, fed the REAL reconstructed base map, reds the
    // lowering and names the non-rubric slice.
    let mono =
        axis_floor_monotonicity(GOVERNANCE_SOURCE_LABEL, &base_axis, &work_axis, |_, _| true);
    assert!(
        mono.violations
            .iter()
            .any(|v| v.contains("sliceDemo") && v.contains("LOWERED")),
        "a lowered non-rubric floor must red naming the slice: {:?}",
        mono.violations
    );
}

#[test]
fn base_reconstruction_reds_a_still_live_non_rubric_floor_deletion() {
    let (fx, base) = fixture_with_base_floor("0.9");
    // Working tree DELETES the demo floor entirely (only the prefix line remains).
    std::fs::write(
        fx.root.join("slices/demo/demo/module.ttl"),
        format!("@prefix gmeow: <{NS}> .\n"),
    )
    .unwrap();

    let base_rubric = base_rubric_at(&fx.root, &base).unwrap().unwrap();
    let base_axis = axis_floors_from_rubric(&base_rubric).unwrap();
    let work_rubric = gmeow_slice_quality::load_repo_rubric(&fx.root).unwrap();
    let work_axis = axis_floors_from_rubric(&work_rubric).unwrap();
    assert_eq!(base_axis.get(&demo_key()).copied(), Some(0.9));
    assert_eq!(work_axis.get(&demo_key()).copied(), None);

    // The slice is still live → deleting its committed floor is a hard violation.
    let mono =
        axis_floor_monotonicity(GOVERNANCE_SOURCE_LABEL, &base_axis, &work_axis, |_, _| true);
    assert!(
        mono.violations
            .iter()
            .any(|v| v.contains("sliceDemo") && v.contains("DELETED")),
        "a still-live non-rubric floor deletion must red naming the slice: {:?}",
        mono.violations
    );
}

// -------------------------------------------------------------------------
// The GRANDFATHER gate's base measurement, over a REAL materialized base tree.
// -------------------------------------------------------------------------

const DEMO_SLICE: &str = "https://blackcatinformatics.ca/gmeow/sliceDemo";
const FRESH_SLICE: &str = "https://blackcatinformatics.ca/gmeow/sliceFresh";

fn shapes_doc(locals: &[&str]) -> String {
    let mut out = format!("@prefix sh: <http://www.w3.org/ns/shacl#> .\n@prefix gmeow: <{NS}> .\n");
    for local in locals {
        out.push_str(&format!("gmeow:{local} a sh:NodeShape .\n"));
    }
    out
}

/// A git repo whose BASE commit carries real ratchet AUTHORING surfaces — a slice
/// `shapes.ttl`, a DEEPLY NESTED `mappings/` file, and the repo-level
/// `dsl/mappings/` surface — and whose WORKING TREE has deleted every one of them
/// and added a brand-new slice directory that never existed at base.
fn fixture_with_base_surfaces() -> (GitFixture, String) {
    let tmp = tempfile::Builder::new()
        .prefix("gmeow-basesurf-")
        .tempdir()
        .expect("create temp dir");
    let root = tmp.path().to_path_buf();
    let fx = GitFixture {
        _tmp: tmp,
        root: root.clone(),
    };

    let demo_dir = root.join("slices/demo/demo");
    std::fs::create_dir_all(demo_dir.join("mappings/nested")).unwrap();
    std::fs::write(demo_dir.join("manifest.ttl"), "# demo slice\n").unwrap();
    std::fs::write(
        demo_dir.join("module.ttl"),
        format!("@prefix gmeow: <{NS}> .\n"),
    )
    .unwrap();
    std::fs::write(demo_dir.join("shapes.ttl"), shapes_doc(&["BaseA", "BaseB"])).unwrap();
    std::fs::write(
        demo_dir.join("mappings/nested/extra.ttl"),
        shapes_doc(&["BaseC"]),
    )
    .unwrap();
    let dsl_dir = root.join("dsl/mappings");
    std::fs::create_dir_all(&dsl_dir).unwrap();
    std::fs::write(dsl_dir.join("transforms.ttl"), shapes_doc(&["BaseD"])).unwrap();

    git(&root, &["init", "-q"]);
    git(&root, &["config", "user.email", "test@example.com"]);
    git(&root, &["config", "user.name", "Test"]);
    git(&root, &["config", "commit.gpgsign", "false"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "base surfaces"]);
    let out = Command::new("git")
        .current_dir(&root)
        .env("HOME", &root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git rev-parse runs");
    assert!(out.status.success(), "git rev-parse failed");
    let base = String::from_utf8_lossy(&out.stdout).trim().to_owned();

    // The WORKING tree keeps nothing of the base authoring surfaces, so any residue
    // the base measurement reports can only have come out of the materialized base
    // tree — never out of the files sitting on disk.
    std::fs::remove_file(demo_dir.join("shapes.ttl")).unwrap();
    std::fs::remove_dir_all(demo_dir.join("mappings")).unwrap();
    std::fs::remove_dir_all(&dsl_dir).unwrap();
    // A brand-new slice directory that does not exist at base at all.
    let fresh_dir = root.join("slices/demo/fresh");
    std::fs::create_dir_all(&fresh_dir).unwrap();
    std::fs::write(fresh_dir.join("manifest.ttl"), "# fresh slice\n").unwrap();
    std::fs::write(fresh_dir.join("shapes.ttl"), shapes_doc(&["FreshA"])).unwrap();

    (fx, base)
}

fn demo_slices(root: &std::path::Path) -> Vec<(std::path::PathBuf, String)> {
    vec![
        (root.join("slices/demo/demo"), DEMO_SLICE.to_owned()),
        (root.join("slices/demo/fresh"), FRESH_SLICE.to_owned()),
    ]
}

#[test]
fn base_residue_is_read_from_the_materialized_base_tree() {
    let (fx, base) = fixture_with_base_surfaces();
    let vocabularies = vec![gmeow_slice_quality::counting::shacl_vocab()];
    let owned = demo_slices(&fx.root);
    let slices: Vec<(&Path, String)> = owned
        .iter()
        .map(|(d, iri)| (d.as_path(), iri.clone()))
        .collect();
    let needed: std::collections::BTreeSet<String> = [
        DEMO_SLICE.to_owned(),
        FRESH_SLICE.to_owned(),
        gmeow_slice_quality::DSL_MAPPING_SURFACE_IRI.to_owned(),
    ]
    .into_iter()
    .collect();

    let measured = measure_base_residues(&fx.root, &base, &vocabularies, &needed, &slices)
        .unwrap()
        .counts();

    // shapes.ttl (2) + the DEEPLY NESTED mappings file (1) — proving the base tree is
    // scanned by the very same recursive `ratchet_surface_paths` the working tree uses.
    assert_eq!(
        measured
            .get(&(DEMO_SLICE.to_owned(), "sh".to_owned()))
            .copied(),
        Some(3),
        "{measured:?}"
    );
    // The repo-level dsl/mappings surface is measured from the same materialized tree.
    assert_eq!(
        measured
            .get(&(
                gmeow_slice_quality::DSL_MAPPING_SURFACE_IRI.to_owned(),
                "sh".to_owned()
            ))
            .copied(),
        Some(1),
        "{measured:?}"
    );
    // A slice directory that does not exist at base contributes NOTHING — the caller
    // reads that as base residue 0, never as the working tree's freshly-authored 1.
    assert!(
        !measured.contains_key(&(FRESH_SLICE.to_owned(), "sh".to_owned())),
        "{measured:?}"
    );
}

#[test]
fn nothing_needed_does_no_git_work_at_all() {
    let (fx, _) = fixture_with_base_surfaces();
    let vocabularies = vec![gmeow_slice_quality::counting::shacl_vocab()];
    let owned = demo_slices(&fx.root);
    let slices: Vec<(&Path, String)> = owned
        .iter()
        .map(|(d, iri)| (d.as_path(), iri.clone()))
        .collect();
    // An unresolvable base ref would hard-fail if git were consulted; with no
    // implicated slice the whole reconstruction is skipped before that can happen.
    let measured = measure_base_residues(
        &fx.root,
        "0000000000000000000000000000000000000000",
        &vocabularies,
        &std::collections::BTreeSet::new(),
        &slices,
    )
    .unwrap();
    assert!(measured.constructs.is_empty() && measured.tree.is_none());
}

#[test]
fn an_unusable_base_ref_hard_fails_rather_than_measuring_zero() {
    let (fx, _) = fixture_with_base_surfaces();
    let vocabularies = vec![gmeow_slice_quality::counting::shacl_vocab()];
    let owned = demo_slices(&fx.root);
    let slices: Vec<(&Path, String)> = owned
        .iter()
        .map(|(d, iri)| (d.as_path(), iri.clone()))
        .collect();
    let needed: std::collections::BTreeSet<String> = [DEMO_SLICE.to_owned()].into_iter().collect();
    assert!(
        measure_base_residues(
            &fx.root,
            "0000000000000000000000000000000000000000",
            &vocabularies,
            &needed,
            &slices,
        )
        .is_err(),
        "a base tree that cannot be materialized must HARD FAIL — a silent residue 0 \
             would grandfather freshly-authored constructs for free"
    );
}
