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
use gmeow_slice_quality::model::{Rubric, SliceAssessment, Tier};
use gmeow_slice_quality::report::{SliceReport, score_slice, score_slice_with_rubric};

use crate::dev_common::{emit_error, fail, note, project_root};
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

/// Wrap a slice-quality-dev error message as a typed diagnostic on the substrate.
fn sqe(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::SourceReadFailed { detail })
}

impl Format {
    fn parse(s: Option<&str>) -> gmeow_errors::Result<Self> {
        match s {
            None | Some("text") => Ok(Self::Text),
            Some("json") => Ok(Self::Json),
            Some("sarif") => Ok(Self::Sarif),
            Some("rdf") => Ok(Self::Rdf),
            Some(other) => Err(sqe(format!(
                "unknown --format {other} (want text|json|sarif|rdf)"
            ))),
        }
    }
}

fn render(report: &SliceReport, format: Format) -> gmeow_errors::Result<String> {
    match format {
        Format::Text => Ok(report.render_text()),
        Format::Json => Ok(gmeow_errors::render::to_json(&report.to_report())?),
        Format::Sarif => Ok(gmeow_errors::render::to_sarif(&report.to_report())?),
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
fn resolve_min_tier<'a>(rubric: &'a Rubric, name: &str) -> gmeow_errors::Result<&'a Tier> {
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
    Err(sqe(format!(
        "slice-quality: unknown --min-tier {name:?} (want one of: {})",
        known.join(", ")
    )))
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
    // The text default is the repo-wide PRIORITIZATION view (G12): the per-axis
    // profile vectors are collected here and, after the sweep, folded into the
    // deterministic Pareto-frontier + capping-axis prioritization. The assessment
    // (the primary object) and the advisory count (display only) are all it needs.
    let mut profiles: Vec<(SliceAssessment, usize)> = Vec::new();
    for dir in &dirs {
        match score_slice_with_rubric(dir, rubric.clone()) {
            Ok(report) => {
                match format {
                    Format::Text => {
                        profiles.push((report.assessment.clone(), report.advisories.len()));
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
            Err(e) => emit_error(
                "gmeow-dev.slice-quality.score",
                format!("slice-quality: {}: {e}", dir.display()),
            ),
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
        Format::Text => {
            // The enriched text default: the repo-wide Pareto-frontier + capping-axis
            // prioritization, computed across every swept slice's profile vector.
            let inputs: Vec<gmeow_slice_quality::prioritize::SliceInput> = profiles
                .iter()
                .map(
                    |(assessment, advice_count)| gmeow_slice_quality::prioritize::SliceInput {
                        assessment,
                        advice_count: *advice_count,
                    },
                )
                .collect();
            let rows = gmeow_slice_quality::prioritize::prioritize(&inputs, &rubric);
            print!("{}", gmeow_slice_quality::prioritize::render_text(&rows));
        }
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
            emit_error(
                "gmeow-dev.slice-quality.gate",
                format!(
                    "FAIL {slice} measures {measured} — below --min-tier {}",
                    required.label
                ),
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

/// The committed PER-AXIS floor artifact: `<slice-iri>\t
/// <axis-local-name>\t<floor:f64>` per line — a NEW file, distinct in shape from
/// [`FLOOR_FILE`]'s per-slice tier ratchet (never overloaded onto it).
const AXIS_FLOOR_FILE: &str = "governance/slice-quality-axis-floors.tsv";

/// The `gmeow:axisGmn1Coverage` local name — the sole axis this gate's per-axis
/// floor pass currently enforces. A grounding slice (directory under
/// `slices/grounding/`) is hard-gated at floor `1.0` even when absent from
/// [`AXIS_FLOOR_FILE`] — grounding coverage is total NOW (Task 6) and never
/// silently unfloored.
const AXIS_GMN1_COVERAGE: &str = "axisGmn1Coverage";

/// Whether `slice_dir` is a grounding slice — the `slices/grounding/` PATH prefix
/// (there is no `gmeow:tierGrounding` predicate to read; `slices/grounding/` is
/// organizational path-only per `slices/vocabulary.ttl`).
fn is_grounding_slice(slice_dir: &Path) -> bool {
    slice_dir
        .components()
        .map(|c| c.as_os_str())
        .collect::<Vec<_>>()
        .windows(2)
        .any(|w| w[0] == "slices" && w[1] == "grounding")
}

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
            emit_error("gmeow-dev.slice-quality.gate", format!("FAIL {e}"));
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
        let floor_rank = floors.get(&report.assessment.slice).map(|f| f.rank);
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
                emit_error(
                    "gmeow-dev.slice-quality.gate",
                    format!(
                        "FAIL {} declared {} but measures {} — uplift the slice or lower is forbidden",
                        report.assessment.slice, declared.label, report.assessment.rollup.label
                    ),
                );
                failures += 1;
            }
            RatchetVerdict::DeclaredBelowFloor => {
                emit_error(
                    "gmeow-dev.slice-quality.gate",
                    format!(
                        "FAIL {} declares {} below its committed ratchet floor — the tier may only be raised",
                        report.assessment.slice, declared.label
                    ),
                );
                failures += 1;
            }
        }
    }

    // SECOND pass: the per-axis committed floor — additive to,
    // never replacing, the roll-up-tier ratchet above. Runs over EVERY discovered
    // slice, opted-in or not: a grounding slice can never clear the gate on
    // axisGmn1Coverage < 1.0 regardless of its roll-up tier or opt-in status.
    let axis_floors = match load_axis_floors(&root) {
        Ok(f) => f,
        Err(e) => return fail(format!("slice-quality-gate: {e}")),
    };
    let mut axis_checked = 0usize;
    let mut axis_failures = 0usize;
    // Every discovered slice's IRI — the "still live" set the floor-monotonicity
    // check consults to tell a permitted greenfield floor removal (slice gone) from
    // a forbidden deletion of a still-live floor line.
    let mut live_slices: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for dir in &dirs {
        let report = match score_slice_with_rubric(dir, rubric.clone()) {
            Ok(r) => r,
            Err(e) => return fail(format!("slice-quality-gate: {}: {e}", dir.display())),
        };
        live_slices.insert(report.assessment.slice.clone());
        let Some(grade) = report
            .assessment
            .grades
            .iter()
            .find(|g| axis_local_name(&g.axis_iri) == AXIS_GMN1_COVERAGE)
        else {
            continue; // the axis is unbound in this rubric snapshot — the binding gate above already caught that
        };
        let floor = axis_floors
            .get(&(
                report.assessment.slice.clone(),
                AXIS_GMN1_COVERAGE.to_owned(),
            ))
            .copied()
            .or_else(|| is_grounding_slice(dir).then_some(1.0));
        let Some(floor) = floor else {
            continue; // no committed floor recorded and not grounding → unfloored, advisory only
        };
        axis_checked += 1;
        use gmeow_slice_quality::gate::AxisRatchetVerdict;
        match gmeow_slice_quality::gate::evaluate_axis_floor(grade.score, floor) {
            AxisRatchetVerdict::Pass => {}
            AxisRatchetVerdict::MeasuredBelowFloor => {
                emit_error(
                    "gmeow-dev.slice-quality.gate",
                    format!(
                        "FAIL {} measures {AXIS_GMN1_COVERAGE} {:.6} — below its committed per-axis floor {floor:.6} ({AXIS_FLOOR_FILE})",
                        report.assessment.slice, grade.score
                    ),
                );
                axis_failures += 1;
            }
        }
    }

    // THIRD check: committed-floor MONOTONICITY. The two passes above only compare
    // measured/declared value against the CURRENT committed floor — neither notices
    // a PR that silently LOWERS a floor line. Enforce each file's own "may only be
    // raised" ratchet promise by diffing the working-tree floor files against their
    // merge-base versions. Reds on any lowered floor or the deletion of a still-live
    // floor; additions and greenfield removals (slice/axis gone) are allowed.
    let live_axes: std::collections::BTreeSet<String> = rubric
        .axes
        .iter()
        .map(|a| axis_local_name(&a.iri).to_owned())
        .collect();
    let mono_failures = match resolve_base_ref(&root) {
        BaseRef::NoUpstream(reason) => {
            note(
                "gmeow-dev.slice-quality.gate",
                format!(
                    "slice-quality-gate: floor-monotonicity check SKIPPED — {reason}; nothing to compare against this run (no origin/main merge base)"
                ),
            );
            0
        }
        BaseRef::Unresolvable(reason) => {
            return fail(format!(
                "slice-quality-gate: cannot verify floor monotonicity — {reason} (the committed floor comparand could not be obtained)"
            ));
        }
        BaseRef::Resolved(base) => {
            let mut mono: Vec<String> = Vec::new();
            // Tier floor file.
            match git_show_base(&root, &base, FLOOR_FILE) {
                BaseFile::Absent => note(
                    "gmeow-dev.slice-quality.gate",
                    format!(
                        "slice-quality-gate: floor-monotonicity check SKIPPED for {FLOOR_FILE} — the file is absent at base {base} (brand-new file, nothing to regress against)"
                    ),
                ),
                BaseFile::Error(e) => return fail(format!("slice-quality-gate: {e}")),
                BaseFile::Contents(text) => {
                    let base_floors =
                        match parse_tier_floors(&text, &format!("{base}:{FLOOR_FILE}"), &rubric) {
                            Ok(m) => m,
                            Err(e) => return fail(format!("slice-quality-gate: {e}")),
                        };
                    mono.extend(gmeow_slice_quality::gate::tier_floor_monotonicity(
                        FLOOR_FILE,
                        &base_floors,
                        &floors,
                        |slice| live_slices.contains(slice),
                    ));
                }
            }
            // Per-axis floor file.
            match git_show_base(&root, &base, AXIS_FLOOR_FILE) {
                BaseFile::Absent => note(
                    "gmeow-dev.slice-quality.gate",
                    format!(
                        "slice-quality-gate: floor-monotonicity check SKIPPED for {AXIS_FLOOR_FILE} — the file is absent at base {base} (brand-new file, nothing to regress against)"
                    ),
                ),
                BaseFile::Error(e) => return fail(format!("slice-quality-gate: {e}")),
                BaseFile::Contents(text) => {
                    let base_axis =
                        match parse_axis_floors(&text, &format!("{base}:{AXIS_FLOOR_FILE}")) {
                            Ok(m) => m,
                            Err(e) => return fail(format!("slice-quality-gate: {e}")),
                        };
                    mono.extend(gmeow_slice_quality::gate::axis_floor_monotonicity(
                        AXIS_FLOOR_FILE,
                        &base_axis,
                        &axis_floors,
                        |slice, axis| live_slices.contains(slice) && live_axes.contains(axis),
                    ));
                }
            }
            for e in &mono {
                emit_error("gmeow-dev.slice-quality.gate", format!("FAIL {e}"));
            }
            mono.len()
        }
    };

    if failures > 0 || axis_failures > 0 || mono_failures > 0 {
        return fail(format!(
            "slice-quality-gate: {failures} of {checked} opted-in slice(s) below their declared tier; {axis_failures} of {axis_checked} slice(s) below a committed per-axis floor; {mono_failures} committed-floor monotonicity violation(s)"
        ));
    }
    println!(
        "slice-quality-gate: {checked} opted-in slice(s) hold their declared tier; {axis_checked} slice(s) hold their committed per-axis floors; committed floors are monotonic vs the merge base"
    );
    0
}

/// The merge-base resolution outcome for the floor-monotonicity check.
///
/// This is a COMPARISON gate: its comparand is `git show <merge-base
/// HEAD origin/main>:<floor-file>`. That comparand is only LEGITIMATELY empty
/// when `origin/main` itself is not a reachable ref (a bare local clone that
/// never fetched it) — there, "may only be raised" is vacuously satisfied and
/// [`BaseRef::NoUpstream`] is a correct, loud skip. Every OTHER failure to
/// obtain the comparand (the ref exists but `merge-base` errors or resolves
/// empty, or git itself cannot run) means the gate cannot perform the
/// comparison it is defined to perform; passing there would let a lowered
/// floor slip through unseen, so [`BaseRef::Unresolvable`] HARD-FAILS the
/// gate instead of skipping it.
enum BaseRef {
    /// The resolved merge-base commit the working floor files are diffed against.
    Resolved(String),
    /// `origin/main` genuinely does not exist as a ref in this checkout — the
    /// only case where "no prior committed state is reachable" is expected
    /// rather than broken. A loud SKIP, never a silent pass.
    NoUpstream(String),
    /// `origin/main` exists (or ref existence couldn't be checked) but the
    /// comparand could not be obtained — mis-provisioned checkout, git error,
    /// empty merge-base, or git binary absent. HARD FAIL: the gate cannot
    /// verify the invariant it is defined to enforce.
    Unresolvable(String),
}

/// Resolve `git merge-base HEAD origin/main` LOCALLY (no network). CI builds the PR
/// merged into `main`, so `origin/main` is present there. A clone that never
/// fetched `origin/main` yields [`BaseRef::NoUpstream`] (legitimate skip); any
/// other failure to resolve the merge-base yields [`BaseRef::Unresolvable`]
/// (hard fail — see the enum doc-comment).
fn resolve_base_ref(root: &Path) -> BaseRef {
    match std::process::Command::new("git")
        .current_dir(root)
        .env("LC_ALL", "C")
        .args(["rev-parse", "--verify", "--quiet", "origin/main"])
        .output()
    {
        Ok(out) if out.status.success() => {}
        Ok(_) => {
            return BaseRef::NoUpstream(
                "`origin/main` does not exist as a ref in this checkout (no upstream fetched)"
                    .to_owned(),
            );
        }
        Err(e) => return BaseRef::Unresolvable(format!("could not run git: {e}")),
    }

    match std::process::Command::new("git")
        .current_dir(root)
        .env("LC_ALL", "C")
        .args(["merge-base", "HEAD", "origin/main"])
        .output()
    {
        Ok(out) if out.status.success() => {
            let sha = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            if sha.is_empty() {
                BaseRef::Unresolvable(
                    "`git merge-base HEAD origin/main` resolved no commit".to_owned(),
                )
            } else {
                BaseRef::Resolved(sha)
            }
        }
        Ok(out) => BaseRef::Unresolvable(format!(
            "`git merge-base HEAD origin/main` failed ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        )),
        Err(e) => BaseRef::Unresolvable(format!("could not run git: {e}")),
    }
}

/// The outcome of reading one floor file at the merge base via `git show`.
enum BaseFile {
    /// The blob contents at the base commit.
    Contents(String),
    /// The file did not exist at the base (a brand-new floor file) — SKIP its
    /// monotonicity check, never mask a real regression.
    Absent,
    /// `git show` failed for a reason OTHER than an absent path — a HARD FAIL.
    Error(String),
}

/// Read `<base>:<rel>` via `git show` (local, no network). A path-absent error is
/// distinguished from any other git failure by the well-known "does not exist in"
/// / "exists on disk, but not in" fatal messages, so a genuinely-new floor file is
/// a skip while a bad object / broken repo is a hard fail.
fn git_show_base(root: &Path, base: &str, rel: &str) -> BaseFile {
    let spec = format!("{base}:{rel}");
    match std::process::Command::new("git")
        .current_dir(root)
        .env("LC_ALL", "C")
        .args(["show", &spec])
        .output()
    {
        Ok(out) if out.status.success() => {
            BaseFile::Contents(String::from_utf8_lossy(&out.stdout).into_owned())
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("does not exist in") || stderr.contains("exists on disk, but not in")
            {
                BaseFile::Absent
            } else {
                BaseFile::Error(format!(
                    "`git show {spec}` failed ({}): {}",
                    out.status,
                    stderr.trim()
                ))
            }
        }
        Err(e) => BaseFile::Error(format!("could not run `git show {spec}`: {e}")),
    }
}

/// The local name of an IRI (the tail after the last `/` or `#`) — used to match a
/// rubric axis IRI against the bare local name [`AXIS_FLOOR_FILE`] rows carry.
fn axis_local_name(iri: &str) -> &str {
    iri.rsplit(['/', '#']).next().unwrap_or(iri)
}

/// Load the committed PER-AXIS floors, keyed by `(slice IRI,
/// axis local name)`.
///
/// # Errors
/// A HARD FAIL (.goals no-optionality) when the gate cannot read the floor it must
/// enforce: the file is missing/unreadable, a non-comment line is not a
/// `<slice-iri>\t<axis-local-name>\t<floor:f64>` triple, or the floor is not a
/// valid `f64`. Never a silently-disabled floor or a silently-skipped line.
fn load_axis_floors(
    root: &Path,
) -> gmeow_errors::Result<std::collections::BTreeMap<(String, String), f64>> {
    let path = root.join(AXIS_FLOOR_FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| {
        sqe(format!(
            "cannot read per-axis ratchet floor file {} (the gate cannot enforce a floor it cannot read): {e}",
            path.display()
        ))
    })?;
    parse_axis_floors(&text, &path.display().to_string())
}

/// Parse per-axis floor file CONTENTS into `(slice IRI, axis local name) → floor`.
/// The single parser shared by [`load_axis_floors`] (working-tree, reading from
/// disk) and the floor-monotonicity check (base version, reading from `git show`)
/// so both handle the format identically — `source_label` names the origin (a path
/// or a `<base>:<file>` git spec) in any error. Same hard-fail semantics as before:
/// a non-comment line that is not a `<slice-iri>\t<axis-local-name>\t<floor:f64>`
/// triple, or a floor that is not a valid `f64`, is a HARD FAIL, never a skip.
fn parse_axis_floors(
    text: &str,
    source_label: &str,
) -> gmeow_errors::Result<std::collections::BTreeMap<(String, String), f64>> {
    let mut out = std::collections::BTreeMap::new();
    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let lineno = idx + 1;
        let Some((iri, rest)) = line.split_once('\t') else {
            return Err(sqe(format!(
                "{source_label}:{lineno}: malformed axis-floor line (want <slice-iri>\\t<axis-local-name>\\t<floor:f64>): {raw:?}"
            )));
        };
        let Some((axis_local, floor_str)) = rest.split_once('\t') else {
            return Err(sqe(format!(
                "{source_label}:{lineno}: malformed axis-floor line (want <slice-iri>\\t<axis-local-name>\\t<floor:f64>): {raw:?}"
            )));
        };
        let floor: f64 = floor_str.trim().parse().map_err(|_| {
            sqe(format!(
                "{source_label}:{lineno}: floor {:?} is not a valid f64",
                floor_str.trim()
            ))
        })?;
        out.insert((iri.trim().to_owned(), axis_local.trim().to_owned()), floor);
    }
    Ok(out)
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
) -> gmeow_errors::Result<std::collections::BTreeMap<String, gmeow_slice_quality::gate::TierFloor>>
{
    let path = root.join(FLOOR_FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| {
        sqe(format!(
            "cannot read ratchet floor file {} (the gate cannot enforce a floor it cannot read): {e}",
            path.display()
        ))
    })?;
    parse_tier_floors(&text, &path.display().to_string(), rubric)
}

/// Parse tier-floor file CONTENTS into `slice IRI → TierFloor`. The single parser
/// shared by [`load_floors`] (working-tree, from disk) and the floor-monotonicity
/// check (base version, from `git show`) so both resolve tier local names against
/// the SAME ladder identically — `source_label` names the origin (a path or a
/// `<base>:<file>` git spec) in any error. Same hard-fail semantics as before: a
/// non-comment line that is not a `<slice-iri>\t<tier-local-name>` pair, or a tier
/// label that names no `gmeow:QualityTier` in the ladder, is a HARD FAIL.
fn parse_tier_floors(
    text: &str,
    source_label: &str,
    rubric: &gmeow_slice_quality::Rubric,
) -> gmeow_errors::Result<std::collections::BTreeMap<String, gmeow_slice_quality::gate::TierFloor>>
{
    let mut out = std::collections::BTreeMap::new();
    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let lineno = idx + 1;
        let Some((iri, tier_local)) = line.split_once('\t') else {
            return Err(sqe(format!(
                "{source_label}:{lineno}: malformed floor line (want <slice-iri>\\t<tier-local-name>): {raw:?}"
            )));
        };
        let tier_local = tier_local.trim();
        let tier_iri = format!("{}{}", gmeow_slice_quality::model::GMEOW, tier_local);
        let Some(tier) = rubric.tier(&tier_iri) else {
            return Err(sqe(format!(
                "{source_label}:{lineno}: unknown tier label {tier_local:?} (names no gmeow:QualityTier in the rubric ladder)"
            )));
        };
        out.insert(
            iri.trim().to_owned(),
            gmeow_slice_quality::gate::TierFloor {
                rank: tier.rank,
                local: tier_local.to_owned(),
            },
        );
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
            commitments: vec![],
            tier_floors: vec![],
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
        assert!(err.message().contains("unknown --min-tier"), "{err}");
        assert!(
            err.message().contains("Platinum"),
            "names the bad input: {err}"
        );
        // Lists the available rungs, ladder-ordered.
        assert!(
            err.message()
                .contains("Registered, Grounded, Linked, Exemplified, Maximal"),
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
            err.message().contains("cannot read ratchet floor file"),
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
        assert!(err.message().contains("malformed floor line"), "{err}");
        assert!(
            err.message().contains(":3:"),
            "names the 1-based line: {err}"
        );
        assert!(
            err.message().contains("no-tab-on-this-line"),
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
        assert!(err.message().contains("unknown tier label"), "{err}");
        assert!(
            err.message().contains("tierBogus"),
            "names the label: {err}"
        );
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
        assert_eq!(
            floors.get("https://x/slices/logic").map(|f| f.rank),
            Some(0)
        );
        std::fs::remove_dir_all(&root).ok();
    }
}
