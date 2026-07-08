// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! End-to-end: score a real slice through the full rubric and render the report.

use std::path::PathBuf;

use gmeow_slice_quality::report::score_slice;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

#[test]
fn scores_the_rubric_slice_across_all_ten_axes() {
    let root = repo_root();
    let dir = root.join("slices/core/slice-quality-rubric");
    let report = score_slice(&root, &dir).expect("the rubric slice scores");

    assert_eq!(report.assessment.grades.len(), 10, "all ten axes graded");
    assert!(
        !report.rollup_label().is_empty(),
        "a roll-up tier is assigned"
    );

    // Deterministic text render is non-empty and names the roll-up.
    let text = report.render_text();
    assert!(text.contains("roll-up tier:"));
    assert!(text.contains("per-axis grades"));
    eprintln!("\n{text}");

    // The advisory Report is on the diagnostics substrate and never gates.
    let diag = report.to_report();
    assert!(
        diag.ok(),
        "a slice-quality report is advisory — it never gates"
    );
}

#[test]
fn scores_the_logic_slice_green_vs_advisory() {
    // The SLICE_QA baseline: the logic slice scores without error and produces a
    // per-axis grade vector; findings are advisory (the report never gates).
    let root = repo_root();
    let dir = root.join("slices/grounding/logic");
    if !dir.join("manifest.ttl").is_file() {
        return; // logic slice not present in this checkout — skip.
    }
    let report = score_slice(&root, &dir).expect("the logic slice scores");
    assert_eq!(
        report.assessment.grades.len(),
        10,
        "all ten axes graded on logic"
    );
    assert!(report.to_report().ok(), "advisory only");
    eprintln!("\nlogic roll-up: {}", report.rollup_label());
}
