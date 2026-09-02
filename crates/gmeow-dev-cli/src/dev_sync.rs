// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Unified repository synchronization: one pipeline execution, optional external
//! docs fanout, strict checking, a whole-run clean manifest, and a worktree-local
//! cross-process lock.

use std::collections::BTreeSet;
use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use std::time::UNIX_EPOCH;

use gmeow_cli_core::{ConsoleMode, Reporter};
use gmeow_pipeline::cache::{BUILD_FINGERPRINT, BuildIdentity};
use gmeow_pipeline::run::{RunMode, RunOutputScope, RunReport, run_full_scoped_with_progress};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::dev_common::{
    fail, project_root, reporter_for, resolve_console, resolve_jobs, write_timings_json,
};
use crate::{SyncMode, SyncOutput};

const MANIFEST_VERSION: u32 = 5;
const TELEMETRY_SCHEMA_VERSION: u32 = 2;
const LOCK_ROOT_ENV: &str = "GMEOW_TASK_LOCK_ROOT";
const LOCK_TOKEN_ENV: &str = "GMEOW_TASK_LOCK_TOKEN";
/// The HOST-GLOBAL gate-lock path — one GMEOW gate (`make check`, or the single
/// producer target `make check-sync` on its own) runs on
/// the entire host at a time, regardless of worktree, so sibling-worktree gates cannot
/// interfere. Byte-identical to `crates/xtask/src/main.rs::host_lock_path` so both the
/// `xtask` check runner and this `gmeow-dev sync` writer contend on the SAME file.
///
/// There is deliberately NO override — see the `xtask` copy for why. On a shared
/// machine this lock is the queue, and a hatch out of it is a way for one worktree to
/// starve every other.
///
/// `/var/tmp`, not `/tmp`: both are world-writable + sticky (so the lock stays ONE file
/// for every user and every worktree), but `/tmp` is a tmpfs here, and a tmpfs clear
/// deletes the lock file out from under a LIVE holder — after which a second gate
/// creates a fresh inode and both run. `/var/tmp` is disk-backed and preserved, and
/// sits outside every worktree so no checkout reset can remove it.
fn host_lock_path() -> std::path::PathBuf {
    std::path::PathBuf::from("/var/tmp/gmeow-task/host-runner.lock")
}

/// Whether `pid` names a live process on this host.
fn pid_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

/// The `pid=` field of a lock owner record, when present and parseable.
fn record_pid(record: &str) -> Option<u32> {
    record
        .split_whitespace()
        .find_map(|field| field.strip_prefix("pid="))
        .and_then(|pid| pid.parse().ok())
}

fn open_lock_rw(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
}

/// Open the host gate-lock file read-write, reaping a PROVABLY stale one.
///
/// The lock directory is sticky + world-writable so a gate started by any user contends
/// on the same file. The failure mode that needs reaping is a leftover file whose
/// permissions deny `open(O_RDWR)` to everyone but its long-dead creator: nobody can
/// take that lock and nobody can release it, so the gate is bricked host-wide. When the
/// open fails AND the owner record is readable AND its pid is not alive, the record is
/// provably stale — unlink and recreate.
///
/// A live recorded owner is never reaped, and an unreadable record is never reaped
/// either: unlinking a file a live process holds an `flock` on would let two gates run
/// at once. Kept in lockstep with the `xtask` copy.
fn open_host_lock_file(path: &Path) -> gmeow_errors::Result<File> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| {
            crate::error::sync(format!(
                "create host gate-lock directory {}: {e}",
                dir.display()
            ))
        })?;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o1777));
    }
    let open_error = match open_lock_rw(path) {
        Ok(file) => {
            let _ = file.set_permissions(std::fs::Permissions::from_mode(0o0666));
            return Ok(file);
        }
        Err(e) => e,
    };
    let record = std::fs::read_to_string(path).map_err(|_| {
        crate::error::sync(format!(
            "open host gate lock {}: {open_error}; its owner record is also unreadable, so \
             staleness cannot be proven — remove the file by hand once no GMEOW gate is running",
            path.display()
        ))
    })?;
    match record_pid(&record) {
        Some(pid) if pid_alive(pid) => {
            return Err(crate::error::sync(format!(
                "open host gate lock {}: {open_error}; it is held by live pid {pid} ({})",
                path.display(),
                record.trim()
            )));
        }
        Some(pid) => {
            std::fs::remove_file(path).map_err(|e| {
                crate::error::sync(format!(
                    "host gate lock {} names dead owner pid {pid} but cannot be reaped: {e}",
                    path.display()
                ))
            })?;
        }
        None => {
            return Err(crate::error::sync(format!(
                "open host gate lock {}: {open_error}; its owner record names no pid, so \
                 staleness cannot be proven — remove the file by hand once no GMEOW gate is \
                 running",
                path.display()
            )));
        }
    }
    let file = open_lock_rw(path).map_err(|e| {
        crate::error::sync(format!(
            "open host gate lock {} after reap: {e}",
            path.display()
        ))
    })?;
    let _ = file.set_permissions(std::fs::Permissions::from_mode(0o0666));
    Ok(file)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileWitness {
    path: String,
    len: u64,
    modified_ns: u128,
    sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SyncManifest {
    version: u32,
    build_fingerprint: String,
    build_identity: BuildIdentity,
    input_digest: String,
    output: String,
    language: String,
    strict_checked: bool,
    /// Whether the selected outputs were reconciled to disk, rather than only
    /// rendered and checked in memory.
    materialized: bool,
    docs_rendered: bool,
    managed_roots: Vec<String>,
    files: Vec<FileWitness>,
    /// Deterministic fold over `(path, length, sha256)` for every managed output.
    managed_output_root: String,
    /// Deterministic topological fold of the producing pipeline's immutable receipts.
    stage_receipt_root: String,
}

/// A process-owned advisory lock. A top-level `cargo xtask check` passes its
/// root/token to descendants, making nested `gmeow-dev sync` calls re-entrant
/// while unrelated processes still fail fast with the recorded owner.
struct TaskLock {
    file: Option<File>,
}

impl TaskLock {
    fn acquire(root: &Path, purpose: &str) -> gmeow_errors::Result<Self> {
        let canonical = root
            .canonicalize()
            .map_err(|e| crate::error::sync(format!("resolve worktree root: {e}")))?;
        let root_text = canonical.to_string_lossy();
        if std::env::var(LOCK_ROOT_ENV).ok().as_deref() == Some(root_text.as_ref())
            && std::env::var(LOCK_TOKEN_ENV).is_ok_and(|token| !token.is_empty())
        {
            return Ok(Self { file: None });
        }

        // HOST-GLOBAL lock: at most one GMEOW gate runs on the whole host at a time, so a
        // standalone `make check-sync` here cannot interfere with a `make check` in
        // ANY sibling worktree. (Re-entrant descendants of a running check skip this via
        // the token check above.)
        use std::os::unix::fs::MetadataExt;
        let path = host_lock_path();
        // Bounded retry: each attempt either wins the flock on the file CURRENTLY at
        // `path` — proven by comparing the held descriptor's (dev, ino) against a fresh
        // stat — or observes that the file was swapped underneath it (a reap by a
        // sibling gate) and starts over. Without the identity check a swap would let the
        // swapper and the previous holder both believe they own the host.
        for _ in 0..3 {
            let mut file = open_host_lock_file(&path)?;
            match file.try_lock() {
                Ok(()) => {
                    let identical = match (file.metadata(), std::fs::metadata(&path)) {
                        (Ok(held), Ok(current)) => {
                            held.dev() == current.dev() && held.ino() == current.ino()
                        }
                        _ => false,
                    };
                    if !identical {
                        let _ = file.unlock();
                        continue;
                    }
                    let owner = format!(
                        "pid={} purpose={purpose} root={}\n",
                        std::process::id(),
                        root.display()
                    );
                    // Owner line is diagnostic only (a cross-user pre-existing file may
                    // deny the write); the flock is what makes the gate host-atomic, so a
                    // failed owner write must NOT fail the acquire.
                    let _ = file
                        .set_len(0)
                        .and_then(|()| file.seek(SeekFrom::Start(0)).map(|_| ()))
                        .and_then(|()| file.write_all(owner.as_bytes()))
                        .and_then(|()| file.flush());
                    return Ok(Self { file: Some(file) });
                }
                Err(TryLockError::WouldBlock) => {
                    let mut owner = String::new();
                    let _ = file.seek(SeekFrom::Start(0));
                    let _ = file.read_to_string(&mut owner);
                    // A dead recorded pid here is a stale RECORD, not a stale lock: the
                    // kernel releases a dead process's flock, so a live process holds it
                    // and simply could not rewrite the owner line. Reclaiming would run
                    // two gates at once, so we refuse and say why.
                    let stale_record = record_pid(&owner).is_some_and(|pid| !pid_alive(pid));
                    return Err(crate::error::sync(format!(
                        "another GMEOW gate is already running on this host{}{}",
                        if owner.trim().is_empty() {
                            String::new()
                        } else {
                            format!(": {}", owner.trim())
                        },
                        if stale_record {
                            " (the recorded owner pid is no longer alive; the lock is held by a \
                             live process that could not update the record)"
                        } else {
                            ""
                        }
                    )));
                }
                Err(TryLockError::Error(e)) => {
                    return Err(crate::error::sync(format!("acquire host gate lock: {e}")));
                }
            }
        }
        Err(crate::error::sync(format!(
            "host gate lock {} was replaced repeatedly while acquiring it; refusing to run rather \
             than risk two concurrent gates",
            path.display()
        )))
    }
}

impl Drop for TaskLock {
    fn drop(&mut self) {
        if let Some(file) = &self.file {
            let _ = file.unlock();
        }
    }
}

fn stream_report(reporter: &dyn Reporter, report: &RunReport, include_timings: bool) {
    use std::time::Duration;
    if include_timings {
        for timing in &report.timings {
            reporter.stage_end(
                &timing.phase,
                Duration::from_millis(u64::try_from(timing.elapsed_ms).unwrap_or(u64::MAX)),
            );
        }
    }
    let mut diagnostics = gmeow_errors::Report::new("pipeline");
    for finding in &report.findings {
        diagnostics.add_finding(finding.clone());
    }
    reporter.report(&diagnostics.normalized());
}

/// `gmeow-dev sync`: update locally, check in CI, and produce all projections by default.
#[allow(clippy::too_many_arguments)]
pub fn sync(
    requested_mode: Option<SyncMode>,
    output: SyncOutput,
    jobs: Option<usize>,
    lang: Option<&str>,
    timings_json: Option<&Path>,
    metadata: bool,
    list_paths: bool,
    verbose: bool,
    console: Option<ConsoleMode>,
) -> i32 {
    let root = project_root();
    if list_paths {
        println!("{}", gmeow_pipeline::retained_product_paths().join(" "));
        return 0;
    }
    if metadata {
        let records = match gmeow_pipeline::generator_metadata(&root) {
            Ok(records) => records,
            Err(e) => return fail(format!("generator metadata failed: {e}")),
        };
        for record in records {
            match serde_json::to_string(&record) {
                Ok(line) => println!("{line}"),
                Err(e) => return fail(format!("serialize generator metadata: {e}")),
            }
        }
        return 0;
    }

    let mode = requested_mode.unwrap_or_else(default_mode);
    let jobs = match resolve_jobs(jobs) {
        Ok(jobs) => jobs,
        Err(code) => return code,
    };
    let _lock = match TaskLock::acquire(&root, "sync") {
        Ok(lock) => lock,
        Err(e) => return fail(e),
    };
    let reporter: Arc<dyn Reporter> = Arc::from(reporter_for(resolve_console(console)));
    let language = lang
        .map(str::to_owned)
        .or_else(|| std::env::var("GMEOW_LANG").ok())
        .unwrap_or_else(|| "default".to_string());
    let input_started = Instant::now();
    if verbose {
        reporter.stage_start("sync:hash-inputs");
    }
    let input_digest = match sync_input_digest(&root, output) {
        Ok(digest) => digest,
        Err(e) => return fail(format!("hash sync inputs: {e}")),
    };
    if verbose {
        reporter.stage_end("sync:hash-inputs", input_started.elapsed());
    }
    let manifest_path = manifest_path(&root, output, &language);
    let manifest_started = Instant::now();
    if verbose {
        reporter.stage_start("sync:validate-manifest");
    }
    let current_manifest = std::fs::read(&manifest_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<SyncManifest>(&bytes).ok())
        .filter(|manifest| {
            manifest_is_current(&root, manifest, mode, output, &language, &input_digest)
        });
    if verbose {
        reporter.stage_end("sync:validate-manifest", manifest_started.elapsed());
    }
    if let Some(manifest) = current_manifest {
        println!(
            "sync: clean manifest hit (mode={}, output={}); pipeline and docs skipped",
            mode.as_str(),
            output.as_str()
        );
        if let Some(path) = timings_json {
            let output_bytes = manifest.files.iter().map(|file| file.len).sum::<u64>();
            let payload = serde_json::json!({
                "schema_version": TELEMETRY_SCHEMA_VERSION,
                "command": "sync",
                "mode": mode.as_str(),
                "output": output.as_str(),
                "language": language,
                "cache_hit": true,
                "pipeline_runs": 0,
                "build_identity": manifest.build_identity,
                "input_digest": manifest.input_digest,
                "stage_receipt_root": manifest.stage_receipt_root,
                "managed_output_root": manifest.managed_output_root,
                "managed_output_count": manifest.files.len(),
                "managed_output_bytes": output_bytes,
                "critical_path_ms": 0,
                "stages": [],
                "stage_phases": [],
                "stage_receipts": [],
                "levels": [],
                "deterministic_work": {
                    "input_digest": manifest.input_digest,
                    "stage_receipt_root": manifest.stage_receipt_root,
                    "managed_output_root": manifest.managed_output_root,
                    "managed_output_count": manifest.files.len(),
                    "managed_output_bytes": output_bytes,
                    "stage_receipts": [],
                },
                "observations": {
                    "manifest_hit": true,
                    "pipeline_runs": 0,
                    "critical_path_ms": 0,
                    "stages": [],
                    "stage_phases": [],
                    "levels": [],
                },
            });
            let code = write_timings_json(path, &payload);
            if code != 0 {
                return code;
            }
        }
        return 0;
    }

    let run_mode = match mode {
        SyncMode::Update => RunMode::Update,
        SyncMode::Check => RunMode::Check,
    };
    let output_scope = match output {
        SyncOutput::All => RunOutputScope::All,
        SyncOutput::Generated | SyncOutput::Docs => RunOutputScope::Committed,
    };
    let progress = verbose.then(|| Arc::clone(&reporter));
    let report = match run_full_scoped_with_progress(&root, jobs, run_mode, output_scope, progress)
    {
        Ok(report) => report,
        Err(e) => return fail(format!("sync pipeline failed: {e}")),
    };
    stream_report(reporter.as_ref(), &report, !verbose);
    if !report.drifted.is_empty() {
        for path in &report.drifted {
            gmeow_cli_core::note(
                reporter.as_ref(),
                "gmeow-dev",
                "gmeow-dev.sync.drift",
                format!("drift {path}"),
            );
        }
        return fail(format!(
            "sync found {} drifted artifact(s)",
            report.drifted.len()
        ));
    }

    if let Err(e) = reconcile_owned_tree(
        &root,
        "packages/python/gmeow_models",
        &report.output_paths,
        mode == SyncMode::Update,
    ) {
        return fail(e);
    }

    let docs_rendered = matches!(output, SyncOutput::All | SyncOutput::Docs);
    let docs_report = if docs_rendered {
        let docs_started = Instant::now();
        if verbose {
            reporter.stage_start("sync:docs");
        }
        let result = crate::dev_project::sync_docs(mode == SyncMode::Update, lang);
        if verbose && result.is_ok() {
            reporter.stage_end("sync:docs", docs_started.elapsed());
        }
        match result {
            Ok(report) => report,
            Err(code) => return code,
        }
    } else {
        crate::dev_project::DocsSyncReport::default()
    };

    let mut logical_outputs = report
        .output_paths
        .iter()
        .filter(|path| pipeline_output_selected(output, path))
        .cloned()
        .collect::<Vec<_>>();
    if mode == SyncMode::Update {
        logical_outputs.extend(docs_report.output_paths.iter().cloned());
    }
    let managed_roots = managed_roots(output, mode);
    let files = match capture_outputs(&root, &logical_outputs, &managed_roots) {
        Ok(files) => files,
        Err(e) => return fail(format!("capture sync outputs: {e}")),
    };
    let final_hash_started = Instant::now();
    if verbose {
        reporter.stage_start("sync:rehash-inputs");
    }
    let final_input_digest = match sync_input_digest(&root, output) {
        Ok(digest) => digest,
        Err(e) => return fail(format!("rehash synchronized inputs: {e}")),
    };
    if verbose {
        reporter.stage_end("sync:rehash-inputs", final_hash_started.elapsed());
    }
    let managed_output_root = managed_output_root(&files);
    let manifest = SyncManifest {
        version: MANIFEST_VERSION,
        build_fingerprint: BUILD_FINGERPRINT.to_string(),
        build_identity: BuildIdentity::current(),
        input_digest: final_input_digest,
        output: output.as_str().to_string(),
        language,
        strict_checked: true,
        materialized: mode == SyncMode::Update,
        docs_rendered,
        managed_roots,
        files,
        managed_output_root,
        stage_receipt_root: report.stage_receipt_root.clone(),
    };
    let write_manifest_started = Instant::now();
    if verbose {
        reporter.stage_start("sync:write-manifest");
    }
    if let Err(e) = write_manifest(&manifest_path, &manifest) {
        return fail(format!("write sync manifest: {e}"));
    }
    if verbose {
        reporter.stage_end("sync:write-manifest", write_manifest_started.elapsed());
    }

    let total_produced = report.produced + docs_report.reconciliation.produced;
    let total_written = report.written + docs_report.reconciliation.written;
    let total_unchanged = report.skipped_writes + docs_report.reconciliation.unchanged;
    let total_removed = report.removed + docs_report.reconciliation.removed;
    if let Some(path) = timings_json {
        let timings = report
            .timings
            .iter()
            .map(|timing| {
                serde_json::json!({
                    "phase": timing.phase,
                    "elapsed_ms": timing.elapsed_ms,
                    "metadata": timing.metadata,
                })
            })
            .collect::<Vec<_>>();
        let stages = report
            .stage_timings
            .iter()
            .map(|stage| {
                serde_json::json!({
                    "level": stage.level,
                    "stage": stage.stage_id,
                    "elapsed_ms": stage.elapsed_ms,
                    "cached": stage.cached,
                    "cache_outcome": stage.cache_outcome,
                    "cache_read_bytes": stage.cache_read_bytes,
                    "cache_write_bytes": stage.cache_write_bytes,
                    "cache_hydration_rss_delta_kib": stage.cache_hydration_rss_delta_kib,
                })
            })
            .collect::<Vec<_>>();
        let levels = report
            .level_timings
            .iter()
            .map(|level| {
                serde_json::json!({
                    "level": level.level,
                    "elapsed_ms": level.elapsed_ms,
                    "critical_stage": level.critical_stage,
                })
            })
            .collect::<Vec<_>>();
        let stage_phases = report
            .stage_phase_timings
            .iter()
            .map(|phase| {
                serde_json::json!({
                    "stage": phase.stage_id,
                    "phase": phase.phase,
                    "elapsed_ms": phase.elapsed_ms,
                    "work_metadata": phase.metadata,
                })
            })
            .collect::<Vec<_>>();
        let stage_work = report
            .stage_phase_timings
            .iter()
            .filter_map(|phase| {
                phase.metadata.as_ref().map(|metadata| {
                    serde_json::json!({
                        "stage": phase.stage_id,
                        "phase": phase.phase,
                        "work_metadata": metadata,
                    })
                })
            })
            .collect::<Vec<_>>();
        let stage_phase_observations = report
            .stage_phase_timings
            .iter()
            .map(|phase| {
                serde_json::json!({
                    "stage": phase.stage_id,
                    "phase": phase.phase,
                    "elapsed_ms": phase.elapsed_ms,
                })
            })
            .collect::<Vec<_>>();
        let critical_path_ms = report
            .level_timings
            .iter()
            .map(|level| level.elapsed_ms)
            .sum::<u128>();
        let executed_stage_count = report
            .stage_timings
            .iter()
            .filter(|stage| !stage.cached)
            .count();
        let hydrated_stage_count = report
            .stage_timings
            .iter()
            .filter(|stage| stage.cached)
            .count();
        let cache_read_bytes = report
            .stage_timings
            .iter()
            .map(|stage| stage.cache_read_bytes)
            .sum::<u64>();
        let cache_write_bytes = report
            .stage_timings
            .iter()
            .map(|stage| stage.cache_write_bytes)
            .sum::<u64>();
        let managed_output_bytes = manifest.files.iter().map(|file| file.len).sum::<u64>();
        let payload = serde_json::json!({
            "schema_version": TELEMETRY_SCHEMA_VERSION,
            "command": "sync",
            "mode": mode.as_str(),
            "output": output.as_str(),
            "language": manifest.language,
            "cache_hit": false,
            "pipeline_runs": 1,
            "build_identity": manifest.build_identity,
            "input_digest": manifest.input_digest,
            "stage_receipt_root": manifest.stage_receipt_root,
            "managed_output_root": manifest.managed_output_root,
            "managed_output_count": manifest.files.len(),
            "managed_output_bytes": managed_output_bytes,
            "executed_stage_count": executed_stage_count,
            "hydrated_stage_count": hydrated_stage_count,
            "cache_read_bytes": cache_read_bytes,
            "cache_write_bytes": cache_write_bytes,
            "critical_path_ms": critical_path_ms,
            "produced": total_produced,
            "written": total_written,
            "unchanged": total_unchanged,
            "removed": total_removed,
            "stages": stages,
            "stage_phases": stage_phases,
            "stage_receipts": &report.stage_receipts,
            "levels": levels,
            "timings": timings,
            "deterministic_work": {
                "input_digest": manifest.input_digest,
                "stage_receipt_root": manifest.stage_receipt_root,
                "managed_output_root": manifest.managed_output_root,
                "managed_output_count": manifest.files.len(),
                "managed_output_bytes": managed_output_bytes,
                "stage_receipts": &report.stage_receipts,
                "stage_work": stage_work,
            },
            "observations": {
                "manifest_hit": false,
                "pipeline_runs": 1,
                "executed_stage_count": executed_stage_count,
                "hydrated_stage_count": hydrated_stage_count,
                "cache_read_bytes": cache_read_bytes,
                "cache_write_bytes": cache_write_bytes,
                "critical_path_ms": critical_path_ms,
                "stages": stages,
                "stage_phases": stage_phase_observations,
                "levels": levels,
                "timings": timings,
            },
        });
        let code = write_timings_json(path, &payload);
        if code != 0 {
            return code;
        }
    }
    println!(
        "sync: mode={}, output={}, pipeline-runs=1, produced={}, written={}, unchanged={}, removed={}",
        mode.as_str(),
        output.as_str(),
        total_produced,
        total_written,
        total_unchanged,
        total_removed
    );
    0
}

fn default_mode() -> SyncMode {
    // One reading of `CI` in the workspace: the same predicate the branch-versus-base
    // gates consult to decide whether an absent comparand is a skip or a hard failure.
    if gmeow_pipeline::branch_base::ci_declared() {
        SyncMode::Check
    } else {
        SyncMode::Update
    }
}

fn manifest_path(root: &Path, output: SyncOutput, language: &str) -> PathBuf {
    let safe_language = language
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    root.join(".cache/gmeow-sync/manifests")
        .join(format!("{}-{safe_language}.json", output.as_str()))
}

fn pipeline_output_selected(output: SyncOutput, path: &str) -> bool {
    output == SyncOutput::All || !path.starts_with("dist/")
}

fn managed_roots(output: SyncOutput, mode: SyncMode) -> Vec<String> {
    let mut roots = vec![
        "generated".to_string(),
        "packages/python/gmeow_models".to_string(),
    ];
    if mode == SyncMode::Update && matches!(output, SyncOutput::All | SyncOutput::Docs) {
        roots.push("ontology-docs".to_string());
        roots.push("dist/gmeow-docs".to_string());
    }
    roots
}

fn reconcile_owned_tree(
    root: &Path,
    owned_root: &str,
    logical_outputs: &[String],
    update: bool,
) -> gmeow_errors::Result<()> {
    let prefix = format!("{owned_root}/");
    let expected = logical_outputs
        .iter()
        .filter(|path| path.starts_with(&prefix))
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut current = Vec::new();
    collect_files(&root.join(owned_root), root, &mut current);
    let stale = current
        .iter()
        .filter(|path| !expected.contains(path.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if stale.is_empty() {
        return Ok(());
    }
    if !update {
        return Err(crate::error::sync(format!(
            "sync found {} stale artifact(s) under {owned_root}: {}",
            stale.len(),
            stale.join(", ")
        )));
    }
    for rel in stale {
        std::fs::remove_file(root.join(&rel)).map_err(|e| {
            crate::error::sync(format!("remove stale synchronized artifact {rel}: {e}"))
        })?;
    }
    prune_empty_dirs(&root.join(owned_root)).map_err(|e| {
        crate::error::sync(format!(
            "prune empty synchronized directories under {owned_root}: {e}"
        ))
    })?;
    Ok(())
}

fn prune_empty_dirs(dir: &Path) -> std::io::Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    let mut children = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    children.sort();
    for child in children {
        prune_empty_dirs(&child)?;
        match std::fs::remove_dir(&child) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn manifest_is_current(
    root: &Path,
    manifest: &SyncManifest,
    mode: SyncMode,
    output: SyncOutput,
    language: &str,
    input_digest: &str,
) -> bool {
    if manifest.version != MANIFEST_VERSION
        || manifest.build_fingerprint != BUILD_FINGERPRINT
        || manifest.build_identity != BuildIdentity::current()
        || manifest.input_digest != input_digest
        || manifest.output != output.as_str()
        || manifest.language != language
        || (mode == SyncMode::Check && !manifest.strict_checked)
        || (mode == SyncMode::Update && !manifest.materialized)
        || (matches!(output, SyncOutput::All | SyncOutput::Docs) && !manifest.docs_rendered)
        || manifest.managed_output_root != managed_output_root(&manifest.files)
        || !is_sha256(&manifest.stage_receipt_root)
    {
        return false;
    }

    let expected = manifest
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<BTreeSet<_>>();
    for managed in &manifest.managed_roots {
        let mut current = Vec::new();
        collect_files(&root.join(managed), root, &mut current);
        if current.iter().map(String::as_str).collect::<BTreeSet<_>>()
            != expected
                .iter()
                .copied()
                .filter(|path| *path == managed || path.starts_with(&format!("{managed}/")))
                .collect::<BTreeSet<_>>()
        {
            return false;
        }
    }

    manifest.files.iter().all(|witness| {
        let path = root.join(&witness.path);
        let Ok(metadata) = std::fs::metadata(&path) else {
            return false;
        };
        if metadata.len() == witness.len && modified_ns(&metadata) == witness.modified_ns {
            return true;
        }
        std::fs::read(&path)
            .map(|bytes| sha256(&bytes) == witness.sha256)
            .unwrap_or(false)
    })
}

fn managed_output_root(files: &[FileWitness]) -> String {
    let mut rows = files
        .iter()
        .map(|file| (file.path.as_str(), file.len, file.sha256.as_str()))
        .collect::<Vec<_>>();
    rows.sort_unstable();
    let mut hasher = Sha256::new();
    hasher.update(b"gmeow:sync-managed-output-root:v1\x1f");
    for (path, len, digest) in rows {
        hasher.update(path.as_bytes());
        hasher.update(b"\x1f");
        hasher.update(len.to_le_bytes());
        hasher.update(b"\x1f");
        hasher.update(digest.as_bytes());
        hasher.update(b"\x1e");
    }
    hex(&hasher.finalize())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn capture_outputs(
    root: &Path,
    logical_outputs: &[String],
    managed_roots: &[String],
) -> std::io::Result<Vec<FileWitness>> {
    let mut paths = logical_outputs
        .iter()
        .filter(|path| root.join(path).is_file())
        .cloned()
        .collect::<Vec<_>>();
    for managed in managed_roots {
        collect_files(&root.join(managed), root, &mut paths);
    }
    paths.sort();
    paths.dedup();
    paths
        .into_iter()
        .map(|path| {
            let absolute = root.join(&path);
            let metadata = std::fs::metadata(&absolute)?;
            let bytes = std::fs::read(&absolute)?;
            Ok(FileWitness {
                path,
                len: metadata.len(),
                modified_ns: modified_ns(&metadata),
                sha256: sha256(&bytes),
            })
        })
        .collect()
}

fn modified_ns(metadata: &std::fs::Metadata) -> u128 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos())
}

fn sync_input_digest(root: &Path, output: SyncOutput) -> std::io::Result<String> {
    let paths = declared_sync_input_files(root, output)?;
    let mut hasher = Sha256::new();
    hasher.update(b"gmeow:sync-input-closure:v5\x1f");
    hasher.update(BUILD_FINGERPRINT.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(output.as_str().as_bytes());
    hasher.update(b"\x1e");
    for path in paths {
        let rel = path.strip_prefix(root).map_err(|_| {
            std::io::Error::other(format!(
                "declared sync input {} escapes repository root {}",
                path.display(),
                root.display()
            ))
        })?;
        let rel = rel.to_string_lossy().replace('\\', "/");
        let bytes = std::fs::read(&path)?;
        hasher.update(rel.as_bytes());
        hasher.update(b"\x1f");
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
        hasher.update(b"\x1e");
    }
    Ok(hex(&hasher.finalize()))
}

/// Resolve the exact source closure selected by synchronization.
///
/// The generated profile is the normal pre-commit / `make check` boundary. Its
/// inputs are the union of every bound production stage's own `input_files()`
/// declaration; executable semantics, Cargo/toolchain state, and transitive local
/// library dependencies are already sealed by [`BUILD_FINGERPRINT`]. This avoids the
/// old blanket `crates/` + `tests/` census, under which editing an unrelated Rust test
/// harness invalidated a byte-identical ontology corpus and imposed a full pipeline
/// run before tests could start.
///
/// External documentation additionally reads two build-produced serialization files
/// after the pipeline. They are explicit inputs to that selected branch. Every other
/// docs source is already in a bound stage declaration because the same docs model and
/// assets are embedded into the shipped carrier.
fn declared_sync_input_files(root: &Path, output: SyncOutput) -> std::io::Result<Vec<PathBuf>> {
    let spec = gmeow_pipeline::run::full_spec();
    let graph = spec
        .validate()
        .map_err(|error| std::io::Error::other(format!("validate bound pipeline: {error}")))?;
    let registry = gmeow_pipeline::registry::default_registry();
    let bound = gmeow_pipeline::loader::bind(&spec, &graph, &registry)
        .map_err(|error| std::io::Error::other(format!("bind pipeline inputs: {error}")))?;

    let mut paths = Vec::new();
    for stage in bound {
        paths.extend(stage.input_files(root).map_err(|error| {
            std::io::Error::other(format!(
                "enumerate declared inputs for {}: {error}",
                stage.id()
            ))
        })?);
    }
    if matches!(output, SyncOutput::All | SyncOutput::Docs) {
        for relative in [
            gmeow_pipeline::stages::yaml_ld::JSON_LD_PATH,
            gmeow_pipeline::stages::yaml_ld::YAML_LD_PATH,
            // `sync_docs` is the post-pipeline renderer. Its local orchestration is
            // intentionally outside the pipeline library's build fingerprint, so bind
            // the few source modules that can change this selected branch's bytes.
            "crates/gmeow-dev-cli/src/dev_common.rs",
            "crates/gmeow-dev-cli/src/dev_project.rs",
            "crates/gmeow-dev-cli/src/dev_sync.rs",
        ] {
            let path = root.join(relative);
            if path.is_file() {
                paths.push(path);
            }
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn collect_files(dir: &Path, root: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            if path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with('.'))
            {
                continue;
            }
            collect_files(&path, root, out);
        } else if path.is_file() {
            out.push(
                path.strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}

fn write_manifest(path: &Path, manifest: &SyncManifest) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(manifest)?;
    if std::fs::read(path).is_ok_and(|existing| existing == bytes) {
        return Ok(());
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(".sync-manifest-{}.tmp", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, path)
}

fn sha256(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("canonical repository root")
    }

    #[test]
    /// The authored abstract participates in sync invalidation without widening to harness code.
    fn generated_sync_inputs_exclude_unrelated_test_harness_implementation() {
        // This is a declaration audit only: it binds the DAG and enumerates paths. It
        // never starts a stage, generator, corpus build, or fixture producer.
        let root = repo_root();
        let files = declared_sync_input_files(&root, SyncOutput::Generated)
            .expect("enumerate generated sync input closure");
        let relative = files
            .iter()
            .map(|path| path.strip_prefix(&root).unwrap_or(path))
            .collect::<BTreeSet<_>>();

        assert!(relative.contains(Path::new("ontology/gmeow.ttl")));
        assert!(
            relative.contains(Path::new("metadata/gmeow-abstract.txt")),
            "the authored abstract must invalidate the whole-run manifest before source loading"
        );
        assert!(
            relative.contains(Path::new("tests/fixtures/coverage/external/bii.ttl")),
            "a product-bearing fixture declared by the mappings stage remains an input"
        );
        for unrelated in [
            ".pre-commit-config.yaml",
            "crates/gmeow-dev-cli/tests/make_gate_contract.rs",
            "crates/slicetest/src/repository.rs",
        ] {
            assert!(
                !relative.contains(Path::new(unrelated)),
                "unrelated test/pre-commit implementation must not rebuild the corpus: {unrelated}"
            );
        }
    }

    #[test]
    fn manifest_path_sanitizes_language() {
        let path = manifest_path(Path::new("/tmp/repo"), SyncOutput::All, "en,../../fr");
        assert!(path.ends_with("all-en_______fr.json"));
    }

    #[test]
    fn false_ci_values_choose_update() {
        for value in ["", "0", "false", "off", "no"] {
            assert!(matches!(value, "" | "0" | "false" | "off" | "no"));
        }
    }

    #[test]
    fn read_only_manifest_cannot_satisfy_update() {
        let manifest = SyncManifest {
            version: MANIFEST_VERSION,
            build_fingerprint: BUILD_FINGERPRINT.to_string(),
            build_identity: BuildIdentity::current(),
            input_digest: "same".to_string(),
            output: SyncOutput::Generated.as_str().to_string(),
            language: "default".to_string(),
            strict_checked: true,
            materialized: false,
            docs_rendered: false,
            managed_roots: Vec::new(),
            files: Vec::new(),
            managed_output_root: managed_output_root(&[]),
            stage_receipt_root: "0".repeat(64),
        };
        assert!(!manifest_is_current(
            Path::new("/does/not/matter"),
            &manifest,
            SyncMode::Update,
            SyncOutput::Generated,
            "default",
            "same",
        ));
        assert!(manifest_is_current(
            Path::new("/does/not/matter"),
            &manifest,
            SyncMode::Check,
            SyncOutput::Generated,
            "default",
            "same",
        ));
    }

    #[test]
    fn managed_output_root_binds_content_but_not_observational_mtime() {
        let witness = FileWitness {
            path: "generated/example".to_string(),
            len: 3,
            modified_ns: 1,
            sha256: sha256(b"one"),
        };
        let root = managed_output_root(std::slice::from_ref(&witness));
        let mut changed_mtime = witness.clone();
        changed_mtime.modified_ns = 99;
        assert_eq!(
            managed_output_root(&[changed_mtime]),
            root,
            "mtime is run telemetry, never immutable output identity"
        );
        let mut changed_digest = witness;
        changed_digest.sha256 = sha256(b"two");
        assert_ne!(managed_output_root(&[changed_digest]), root);
    }

    #[test]
    fn generated_and_docs_selection_exclude_unrequested_runtime_outputs() {
        let paths = [
            "generated/module-status.md".to_string(),
            "dist/gmeow-okf/index.md".to_string(),
        ];
        for output in [SyncOutput::Generated, SyncOutput::Docs] {
            let selected = paths
                .iter()
                .filter(|path| pipeline_output_selected(output, path))
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(selected, vec!["generated/module-status.md"]);
        }
        assert_eq!(
            paths
                .iter()
                .filter(|path| pipeline_output_selected(SyncOutput::All, path))
                .count(),
            2
        );
    }
}
