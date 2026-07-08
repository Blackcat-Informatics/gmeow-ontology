// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-dev slice-quality <path>` — the per-slice quality report + uplift advisor.
//!
//! Scores a slice against the ontology-resident rubric and emits a ranked,
//! deterministic advice list on the diagnostics substrate at `Standpoint::Advisory`.
//! The command itself never gates (it is advisory); the `make check` tier ratchet
//! is a separate gate. `--all` sweeps every slice.

use std::io::IsTerminal;
use std::path::Path;

use gmeow_cli_core::{ConsoleMode, DiagnosticsConfig};
use gmeow_errors::Report;
use gmeow_slice_quality::report::{SliceReport, score_slice, score_slice_with_rubric};

use crate::dev_common::{fail, project_root};
use crate::dev_feedback::{diagnostics_env, write_artifacts};

/// The output rendering the caller asked for.
#[derive(Clone, Copy)]
pub enum Format {
    /// Human-facing ranked text (default).
    Text,
    /// The diagnostics `Report` as JSON.
    Json,
    /// The diagnostics `Report` as SARIF.
    Sarif,
    /// The assessment graph as `gmeow:QualityAssessment` N-Quads.
    Rdf,
}

impl Format {
    fn parse(s: Option<&str>) -> Result<Self, String> {
        match s {
            None | Some("text") => Ok(Self::Text),
            Some("json") => Ok(Self::Json),
            Some("sarif") => Ok(Self::Sarif),
            Some("rdf") => Ok(Self::Rdf),
            Some(other) => Err(format!(
                "unknown --format {other} (want text|json|sarif|rdf)"
            )),
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
        Format::Rdf => Ok(report.to_gmeow_rdf()),
    }
}

/// Run the command. `path` is a slice directory; `all` sweeps every slice.
///
/// `format` controls the stdout rendering (the advisory human/JSON/SARIF/RDF
/// surface); the `--diagnostics-*` family controls first-class artifact emission
/// on the shared diagnostics rail, exactly as `feedback`/`external-tool` do. The
/// two compose: the stdout render is unchanged, and when `--diagnostics-artifacts`
/// names any of `{json,sarif,html}` the same-named projections of the advisory
/// report are written under the resolved directory.
#[allow(clippy::too_many_arguments)]
pub fn slice_quality(
    path: Option<&Path>,
    all: bool,
    format: Option<&str>,
    console: Option<ConsoleMode>,
    artifacts: Option<&str>,
    directory: Option<&Path>,
    stem: Option<&str>,
    category: Option<&str>,
) -> i32 {
    let format = match Format::parse(format) {
        Ok(f) => f,
        Err(e) => return fail(e),
    };
    let root = project_root();
    let config = match DiagnosticsConfig::resolve(
        console.map(ConsoleMode::as_str),
        artifacts,
        directory,
        stem,
        category,
        &diagnostics_env(),
        std::io::stderr().is_terminal(),
        &root.join("dist"),
    ) {
        Ok(c) => c,
        Err(e) => return fail(e.to_string()),
    };

    if all {
        return sweep(&root, format, &config);
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
            let mut diag = report.to_report();
            diag.metadata
                .insert("category".into(), serde_json::json!(config.category));
            if let Err(code) = write_artifacts(&diag, &config) {
                return code;
            }
            0 // advisory — the command never gates
        }
        Err(e) => fail(format!("slice-quality: {e}")),
    }
}

/// Score every discovered slice against one loaded rubric and print a roll-up
/// summary.
///
/// This CLI surface is the human-facing roll-up printer; it does NOT fold anything into
/// `gmeow.gts`. The carrier attach of the `gmeow:QualityAssessment` graph is done by the
/// regeneration pipeline (`stage-source-load` scores every slice via
/// [`gmeow_slice_quality::assessment_nquads`] and attaches the result under the
/// `graph/quality-assessment` named graph, projected on-disk to
/// `generated/quality/gmeow.quality-assessment.nt`). Both surfaces score the SAME slice
/// set through [`gmeow_slice_quality::discover_slice_dirs`], so the printed roll-up and
/// the shipped graph never diverge.
fn sweep(root: &Path, format: Format, config: &DiagnosticsConfig) -> i32 {
    let dirs = gmeow_slice_quality::discover_slice_dirs(&root.join("slices"));
    let rubric = match gmeow_slice_quality::load_repo_rubric(root) {
        Ok(r) => r,
        Err(e) => return fail(format!("slice-quality: {e}")),
    };
    let mut printed = 0usize;
    // The aggregate diagnostics report: every scored slice's advisory findings +
    // help-URI rules folded into one report, projected to the requested artifacts.
    let mut aggregate = Report::new("slice-quality");
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
                    Format::Json | Format::Sarif | Format::Rdf => match render(&report, format) {
                        Ok(t) => println!("{t}"),
                        Err(e) => return fail(e),
                    },
                }
                if !config.artifacts.is_empty() {
                    let diag = report.to_report();
                    for finding in diag.findings {
                        aggregate.add_finding(finding);
                    }
                    for rule in diag.rules {
                        aggregate.add_rule(rule);
                    }
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
    aggregate
        .metadata
        .insert("category".into(), serde_json::json!(config.category));
    if let Err(code) = write_artifacts(&aggregate, config) {
        return code;
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

    // Axis→producer binding gate, projection completeness gate, and exemption
    // staleness gate — all reported together before the per-slice ratchet.
    let mut structural: Vec<String> = Vec::new();
    structural.extend(gmeow_slice_quality::gate::binding_gate(&rubric));
    structural.extend(gmeow_slice_quality::gate::completeness_gate(&rubric));
    structural.extend(gmeow_slice_quality::gate::stale_exemptions(
        &rubric,
        |symbol| symbol_resolves_in_repo(&root, symbol),
    ));
    if !structural.is_empty() {
        for e in &structural {
            eprintln!("FAIL {e}");
        }
        return fail(format!(
            "slice-quality-gate: {} rubric structural failure(s)",
            structural.len()
        ));
    }

    // Floor ranks by slice IRI, resolved against the ladder.
    let floors = load_floors(&root, &rubric);

    let dirs = gmeow_slice_quality::discover_slice_dirs(&root.join("slices"));
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

/// Whether `symbol` is defined as a Rust item anywhere under `crates/` — the
/// staleness-gate resolver. A definition keyword immediately followed by the
/// symbol name counts (the constitution-gate integrity style), so a mere mention
/// in a comment or string does not falsely retire an exemption.
fn symbol_resolves_in_repo(root: &Path, symbol: &str) -> bool {
    let keywords = [
        "struct", "enum", "fn", "trait", "type", "const", "static", "union",
    ];
    let needles: Vec<String> = keywords.iter().map(|k| format!("{k} {symbol}")).collect();
    let mut found = false;
    scan_rs(&root.join("crates"), &mut |text| {
        if needles.iter().any(|n| text.contains(n.as_str())) {
            found = true;
        }
    });
    found
}

/// Walk `.rs` files under `dir`, calling `f` with each file's text.
fn scan_rs(dir: &Path, f: &mut impl FnMut(&str)) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            scan_rs(&p, f);
        } else if p.extension().is_some_and(|x| x == "rs")
            && let Ok(text) = std::fs::read_to_string(&p)
        {
            f(&text);
        }
    }
}
