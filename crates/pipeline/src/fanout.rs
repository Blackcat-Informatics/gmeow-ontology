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

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::run::{GTS_PATH, write_artifact};
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
    /// Stale files removed from the generator-owned `generated/` tree.
    pub removed: usize,
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
pub fn fanout(root: &Path, jobs: usize) -> Result<FanoutReport, gmeow_errors::Diag> {
    // GENERATED-READ-OK: fanout is the post-pipeline projection phase (PIPELINE_SPINE §6); it reads
    // the freshly-emitted terminal bundle gmeow.gts (the source of truth, never a fanout projection)
    // to write the rest of generated/ outward — the read result never folds into gmeow.gts, so it
    // cannot trigger the stale-disk-fold bug class.
    let gts_path = root.join(GTS_PATH);
    let gts = std::fs::read(&gts_path).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "fanout".to_string(),
            message: format!("read {}: {e}", gts_path.display()),
        })
    })?;
    let projection = project_bundle(&gts)?;

    // Collect to a Vec of borrows so the parallel iterator is over a slice (no reliance
    // on `BTreeMap`'s parallel-iterator support). Order is irrelevant: writes are
    // independent and the counters are order-insensitive.
    let items: Vec<(&String, &Vec<u8>)> = projection.files.iter().collect();

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(jobs.max(1))
        .build()
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "fanout".to_string(),
                message: format!("failed to build rayon pool: {e}"),
            })
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
    let mut expected = projection.files.keys().cloned().collect::<BTreeSet<_>>();
    // The terminal bundle is fanout's source rather than one of its projections.
    // The optional full release bundle is likewise outside the flat projection.
    expected.extend(crate::stages::superset::EXCLUDED.map(str::to_string));
    let removed = remove_stale_generated(root, &expected)?;
    Ok(FanoutReport {
        produced: projection.files.len(),
        written,
        skipped: projection.files.len() - written,
        removed,
    })
}

/// Remove files no longer represented by the carrier, then prune empty
/// directories. Hidden runtime directories are not part of the committed tree and
/// are never traversed. Byte-identical live files remain untouched.
fn remove_stale_generated(
    root: &Path,
    expected: &BTreeSet<String>,
) -> Result<usize, gmeow_errors::Diag> {
    // GENERATED-READ-OK: post-pipeline fanout enumerates the projection-owned tree only to
    // remove stale leaves; these disk bytes never feed the carrier or any produced artifact.
    let base = root.join("generated");
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    collect_generated_members(&base, root, &mut files, &mut dirs)?;
    let mut removed = 0;
    for (rel, path) in files {
        if !expected.contains(&rel) {
            std::fs::remove_file(&path).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: "fanout".to_string(),
                    message: format!("remove stale generated artifact {}: {e}", path.display()),
                })
            })?;
            removed += 1;
        }
    }
    dirs.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for path in dirs {
        match std::fs::remove_dir(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: "fanout".to_string(),
                    message: format!("prune empty generated directory {}: {e}", path.display()),
                }));
            }
        }
    }
    Ok(removed)
}

fn collect_generated_members(
    dir: &Path,
    root: &Path,
    files: &mut Vec<(String, PathBuf)>,
    dirs: &mut Vec<PathBuf>,
) -> Result<(), gmeow_errors::Diag> {
    let entries = std::fs::read_dir(dir).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "fanout".to_string(),
            message: format!("read generated directory {}: {e}", dir.display()),
        })
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "fanout".to_string(),
                message: format!("read generated directory entry: {e}"),
            })
        })?;
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "fanout".to_string(),
                message: format!("read file type for {}: {e}", path.display()),
            })
        })?;
        if kind.is_dir() {
            collect_generated_members(&path, root, files, dirs)?;
            dirs.push(path);
        } else {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            files.push((rel, path));
        }
    }
    Ok(())
}
