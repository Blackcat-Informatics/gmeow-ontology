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
//!
//! §6 mandates that fanout be "embarrassingly parallel". It is: each `(path, bytes)`
//! entry is written independently. Safety under concurrency comes from the projection's
//! keys being UNIQUE repo-relative paths (`project_bundle` builds a `BTreeMap` and
//! hard-fails on any key collision), and [`write_artifact`] deriving its temp-file name
//! from the target's own parent directory + basename before an atomic rename — so
//! distinct final paths yield distinct temp paths (it is NOT the wall-clock nonce that
//! prevents collisions; that can repeat on a coarse clock). `create_dir_all` swallows
//! `EEXIST` on Linux, so concurrent creation of a shared parent directory is safe here.
//! The write set is a pure function of the bundle, so the produced tree and the
//! `FanoutReport` counters are identical regardless of `jobs`.

use std::path::Path;

use rayon::prelude::*;

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
///
/// `jobs` is the parallelism budget: the independent per-file writes run on a local
/// rayon pool of that many threads (clamped to `>= 1`), honouring the budget without
/// touching the global pool (mirroring [`crate::scheduler`]).
pub fn fanout(root: &Path, jobs: usize) -> Result<FanoutReport, PipelineError> {
    // GENERATED-READ-OK: fanout is the post-pipeline projection phase (PIPELINE_SPINE §6); it reads
    // the freshly-emitted terminal bundle gmeow.gts (the source of truth, never a fanout projection)
    // to write the rest of generated/ outward — the read result never folds into gmeow.gts, so it
    // cannot trigger the stale-disk-fold bug class.
    let gts_path = root.join(GTS_PATH);
    let gts = std::fs::read(&gts_path).map_err(|e| PipelineError::Stage {
        stage: "fanout".to_string(),
        message: format!("read {}: {e}", gts_path.display()),
    })?;
    let projection = project_bundle(&gts)?;

    // Collect to a Vec of borrows so the parallel iterator is over a slice (no reliance
    // on `BTreeMap`'s parallel-iterator support). Order is irrelevant: writes are
    // independent and the counters are order-insensitive.
    let items: Vec<(&String, &Vec<u8>)> = projection.files.iter().collect();

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(jobs.max(1))
        .build()
        .map_err(|e| PipelineError::Stage {
            stage: "fanout".to_string(),
            message: format!("failed to build rayon pool: {e}"),
        })?;

    // Each entry -> `true` when the write reconciler rewrote changed bytes, `false` when
    // the on-disk file was already byte-identical. A write error aborts the whole fanout.
    let rewritten: Vec<bool> = pool.install(|| {
        items
            .par_iter()
            .map(|(path, bytes)| write_artifact(root, path, bytes))
            .collect::<Result<Vec<bool>, _>>()
    })?;

    let written = rewritten.iter().filter(|&&b| b).count();
    Ok(FanoutReport {
        produced: projection.files.len(),
        written,
        skipped: projection.files.len() - written,
    })
}
