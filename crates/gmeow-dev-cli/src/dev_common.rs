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

use gmeow_cli_core::{ConsoleMode, Reporter, report_diag};
use gmeow_errors::{Diag, FindingCategory, Grade, Severity, Standpoint};
// The reporter factory is the shared cli-core surface both bins construct from
// (no per-crate re-implementation) — re-exported here so the dev command modules
// keep importing it from `crate::dev_common`.
pub use gmeow_cli_core::reporter_for;

/// gmeow's canonical ontology IRI (no trailing slash) — the `ONTOLOGY_IRI`.
pub const ONTOLOGY_IRI: &str = "https://blackcatinformatics.ca/gmeow";
/// gmeow's canonical namespace (trailing slash) — the `NAMESPACE`.
pub const NAMESPACE: &str = "https://blackcatinformatics.ca/gmeow/";
/// The committed unsigned bundle path, relative to the repo root.
pub const GTS_SNAPSHOT_REL: &str = "generated/dist/gmeow.gts";

/// The dev-CLI diagnostic reporter. Diagnostics default to the HUMAN stderr
/// surface (`resolve_stderr_default`, not the NDJSON-default `resolve_console`):
/// the dev CLI's stdout carries product data (committed paths, projected graphs,
/// serialized artifacts), so a handled failure or status witness must stay on
/// stderr and never interleave NDJSON into a pipe. An agent still opts into the
/// machine surface with `GMEOW_CONSOLE=jsonl`. Reporters are zero-sized, so
/// resolving one per diagnostic site is free.
pub fn dev_reporter() -> Box<dyn Reporter> {
    let env_val = std::env::var("GMEOW_CONSOLE").ok();
    let mode = ConsoleMode::resolve_stderr_default(
        None,
        env_val.as_deref(),
        std::io::stderr().is_terminal(),
    );
    reporter_for(mode)
}

/// An Error-grade dev diagnostic carrying a per-site stable code — the graded
/// pre-carrier witness a handled `gmeow-dev` failure lowers to (never a bare
/// string). The `code` is interned once (idempotently); the message carries the
/// specifics.
fn error_diag(code: &str, message: impl Into<String>) -> Diag {
    Diag::new(
        gmeow_errors::code::register_code(code),
        Grade::new(
            Severity::Error,
            FindingCategory::ModelingDisciplineViolation,
            Standpoint::Binding,
        ),
        message,
    )
}

/// Emit an Error-grade dev diagnostic on the console sink (human text on stderr,
/// an NDJSON `finding` line for agents, dropped by a silent sink) WITHOUT altering
/// the exit code — the substrate replacement for a bare error stderr write at a
/// site that already carries its own return value.
pub fn emit_error(code: &str, message: impl Into<String>) {
    dev_reporter().report(&report_diag(error_diag(code, message), "gmeow-dev"));
}

/// Emit a Transient status/progress witness (never gating) on the console sink —
/// the substrate replacement for a chatter / per-item status stderr line.
pub fn note(code: &str, message: impl Into<String>) {
    gmeow_cli_core::note(dev_reporter().as_ref(), "gmeow-dev", code, message);
}

/// Project a whole diagnostics [`gmeow_errors::Report`] onto the console sink —
/// the substrate replacement for a hand-rendered `render::to_text(&report)` write: a
/// TTY sees the rendered text on stderr, an agent the NDJSON `finding` lines, and
/// a silent sink drops it. An empty report renders nothing.
pub fn emit_report(report: &gmeow_errors::Report) {
    dev_reporter().report(&report.normalized());
}

/// Emit an Error-grade dev diagnostic on the console sink and yield the failure
/// exit code `1` — the substrate replacement for the old stderr `fail`.
pub fn fail(message: impl std::fmt::Display) -> i32 {
    emit_error("gmeow-dev.cli.fail", message.to_string());
    1
}

/// Emit an Error-grade dev diagnostic on the console sink and yield an explicit
/// exit code (e.g. `2` for a tool-unavailable condition, mirroring the Python
/// `_fail(code=2)` paths).
pub fn fail_code(message: impl std::fmt::Display, code: i32) -> i32 {
    emit_error("gmeow-dev.cli.fail", message.to_string());
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
            "cannot read {} ({e}); run `make regen` first",
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

/// Reject a non-positive `--jobs` before it reaches the native `usize` boundary
/// (mirrors the Python `_validate_jobs`). `None` means every available CPU.
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
