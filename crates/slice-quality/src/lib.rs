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
pub mod graph;
pub mod lattice;
pub mod model;
pub mod reasoner;
pub mod report;
pub mod rubric;
pub mod score;

use std::path::Path;
use std::sync::Arc;

use purrdf::RdfDataset;

pub use model::{
    Axis, AxisGrade, ContextScope, Exemption, Rubric, SliceAssessment, Threshold, Tier,
};

/// Parse one or more Turtle files into a single merged dataset.
///
/// # Errors
/// Returns a message if a file cannot be read or fails to parse.
pub fn dataset_from_paths(paths: &[&Path]) -> Result<Arc<RdfDataset>, String> {
    let mut builder = purrdf::RdfDatasetBuilder::new();
    for path in paths {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let ds = purrdf::parse_dataset(&bytes, "text/turtle", None)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        builder.push_dataset(&ds);
    }
    builder
        .freeze()
        .map_err(|e| format!("dataset freeze failed: {e}"))
}

/// Load the rubric from the canonical rubric slice under `repo_root`.
///
/// # Errors
/// Returns a message if the rubric module cannot be read or is structurally
/// incomplete (a missing tier ladder, an axis without a producer, etc.).
pub fn load_repo_rubric(repo_root: &Path) -> Result<Rubric, String> {
    let module = repo_root.join("slices/core/slice-quality-rubric/module.ttl");
    let ds = dataset_from_paths(&[&module])?;
    rubric::load_rubric(&ds)
}
