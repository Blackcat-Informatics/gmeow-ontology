// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! End-to-end: score a real slice through the full rubric and render the report.

use std::path::{Path, PathBuf};

use gmeow_slice_quality::ScoringEnv;
use gmeow_slice_quality::report::{SliceReport, score_slice_with_standard};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// Score a slice against the repo rubric's measurement standard, in repo mode —
/// the in-repo replacement for the retired `score_slice(root, dir)`.
fn score(root: &Path, dir: &Path) -> gmeow_errors::Result<SliceReport> {
    let module = root.join("slices/core/slice-quality-rubric/module.ttl");
    let ds = gmeow_slice_quality::dataset_from_paths(&[&module])?;
    let standard = gmeow_slice_quality::rubric::load_rubric(&ds)?.standard;
    score_slice_with_standard(dir, &standard, ScoringEnv::Repo)
}

#[test]
fn scores_the_rubric_slice_across_all_sixteen_axes() {
    let root = repo_root();
    let dir = root.join("slices/core/slice-quality-rubric");
    let report = score(&root, &dir).expect("the rubric slice scores");

    assert_eq!(
        report.assessment.grades.len(),
        16,
        "all sixteen axes graded"
    );
    assert!(
        !report.rollup_label().is_empty(),
        "a roll-up tier is assigned"
    );

    // Deterministic text render is non-empty and names the roll-up.
    let text = report.render_text();
    assert!(text.contains("roll-up tier:"));
    assert!(text.contains("per-axis grades"));

    // The advisory Report is on the diagnostics substrate and never gates.
    let diag = report.to_report();
    assert!(
        diag.ok(),
        "a slice-quality report is advisory — it never gates"
    );
}

#[test]
fn scoring_is_deterministic() {
    // Same slice + same rubric → byte-identical report. Determinism is a hard
    // requirement: the tool is a gate input and a golden source.
    let root = repo_root();
    let dir = root.join("slices/core/slice-quality-rubric");
    let a = score(&root, &dir).unwrap();
    let b = score(&root, &dir).unwrap();
    assert_eq!(
        a.render_text(),
        b.render_text(),
        "text render is deterministic"
    );
    assert_eq!(
        gmeow_errors::render::to_json(&a.to_report()).unwrap(),
        gmeow_errors::render::to_json(&b.to_report()).unwrap(),
        "JSON render is deterministic"
    );
}

#[test]
fn emitted_codes_are_registered_and_carry_help_uris() {
    // Every finding a slice-quality report emits must carry a REGISTERED diagnostic
    // code (never a bare string) and a help URI pointing into the generated
    // constraint catalog — the AC1/H1 requirement. Scoring a real slice through
    // `to_report` seeds the codes and attaches the rule descriptors.
    let root = repo_root();
    let dir = root.join("slices/core/slice-quality-rubric");
    let report = score(&root, &dir).expect("the rubric slice scores");
    let diag = report.to_report();
    assert!(!diag.findings.is_empty(), "the report emits findings");

    // The full code enumeration is registered in the process-wide registry.
    for code in gmeow_slice_quality::report::FINDING_CODES {
        assert!(
            gmeow_errors::intern_code(code).is_ok(),
            "slice-quality code `{code}` must be registered after to_report()"
        );
    }

    // Every distinct emitted finding code resolves to a rule carrying a help URI
    // anchored in the generated constraint catalog.
    let base = "https://blackcatinformatics.ca/gmeow/docs/enforced-constraints#";
    let mut codes: Vec<&str> = diag.findings.iter().map(|f| f.code.as_str()).collect();
    codes.sort_unstable();
    codes.dedup();
    for code in codes {
        assert!(
            gmeow_errors::intern_code(code).is_ok(),
            "emitted finding code `{code}` is registered"
        );
        let rule = diag
            .rules
            .iter()
            .find(|r| r.id == code)
            .unwrap_or_else(|| panic!("finding code `{code}` has a rule descriptor"));
        let uri = rule
            .help_uri
            .as_deref()
            .unwrap_or_else(|| panic!("rule `{code}` carries a help URI"));
        assert!(
            uri.starts_with(base),
            "help URI `{uri}` for `{code}` points into the generated catalog"
        );
        let slug = gmeow_validate::rule_catalog::help_uri_for(code);
        assert_eq!(
            uri, slug,
            "help URI uses the canonical `help_uri_for` anchor"
        );
    }
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
    let report = score(&root, &dir).expect("the logic slice scores");
    assert_eq!(
        report.assessment.grades.len(),
        16,
        "all sixteen axes graded on logic"
    );
    assert!(report.to_report().ok(), "advisory only");
}
