// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-dev slice-quality <path>` — the per-slice quality report + uplift advisor.
//!
//! Scores a slice against the ontology-resident rubric and emits a ranked,
//! deterministic advice list on the diagnostics substrate at `Standpoint::Advisory`.
//! The command itself never gates (it is advisory); the `make check` tier ratchet
//! is a separate gate. `--all` sweeps every slice.

use std::path::{Path, PathBuf};

use gmeow_slice_quality::report::{SliceReport, score_slice, score_slice_with_rubric};

use crate::dev_common::{fail, project_root};

/// The output rendering the caller asked for.
#[derive(Clone, Copy)]
pub enum Format {
    /// Human-facing ranked text (default).
    Text,
    /// The diagnostics `Report` as JSON.
    Json,
    /// The diagnostics `Report` as SARIF.
    Sarif,
}

impl Format {
    fn parse(s: Option<&str>) -> Result<Self, String> {
        match s {
            None | Some("text") => Ok(Self::Text),
            Some("json") => Ok(Self::Json),
            Some("sarif") => Ok(Self::Sarif),
            Some(other) => Err(format!("unknown --format {other} (want text|json|sarif)")),
        }
    }
}

fn render(report: &SliceReport, format: Format) -> Result<String, String> {
    match format {
        Format::Text => Ok(report.render_text()),
        Format::Json => {
            gmeow_errors::render::to_json(&report.to_report()).map_err(|e| e.to_string())
        }
        Format::Sarif => {
            gmeow_errors::render::to_sarif(&report.to_report()).map_err(|e| e.to_string())
        }
    }
}

/// Run the command. `path` is a slice directory; `all` sweeps every slice.
pub fn slice_quality(path: Option<&Path>, all: bool, format: Option<&str>) -> i32 {
    let format = match Format::parse(format) {
        Ok(f) => f,
        Err(e) => return fail(e),
    };
    let root = project_root();

    if all {
        return sweep(&root, format);
    }

    let Some(dir) = path else {
        return fail("slice-quality: a slice path is required (or pass --all)");
    };
    match score_slice(&root, dir) {
        Ok(report) => {
            match render(&report, format) {
                Ok(text) => print!("{text}"),
                Err(e) => return fail(e),
            }
            0 // advisory — the command never gates
        }
        Err(e) => fail(format!("slice-quality: {e}")),
    }
}

/// Score every discovered slice against one loaded rubric and print a roll-up
/// summary. (Attaching the assessment graph to the carrier for `gmeow.gts` is the
/// pipeline sweep; this CLI surface prints the per-slice roll-up.)
fn sweep(root: &Path, format: Format) -> i32 {
    let slices = root.join("slices");
    let mut dirs = discover_slice_dirs(&slices);
    dirs.sort();
    let rubric = match gmeow_slice_quality::load_repo_rubric(root) {
        Ok(r) => r,
        Err(e) => return fail(format!("slice-quality: {e}")),
    };
    let mut printed = 0usize;
    for dir in &dirs {
        match score_slice_with_rubric(dir, rubric.clone()) {
            Ok(report) => {
                match format {
                    Format::Text => {
                        println!(
                            "{}\t{}\t{} advice",
                            report.assessment.slice,
                            report.rollup_label(),
                            report.advisories.len()
                        );
                    }
                    Format::Json | Format::Sarif => match render(&report, format) {
                        Ok(t) => println!("{t}"),
                        Err(e) => return fail(e),
                    },
                }
                printed += 1;
            }
            // A slice that cannot be scored is reported, not silently skipped.
            Err(e) => eprintln!("slice-quality: {}: {e}", dir.display()),
        }
    }
    if printed == 0 {
        return fail("slice-quality: no slices scored");
    }
    0
}

/// The committed ratchet-floor artifact: `<slice-iri>\t<tier-local>` per line.
/// Absent slices have no floor (their first declaration sets it in review).
const FLOOR_FILE: &str = "governance/slice-quality-floors.tsv";

/// The `make check` opt-in tier ratchet gate.
///
/// For every slice that declares `gmeow:sliceQualityTier`: the measured roll-up
/// must be ≥ the declared tier, and the declared tier must be ≥ the committed
/// floor. Undeclared slices are advisory and never fail. Exit 1 on any failure.
pub fn slice_quality_gate() -> i32 {
    let root = project_root();
    let rubric = match gmeow_slice_quality::load_repo_rubric(&root) {
        Ok(r) => r,
        Err(e) => return fail(format!("slice-quality-gate: {e}")),
    };
    // Floor ranks by slice IRI, resolved against the ladder.
    let floors = load_floors(&root, &rubric);

    let mut dirs = discover_slice_dirs(&root.join("slices"));
    dirs.sort();
    let mut failures = 0usize;
    let mut checked = 0usize;
    for dir in &dirs {
        let declared = match gmeow_slice_quality::gate::declared_tier(dir, &rubric) {
            Ok(d) => d,
            Err(e) => return fail(format!("slice-quality-gate: {e}")),
        };
        let Some(declared) = declared else { continue }; // undeclared → advisory
        checked += 1;
        let report = match score_slice_with_rubric(dir, rubric.clone()) {
            Ok(r) => r,
            Err(e) => return fail(format!("slice-quality-gate: {}: {e}", dir.display())),
        };
        let measured_rank = report.assessment.rollup.rank;
        let floor_rank = floors.get(&report.assessment.slice).copied();
        let verdict = gmeow_slice_quality::gate::evaluate_ratchet(
            Some(declared.rank),
            measured_rank,
            floor_rank,
        );
        use gmeow_slice_quality::gate::RatchetVerdict;
        match verdict {
            RatchetVerdict::Pass => {
                println!(
                    "ok   {} declared {} measured {}",
                    report.assessment.slice, declared.label, report.assessment.rollup.label
                );
            }
            RatchetVerdict::MeasuredBelowDeclared => {
                eprintln!(
                    "FAIL {} declared {} but measures {} — uplift the slice or lower is forbidden",
                    report.assessment.slice, declared.label, report.assessment.rollup.label
                );
                failures += 1;
            }
            RatchetVerdict::DeclaredBelowFloor => {
                eprintln!(
                    "FAIL {} declares {} below its committed ratchet floor — the tier may only be raised",
                    report.assessment.slice, declared.label
                );
                failures += 1;
            }
        }
    }
    if failures > 0 {
        return fail(format!(
            "slice-quality-gate: {failures} of {checked} opted-in slice(s) below their declared tier"
        ));
    }
    println!("slice-quality-gate: {checked} opted-in slice(s) hold their declared tier");
    0
}

/// Load the committed floor ranks keyed by slice IRI.
fn load_floors(
    root: &Path,
    rubric: &gmeow_slice_quality::Rubric,
) -> std::collections::HashMap<String, i64> {
    let mut out = std::collections::HashMap::new();
    let Ok(text) = std::fs::read_to_string(root.join(FLOOR_FILE)) else {
        return out;
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((iri, tier_local)) = line.split_once('\t') {
            let tier_iri = format!("{}{}", gmeow_slice_quality::model::GMEOW, tier_local.trim());
            if let Some(tier) = rubric.tier(&tier_iri) {
                out.insert(iri.trim().to_owned(), tier.rank);
            }
        }
    }
    out
}

/// Every directory under `slices/` that holds a `manifest.ttl`.
fn discover_slice_dirs(slices: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(slices, &mut out);
    out
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.join("manifest.ttl").is_file() {
                out.push(p.clone());
            }
            walk(&p, out);
        }
    }
}
