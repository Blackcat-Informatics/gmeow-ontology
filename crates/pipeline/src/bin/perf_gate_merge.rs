// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Merge per-command perf-gate timing JSONs into `gate-timings.json`.
//!
//! Replaces the inline Python one-liner in `make perf-gate`. Reads
//! `validate.json`, `check-generated.json`, and `reason-verify.json` from the
//! given directory and emits a single JSON object with the identical schema:
//!
//! ```json
//! {"commands": [<validate>, <check-generated>, <reason-verify>]}
//! ```

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::Duration;

use gmeow_cli_core::{ConsoleMode, Reporter};

const FILES: [&str; 3] = [
    "validate.json",
    "check-generated.json",
    "reason-verify.json",
];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: perf-gate-merge <perf-dir>");
        std::process::exit(2);
    }
    let perf_dir = PathBuf::from(&args[1]);
    let console = ConsoleMode::resolve(
        None,
        std::env::var("GMEOW_CONSOLE").ok().as_deref(),
        std::io::stderr().is_terminal(),
    );
    let reporter: Box<dyn Reporter> = match console {
        ConsoleMode::Jsonl => Box::new(gmeow_cli_core::NdjsonReporter::new()),
        ConsoleMode::Silent => Box::new(SilentReporter),
        _ => Box::new(gmeow_cli_core::HumanReporter::new()),
    };

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
                eprintln!("perf-gate-merge: cannot read {}: {e}", path.display());
                return 1;
            }
        };
        let value: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("perf-gate-merge: cannot parse {}: {e}", path.display());
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
            eprintln!("perf-gate-merge: cannot serialize output: {e}");
            return 1;
        }
    };
    if let Err(e) = std::fs::write(&out_path, text) {
        eprintln!("perf-gate-merge: cannot write {}: {e}", out_path.display());
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
