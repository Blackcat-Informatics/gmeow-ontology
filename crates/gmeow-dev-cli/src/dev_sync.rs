// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Unified repository synchronization: one pipeline execution, optional external
//! docs fanout, strict checking, a whole-run clean manifest, and a worktree-local
//! cross-process lock.

use std::collections::BTreeSet;
use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use gmeow_cli_core::{ConsoleMode, Reporter};
use gmeow_pipeline::cache::BUILD_FINGERPRINT;
use gmeow_pipeline::run::{RunMode, RunReport, run_full};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::dev_common::{
    fail, project_root, reporter_for, resolve_console, resolve_jobs, write_timings_json,
};
use crate::{SyncMode, SyncOutput};

const MANIFEST_VERSION: u32 = 1;
const LOCK_ROOT_ENV: &str = "GMEOW_TASK_LOCK_ROOT";
const LOCK_TOKEN_ENV: &str = "GMEOW_TASK_LOCK_TOKEN";

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
    input_digest: String,
    output: String,
    language: String,
    strict_checked: bool,
    docs_rendered: bool,
    managed_roots: Vec<String>,
    files: Vec<FileWitness>,
}

/// A process-owned advisory lock. A top-level `cargo xtask check` passes its
/// root/token to descendants, making nested `gmeow-dev sync` calls re-entrant
/// while unrelated processes still fail fast with the recorded owner.
struct TaskLock {
    file: Option<File>,
}

impl TaskLock {
    fn acquire(root: &Path, purpose: &str) -> Result<Self, String> {
        let canonical = root
            .canonicalize()
            .map_err(|e| format!("resolve worktree root: {e}"))?;
        let root_text = canonical.to_string_lossy();
        if std::env::var(LOCK_ROOT_ENV).ok().as_deref() == Some(root_text.as_ref())
            && std::env::var(LOCK_TOKEN_ENV).is_ok_and(|token| !token.is_empty())
        {
            return Ok(Self { file: None });
        }

        let dir = root.join(".cache/gmeow-task");
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("create task-lock directory {}: {e}", dir.display()))?;
        let path = dir.join("runner.lock");
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| format!("open task lock {}: {e}", path.display()))?;
        match file.try_lock() {
            Ok(()) => {
                let owner = format!(
                    "pid={} purpose={purpose} root={}\n",
                    std::process::id(),
                    root.display()
                );
                file.set_len(0)
                    .map_err(|e| format!("truncate task lock: {e}"))?;
                file.seek(SeekFrom::Start(0))
                    .map_err(|e| format!("seek task lock: {e}"))?;
                file.write_all(owner.as_bytes())
                    .map_err(|e| format!("write task lock: {e}"))?;
                file.flush().map_err(|e| format!("flush task lock: {e}"))?;
                Ok(Self { file: Some(file) })
            }
            Err(TryLockError::WouldBlock) => {
                let mut owner = String::new();
                let _ = file.seek(SeekFrom::Start(0));
                let _ = file.read_to_string(&mut owner);
                Err(format!(
                    "another GMEOW task owns this worktree{}",
                    if owner.trim().is_empty() {
                        String::new()
                    } else {
                        format!(": {}", owner.trim())
                    }
                ))
            }
            Err(TryLockError::Error(e)) => Err(format!("acquire task lock: {e}")),
        }
    }
}

impl Drop for TaskLock {
    fn drop(&mut self) {
        if let Some(file) = &self.file {
            let _ = file.unlock();
        }
    }
}

fn stream_report(reporter: &dyn Reporter, report: &RunReport) {
    use std::time::Duration;
    for timing in &report.timings {
        reporter.stage_end(
            &timing.phase,
            Duration::from_millis(timing.elapsed_ms as u64),
        );
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
    console: Option<ConsoleMode>,
) -> i32 {
    let root = project_root();
    if list_paths {
        println!("{}", gmeow_pipeline::committed_generated_paths().join(" "));
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
    let reporter = reporter_for(resolve_console(console));
    let language = lang
        .map(str::to_owned)
        .or_else(|| std::env::var("GMEOW_LANG").ok())
        .unwrap_or_else(|| "default".to_string());
    let input_digest = match sync_input_digest(&root) {
        Ok(digest) => digest,
        Err(e) => return fail(format!("hash sync inputs: {e}")),
    };
    let manifest_path = manifest_path(&root, output, &language);
    if let Ok(bytes) = std::fs::read(&manifest_path)
        && let Ok(manifest) = serde_json::from_slice::<SyncManifest>(&bytes)
        && manifest_is_current(&root, &manifest, mode, output, &language, &input_digest)
    {
        println!(
            "sync: clean manifest hit (mode={}, output={}); pipeline and docs skipped",
            mode.as_str(),
            output.as_str()
        );
        if let Some(path) = timings_json {
            let payload = serde_json::json!({
                "command": "sync",
                "mode": mode.as_str(),
                "output": output.as_str(),
                "cache_hit": true,
                "pipeline_runs": 0,
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
    let report = match run_full(&root, jobs, run_mode) {
        Ok(report) => report,
        Err(e) => return fail(format!("sync pipeline failed: {e}")),
    };
    stream_report(reporter.as_ref(), &report);
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

    let docs_rendered = matches!(output, SyncOutput::All | SyncOutput::Docs);
    let docs_paths = if docs_rendered {
        match crate::dev_project::sync_docs(mode == SyncMode::Update, lang) {
            Ok(paths) => paths,
            Err(code) => return code,
        }
    } else {
        Vec::new()
    };

    let mut logical_outputs = report.output_paths.clone();
    if mode == SyncMode::Update {
        logical_outputs.extend(docs_paths);
    }
    let managed_roots = managed_roots(output, mode);
    let files = match capture_outputs(&root, &logical_outputs, &managed_roots) {
        Ok(files) => files,
        Err(e) => return fail(format!("capture sync outputs: {e}")),
    };
    let final_input_digest = match sync_input_digest(&root) {
        Ok(digest) => digest,
        Err(e) => return fail(format!("rehash synchronized inputs: {e}")),
    };
    let manifest = SyncManifest {
        version: MANIFEST_VERSION,
        build_fingerprint: BUILD_FINGERPRINT.to_string(),
        input_digest: final_input_digest,
        output: output.as_str().to_string(),
        language,
        strict_checked: true,
        docs_rendered,
        managed_roots,
        files,
    };
    if let Err(e) = write_manifest(&manifest_path, &manifest) {
        return fail(format!("write sync manifest: {e}"));
    }

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
        let payload = serde_json::json!({
            "command": "sync",
            "mode": mode.as_str(),
            "output": output.as_str(),
            "cache_hit": false,
            "pipeline_runs": 1,
            "produced": report.produced,
            "written": report.written,
            "unchanged": report.skipped_writes,
            "timings": timings,
        });
        let code = write_timings_json(path, &payload);
        if code != 0 {
            return code;
        }
    }
    println!(
        "sync: mode={}, output={}, pipeline-runs=1, written={}, unchanged={}",
        mode.as_str(),
        output.as_str(),
        report.written,
        report.skipped_writes
    );
    0
}

fn default_mode() -> SyncMode {
    if std::env::var("CI").is_ok_and(|value| {
        !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "" | "0" | "false" | "off" | "no"
        )
    }) {
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

fn managed_roots(output: SyncOutput, mode: SyncMode) -> Vec<String> {
    let mut roots = vec!["generated".to_string()];
    if mode == SyncMode::Update && matches!(output, SyncOutput::All | SyncOutput::Docs) {
        roots.push("ontology-docs".to_string());
        roots.push("dist/gmeow-docs".to_string());
    }
    roots
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
        || manifest.input_digest != input_digest
        || manifest.output != output.as_str()
        || manifest.language != language
        || (mode == SyncMode::Check && !manifest.strict_checked)
        || (matches!(output, SyncOutput::All | SyncOutput::Docs) && !manifest.docs_rendered)
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

fn sync_input_digest(root: &Path) -> std::io::Result<String> {
    let mut paths = Vec::new();
    for rel in [
        "slices",
        "dsl",
        "imports",
        "metadata",
        "shapes",
        "queries",
        "evals",
        "bench",
        "docs",
        "i18n",
        "ontology",
        "governance",
        "config",
        "validations",
    ] {
        collect_files(&root.join(rel), root, &mut paths);
    }
    for rel in ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml"] {
        if root.join(rel).is_file() {
            paths.push(rel.to_string());
        }
    }
    paths.sort();
    paths.dedup();
    let mut hasher = Sha256::new();
    hasher.update(BUILD_FINGERPRINT.as_bytes());
    hasher.update(b"\x1e");
    for rel in paths {
        let bytes = std::fs::read(root.join(&rel))?;
        hasher.update(rel.as_bytes());
        hasher.update(b"\x1f");
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
        hasher.update(b"\x1e");
    }
    Ok(hex(&hasher.finalize()))
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
}
