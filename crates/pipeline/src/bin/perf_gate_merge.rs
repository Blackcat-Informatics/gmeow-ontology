// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Merge per-command perf-gate timing JSONs into `gate-timings.json`.
//!
//! Replaces the inline Python one-liner in `make perf-gate`. Reads
//! `validate.json`, `sync.json`, and `reason-verify.json` from the
//! given directory and emits a single JSON object with the identical schema:
//!
//! ```json
//! {"commands": [<validate>, <sync>, <reason-verify>]}
//! ```

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::Duration;

use gmeow_cli_core::{ConsoleMode, Reporter, report_diag};
use gmeow_errors::{Diag, FindingCategory, Grade, Severity, Standpoint};

const FILES: [&str; 3] = ["validate.json", "sync.json", "reason-verify.json"];

/// The emitting tool name every diagnostic here is stamped with.
const TOOL: &str = "perf-gate-merge";

/// An Error-grade pre-carrier diagnostic carrying a per-site stable code — the
/// graded witness a handled failure lowers to (never a bare string). The `code`
/// is interned once (idempotently); the message carries the specifics.
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

/// Surface an Error-grade diagnostic on the console sink WITHOUT altering the
/// caller's return value — the substrate replacement for a bare error stderr
/// write at a site that already carries its own exit path.
fn emit_error(reporter: &dyn Reporter, code: &str, message: impl Into<String>) {
    reporter.report(&report_diag(error_diag(code, message), TOOL));
}

fn main() {
    // stdout carries product data (the written path), so diagnostics default to
    // the HUMAN stderr surface and never interleave NDJSON into a pipe; an agent
    // opts into the machine surface with `GMEOW_CONSOLE=jsonl`.
    let console = ConsoleMode::resolve_stderr_default(
        None,
        std::env::var("GMEOW_CONSOLE").ok().as_deref(),
        std::io::stderr().is_terminal(),
    );
    let reporter: Box<dyn Reporter> = match console {
        ConsoleMode::Jsonl => Box::new(gmeow_cli_core::NdjsonReporter::new()),
        ConsoleMode::Silent => Box::new(SilentReporter),
        _ => Box::new(gmeow_cli_core::HumanReporter::new()),
    };

    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        emit_error(
            reporter.as_ref(),
            "gmeow-pipeline.perf-gate-merge.usage",
            "usage: perf-gate-merge <perf-dir>",
        );
        std::process::exit(2);
    }
    let perf_dir = PathBuf::from(&args[1]);

    std::process::exit(run(&perf_dir, reporter.as_ref()));
}

fn run(perf_dir: &Path, reporter: &dyn Reporter) -> i32 {
    use std::time::Instant;
    let started = Instant::now();
    reporter.stage_start("perf-gate-merge");

    let mut commands: Vec<serde_json::Value> = Vec::with_capacity(FILES.len());
    for file in FILES {
        let path = perf_dir.join(file);
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                emit_error(
                    reporter,
                    "gmeow-pipeline.perf-gate-merge.read",
                    format!("cannot read {}: {e}", path.display()),
                );
                return 1;
            }
        };
        let value: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                emit_error(
                    reporter,
                    "gmeow-pipeline.perf-gate-merge.parse",
                    format!("cannot parse {}: {e}", path.display()),
                );
                return 1;
            }
        };
        commands.push(value);
    }

    let output = serde_json::json!({ "commands": commands });
    let out_path = perf_dir.join("gate-timings.json");
    let text = match serde_json::to_string_pretty(&output) {
        Ok(s) => format!("{s}\n"),
        Err(e) => {
            emit_error(
                reporter,
                "gmeow-pipeline.perf-gate-merge.serialize",
                format!("cannot serialize output: {e}"),
            );
            return 1;
        }
    };
    if let Err(e) = std::fs::write(&out_path, text) {
        emit_error(
            reporter,
            "gmeow-pipeline.perf-gate-merge.write",
            format!("cannot write {}: {e}", out_path.display()),
        );
        return 1;
    }

    reporter.stage_end("perf-gate-merge", started.elapsed());
    println!("{}", out_path.display());
    0
}

#[derive(Debug, Default, Clone, Copy)]
struct SilentReporter;

impl Reporter for SilentReporter {
    fn report(&self, _report: &gmeow_errors::Report) {}
    fn stage_start(&self, _stage: &str) {}
    fn stage_end(&self, _stage: &str, _elapsed: Duration) {}
    fn summary(&self, _report: &gmeow_errors::Report) {}
}
