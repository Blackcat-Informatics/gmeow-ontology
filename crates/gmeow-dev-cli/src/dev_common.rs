// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Cross-cutting helpers shared by the `gmeow-dev` command modules.
//!
//! The dev CLI is repo-anchored: unlike the consumer `gmeow` binary (which reads
//! an embedded bundle), every command here operates on the working tree. This
//! module owns the tree-relative path resolution, the on-disk snapshot read, the
//! console convention (product → stdout, diagnostics → stderr), the shared
//! `--jobs` validation, and the deterministic `--timings-json` writer.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::Instant;

use gmeow_cli_core::{ConsoleMode, HumanReporter, NdjsonReporter, Reporter};

/// gmeow's canonical ontology IRI (no trailing slash) — the `ONTOLOGY_IRI`.
pub const ONTOLOGY_IRI: &str = "https://blackcatinformatics.ca/gmeow";
/// gmeow's canonical namespace (trailing slash) — the `NAMESPACE`.
pub const NAMESPACE: &str = "https://blackcatinformatics.ca/gmeow/";
/// The committed unsigned bundle path, relative to the repo root.
pub const GTS_SNAPSHOT_REL: &str = "generated/dist/gmeow.gts";

/// Print an error to stderr and yield the failure exit code `1`.
pub fn fail(message: impl AsRef<str>) -> i32 {
    eprintln!("{}", message.as_ref());
    1
}

/// Print an error to stderr and yield an explicit exit code (e.g. `2` for a
/// tool-unavailable condition, mirroring the Python `_fail(code=2)` paths).
pub fn fail_code(message: impl AsRef<str>, code: i32) -> i32 {
    eprintln!("{}", message.as_ref());
    code
}

/// The repository root the dev CLI operates on.
///
/// The dev binary is invoked from within the checkout (the Makefile shells it),
/// so the current working directory anchors every tree-relative path. `GMEOW_ROOT`
/// overrides it (the parity tests set it to point at the worktree), and as a final
/// fallback the compile-time manifest's grandparent locates the checkout even when
/// the CWD has drifted.
pub fn project_root() -> PathBuf {
    if let Ok(root) = std::env::var("GMEOW_ROOT") {
        return PathBuf::from(root);
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if cwd.join(GTS_SNAPSHOT_REL).exists() || cwd.join("slices").is_dir() {
        return cwd;
    }
    // The workspace root is two levels above this crate's manifest.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap_or(cwd)
}

/// The committed unsigned bundle bytes read from the working tree.
pub fn snapshot_bytes(root: &Path) -> Result<Vec<u8>, i32> {
    let path = root.join(GTS_SNAPSHOT_REL);
    std::fs::read(&path).map_err(|e| {
        fail(format!(
            "cannot read {} ({e}); run `make regenerate` first",
            path.display()
        ))
    })
}

/// Resolve the effective [`ConsoleMode`] from the flag, the `GMEOW_CONSOLE`
/// environment value, and whether stderr is a TTY.
pub fn resolve_console(flag: Option<ConsoleMode>) -> ConsoleMode {
    let env_val = std::env::var("GMEOW_CONSOLE").ok();
    ConsoleMode::resolve(flag, env_val.as_deref(), std::io::stderr().is_terminal())
}

/// A boxed [`Reporter`] for the resolved console mode: human-facing stderr text
/// for interactive/`pretty`/`text` surfaces, line-framed NDJSON for `jsonl`
/// (agents/pipelines), and a silent sink for `silent`.
pub fn reporter_for(mode: ConsoleMode) -> Box<dyn Reporter> {
    match mode {
        ConsoleMode::Jsonl => Box::new(NdjsonReporter::new()),
        ConsoleMode::Silent => Box::new(SilentReporter),
        _ => Box::new(HumanReporter::new()),
    }
}

/// A reporter that suppresses all diagnostic chrome (the `silent` surface).
#[derive(Debug, Default, Clone, Copy)]
pub struct SilentReporter;

impl Reporter for SilentReporter {
    fn report(&self, _report: &gmeow_diagnostics::Report) {}
    fn stage_start(&self, _stage: &str) {}
    fn stage_end(&self, _stage: &str, _elapsed: std::time::Duration) {}
    fn summary(&self, _report: &gmeow_diagnostics::Report) {}
}

/// Reject a non-positive `--jobs` before it reaches the native `usize` boundary
/// (mirrors the Python `_validate_jobs`). `None` means "capped CPU count".
pub fn resolve_jobs(jobs: Option<usize>) -> Result<usize, i32> {
    match jobs {
        Some(0) => Err(fail("number of jobs must be at least 1")),
        Some(n) => Ok(n),
        None => Ok(std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)),
    }
}

/// Elapsed milliseconds since `started`.
pub fn elapsed_ms(started: Instant) -> u128 {
    started.elapsed().as_millis()
}

/// Write a deterministic `--timings-json` artifact (sorted keys, trailing
/// newline). This is PRODUCT DATA, not a log line, so it is written verbatim.
pub fn write_timings_json(path: &Path, value: &serde_json::Value) -> i32 {
    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return fail(format!("cannot create {}: {e}", parent.display()));
    }
    let text = match serde_json::to_string_pretty(value) {
        Ok(s) => s,
        Err(e) => return fail(format!("cannot serialize timings JSON: {e}")),
    };
    if let Err(e) = std::fs::write(path, format!("{text}\n")) {
        return fail(format!("cannot write {}: {e}", path.display()));
    }
    0
}

/// The `logic:` relative-path prefixes the `logic compile --check` drift gate
/// filters the whole-pipeline drift set to (mirrors the Python `_logic_prefixes`).
pub const LOGIC_DRIFT_PREFIXES: &[&str] = &[
    "generated/logic/",
    "generated/owl/",
    "generated/datalog/",
    "generated/n3/",
    "generated/foundation/",
    "generated/shacl-af/",
    "generated/cl/",
];
