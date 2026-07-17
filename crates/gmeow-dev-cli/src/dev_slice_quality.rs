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
use gmeow_slice_quality::ScoringEnv;
#[cfg(test)]
use gmeow_slice_quality::model::{MeasurementStandard, Tier};
use gmeow_slice_quality::model::{Rubric, SliceAssessment};
use gmeow_slice_quality::report::{SliceReport, score_slice_with_standard};
use gmeow_slice_quality::{resolve_min_tier, tier_gate_passes};

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
    // Load the floor-free measurement standard from the repo rubric, then score the one
    // slice against it in repo mode (byte-identical to the retired repo-coupled path).
    let standard = match repo_rubric(&root) {
        Ok(r) => r.standard,
        Err(e) => return fail(format!("slice-quality: {e}")),
    };
    match score_slice_with_standard(&dir, &standard, ScoringEnv::Repo) {
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
            let required = match resolve_min_tier(&report.standard, required) {
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
    let rubric = match repo_rubric(root) {
        Ok(r) => r,
        Err(e) => return fail(format!("slice-quality: {e}")),
    };
    // G11 sweep gate: resolve the required tier once, up front, so an unknown tier
    // is a clear error before any slice is scored. Gating the sweep (fail if ANY
    // slice is below) is the more useful choice than conflicting with --all.
    let required = match min_tier {
        Some(name) => match resolve_min_tier(&rubric.standard, name) {
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
    let scored = gmeow_slice_quality::score_slices_with_rubric(root, &dirs, &rubric);
    for (dir, result) in dirs.iter().zip(scored) {
        match result {
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

/// The canonical, ontology-resident home of the committed floors: the rubric slice
/// module the gate reads BOTH the per-axis measured-score floors
/// (`gmeow:AxisFloorCommitment`) and the per-slice roll-up tier floors
/// (`gmeow:SliceTierFloor`) out of, and the file the floor-monotonicity check diffs
/// against its merge-base version. The governance TSVs are no longer read.
const RUBRIC_MODULE: &str = "slices/core/slice-quality-rubric/module.ttl";

/// Load the whole rubric (the floor-free measurement `standard` scoring reads plus the
/// governance `floors` the ratchet gate reads) from the canonical [`RUBRIC_MODULE`]
/// under `root`. The engine no longer exposes a conflated repo-rubric loader; this gate
/// is a legitimate consumer of BOTH halves (it scores AND ratchets in one pass), so it
/// reconstructs the whole from the same on-disk file — byte-identical to the retired
/// `load_repo_rubric`.
///
/// # Errors
/// Returns a diagnostic if the rubric module cannot be read/parsed or is structurally
/// incomplete (the same hard-fail conditions the engine loader enforced).
fn repo_rubric(root: &Path) -> gmeow_errors::Result<Rubric> {
    let module = root.join(RUBRIC_MODULE);
    let ds = gmeow_slice_quality::dataset_from_paths(&[&module])?;
    gmeow_slice_quality::rubric::load_rubric(&ds)
}

/// The generated per-axis floor projection path named in a per-axis floor failure
/// message — the lossy TSV view of the ontology-resident commitments, kept only as
/// a human pointer in the diagnostic (the canonical source is [`RUBRIC_MODULE`]).
const AXIS_FLOOR_PROJECTION: &str = "generated/governance/slice-quality-axis-floors.tsv";

/// The `gmeow:axisGmn1Coverage` local name — used SOLELY for the grounding-slice
/// `1.0` default: a grounding slice (directory under `slices/grounding/`) is
/// hard-gated at floor `1.0` on this axis even with no explicit
/// `gmeow:AxisFloorCommitment`. Every OTHER (slice, axis) floor is enforced only
/// when an explicit commitment records it — grounding coverage is total and never
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
    let rubric = match repo_rubric(&root) {
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

    // Coat-side DISTINCTIVENESS gate: within a slice, no two distinct TBox terms may
    // share a normalized skeleton for a distinguishing coat — usage coats
    // (useWhen/avoidWhen/howToUse) and skos:definition, all under one no-strip skeleton
    // (lowercase + whitespace-collapse; load-bearing CURIEs kept as content). A hard
    // boolean reject at N=2 (any collision), NOT a scored axis or a tuned floor: a coat
    // cosmetically dressed up but substantively identical to another term's is a
    // near-duplicate template. Reds the gate on any collision, naming the slice,
    // predicate, skeleton, and colliding terms.
    let coat_dirs = gmeow_slice_quality::discover_slice_dirs(&root.join("slices"));
    let mut coat_collisions: Vec<String> = Vec::new();
    for dir in &coat_dirs {
        match gmeow_slice_quality::coat_guard::slice_coat_collisions(dir) {
            Ok(hits) => coat_collisions.extend(hits),
            Err(e) => return fail(format!("slice-quality-gate: {e}")),
        }
    }
    if !coat_collisions.is_empty() {
        for e in &coat_collisions {
            emit_error("gmeow-dev.slice-quality.gate", format!("FAIL {e}"));
        }
        return fail(format!(
            "slice-quality-gate: {} coat distinctiveness violation(s) — a coat must distinguish its term",
            coat_collisions.len()
        ));
    }

    // Roll-up tier floor ranks by slice IRI, projected from the ontology-resident
    // gmeow:SliceTierFloor commitments and resolved against the ladder. An unknown
    // floorTier is a HARD FAIL here (.goals no-optionality), never a silently-
    // disabled floor.
    let floors = match tier_floors_from_rubric(&rubric) {
        Ok(f) => f,
        Err(e) => return fail(format!("slice-quality-gate: {e}")),
    };
    // Per-axis measured-score floors, projected from the ontology-resident
    // gmeow:AxisFloorCommitment commitments, keyed by (slice IRI, axis local name).
    let axis_floors = match axis_floors_from_rubric(&rubric) {
        Ok(m) => m,
        Err(e) => return fail(format!("slice-quality-gate: {e}")),
    };

    let dirs = gmeow_slice_quality::discover_slice_dirs(&root.join("slices"));
    // Score every discovered slice EXACTLY ONCE, in deterministic dir order, and feed
    // BOTH the roll-up-tier ratchet pass and the per-axis floor pass from these shared
    // reports — a slice is never scored twice.
    let score_results = gmeow_slice_quality::score_slices_with_rubric(&root, &dirs, &rubric);
    let mut scored: Vec<(&Path, SliceReport)> = Vec::with_capacity(dirs.len());
    for (dir, result) in dirs.iter().zip(score_results) {
        let report = match result {
            Ok(r) => r,
            Err(e) => return fail(format!("slice-quality-gate: {}: {e}", dir.display())),
        };
        scored.push((dir.as_path(), report));
    }

    let mut failures = 0usize;
    let mut checked = 0usize;
    for (dir, report) in &scored {
        let declared = match gmeow_slice_quality::gate::declared_tier(dir, &rubric) {
            Ok(d) => d,
            Err(e) => return fail(format!("slice-quality-gate: {e}")),
        };
        let Some(declared) = declared else { continue }; // undeclared → advisory
        checked += 1;
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

    // SECOND pass: the per-axis committed floor — additive to, never replacing, the
    // roll-up-tier ratchet above. Runs over EVERY discovered slice and EVERY axis it
    // grades: a floor binds an axis when an explicit gmeow:AxisFloorCommitment records
    // (slice, axis), OR — only for axisGmn1Coverage on a grounding slice — the
    // total-coverage 1.0 default. So a grounding slice can never clear the gate on
    // axisGmn1Coverage < 1.0 regardless of its roll-up tier or opt-in status, and
    // every other committed per-axis floor is enforced independently on its own axis.
    let mut axis_checked = 0usize;
    let mut axis_failures = 0usize;
    // Every discovered slice's IRI — the "still live" set the floor-monotonicity
    // check consults to tell a permitted greenfield floor removal (slice gone) from
    // a forbidden deletion of a still-live floor line.
    let mut live_slices: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for (dir, report) in &scored {
        live_slices.insert(report.assessment.slice.clone());
        let grounding = is_grounding_slice(dir);
        for grade in &report.assessment.grades {
            let axis_local = axis_local_name(&grade.axis_iri);
            let floor = match axis_floor_for(
                &axis_floors,
                &report.assessment.slice,
                axis_local,
                grounding,
            ) {
                Ok(Some(f)) => f,
                Ok(None) => continue, // no committed floor and not the grounding default → unfloored, advisory only
                Err(e) => return fail(format!("slice-quality-gate: {e}")),
            };
            axis_checked += 1;
            use gmeow_slice_quality::gate::AxisRatchetVerdict;
            match gmeow_slice_quality::gate::evaluate_axis_floor(grade.score, floor) {
                AxisRatchetVerdict::Pass => {}
                AxisRatchetVerdict::MeasuredBelowFloor => {
                    emit_error(
                        "gmeow-dev.slice-quality.gate",
                        format!(
                            "FAIL {} measures {axis_local} {:.6} — below its committed per-axis floor {floor:.6} ({AXIS_FLOOR_PROJECTION})",
                            report.assessment.slice, grade.score
                        ),
                    );
                    axis_failures += 1;
                }
            }
        }
    }

    // The projection-vocabulary RATCHET's shared inputs — computed here (BEFORE the
    // merge-base match below) because the grandfather sub-check folded into that
    // match's `BaseFile::Contents` arm needs `vocabularies`/`working_ceilings`/
    // `working_residues` in scope. The COUNT-GATE evaluation loop over these same
    // values runs later, after the FOURTH (coherence) check, so every diagnostic
    // this gate can emit is grouped by check rather than by where its inputs happen
    // to be computed.
    let vocabularies = &rubric.floors.vocabularies;
    let working_ceilings = ceilings_from_rubric(&rubric);
    let working_residues = match gmeow_slice_quality::measure_repo_residues(&root, vocabularies) {
        Ok(m) => m,
        Err(e) => return fail(format!("slice-quality-gate: {e}")),
    };
    // The effective ceiling a (slice, vocab) cell with no explicit commitment is
    // held to: that vocab's `gmeow:vocabularyDefaultCeiling` (0 for every guarded
    // vocab today).
    let default_ceiling: std::collections::BTreeMap<&str, u64> = vocabularies
        .iter()
        .map(|v| (v.prefix.as_str(), v.default_ceiling))
        .collect();

    // THIRD check: committed-floor MONOTONICITY. The two passes above only compare
    // measured/declared value against the CURRENT committed floor — neither notices a
    // PR that silently LOWERS a floor. Both floor levels now live in the rubric
    // module, so enforce their shared "may only be raised" ratchet promise by diffing
    // the working-tree module.ttl commitments against the merge-base module.ttl's,
    // parsed through the SAME rubric loader. Reds on any lowered floor or the deletion
    // of a still-live floor; additions and greenfield removals (slice/axis gone) are
    // allowed. NOTE: at a merge base predating the migration the module.ttl carries no
    // floor commitments, so every working floor reads as an addition (allowed) — the
    // value-preservation golden test guards the migrated values instead.
    let live_axes: std::collections::BTreeSet<String> = rubric
        .standard
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
            // Both floor levels are diffed against the ONE rubric module at the base.
            match git_show_base(&root, &base, RUBRIC_MODULE) {
                BaseFile::Absent => note(
                    "gmeow-dev.slice-quality.gate",
                    format!(
                        "slice-quality-gate: floor-monotonicity check SKIPPED for {RUBRIC_MODULE} — the file is absent at base {base} (brand-new file, nothing to regress against)"
                    ),
                ),
                BaseFile::Error(e) => return fail(format!("slice-quality-gate: {e}")),
                BaseFile::Contents(text) => {
                    let base_rubric =
                        match load_rubric_from_ttl(&text, &format!("{base}:{RUBRIC_MODULE}")) {
                            Ok(r) => r,
                            Err(e) => return fail(format!("slice-quality-gate: {e}")),
                        };
                    // Tier floors: project the base commitments through the SAME
                    // ladder-resolving projection the working set used.
                    let base_floors = match tier_floors_from_rubric(&base_rubric) {
                        Ok(m) => m,
                        Err(e) => return fail(format!("slice-quality-gate: {e}")),
                    };
                    let tier_mono = gmeow_slice_quality::gate::tier_floor_monotonicity(
                        RUBRIC_MODULE,
                        &base_floors,
                        &floors,
                        |slice| live_slices.contains(slice),
                    );
                    mono.extend(tier_mono.violations);
                    // Per-axis floors: same projection, keyed by (slice, axis local).
                    let base_axis = match axis_floors_from_rubric(&base_rubric) {
                        Ok(m) => m,
                        Err(e) => return fail(format!("slice-quality-gate: {e}")),
                    };
                    let axis_mono = gmeow_slice_quality::gate::axis_floor_monotonicity(
                        RUBRIC_MODULE,
                        &base_axis,
                        &axis_floors,
                        |slice, axis| live_slices.contains(slice) && live_axes.contains(axis),
                    );
                    mono.extend(axis_mono.violations);

                    // Projection-ceiling MONOTONICITY (ratchet invariant 2): a
                    // committed ceiling shared by base and working may never RISE.
                    let base_ceilings = ceilings_from_rubric(&base_rubric);
                    let cmono = gmeow_slice_quality::gate::projection_ceiling_monotonicity(
                        RUBRIC_MODULE,
                        &base_ceilings,
                        &working_ceilings,
                    );
                    mono.extend(cmono.violations);

                    // Registry meta-ratchet (C8): the guarded-vocabulary REGISTRY may
                    // only get STRONGER — deleting a vocab, narrowing a namespace,
                    // weakening a count-kind, dropping a counted predicate, raising a
                    // default ceiling, or expanding an exemption set all red the gate,
                    // so the gate cannot be quietly weakened without raising a cell.
                    mono.extend(gmeow_slice_quality::gate::registry_ratchet_monotonicity(
                        RUBRIC_MODULE,
                        &base_rubric.floors.vocabularies,
                        &rubric.floors.vocabularies,
                    ));

                    // GRANDFATHER gate (ratchet invariant 3): a ceiling that is NEW
                    // in the working tree (absent at base) may only record residue
                    // that ALREADY EXISTED at the merge base — never freshly
                    // authored constructs. Base measured is reconstructed by
                    // feeding the SAME counter the base bytes over the SAME
                    // multi-surface authoring set (module.ttl + shapes.ttl +
                    // mappings/*.ttl), never a singular `git show`.
                    let new_keys: std::collections::BTreeSet<String> = working_ceilings
                        .keys()
                        .filter(|k| !base_ceilings.contains_key(*k))
                        .map(|(slice, _)| slice.clone())
                        .collect();
                    if !new_keys.is_empty() {
                        let base_res =
                            match measure_base_residues(&root, &base, vocabularies, &new_keys) {
                                Ok(r) => r,
                                Err(e) => return fail(format!("slice-quality-gate: {e}")),
                            };
                        for (key, committed) in &working_ceilings {
                            if base_ceilings.contains_key(key) {
                                continue; // not new — covered by the monotonicity check above
                            }
                            let (slice, vocab) = key;
                            let bm = base_res.get(key).copied().unwrap_or(0);
                            if *committed > bm {
                                mono.push(format!(
                                    "{RUBRIC_MODULE}: NEW projection ceiling slice {slice} vocab {vocab} count {committed} exceeds base measured residue {bm} — a new ceiling may only grandfather residue present at the merge base, never freshly-authored constructs"
                                ));
                            }
                        }
                    }
                }
            }
            // Floors are raise-only: a LOWERING and a still-live DELETION are both hard
            // violations. Re-baselining a floor downward is a maintainer-only decision,
            // authorized out-of-band by merging past this red — there is no in-repo permit.
            for e in &mono {
                emit_error("gmeow-dev.slice-quality.gate", format!("FAIL {e}"));
            }
            mono.len()
        }
    };

    // FOURTH check: FLOOR COHERENCE — the lattice morphism tying the two committed
    // floor levels together. Pure over the COMMITTED floors, reading BOTH levels
    // straight from the already-loaded rubric — it needs NO scoring at all (it never
    // touches a measured score), so it adds no scoring sweep. For any slice carrying
    // BOTH a gmeow:SliceTierFloor (rank T) and ≥1 gmeow:AxisFloorCommitment: every
    // axis floor must grade (through that axis's rubric thresholds) to a tier ≥ T
    // (the roll-up is a meet, so a tier floor demands every axis floor back it); and
    // when a slice is floored on EVERY rubric axis, T must EQUAL the meet of the
    // implied tiers (a tier floor below the achievable meet is a dead guarantee).
    // Today's corpus is all-tierRegistered(0) floors, so the backing invariant is
    // trivially satisfied and no slice is floored on all axes → this holds dormant.
    let coherence = gmeow_slice_quality::gate::evaluate_coherence(&rubric);
    let coherence_failures = coherence.len();
    for v in &coherence {
        emit_error(
            "gmeow-dev.slice-quality.gate",
            format!("FAIL {}", v.message),
        );
    }
    // The tier-floored slices that ALSO carry ≥1 axis floor — the pairings the
    // coherence morphism actually examines (reported so the guard's reach is visible
    // even when it holds silently).
    let coherence_checked = rubric
        .floors
        .tier_floors
        .iter()
        .filter(|tf| {
            rubric
                .floors
                .commitments
                .iter()
                .any(|c| c.slice == tf.slice)
        })
        .count();

    // FIFTH check: the projection-vocabulary RATCHET's COUNT GATE (invariant 1) —
    // every (slice, vocab) cell with a nonzero measured ungrounded residue must not
    // exceed its effective ceiling (the committed `gmeow:ceilingCount` if present,
    // else the vocab's `gmeow:vocabularyDefaultCeiling`, 0 for every guarded vocab
    // today). `working_residues`/`working_ceilings`/`default_ceiling` were computed
    // above (before the merge-base match) so the grandfather sub-check could share
    // them; this is where they are actually evaluated and reported.
    let mut ceiling_checked = 0usize;
    let mut ceiling_failures = 0usize;
    for ((slice, vocab), measured) in &working_residues {
        let effective = working_ceilings
            .get(&(slice.clone(), vocab.clone()))
            .copied()
            .unwrap_or_else(|| default_ceiling.get(vocab.as_str()).copied().unwrap_or(0));
        ceiling_checked += 1;
        use gmeow_slice_quality::gate::CeilingVerdict;
        match gmeow_slice_quality::gate::evaluate_projection_ceiling(*measured, effective) {
            CeilingVerdict::Pass => {}
            CeilingVerdict::MeasuredAboveCeiling => {
                emit_error(
                    "gmeow-dev.slice-quality.gate",
                    format!(
                        "FAIL {slice} vocab {vocab} measures ungrounded residue {measured} — above its committed projection ceiling {effective}; author the new logic as logic: and project it, do not hand-author {vocab}"
                    ),
                );
                ceiling_failures += 1;
            }
        }
    }

    if failures > 0
        || axis_failures > 0
        || mono_failures > 0
        || coherence_failures > 0
        || ceiling_failures > 0
    {
        return fail(format!(
            "slice-quality-gate: {failures} of {checked} opted-in slice(s) below their declared tier; {axis_failures} of {axis_checked} slice(s) below a committed per-axis floor; {mono_failures} committed-floor monotonicity violation(s); {coherence_failures} floor-coherence violation(s); {ceiling_failures} of {ceiling_checked} (slice,vocab) cell(s) above their committed projection ceiling"
        ));
    }
    println!(
        "slice-quality-gate: {checked} opted-in slice(s) hold their declared tier; {axis_checked} slice(s) hold their committed per-axis floors; committed floors are monotonic vs the merge base; {coherence_checked} tier-floored slice(s) cohere with their axis floors (0 floor-coherence violation(s)); {ceiling_checked} (slice,vocab) cell(s) hold their projection ceiling"
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
/// rubric axis or tier IRI against the bare local name the gate reasons over.
fn axis_local_name(iri: &str) -> &str {
    iri.rsplit(['/', '#']).next().unwrap_or(iri)
}

/// Project the ontology-resident `gmeow:AxisFloorCommitment` set into the
/// `(slice IRI, axis local name) → floor` map the per-axis floor pass and the
/// axis-floor monotonicity check consume. This first enforces that every rubric
/// axis's local name (the tail after the last `/` or `#`) is GLOBALLY UNIQUE across
/// `rubric.axes` — the floor gate keys every lookup by local name (`axis_floor_for`
/// via `axis_local_name`), so two distinct axis IRIs sharing a local name would let a
/// commitment against one axis silently apply to the other's grade. With that
/// global uniqueness established, the rubric loader's existing hard-fail on
/// duplicate `(slice, full-axis-IRI)` commitments guarantees the projection below
/// (keyed on `(slice, axis local name)`) can never collide, so it is a plain
/// projection — the same map shape the removed governance-TSV parser produced.
///
/// # Errors
/// A HARD FAIL (.goals no-optionality) when two DISTINCT rubric axis IRIs share the
/// same local name (e.g. `ns1#axisFoo` and `ns2#axisFoo`).
fn axis_floors_from_rubric(
    rubric: &Rubric,
) -> gmeow_errors::Result<std::collections::BTreeMap<(String, String), f64>> {
    let mut axes_by_local: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    for axis in &rubric.standard.axes {
        let local = axis_local_name(&axis.iri).to_owned();
        if let Some(prior) = axes_by_local.insert(local.clone(), axis.iri.clone()) {
            return Err(sqe(format!(
                "rubric axes {prior} and {} collide on axis local name {local:?} — the floor \
                 gate keys lookups by local name, so a committed floor could be applied to the \
                 wrong axis",
                axis.iri
            )));
        }
    }

    let mut out = std::collections::BTreeMap::new();
    for c in &rubric.floors.commitments {
        out.insert(
            (c.slice.clone(), axis_local_name(&c.axis).to_owned()),
            c.floor,
        );
    }
    Ok(out)
}

/// The tolerance an explicit `axisGmn1Coverage` grounding-floor commitment is
/// checked against the definitional `1.0` floor under. All grounding GMN1
/// commitments are exactly `1.0` today, so this guard is a no-op on the current
/// corpus — kept tight rather than a large tolerance so a genuine sub-1.0
/// contradiction is never masked.
const GROUNDING_FLOOR_EPS: f64 = 1e-9;

// -----------------------------------------------------------------------------
// The projection-vocabulary RATCHET driver helpers — the inverse-polarity twin
// of the axis-floor helpers above. See `gmeow_slice_quality::gate`'s ratchet
// doc-comment block for the three hard-fail invariants these back.
// -----------------------------------------------------------------------------

/// Project the ontology-resident `gmeow:ProjectionCeilingCommitment` set into the
/// `(slice IRI, vocab prefix) -> count` map every ratchet pass (count gate,
/// monotonicity, grandfather) reads. The rubric loader (Task 4) already enforces
/// `(slice, vocab)` uniqueness across the loaded commitments, so this is a plain
/// projection — no collision handling needed here.
fn ceilings_from_rubric(rubric: &Rubric) -> std::collections::BTreeMap<(String, String), u64> {
    rubric
        .floors
        .ceilings
        .iter()
        .map(|c| ((c.slice.clone(), c.vocab_prefix.clone()), c.count))
        .collect()
}

/// List every path `git ls-tree -r --name-only <base> -- <rel_dir>` reports —
/// mirrors [`git_show_base`]'s `Command` style (local, no network,
/// `current_dir(root)`, `LC_ALL=C`). `git ls-tree` does not error on a pathspec
/// that matches nothing, so a `rel_dir` absent at `base` (a genuinely new slice)
/// yields empty stdout with a SUCCESSFUL exit — `Ok(vec![])`, which the
/// grandfather reconstruction ([`measure_base_residues`]) treats as "no surface
/// texts at base," i.e. base measured 0. A non-zero exit means git itself could
/// not answer the question and is a HARD-FAIL, never a silent "nothing there."
fn git_ls_tree(root: &Path, base: &str, rel_dir: &str) -> gmeow_errors::Result<Vec<String>> {
    let out = std::process::Command::new("git")
        .current_dir(root)
        .env("LC_ALL", "C")
        .args(["ls-tree", "-r", "--name-only", base, "--", rel_dir])
        .output()
        .map_err(|e| {
            sqe(format!(
                "could not run `git ls-tree {base} -- {rel_dir}`: {e}"
            ))
        })?;
    if !out.status.success() {
        return Err(sqe(format!(
            "`git ls-tree {base} -- {rel_dir}` failed ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Whether `rel_path` (a repo-relative path as `git ls-tree` reports it) belongs
/// to the ratchet's authoring surface — the SAME surface set
/// `gmeow_slice_quality::ratchet_surface_paths` scans on the working tree:
/// basename `module.ttl` or `shapes.ttl`, or any `.ttl` under a `mappings/`
/// directory.
fn is_ratchet_surface(rel_path: &str) -> bool {
    let basename = rel_path.rsplit('/').next().unwrap_or(rel_path);
    basename == "module.ttl"
        || basename == "shapes.ttl"
        || (rel_path.contains("/mappings/") && rel_path.ends_with(".ttl"))
}

/// Reconstruct the ungrounded residue AT THE MERGE BASE for exactly the (slice,
/// vocab) cells whose slice appears in `needed` — the slices whose committed
/// projection ceiling is NEW in the working tree (ratchet invariant 3, the
/// grandfather gate). For each discovered (working-tree) slice dir whose
/// `gmeow_slice_quality::slice_iri_of_dir` is in `needed`: list its base fileset
/// via [`git_ls_tree`], keep only [`is_ratchet_surface`] paths, and read each via
/// [`git_show_base`] — a surface ABSENT at base is skipped (a genuinely new file,
/// not an error); any OTHER git failure is a HARD-FAIL, never silently treated as
/// absent. A slice with no surface texts at base (the whole slice directory is
/// new) contributes NOTHING (base residue 0 for every vocab, via the caller's
/// `unwrap_or(0)`). This feeds the SAME `gmeow_slice_quality::counting::residue`
/// counter base bytes instead of working-tree files
/// (`gmeow_slice_quality::residue_over_texts`), so "measured" can never diverge
/// between the working-tree gate and this base reconstruction.
///
/// # Errors
/// HARD-FAILS on any `git` failure other than a legitimately-absent path/dir
/// (propagated from [`git_ls_tree`] / [`git_show_base`]), or on a Turtle
/// parse/merge failure of a present base surface (propagated from
/// `gmeow_slice_quality::residue_over_texts`).
fn measure_base_residues(
    root: &Path,
    base: &str,
    vocabularies: &[gmeow_slice_quality::model::ProjectionVocabulary],
    needed: &std::collections::BTreeSet<String>,
) -> gmeow_errors::Result<std::collections::BTreeMap<(String, String), u64>> {
    let mut out = std::collections::BTreeMap::new();
    for dir in gmeow_slice_quality::discover_slice_dirs(&root.join("slices")) {
        let slice_iri = gmeow_slice_quality::slice_iri_of_dir(&dir)?;
        if !needed.contains(&slice_iri) {
            continue;
        }
        let rel_dir = dir
            .strip_prefix(root)
            .map_err(|e| sqe(format!("failed to strip prefix {root:?} from {dir:?}: {e}")))?
            .to_string_lossy()
            .replace('\\', "/");
        let entries = git_ls_tree(root, base, &rel_dir)?;
        let mut texts: Vec<String> = Vec::new();
        for rel in entries.iter().filter(|p| is_ratchet_surface(p)) {
            match git_show_base(root, base, rel) {
                BaseFile::Absent => {}
                BaseFile::Error(e) => return Err(sqe(e)),
                BaseFile::Contents(text) => texts.push(text),
            }
        }
        if texts.is_empty() {
            continue; // the slice is new at base → contributes 0 to every vocab
        }
        for vocab in vocabularies {
            let r = gmeow_slice_quality::residue_over_texts(&texts, vocab, &slice_iri)?;
            if r > 0 {
                out.insert((slice_iri.clone(), vocab.prefix.clone()), r);
            }
        }
    }
    // The repo-level dsl/mappings/ surface (attributed to the DSL surface IRI) is not
    // under any slice dir, so reconstruct its base residue separately — the same
    // recursive `/mappings/` surfaces `is_ratchet_surface` matches, read at base — so a
    // NEW dsl-surface ceiling can be grandfathered against real base residue.
    if needed.contains(gmeow_slice_quality::DSL_MAPPING_SURFACE_IRI) {
        let entries = git_ls_tree(root, base, "dsl/mappings")?;
        let mut texts: Vec<String> = Vec::new();
        for rel in entries.iter().filter(|p| is_ratchet_surface(p)) {
            match git_show_base(root, base, rel) {
                BaseFile::Absent => {}
                BaseFile::Error(e) => return Err(sqe(e)),
                BaseFile::Contents(text) => texts.push(text),
            }
        }
        if !texts.is_empty() {
            for vocab in vocabularies {
                let r = gmeow_slice_quality::residue_over_texts(
                    &texts,
                    vocab,
                    gmeow_slice_quality::DSL_MAPPING_SURFACE_IRI,
                )?;
                if r > 0 {
                    out.insert(
                        (
                            gmeow_slice_quality::DSL_MAPPING_SURFACE_IRI.to_owned(),
                            vocab.prefix.clone(),
                        ),
                        r,
                    );
                }
            }
        }
    }
    Ok(out)
}

/// Resolve the committed floor for one `(slice, axis)` grade: the explicit
/// `gmeow:AxisFloorCommitment` floor if one is recorded, else — ONLY for
/// `axisGmn1Coverage` on a grounding slice — the total-coverage `1.0` default, else
/// `None` (unfloored → advisory). This is the SOLE site the grounding `1.0` default
/// is applied; no other axis carries an implicit floor.
///
/// # Errors
/// A HARD FAIL (.goals no-optionality) when a grounding slice carries an
/// explicit `axisGmn1Coverage` commitment BELOW `1.0`. A grounding slice's GMN1
/// coverage floor is definitionally `1.0` (total coverage is what makes it a
/// grounding slice); an explicit commitment may only restate that `1.0`, never
/// undercut it. Silently clamping to `1.0` (a `max()`) would itself be a
/// papering-over optionality violation, so a contradictory sub-1.0 commitment
/// is surfaced as an error instead of silently overridden.
fn axis_floor_for(
    axis_floors: &std::collections::BTreeMap<(String, String), f64>,
    slice: &str,
    axis_local: &str,
    is_grounding: bool,
) -> gmeow_errors::Result<Option<f64>> {
    let key = (slice.to_owned(), axis_local.to_owned());
    if is_grounding && axis_local == AXIS_GMN1_COVERAGE {
        if let Some(explicit) = axis_floors.get(&key)
            && *explicit < 1.0 - GROUNDING_FLOOR_EPS
        {
            return Err(sqe(format!(
                "grounding slice {slice} commits an axisGmn1Coverage floor {explicit:.6} < 1.0 \
                 — a grounding slice's GMN1 coverage floor is definitionally 1.0; this undercuts \
                 the total-coverage gate"
            )));
        }
        return Ok(Some(1.0));
    }
    Ok(axis_floors.get(&key).copied())
}

/// Project the ontology-resident `gmeow:SliceTierFloor` set into the
/// `slice IRI → TierFloor` map the roll-up-tier ratchet and the tier-floor
/// monotonicity check consume, resolving each `gmeow:floorTier` against the rubric
/// ladder for its rank.
///
/// # Errors
/// A HARD FAIL (.goals no-optionality) when a tier floor names a `gmeow:floorTier`
/// that resolves to no `gmeow:QualityTier` in the loaded ladder — the gate never
/// silently drops a floor it cannot rank.
fn tier_floors_from_rubric(
    rubric: &Rubric,
) -> gmeow_errors::Result<std::collections::BTreeMap<String, gmeow_slice_quality::gate::TierFloor>>
{
    let mut out = std::collections::BTreeMap::new();
    for tf in &rubric.floors.tier_floors {
        let Some(tier) = rubric.standard.tier(&tf.tier) else {
            return Err(sqe(format!(
                "tier floor for slice {} names tier {} that resolves to no gmeow:QualityTier in the rubric ladder",
                tf.slice, tf.tier
            )));
        };
        out.insert(
            tf.slice.clone(),
            gmeow_slice_quality::gate::TierFloor {
                rank: tier.rank,
                local: axis_local_name(&tf.tier).to_owned(),
            },
        );
    }
    Ok(out)
}

/// Parse rubric-module Turtle TEXT (a `git show <base>:module.ttl` blob) through the
/// SAME rubric loader the working tree uses, so the base and working floor sets are
/// projected identically for the monotonicity diff. `source_label` names the origin
/// (a `<base>:<file>` git spec) in any parse/freeze error.
///
/// # Errors
/// A HARD FAIL when the base module text cannot be parsed/frozen or is not a
/// structurally-complete rubric — the gate never compares against an unreadable base.
fn load_rubric_from_ttl(text: &str, source_label: &str) -> gmeow_errors::Result<Rubric> {
    let ds = purrdf::parse_dataset(text.as_bytes(), "text/turtle", None)
        .map_err(|e| sqe(format!("{source_label}: parse failed: {e}")))?;
    let mut b = purrdf::RdfDatasetBuilder::new();
    b.push_dataset(&ds);
    let frozen = b
        .freeze()
        .map_err(|e| sqe(format!("{source_label}: dataset freeze failed: {e}")))?;
    gmeow_slice_quality::rubric::load_rubric(&frozen)
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

/// Which axes the seeder targets: exactly one named rubric axis, or every rubric
/// axis a slice grades. EXACTLY ONE of `--axis`/`--all-axes` selects this — neither
/// nor both is a hard error, never a silent default.
#[derive(Clone, Copy)]
enum SeedSelector<'a> {
    /// `--axis <axis-local>`: seed only the one named rubric axis.
    One(&'a str),
    /// `--all-axes`: seed every rubric axis a slice grades that lacks a floor.
    All,
}

/// Render a measured `AxisGrade.score` as an `xsd:decimal` lexical the rubric loader
/// accepts, at FULL f64 precision via Rust's `{}` Display (the shortest
/// round-tripping decimal). Display prints an integer-valued float as `1` / `0`, so
/// a fractionless render gets a `.0` appended — this both parses as a decimal and
/// matches the on-disk convention (`gmeow:floorValue 1.0`). `parse::<f64>()` of the
/// result equals `score` exactly, so the seeded value satisfies the gate's
/// `measured + f64::EPSILON >= floor` at the same live measurement.
fn format_floor_value(score: f64) -> String {
    let s = format!("{score}");
    if s.contains('.') { s } else { format!("{s}.0") }
}

/// Render one `gmeow:AxisFloorCommitment` TTL line in the exact on-disk format the
/// gate reads and the human pastes into `module.ttl`: subject `gmeow:afc-<sliceLocal>-
/// <axisLocal>` (where `<sliceLocal>` is the last path segment of the slice IRI), the
/// full slice IRI in angle brackets, the `gmeow:`-prefixed axis local, and the
/// measured score at full precision.
fn format_floor_line(slice_iri: &str, axis_local: &str, score: f64) -> String {
    let slice_local = axis_local_name(slice_iri);
    format!(
        "gmeow:afc-{slice_local}-{axis_local} a gmeow:AxisFloorCommitment ; rdfs:label \"axis-floor commitment — {slice_local} / {axis_local}\"@x-gmeow-english ; skos:definition \"The committed raise-only measured-score floor for the {axis_local} quality axis on the {slice_local} slice; the gate reds if the slice's measured score falls below it.\"@x-gmeow-english ; rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/slice-quality-rubric> ; gmeow:graphBoxRole gmeow:boxABox ; gmeow:floorSlice <{slice_iri}> ; gmeow:floorAxis gmeow:{axis_local} ; gmeow:floorValue {} .",
        format_floor_value(score)
    )
}

/// The pure seeding pass: over every scored slice assessment, emit one floor line per
/// selected `(slice, axis)` whose axis is NOT already committed for that slice, at the
/// live measured score. Deterministically ordered by (slice IRI, axis local).
///
/// REFUSE TO LOWER: a selected `(slice, axis)` that ALREADY has a committed floor is
/// never re-emitted (no overwrite); but if its live measured score is BELOW the
/// committed floor, that is a real regression the gate already reds — a HARD FAIL
/// here, so the seeder never masks it. (Normally `--axis`/`--all-axes` target only
/// UNfloored pairs; this guards a re-run against an already-floored axis.)
///
/// # Errors
/// The `Err` is a hard-fail message naming the regressing `(slice, axis)` and its
/// measured/floor pair — the seeder emits nothing when any target regresses.
fn collect_seed_lines(
    assessments: &[&SliceAssessment],
    committed: &std::collections::BTreeMap<(String, String), f64>,
    selector: SeedSelector<'_>,
) -> gmeow_errors::Result<Vec<String>> {
    use gmeow_slice_quality::gate::{AxisRatchetVerdict, evaluate_axis_floor};
    // (slice IRI, axis local) → line, so the output is deterministically ordered by
    // that key regardless of assessment/grade iteration order.
    let mut out: std::collections::BTreeMap<(String, String), String> =
        std::collections::BTreeMap::new();
    for a in assessments {
        for grade in &a.grades {
            let axis_local = axis_local_name(&grade.axis_iri);
            let wanted = match selector {
                SeedSelector::One(name) => axis_local == name,
                SeedSelector::All => true,
            };
            if !wanted {
                continue;
            }
            let key = (a.slice.clone(), axis_local.to_owned());
            if let Some(&floor) = committed.get(&key) {
                // Already floored: never overwrite. A live score below the committed
                // floor is a regression the gate reds — hard-fail, do not emit.
                if matches!(
                    evaluate_axis_floor(grade.score, floor),
                    AxisRatchetVerdict::MeasuredBelowFloor
                ) {
                    return Err(sqe(format!(
                        "slice-quality-seed-floors: {} measures {axis_local} {} — BELOW its already-committed floor {floor}; this is a regression the gate reds. Refusing to emit (a floored axis is never re-seeded; raise a floor only by a deliberate hand-edit of the individual, never a seeder re-run).",
                        a.slice, grade.score
                    )));
                }
                continue; // already floored → nothing to seed for this pair
            }
            out.insert(key, format_floor_line(&a.slice, axis_local, grade.score));
        }
    }
    Ok(out.into_values().collect())
}

/// `gmeow-dev slice-quality-seed-floors` — emit `gmeow:AxisFloorCommitment` TTL for
/// the live measured scores, so a human can seed a NEW axis's floors at the actual
/// live measurement and paste them into
/// `slices/core/slice-quality-rubric/module.ttl`.
///
/// EXACTLY ONE of `--axis <axis-local>` (seed the one named rubric axis) or
/// `--all-axes` (seed every rubric axis a slice grades that lacks a floor) must be
/// given — neither nor both is a hard error, never a silent default. The score used
/// is the SAME single-score pass the gate reads (`score_slices_with_rubric` over every
/// discovered slice), so what is seeded is exactly what the gate enforces.
///
/// ONE-SHOT per axis: this seeds a NEW axis's floors ONCE. Re-running to "refresh" an
/// already-floored axis is forbidden — a dropped score would red monotonicity and a
/// risen score would silently ratchet the floor up (banned auto-calibration). Raising
/// a floor later is a deliberate hand-edit of the individual, never a seeder re-run.
/// The command is emit-only: it writes TTL to stdout; the human commits it.
pub fn slice_quality_seed_floors(axis: Option<&str>, all_axes: bool) -> i32 {
    // EXACTLY ONE selector — neither nor both is a hard error (no silent default).
    let selector = match (axis, all_axes) {
        (Some(a), false) => SeedSelector::One(a),
        (None, true) => SeedSelector::All,
        (None, false) => {
            return fail(
                "slice-quality-seed-floors: exactly one of --axis <axis-local> or --all-axes is required (got neither)",
            );
        }
        (Some(_), true) => {
            return fail(
                "slice-quality-seed-floors: --axis and --all-axes are mutually exclusive — pass exactly one",
            );
        }
    };

    let root = project_root();
    let rubric = match repo_rubric(&root) {
        Ok(r) => r,
        Err(e) => return fail(format!("slice-quality-seed-floors: {e}")),
    };

    // A `--axis` that names no rubric axis is a HARD FAIL, never silent empty output.
    if let SeedSelector::One(name) = selector {
        let known: Vec<String> = rubric
            .standard
            .axes
            .iter()
            .map(|a| axis_local_name(&a.iri).to_owned())
            .collect();
        if !known.iter().any(|k| k == name) {
            let mut rungs = known;
            rungs.sort();
            return fail(format!(
                "slice-quality-seed-floors: unknown --axis {name:?} (want one of: {})",
                rungs.join(", ")
            ));
        }
    }

    // The SAME single-score pass the gate reads: score every discovered slice once,
    // in deterministic dir order, through the shared rubric.
    let committed = match axis_floors_from_rubric(&rubric) {
        Ok(m) => m,
        Err(e) => return fail(format!("slice-quality-seed-floors: {e}")),
    };
    let dirs = gmeow_slice_quality::discover_slice_dirs(&root.join("slices"));
    let score_results = gmeow_slice_quality::score_slices_with_rubric(&root, &dirs, &rubric);
    let mut assessments: Vec<SliceAssessment> = Vec::with_capacity(dirs.len());
    for (dir, result) in dirs.iter().zip(score_results) {
        match result {
            Ok(report) => assessments.push(report.assessment),
            // A slice that cannot be scored is a hard fail — never a silent skip that
            // would seed an incomplete floor set.
            Err(e) => return fail(format!("slice-quality-seed-floors: {}: {e}", dir.display())),
        }
    }
    let refs: Vec<&SliceAssessment> = assessments.iter().collect();

    let lines = match collect_seed_lines(&refs, &committed, selector) {
        Ok(l) => l,
        Err(e) => return fail(e),
    };

    // A short comment header (no issue/PR numbers) — then only the TTL lines.
    let scope = match selector {
        SeedSelector::One(name) => name.to_owned(),
        SeedSelector::All => "all unfloored axes".to_owned(),
    };
    println!(
        "# seeded gmeow:AxisFloorCommitment individuals for axis {scope} — paste into {RUBRIC_MODULE}"
    );
    for line in &lines {
        println!("{line}");
    }
    0
}

/// Render one `gmeow:ProjectionCeilingCommitment` TTL line in the exact on-disk
/// format the gate reads and the human pastes into `module.ttl`: subject
/// `gmeow:pcc-<sliceLocal>-<vocabPrefix>` (where `<sliceLocal>` is the last path
/// segment of the slice IRI), the full slice IRI in angle brackets, the
/// `gmeow:projVocab-<vocabPrefix>` vocabulary reference, and the measured residue —
/// the inverse-polarity mirror of [`format_floor_line`].
fn format_ceiling_line(slice_iri: &str, vocab_prefix: &str, count: u64) -> String {
    let slice_local = axis_local_name(slice_iri);
    format!(
        "gmeow:pcc-{slice_local}-{vocab_prefix} a gmeow:ProjectionCeilingCommitment ; rdfs:label \"projection-ceiling commitment — {slice_local} / {vocab_prefix}\"@x-gmeow-english ; skos:definition \"The committed lower-only ungrounded-residue ceiling for the {vocab_prefix} projection vocabulary on the {slice_local} slice; the gate reds if the slice's measured residue rises above it.\"@x-gmeow-english ; rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/slice-quality-rubric> ; gmeow:graphBoxRole gmeow:boxABox ; gmeow:ceilingSlice <{slice_iri}> ; gmeow:ceilingVocabulary gmeow:projVocab-{vocab_prefix} ; gmeow:ceilingCount {count} ."
    )
}

/// `gmeow-dev slice-quality-seed-ceilings` — emit `gmeow:ProjectionCeilingCommitment`
/// TTL at the CURRENT measured ungrounded residue for every (slice, guarded
/// projection-vocabulary) pair with nonzero residue, so a human can grandfather the
/// existing residue and paste it into
/// `slices/core/slice-quality-rubric/module.ttl`.
///
/// Reads the guarded vocabulary registry off the loaded rubric
/// (`rubric.floors.vocabularies` — the ontology-resident set Task 2 seeded) and
/// measures every discovered slice against it through
/// `gmeow_slice_quality::measure_repo_residues`, the SAME shared counter the ratchet
/// gate reads — seed and gate can never diverge on what "measured" means.
///
/// EMIT-ONLY, GRANDFATHER-ONCE: this seeds the ceiling ABox at whatever residue is
/// live the moment it is run. Re-running it to "refresh" a ceiling whose measured
/// residue has since RISEN is a banned auto-calibration — the correct response to a
/// risen residue is the gate reading, never a re-seed that raises the ceiling to
/// match. Lowering a ceiling later, after a genuine measured migration grounds
/// constructs out of the residue, is always a deliberate hand-edit of the
/// individual, never a seeder re-run. The command writes TTL to stdout only; the
/// human commits it.
pub fn slice_quality_seed_ceilings() -> i32 {
    let root = project_root();
    let rubric = match repo_rubric(&root) {
        Ok(r) => r,
        Err(e) => return fail(format!("slice-quality-seed-ceilings: {e}")),
    };

    // The guarded set must be loaded (Task 2's ontology-resident registry) — an
    // empty set here means the registry failed to load, never a legitimate "guard
    // nothing" state (.goals no-optionality).
    let vocabularies = rubric.floors.vocabularies;
    if vocabularies.is_empty() {
        return fail(
            "slice-quality-seed-ceilings: no gmeow:ProjectionVocabulary individuals loaded from the rubric — the guarded projection-vocabulary registry must be loaded before ceilings can be seeded",
        );
    }

    // The SAME shared counter the ratchet gate reads — seed and gate can never
    // diverge on what "measured" means.
    let residues = match gmeow_slice_quality::measure_repo_residues(&root, &vocabularies) {
        Ok(r) => r,
        Err(e) => return fail(format!("slice-quality-seed-ceilings: {e}")),
    };

    // Sort deterministically by the emitted individual's IRI (not merely by the
    // BTreeMap's (slice IRI, vocab prefix) key order, which can diverge from
    // sorting by slice LOCAL name once two slices' full IRIs and local names order
    // differently).
    let mut entries: Vec<(String, String)> = residues
        .into_iter()
        .map(|((slice_iri, vocab_prefix), count)| {
            let pcc_iri = format!("gmeow:pcc-{}-{vocab_prefix}", axis_local_name(&slice_iri));
            (
                pcc_iri,
                format_ceiling_line(&slice_iri, &vocab_prefix, count),
            )
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    // A short comment header (no issue/PR numbers) — then only the TTL lines.
    println!("# seeded gmeow:ProjectionCeilingCommitment individuals — paste into {RUBRIC_MODULE}");
    for (_, line) in &entries {
        println!("{line}");
    }
    0
}

/// `gmeow-dev slice-quality-projection-debt` — a live migration dashboard over the
/// projection-vocabulary ratchet: for every (slice, guarded vocab) with either a
/// LIVE measured ungrounded residue or a committed ceiling, print the measured
/// count, the effective ceiling, and the headroom between them.
///
/// `measured` is computed on every run through
/// `gmeow_slice_quality::measure_repo_residues` — the SAME shared counter the
/// ratchet gate reads — so this report can never diverge from what the gate would
/// see. It is NEVER persisted as a `SoundUnder` projection: unlike the committed
/// ceiling ABox, a live scan result is entailed by no resident individual, so
/// folding it into the bundle as a projection would be a false loss judgment (the
/// pipeline's `projection_ceilings` stage folds only the resident ceiling/
/// vocabulary TSVs, never this scan). REPORT-ONLY: this command always exits 0 —
/// it never gates `make check` (that is `slice-quality-gate`'s job) — and its
/// output is never fed back into a ceiling; a ceiling is lowered only by a
/// deliberate hand-edit of the committed individual after a genuine measured
/// migration, never by tuning it toward this report's numbers.
pub fn slice_quality_projection_debt() -> i32 {
    let root = project_root();
    let rubric = match repo_rubric(&root) {
        Ok(r) => r,
        Err(e) => return fail(format!("slice-quality-projection-debt: {e}")),
    };

    let vocabularies = rubric.floors.vocabularies;
    if vocabularies.is_empty() {
        return fail(
            "slice-quality-projection-debt: no gmeow:ProjectionVocabulary individuals loaded from the rubric — the guarded projection-vocabulary registry must be loaded before residue can be measured",
        );
    }
    let ceilings = rubric.floors.ceilings;

    // The SAME shared counter the ratchet gate reads — this report can never
    // diverge from what the gate would see.
    let measured = match gmeow_slice_quality::measure_repo_residues(&root, &vocabularies) {
        Ok(m) => m,
        Err(e) => return fail(format!("slice-quality-projection-debt: {e}")),
    };

    // Every (slice, vocab) cell with EITHER a measured residue OR a committed
    // ceiling — the union of the two key sets, sorted by (slice, vocab).
    let mut cells: std::collections::BTreeSet<(String, String)> =
        measured.keys().cloned().collect();
    for ceiling in &ceilings {
        cells.insert((ceiling.slice.clone(), ceiling.vocab_prefix.clone()));
    }

    println!("slice\tvocab\tmeasured\tceiling\theadroom");
    let mut total_measured: u64 = 0;
    let mut total_headroom: i64 = 0;
    let mut at_ceiling: u64 = 0;
    for (slice, vocab_prefix) in &cells {
        let measured_count = measured
            .get(&(slice.clone(), vocab_prefix.clone()))
            .copied()
            .unwrap_or(0);
        let default_ceiling = vocabularies
            .iter()
            .find(|v| &v.prefix == vocab_prefix)
            .map_or(0, |v| v.default_ceiling);
        let ceiling_count = ceilings
            .iter()
            .find(|c| &c.slice == slice && &c.vocab_prefix == vocab_prefix)
            .map_or(default_ceiling, |c| c.count);
        let headroom = i64::try_from(ceiling_count).unwrap_or(i64::MAX)
            - i64::try_from(measured_count).unwrap_or(i64::MAX);

        println!("{slice}\t{vocab_prefix}\t{measured_count}\t{ceiling_count}\t{headroom}");

        total_measured += measured_count;
        total_headroom += headroom;
        if headroom == 0 {
            at_ceiling += 1;
        }
    }
    println!(
        "# total measured={total_measured} total headroom={total_headroom} at-ceiling={at_ceiling} cells={}",
        cells.len()
    );
    0
}

#[cfg(test)]
mod min_tier_tests {
    use super::*;

    /// A five-rung ladder mirroring the shipped rubric (Registered..Maximal).
    fn ladder() -> MeasurementStandard {
        let rung = |local: &str, label: &str, rank: i64| Tier {
            iri: format!("{}{local}", gmeow_slice_quality::model::GMEOW),
            label: label.to_owned(),
            rank,
        };
        MeasurementStandard {
            tiers: vec![
                rung("tierRegistered", "Registered", 0),
                rung("tierGrounded", "Grounded", 1),
                rung("tierLinked", "Linked", 2),
                rung("tierExemplified", "Exemplified", 3),
                rung("tierMaximal", "Maximal", 4),
            ],
            axes: vec![],
        }
    }

    fn tier(r: &MeasurementStandard, label: &str) -> Tier {
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
mod floor_projection_tests {
    use super::*;
    use gmeow_slice_quality::gate::{
        AxisRatchetVerdict, axis_floor_monotonicity, evaluate_axis_floor, tier_floor_monotonicity,
    };

    const NS: &str = "https://blackcatinformatics.ca/gmeow/";

    /// A structurally-complete minimal rubric TTL (one two-rung ladder, one axis with
    /// a threshold) with `body` appended — the same scaffolding the rubric loader
    /// needs, used to exercise the floor projections through `load_rubric_from_ttl`
    /// exactly as the base-`module.ttl` monotonicity path does.
    fn mini_rubric(body: &str) -> String {
        format!(
            r#"@prefix gmeow: <{NS}> .
gmeow:tierRegistered a gmeow:QualityTier ; gmeow:tierRank 0 .
gmeow:tierGrounded a gmeow:QualityTier ; gmeow:tierRank 1 .
gmeow:axisGmn1Coverage a gmeow:QualityAxis ;
    gmeow:axisProducer "gmn1_coverage_axis" ;
    gmeow:axisDimension gmeow:dimGmn ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ;
    gmeow:axisThreshold gmeow:thrGmn .
gmeow:thrGmn a gmeow:AxisThreshold ;
    gmeow:thresholdTier gmeow:tierRegistered ;
    gmeow:thresholdFloor 0.0 .
{body}
"#
        )
    }

    #[test]
    fn axis_floors_project_from_commitments() {
        // The (slice, axis-local) → floor projection carries the committed value.
        let rubric = load_rubric_from_ttl(
            &mini_rubric(
                r#"gmeow:afc a gmeow:AxisFloorCommitment ; gmeow:floorSlice gmeow:sliceX ; gmeow:floorAxis gmeow:axisGmn1Coverage ; gmeow:floorValue 0.9954337899543378 ."#,
            ),
            "test",
        )
        .unwrap();
        let map = axis_floors_from_rubric(&rubric).unwrap();
        assert_eq!(
            map.get(&(format!("{NS}sliceX"), "axisGmn1Coverage".to_owned()))
                .copied(),
            Some(0.9954337899543378)
        );
    }

    #[test]
    fn axis_floors_from_rubric_hard_fails_on_global_axis_local_name_collision() {
        // `axis_floors_from_rubric` first enforces that every rubric axis's local name
        // is GLOBALLY UNIQUE across `rubric.axes` — the floor gate keys every lookup
        // by local name, so two DISTINCT axis IRIs sharing a local name would let a
        // commitment against one axis silently apply to the other's grade. Positive
        // control first: two DISTINCT local names for the same slice map to two
        // entries with no collision at all.
        let clean = load_rubric_from_ttl(
            &mini_rubric(
                r#"<https://a.example/ns#axisFoo> a gmeow:QualityAxis ;
    gmeow:axisProducer "syn_foo_a" ; gmeow:axisDimension gmeow:dimSyn ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ; gmeow:axisThreshold gmeow:thrSynFooA .
gmeow:thrSynFooA a gmeow:AxisThreshold ; gmeow:thresholdTier gmeow:tierRegistered ; gmeow:thresholdFloor 0.0 .
<https://a.example/ns#axisBar> a gmeow:QualityAxis ;
    gmeow:axisProducer "syn_bar_a" ; gmeow:axisDimension gmeow:dimSyn ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ; gmeow:axisThreshold gmeow:thrSynBarA .
gmeow:thrSynBarA a gmeow:AxisThreshold ; gmeow:thresholdTier gmeow:tierRegistered ; gmeow:thresholdFloor 0.0 .
gmeow:afc1 a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceX ; gmeow:floorAxis <https://a.example/ns#axisFoo> ;
    gmeow:floorValue 0.9 .
gmeow:afc2 a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceX ; gmeow:floorAxis <https://a.example/ns#axisBar> ;
    gmeow:floorValue 0.5 ."#,
            ),
            "test",
        )
        .unwrap();
        let clean_map = axis_floors_from_rubric(&clean).unwrap();
        assert_eq!(
            clean_map
                .get(&(format!("{NS}sliceX"), "axisFoo".to_owned()))
                .copied(),
            Some(0.9)
        );
        assert_eq!(
            clean_map
                .get(&(format!("{NS}sliceX"), "axisBar".to_owned()))
                .copied(),
            Some(0.5)
        );

        // Now the collision: two DISTINCT rubric AXES (not merely commitments) share
        // the SAME local name `axisFoo` across two different namespaces. This must
        // hard-fail at the axis level, independent of any commitment against either
        // axis, because the floor gate would otherwise key lookups on the shared
        // local name and could apply a commitment to the wrong axis's grade.
        let colliding = load_rubric_from_ttl(
            &mini_rubric(
                r#"<https://a.example/ns#axisFoo> a gmeow:QualityAxis ;
    gmeow:axisProducer "syn_foo_a" ; gmeow:axisDimension gmeow:dimSyn ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ; gmeow:axisThreshold gmeow:thrSynFooA .
gmeow:thrSynFooA a gmeow:AxisThreshold ; gmeow:thresholdTier gmeow:tierRegistered ; gmeow:thresholdFloor 0.0 .
<https://b.example/other#axisFoo> a gmeow:QualityAxis ;
    gmeow:axisProducer "syn_foo_b" ; gmeow:axisDimension gmeow:dimSyn ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ; gmeow:axisThreshold gmeow:thrSynFooB .
gmeow:thrSynFooB a gmeow:AxisThreshold ; gmeow:thresholdTier gmeow:tierRegistered ; gmeow:thresholdFloor 0.0 .
gmeow:afc1 a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceX ; gmeow:floorAxis <https://a.example/ns#axisFoo> ;
    gmeow:floorValue 0.9 .
gmeow:afc2 a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceX ; gmeow:floorAxis <https://b.example/other#axisFoo> ;
    gmeow:floorValue 0.5 ."#,
            ),
            "test",
        )
        .unwrap();
        let err = axis_floors_from_rubric(&colliding).unwrap_err();
        assert!(
            err.message().contains("https://a.example/ns#axisFoo")
                && err.message().contains("https://b.example/other#axisFoo"),
            "names both colliding full axis IRIs: {err}"
        );
        assert!(
            err.message().contains("axisFoo"),
            "names the shared local name: {err}"
        );
    }

    #[test]
    fn tier_floors_project_and_resolve_rank() {
        let rubric = load_rubric_from_ttl(
            &mini_rubric(
                r#"gmeow:stf a gmeow:SliceTierFloor ; gmeow:floorSlice gmeow:sliceX ; gmeow:floorTier gmeow:tierGrounded ."#,
            ),
            "test",
        )
        .unwrap();
        let map = tier_floors_from_rubric(&rubric).unwrap();
        let f = map.get(&format!("{NS}sliceX")).unwrap();
        assert_eq!(f.rank, 1, "tierGrounded resolves to rank 1");
        assert_eq!(f.local, "tierGrounded");
    }

    #[test]
    fn tier_floor_naming_unknown_tier_hard_fails() {
        // A gmeow:floorTier that resolves to no ladder rung is a hard fail — the gate
        // never silently drops a floor it cannot rank. Since Gap 4 the rubric LOADER
        // already rejects an unknown gmeow:floorTier at load time, so this case can no
        // longer be reached through `load_rubric_from_ttl`. The `tier_floors_from_rubric`
        // guard is now defense-in-depth behind that load-time validation, so this test
        // builds a `Rubric` struct literal directly (bypassing the loader) to still
        // exercise the guard itself.
        use gmeow_slice_quality::model::{GovernanceFloors, SliceTierFloorCommitment};

        let rubric = Rubric {
            standard: MeasurementStandard {
                tiers: vec![Tier {
                    iri: format!("{NS}tierRegistered"),
                    label: "Registered".to_owned(),
                    rank: 0,
                }],
                axes: Vec::new(),
            },
            floors: GovernanceFloors {
                exemptions: Vec::new(),
                commitments: Vec::new(),
                tier_floors: vec![SliceTierFloorCommitment {
                    slice: format!("{NS}sliceX"),
                    tier: format!("{NS}tierBogus"),
                }],
                ..Default::default()
            },
        };
        let err = tier_floors_from_rubric(&rubric).unwrap_err();
        assert!(err.message().contains("tierBogus"), "names the tier: {err}");
        assert!(
            err.message().contains("resolves to no gmeow:QualityTier"),
            "{err}"
        );
    }

    #[test]
    fn non_gmn1_axis_floor_is_enforced() {
        // (a) A committed floor on an axis OTHER than axisGmn1Coverage is resolved
        // and enforced: an explicit floor is found regardless of grounding, and a
        // measured score below it fails.
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            ("ex:slice".to_owned(), "axisProseQuality".to_owned()),
            0.80_f64,
        );
        assert_eq!(
            axis_floor_for(&map, "ex:slice", "axisProseQuality", false).unwrap(),
            Some(0.80),
            "an explicit non-GMN1 floor is resolved even off a grounding slice"
        );
        assert_eq!(
            evaluate_axis_floor(0.50, 0.80),
            AxisRatchetVerdict::MeasuredBelowFloor,
            "measured below the non-GMN1 floor fails"
        );
    }

    #[test]
    fn gmn1_grounding_default_holds() {
        // (b) With no explicit commitment, axisGmn1Coverage on a grounding slice is
        // floored at 1.0; on a non-grounding slice it is unfloored; and the 1.0
        // default is applied to NO other axis, even on a grounding slice.
        let empty = std::collections::BTreeMap::new();
        assert_eq!(
            axis_floor_for(&empty, "ex:slice", AXIS_GMN1_COVERAGE, true).unwrap(),
            Some(1.0),
            "grounding GMN1 defaults to 1.0"
        );
        assert_eq!(
            axis_floor_for(&empty, "ex:slice", AXIS_GMN1_COVERAGE, false).unwrap(),
            None,
            "non-grounding GMN1 with no commitment is unfloored"
        );
        assert_eq!(
            axis_floor_for(&empty, "ex:slice", "axisProseQuality", true).unwrap(),
            None,
            "the 1.0 default is GMN1-only, never any other axis"
        );
    }

    #[test]
    fn grounding_gmn1_sub_one_floor_hard_fails() {
        // A grounding slice's axisGmn1Coverage floor is definitionally 1.0; an
        // explicit commitment BELOW 1.0 contradicts that definition and must
        // hard-fail (.goals no-optionality), never be silently clamped up.
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            ("ex:slice".to_owned(), AXIS_GMN1_COVERAGE.to_owned()),
            0.9_f64,
        );
        let err = axis_floor_for(&map, "ex:slice", AXIS_GMN1_COVERAGE, true).unwrap_err();
        assert!(
            err.message().contains("grounding") && err.message().contains("1.0"),
            "names the grounding contradiction and the definitional 1.0: {err}"
        );

        // Positive control: an explicit commitment that RESTATES 1.0 is accepted,
        // not treated as a contradiction.
        let mut map_one = std::collections::BTreeMap::new();
        map_one.insert(
            ("ex:slice".to_owned(), AXIS_GMN1_COVERAGE.to_owned()),
            1.0_f64,
        );
        assert_eq!(
            axis_floor_for(&map_one, "ex:slice", AXIS_GMN1_COVERAGE, true).unwrap(),
            Some(1.0),
            "an explicit 1.0 grounding commitment restates, not undercuts, the default"
        );

        // The guard is grounding-only: the SAME sub-1.0 commitment on a
        // non-grounding slice loads fine — no hard fail.
        assert_eq!(
            axis_floor_for(&map, "ex:slice", AXIS_GMN1_COVERAGE, false).unwrap(),
            Some(0.9),
            "a sub-1.0 GMN1 floor on a non-grounding slice is not a contradiction"
        );
    }

    #[test]
    fn multiple_axes_are_floored_independently_on_one_slice() {
        // (c) Two committed axis floors on the SAME slice are each resolved and
        // evaluated independently — one can fail while the other passes.
        let s = "ex:slice".to_owned();
        let mut map = std::collections::BTreeMap::new();
        map.insert((s.clone(), "axisProseQuality".to_owned()), 0.80_f64);
        map.insert((s.clone(), "axisLinkageCalculus".to_owned()), 0.60_f64);
        assert_eq!(
            axis_floor_for(&map, &s, "axisProseQuality", false).unwrap(),
            Some(0.80)
        );
        assert_eq!(
            axis_floor_for(&map, &s, "axisLinkageCalculus", false).unwrap(),
            Some(0.60)
        );
        // Independent verdicts: prose below its floor fails, linkage above its passes.
        assert!(evaluate_axis_floor(0.70, 0.80).is_failure());
        assert!(!evaluate_axis_floor(0.70, 0.60).is_failure());
    }

    #[test]
    fn axis_floor_monotonicity_reds_on_lowered_commitment_vs_base_ttl() {
        // (d) A working-tree module.ttl that LOWERS a committed per-axis floor below
        // its base-TTL value is a hard violation (floors are raise-only) — parsed and
        // projected through the SAME loader path the gate uses.
        let base = load_rubric_from_ttl(
            &mini_rubric(
                r#"gmeow:afc a gmeow:AxisFloorCommitment ; gmeow:floorSlice gmeow:sliceX ; gmeow:floorAxis gmeow:axisGmn1Coverage ; gmeow:floorValue 0.98 ."#,
            ),
            "base",
        )
        .unwrap();
        let work = load_rubric_from_ttl(
            &mini_rubric(
                r#"gmeow:afc a gmeow:AxisFloorCommitment ; gmeow:floorSlice gmeow:sliceX ; gmeow:floorAxis gmeow:axisGmn1Coverage ; gmeow:floorValue 0.90 ."#,
            ),
            "work",
        )
        .unwrap();
        let base_map = axis_floors_from_rubric(&base).unwrap();
        let work_map = axis_floors_from_rubric(&work).unwrap();
        let out = axis_floor_monotonicity(RUBRIC_MODULE, &base_map, &work_map, |_, _| true);
        assert_eq!(
            out.violations.len(),
            1,
            "the lowered axis floor reds: {out:#?}"
        );
        assert!(
            out.violations[0].contains("axisGmn1Coverage") && out.violations[0].contains("LOWERED"),
            "names the axis and the lowering: {out:#?}"
        );
        // The reverse direction (holding at base) is clean.
        let up = axis_floor_monotonicity(RUBRIC_MODULE, &base_map, &base_map, |_, _| true);
        assert!(up.violations.is_empty());
    }

    #[test]
    fn tier_floor_monotonicity_reds_on_lowered_tier_vs_base_ttl() {
        // (e) A working-tree module.ttl that LOWERS a committed roll-up tier floor
        // (tierGrounded → tierRegistered) is a hard violation (floors are raise-only).
        let base = load_rubric_from_ttl(
            &mini_rubric(
                r#"gmeow:stf a gmeow:SliceTierFloor ; gmeow:floorSlice gmeow:sliceX ; gmeow:floorTier gmeow:tierGrounded ."#,
            ),
            "base",
        )
        .unwrap();
        let work = load_rubric_from_ttl(
            &mini_rubric(
                r#"gmeow:stf a gmeow:SliceTierFloor ; gmeow:floorSlice gmeow:sliceX ; gmeow:floorTier gmeow:tierRegistered ."#,
            ),
            "work",
        )
        .unwrap();
        let base_map = tier_floors_from_rubric(&base).unwrap();
        let work_map = tier_floors_from_rubric(&work).unwrap();
        let out = tier_floor_monotonicity(RUBRIC_MODULE, &base_map, &work_map, |_| true);
        assert_eq!(
            out.violations.len(),
            1,
            "the lowered tier floor reds: {out:#?}"
        );
        assert!(
            out.violations[0].contains("LOWERED")
                && out.violations[0].contains("tierGrounded")
                && out.violations[0].contains("tierRegistered"),
            "names the lowering old → new: {out:#?}"
        );
    }
}

#[cfg(test)]
mod seed_floors_tests {
    use super::*;
    use gmeow_slice_quality::model::{AxisGrade, GMEOW};
    use std::collections::BTreeMap;

    /// A throwaway bottom tier for the grade/roll-up fields the seeder never reads.
    fn tier0() -> Tier {
        Tier {
            iri: format!("{GMEOW}tierRegistered"),
            label: "Registered".to_owned(),
            rank: 0,
        }
    }

    fn grade(axis_local: &str, score: f64) -> AxisGrade {
        AxisGrade {
            axis_iri: format!("{GMEOW}{axis_local}"),
            score,
            tier: tier0(),
        }
    }

    /// A slice assessment keyed by the on-disk slice IRI shape
    /// (`…/gmeow/slices/<local>`), so the emitted subject/`floorSlice` match module.ttl.
    fn assessment(slice_local: &str, grades: Vec<AxisGrade>) -> SliceAssessment {
        SliceAssessment {
            slice: format!("{GMEOW}slices/{slice_local}"),
            grades,
            rollup: tier0(),
        }
    }

    /// Extract the `gmeow:floorValue` decimal lexical from an emitted floor line.
    fn floor_value_of(line: &str) -> &str {
        line.rsplit("gmeow:floorValue ")
            .next()
            .unwrap()
            .trim_end_matches(" .")
    }

    #[test]
    fn emitted_floor_value_equals_live_measured_score() {
        // (a) The seeded floorValue is EXACTLY the live measured AxisGrade.score —
        // what the seeder emits is what the gate reads.
        let score = 0.571_428_571_428_571_4;
        let a = assessment("diagnostics", vec![grade("axisShapeMigration", score)]);
        let committed = BTreeMap::new();
        let lines =
            collect_seed_lines(&[&a], &committed, SeedSelector::One("axisShapeMigration")).unwrap();
        assert_eq!(lines.len(), 1, "one target → one line");
        let line = &lines[0];
        assert!(
            line.starts_with(
                "gmeow:afc-diagnostics-axisShapeMigration a gmeow:AxisFloorCommitment"
            ),
            "subject/type: {line}"
        );
        assert!(
            line.contains(
                "gmeow:floorSlice <https://blackcatinformatics.ca/gmeow/slices/diagnostics>"
            ),
            "full slice IRI: {line}"
        );
        assert!(
            line.contains("gmeow:floorAxis gmeow:axisShapeMigration"),
            "prefixed axis local: {line}"
        );
        let parsed: f64 = floor_value_of(line).parse().unwrap();
        assert_eq!(parsed, score, "emitted value == live measured score");
    }

    #[test]
    fn seeded_value_round_trips_and_satisfies_the_gate() {
        // (b) parse(Display(score)) == score for every score shape, so the seeded
        // floor satisfies the gate's `measured + f64::EPSILON >= floor` at the same
        // live measurement (floor == parsed == score).
        for &score in &[
            0.0_f64,
            1.0,
            0.571_428_571_428_571_4,
            0.995_433_789_954_337_8,
            0.123_456_789,
            1e-9,
        ] {
            let rendered = format_floor_value(score);
            let parsed: f64 = rendered.parse().unwrap();
            assert_eq!(parsed, score, "round trip for {score} rendered {rendered}");
            assert!(
                score + f64::EPSILON >= parsed,
                "gate holds at the seeded floor for {score}"
            );
        }
        // Integer-valued floats gain a `.0` so they parse as xsd:decimal and match the
        // on-disk `1.0`/`0.0` convention (Display would print `1`/`0`).
        assert_eq!(format_floor_value(1.0), "1.0");
        assert_eq!(format_floor_value(0.0), "0.0");
        // A fractional value is rendered at full precision, untouched.
        assert_eq!(format_floor_value(0.5), "0.5");
    }

    #[test]
    fn output_is_deterministically_ordered_by_slice_then_axis() {
        // (c) Assessments and grades fed OUT of order still emit sorted by
        // (slice IRI, axis local).
        let a1 = assessment("zebra", vec![grade("axisB", 0.2), grade("axisA", 0.3)]);
        let a2 = assessment("alpha", vec![grade("axisA", 0.4)]);
        let committed = BTreeMap::new();
        let lines = collect_seed_lines(&[&a1, &a2], &committed, SeedSelector::All).unwrap();
        let subjects: Vec<&str> = lines.iter().map(|l| l.split(' ').next().unwrap()).collect();
        assert_eq!(
            subjects,
            vec![
                "gmeow:afc-alpha-axisA",
                "gmeow:afc-zebra-axisA",
                "gmeow:afc-zebra-axisB",
            ]
        );
    }

    #[test]
    fn already_floored_pair_at_or_above_is_not_re_emitted() {
        // A committed floor the live score still holds is never re-emitted (no
        // overwrite, no silent ratchet) — the seeder is emit-only for UNfloored pairs.
        let a = assessment("diagnostics", vec![grade("axisShapeMigration", 0.90)]);
        let mut committed = BTreeMap::new();
        committed.insert((a.slice.clone(), "axisShapeMigration".to_owned()), 0.80);
        let lines =
            collect_seed_lines(&[&a], &committed, SeedSelector::One("axisShapeMigration")).unwrap();
        assert!(
            lines.is_empty(),
            "already-floored pair is skipped: {lines:?}"
        );
    }

    #[test]
    fn seeding_below_an_already_committed_floor_hard_fails() {
        // (d) A live score BELOW an already-committed floor is a regression the gate
        // reds — the seeder hard-fails and emits nothing, never lowering the floor.
        let a = assessment("diagnostics", vec![grade("axisShapeMigration", 0.40)]);
        let mut committed = BTreeMap::new();
        committed.insert((a.slice.clone(), "axisShapeMigration".to_owned()), 0.80);
        let err = collect_seed_lines(&[&a], &committed, SeedSelector::One("axisShapeMigration"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("regression"), "names the regression: {err}");
        assert!(err.contains("axisShapeMigration"), "names the axis: {err}");
        assert!(err.contains("0.8"), "names the committed floor: {err}");
    }

    #[test]
    fn unknown_axis_name_hard_fails() {
        // (e) `--axis` naming no rubric axis is a hard fail (nonzero exit), never a
        // silent empty emission. The unknown-axis guard fires right after the rubric
        // load, before any slice is scored.
        assert_ne!(
            slice_quality_seed_floors(Some("axisDefinitelyNotAReal_Axis"), false),
            0
        );
    }

    #[test]
    fn neither_selector_hard_fails() {
        // (f) Neither --axis nor --all-axes → hard fail, no silent default.
        assert_ne!(slice_quality_seed_floors(None, false), 0);
    }

    #[test]
    fn both_selectors_hard_fail() {
        // (f) Both --axis and --all-axes → hard fail (mutually exclusive).
        assert_ne!(slice_quality_seed_floors(Some("axisGmn1Coverage"), true), 0);
    }
}
