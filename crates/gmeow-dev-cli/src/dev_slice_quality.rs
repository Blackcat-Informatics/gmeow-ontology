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
use gmeow_slice_quality::model::{Rubric, Tier};
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
///
/// `min_tier` is the G11 gate: when `None` the command is advisory and always
/// exits 0 (today's behavior); when `Some(tier)` the measured roll-up is compared
/// against the named tier using the rubric ladder's total order, and the command
/// exits non-zero if the slice measures below it (naming measured vs required).
/// With `--all` this gates the whole sweep — it fails if ANY swept slice is below
/// the required tier, naming every failing slice — which is more useful than a
/// single-slice-only gate.
#[allow(clippy::too_many_arguments)]
pub fn slice_quality(
    path: Option<&Path>,
    all: bool,
    format: Option<&str>,
    min_tier: Option<&str>,
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
        return sweep(&root, format, min_tier, &config);
    }

    let Some(dir) = path else {
        return fail("slice-quality: a slice path is required (or pass --all)");
    };
    // Resolve the slice path against the repo root (consistent with `--all` and the
    // MCP tool), so a relative `slices/<group>/<name>` is not accidentally read
    // against the caller's CWD. An absolute path is left untouched by `join`.
    let dir = root.join(dir);
    match score_slice(&root, &dir) {
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
            // G11 gate: render/emit above always happen; the gate only decides the
            // exit code. Unset (`min_tier == None`) preserves the advisory exit 0.
            let Some(required) = min_tier else {
                return 0; // advisory — the command never gates without --min-tier
            };
            let required = match resolve_min_tier(&report.rubric, required) {
                Ok(t) => t,
                Err(e) => return fail(e),
            };
            let measured = &report.assessment.rollup;
            if !tier_gate_passes(measured, Some(required)) {
                return fail(format!(
                    "slice-quality: {} measures {} but --min-tier requires {} — below the required tier",
                    report.assessment.slice, measured.label, required.label
                ));
            }
            0
        }
        Err(e) => fail(format!("slice-quality: {e}")),
    }
}

/// The G11 gate decision for one slice: does `measured` satisfy the `--min-tier`
/// bar? `required == None` is the advisory case (always passes / exit 0); otherwise
/// the ladder's total order (`Tier::sort_key`) decides, so measured must be at or
/// above the required tier. This is the single source of truth for both the
/// single-slice and `--all` sweep gates.
#[must_use]
fn tier_gate_passes(measured: &Tier, required: Option<&Tier>) -> bool {
    match required {
        None => true,
        Some(req) => measured.sort_key() >= req.sort_key(),
    }
}

/// Resolve a `--min-tier` argument against the rubric ladder, accepting either a
/// tier's human label (`Grounded`) or its IRI local name (`tierGrounded`),
/// case-insensitively. Returns a clear error naming the available rungs on an
/// unknown tier — a HARD FAIL, never a silently-ignored gate request.
fn resolve_min_tier<'a>(rubric: &'a Rubric, name: &str) -> Result<&'a Tier, String> {
    let local_of =
        |iri: &str| -> String { iri.rsplit(['/', '#']).next().unwrap_or(iri).to_owned() };
    if let Some(t) = rubric
        .tiers
        .iter()
        .find(|t| t.label.eq_ignore_ascii_case(name) || local_of(&t.iri).eq_ignore_ascii_case(name))
    {
        return Ok(t);
    }
    let mut rungs: Vec<&Tier> = rubric.tiers.iter().collect();
    rungs.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
    let known: Vec<String> = rungs.iter().map(|t| t.label.clone()).collect();
    Err(format!(
        "slice-quality: unknown --min-tier {name:?} (want one of: {})",
        known.join(", ")
    ))
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
fn sweep(root: &Path, format: Format, min_tier: Option<&str>, config: &DiagnosticsConfig) -> i32 {
    let dirs = gmeow_slice_quality::discover_slice_dirs(&root.join("slices"));
    let rubric = match gmeow_slice_quality::load_repo_rubric(root) {
        Ok(r) => r,
        Err(e) => return fail(format!("slice-quality: {e}")),
    };
    // G11 sweep gate: resolve the required tier once, up front, so an unknown tier
    // is a clear error before any slice is scored. Gating the sweep (fail if ANY
    // slice is below) is the more useful choice than conflicting with --all.
    let required = match min_tier {
        Some(name) => match resolve_min_tier(&rubric, name) {
            Ok(t) => Some(t.clone()),
            Err(e) => return fail(e),
        },
        None => None,
    };
    // Slices measuring below the required tier, collected to name every failure.
    let mut below: Vec<(String, String)> = Vec::new();
    let mut printed = 0usize;
    // The aggregate diagnostics report: every scored slice's advisory findings +
    // help-URI rules folded into ONE report. It is both the single stdout document
    // for the structured (json/sarif) formats and the source of the written
    // artifacts — so `--all --format json|sarif` emits ONE parseable document, not a
    // JSON-Lines stream of one object per slice.
    let mut aggregate = Report::new("slice-quality");
    // The RDF projection concatenates validly (deterministic N-Quads, one graph), so
    // it is streamed into a single buffer and printed once.
    let mut rdf_out = String::new();
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
                    Format::Rdf => rdf_out.push_str(&report.to_gmeow_rdf()),
                    // Json/Sarif are emitted once, after the loop, from `aggregate`.
                    Format::Json | Format::Sarif => {}
                }
                // Always fold each slice's diagnostics into the aggregate: it backs
                // both the single structured stdout document and the artifacts.
                let diag = report.to_report();
                for finding in diag.findings {
                    aggregate.add_finding(finding);
                }
                for rule in diag.rules {
                    aggregate.add_rule(rule);
                }
                // Record slices below the --min-tier bar (measured < required).
                let measured = &report.assessment.rollup;
                if !tier_gate_passes(measured, required.as_ref()) {
                    below.push((report.assessment.slice.clone(), measured.label.clone()));
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
    // Emit the structured formats as a SINGLE parseable artifact.
    match format {
        Format::Json => match gmeow_errors::render::to_json(&aggregate) {
            Ok(t) => println!("{t}"),
            Err(e) => return fail(e.to_string()),
        },
        Format::Sarif => match gmeow_errors::render::to_sarif(&aggregate) {
            Ok(t) => println!("{t}"),
            Err(e) => return fail(e.to_string()),
        },
        Format::Rdf => print!("{rdf_out}"),
        Format::Text => {}
    }
    aggregate
        .metadata
        .insert("category".into(), serde_json::json!(config.category));
    if let Err(code) = write_artifacts(&aggregate, config) {
        return code;
    }
    // G11 sweep gate: render/emit above always happen; only now does the exit code
    // reflect the tier bar. Name every failing slice so the failure is actionable.
    if let Some(required) = &required
        && !below.is_empty()
    {
        for (slice, measured) in &below {
            eprintln!(
                "FAIL {slice} measures {measured} — below --min-tier {}",
                required.label
            );
        }
        return fail(format!(
            "slice-quality: {} slice(s) below --min-tier {}",
            below.len(),
            required.label
        ));
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
    // staleness gate — all reported together before the per-slice ratchet. The
    // full set of Rust item definitions under `crates/` is resolved by ONE walk
    // (`resolvable_symbols`) and reused as the resolver for both the binding gate
    // (each axis producer must be a real primitive item) and the staleness gate
    // (an exemption whose producer has landed is stale) — never re-scanned per
    // symbol.
    let symbols = resolvable_symbols(&root);
    let mut structural: Vec<String> = Vec::new();
    structural.extend(gmeow_slice_quality::gate::binding_gate(&rubric, |symbol| {
        symbols.contains(symbol)
    }));
    structural.extend(gmeow_slice_quality::gate::completeness_gate(&rubric));
    structural.extend(gmeow_slice_quality::gate::stale_exemptions(
        &rubric,
        |symbol| symbols.contains(symbol),
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

    // Floor ranks by slice IRI, resolved against the ladder. The gate cannot
    // enforce a ratchet it cannot read: a missing or malformed floors file is a
    // HARD FAIL here (.goals no-optionality), never a silently-disabled floor.
    let floors = match load_floors(&root, &rubric) {
        Ok(f) => f,
        Err(e) => return fail(format!("slice-quality-gate: {e}")),
    };

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
///
/// # Errors
/// A HARD FAIL (.goals no-optionality) when the gate cannot read the ratchet it
/// must enforce: the floors file is missing/unreadable, a non-comment line is not
/// a `<slice-iri>\t<tier-local-name>` pair, or a tier label names no
/// `gmeow:QualityTier` in the loaded ladder. Never a silently-disabled floor or a
/// silently-skipped line — the error names the file, the 1-based line, and the
/// offending label.
fn load_floors(
    root: &Path,
    rubric: &gmeow_slice_quality::Rubric,
) -> Result<std::collections::HashMap<String, i64>, String> {
    let path = root.join(FLOOR_FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "cannot read ratchet floor file {} (the gate cannot enforce a floor it cannot read): {e}",
            path.display()
        )
    })?;
    let mut out = std::collections::HashMap::new();
    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let lineno = idx + 1;
        let Some((iri, tier_local)) = line.split_once('\t') else {
            return Err(format!(
                "{}:{lineno}: malformed floor line (want <slice-iri>\\t<tier-local-name>): {raw:?}",
                path.display()
            ));
        };
        let tier_local = tier_local.trim();
        let tier_iri = format!("{}{}", gmeow_slice_quality::model::GMEOW, tier_local);
        let Some(tier) = rubric.tier(&tier_iri) else {
            return Err(format!(
                "{}:{lineno}: unknown tier label {tier_local:?} (names no gmeow:QualityTier in the rubric ladder)",
                path.display()
            ));
        };
        out.insert(iri.trim().to_owned(), tier.rank);
    }
    Ok(out)
}

/// The set of every Rust *item* name defined anywhere under `crates/` — built by a
/// SINGLE walk that feeds each `.rs` file through the constitution-gate AST resolver
/// [`gmeow_validate::constitution::rust_item_names`] and unions the results. That
/// resolver comment/string-strips the source and collects only the identifier
/// immediately following an item-introducer keyword (`fn`/`struct`/`enum`/…), so the
/// set is identifier-boundary-correct: a symbol that is a strict *prefix* of a real
/// item (`grounding_ax` vs `grounding_axis`), or that appears only in a comment or
/// string, is NOT present.
///
/// The same set is reused across every axis producer (binding gate) and every
/// exemption producer (staleness gate), so `crates/` is walked exactly once rather
/// than once per symbol.
fn resolvable_symbols(root: &Path) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    scan_rs(&root.join("crates"), &mut |text| {
        names.extend(gmeow_validate::constitution::rust_item_names(text));
    });
    names
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

#[cfg(test)]
mod min_tier_tests {
    use super::*;

    /// A five-rung ladder mirroring the shipped rubric (Registered..Maximal).
    fn ladder() -> Rubric {
        let rung = |local: &str, label: &str, rank: i64| Tier {
            iri: format!("{}{local}", gmeow_slice_quality::model::GMEOW),
            label: label.to_owned(),
            rank,
        };
        Rubric {
            tiers: vec![
                rung("tierRegistered", "Registered", 0),
                rung("tierGrounded", "Grounded", 1),
                rung("tierLinked", "Linked", 2),
                rung("tierExemplified", "Exemplified", 3),
                rung("tierMaximal", "Maximal", 4),
            ],
            axes: vec![],
            exemptions: vec![],
        }
    }

    fn tier(r: &Rubric, label: &str) -> Tier {
        resolve_min_tier(r, label).unwrap().clone()
    }

    #[test]
    fn gate_below_required_fails() {
        // Measured Grounded(1) vs required Maximal(4) → below the bar, gate fails.
        let r = ladder();
        let measured = tier(&r, "Grounded");
        let required = tier(&r, "Maximal");
        assert!(
            !tier_gate_passes(&measured, Some(&required)),
            "measured below required must not pass"
        );
    }

    #[test]
    fn gate_at_or_above_required_passes() {
        let r = ladder();
        let required = tier(&r, "Linked");
        // Exactly at the bar passes.
        assert!(tier_gate_passes(&tier(&r, "Linked"), Some(&required)));
        // Above the bar passes.
        assert!(tier_gate_passes(&tier(&r, "Maximal"), Some(&required)));
    }

    #[test]
    fn gate_unset_is_advisory_pass() {
        // --min-tier unset → advisory, always passes even at the floor tier.
        let r = ladder();
        assert!(tier_gate_passes(&tier(&r, "Registered"), None));
    }

    #[test]
    fn resolve_accepts_label_and_local_case_insensitively() {
        let r = ladder();
        assert_eq!(resolve_min_tier(&r, "Grounded").unwrap().rank, 1);
        assert_eq!(resolve_min_tier(&r, "grounded").unwrap().rank, 1);
        // The IRI local name is also accepted.
        assert_eq!(resolve_min_tier(&r, "tierMaximal").unwrap().rank, 4);
    }

    #[test]
    fn resolve_unknown_tier_errors_naming_the_rungs() {
        let r = ladder();
        let err = resolve_min_tier(&r, "Platinum").unwrap_err();
        assert!(err.contains("unknown --min-tier"), "{err}");
        assert!(err.contains("Platinum"), "names the bad input: {err}");
        // Lists the available rungs, ladder-ordered.
        assert!(
            err.contains("Registered, Grounded, Linked, Exemplified, Maximal"),
            "lists rungs: {err}"
        );
    }
}

#[cfg(test)]
mod load_floors_tests {
    use super::*;

    /// A minimal one-rung ladder so `load_floors` can resolve `tierRegistered`.
    fn registered_rubric() -> gmeow_slice_quality::Rubric {
        let mut r = gmeow_slice_quality::Rubric::default();
        r.tiers.push(gmeow_slice_quality::Tier {
            iri: format!("{}tierRegistered", gmeow_slice_quality::model::GMEOW),
            label: "Registered".to_owned(),
            rank: 0,
        });
        r
    }

    /// A throwaway repo root with an empty `governance/` directory.
    fn temp_root(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut p = std::env::temp_dir();
        p.push(format!(
            "gmeow-floors-{name}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(p.join("governance")).unwrap();
        p
    }

    fn write_floors(root: &Path, body: &str) {
        std::fs::write(root.join(FLOOR_FILE), body).unwrap();
    }

    #[test]
    fn missing_floors_file_hard_fails() {
        // (c) The gate cannot enforce a ratchet it cannot read.
        let root = temp_root("missing");
        let err = load_floors(&root, &registered_rubric()).unwrap_err();
        assert!(
            err.contains("cannot read ratchet floor file"),
            "missing floors file must hard-fail: {err}"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn malformed_line_hard_fails_naming_the_line() {
        // (a) A non-comment line without a tab is a format error, not a skip.
        let root = temp_root("malformed");
        write_floors(
            &root,
            "# header\nhttps://x/slices/logic\ttierRegistered\nno-tab-on-this-line\n",
        );
        let err = load_floors(&root, &registered_rubric()).unwrap_err();
        assert!(err.contains("malformed floor line"), "{err}");
        assert!(err.contains(":3:"), "names the 1-based line: {err}");
        assert!(
            err.contains("no-tab-on-this-line"),
            "quotes the line: {err}"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn unknown_tier_label_hard_fails_naming_the_label() {
        // (b) A tier label that names no gmeow:QualityTier is a format error.
        let root = temp_root("unknown-tier");
        write_floors(&root, "https://x/slices/logic\ttierBogus\n");
        let err = load_floors(&root, &registered_rubric()).unwrap_err();
        assert!(err.contains("unknown tier label"), "{err}");
        assert!(err.contains("tierBogus"), "names the label: {err}");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn well_formed_floors_load() {
        // A comment, blank line, and a real rung resolve to the tier's rank.
        let root = temp_root("ok");
        write_floors(
            &root,
            "# committed floors\n\nhttps://x/slices/logic\ttierRegistered\n",
        );
        let floors = load_floors(&root, &registered_rubric()).unwrap();
        assert_eq!(floors.get("https://x/slices/logic").copied(), Some(0));
        std::fs::remove_dir_all(&root).ok();
    }
}
