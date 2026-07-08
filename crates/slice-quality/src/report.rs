// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! End-to-end slice scoring: assemble the slice graph, run every rubric axis, and
//! build the assessment + the advisory report on the diagnostics substrate.

use std::path::{Path, PathBuf};

use gmeow_errors::{Finding, Report, Severity, Standpoint};

use crate::graph::{self, instances_of};
use crate::model::{Axis, AxisGrade, Rubric, SliceAssessment};
use crate::score::ScoreContext;
use crate::{axes, lattice, rubric};

/// The full result of scoring one slice.
pub struct SliceReport {
    /// The rubric the slice was scored against.
    pub rubric: Rubric,
    /// The per-axis grade vector + roll-up tier.
    pub assessment: SliceAssessment,
    /// Every advisory finding the axes surfaced, ranked (heaviest axis first).
    pub advisories: Vec<Finding>,
    /// The axis IRI paired with each advisory, for weight-ranking and grouping.
    axis_weight: std::collections::HashMap<String, f64>,
}

/// Discover a slice's ontology IRI from its `manifest.ttl` (`a gmeow:Slice`).
fn slice_iri_of(slice_dir: &Path) -> Result<String, String> {
    let manifest = slice_dir.join("manifest.ttl");
    let ds = crate::dataset_from_paths(&[&manifest])?;
    instances_of(&ds, &graph::g("Slice"))
        .into_iter()
        .next()
        .ok_or_else(|| format!("{} declares no gmeow:Slice", manifest.display()))
}

/// Every `.ttl` under `slice_dir`'s `module.ttl`, `examples/`, and `tests/`.
fn slice_ttl_paths(slice_dir: &Path) -> Vec<PathBuf> {
    let mut paths = vec![slice_dir.join("module.ttl")];
    for sub in ["examples", "tests"] {
        collect_ttl(&slice_dir.join(sub), &mut paths);
    }
    paths.retain(|p| p.is_file());
    paths.sort();
    paths
}

fn collect_ttl(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_ttl(&p, out);
        } else if p.extension().is_some_and(|x| x == "ttl") {
            out.push(p);
        }
    }
}

/// Score `slice_dir` against the repo's rubric.
///
/// Every rubric axis binds a measurement primitive; an axis whose producer the
/// kernel does not implement is a hard error (never a silent skip).
///
/// # Errors
/// Returns a message if the rubric or the slice graph cannot be loaded, or if the
/// rubric names a producer with no implemented primitive.
pub fn score_slice(repo_root: &Path, slice_dir: &Path) -> Result<SliceReport, String> {
    let rubric = crate::load_repo_rubric(repo_root)?;
    score_slice_with_rubric(slice_dir, rubric)
}

/// Score `slice_dir` against an already-loaded rubric (the sweep path reuses one).
///
/// # Errors
/// As [`score_slice`].
pub fn score_slice_with_rubric(slice_dir: &Path, rubric: Rubric) -> Result<SliceReport, String> {
    let slice_iri = slice_iri_of(slice_dir)?;
    let paths = slice_ttl_paths(slice_dir);
    let path_refs: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
    let ds = crate::dataset_from_paths(&path_refs)?;
    let ctx = ScoreContext::new(slice_iri.clone(), slice_dir.to_path_buf(), &ds);

    let mut scores: Vec<(&Axis, f64)> = Vec::with_capacity(rubric.axes.len());
    let mut advisories: Vec<(String, f64, Finding)> = Vec::new();
    let mut axis_weight = std::collections::HashMap::new();
    for axis in &rubric.axes {
        axis_weight.insert(axis.iri.clone(), axis.weight);
        let primitive = axes::resolve(&axis.producer).ok_or_else(|| {
            format!(
                "rubric axis {} names producer '{}' with no implemented primitive (hard fail)",
                axis.iri, axis.producer
            )
        })?;
        let result = primitive(&ctx);
        for f in result.findings {
            advisories.push((axis.iri.clone(), axis.weight, f));
        }
        scores.push((axis, result.score.clamp(0.0, 1.0)));
    }

    let assessment = lattice::assess(&slice_iri, &scores, &rubric);

    // Rank advice: heaviest axis first, then finding code, then message — a
    // deterministic total order (no derived Ord over the float; explicit key).
    advisories.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.2.code.cmp(&b.2.code))
            .then_with(|| a.2.message.cmp(&b.2.message))
    });
    let advisories: Vec<Finding> = advisories.into_iter().map(|(_, _, f)| f).collect();

    Ok(SliceReport {
        rubric,
        assessment,
        advisories,
        axis_weight,
    })
}

impl SliceReport {
    /// The roll-up tier label.
    #[must_use]
    pub fn rollup_label(&self) -> &str {
        &self.assessment.rollup.label
    }

    /// The per-axis grades sorted by the weakest tier first (the uplift order).
    #[must_use]
    pub fn grades_weakest_first(&self) -> Vec<&AxisGrade> {
        let mut v: Vec<&AxisGrade> = self.assessment.grades.iter().collect();
        v.sort_by(|a, b| {
            a.tier
                .rank
                .cmp(&b.tier.rank)
                .then_with(|| {
                    let wa = self.axis_weight.get(&a.axis_iri).copied().unwrap_or(0.0);
                    let wb = self.axis_weight.get(&b.axis_iri).copied().unwrap_or(0.0);
                    wb.partial_cmp(&wa).unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| a.axis_iri.cmp(&b.axis_iri))
        });
        v
    }

    /// Build the advisory [`Report`] on the diagnostics substrate: every axis grade
    /// as an informational note, every uplift item as an `Advisory` warning.
    #[must_use]
    pub fn to_report(&self) -> Report {
        let mut report = Report::new("slice-quality");
        // Per-axis grade notes (never gating).
        for grade in self.grades_weakest_first() {
            let local = grade
                .axis_iri
                .rsplit(['/', '#'])
                .next()
                .unwrap_or(&grade.axis_iri);
            report.add_finding(
                Finding::new(
                    Severity::Info,
                    "slice-quality.grade",
                    format!("{local}: {} (score {:.2})", grade.tier.label, grade.score),
                )
                .with_tool("slice-quality")
                .with_standpoint(Standpoint::Advisory),
            );
        }
        // Roll-up.
        report.add_finding(
            Finding::new(
                Severity::Info,
                "slice-quality.rollup",
                format!(
                    "roll-up tier: {} ({})",
                    self.assessment.rollup.label, self.assessment.slice
                ),
            )
            .with_tool("slice-quality")
            .with_standpoint(Standpoint::Advisory),
        );
        // Ranked uplift advisories.
        for f in &self.advisories {
            report.add_finding(f.clone());
        }
        report
    }

    /// A deterministic human-facing text rendering.
    #[must_use]
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "slice-quality: {}\n  roll-up tier: {}\n",
            self.assessment.slice, self.assessment.rollup.label
        ));
        out.push_str("  per-axis grades (weakest first):\n");
        for grade in self.grades_weakest_first() {
            let local = grade
                .axis_iri
                .rsplit(['/', '#'])
                .next()
                .unwrap_or(&grade.axis_iri);
            out.push_str(&format!(
                "    {local}: {} ({:.2})\n",
                grade.tier.label, grade.score
            ));
        }
        if self.advisories.is_empty() {
            out.push_str("  no uplift advice — the slice meets every axis.\n");
        } else {
            out.push_str(&format!(
                "  ranked uplift advice ({}):\n",
                self.advisories.len()
            ));
            for (i, f) in self.advisories.iter().enumerate() {
                out.push_str(&format!("    {}. [{}] {}\n", i + 1, f.code, f.message));
            }
        }
        out
    }
}

/// Re-export the rubric loader path used by the sweep.
pub use rubric::load_rubric;
