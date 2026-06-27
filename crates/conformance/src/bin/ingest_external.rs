// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `ingest-external` — the external-corpus ingestion CLI (#753).
//!
//! Concrete proof of AC1 ("the runner ingests a W3C `manifest.ttl` AND a TPTP SZS
//! problem → produces a runner verdict") and the reproducible refresh entry point
//! the vendoring procedure (X2–X5) follows.
//!
//! Usage:
//!
//! ```text
//! ingest-external --szs <problem.p> [--world <iri> --quads <n>]
//!     Parse a TPTP SZS problem and print the runner verdict. With --world/--quads,
//!     print the full world-indexed verdicts.json value; otherwise print the bare
//!     status (consistent | inconsistent | incomplete).
//!
//! ingest-external --manifest <manifest.ttl>
//!     Parse a W3C entailment manifest and print one `<name>\t<status>` line per
//!     mf:PositiveEntailment / mf:NegativeEntailment entry.
//! ```

use std::path::PathBuf;

use gmeow_conformance::external::{
    outcome_from_szs, parse_entailment_manifest, runner_verdict_json,
};

const USAGE: &str = "\
usage:
  ingest-external --szs <problem.p> [--world <iri> --quads <n>]
  ingest-external --manifest <manifest.ttl>";

fn main() -> Result<(), String> {
    let mut szs: Option<PathBuf> = None;
    let mut manifest: Option<PathBuf> = None;
    let mut world: Option<String> = None;
    let mut quads: Option<u64> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--szs" => szs = Some(PathBuf::from(next(&mut args, "--szs")?)),
            "--manifest" => manifest = Some(PathBuf::from(next(&mut args, "--manifest")?)),
            "--world" => world = Some(next(&mut args, "--world")?),
            "--quads" => {
                quads = Some(
                    next(&mut args, "--quads")?
                        .parse()
                        .map_err(|e| format!("--quads must be a non-negative integer: {e}"))?,
                )
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => return Err(format!("unknown argument: {other}\n{USAGE}")),
        }
    }

    match (szs, manifest) {
        (Some(path), None) => ingest_szs(&path, world.as_deref(), quads),
        (None, Some(path)) => ingest_manifest(&path),
        (Some(_), Some(_)) => Err(format!(
            "--szs and --manifest are mutually exclusive\n{USAGE}"
        )),
        (None, None) => Err(format!("one of --szs / --manifest is required\n{USAGE}")),
    }
}

/// Ingest a TPTP SZS problem → runner verdict.
fn ingest_szs(
    path: &std::path::Path,
    world: Option<&str>,
    quads: Option<u64>,
) -> Result<(), String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let outcome = outcome_from_szs(&text)?;
    match (world, quads) {
        (Some(world), Some(quads)) => {
            let verdict = runner_verdict_json(world, quads, outcome);
            println!(
                "{}",
                serde_json::to_string_pretty(&verdict)
                    .map_err(|e| format!("serialize verdict: {e}"))?
            );
        }
        (None, None) => println!("{}", outcome.verdict_status().as_str()),
        _ => return Err("--world and --quads must be given together".to_string()),
    }
    Ok(())
}

/// Ingest a W3C entailment manifest → one `<name>\t<status>` line per entry.
fn ingest_manifest(path: &std::path::Path) -> Result<(), String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let base = format!("file://{}", path.display());
    let entries = parse_entailment_manifest(&text, Some(&base))?;
    if entries.is_empty() {
        return Err(format!("no entailment entries in {}", path.display()));
    }
    for entry in entries {
        println!(
            "{}\t{}",
            entry.name,
            entry.outcome().verdict_status().as_str()
        );
    }
    Ok(())
}

/// Read the value following a flag, or error with the flag name.
fn next(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}
