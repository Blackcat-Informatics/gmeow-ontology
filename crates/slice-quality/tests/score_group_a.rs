// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Score the real rubric slice with the group-A primitives.
//!
//! The rubric slice is its own exemplar, so its provenance-honesty score must be
//! a perfect 1.0 — none of its rationales name a test artifact. This is the
//! executable form of the "a test is not a rationale" acceptance criterion.

use std::path::{Path, PathBuf};

use gmeow_slice_quality::axes;
use gmeow_slice_quality::score::{ScoreContext, ScoringEnv};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn slice_dir() -> PathBuf {
    repo_root().join("slices/core/slice-quality-rubric")
}

/// Assemble the slice's own graph: module + examples + tests (so rationales in the
/// test cells are visible to the provenance-honesty primitive). Uses the crate's
/// SINGLE path-collection authority so the test graph matches the scored graph.
fn slice_graph() -> std::sync::Arc<purrdf::RdfDataset> {
    let paths = gmeow_slice_quality::report::slice_ttl_paths(&slice_dir());
    let refs: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
    gmeow_slice_quality::dataset_from_paths(&refs).expect("slice graph parses")
}

#[test]
fn provenance_honesty_is_perfect_on_its_own_exemplar() {
    let ds = slice_graph();
    let ctx = ScoreContext::new(
        "https://blackcatinformatics.ca/gmeow/slices/slice-quality-rubric".to_owned(),
        slice_dir(),
        &ds,
        ScoringEnv::Repo,
    );
    let prov = axes::resolve("provenance_honesty").unwrap()(&ctx);
    assert_eq!(
        prov.score,
        1.0,
        "the rubric slice names no test artifact in any rationale; findings: {}",
        prov.findings.len()
    );
    assert!(
        prov.findings.is_empty(),
        "no provenance advisories on the exemplar"
    );
}

#[test]
fn group_a_axes_produce_real_scores() {
    let ds = slice_graph();
    let ctx = ScoreContext::new(
        "https://blackcatinformatics.ca/gmeow/slices/slice-quality-rubric".to_owned(),
        slice_dir(),
        &ds,
        ScoringEnv::Repo,
    );
    assert!(!ctx.terms.is_empty(), "the slice has authored terms");

    for producer in ["grounding_axis", "information_axis", "prose_axis"] {
        let result = axes::resolve(producer).unwrap()(&ctx);
        assert!(
            (0.0..=1.0).contains(&result.score),
            "{producer} score {} is a normalized fraction",
            result.score
        );
    }

    // The rubric slice carries a full annotation coat, so information should be high.
    let info = axes::resolve("information_axis").unwrap()(&ctx);
    assert!(
        info.score > 0.8,
        "the rubric slice's annotation coat should score high, got {}",
        info.score
    );
}
