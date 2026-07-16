// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The flagship-counter-example-depth axis over the real grounding slices.
//!
//! The axis measures the fraction of a slice's `gmeow:FlagshipScenario`
//! individuals whose guarding counter-example is reasoner-driven rather than a
//! structural/SHACL proxy. Every flagship counter-example in the repo is structural
//! today, so the axis must score exactly 0.0 on each grounding slice and surface one
//! advisory per structural-only scenario — the opportunity, honestly measured. A
//! slice with no flagship manifest scores vacuously 1.0.

use std::path::{Path, PathBuf};

use gmeow_slice_quality::axes;
use gmeow_slice_quality::score::{ScoreContext, ScoringEnv};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn slice_graph(slice_dir: &Path) -> std::sync::Arc<purrdf::RdfDataset> {
    let paths = gmeow_slice_quality::report::slice_ttl_paths(slice_dir);
    let refs: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
    gmeow_slice_quality::dataset_from_paths(&refs).expect("slice graph parses")
}

fn score(slice: &str, iri: &str) -> gmeow_slice_quality::score::AxisScore {
    let dir = repo_root().join(slice);
    let ds = slice_graph(&dir);
    let ctx = ScoreContext::new(iri.to_owned(), dir, &ds, ScoringEnv::Repo);
    axes::resolve("flagship_counterexample_depth_axis").unwrap()(&ctx)
}

#[test]
fn flagship_counterexample_depth_tracks_each_grounding_slice() {
    // Logic has uplifted all five flagships to native reasoner discharge. Lang and math retain
    // their structural/SHACL proxies until their own slice-local uplift lands.
    for (slice, iri) in [
        (
            "slices/grounding/logic",
            "https://blackcatinformatics.ca/gmeow/slices/logic",
        ),
        (
            "slices/grounding/lang",
            "https://blackcatinformatics.ca/gmeow/slices/lang",
        ),
        (
            "slices/grounding/math",
            "https://blackcatinformatics.ca/gmeow/slices/math",
        ),
    ] {
        let dir = repo_root().join(slice);
        if !dir.join("manifest.ttl").is_file() {
            continue; // slice not present in this checkout — skip.
        }
        let result = score(slice, iri);
        let (expected_score, expected_findings) = if slice.ends_with("/logic") {
            (1.0, 0)
        } else {
            (0.0, 5)
        };
        assert_eq!(
            result.score, expected_score,
            "{slice}: reasoner-depth score"
        );
        assert_eq!(
            result.findings.len(),
            expected_findings,
            "{slice}: structural-only advisory count",
        );
        for f in &result.findings {
            assert_eq!(
                f.code,
                "slice-quality.flagship.counterexample-structural-only"
            );
        }
    }
}

#[test]
fn no_flagship_manifest_scores_vacuously_one() {
    // The rubric slice declares no flagship scenarios → the axis is not applicable
    // and takes the vacuous 1.0 (never silently "deep" — it simply does not apply).
    let result = score(
        "slices/core/slice-quality-rubric",
        "https://blackcatinformatics.ca/gmeow/slices/slice-quality-rubric",
    );
    assert_eq!(result.score, 1.0, "no flagship scenarios → vacuous 1.0");
    assert!(
        result.findings.is_empty(),
        "no advisories when not applicable"
    );
}
