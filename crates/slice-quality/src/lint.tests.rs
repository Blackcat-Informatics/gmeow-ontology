// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::model::{Axis, AxisGrade, ContextScope, SliceAssessment, Threshold};
use std::collections::HashMap;

fn tier(local: &str, rank: i64) -> Tier {
    Tier {
        iri: format!("https://blackcatinformatics.ca/gmeow/tier{local}"),
        label: local.to_owned(),
        rank,
    }
}

fn ladder() -> Vec<Tier> {
    vec![
        tier("Registered", 0),
        tier("Grounded", 1),
        tier("Linked", 2),
        tier("Exemplified", 3),
        tier("Maximal", 4),
    ]
}

fn standard(axes: Vec<Axis>) -> MeasurementStandard {
    MeasurementStandard {
        tiers: ladder(),
        axes,
    }
}

/// The axis IRI an `axis(local)`/`grade(local, ..)` pair share — the exact
/// stored-back-reference value a real `advisory_axes` entry would carry for
/// that axis, so tests can pin genuine axis provenance instead of relying on
/// the finding code's text to line up with an axis's local name.
fn axis_iri(local: &str) -> String {
    format!("https://blackcatinformatics.ca/gmeow/axis{local}")
}

fn axis(local: &str) -> Axis {
    Axis {
        iri: format!("https://blackcatinformatics.ca/gmeow/axis{local}"),
        label: local.to_owned(),
        producer: "test".to_owned(),
        dimension_iri: "ex:d".to_owned(),
        thresholds: vec![Threshold {
            tier_iri: "https://blackcatinformatics.ca/gmeow/tierGrounded".to_owned(),
            floor: 0.5,
        }],
        weight: 1.0,
        scope: ContextScope::SliceLocal,
        advice: String::new(),
    }
}

fn grade(axis_local: &str, tier: Tier, score: f64) -> AxisGrade {
    AxisGrade {
        axis_iri: format!("https://blackcatinformatics.ca/gmeow/axis{axis_local}"),
        score,
        tier,
    }
}

fn assessment(grades: Vec<AxisGrade>, rollup: Tier) -> SliceAssessment {
    SliceAssessment {
        slice: "ex:slice".to_owned(),
        grades,
        rollup,
    }
}

/// `advisory_axes` must be index-parallel to `advisories` (or empty, for a
/// test that does not care about axis attribution at all — see
/// [`SliceReport::for_test`]).
fn report_with(
    axes: Vec<Axis>,
    grades: Vec<AxisGrade>,
    rollup: Tier,
    advisories: Vec<Finding>,
    advisory_axes: Vec<String>,
) -> SliceReport {
    SliceReport::for_test(
        standard(axes),
        assessment(grades, rollup),
        advisories,
        advisory_axes,
        HashMap::new(),
    )
}

#[test]
fn no_declared_no_required_always_passes() {
    // (a) no declared claim + no --min-tier bar → pure advisory view: passed,
    // and no below-min-tier finding is minted at all.
    let report = report_with(
        vec![axis("Grounding")],
        vec![grade("Grounding", tier("Linked", 2), 0.9)],
        tier("Linked", 2),
        vec![],
        vec![],
    );
    let outcome = lint_report(&report, None, None);
    assert!(outcome.passed);
    assert!(outcome.effective_bar.is_none());
    assert!(
        !outcome
            .findings
            .findings
            .iter()
            .any(|f| f.code == "slice-quality.lint.below-min-tier"),
        "no bar ⇒ no below-min-tier finding: {:#?}",
        outcome.findings.findings
    );
}

#[test]
fn declared_equal_to_measured_passes() {
    // (b) declared == measured roll-up → holding exactly at the bar passes.
    let report = report_with(
        vec![axis("Grounding")],
        vec![grade("Grounding", tier("Linked", 2), 0.9)],
        tier("Linked", 2),
        vec![],
        vec![],
    );
    let declared = tier("Linked", 2);
    let outcome = lint_report(&report, Some(&declared), None);
    assert!(outcome.passed);
    assert_eq!(outcome.effective_bar, Some(tier("Linked", 2)));
}

#[test]
fn declared_above_measured_fails_with_below_bar_finding() {
    // (c) declared (Exemplified/3) above measured (Grounded/1) → fails, and a
    // slice-quality.lint.below-min-tier Error finding is present.
    let report = report_with(
        vec![axis("Grounding")],
        vec![grade("Grounding", tier("Grounded", 1), 0.6)],
        tier("Grounded", 1),
        vec![],
        vec![],
    );
    let declared = tier("Exemplified", 3);
    let outcome = lint_report(&report, Some(&declared), None);
    assert!(!outcome.passed);
    let below = outcome
        .findings
        .findings
        .iter()
        .find(|f| f.code == "slice-quality.lint.below-min-tier")
        .expect("below-min-tier finding present on failure");
    assert_eq!(below.severity, Severity::Error);
    assert!(below.message.contains("Grounded") && below.message.contains("Exemplified"));
}

#[test]
fn required_min_tier_above_measured_fails() {
    // (d) an explicit --min-tier above measured fails too, with no declared
    // claim at all.
    let report = report_with(
        vec![axis("Grounding")],
        vec![grade("Grounding", tier("Grounded", 1), 0.6)],
        tier("Grounded", 1),
        vec![],
        vec![],
    );
    let required = tier("Maximal", 4);
    let outcome = lint_report(&report, None, Some(&required));
    assert!(!outcome.passed);
    assert_eq!(outcome.effective_bar, Some(tier("Maximal", 4)));
}

#[test]
fn effective_bar_is_the_higher_rank_of_declared_and_required() {
    // (e) effective_bar == max(declared, required), in either order.
    let report = report_with(
        vec![axis("Grounding")],
        vec![grade("Grounding", tier("Linked", 2), 0.9)],
        tier("Linked", 2),
        vec![],
        vec![],
    );
    let low = tier("Grounded", 1);
    let high = tier("Linked", 2);
    let a = lint_report(&report, Some(&low), Some(&high));
    assert_eq!(a.effective_bar, Some(tier("Linked", 2)));
    let b = lint_report(&report, Some(&high), Some(&low));
    assert_eq!(b.effective_bar, Some(tier("Linked", 2)));
}

#[test]
fn advisory_severity_graded_by_axis_and_bar_missing_template_stays_info() {
    // (f) An advisory attributable to a below-bar axis is stamped Error; one
    // attributable to an at/above-bar axis stays Warning; the rubric-gap
    // "missing-template" code stays Info regardless of the bar.
    let report = report_with(
        vec![axis("Grounding"), axis("Prose")],
        vec![
            grade("Grounding", tier("Grounded", 1), 0.6), // below the Linked bar
            grade("Prose", tier("Linked", 2), 0.9),       // at the Linked bar
        ],
        tier("Grounded", 1), // roll-up meet is the weaker axis
        vec![
            Finding::new(
                Severity::Warning,
                "slice-quality.grounding.no-stereotype",
                "on the below-bar axis",
            )
            .with_tool("slice-quality"),
            Finding::new(
                Severity::Warning,
                "slice-quality.prose.test-rationale",
                "on the at-bar axis",
            )
            .with_tool("slice-quality"),
            Finding::new(
                Severity::Warning,
                "slice-quality.axis-advice.missing-template",
                "Grounding: axis is deficient but carries no advice template",
            )
            .with_tool("slice-quality"),
        ],
        vec![
            axis_iri("Grounding"),
            axis_iri("Prose"),
            axis_iri("Grounding"),
        ],
    );
    let bar = tier("Linked", 2);
    let outcome = lint_report(&report, Some(&bar), None);
    assert!(!outcome.passed);

    let grounding = outcome
        .findings
        .findings
        .iter()
        .find(|f| f.code == "slice-quality.grounding.no-stereotype")
        .expect("grounding advisory present");
    assert_eq!(
        grounding.severity,
        Severity::Error,
        "below-bar axis advisory escalates to Error"
    );

    let prose = outcome
        .findings
        .findings
        .iter()
        .find(|f| f.code == "slice-quality.prose.test-rationale")
        .expect("prose advisory present");
    assert_eq!(
        prose.severity,
        Severity::Warning,
        "at/above-bar axis advisory stays Warning"
    );

    let missing_template = outcome
        .findings
        .findings
        .iter()
        .find(|f| f.code == "slice-quality.axis-advice.missing-template")
        .expect("missing-template advisory present");
    assert_eq!(
        missing_template.severity,
        Severity::Info,
        "rubric-provenance gap never escalates, even below the bar"
    );
}

#[test]
fn stored_axis_provenance_attributes_even_when_the_code_textually_mismatches_the_axis() {
    // The stored `advisory_axes` back-reference (report.rs) exists precisely to
    // fix the case the removed `attribute_axis` textual join could not: a
    // finding whose CODE's domain token ("testing") shares no substring with
    // its producing axis's local name ("Grounding") — normalize("axisGrounding")
    // == "axisgrounding" does not contain "testing", so the old best-effort join
    // would have returned `None` and this advisory would have stayed the safe
    // (never-escalating) `Warning` default even though it sits on a below-bar
    // axis. The stored back-reference attributes it exactly, regardless of code
    // spelling, so it correctly escalates to `Error`.
    let report = report_with(
        vec![axis("Grounding")],
        vec![grade("Grounding", tier("Grounded", 1), 0.6)], // below the Linked bar
        tier("Grounded", 1),
        vec![
            Finding::new(
                Severity::Warning,
                "slice-quality.testing.untested-term",
                "produced by the Grounding axis despite an unrelated code domain token",
            )
            .with_tool("slice-quality"),
        ],
        vec![axis_iri("Grounding")],
    );
    let bar = tier("Linked", 2);
    let outcome = lint_report(&report, Some(&bar), None);
    let finding = outcome
        .findings
        .findings
        .iter()
        .find(|f| f.code == "slice-quality.testing.untested-term")
        .expect("finding present");
    assert_eq!(
        finding.severity,
        Severity::Error,
        "stored axis provenance escalates a below-bar advisory even though its \
             code's domain token matches no axis textually: {finding:#?}"
    );
}

#[test]
fn resolve_min_tier_unknown_names_the_rungs() {
    // (g) An unknown --min-tier is a hard fail naming every known rung.
    let std = standard(vec![]);
    let err = resolve_min_tier(&std, "bogus").expect_err("unknown tier must error");
    let message = err.to_string();
    for label in ["Registered", "Grounded", "Linked", "Exemplified", "Maximal"] {
        assert!(
            message.contains(label),
            "error names every rung, missing {label}: {message}"
        );
    }
}

#[test]
fn resolve_min_tier_accepts_label_or_local_name_case_insensitively() {
    let std = standard(vec![]);
    let by_label = resolve_min_tier(&std, "grounded").expect("label matches");
    assert_eq!(by_label.rank, 1);
    let by_local = resolve_min_tier(&std, "tierLINKED").expect("local name matches");
    assert_eq!(by_local.rank, 2);
}

#[test]
/// SARIF receives real manifest/module files while preserving file-level semantics.
fn lint_sarif_uses_real_file_level_slice_sources_without_fake_spans() {
    let mut term_finding = Finding::new(
        Severity::Warning,
        "slice-quality.grounding.no-stereotype",
        "term-owned finding",
    )
    .with_tool("slice-quality");
    term_finding
        .documented_terms
        .push("https://example.test/Term".to_owned());
    let slice_finding = Finding::new(
        Severity::Warning,
        "slice-quality.documentation.no-docs",
        "slice-owned finding",
    )
    .with_tool("slice-quality");
    let report = report_with(
        vec![axis("Grounding")],
        vec![grade("Grounding", tier("Grounded", 1), 0.6)],
        tier("Grounded", 1),
        vec![term_finding, slice_finding],
        vec![axis_iri("Grounding"), axis_iri("Grounding")],
    );

    let outcome = lint_report(&report, None, None);
    let paths: Vec<&str> = outcome
        .findings
        .findings
        .iter()
        .map(|finding| {
            let location = finding.locations.first().expect("physical location");
            assert!(location.line.is_none() && location.column.is_none());
            location.path.as_deref().expect("source path")
        })
        .collect();
    assert_eq!(paths, ["module.ttl", "manifest.ttl"]);

    let sarif = gmeow_errors::render::to_sarif(&outcome.findings).expect("lint SARIF renders");
    assert!(!sarif.contains("ontology/gmeow.ttl"));
    assert!(!sarif.contains("gmeow.syntheticPhysicalLocation"));
    assert!(!sarif.contains("\"region\""));
}

#[test]
/// A missing module falls back to the slice manifest, never the shared ontology.
fn term_finding_without_module_uses_manifest_not_shared_ontology() {
    let mut finding = Finding::new(
        Severity::Warning,
        "slice-quality.grounding.no-stereotype",
        "term-owned finding without a module",
    )
    .with_tool("slice-quality");
    finding
        .documented_terms
        .push("https://example.test/Term".to_owned());
    let mut report = report_with(
        vec![axis("Grounding")],
        vec![grade("Grounding", tier("Grounded", 1), 0.6)],
        tier("Grounded", 1),
        vec![finding],
        vec![axis_iri("Grounding")],
    );
    report.remove_module_source_for_test();

    let outcome = lint_report(&report, None, None);
    let location = outcome.findings.findings[0]
        .locations
        .first()
        .expect("file-level source");
    assert_eq!(location.path.as_deref(), Some("manifest.ttl"));
    assert!(location.line.is_none() && location.column.is_none());

    let sarif = gmeow_errors::render::to_sarif(&outcome.findings).expect("lint SARIF renders");
    assert!(!sarif.contains("ontology/gmeow.ttl"));
    assert!(!sarif.contains("\"region\""));
}

#[test]
fn degenerate_empty_grade_slice_does_not_panic() {
    // (h) A parseable slice with an EMPTY grade vector (the meet of ∅, per
    // `lattice::meet`, is the ladder bottom) must still lint without
    // panicking, and domination stays well-typed against a bar.
    let bottom = tier("Registered", 0);
    let report = report_with(vec![], vec![], bottom.clone(), vec![], vec![]);

    // No bar at all → trivially passes, no panic.
    let advisory_only = lint_report(&report, None, None);
    assert!(advisory_only.passed);
    let _ = advisory_only.render_text(&report);

    // A bar strictly above the empty-meet bottom → fails cleanly, no panic.
    let bar = tier("Grounded", 1);
    let gated = lint_report(&report, Some(&bar), None);
    assert!(!gated.passed);
    assert_eq!(gated.effective_bar, Some(bar));
    let rendered = gated.render_text(&report);
    assert!(rendered.contains("lint FAILED"));
}
