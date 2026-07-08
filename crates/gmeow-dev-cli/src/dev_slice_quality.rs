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
