// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Merge per-command perf-gate timing JSONs into `gate-timings.json`.
//!
//! Replaces the inline Python one-liner in `make perf-gate`. Reads
//! `validate.json`, `sync.json`, and `reason-verify.json` from the
//! given directory and emits one versioned envelope. Deterministic work counters and
//! observational timings/cache-transfer evidence are projected into separate arrays so
//! a faster sample cannot masquerade as less work:
//!
//! ```json
//! {"schema_version": 1, "commands": [...], "deterministic_work": [...], "observations": [...]}
//! ```

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::Duration;

use gmeow_cli_core::{ConsoleMode, Reporter, report_diag};
use gmeow_errors::{Diag, FindingCategory, Grade, Severity, Standpoint};
use purrdf_lex::json::{self, Object, Value};

const FILES: [(&str, &str); 3] = [
    ("validate.json", "validate"),
    ("sync.json", "sync"),
    ("reason-verify.json", "reason-verify"),
];
const SCHEMA_VERSION: u32 = 1;

/// One complete command document plus the admitted projections it contributes.
struct Command {
    document: Value,
    label: String,
    work: Value,
    observations: Value,
}

#[derive(Debug)]
enum InputError {
    Parse(json::Error),
    Contract(String),
}

/// Read native last-member-wins JSON and admit only the fields this merger uses.
/// Inner payloads and input versions remain opaque; numeric tokens are not rounded.
fn read_command(raw: &[u8], expected: &str) -> Result<Command, InputError> {
    let document = json::record::read_document(raw).map_err(InputError::Parse)?;
    if document.as_object().is_none() {
        return Err(InputError::Contract(
            "document must be an object".to_owned(),
        ));
    }
    let label = document.get("command").and_then(Value::as_str);
    if label != Some(expected) {
        return Err(InputError::Contract(format!(
            "field command must be the string {expected:?}; found {:?}",
            document.get("command"),
        )));
    }
    let object = |field| {
        document
            .get(field)
            .filter(|value| value.as_object().is_some())
            .cloned()
            .ok_or_else(|| {
                InputError::Contract(format!(
                    "field {field} must be an object; found {:?}",
                    document.get(field),
                ))
            })
    };
    let work = object("deterministic_work")?;
    let observations = object("observations")?;
    Ok(Command {
        document,
        label: expected.to_owned(),
        work,
        observations,
    })
}

/// Assemble the actual product without collecting measurements or minting identities.
fn merged_json(commands: Vec<Command>) -> String {
    let work = commands
        .iter()
        .map(|command| {
            Value::from(
                Object::new()
                    .with("command", command.label.clone())
                    .with("work", command.work.clone()),
            )
        })
        .collect();
    let observations = commands
        .iter()
        .map(|command| {
            Value::from(
                Object::new()
                    .with("command", command.label.clone())
                    .with("observations", command.observations.clone()),
            )
        })
        .collect();
    let documents = commands
        .into_iter()
        .map(|command| command.document)
        .collect();
    let mut output = Value::from(
        Object::new()
            .with("schema_version", SCHEMA_VERSION)
            .with("command", "perf-gate")
            .with("deterministic_work", Value::Array(work))
            .with("observations", Value::Array(observations))
            .with("commands", Value::Array(documents)),
    );
    output.sort_keys();
    let mut text = json::write_pretty(&output);
    text.push('\n');
    text
}

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

    let mut commands = Vec::with_capacity(FILES.len());
    for (file, expected) in FILES {
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
        let value = match read_command(text.as_bytes(), expected) {
            Ok(v) => v,
            Err(InputError::Parse(e)) => {
                emit_error(
                    reporter,
                    "gmeow-pipeline.perf-gate-merge.parse",
                    format!("cannot parse {}: {e}", path.display()),
                );
                return 1;
            }
            Err(InputError::Contract(message)) => {
                emit_error(
                    reporter,
                    "gmeow-pipeline.perf-gate-merge.input",
                    format!("cannot admit {}: {message}", path.display()),
                );
                return 1;
            }
        };
        commands.push(value);
    }

    let out_path = perf_dir.join("gate-timings.json");
    let text = merged_json(commands);
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

#[cfg(test)]
#[path = "perf_gate_merge/json.tests.rs"]
mod json_tests;
