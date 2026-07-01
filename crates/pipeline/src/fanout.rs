// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Fanout: project the flat consumer tree back out of `gmeow.gts`.
//!
//! Fanout is a separate phase that runs AFTER the pipeline ends (PIPELINE_SPINE §6).
//! It is pure projection — it reads the shipped `generated/dist/gmeow.gts` and writes
//! every other committed file under `generated/`, performing NO computation, reasoning,
//! or assembly. Each committed output is reconstructed by
//! [`crate::stages::superset::project_bundle`] — the single reconstruction authority the
//! superset gate uses to byte-compare — so fanout WRITES exactly what the gate proves
//! is reconstructible. An output that cannot be produced by projection alone is a §5
//! violation upstream, not a need for computation here.

use std::path::Path;

use crate::error::PipelineError;
use crate::run::{write_artifact, GTS_PATH};
use crate::stages::superset::project_bundle;

/// The outcome of one fanout run: how many committed files the bundle projected and
/// how many the write reconciler actually rewrote (vs. left unchanged).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FanoutReport {
    /// Committed `generated/` files reconstructed from the bundle.
    pub produced: usize,
    /// Files whose on-disk bytes changed and were rewritten.
    pub written: usize,
    /// Files already byte-identical on disk (no rewrite needed).
    pub skipped: usize,
}

/// Reconstruct every committed `generated/` file from `<root>/generated/dist/gmeow.gts`
/// and write it to its path under `root`. Pure projection: the ONLY input is the
/// shipped bundle (read from disk), so this reproduces `generated/` from `gmeow.gts`
/// alone. The bundle itself (`GTS_PATH`) is never rewritten — it is the source, not a
/// projection of itself. A missing bundle HARD-fails (no degraded pass).
pub fn fanout(root: &Path) -> Result<FanoutReport, PipelineError> {
    let gts_path = root.join(GTS_PATH);
    let gts = std::fs::read(&gts_path).map_err(|e| PipelineError::Stage {
        stage: "fanout".to_string(),
        message: format!("read {}: {e}", gts_path.display()),
    })?;
    let projection = project_bundle(&gts)?;

    let mut written = 0usize;
    let mut skipped = 0usize;
    for (path, bytes) in &projection.files {
        if write_artifact(root, path, bytes)? {
            written += 1;
        } else {
            skipped += 1;
        }
    }
    Ok(FanoutReport {
        produced: projection.files.len(),
        written,
        skipped,
    })
}
