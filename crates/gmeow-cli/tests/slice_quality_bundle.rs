// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! End-to-end proof that the `gmeow` wheel scores a FOREIGN slice against the
//! embedded `gmeow.gts` bundle — with no repo checkout, no generator inputs, and
//! no network — via `gmeow_slice_quality::score_external_slice`.
//!
//! The fixture under `tests/fixtures/external-slice/` is a well-formed slice that
//! scores STRICTLY between 0 and 1 on the three environment-anchored axes:
//!   * `gmn1_coverage` — one module quad references an IRI under an unregistered
//!     namespace (uncovered), the rest are codec-covered;
//!   * `DocMaturity` — the slice ships no realized-state design-set table, so a
//!     FULL-anchor dimension is a gated miss;
//!   * `translation` — only the widget term's carriers are translated into fr, and
//!     cmn ships no catalog at all.
//!
//! Each test copies the fixture into a FRESH temp dir so the scored path has no
//! `slices/` ancestor and no relationship to this repo — exactly the consumer's
//! situation.

use std::fs;
use std::path::{Path, PathBuf};

use gmeow_slice_quality::report::SliceReport;
use gmeow_slice_quality::{BundleStandards, MeasurementStandard, ScoringEnv, score_external_slice};

/// The four environment-anchored axis IRIs, keyed by the rubric individual names.
const AXIS_GMN1: &str = "https://blackcatinformatics.ca/gmeow/axisGmn1Coverage";
const AXIS_GMN_GLYPH: &str = "https://blackcatinformatics.ca/gmeow/axisGmnGlyphOptimality";
const AXIS_DOC_MATURITY: &str = "https://blackcatinformatics.ca/gmeow/axisDocMaturity";
const AXIS_TRANSLATION: &str = "https://blackcatinformatics.ca/gmeow/axisTranslationCoverage";
/// The advice-harvest-coverage axis: like `gmn1`/`DocMaturity`, its Repo-mode
/// producer ([`gmeow_slice_quality::axes`]'s `advice_coverage_axis`) resolves the
/// advisory-constraint authority off `ctx.slice_dir`'s `slices/` ancestor
/// (`repo_root_of`), so a slice staged with NO such ancestor (this fixture's
/// deliberately repo-free staging) goes vacuous 1.0 in Repo mode — while Bundle
/// mode measures the fixture's OWN graph self-containedly and finds neither
/// advisory-prose term backed by a realized carrier, scoring the real 0.0. It is
/// therefore environment-DIVERGENT by construction, not a byte-equality bug.
const AXIS_ADVICE_COVERAGE: &str = "https://blackcatinformatics.ca/gmeow/axisAdviceCoverage";

/// The fixture slice's stable IRI (matches `manifest.ttl`).
const FIXTURE_SLICE_IRI: &str =
    "https://blackcatinformatics.ca/gmeow/slices/fixture-external-slice";

/// The authored fixture root under this crate's `tests/` tree.
fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/external-slice")
}

/// Recursively copy `src` into `dst` (creating `dst`). Deterministic, files + dirs.
fn copy_tree(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("create dest dir");
    for entry in fs::read_dir(src).expect("read source dir") {
        let entry = entry.expect("dir entry");
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&from, &to);
        } else {
            fs::copy(&from, &to).expect("copy file");
        }
    }
}

/// Copy the fixture into a fresh temp dir with NO `slices/` ancestor and return the
/// (owned tempdir, scored slice path). The tempdir must be kept alive by the caller.
fn staged_fixture() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let slice_dir = tmp.path().join("external-slice");
    copy_tree(&fixture_root(), &slice_dir);
    // Sanity: the scored path carries no `slices/` component (the consumer's case).
    assert!(
        !slice_dir.components().any(|c| c.as_os_str() == "slices"),
        "the staged slice path must have no slices/ ancestor: {}",
        slice_dir.display()
    );
    (tmp, slice_dir)
}

/// The grade for `axis_iri` in `report`, or panic (a rubric axis must always grade).
fn grade_for<'a>(report: &'a SliceReport, axis_iri: &str) -> &'a gmeow_slice_quality::AxisGrade {
    report
        .assessment
        .grades
        .iter()
        .find(|g| g.axis_iri == axis_iri)
        .unwrap_or_else(|| panic!("no grade for axis {axis_iri}"))
}

/// The advisory codes the report surfaces (per-axis + template items).
fn advisory_codes(report: &SliceReport) -> Vec<String> {
    report.advisories.iter().map(|f| f.code.clone()).collect()
}

/// The exact set of `gmeow:dim*` local names named by the report's
/// `doc-maturity.missing-dimension` advisories (each message embeds exactly one
/// `gmeow:dim*` token — the structured [`gmeow_slice_quality::score::Finding`] the
/// axis producer emits, not the rendered report text).
fn missing_doc_maturity_dimensions(report: &SliceReport) -> std::collections::BTreeSet<String> {
    report
        .advisories
        .iter()
        .filter(|f| f.code == "slice-quality.doc-maturity.missing-dimension")
        .map(|f| {
            f.message
                .split_whitespace()
                .find_map(|tok| tok.strip_prefix("gmeow:"))
                .unwrap_or_else(|| {
                    panic!(
                        "missing-dimension advisory names a gmeow:dim* token: {}",
                        f.message
                    )
                })
                .to_owned()
        })
        .collect()
}

// ── AC1: the wheel's zero-config external-slice scoring path ────────────────────

#[test]
fn ac1_scores_external_slice_against_embedded_bundle() {
    let (_tmp, slice_dir) = staged_fixture();
    let std = BundleStandards::from_gts(gmeow_cli::BUNDLE_GTS).expect("load bundle standards");
    let report = score_external_slice(&std, &slice_dir).expect("score external slice");

    // (a) A grade for EVERY axis the bundle-loaded rubric defines.
    assert!(
        !report.standard.axes.is_empty(),
        "the bundle rubric defines at least one axis"
    );
    assert_eq!(
        report.assessment.grades.len(),
        report.standard.axes.len(),
        "one grade per rubric axis"
    );
    for axis in &report.standard.axes {
        assert!(
            report
                .assessment
                .grades
                .iter()
                .any(|g| g.axis_iri == axis.iri),
            "axis {} carries a grade",
            axis.iri
        );
    }

    let codes = advisory_codes(&report);

    // (b) gmn1-coverage: no tolerant no-repo-root / no-dictionary advisory (Bundle
    // mode always has a valid embedded dictionary), and the score is measured < 1.0.
    assert!(
        !codes
            .iter()
            .any(|c| c == "slice-quality.gmn1-coverage.no-repo-root"),
        "Bundle mode must not emit the no-repo-root advisory: {codes:?}"
    );
    assert!(
        !codes
            .iter()
            .any(|c| c == "slice-quality.gmn1-coverage.no-dictionary"),
        "Bundle mode must not emit the no-dictionary advisory: {codes:?}"
    );
    let gmn1 = grade_for(&report, AXIS_GMN1);
    assert!(
        gmn1.score < 1.0 && gmn1.score > 0.0,
        "gmn1-coverage is measured strictly in (0,1): {}",
        gmn1.score
    );
    assert!(
        codes
            .iter()
            .any(|c| c == "slice-quality.gmn1-coverage.uncovered"),
        "the uncovered GMN-0 quad surfaces an uplift advisory: {codes:?}"
    );

    // (c) DocMaturity: no model-unavailable advisory, measured < 1.0, and at least
    // one SPECIFIC missing-dimension advisory (the omitted realized-state table).
    assert!(
        !codes
            .iter()
            .any(|c| c == "slice-quality.doc-maturity.model-unavailable"),
        "the single-slice documentation model must build off-repo: {codes:?}"
    );
    let doc = grade_for(&report, AXIS_DOC_MATURITY);
    assert!(
        doc.score < 1.0 && doc.score > 0.0,
        "DocMaturity is measured strictly in (0,1): {}",
        doc.score
    );
    // Pin the EXACT off-repo DocMaturity oracle for this fixture: the axis score is
    // the slice's bounded FULL-anchor coverage fraction (9 of the 12 FULL dimensions
    // covered), consumed verbatim from `crates/docs` (no partial-credit smoothing —
    // exact equality is the right idiom for a ratio the producer computes as
    // `covered.len() as f64 / full_intent.len() as f64` = 9.0/12.0, exactly
    // representable in f64). A drift here means a cross-slice dependency silently
    // changed which FULL dimensions this off-repo fixture earns.
    assert_eq!(
        doc.score, 0.75,
        "DocMaturity's exact fraction for the fixture is pinned at 9/12 FULL dimensions"
    );
    let missing_dims = missing_doc_maturity_dimensions(&report);
    let expected_missing: std::collections::BTreeSet<String> =
        ["dimExample", "dimRealizedState", "dimScopeNote"]
            .into_iter()
            .map(str::to_owned)
            .collect();
    assert_eq!(
        missing_dims, expected_missing,
        "the EXACT set of missing FULL-anchor dimensions is pinned: {codes:?}"
    );
    // The complementary covered set (FULL intent minus the pinned misses) — no
    // covered dimension is ever named as missing.
    let expected_covered = [
        "dimDefinition",
        "dimLabel",
        "dimUsageAdvice",
        "dimAlignment",
        "dimFixturePair",
        "dimCompetencyRationale",
        "dimWorkedInstance",
        "dimLossLedgerRow",
        "dimLinkageCoverage",
    ];
    for dim in expected_covered {
        assert!(
            !missing_dims.contains(dim),
            "{dim} is covered by the fixture and must never be named missing: {missing_dims:?}"
        );
    }
    assert_eq!(
        expected_covered.len() + expected_missing.len(),
        12,
        "covered + missing accounts for exactly the 12-dimension FULL anchor intent"
    );

    // (d) translation: partial fr + absent cmn ⇒ measured < 1.0.
    let trans = grade_for(&report, AXIS_TRANSLATION);
    assert!(
        trans.score < 1.0 && trans.score > 0.0,
        "translation is measured strictly in (0,1): {}",
        trans.score
    );

    // (e) the roll-up is a real ladder rung (a loaded tier, non-vacuous meet).
    let rollup = &report.assessment.rollup;
    assert!(
        !rollup.label.is_empty(),
        "the roll-up tier carries a real label"
    );
    assert!(
        report.standard.tiers.iter().any(|t| t == rollup),
        "the roll-up tier is a genuine rung of the bundle-loaded ladder: {rollup:?}"
    );
    let meet_rank = report
        .assessment
        .grades
        .iter()
        .map(|g| g.tier.rank)
        .min()
        .expect("at least one grade");
    assert_eq!(
        rollup.rank, meet_rank,
        "the roll-up is the unweighted meet (least rank) of the axis grades"
    );

    // (f) the RDF projection yields non-empty gmeow:QualityAssessment N-Quads.
    let nquads = report.to_gmeow_rdf();
    assert!(!nquads.trim().is_empty(), "the RDF projection is non-empty");
    let assessment_count = nquads
        .matches("<https://blackcatinformatics.ca/gmeow/QualityAssessment>")
        .count();
    assert!(
        assessment_count >= report.standard.axes.len(),
        "one gmeow:QualityAssessment per axis grade (+ roll-up): saw {assessment_count}"
    );
    assert!(
        nquads.contains(FIXTURE_SLICE_IRI),
        "the assessed entity is the fixture slice IRI"
    );

    // (g) at least one advisory carries a resolved help URI (attached per code on
    // the diagnostics report's rule registry via help_uri_for).
    let diag = report.to_report();
    let advisory_code_set: std::collections::BTreeSet<&str> =
        codes.iter().map(String::as_str).collect();
    let resolved_help = diag.rules.iter().any(|r| {
        advisory_code_set.contains(r.id.as_str())
            && r.help_uri.as_deref().is_some_and(|u| !u.is_empty())
    });
    assert!(
        resolved_help,
        "≥1 advisory code exposes a resolved, non-empty help_uri via its rule"
    );
}

// ── AC5: non-interference oracle — the 12 env-agnostic axes are byte-identical ──

#[test]
fn ac5_env_agnostic_axes_are_byte_equal_across_repo_and_bundle() {
    let (_tmp, slice_dir) = staged_fixture();

    // Bundle run (the wheel path).
    let std = BundleStandards::from_gts(gmeow_cli::BUNDLE_GTS).expect("load bundle standards");
    let bundle = score_external_slice(&std, &slice_dir).expect("bundle score");

    // Repo run against the SAME standard (loaded via the public rubric API), scored
    // in Repo mode. Off-repo, gmn1 + advice-coverage legitimately go vacuous (1.0),
    // while GMN glyph optimality and DocMaturity fail CLOSED (0.0) because neither's
    // authority — the canonical lang audit graph, the repo documentation model — can be
    // assembled. Every other axis must be identical to the Bundle run.
    let ds = purrdf::gts::flattened_dataset_from_bytes(gmeow_cli::BUNDLE_GTS)
        .expect("flatten bundle dataset");
    let std_meas: MeasurementStandard = gmeow_slice_quality::rubric::load_rubric(&ds)
        .expect("load rubric standard")
        .standard;
    let repo = gmeow_slice_quality::report::score_slice_with_standard(
        &slice_dir,
        &std_meas,
        ScoringEnv::Repo,
    )
    .expect("repo score");

    // The four env-DIVERGENT axes: two are vacuous off-repo; glyph optimality and
    // DocMaturity are deliberately non-vacuous because losing their measuring authority
    // must be VISIBLE. An axis that could not be measured is a defect to fix, never a
    // grade to bank — scoring it 1.0 would launder "nothing was known" into the maximum.
    assert_eq!(
        grade_for(&repo, AXIS_GMN1).score,
        1.0,
        "gmn1 goes vacuous 1.0 in Repo mode off-repo"
    );
    assert_eq!(
        grade_for(&repo, AXIS_DOC_MATURITY).score,
        0.0,
        "DocMaturity fails closed at 0.0 in Repo mode off-repo: the repo documentation model \
         cannot be built there, so its coverage is UNMEASURED — never vacuously maximal"
    );
    assert!(
        advisory_codes(&repo)
            .iter()
            .any(|code| code == "slice-quality.doc-maturity.model-unavailable"),
        "the failed-closed DocMaturity score names the model it could not build"
    );
    assert_eq!(
        grade_for(&repo, AXIS_ADVICE_COVERAGE).score,
        1.0,
        "advice-coverage goes vacuous 1.0 in Repo mode off-repo (no slices/ ancestor to \
         resolve the central logic:Constraint / logic:AdviceGuidance authority from)"
    );
    assert_eq!(
        grade_for(&bundle, AXIS_ADVICE_COVERAGE).score,
        0.0,
        "Bundle mode measures the fixture's own graph self-containedly: its avoidWhen/useWhen \
         terms author no realized advisory carrier of their own, so the real score is 0.0"
    );
    assert_eq!(
        grade_for(&repo, AXIS_GMN_GLYPH).score,
        0.0,
        "glyph optimality fails closed when Repo mode has no lang audit authority"
    );
    assert!(
        advisory_codes(&repo)
            .iter()
            .any(|code| code == "slice-quality.gmn-glyph-optimality.audit-graph-unavailable"),
        "the failed-closed glyph score names the missing audit graph"
    );

    // Every OTHER axis grade is byte-for-byte equal between the two runs.
    let mut compared = 0usize;
    for bg in &bundle.assessment.grades {
        if bg.axis_iri == AXIS_GMN1
            || bg.axis_iri == AXIS_GMN_GLYPH
            || bg.axis_iri == AXIS_DOC_MATURITY
            || bg.axis_iri == AXIS_ADVICE_COVERAGE
        {
            continue;
        }
        let rg = grade_for(&repo, &bg.axis_iri);
        assert_eq!(
            bg.score.to_bits(),
            rg.score.to_bits(),
            "axis {} score is env-agnostic (byte-equal)",
            bg.axis_iri
        );
        assert_eq!(
            bg.tier, rg.tier,
            "axis {} tier is env-agnostic",
            bg.axis_iri
        );
        compared += 1;
    }
    assert_eq!(
        compared, 12,
        "exactly the 12 env-agnostic axes are compared (16 total minus four environment-dependent axes)"
    );
}

// ── AC6: hard-fail on junk — never a vacuous passing grade ──────────────────────

#[test]
fn ac6_corrupt_bundle_and_malformed_manifest_hard_fail() {
    // A corrupt wheel cannot be flattened → hard fail (never a degraded standard).
    let corrupt: &[u8] = b"this is not a gmeow.gts bundle at all \x00\x01\x02";
    assert!(
        BundleStandards::from_gts(corrupt).is_err(),
        "a corrupt bundle must hard-fail, never load a vacuous standard"
    );

    // A slice dir whose manifest.ttl is malformed → hard fail (never a passing grade).
    let std = BundleStandards::from_gts(gmeow_cli::BUNDLE_GTS).expect("load bundle standards");
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let junk_slice = tmp.path().join("junk-slice");
    fs::create_dir_all(&junk_slice).expect("create junk slice dir");
    fs::write(
        junk_slice.join("manifest.ttl"),
        b"@@@ this is not valid turtle and declares no gmeow:Slice @@@",
    )
    .expect("write malformed manifest");
    assert!(
        score_external_slice(&std, &junk_slice).is_err(),
        "a malformed manifest must hard-fail, never return a vacuous passing grade"
    );
}
