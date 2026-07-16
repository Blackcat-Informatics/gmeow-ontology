// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Materialize the logic-conformance suite verdicts as a single canonical-JSON
//! artifact, for folding into the signed release bundle (§18).
//!
//! `make conformance` runs the corpus under `cargo-nextest` and *discards* the
//! per-case verdicts once the goldens match. The release-as-evidence lane needs
//! those verdicts to *ride into* the bundle as the attested "conformance
//! verdicts" frame, so this binary re-runs every discovered case through the
//! SAME native cores (`discover_cases` + `run_case`) and writes an aggregated,
//! deterministic report to `--out`.
//!
//! Determinism (§18): cases are keyed in a `BTreeMap` by `case_id` and the
//! report is serialized with `serde_json` (no `preserve_order` → sorted keys),
//! so the bytes are a pure function of the corpus — content-addressable when
//! folded. Nothing here samples a clock or the environment.

use std::collections::BTreeMap;
use std::path::PathBuf;

use gmeow_conformance::error::{CaseAnatomy, Cli, Io, Serialize};
use gmeow_conformance::{discover, paths, run};
use gmeow_errors::Diag;

/// Route a Transient progress witness (never gating) through the shared console
/// sink — human text on stderr, an NDJSON `finding` line for agents, dropped by a
/// silent sink. stdout is not used for product here, but the report artifact is
/// the product, so diagnostics default to the stderr surface.
fn note(message: impl Into<String>) {
    let mode = gmeow_cli_core::ConsoleMode::resolve_stderr_default(
        None,
        std::env::var("GMEOW_CONSOLE").ok().as_deref(),
        std::io::IsTerminal::is_terminal(&std::io::stderr()),
    );
    let reporter = gmeow_cli_core::reporter_for(mode);
    gmeow_cli_core::note(
        reporter.as_ref(),
        "conformance-report",
        "gmeow-conformance.report.note",
        message,
    );
}

fn main() -> gmeow_errors::Result<()> {
    let mut out: Option<PathBuf> = None;
    let mut cases_root: Option<PathBuf> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => {
                out = Some(PathBuf::from(args.next().ok_or_else(|| {
                    Diag::of_kind(Cli {
                        detail: "--out requires a path value".to_string(),
                    })
                })?));
            }
            "--cases-root" => {
                cases_root = Some(PathBuf::from(args.next().ok_or_else(|| {
                    Diag::of_kind(Cli {
                        detail: "--cases-root requires a path value".to_string(),
                    })
                })?));
            }
            other => {
                return Err(Diag::of_kind(Cli {
                    detail: format!("unknown argument: {other}"),
                }));
            }
        }
    }

    let out = out.ok_or_else(|| {
        Diag::of_kind(Cli {
            detail: "--out <path> is required".to_string(),
        })
    })?;
    let cases_root = cases_root.unwrap_or_else(paths::cases_root);

    let cases = discover::discover_cases(&cases_root)?;
    if cases.is_empty() {
        return Err(Diag::of_kind(CaseAnatomy {
            detail: format!(
                "no conformance cases discovered under {}",
                cases_root.display()
            ),
        }));
    }

    // BTreeMap keys → sorted, deterministic ordering independent of discovery.
    let mut by_case: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for case in &cases {
        // Lane routing: the release-as-evidence report is the Lane-A native
        // artifact. A Lane-B external corpus is oracle-backed and runs only in
        // `make -C validations/classic-cross-check validate`, so it is excluded here (it may also lie
        // outside the native fragment and would not be honestly decided).
        if matches!(
            gmeow_conformance::vendored::lane_for_case(&case.case_dir)?,
            Some(gmeow_conformance::vendored::Lane::B)
        ) {
            continue;
        }
        let outputs = run::run_case(&case.case_dir)?;
        by_case.insert(
            outputs.case_id.clone(),
            serde_json::json!({
                "verdicts": outputs.verdicts,
                "certification": outputs.certification,
                "budget": outputs.budget_status,
                "incomplete": outputs.incomplete,
            }),
        );
    }

    let report = serde_json::json!({
        "suite": "logic-conformance",
        "case_count": by_case.len(),
        "cases": by_case,
    });

    let json = serde_json::to_string_pretty(&report).map_err(|e| {
        Diag::of_kind(Serialize {
            detail: e.to_string(),
        })
    })? + "\n";
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            Diag::of_kind(Io {
                detail: format!("creating {}: {e}", parent.display()),
            })
        })?;
    }
    std::fs::write(&out, json).map_err(|e| {
        Diag::of_kind(Io {
            detail: format!("writing {}: {e}", out.display()),
        })
    })?;

    note(format!("{} case(s) → {}", by_case.len(), out.display()));
    Ok(())
}
