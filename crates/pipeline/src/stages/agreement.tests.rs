// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use std::path::Path;

/// Repo root (the workspace, two levels up from this crate's manifest).
fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

#[test]
fn authenticated_agreement_matrix_carries_the_corpus_summary() {
    // Corpus grading and rendering belong to the producer DAG. This test consumes
    // that exact admitted product and never walks or grades the corpus itself.
    let matrix = crate::fixture::authenticated_artifact(
        &repo_root(),
        "stage-export-agreement",
        AGREEMENT_MATRIX_PATH,
    )
    .expect("load authenticated agreement matrix without rebuilding it");
    let matrix = String::from_utf8(matrix).expect("agreement matrix is UTF-8");
    assert!(matrix.contains("Agreement"), "{matrix}");
    assert!(matrix.contains("**TOTAL**"), "{matrix}");
}

/// A tally JSON with two agreement-expected corpora and one documented-divergence
/// corpus, exercising a perfect corpus, a partial corpus, and a divergence corpus.
fn sample_tallies() -> Vec<u8> {
    // agree rates: tptp-mini 6/6 = 100.0%; w3c-owl2-el 18/19 (1 dl-gap) = 94.7%.
    // headline over both: 24/25 = 96.0%.
    let json = r#"{
          "tptp-mini":      { "lane": "a", "cases": 6,  "agree": 6,  "corpus_only": 0, "dl_gap": 0 },
          "w3c-owl2-el":    { "lane": "a", "cases": 19, "agree": 18, "corpus_only": 0, "dl_gap": 1 },
          "w3c-owl2-el-divergence": { "lane": "divergence", "cases": 2, "agree": 0, "corpus_only": 2, "dl_gap": 0 }
        }"#;
    json.as_bytes().to_vec()
}

fn matrix(tallies: &[u8]) -> String {
    let arts = render_agreement_matrix(tallies).expect("render");
    String::from_utf8(arts.get(AGREEMENT_MATRIX_PATH).expect("matrix").clone()).expect("utf-8")
}

#[test]
fn renders_headline_rate_and_per_corpus_rows() {
    let md = matrix(&sample_tallies());
    // Per-corpus permille rates (integer math, no f64 drift).
    assert!(
        md.contains("| tptp-mini | a | 6 | 6 | 0 | 0 | 100.0% |"),
        "{md}"
    );
    assert!(
        md.contains("| w3c-owl2-el | a | 19 | 18 | 0 | 1 | 94.7% |"),
        "{md}"
    );
    // Headline TOTAL over the agreement-expected lanes only: 24/25 = 96.0%.
    assert!(
        md.contains("| **TOTAL** | — | 25 | 24 | 0 | 1 | **96.0%** |"),
        "{md}"
    );
}

#[test]
fn divergence_corpus_is_segregated_not_counted_as_failure() {
    let md = matrix(&sample_tallies());
    // The divergence corpus is NOT in the headline (else TOTAL cases would be 27).
    assert!(
        !md.contains("27"),
        "divergence cases must not enter the headline: {md}"
    );
    // It appears under the documented-divergences section with its counts.
    let (head, tail) = md
        .split_once("## Documented divergences")
        .expect("divergence section present");
    assert!(
        !head.contains("w3c-owl2-el-divergence"),
        "divergence corpus must not appear in the agreement section"
    );
    // Divergence table carries its own agree column: cases | agree | corpus-only | dl-gap.
    assert!(
        tail.contains("| w3c-owl2-el-divergence | 2 | 0 | 2 | 0 |"),
        "divergence corpus row missing: {tail}"
    );
}

#[test]
fn gap_shape_breakdown_renders_and_aggregates_across_corpora() {
    // Two divergence corpora carrying structured gap shapes; the breakdown section
    // aggregates them by shape, sorted, with integer counts.
    let json = r#"{
          "entailment-mini": { "lane": "a", "cases": 4, "agree": 4, "corpus_only": 0, "dl_gap": 0 },
          "entailment-mini-divergence": { "lane": "divergence", "cases": 2, "agree": 0, "corpus_only": 0, "dl_gap": 2,
            "gap_shapes": { "vendoring-multi-goal": 1, "role-assertion": 1 } },
          "other-divergence": { "lane": "divergence", "cases": 1, "agree": 0, "corpus_only": 0, "dl_gap": 1,
            "gap_shapes": { "role-assertion": 3 } }
        }"#;
    let md = matrix(json.as_bytes());
    assert!(md.contains("## Capability gaps (by shape)"), "{md}");
    // role-assertion aggregates across both divergence corpora: 1 + 3 = 4.
    assert!(md.contains("| role-assertion | 4 |"), "{md}");
    assert!(md.contains("| vendoring-multi-goal | 1 |"), "{md}");
}

#[test]
fn no_gap_shapes_renders_none_in_the_breakdown() {
    let json = r#"{ "tptp-mini": { "lane": "a", "cases": 2, "agree": 2, "corpus_only": 0, "dl_gap": 0 } }"#;
    let md = matrix(json.as_bytes());
    let (_, tail) = md
        .split_once("## Capability gaps (by shape)")
        .expect("gap-shape section present");
    assert!(
        tail.contains("_(none in the committed corpus)_"),
        "empty gap-shape breakdown renders none: {tail}"
    );
}

#[test]
fn no_divergence_corpora_renders_none() {
    let json = r#"{ "tptp-mini": { "lane": "a", "cases": 2, "agree": 2, "corpus_only": 0, "dl_gap": 0 } }"#;
    let md = matrix(json.as_bytes());
    assert!(md.contains("_(none in the committed corpus)_"), "{md}");
}

#[test]
fn zero_case_corpus_hard_fails() {
    let json =
        r#"{ "broken": { "lane": "a", "cases": 0, "agree": 0, "corpus_only": 0, "dl_gap": 0 } }"#;
    let err = render_agreement_matrix(json.as_bytes()).unwrap_err();
    assert!(
        format!("{err:?}").contains("zero cases"),
        "expected a zero-cases hard-fail, got {err:?}"
    );
}

#[test]
fn render_is_deterministic() {
    let t = sample_tallies();
    assert_eq!(
        render_agreement_matrix(&t).unwrap(),
        render_agreement_matrix(&t).unwrap()
    );
}

#[test]
fn agree_rate_rounds_to_nearest_tenth() {
    // Round half up, not floor: 2/3 = 66.66..% → 66.7% (floor would say 66.6%);
    // 37/38 = 97.36..% → 97.4% (floor would say 97.3%).
    assert_eq!(agree_rate(2, 3).unwrap(), "66.7%");
    assert_eq!(agree_rate(37, 38).unwrap(), "97.4%");
    // Exact tenths and a perfect corpus are unchanged.
    assert_eq!(agree_rate(18, 19).unwrap(), "94.7%");
    assert_eq!(agree_rate(6, 6).unwrap(), "100.0%");
}

#[test]
fn agree_rate_never_overclaims_100() {
    // An imperfect corpus that rounds up to 1000 permille is clamped to 99.9% — the
    // benchmark must never render a real gap/disagreement away into a false 100%.
    assert_eq!(agree_rate(9999, 10000).unwrap(), "99.9%");
    assert_eq!(agree_rate(1999, 2000).unwrap(), "99.9%");
    // Only an exactly-perfect corpus earns 100.0%.
    assert_eq!(agree_rate(10000, 10000).unwrap(), "100.0%");
}
