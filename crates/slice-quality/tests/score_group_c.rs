// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Score the real rubric slice with the group-C primitives (linkage, projection,
//! testing, documentation, translation).

use std::path::{Path, PathBuf};

use gmeow_slice_quality::axes;
use gmeow_slice_quality::score::ScoreContext;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn slice_dir() -> PathBuf {
    repo_root().join("slices/core/slice-quality-rubric")
}

fn slice_graph() -> std::sync::Arc<purrdf::RdfDataset> {
    let dir = slice_dir();
    let mut paths: Vec<PathBuf> = vec![dir.join("module.ttl")];
    for sub in ["examples", "tests"] {
        if let Ok(rd) = std::fs::read_dir(dir.join(sub)) {
            for e in rd.flatten() {
                if e.path().extension().is_some_and(|x| x == "ttl") {
                    paths.push(e.path());
                }
            }
        }
    }
    paths.sort();
    let refs: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
    gmeow_slice_quality::dataset_from_paths(&refs).unwrap()
}

fn ctx(ds: &purrdf::RdfDataset) -> ScoreContext<'_> {
    ScoreContext::new(
        "https://blackcatinformatics.ca/gmeow/slices/slice-quality-rubric".to_owned(),
        slice_dir(),
        ds,
    )
}

#[test]
fn all_group_c_producers_yield_normalized_scores() {
    let ds = slice_graph();
    let c = ctx(&ds);
    for producer in [
        "linkage_axis",
        "projection_axis",
        "testing_axis",
        "documentation_axis",
        "translation_axis",
    ] {
        let r = axes::resolve(producer).unwrap()(&c);
        assert!(
            (0.0..=1.0).contains(&r.score),
            "{producer} → {} not in 0..=1",
            r.score
        );
    }
}

#[test]
fn documentation_thesis_is_present() {
    let ds = slice_graph();
    let doc = axes::resolve("documentation_axis").unwrap()(&ctx(&ds));
    assert_eq!(
        doc.score, 1.0,
        "the rubric slice ships a narrative docs.md thesis"
    );
}

#[test]
fn translation_reflects_the_missing_mandarin_catalog_honestly() {
    // The slice ships fr.po but not (yet) zh.po, so translation must NOT be a
    // perfect 1.0 — the axis reports the real gap rather than smoothing it over.
    let ds = slice_graph();
    let tr = axes::resolve("translation_axis").unwrap()(&ctx(&ds));
    assert!(
        tr.score < 1.0,
        "translation must reflect the missing Mandarin catalog, got {}",
        tr.score
    );
    assert!(
        tr.score > 0.3,
        "English + French are present, so it is not near zero"
    );
    assert!(
        tr.findings.iter().any(|f| f.message.contains("zh")),
        "an advisory names the missing Mandarin coverage"
    );
}
