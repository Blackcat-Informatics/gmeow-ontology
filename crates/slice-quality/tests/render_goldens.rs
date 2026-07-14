// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Frozen byte-goldens for the four slice-quality renderings (issue AC9,
//! "golden-tested"): text, JSON, SARIF, and RDF (N-Quads).
//!
//! Determinism is asserted inline elsewhere; this test adds the missing regression
//! net — a committed byte-golden per rendering, so a change to any render FORMAT is
//! caught as a diff against the frozen bytes rather than passing silently.
//!
//! Robustness: the goldens are NOT taken over the repo-wide `--all` output (81
//! slices, brittle — it changes whenever any slice changes). They are taken over a
//! tiny, self-contained fixture slice (`tests/fixtures/sample-slice/`) whose
//! identity is a stable absolute IRI, so the goldens are machine-independent and
//! change only when a render format changes.
//!
//! Update mechanism: mirrors the repo's existing byte-file golden convention
//! (`crates/foundation-corpus/tests/golden.rs` reads committed golden files and
//! compares bytes) plus the conformance-suite bless-env pattern
//! (`GMEOW_CONFORMANCE_BLESS`). Run `UPDATE_GOLDENS=1 cargo test -p
//! gmeow-slice-quality --test render_goldens` to regenerate the four goldens from
//! the REAL output of the committed code (never hand-authored idealized bytes).

use std::path::{Path, PathBuf};

use gmeow_errors::render::{to_json, to_sarif};
use gmeow_slice_quality::ScoringEnv;
use gmeow_slice_quality::report::{SliceReport, score_slice_with_standard};

/// Score a slice against the repo rubric's measurement standard, in repo mode — the
/// in-repo replacement for the retired `score_slice(root, dir)`.
fn score(root: &Path, dir: &Path) -> gmeow_errors::Result<SliceReport> {
    let module = root.join("slices/core/slice-quality-rubric/module.ttl");
    let ds = gmeow_slice_quality::dataset_from_paths(&[&module])?;
    let standard = gmeow_slice_quality::rubric::load_rubric(&ds)?.standard;
    score_slice_with_standard(dir, &standard, ScoringEnv::Repo)
}

/// The env var that flips the test into bless mode: write the produced output back
/// to the golden files instead of asserting against them.
const UPDATE_ENV: &str = "UPDATE_GOLDENS";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample-slice")
}

fn goldens_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/goldens")
}

/// Render the fixture slice all four ways: (name, produced-bytes).
fn render_all(report: &SliceReport) -> Vec<(&'static str, String)> {
    let diag = report.to_report();
    vec![
        ("sample-slice.text", report.render_text()),
        (
            "sample-slice.json",
            to_json(&diag).expect("JSON render succeeds"),
        ),
        (
            "sample-slice.sarif",
            to_sarif(&diag).expect("SARIF render succeeds"),
        ),
        ("sample-slice.rdf.nq", report.to_gmeow_rdf()),
    ]
}

/// A produced golden must carry no machine-specific bytes: no absolute home/tmp
/// path and no worktree path. The renderings are functions of the slice IRI and
/// term IRIs (all stable absolute ontology IRIs), never of the fixture's on-disk
/// location — this pins that invariant so a future render change that leaks a path
/// is caught here, not shipped in a golden.
fn assert_no_machine_paths(name: &str, produced: &str) {
    let worktree = env!("CARGO_MANIFEST_DIR");
    for needle in ["/home/", "/tmp/", "/private/var/", worktree] {
        assert!(
            !produced.contains(needle),
            "{name} render leaks a machine-specific path (`{needle}`) — the golden would not be reproducible; fix the render input",
        );
    }
}

#[test]
fn four_renderings_match_committed_byte_goldens() {
    let report = score(&repo_root(), &fixture_dir()).expect("the fixture slice scores");
    let goldens = goldens_dir();
    let bless = std::env::var(UPDATE_ENV).is_ok_and(|v| !v.is_empty() && v != "0");

    let produced = render_all(&report);

    // First: no rendering may contain a machine-specific path, whether we are
    // blessing or asserting — a leaked path must never be written into a golden.
    for (name, bytes) in &produced {
        assert_no_machine_paths(name, bytes);
    }

    if bless {
        std::fs::create_dir_all(&goldens).expect("create goldens dir");
        for (name, bytes) in &produced {
            std::fs::write(goldens.join(name), bytes.as_bytes())
                .unwrap_or_else(|e| panic!("write golden {name}: {e}"));
        }
        // A bless run does not also assert (the files were just overwritten); a
        // normal `cargo test` run (no env) exercises the byte-for-byte compare.
        return;
    }

    for (name, bytes) in &produced {
        let golden = read_golden(&goldens, name);
        assert_eq!(
            bytes.as_bytes(),
            golden.as_slice(),
            "byte mismatch in {name} — the render format changed. If intentional, regenerate with `UPDATE_GOLDENS=1 cargo test -p gmeow-slice-quality --test render_goldens`.\n--- produced ---\n{bytes}\n--- golden ---\n{}",
            String::from_utf8_lossy(&golden),
        );
    }
}

fn read_golden(dir: &Path, name: &str) -> Vec<u8> {
    std::fs::read(dir.join(name)).unwrap_or_else(|e| {
        panic!(
            "missing golden {name}: {e} — generate it with `UPDATE_GOLDENS=1 cargo test -p gmeow-slice-quality --test render_goldens`"
        )
    })
}

#[test]
fn renderings_are_byte_identical_across_two_calls() {
    // Determinism is a hard requirement: the tool is a gate input and a golden
    // source. Two independent scorings of the same fixture must render identically.
    let a = score(&repo_root(), &fixture_dir()).expect("scores");
    let b = score(&repo_root(), &fixture_dir()).expect("scores");
    let ra = render_all(&a);
    let rb = render_all(&b);
    for ((name, bytes_a), (_, bytes_b)) in ra.iter().zip(rb.iter()) {
        assert_eq!(bytes_a, bytes_b, "{name} render is non-deterministic");
    }
}
