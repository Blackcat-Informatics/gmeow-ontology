// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `bench-compare`: the report-only perf regression scoreboard and the
//! single `bench/baseline.json` producer.
//!
//! Two modes read a live Criterion run under `target/criterion`; comparison also
//! requires committed `bench/baseline.json`. Paths are relative to the current
//! directory (the workspace root when driven by `make`):
//!
//!   * default — print the regression scoreboard (live vs baseline) to stdout and
//!     exit 0 for valid measurements, including regressions. Invalid required
//!     inputs fail explicitly; runner jitter remains advisory (Principle 18).
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
            Err(error) => {
                reporter.report(&report_diag(
                    error.with_context("bench-compare --emit-baseline"),
                    TOOL,
                ));
                ExitCode::FAILURE
            }
        }
    } else {
        let baseline = match std::fs::read_to_string(bench::BASELINE_PATH) {
            Ok(baseline) => baseline,
            Err(error) => {
                reporter.report(&report_diag(
                    gmeow_errors::Diag::from(error).with_context(format!(
                        "read required timing baseline {}",
                        bench::BASELINE_PATH,
                    )),
                    TOOL,
                ));
                return ExitCode::FAILURE;
            }
        };
        match bench::compare_against_baseline(criterion_root, &baseline) {
            Ok(report) => {
                print!("{report}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                reporter.report(&report_diag(
                    error.with_context("bench-compare timing comparison"),
                    TOOL,
                ));
                ExitCode::FAILURE
            }
        }
    }
}
