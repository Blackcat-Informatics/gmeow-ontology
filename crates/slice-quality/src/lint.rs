// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The shared slice-quality LINT policy: a tier-domination gate over the tier
//! lattice plus graded (never-gating) advisories.
//!
//! `lint_report` is a pure view over an already-computed [`SliceReport`] — it
//! never re-scores. This is the checkout-free consumer analogue of the dev
//! ratchet gate in [`crate::gate`]: the same objective ladder domination, but
//! evaluated against (i) the slice's own declared `gmeow:sliceQualityTier`
//! claim (undeclared ⇒ no claim to gate) and/or (ii) an explicit `--min-tier`
//! bar, with no calibration-tuned floor. [`tier_gate_passes`] and
//! [`resolve_min_tier`] are relocated here (byte-identical) from the
//! `gmeow-dev-cli` dev-local copies they replace, so the dev G11 gate and the
//! consumer `slice lint` gate share one policy, never two independently
//! drifting implementations.

use std::collections::BTreeMap;
use std::path::Path;

use gmeow_errors::{Finding, Location, Report, Rule, Severity};
use gmeow_validate::rule_catalog::help_uri_for;

use crate::gate;
use crate::model::{MeasurementStandard, Tier};
use crate::report::{self, SliceReport};

/// The tier-domination gate decision: does `measured` satisfy the `required`
/// bar? `required == None` is the advisory case (always passes); otherwise the
/// ladder's total order ([`Tier::sort_key`]) decides, so `measured` must be at
/// or above `required`. This is the single source of truth both the dev
/// `--min-tier` gate and the consumer `slice lint` gate read.
#[must_use]
pub fn tier_gate_passes(measured: &Tier, required: Option<&Tier>) -> bool {
    match required {
        None => true,
        Some(req) => measured.sort_key() >= req.sort_key(),
    }
}

/// Resolve a `--min-tier` argument against the rubric ladder, accepting either a
/// tier's human label (`Grounded`) or its IRI local name (`tierGrounded`),
/// case-insensitively. Returns a clear error naming the available rungs on an
/// unknown tier — a HARD FAIL, never a silently-ignored gate request.
///
/// # Errors
/// Returns a message naming every known rung when `name` matches none of them.
pub fn resolve_min_tier<'a>(
    standard: &'a MeasurementStandard,
    name: &str,
) -> gmeow_errors::Result<&'a Tier> {
    let local_of =
        |iri: &str| -> String { iri.rsplit(['/', '#']).next().unwrap_or(iri).to_owned() };
    if let Some(t) = standard
        .tiers
        .iter()
        .find(|t| t.label.eq_ignore_ascii_case(name) || local_of(&t.iri).eq_ignore_ascii_case(name))
    {
        return Ok(t);
    }
    let mut rungs: Vec<&Tier> = standard.tiers.iter().collect();
    rungs.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
    let known: Vec<String> = rungs.iter().map(|t| t.label.clone()).collect();
    Err(gmeow_errors::Diag::of_kind(crate::error::Gate {
        detail: format!(
            "slice-quality: unknown --min-tier {name:?} (want one of: {})",
            known.join(", ")
        ),
    }))
}

/// The `gmeow:sliceQualityTier` a slice's `manifest.ttl` declares, resolved
/// against a bundle-flattened [`MeasurementStandard`]'s ladder — the
/// checkout-free analogue of [`gate::declared_tier`] (which resolves against a
/// full repo-loaded [`crate::model::Rubric`]). Shares its manifest-reading and
/// ladder-resolution logic via [`gate::declared_tier_against`], so the two
/// never independently drift on what predicate they read or how they resolve
/// it — only the ladder source differs (`Rubric` vs `MeasurementStandard`).
/// `None` when the slice declares no claim (undeclared, advisory-only, never
/// a gate).
///
/// # Errors
/// Returns a message if the manifest cannot be read or names a tier the
/// standard's ladder does not define (a hard error — an unknown tier is never
/// silently ignored).
pub fn declared_quality_tier(
    slice_dir: &Path,
    standard: &MeasurementStandard,
) -> gmeow_errors::Result<Option<Tier>> {
    gate::declared_tier_against(slice_dir, &standard.tiers)
}

/// The higher-rank of two optional tiers — `None` iff both are `None`. The
/// effective lint bar is the stricter of the slice's own declared claim and an
/// explicit `--min-tier`.
fn higher_rank(a: Option<&Tier>, b: Option<&Tier>) -> Option<Tier> {
    match (a, b) {
        (None, None) => None,
        (Some(t), None) | (None, Some(t)) => Some(t.clone()),
        (Some(x), Some(y)) => Some(if x.rank >= y.rank {
            x.clone()
        } else {
            y.clone()
        }),
    }
}

/// Add an honest slice-owned file anchor when an advisory has no parser location.
fn attach_file_level_source(report: &SliceReport, finding: &mut Finding) {
    if finding
        .locations
        .iter()
        .any(|location| location.path.is_some())
    {
        return;
    }
    finding.locations.insert(
        0,
        Location::new(
            Some(report.lint_source_path(finding).to_owned()),
            None,
            None,
            None,
        ),
    );
}

/// The full result of linting an already-scored [`SliceReport`] against an
/// effective tier bar.
pub struct LintOutcome {
    /// The lint-view diagnostics report: every advisory re-stamped by severity
    /// relative to the bar, plus (when failing) the synthetic below-bar
    /// finding. Does NOT carry the Info per-axis grade/roll-up notes —
    /// that is [`SliceReport::to_report`]'s view, not lint's.
    pub findings: Report,
    /// Whether the measured roll-up dominates the effective bar (or no bar
    /// applies at all).
    pub passed: bool,
    /// The stricter (higher-rank) of the slice's declared claim and any
    /// explicit `--min-tier`; `None` when neither is present (a pure advisory
    /// view — always passes).
    pub effective_bar: Option<Tier>,
    /// The number of advisories the underlying [`SliceReport`] surfaced
    /// (`report.advisories.len()`), independent of how lint re-stamped them.
    pub issue_count: usize,
}

/// Tier-domination gate + graded advisories over an already-computed
/// [`SliceReport`]. Never re-scores.
///
/// `effective_bar` is the higher-rank of `declared` (the slice's own
/// `gmeow:sliceQualityTier` claim, via [`declared_quality_tier`]) and
/// `required` (an explicit `--min-tier`, via [`resolve_min_tier`]); `None`
/// when both are absent (a pure advisory view — always passes). `passed`
/// holds iff `effective_bar` is `None` or the measured roll-up's rank is at
/// least the bar's rank.
///
/// Every advisory in `report.advisories` is re-stamped by severity: `Error`
/// when its stored producing axis ([`SliceReport::advisory_axis`]) is graded
/// below the bar (actionable to reach it), else `Warning` — except
/// `slice-quality.axis-advice.missing-template` (a bundle-rubric data gap,
/// never the foreign author's fault), which always stays `Info` and never
/// escalates. When the gate fails, one more synthetic
/// `slice-quality.lint.below-min-tier` `Error` finding is added naming the
/// measured roll-up and the bar.
#[must_use]
pub fn lint_report(
    report: &SliceReport,
    declared: Option<&Tier>,
    required: Option<&Tier>,
) -> LintOutcome {
    let effective_bar = higher_rank(declared, required);
    let passed = match &effective_bar {
        None => true,
        Some(bar) => report.assessment.rollup.rank >= bar.rank,
    };

    // Register every slice-quality code before any finding is built, so the
    // emitted report carries registered (never bare) diagnostic codes — the
    // same discipline `SliceReport::to_report` follows.
    report::seed_finding_codes();
    let mut findings = Report::new("slice-quality");
    for (idx, advisory) in report.advisories.iter().enumerate() {
        let mut finding = advisory.clone();
        finding.severity = if finding.code == "slice-quality.axis-advice.missing-template" {
            // A rubric-provenance data gap, never the foreign author's fault —
            // never gates, never escalates with the bar.
            Severity::Info
        } else {
            let below_bar = effective_bar.as_ref().is_some_and(|bar| {
                report
                    .advisory_axis(idx)
                    .and_then(|axis_iri| report.grade_for_axis_iri(axis_iri))
                    .is_some_and(|grade| grade.tier.rank < bar.rank)
            });
            if below_bar {
                Severity::Error
            } else {
                Severity::Warning
            }
        };
        attach_file_level_source(report, &mut finding);
        findings.add_finding(finding);
    }
    if !passed && let Some(bar) = &effective_bar {
        let mut finding = Finding::new(
            Severity::Error,
            "slice-quality.lint.below-min-tier",
            format!(
                "roll-up tier {} is below the required {}",
                report.assessment.rollup.label, bar.label
            ),
        )
        .with_tool("slice-quality");
        attach_file_level_source(report, &mut finding);
        findings.add_finding(finding);
    }

    // Attach a rule descriptor for every distinct emitted code, each carrying a
    // help URI into the generated constraint catalog (`help_uri_for`) — mirrors
    // `SliceReport::to_report`'s rule-attachment tail exactly, so every lint
    // finding surfaces a registered code + help URI through the json/sarif
    // renderers too.
    let mut severities: BTreeMap<String, Severity> = BTreeMap::new();
    for finding in &findings.findings {
        severities
            .entry(finding.code.clone())
            .or_insert(finding.severity);
    }
    for (code, severity) in severities {
        let mut rule = Rule::new(code.clone(), severity);
        rule.help_uri = Some(help_uri_for(&code));
        findings.add_rule(rule);
    }

    LintOutcome {
        findings,
        passed,
        effective_bar,
        issue_count: report.advisories.len(),
    }
}

impl LintOutcome {
    /// A deterministic human-facing text rendering: the measured roll-up, the
    /// per-axis grades weakest-first each marked `OK`/`below` relative to the
    /// effective bar, the advisories grouped under their attributed axis
    /// (weakest-first), and a final pass/fail summary line.
    #[must_use]
    pub fn render_text(&self, report: &SliceReport) -> String {
        struct Grouped<'a> {
            axis: Option<&'a str>,
            severity: Severity,
            code: &'a str,
            message: &'a str,
        }

        let mut out = String::new();
        out.push_str(&format!(
            "slice-quality lint: {}\n  roll-up tier: {}\n",
            report.assessment.slice, report.assessment.rollup.label
        ));

        let grades = report.grades_weakest_first();
        out.push_str("  per-axis grades (weakest first):\n");
        for grade in grades.iter().copied() {
            let local = grade
                .axis_iri
                .rsplit(['/', '#'])
                .next()
                .unwrap_or(grade.axis_iri.as_str());
            let marker = self.effective_bar.as_ref().map_or("OK", |bar| {
                if grade.tier.rank >= bar.rank {
                    "OK"
                } else {
                    "below"
                }
            });
            out.push_str(&format!(
                "    [{marker}] {local}: {} ({:.2})\n",
                grade.tier.label, grade.score
            ));
        }

        // Advisories, grouped under their attributed axis — excludes the synthetic
        // below-bar finding (it names no single axis). `self.findings.findings` is
        // built by `lint_report` in the same order as `report.advisories`/
        // `report.advisory_axis`, with the synthetic finding (when present) always
        // appended last, so the pre-filter index still identifies the producing
        // advisory exactly.
        let grouped: Vec<Grouped<'_>> = self
            .findings
            .findings
            .iter()
            .enumerate()
            .filter(|(_, f)| f.code != "slice-quality.lint.below-min-tier")
            .map(|(idx, f)| Grouped {
                axis: report.advisory_axis(idx),
                severity: f.severity,
                code: f.code.as_str(),
                message: f.message.as_str(),
            })
            .collect();

        if grouped.is_empty() {
            out.push_str("  no advisories.\n");
        } else {
            out.push_str(&format!(
                "  advisories ({}), by axis (weakest first):\n",
                grouped.len()
            ));
            for grade in grades.iter().copied() {
                let mine: Vec<&Grouped<'_>> = grouped
                    .iter()
                    .filter(|g| g.axis == Some(grade.axis_iri.as_str()))
                    .collect();
                if mine.is_empty() {
                    continue;
                }
                let local = grade
                    .axis_iri
                    .rsplit(['/', '#'])
                    .next()
                    .unwrap_or(grade.axis_iri.as_str());
                out.push_str(&format!("    {local}:\n"));
                for g in mine {
                    out.push_str(&format!(
                        "      [{}] {} — {}\n",
                        g.severity.as_str(),
                        g.code,
                        g.message
                    ));
                }
            }
            let unattributed: Vec<&Grouped<'_>> =
                grouped.iter().filter(|g| g.axis.is_none()).collect();
            if !unattributed.is_empty() {
                out.push_str("    (unattributed):\n");
                for g in unattributed {
                    out.push_str(&format!(
                        "      [{}] {} — {}\n",
                        g.severity.as_str(),
                        g.code,
                        g.message
                    ));
                }
            }
        }

        if self.passed {
            out.push_str(&format!(
                "lint OK — {} advisory warning(s)\n",
                self.issue_count
            ));
        } else {
            let bar_label = self
                .effective_bar
                .as_ref()
                .map_or("<none>", |t| t.label.as_str());
            out.push_str(&format!(
                "lint FAILED: roll-up {} below required {bar_label}\n",
                report.assessment.rollup.label
            ));
        }
        out
    }
}

#[path = "lint.tests.rs"]
#[cfg(test)]
mod tests;
