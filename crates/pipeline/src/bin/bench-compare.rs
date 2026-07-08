// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `bench-compare`: the report-only perf regression scoreboard and the
//! single `bench/baseline.json` producer.
//!
//! Two modes, both reading a live criterion run under `target/criterion` and the
//! committed `bench/baseline.json` relative to the current directory (the
//! workspace root when driven by `make`):
//!
//!   * default — print the regression scoreboard (live vs baseline) to stdout and
//!     ALWAYS exit 0. This is advisory: it runs only in the off-gate
//!     `suite-quality` lane and never fails a PR (Principle 18).
//!   * `--emit-baseline` — flatten the live criterion run into the committed
//!     baseline JSON (integer nanoseconds, sorted keys) on stdout. This is the
//!     ONLY producer of `bench/baseline.json`; `make maint-bench-baseline` wires
//!     it. A flatten failure exits non-zero (the maintainer must see it).
//!
//! It subsumes the retired `scripts/bench_to_json.py` — same criterion-tree shape,
//! now in Rust with an explicit integer-ns contract.

use std::io::IsTerminal;
use std::path::Path;
use std::process::ExitCode;

use gmeow_cli_core::{ConsoleMode, Reporter, report_diag};
use gmeow_errors::{Diag, FindingCategory, Grade, Severity, Standpoint};
use gmeow_pipeline::stages::bench;

/// The emitting tool name every diagnostic here is stamped with.
const TOOL: &str = "bench-compare";

/// A boxed reporter for this bin. stdout carries product data (the emitted
/// baseline JSON / the scoreboard), so diagnostics default to the HUMAN stderr
/// surface and never interleave NDJSON into a pipe; an agent opts into the
/// machine surface with `GMEOW_CONSOLE=jsonl`.
fn reporter() -> Box<dyn Reporter> {
    let mode = ConsoleMode::resolve_stderr_default(
        None,
        std::env::var("GMEOW_CONSOLE").ok().as_deref(),
        std::io::stderr().is_terminal(),
    );
    gmeow_cli_core::reporter_for(mode)
}

fn main() -> ExitCode {
    let criterion_root = Path::new("target/criterion");
    let emit = std::env::args().any(|a| a == "--emit-baseline");
    let reporter = reporter();

    if emit {
        match bench::emit_baseline(criterion_root) {
            Ok(json) => {
                print!("{json}");
                ExitCode::SUCCESS
            }
            Err(err) => {
                let diag = Diag::new(
                    gmeow_errors::code::register_code("gmeow-pipeline.bench-compare.emit-baseline"),
                    Grade::new(
                        Severity::Error,
                        FindingCategory::ModelingDisciplineViolation,
                        Standpoint::Binding,
                    ),
                    format!("--emit-baseline failed: {err}"),
                );
                reporter.report(&report_diag(diag, TOOL));
                ExitCode::FAILURE
            }
        }
    } else {
        // Report-only: a missing baseline degrades silently to an all-`new`
        // board (the legitimate first-run case). A baseline that is PRESENT but
        // unreadable is a real degradation — surface it (don't swallow it),
        // then still degrade. The scoreboard NEVER gates — always exit 0.
        let baseline = match std::fs::read_to_string(bench::BASELINE_PATH) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => {
                gmeow_cli_core::note(
                    reporter.as_ref(),
                    TOOL,
                    "gmeow-pipeline.bench-compare.note",
                    format!(
                        "committed `{}` is present but unreadable ({e}); \
treating every benchmark as `new`.",
                        bench::BASELINE_PATH
                    ),
                );
                String::new()
            }
        };
        print!(
            "{}",
            bench::compare_against_baseline(criterion_root, &baseline)
        );
        ExitCode::SUCCESS
    }
}
