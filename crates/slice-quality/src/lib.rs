// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-slice quality report + opinionated uplift advisor — the shared kernel.
//!
//! `gmeow-dev slice-quality slices/<group>/<name>/` scores a slice across the
//! quality axes declared in the ontology-resident rubric
//! (`slices/core/slice-quality-rubric/`) and emits ranked, deterministic uplift
//! advice on the diagnostics substrate at `Standpoint::Advisory`.
//!
//! The rubric is **data** ([`rubric::load_rubric`]); Rust ships only a closed set
//! of measurement primitives the rubric's axes bind to. Grades form a bounded
//! lattice: the roll-up tier is the unweighted meet of the per-axis grades
//! ([`lattice`]). This crate is bound by both the dev CLI and the pipeline MCP.

pub mod axes;
pub mod error;
pub mod gate;
pub mod graph;
pub mod lattice;
pub mod model;
pub mod prioritize;
pub mod reasoner;
pub mod report;
pub mod rubric;
pub mod score;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use purrdf::RdfDataset;

pub use model::{
    Axis, AxisFloorCommitment, AxisGrade, ContextScope, Exemption, Rubric, SliceAssessment,
    SliceTierFloorCommitment, Threshold, Tier,
};

/// The repo-wide slice-quality sweep products, scored in one pass over the discovered
/// slice set: the RDF assessment graph and the diagnostics report that backs JSON/SARIF/HTML
/// projections. Keeping these together prevents the pipeline from running the expensive
/// sweep twice when it needs both the queryable graph and the human-facing report.
pub struct AssessmentArtifacts {
    /// Deterministic N-Quads in the slice-quality assessment graph.
    pub nquads: String,
    /// Aggregate diagnostics report containing every scored slice's grades and advisories.
    pub report: gmeow_errors::Report,
}

/// Parse one or more Turtle files into a single merged dataset.
///
/// # Errors
/// Returns a message if a file cannot be read or fails to parse.
pub fn dataset_from_paths(paths: &[&Path]) -> gmeow_errors::Result<Arc<RdfDataset>> {
    let mut builder = purrdf::RdfDatasetBuilder::new();
    for path in paths {
        let bytes = std::fs::read(path).map_err(|e| {
            gmeow_errors::Diag::of_kind(error::Io {
                detail: format!("{}: {e}", path.display()),
            })
        })?;
        let ds = purrdf::parse_dataset(&bytes, "text/turtle", None).map_err(|e| {
            gmeow_errors::Diag::of_kind(error::Io {
                detail: format!("{}: {e}", path.display()),
            })
        })?;
        builder.push_dataset(&ds);
    }
    builder.freeze().map_err(|e| {
        gmeow_errors::Diag::of_kind(error::Io {
            detail: format!("dataset freeze failed: {e}"),
        })
    })
}

/// Load the rubric from the canonical rubric slice under `repo_root`.
///
/// # Errors
/// Returns a message if the rubric module cannot be read or is structurally
/// incomplete (a missing tier ladder, an axis without a producer, etc.).
pub fn load_repo_rubric(repo_root: &Path) -> gmeow_errors::Result<Rubric> {
    let module = repo_root.join("slices/core/slice-quality-rubric/module.ttl");
    let ds = dataset_from_paths(&[&module])?;
    rubric::load_rubric(&ds)
}

/// Every `slices/<group>/<name>/` directory that holds a `manifest.ttl` — the slice
/// set the quality sweep scores, in deterministic (sorted) order. This is the SINGLE
/// discovery authority shared by the dev CLI sweep, the ratchet gate, and the pipeline
/// carrier producer, so all three score exactly the same slice set (dogfooding
/// coherence: the printed roll-up and the folded `graph/quality-assessment` agree).
#[must_use]
pub fn discover_slice_dirs(slices_root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                if p.join("manifest.ttl").is_file() {
                    out.push(p.clone());
                }
                walk(&p, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(slices_root, &mut out);
    out.sort();
    out
}

/// Every authored `.ttl` the quality sweep reads across all slices: the rubric module,
/// each slice's `manifest.ttl`, and the files [`report::score_slice`] ingests per slice
/// (`module.ttl`, `examples/`, `tests/`). This is the SINGLE authority the pipeline's
/// source-load cache key over the assessment graph consults — if any scored file
/// changes, the attached `graph/quality-assessment` must be recomputed (cache soundness:
/// a stale scored input would ship a stale assessment in `gmeow.gts`). Deterministic and
/// deduplicated; only files that exist are returned.
#[must_use]
pub fn scored_source_files(repo_root: &Path) -> Vec<PathBuf> {
    let mut files = vec![repo_root.join("slices/core/slice-quality-rubric/module.ttl")];
    for dir in discover_slice_dirs(&repo_root.join("slices")) {
        files.push(dir.join("manifest.ttl"));
        files.extend(report::slice_ttl_paths(&dir));
    }
    files.retain(|p| p.is_file());
    files.sort();
    files.dedup();
    files
}

/// Score every discovered slice once and return all first-class assessment products:
/// the RDF graph projection and the aggregate diagnostics report. This is the shared
/// authority for repo-wide outputs, so the dev CLI, pipeline graph, and embedded HTML
/// report can agree without separate sweeps.
///
/// # Errors
/// Hard-fails if the rubric or ANY discovered slice cannot be scored.
pub fn assessment_artifacts(repo_root: &Path) -> gmeow_errors::Result<AssessmentArtifacts> {
    let rubric = load_repo_rubric(repo_root)?;
    let dirs = discover_slice_dirs(&repo_root.join("slices"));
    if dirs.is_empty() {
        return Err(gmeow_errors::Diag::of_kind(error::Report {
            detail: "quality-assessment sweep found no slices".to_string(),
        }));
    }

    let mut nquads = String::new();
    let mut aggregate = gmeow_errors::Report::new("slice-quality");
    for dir in dirs {
        let report = report::score_slice_with_rubric(&dir, rubric.clone())?;
        nquads.push_str(&report.to_gmeow_rdf());
        let diagnostics = report.to_report();
        for finding in diagnostics.findings {
            aggregate.add_finding(finding);
        }
        for rule in diagnostics.rules {
            aggregate.add_rule(rule);
        }
    }
    Ok(AssessmentArtifacts {
        nquads,
        report: aggregate.normalized(),
    })
}

/// Score every discovered slice against the repo rubric and project the combined
/// assessment as deterministic N-Quads in the `gmeow:graph/slice-quality` named graph
/// (each slice's [`report::SliceReport::to_gmeow_rdf`] concatenated in sorted slice-dir
/// order). This is the SINGLE producer the pipeline attaches to the in-memory carrier
/// under `graph/quality-assessment`, so the `gmeow:QualityAssessment` graph ships inside
/// `gmeow.gts` (the issue's headline dogfooding deliverable) rather than only printing.
///
/// # Errors
/// Hard-fails (never a silent skip — no-optionality) if the rubric or ANY discovered
/// slice cannot be scored.
pub fn assessment_nquads(repo_root: &Path) -> gmeow_errors::Result<String> {
    Ok(assessment_artifacts(repo_root)?.nquads)
}
