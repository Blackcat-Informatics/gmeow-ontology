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
// `RUBRIC_MODULE` is the canonical, ontology-resident home of the CENTRALIZED
// rubric authority — the measurement standard (tier ladder + axes) and the
// guarded-vocabulary registry (single defining literal lives in
// `gmeow_slice_quality`; this crate never redeclares it). It anchors the
// merge-base reconstruction ([`base_rubric_at`]) and the seed-command paste
// hints below.
use gmeow_slice_quality::{RUBRIC_MODULE, resolve_min_tier, tier_gate_passes};

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
    let standard = match gmeow_slice_quality::load_repo_rubric(&root) {
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
    let rubric = match gmeow_slice_quality::load_repo_rubric(root) {
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

/// The human-facing source label prefixing floor / ceiling / registry monotonicity
/// violation messages. The messages themselves already name the offending slice / axis /
/// vocabulary; this labels the authoring surface, which is now every slice's `module.ttl`
/// rather than one rubric module.
const GOVERNANCE_SOURCE_LABEL: &str = "governance floors (authored across slices' module.ttl)";

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

/// The `make check` opt-in tier ratchet gate, over the real repository root.
///
/// For every slice that declares `gmeow:sliceQualityTier`: the measured roll-up
/// must be ≥ the declared tier, and the declared tier must be ≥ the committed
/// floor. Undeclared slices are advisory and never fail. Exit 1 on any failure.
pub fn slice_quality_gate() -> i32 {
    slice_quality_gate_at(&project_root())
}

/// The root-parameterized core of [`slice_quality_gate`]: run the whole opt-in
/// ratchet gate against `repo_root` rather than the hardwired [`project_root`], so the
/// gate can be driven end-to-end against a fixture repository (dependency-injected
/// root) instead of only the live checkout. The public zero-argument
/// [`slice_quality_gate`] is the thin `make check` entry point over `project_root()`.
pub(crate) fn slice_quality_gate_at(repo_root: &Path) -> i32 {
    let root = repo_root.to_path_buf();
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
    // Every discovered slice's (dir, IRI) pair, resolved ONCE by the scoring pass above.
    // The merge-base residue reconstruction reuses it instead of re-parsing every
    // slice's manifest.ttl a second time just to discard all but the implicated few.
    let slice_dirs: Vec<(&Path, String)> = scored
        .iter()
        .map(|(dir, report)| (*dir, report.assessment.slice.clone()))
        .collect();
    let working_ceilings = ceilings_from_rubric(&rubric);
    // ONE working-tree measurement, two views: the CONSTRUCT sets (which carry each
    // residue construct's relocation-invariant witness, read by the rebalance) and
    // their `.len()` counts (read by the count gate). Never two sweeps.
    let working_constructs =
        match gmeow_slice_quality::measure_repo_residue_constructs(&root, vocabularies) {
            Ok(m) => m,
            Err(e) => return fail(format!("slice-quality-gate: {e}")),
        };
    let working_residues: std::collections::BTreeMap<(String, String), u64> = working_constructs
        .iter()
        .map(|(key, constructs)| (key.clone(), constructs.len() as u64))
        .collect();
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
    // The relocation transfers the rebalance ACCEPTED, and the aggregate-conservation
    // violations — both produced inside the merge-base arm below, both consumed after
    // it (the accepted set is minted onto the diagnostics ledger; the conservation
    // violations join the failure/green summary as the SIXTH check).
    let mut accepted_transfers: Vec<gmeow_slice_quality::gate::AcceptedTransfer> = Vec::new();
    let mut conservation: Vec<String> = Vec::new();
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
            // The floor / ceiling / registry ratchets are diffed against the base rubric
            // reconstructed over EVERY slice's module.ttl at the base (not one file), so a
            // floor lowered in ANY slice is caught — not only one authored in the rubric.
            match base_rubric_at(&root, &base) {
                Ok(None) => note(
                    "gmeow-dev.slice-quality.gate",
                    format!(
                        "slice-quality-gate: floor-monotonicity check SKIPPED — {RUBRIC_MODULE} is absent at base {base} (brand-new rubric, nothing to regress against)"
                    ),
                ),
                Err(e) => return fail(format!("slice-quality-gate: {e}")),
                Ok(Some(base_rubric)) => {
                    // Tier floors: project the base commitments through the SAME
                    // ladder-resolving projection the working set used.
                    let base_floors = match tier_floors_from_rubric(&base_rubric) {
                        Ok(m) => m,
                        Err(e) => return fail(format!("slice-quality-gate: {e}")),
                    };
                    let tier_mono = gmeow_slice_quality::gate::tier_floor_monotonicity(
                        GOVERNANCE_SOURCE_LABEL,
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
                        GOVERNANCE_SOURCE_LABEL,
                        &base_axis,
                        &axis_floors,
                        |slice, axis| live_slices.contains(slice) && live_axes.contains(axis),
                    );
                    mono.extend(axis_mono.violations);

                    let base_ceilings = ceilings_from_rubric(&base_rubric);

                    // Registry meta-ratchet (C8): the guarded-vocabulary REGISTRY may
                    // only get STRONGER — deleting a vocab, narrowing a namespace,
                    // weakening a count-kind, dropping a counted predicate, raising a
                    // default ceiling, or expanding an exemption set all red the gate,
                    // so the gate cannot be quietly weakened without raising a cell.
                    mono.extend(gmeow_slice_quality::gate::registry_ratchet_monotonicity(
                        GOVERNANCE_SOURCE_LABEL,
                        &base_rubric.floors.vocabularies,
                        &rubric.floors.vocabularies,
                    ));

                    // Projection-ceiling REBALANCE — ratchet invariants 2 (base∩working
                    // monotonicity) and 3 (the grandfather gate for a NEW ceiling) under
                    // ONE rule: a committed ceiling may never exceed its
                    // RELOCATION-ADJUSTED base allowance (the committed base ceiling, or
                    // the measured BASE residue when the ceiling is new). A rule that
                    // held at one ceiling gate and not the other would not be a rule.
                    //
                    // The base measurement is needed for exactly three families of cell:
                    // a NEW ceiling (its grandfather allowance), a RAISED ceiling (its
                    // arrival witness), and either endpoint of an authored
                    // gmeow:CeilingRelocation (its departure/arrival witness). With no
                    // raises, no new ceilings, and no declarations the set is empty and
                    // no `git` work happens at all.
                    let declarations = &rubric.floors.relocations;
                    let mut needed: std::collections::BTreeSet<String> =
                        std::collections::BTreeSet::new();
                    for (key, committed) in &working_ceilings {
                        match base_ceilings.get(key) {
                            None => {
                                needed.insert(key.0.clone());
                            }
                            Some(before) if committed > before => {
                                needed.insert(key.0.clone());
                            }
                            Some(_) => {}
                        }
                    }
                    for d in declarations {
                        needed.insert(d.from_slice.clone());
                        needed.insert(d.to_slice.clone());
                    }
                    let base_meas = match measure_base_residues(
                        &root,
                        &base,
                        vocabularies,
                        &needed,
                        &slice_dirs,
                    ) {
                        Ok(r) => r,
                        Err(e) => return fail(format!("slice-quality-gate: {e}")),
                    };
                    let base_measured = base_meas.counts();
                    let edge_reasons = match derive_edge_reasons(
                        &base_meas,
                        declarations,
                        vocabularies,
                        &slice_dirs,
                    ) {
                        Ok(m) => m,
                        Err(e) => return fail(format!("slice-quality-gate: {e}")),
                    };
                    let default_ceiling_by_prefix: std::collections::BTreeMap<String, u64> =
                        vocabularies
                            .iter()
                            .map(|v| (v.prefix.clone(), v.default_ceiling))
                            .collect();
                    let rebalance = gmeow_slice_quality::gate::projection_ceiling_monotonicity(
                        &gmeow_slice_quality::gate::CeilingComparison {
                            file_label: GOVERNANCE_SOURCE_LABEL,
                            base_ceilings: &base_ceilings,
                            working_ceilings: &working_ceilings,
                            base_measured: &base_measured,
                            working_measured: &working_residues,
                            base_constructs: &base_meas.constructs,
                            working_constructs: &working_constructs,
                            default_ceilings: &default_ceiling_by_prefix,
                            declarations,
                            edge_reasons: &edge_reasons,
                        },
                    );
                    mono.extend(rebalance.violations);
                    accepted_transfers = rebalance.accepted;

                    // SIXTH check: aggregate CONSERVATION, scoped to base ∩ working —
                    // per vocabulary the TOTAL committed ceiling over the cells committed
                    // on BOTH sides may never rise. Relocation moves budget between
                    // cells; it can never create budget. Scoping is load-bearing: a
                    // brand-new ceiling grandfathered under invariant 3 legitimately
                    // raises an unscoped Σ, and deletions only ever lower it.
                    conservation = gmeow_slice_quality::gate::ceiling_conservation(
                        GOVERNANCE_SOURCE_LABEL,
                        &base_ceilings,
                        &working_ceilings,
                    );
                    for e in &conservation {
                        emit_error("gmeow-dev.slice-quality.gate", format!("FAIL {e}"));
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

    // Every ACCEPTED relocation transfer is minted onto the diagnostics LEDGER — a
    // stable finding IRI, the destination cell as its anchor, and the witnessed anchor
    // terms as its ANTECEDENTS. A hand-built Finding (or a bare println) could not be
    // joined by the reasoner over the finding graph and would derive DARK, so the
    // producer routes through the ledger like every other first-class witness.
    let accepted_units: u64 = accepted_transfers.iter().map(|t| t.units).sum();
    emit_accepted_transfers(&accepted_transfers);

    if failures > 0
        || axis_failures > 0
        || mono_failures > 0
        || coherence_failures > 0
        || ceiling_failures > 0
        || !conservation.is_empty()
    {
        return fail(format!(
            "slice-quality-gate: {failures} of {checked} opted-in slice(s) below their declared tier; {axis_failures} of {axis_checked} slice(s) below a committed per-axis floor; {mono_failures} committed-floor monotonicity violation(s); {coherence_failures} floor-coherence violation(s); {ceiling_failures} of {ceiling_checked} (slice,vocab) cell(s) above their committed projection ceiling; {} aggregate ceiling-conservation violation(s)",
            conservation.len()
        ));
    }
    println!(
        "slice-quality-gate: {checked} opted-in slice(s) hold their declared tier; {axis_checked} slice(s) hold their committed per-axis floors; committed floors are monotonic vs the merge base; {coherence_checked} tier-floored slice(s) cohere with their axis floors (0 floor-coherence violation(s)); {ceiling_checked} (slice,vocab) cell(s) hold their projection ceiling; {} witnessed relocation transfer(s) carrying {accepted_units} unit(s) re-projected the base ceiling (0 aggregate conservation violation(s))",
        accepted_transfers.len()
    );
    0
}

/// The finding code every ACCEPTED relocation transfer is interned under.
const RELOCATION_ACCEPTED_CODE: &str = "gmeow-dev.slice-quality.ceiling-relocation.accepted";

/// The finding code each WITNESS term of an accepted transfer is interned under — the
/// antecedent node the transfer's finding hangs its DAG edge on.
const RELOCATION_WITNESS_CODE: &str = "gmeow-dev.slice-quality.ceiling-relocation.witness";

/// Mint every accepted relocation transfer onto a [`gmeow_errors::DiagLedger`] and
/// project the result onto the console sink.
///
/// Each transfer becomes ONE content-addressed witness whose ANTECEDENTS are the
/// witnessed anchor terms that funded it, so the accepted adjustment is a joinable DAG
/// node — `gmeow explain <finding-iri>` resolves it, and a reasoner pass over the
/// finding graph can walk from the transfer to the exact terms that moved. A
/// hand-built `Finding` (or a bare `println!`) would carry no fingerprint identity, no
/// anchor, and no antecedents, so nothing could join it and it would derive DARK.
///
/// Transfers are NOTE-grade: an accepted transfer is an audited fact about a passing
/// gate, never a failure, so it must not gate.
fn emit_accepted_transfers(transfers: &[gmeow_slice_quality::gate::AcceptedTransfer]) {
    use gmeow_errors::{DiagLedger, StageId};
    if transfers.is_empty() {
        return;
    }
    let stage = StageId::new("slice-quality-gate");
    let mut ledger = DiagLedger::new();
    for t in transfers {
        // The witness antecedents FIRST: each witnessed term is its own interned node,
        // anchored on the term IRI, so two transfers sharing a term share one witness.
        let antecedents: Vec<gmeow_errors::DiagRef> = t
            .witnesses
            .iter()
            .map(|term| {
                let diag = gmeow_errors::Diag::note(
                    gmeow_errors::register_code(RELOCATION_WITNESS_CODE),
                    format!(
                        "relocation witness: {term} departed {} and arrived at {} in the {} residue",
                        t.from, t.to, t.vocab
                    ),
                )
                .with_focus(term.clone())
                .with_location(gmeow_errors::Location {
                    logical: Some(term.clone()),
                    ..gmeow_errors::Location::default()
                });
                ledger.attach(diag, stage.clone())
            })
            .collect();
        let anchor = format!("{}#{}", t.to, t.vocab);
        let diag = gmeow_errors::Diag::note(
            gmeow_errors::register_code(RELOCATION_ACCEPTED_CODE),
            format!(
                "accepted relocation transfer: {} unit(s) of {} residue moved {} → {}, re-projecting the base ceiling of the destination cell by exactly that much; witnessed by {}; declared by {}",
                t.units,
                t.vocab,
                t.from,
                t.to,
                t.witnesses.join(", "),
                t.declarations.join(", ")
            ),
        )
        .with_focus(anchor.clone())
        .with_location(gmeow_errors::Location {
            logical: Some(anchor),
            ..gmeow_errors::Location::default()
        })
        .with_antecedents(antecedents);
        ledger.attach(diag, stage.clone());
    }
    crate::dev_common::emit_report(&ledger.project_report("gmeow-dev").normalized());
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

/// The repo-relative directory holding the repo-level (non-slice) `dsl/mappings/`
/// authoring surface — the pathspec half of
/// `gmeow_slice_quality::DSL_MAPPING_SURFACE_IRI`.
const DSL_MAPPINGS_REL_DIR: &str = "dsl/mappings";

/// A temporary directory holding the merge base's AUTHORING SURFACES as plain files,
/// materialized by ONE `git archive` (see [`materialize_base_tree`]). Removed on drop —
/// including on every `?` early return, so a hard-failing gate never leaks it.
struct BaseTree {
    /// The extraction root; base surfaces sit under it at their repo-relative paths.
    root: std::path::PathBuf,
}

impl Drop for BaseTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Which of `dirs` (repo-relative directory paths) EXIST at `base`. One `git ls-tree`
/// for the whole set, not one per directory.
///
/// This is a pure EXISTENCE probe, never a reconstruction of the ratchet's surface
/// fileset: [`materialize_base_tree`] hands the surviving directories to `git archive`
/// wholesale and `gmeow_slice_quality::ratchet_surface_paths` then scans the extracted
/// tree, so there is exactly ONE definition of "which files are a ratchet surface" and
/// it is shared with the working-tree measurement. The probe exists only because
/// `git archive` HARD-FAILS on a pathspec matching nothing, while a slice directory
/// that is genuinely new in the working tree must legitimately contribute base residue
/// 0. `git ls-tree` does not error on a non-matching pathspec, so an absent directory
/// is simply missing from the returned set; a non-zero exit means git could not answer
/// and is a HARD FAIL, never a silent "nothing there".
fn base_dirs_present(
    root: &Path,
    base: &str,
    dirs: &[String],
) -> gmeow_errors::Result<std::collections::BTreeSet<String>> {
    let out = std::process::Command::new("git")
        .current_dir(root)
        .env("LC_ALL", "C")
        .args(["ls-tree", "-d", "--name-only", base, "--"])
        .args(dirs)
        .output()
        .map_err(|e| sqe(format!("could not run `git ls-tree -d {base}`: {e}")))?;
    if !out.status.success() {
        return Err(sqe(format!(
            "`git ls-tree -d {base}` failed ({}): {}",
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

/// Materialize `pathspecs` as they existed at `base` into a fresh temp directory with a
/// SINGLE `git archive <base> -- <pathspecs> | tar -x`, so the base surfaces can be read
/// as plain files through the very same code path the working tree uses.
///
/// One archive replaces the former per-file `git show` fan-out, and — more importantly —
/// removes the second, base-only path reconstruction that could drift from
/// `gmeow_slice_quality::ratchet_surface_paths`: after extraction there is one scanner
/// for both sides, so base-vs-working is an apples-to-apples measurement.
///
/// Any failure of either process is a HARD FAIL (propagated), never a silent fall-back
/// to "no base surfaces" — a silently-empty base tree would measure residue 0 and hand
/// out a free grandfather for freshly-authored constructs, exactly the degradation
/// `counting`'s module doc forbids.
///
/// # Errors
/// A HARD FAIL if the temp directory cannot be created, if `git archive` or `tar` cannot
/// be spawned, or if either exits non-zero.
fn materialize_base_tree(
    root: &Path,
    base: &str,
    pathspecs: &[String],
) -> gmeow_errors::Result<BaseTree> {
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);

    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "gmeow-ratchet-base-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)
        .map_err(|e| sqe(format!("could not create base-tree temp dir {dir:?}: {e}")))?;
    // Construct the guard BEFORE anything else can fail, so every path below cleans up.
    let tree = BaseTree { root: dir };

    let mut archive = Command::new("git")
        .current_dir(root)
        .env("LC_ALL", "C")
        .args(["archive", "--format=tar", base, "--"])
        .args(pathspecs)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| sqe(format!("could not run `git archive {base}`: {e}")))?;
    let stdout = archive
        .stdout
        .take()
        .ok_or_else(|| sqe(format!("`git archive {base}` produced no stdout pipe")))?;
    let extract = Command::new("tar")
        .current_dir(&tree.root)
        .env("LC_ALL", "C")
        .args(["-x", "-f", "-"])
        .stdin(Stdio::from(stdout))
        .output()
        .map_err(|e| {
            sqe(format!(
                "could not run `tar -x` for `git archive {base}`: {e}"
            ))
        })?;
    let archived = archive
        .wait_with_output()
        .map_err(|e| sqe(format!("could not wait for `git archive {base}`: {e}")))?;
    if !archived.status.success() {
        return Err(sqe(format!(
            "`git archive {base} -- {}` failed ({}): {}",
            pathspecs.join(" "),
            archived.status,
            String::from_utf8_lossy(&archived.stderr).trim()
        )));
    }
    if !extract.status.success() {
        return Err(sqe(format!(
            "extracting the `git archive {base}` stream failed ({}): {}",
            extract.status,
            String::from_utf8_lossy(&extract.stderr).trim()
        )));
    }
    Ok(tree)
}

/// The merge-base residue measurement, with the materialized base tree kept ALIVE.
///
/// The tree is retained (rather than dropped as soon as the counts are read) because
/// the relocation accounting needs to RE-READ the base authoring surfaces: deriving
/// WHY a construct's residue membership failed to be conserved across a move
/// ([`gmeow_slice_quality::relocation_reasons_for_surfaces`]) requires the real base
/// dataset, not merely the constructs counted out of it. Dropping the struct removes
/// the temp directory, so the lifetime is explicit rather than implicit.
#[derive(Default)]
struct BaseMeasurement {
    /// The extracted base tree — `None` when no slice was implicated and no `git`
    /// work was done at all.
    tree: Option<BaseTree>,
    /// `slice IRI (or the DSL surface IRI) -> repo-relative directory`, for exactly the
    /// implicated surfaces that EXIST at base.
    dirs: std::collections::BTreeMap<String, String>,
    /// `(slice IRI, vocab prefix) -> the residue CONSTRUCTS at base`, each carrying its
    /// relocation-invariant [`gmeow_slice_quality::Witness`].
    constructs: std::collections::BTreeMap<(String, String), Vec<gmeow_slice_quality::Construct>>,
}

impl BaseMeasurement {
    /// The `.len()` projection of [`Self::constructs`] — the counted base residue the
    /// grandfather gate compares a NEW ceiling against. One measurement, two views.
    fn counts(&self) -> std::collections::BTreeMap<(String, String), u64> {
        self.constructs
            .iter()
            .map(|(key, constructs)| (key.clone(), constructs.len() as u64))
            .collect()
    }

    /// The base authoring-surface fileset for `slice_iri`, inside the materialized
    /// tree — empty when the surface did not exist at base.
    fn surface_paths(&self, slice_iri: &str) -> Vec<std::path::PathBuf> {
        let (Some(tree), Some(rel)) = (self.tree.as_ref(), self.dirs.get(slice_iri)) else {
            return Vec::new();
        };
        if slice_iri == gmeow_slice_quality::DSL_MAPPING_SURFACE_IRI {
            gmeow_slice_quality::ratchet_dsl_surface_paths(&tree.root)
        } else {
            gmeow_slice_quality::ratchet_surface_paths(&tree.root.join(rel))
        }
    }
}

/// Reconstruct the ungrounded residue AT THE MERGE BASE for exactly the (slice, vocab)
/// cells whose slice appears in `needed` — the slices whose committed projection ceiling
/// is NEW in the working tree (ratchet invariant 3, the grandfather gate).
///
/// `slices` carries the working tree's ALREADY-RESOLVED `(slice dir, slice IRI)` pairs
/// (the gate's scoring pass resolved every manifest once), so the `needed` filter is
/// applied FIRST and no manifest is re-parsed here at all — the former sweep resolved
/// `slice_iri_of_dir` for all ~600 discovered slices before discarding all but a
/// handful. Attribution stays on the WORKING slice IRI, exactly as before.
///
/// The surviving directories are materialized at `base` by ONE
/// [`materialize_base_tree`] call and then measured through
/// `gmeow_slice_quality::ratchet_surface_paths` +
/// `gmeow_slice_quality::measure_surface_residue_constructs` — the SAME functions
/// `measure_repo_residues` runs over the working tree. A slice directory absent at base
/// (a genuinely new slice) is dropped by the [`base_dirs_present`] probe and contributes
/// NOTHING, i.e. base residue 0 for every vocab via the caller's `unwrap_or(0)`.
///
/// # Errors
/// HARD-FAILS on any `git`/`tar` failure (propagated from [`base_dirs_present`] /
/// [`materialize_base_tree`]), on a working-tree path that is not under `root`, or on a
/// Turtle parse/merge failure of a present base surface (propagated from
/// `gmeow_slice_quality::measure_surface_residue_constructs`). Never a silent fall-back
/// to residue 0.
fn measure_base_residues(
    root: &Path,
    base: &str,
    vocabularies: &[gmeow_slice_quality::model::ProjectionVocabulary],
    needed: &std::collections::BTreeSet<String>,
    slices: &[(&Path, String)],
) -> gmeow_errors::Result<BaseMeasurement> {
    let mut out = BaseMeasurement::default();

    // FILTER FIRST: only the slices a new ceiling actually implicates do any work.
    let mut wanted: Vec<(&str, String)> = Vec::new(); // (slice IRI, repo-relative dir)
    for (dir, slice_iri) in slices {
        if !needed.contains(slice_iri) {
            continue;
        }
        let rel_dir = dir
            .strip_prefix(root)
            .map_err(|e| sqe(format!("failed to strip prefix {root:?} from {dir:?}: {e}")))?
            .to_string_lossy()
            .replace('\\', "/");
        wanted.push((slice_iri.as_str(), rel_dir));
    }
    let dsl_needed = needed.contains(gmeow_slice_quality::DSL_MAPPING_SURFACE_IRI);
    if wanted.is_empty() && !dsl_needed {
        return Ok(out);
    }

    let mut probe: Vec<String> = wanted.iter().map(|(_, rel)| rel.clone()).collect();
    if dsl_needed {
        probe.push(DSL_MAPPINGS_REL_DIR.to_owned());
    }
    let present = base_dirs_present(root, base, &probe)?;
    let pathspecs: Vec<String> = probe.into_iter().filter(|p| present.contains(p)).collect();
    if pathspecs.is_empty() {
        return Ok(out); // every implicated directory is new at base → base residue 0
    }
    let tree = materialize_base_tree(root, base, &pathspecs)?;

    for (slice_iri, rel_dir) in &wanted {
        if !present.contains(rel_dir) {
            continue; // the slice directory does not exist at base → base residue 0
        }
        out.dirs.insert((*slice_iri).to_owned(), rel_dir.clone());
        let paths = gmeow_slice_quality::ratchet_surface_paths(&tree.root.join(rel_dir));
        for (prefix, constructs) in gmeow_slice_quality::measure_surface_residue_constructs(
            &paths,
            slice_iri,
            vocabularies,
        )? {
            out.constructs
                .insert(((*slice_iri).to_owned(), prefix), constructs);
        }
    }
    // The repo-level dsl/mappings/ surface (attributed to the DSL surface IRI) is not
    // under any slice dir — measure it from the same materialized base tree, through the
    // same scanner the working tree uses, so a NEW dsl-surface ceiling is grandfathered
    // against real base residue.
    if dsl_needed && present.contains(DSL_MAPPINGS_REL_DIR) {
        out.dirs.insert(
            gmeow_slice_quality::DSL_MAPPING_SURFACE_IRI.to_owned(),
            DSL_MAPPINGS_REL_DIR.to_owned(),
        );
        let paths = gmeow_slice_quality::ratchet_dsl_surface_paths(&tree.root);
        for (prefix, constructs) in gmeow_slice_quality::measure_surface_residue_constructs(
            &paths,
            gmeow_slice_quality::DSL_MAPPING_SURFACE_IRI,
            vocabularies,
        )? {
            out.constructs.insert(
                (
                    gmeow_slice_quality::DSL_MAPPING_SURFACE_IRI.to_owned(),
                    prefix,
                ),
                constructs,
            );
        }
    }
    out.tree = Some(tree);
    Ok(out)
}

/// Derive, per declared `(from, to, vocab)` edge, WHY the residue of the moving
/// constructs is not conserved across the move — the three Task-2 reason codes
/// (`exemption-shift-owner-boundary`, `grounding-orphaned`, `bridge-exempt-both-sides`),
/// keyed by relocation-invariant anchor IRI.
///
/// Residue is a function of `(dataset, surface_iri)`, not of the construct alone, so a
/// construct crossing a vocabulary's owner boundary — or moving away from the
/// `logic:Formula` that grounded it — has residue CREATED or DESTROYED with no
/// authoring at all. The gate reports these verbatim on a refusal so a maintainer sees
/// the real reason a declared relocation failed to balance instead of only a count
/// delta.
///
/// The SOURCE side is read out of the MATERIALIZED merge-base tree (where the
/// constructs sat) and the DESTINATION side out of the working tree (where they now
/// live) — the same two views the rebalance itself compares.
///
/// # Errors
/// HARD-FAILS if either side's authoring surface cannot be read or parsed. A
/// declaration whose source surface is absent at base contributes NO reasons (there is
/// nothing to have moved), which is a real measurement, not a fallback.
fn derive_edge_reasons(
    base: &BaseMeasurement,
    declarations: &[gmeow_slice_quality::CeilingRelocation],
    vocabularies: &[gmeow_slice_quality::model::ProjectionVocabulary],
    slices: &[(&Path, String)],
) -> gmeow_errors::Result<gmeow_slice_quality::gate::EdgeRelocationReasons> {
    let mut out = gmeow_slice_quality::gate::EdgeRelocationReasons::new();
    let working_dir = |iri: &str| -> Option<&Path> {
        slices
            .iter()
            .find(|(_, slice_iri)| slice_iri == iri)
            .map(|(dir, _)| *dir)
    };
    for d in declarations {
        let source_paths = base.surface_paths(&d.from_slice);
        if source_paths.is_empty() {
            continue; // the source surface did not exist at base — nothing moved out of it
        }
        let Some(dest_dir) = working_dir(&d.to_slice) else {
            continue; // the destination is not a discovered slice — the witness will red
        };
        let dest_paths = gmeow_slice_quality::ratchet_surface_paths(dest_dir);
        for vocab in vocabularies {
            if d.vocabulary.as_ref().is_some_and(|dv| dv != &vocab.prefix) {
                continue;
            }
            let reasons = gmeow_slice_quality::relocation_reasons_for_surfaces(
                &source_paths,
                &d.from_slice,
                &dest_paths,
                &d.to_slice,
                vocab,
            )?;
            let declared: std::collections::BTreeSet<&str> =
                d.terms.iter().map(String::as_str).collect();
            let scoped: std::collections::BTreeMap<_, _> = reasons
                .into_iter()
                .filter(|(anchor, _)| declared.contains(anchor.as_str()))
                .collect();
            if !scoped.is_empty() {
                out.insert(
                    (
                        d.from_slice.clone(),
                        d.to_slice.clone(),
                        vocab.prefix.clone(),
                    ),
                    scoped,
                );
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
#[cfg(test)]
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

/// Reconstruct the whole SEGREGATED rubric as it existed at merge base `base`, unioning
/// EVERY working-tree slice's `module.ttl` read at the base ref (mirroring
/// [`measure_base_residues`]' multi-file base read) so the floor-monotonicity diff
/// compares the working floor set against the base floor set authored across ALL slices,
/// not only the single rubric module. The centralized measurement standard + vocabulary
/// registry come from the rubric module at base; the distributed floor / tier-floor /
/// ceiling commitments come from the base union — segregated through the SAME
/// [`gmeow_slice_quality::segregate_rubric`] the working-tree loader uses, so the base
/// comparand can never diverge from how the working set is assembled.
///
/// Returns `Ok(None)` when the rubric module itself is ABSENT at base (a merge base
/// predating the rubric slice, or a brand-new file) — the earlier single-file check
/// skipped the monotonicity diff in exactly that case, so this preserves that behavior.
/// A slice whose `module.ttl` is absent at base contributes nothing (its working floors
/// read as additions — allowed). Because the diff keys on `(slice, axis)` rather than on
/// file, a floor MOVED between two slice modules base→working still compares by value.
///
/// # Errors
/// HARD-FAILS on any `git` failure other than a legitimately-absent path (propagated
/// from [`git_show_base`]), on a Turtle parse/freeze failure of a present base module,
/// or on the centralized-authority guard (a centralized individual authored outside the
/// rubric slice at base).
fn base_rubric_at(root: &Path, base: &str) -> gmeow_errors::Result<Option<Rubric>> {
    // Centralized half: the rubric module at base. Absent → skip the whole diff.
    let rubric_text = match git_show_base(root, base, RUBRIC_MODULE) {
        BaseFile::Absent => return Ok(None),
        BaseFile::Error(e) => return Err(sqe(e)),
        BaseFile::Contents(text) => text,
    };
    let canonical =
        gmeow_slice_quality::rubric::load_rubric(&*gmeow_slice_quality::dataset_from_texts(&[
            rubric_text.as_str(),
        ])?)?;

    // Distributed half: every discovered slice's module.ttl read at base, unioned (the
    // rubric slice is itself discovered, so the union carries the tier ladder + axes the
    // widened load requires). Track each text's rel-path label alongside it so a
    // cross-file governance collision at base can be diagnosed with both offending
    // filenames — the same precision the working-tree loader gives via
    // `detect_cross_file_governance_collisions`.
    let mut union_labeled: Vec<(String, String)> = Vec::new();
    for dir in gmeow_slice_quality::discover_slice_dirs(&root.join("slices")) {
        let rel = dir
            .join("module.ttl")
            .strip_prefix(root)
            .map_err(|e| sqe(format!("failed to strip prefix {root:?} from {dir:?}: {e}")))?
            .to_string_lossy()
            .replace('\\', "/");
        match git_show_base(root, base, &rel) {
            BaseFile::Absent => {}
            BaseFile::Error(e) => return Err(sqe(e)),
            BaseFile::Contents(text) => union_labeled.push((rel, text)),
        }
    }
    let collision_refs: Vec<(&str, &str)> = union_labeled
        .iter()
        .map(|(rel, text)| (rel.as_str(), text.as_str()))
        .collect();
    gmeow_slice_quality::detect_cross_file_governance_collisions_texts(&collision_refs)?;
    let union_refs: Vec<&str> = union_labeled
        .iter()
        .map(|(_, text)| text.as_str())
        .collect();
    let widened = gmeow_slice_quality::rubric::load_rubric(
        &*gmeow_slice_quality::dataset_from_texts(&union_refs)?,
    )?;

    Ok(Some(gmeow_slice_quality::segregate_rubric(
        canonical, widened,
    )?))
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
        "gmeow:afc-{slice_local}-{axis_local} a gmeow:AxisFloorCommitment ; rdfs:label \"axis-floor commitment — {slice_local} / {axis_local}\"@x-gmeow-english ; skos:definition \"The committed raise-only measured-score floor for the {axis_local} quality axis on the {slice_local} slice; the gate reds if the slice's measured score falls below it.\"@x-gmeow-english ; rdfs:isDefinedBy <{slice_iri}> ; gmeow:graphBoxRole gmeow:boxABox ; gmeow:floorSlice <{slice_iri}> ; gmeow:floorAxis gmeow:{axis_local} ; gmeow:floorValue {} .",
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
/// live measurement and paste them into the owning slice's own `module.ttl`
/// (the floor is authored by the slice it governs, not necessarily the rubric
/// slice — see `rdfs:isDefinedBy <{slice_iri}>` in [`format_floor_line`]).
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
    let rubric = match gmeow_slice_quality::load_repo_rubric(&root) {
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
        "# seeded gmeow:AxisFloorCommitment individuals for axis {scope} — paste into the owning slice's own module.ttl"
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
        "gmeow:pcc-{slice_local}-{vocab_prefix} a gmeow:ProjectionCeilingCommitment ; rdfs:label \"projection-ceiling commitment — {slice_local} / {vocab_prefix}\"@x-gmeow-english ; skos:definition \"The committed lower-only ungrounded-residue ceiling for the {vocab_prefix} projection vocabulary on the {slice_local} slice; the gate reds if the slice's measured residue rises above it.\"@x-gmeow-english ; rdfs:isDefinedBy <{slice_iri}> ; gmeow:graphBoxRole gmeow:boxABox ; gmeow:ceilingSlice <{slice_iri}> ; gmeow:ceilingVocabulary gmeow:projVocab-{vocab_prefix} ; gmeow:ceilingCount {count} ."
    )
}

/// `gmeow-dev slice-quality-seed-ceilings` — emit `gmeow:ProjectionCeilingCommitment`
/// TTL at the CURRENT measured ungrounded residue for every (slice, guarded
/// projection-vocabulary) pair with nonzero residue, so a human can grandfather the
/// existing residue and paste it into the owning slice's own `module.ttl`
/// (the ceiling is authored by the slice it governs, not necessarily the rubric
/// slice — see `rdfs:isDefinedBy <{slice_iri}>` in [`format_ceiling_line`]).
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
    let rubric = match gmeow_slice_quality::load_repo_rubric(&root) {
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
    println!(
        "# seeded gmeow:ProjectionCeilingCommitment individuals — paste into the owning slice's own module.ttl"
    );
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
    let rubric = match gmeow_slice_quality::load_repo_rubric(&root) {
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

/// One vocabulary's relocation TRANSPORT PLAN: what the source's lowering would raise
/// as credit, what the destination's raise would demand, and how much of that demand no
/// credit covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RelocationPlan {
    /// The credit the source's lowering-to-its-post-move-measured-residue would raise,
    /// clamped to the units that actually move (the gate applies the same clamp against
    /// the DECLARED, WITNESSED departures, so a lowering of dead headroom buys nothing).
    credit: u64,
    /// The raise the destination would have to commit: its post-move measured residue
    /// minus the ceiling it already holds.
    demand: u64,
    /// The part of `demand` no credit covers — exactly what the gate would refuse.
    unpaid: u64,
}

/// Compute one vocabulary's transport plan from the two live measurements and the two
/// committed ceilings. Pure, so the arithmetic the preview reports is testable
/// independently of a repository state.
///
/// The maintainer's move is modelled as the gate expects it to be authored: lower the
/// source's `gmeow:ceilingCount` to its post-move measured residue, and pin the
/// destination's to ITS post-move measured residue.
///
/// Note the structural consequence, which the preview states in its legend rather than
/// leaving implicit: on a corpus where the COUNT gate is green (`to_measured <=
/// to_ceiling` everywhere), `demand = to_measured + moving - to_ceiling <= moving` and
/// `credit = moving`, so `unpaid` is necessarily `0`. A nonzero `unpaid` therefore means
/// the destination cell is ALREADY over its ceiling — the move is not the problem, the
/// destination is.
fn relocation_plan(
    moving: u64,
    from_measured: u64,
    from_ceiling: u64,
    to_measured: u64,
    to_ceiling: u64,
) -> RelocationPlan {
    let credit = from_ceiling
        .saturating_sub(from_measured.saturating_sub(moving))
        .min(moving);
    let demand = (to_measured + moving).saturating_sub(to_ceiling);
    RelocationPlan {
        credit,
        demand,
        unpaid: demand.saturating_sub(credit),
    }
}

/// How many relocatable anchor terms the preview's DISCOVERY listing prints per
/// vocabulary before it truncates. A slice can carry dozens of anchors and the listing
/// is a navigation aid, not a report — but a silent truncation would be a lie about
/// what is movable, so the cap is always accompanied by an explicit "… and N more"
/// line naming the remainder count.
const RELOCATION_ANCHOR_LISTING_CAP: usize = 20;

/// Print, per guarded vocabulary, the terms in `slice_iri`'s residue that DO anchor at
/// least one construct, with the construct count each would carry across a move.
///
/// This is the preview's DISCOVERY surface. A maintainer asking "what would this move
/// cost me?" does not know the anchor IRIs, and there is no other way to find them: the
/// residue counter's relocation-invariant anchor is a derived quantity (a nested
/// anonymous `sh:property` block anchors on the nearest NAMED ancestor, which is not
/// visible by reading the Turtle), so guessing a term IRI out of a slice's source is
/// unreliable. Without this listing the command cannot be used at all.
///
/// Deterministic: vocabularies in registry order, anchors sorted by descending construct
/// count then by IRI, so the terms that would carry the most residue lead. Truncation is
/// NEVER silent — a capped list always states how many anchors it omitted.
fn print_relocatable_anchors(
    residue: &std::collections::BTreeMap<String, Vec<gmeow_slice_quality::Construct>>,
    vocabularies: &[gmeow_slice_quality::model::ProjectionVocabulary],
    slice_iri: &str,
) {
    println!("# relocatable anchor terms in {slice_iri} — pass one of these as --term");
    println!("vocab\tterm\tconstructs");
    let mut any = false;
    for vocab in vocabularies {
        let Some(constructs) = residue.get(&vocab.prefix) else {
            continue;
        };
        let mut by_anchor: std::collections::BTreeMap<&str, u64> =
            std::collections::BTreeMap::new();
        let mut non_relocatable = 0u64;
        for c in constructs {
            match c.witness.anchor() {
                Some(anchor) => *by_anchor.entry(anchor).or_insert(0) += 1,
                None => non_relocatable += 1,
            }
        }
        // Descending construct count, then IRI — the biggest movers first, ties stable.
        let mut ranked: Vec<(&str, u64)> = by_anchor.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        if ranked.is_empty() {
            // Every construct in this vocabulary is blank-subject residue: it is real
            // residue, but NONE of it can ever witness a relocation. Say so rather than
            // omit the vocabulary and imply it is clean.
            if non_relocatable > 0 {
                println!(
                    "# {}: {non_relocatable} construct(s), all blank-subject with no named anchor — none can witness a relocation",
                    vocab.prefix
                );
            }
            continue;
        }
        any = true;
        let shown = ranked.len().min(RELOCATION_ANCHOR_LISTING_CAP);
        for (anchor, count) in &ranked[..shown] {
            println!("{}\t{anchor}\t{count}", vocab.prefix);
        }
        if ranked.len() > shown {
            println!(
                "# {}: … and {} more anchor term(s) not shown",
                vocab.prefix,
                ranked.len() - shown
            );
        }
        if non_relocatable > 0 {
            println!(
                "# {}: plus {non_relocatable} blank-subject construct(s) with no named anchor — none can witness a relocation",
                vocab.prefix
            );
        }
    }
    if !any {
        println!(
            "# {slice_iri} carries no anchored residue in any guarded vocabulary — no relocation out of it can be witnessed"
        );
    }
}

/// `gmeow-dev slice-quality-relocation-preview --term <iri>… --from <slice> --to <slice>`
/// — a REPORT-ONLY preview of what relocating `terms` from `from` to `to` would cost
/// and what it would need to be paid for.
///
/// The ratchet's ceiling side is relocation-aware: a ceiling budgets NET-NEW UNGROUNDED
/// AUTHORING, which is location-independent, so a declared-and-corroborated relocation
/// re-projects the base ceiling before the lower-only comparison runs. Its FLOOR side
/// deliberately is NOT: an axis floor measures the documentation quality of the
/// inventory a slice currently OWNS, which genuinely is location-dependent — importing
/// an under-documented term really does lower the destination's quality, and the answer
/// is to document it, not to net it away. This command prints both halves so the
/// asymmetry is visible BEFORE the move, not discovered after it.
///
/// Per guarded vocabulary it reports:
/// - the TRANSPORT PLAN: how many residue constructs anchored on the named terms would
///   move, the credit the source's lowering-to-measured would raise, and the demand the
///   destination's raise-to-measured would create;
/// - the RESIDUAL UNPAID demand (`max(0, demand − credit)`), which is exactly what the
///   gate would refuse;
/// - the three residue-conservation reason codes
///   ([`gmeow_slice_quality::RelocationReason`]) for every named term whose residue
///   membership genuinely changes across the move.
///
/// When NONE of the requested terms anchors residue in the source slice, it says so
/// precisely (that is a statement about the requested terms, never about whether the
/// slice carries residue at all) and then prints the DISCOVERY listing
/// ([`print_relocatable_anchors`]): every term in the source that DOES anchor residue,
/// per vocabulary, with the construct count each would carry.
///
/// Then, once, the AXIS-FLOOR COLLATERAL: every committed `gmeow:AxisFloorCommitment`
/// on either slice with its live measured score and headroom.
///
/// Always exits 0 — it never gates, and its numbers are never fed back into a ceiling
/// (a ceiling is lowered only by a deliberate hand-edit after a genuine measured
/// migration, and raised only through an authored `gmeow:CeilingRelocation` the gate
/// then corroborates against the derived witness).
pub fn slice_quality_relocation_preview(terms: &[String], from: &str, to: &str) -> i32 {
    let root = project_root();
    let rubric = match gmeow_slice_quality::load_repo_rubric(&root) {
        Ok(r) => r,
        Err(e) => return fail(format!("slice-quality-relocation-preview: {e}")),
    };
    let vocabularies = &rubric.floors.vocabularies;
    if vocabularies.is_empty() {
        return fail(
            "slice-quality-relocation-preview: no gmeow:ProjectionVocabulary individuals loaded from the rubric — the guarded projection-vocabulary registry must be loaded before residue can be measured",
        );
    }

    // Resolve each slice reference to a discovered slice directory + IRI. A full IRI
    // or a bare local name both resolve; an unresolvable reference is a hard fail
    // (never a silent empty report).
    let dirs = gmeow_slice_quality::discover_slice_dirs(&root.join("slices"));
    let mut resolved: Vec<(std::path::PathBuf, String)> = Vec::with_capacity(dirs.len());
    for dir in &dirs {
        match gmeow_slice_quality::slice_iri_of_dir(dir) {
            Ok(iri) => resolved.push((dir.clone(), iri)),
            Err(e) => return fail(format!("slice-quality-relocation-preview: {e}")),
        }
    }
    let find = |reference: &str| -> Option<&(std::path::PathBuf, String)> {
        resolved
            .iter()
            .find(|(_, iri)| iri == reference || axis_local_name(iri) == reference)
    };
    let (Some((from_dir, from_iri)), Some((to_dir, to_iri))) = (find(from), find(to)) else {
        return fail(format!(
            "slice-quality-relocation-preview: --from {from:?} / --to {to:?} must each name a discovered gmeow:Slice (by full IRI or local name)"
        ));
    };
    if from_iri == to_iri {
        return fail(
            "slice-quality-relocation-preview: --from and --to name the same slice — a relocation that does not cross a slice boundary moves no residue",
        );
    }
    let wanted: std::collections::BTreeSet<&str> = terms.iter().map(String::as_str).collect();

    let from_paths = gmeow_slice_quality::ratchet_surface_paths(from_dir);
    let to_paths = gmeow_slice_quality::ratchet_surface_paths(to_dir);
    let from_residue = match gmeow_slice_quality::measure_surface_residue_constructs(
        &from_paths,
        from_iri,
        vocabularies,
    ) {
        Ok(m) => m,
        Err(e) => return fail(format!("slice-quality-relocation-preview: {e}")),
    };
    let to_residue = match gmeow_slice_quality::measure_surface_residue_constructs(
        &to_paths,
        to_iri,
        vocabularies,
    ) {
        Ok(m) => m,
        Err(e) => return fail(format!("slice-quality-relocation-preview: {e}")),
    };
    let ceilings = &rubric.floors.ceilings;
    let ceiling_of =
        |slice: &str, vocab: &gmeow_slice_quality::model::ProjectionVocabulary| -> u64 {
            ceilings
                .iter()
                .find(|c| c.slice == slice && c.vocab_prefix == vocab.prefix)
                .map_or(vocab.default_ceiling, |c| c.count)
        };

    println!("# relocation preview: {from_iri} → {to_iri}");
    println!("# terms: {}", terms.join(", "));
    println!(
        "vocab\tmoving\tfrom-measured\tfrom-ceiling\tcredit\tto-measured\tto-ceiling\tdemand\tunpaid"
    );
    let mut any_moving = false;
    for vocab in vocabularies {
        let from_constructs = from_residue
            .get(&vocab.prefix)
            .map_or(&[][..], Vec::as_slice);
        let to_constructs = to_residue.get(&vocab.prefix).map_or(&[][..], Vec::as_slice);
        let moving = from_constructs
            .iter()
            .filter(|c| c.witness.anchor().is_some_and(|a| wanted.contains(a)))
            .count() as u64;
        if moving == 0 {
            continue;
        }
        any_moving = true;
        let from_measured = from_constructs.len() as u64;
        let to_measured = to_constructs.len() as u64;
        let from_ceiling = ceiling_of(from_iri, vocab);
        let to_ceiling = ceiling_of(to_iri, vocab);
        // The maintainer lowers the source ceiling to its post-move measured residue
        // and raises the destination ceiling to its post-move measured residue; the
        // gate then clamps the credit to the DECLARED, WITNESSED departures.
        let plan = relocation_plan(moving, from_measured, from_ceiling, to_measured, to_ceiling);
        let RelocationPlan {
            credit,
            demand,
            unpaid,
        } = plan;
        println!(
            "{}\t{moving}\t{from_measured}\t{from_ceiling}\t{credit}\t{to_measured}\t{to_ceiling}\t{demand}\t{unpaid}",
            vocab.prefix
        );
        // State the verdict in WORDS, not only as a column a reader must interpret —
        // "nothing would move" and "something would move but is unpaid" must be
        // distinguishable without arithmetic.
        if demand == 0 {
            println!(
                "# {}: {moving} unit(s) would move; {to_iri} already holds enough committed headroom, so no ceiling raise is needed at all.",
                vocab.prefix
            );
        } else if unpaid == 0 {
            println!(
                "# {}: {moving} unit(s) would move and the whole {demand}-unit raise is payable — the gate would accept it, given a gmeow:CeilingRelocation declaring these terms.",
                vocab.prefix
            );
        } else {
            println!(
                "# {}: {moving} unit(s) would move but {unpaid} of the {demand}-unit raise is UNPAID (credit {credit}) — the gate would REFUSE it. A nonzero unpaid means {to_iri} is already above its {} ceiling; fix that first, the move is not what is wrong.",
                vocab.prefix, vocab.prefix
            );
        }

        match gmeow_slice_quality::relocation_reasons_for_surfaces(
            &from_paths,
            from_iri,
            &to_paths,
            to_iri,
            vocab,
        ) {
            Ok(reasons) => {
                for (anchor, codes) in reasons.iter().filter(|(a, _)| wanted.contains(a.as_str())) {
                    let codes: Vec<&str> = codes.iter().map(|c| c.code()).collect();
                    println!(
                        "# {} residue NOT conserved moving {anchor}: {}",
                        vocab.prefix,
                        codes.join(", ")
                    );
                }
            }
            Err(e) => return fail(format!("slice-quality-relocation-preview: {e}")),
        }
    }
    if !any_moving {
        println!(
            "# NONE of the {} requested term(s) anchors any residue construct in {from_iri} — nothing would move.",
            terms.len()
        );
        println!(
            "# (This says nothing about whether {from_iri} carries residue: the terms below are the ones that DO.)"
        );
        print_relocatable_anchors(&from_residue, vocabularies, from_iri);
    }

    // AXIS-FLOOR COLLATERAL. Floors are deliberately NOT netted by relocation: the
    // destination genuinely takes on the documentation debt of what it imports. Print
    // every committed floor on both slices with its live measured score and headroom
    // so the cost is visible before the move.
    let axis_floors = match axis_floors_from_rubric(&rubric) {
        Ok(m) => m,
        Err(e) => return fail(format!("slice-quality-relocation-preview: {e}")),
    };
    println!(
        "# axis-floor collateral (floors are NOT netted by relocation — the importer pays the full documentation cost)"
    );
    println!("slice\taxis\tfloor\tmeasured\theadroom");
    let scored = gmeow_slice_quality::score_slices_with_rubric(
        &root,
        &[from_dir.clone(), to_dir.clone()],
        &rubric,
    );
    for report in scored {
        let report = match report {
            Ok(r) => r,
            Err(e) => return fail(format!("slice-quality-relocation-preview: {e}")),
        };
        let slice = &report.assessment.slice;
        let grounding = slice == from_iri && is_grounding_slice(from_dir)
            || slice == to_iri && is_grounding_slice(to_dir);
        for grade in &report.assessment.grades {
            let axis_local = axis_local_name(&grade.axis_iri);
            let floor = match axis_floor_for(&axis_floors, slice, axis_local, grounding) {
                Ok(Some(f)) => f,
                Ok(None) => continue, // unfloored → the move cannot cost it anything gate-visible
                Err(e) => return fail(format!("slice-quality-relocation-preview: {e}")),
            };
            println!(
                "{slice}\t{axis_local}\t{floor:.6}\t{:.6}\t{:.6}",
                grade.score,
                grade.score - floor
            );
        }
    }
    0
}

#[cfg(test)]
mod relocation_preview_tests {
    use super::*;

    #[test]
    fn a_full_move_off_a_green_corpus_is_always_payable() {
        // The live-corpus case: the source sits AT its ceiling and the destination is at
        // or under its own, so the credit the lowering raises exactly covers the raise
        // the destination must commit. This is why the `unpaid` column reads 0 on every
        // real preview — the preview says so in words rather than leaving a maintainer
        // to wonder whether the column is even wired up.
        assert_eq!(
            relocation_plan(4, 17, 17, 0, 0),
            RelocationPlan {
                credit: 4,
                demand: 4,
                unpaid: 0
            }
        );
        // The same with stale headroom at the source and existing headroom at the
        // destination: the credit is still clamped to what actually moves, and the
        // demand shrinks because the destination already had room.
        assert_eq!(
            relocation_plan(3, 39, 40, 0, 4),
            RelocationPlan {
                credit: 3,
                demand: 0,
                unpaid: 0
            }
        );
    }

    #[test]
    fn unpaid_is_nonzero_only_when_the_destination_is_already_over_its_ceiling() {
        // The structural fact the preview's verdict line names: `demand` exceeds
        // `moving` — and therefore exceeds the clamped `credit` — exactly when the
        // destination's measured residue already sits above its committed ceiling. Here
        // the destination measures 5 against a ceiling of 2, so three units of the raise
        // are debt that predates the move and no relocation can pay for.
        assert_eq!(
            relocation_plan(1, 10, 10, 5, 2),
            RelocationPlan {
                credit: 1,
                demand: 4,
                unpaid: 3
            }
        );
    }

    #[test]
    fn the_credit_clamp_is_load_bearing() {
        // A source carrying huge STALE headroom (ceiling 90 against a measured residue
        // of 10) lowers by a lot, but only ONE construct actually moves. Lowering dead
        // headroom surrenders no authoring, so the credit is clamped to the one unit
        // that moved — never the 81-unit paper drop.
        assert_eq!(relocation_plan(1, 10, 90, 0, 0).credit, 1);
    }
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

/// Git-backed coverage for the floor-MONOTONICITY base reconstruction over ALL slices.
/// Before the widening, the base side of the raise-only ratchet read floors from a
/// single `git show <base>:slices/core/slice-quality-rubric/module.ttl`, so a floor
/// authored in a NON-rubric slice and then lowered read as a fresh addition (allowed) —
/// the monotonicity ratchet was blind to it. These tests build a real two-state git
/// repo (a base commit authoring a non-rubric floor, a working tree lowering/deleting
/// it) and drive the real [`base_rubric_at`] multi-slice `git show` reconstruction, so
/// they fail against the pre-widening single-file base read and pass after it.
///
/// The second half of the module covers the grandfather gate's BASE RESIDUE
/// reconstruction over a real materialized base tree ([`measure_base_residues`]):
/// authoring surfaces present at base but deleted in the working tree, a deeply nested
/// `mappings/` file, the repo-level `dsl/mappings/` surface, a slice directory that does
/// not exist at base, and the hard-fail on a base tree that cannot be materialized.
#[cfg(test)]
mod base_monotonicity_git_tests {
    use super::*;
    use gmeow_slice_quality::gate::axis_floor_monotonicity;
    use std::process::Command;
    use std::sync::atomic::{AtomicU32, Ordering};

    const NS: &str = "https://blackcatinformatics.ca/gmeow/";
    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// A structurally-complete minimal rubric module (a two-rung ladder, one axis, one
    /// threshold) — the CENTRALIZED authority the base reconstruction reads from the
    /// rubric slice.
    fn rubric_module() -> String {
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
"#
        )
    }

    /// A DEMO (non-rubric) slice `module.ttl` authoring one `gmeow:AxisFloorCommitment`
    /// against the demo slice on `axisGmn1Coverage` at `floor`.
    fn demo_module(floor: &str) -> String {
        format!(
            r#"@prefix gmeow: <{NS}> .
gmeow:afc-demo a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceDemo ;
    gmeow:floorAxis gmeow:axisGmn1Coverage ;
    gmeow:floorValue {floor} .
"#
        )
    }

    struct GitFixture {
        root: std::path::PathBuf,
    }
    impl Drop for GitFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// Run a git command in `root`, isolated from user/system config (never signs).
    fn git(root: &std::path::Path, args: &[&str]) {
        let out = Command::new("git")
            .current_dir(root)
            .env("LC_ALL", "C")
            .env("HOME", root)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .args(args)
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// Build a git repo fixture holding the rubric slice + a demo slice authoring a
    /// non-rubric floor at `base_floor`, commit it (the merge base), and return the
    /// fixture and the base commit SHA. The caller then rewrites the working tree.
    fn fixture_with_base_floor(base_floor: &str) -> (GitFixture, String) {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let mut root = std::env::temp_dir();
        root.push(format!("gmeow-basemono-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let fx = GitFixture { root: root.clone() };

        let rubric_dir = root.join("slices/core/slice-quality-rubric");
        let demo_dir = root.join("slices/demo/demo");
        std::fs::create_dir_all(&rubric_dir).unwrap();
        std::fs::create_dir_all(&demo_dir).unwrap();
        std::fs::write(rubric_dir.join("manifest.ttl"), "# rubric slice\n").unwrap();
        std::fs::write(rubric_dir.join("module.ttl"), rubric_module()).unwrap();
        std::fs::write(demo_dir.join("manifest.ttl"), "# demo slice\n").unwrap();
        std::fs::write(demo_dir.join("module.ttl"), demo_module(base_floor)).unwrap();

        git(&root, &["init", "-q"]);
        git(&root, &["config", "user.email", "test@example.com"]);
        git(&root, &["config", "user.name", "Test"]);
        git(&root, &["config", "commit.gpgsign", "false"]);
        git(&root, &["add", "-A"]);
        git(&root, &["commit", "-q", "-m", "base"]);
        let out = Command::new("git")
            .current_dir(&root)
            .env("HOME", &root)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .args(["rev-parse", "HEAD"])
            .output()
            .expect("git rev-parse runs");
        assert!(out.status.success(), "git rev-parse failed");
        let base = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        (fx, base)
    }

    fn demo_key() -> (String, String) {
        (format!("{NS}sliceDemo"), "axisGmn1Coverage".to_owned())
    }

    #[test]
    fn base_reconstruction_sees_a_non_rubric_floor_and_reds_a_lowering() {
        // Base commit authors a non-rubric floor at 0.90; working tree lowers it to 0.50.
        let (fx, base) = fixture_with_base_floor("0.9");
        std::fs::write(
            fx.root.join("slices/demo/demo/module.ttl"),
            demo_module("0.5"),
        )
        .unwrap();

        // The base rubric reconstructed over ALL slices' module.ttl at base MUST carry
        // the demo slice's 0.90 floor — proving the multi-slice git-show base
        // reconstruction sees floors authored OUTSIDE the rubric module (the whole point
        // of the widening; the single-file base read never saw it).
        let base_rubric = base_rubric_at(&fx.root, &base)
            .expect("base reconstruction succeeds")
            .expect("rubric module present at base");
        let base_axis = axis_floors_from_rubric(&base_rubric).unwrap();
        assert_eq!(
            base_axis.get(&demo_key()).copied(),
            Some(0.9),
            "base reconstruction must see the non-rubric slice's floor"
        );

        // The working set, through the real segregated loader, carries 0.50.
        let work_rubric = gmeow_slice_quality::load_repo_rubric(&fx.root).unwrap();
        let work_axis = axis_floors_from_rubric(&work_rubric).unwrap();
        assert_eq!(work_axis.get(&demo_key()).copied(), Some(0.5));

        // The monotonicity comparator, fed the REAL reconstructed base map, reds the
        // lowering and names the non-rubric slice.
        let mono =
            axis_floor_monotonicity(GOVERNANCE_SOURCE_LABEL, &base_axis, &work_axis, |_, _| true);
        assert!(
            mono.violations
                .iter()
                .any(|v| v.contains("sliceDemo") && v.contains("LOWERED")),
            "a lowered non-rubric floor must red naming the slice: {:?}",
            mono.violations
        );
    }

    #[test]
    fn base_reconstruction_reds_a_still_live_non_rubric_floor_deletion() {
        let (fx, base) = fixture_with_base_floor("0.9");
        // Working tree DELETES the demo floor entirely (only the prefix line remains).
        std::fs::write(
            fx.root.join("slices/demo/demo/module.ttl"),
            format!("@prefix gmeow: <{NS}> .\n"),
        )
        .unwrap();

        let base_rubric = base_rubric_at(&fx.root, &base).unwrap().unwrap();
        let base_axis = axis_floors_from_rubric(&base_rubric).unwrap();
        let work_rubric = gmeow_slice_quality::load_repo_rubric(&fx.root).unwrap();
        let work_axis = axis_floors_from_rubric(&work_rubric).unwrap();
        assert_eq!(base_axis.get(&demo_key()).copied(), Some(0.9));
        assert_eq!(work_axis.get(&demo_key()).copied(), None);

        // The slice is still live → deleting its committed floor is a hard violation.
        let mono =
            axis_floor_monotonicity(GOVERNANCE_SOURCE_LABEL, &base_axis, &work_axis, |_, _| true);
        assert!(
            mono.violations
                .iter()
                .any(|v| v.contains("sliceDemo") && v.contains("DELETED")),
            "a still-live non-rubric floor deletion must red naming the slice: {:?}",
            mono.violations
        );
    }

    // -------------------------------------------------------------------------
    // The GRANDFATHER gate's base measurement, over a REAL materialized base tree.
    // -------------------------------------------------------------------------

    const DEMO_SLICE: &str = "https://blackcatinformatics.ca/gmeow/sliceDemo";
    const FRESH_SLICE: &str = "https://blackcatinformatics.ca/gmeow/sliceFresh";

    fn shapes_doc(locals: &[&str]) -> String {
        let mut out =
            format!("@prefix sh: <http://www.w3.org/ns/shacl#> .\n@prefix gmeow: <{NS}> .\n");
        for local in locals {
            out.push_str(&format!("gmeow:{local} a sh:NodeShape .\n"));
        }
        out
    }

    /// A git repo whose BASE commit carries real ratchet AUTHORING surfaces — a slice
    /// `shapes.ttl`, a DEEPLY NESTED `mappings/` file, and the repo-level
    /// `dsl/mappings/` surface — and whose WORKING TREE has deleted every one of them
    /// and added a brand-new slice directory that never existed at base.
    fn fixture_with_base_surfaces() -> (GitFixture, String) {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let mut root = std::env::temp_dir();
        root.push(format!("gmeow-basesurf-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let fx = GitFixture { root: root.clone() };

        let demo_dir = root.join("slices/demo/demo");
        std::fs::create_dir_all(demo_dir.join("mappings/nested")).unwrap();
        std::fs::write(demo_dir.join("manifest.ttl"), "# demo slice\n").unwrap();
        std::fs::write(
            demo_dir.join("module.ttl"),
            format!("@prefix gmeow: <{NS}> .\n"),
        )
        .unwrap();
        std::fs::write(demo_dir.join("shapes.ttl"), shapes_doc(&["BaseA", "BaseB"])).unwrap();
        std::fs::write(
            demo_dir.join("mappings/nested/extra.ttl"),
            shapes_doc(&["BaseC"]),
        )
        .unwrap();
        let dsl_dir = root.join("dsl/mappings");
        std::fs::create_dir_all(&dsl_dir).unwrap();
        std::fs::write(dsl_dir.join("transforms.ttl"), shapes_doc(&["BaseD"])).unwrap();

        git(&root, &["init", "-q"]);
        git(&root, &["config", "user.email", "test@example.com"]);
        git(&root, &["config", "user.name", "Test"]);
        git(&root, &["config", "commit.gpgsign", "false"]);
        git(&root, &["add", "-A"]);
        git(&root, &["commit", "-q", "-m", "base surfaces"]);
        let out = Command::new("git")
            .current_dir(&root)
            .env("HOME", &root)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .args(["rev-parse", "HEAD"])
            .output()
            .expect("git rev-parse runs");
        assert!(out.status.success(), "git rev-parse failed");
        let base = String::from_utf8_lossy(&out.stdout).trim().to_owned();

        // The WORKING tree keeps nothing of the base authoring surfaces, so any residue
        // the base measurement reports can only have come out of the materialized base
        // tree — never out of the files sitting on disk.
        std::fs::remove_file(demo_dir.join("shapes.ttl")).unwrap();
        std::fs::remove_dir_all(demo_dir.join("mappings")).unwrap();
        std::fs::remove_dir_all(&dsl_dir).unwrap();
        // A brand-new slice directory that does not exist at base at all.
        let fresh_dir = root.join("slices/demo/fresh");
        std::fs::create_dir_all(&fresh_dir).unwrap();
        std::fs::write(fresh_dir.join("manifest.ttl"), "# fresh slice\n").unwrap();
        std::fs::write(fresh_dir.join("shapes.ttl"), shapes_doc(&["FreshA"])).unwrap();

        (fx, base)
    }

    fn demo_slices(root: &std::path::Path) -> Vec<(std::path::PathBuf, String)> {
        vec![
            (root.join("slices/demo/demo"), DEMO_SLICE.to_owned()),
            (root.join("slices/demo/fresh"), FRESH_SLICE.to_owned()),
        ]
    }

    #[test]
    fn base_residue_is_read_from_the_materialized_base_tree() {
        let (fx, base) = fixture_with_base_surfaces();
        let vocabularies = vec![gmeow_slice_quality::counting::shacl_vocab()];
        let owned = demo_slices(&fx.root);
        let slices: Vec<(&Path, String)> = owned
            .iter()
            .map(|(d, iri)| (d.as_path(), iri.clone()))
            .collect();
        let needed: std::collections::BTreeSet<String> = [
            DEMO_SLICE.to_owned(),
            FRESH_SLICE.to_owned(),
            gmeow_slice_quality::DSL_MAPPING_SURFACE_IRI.to_owned(),
        ]
        .into_iter()
        .collect();

        let measured = measure_base_residues(&fx.root, &base, &vocabularies, &needed, &slices)
            .unwrap()
            .counts();

        // shapes.ttl (2) + the DEEPLY NESTED mappings file (1) — proving the base tree is
        // scanned by the very same recursive `ratchet_surface_paths` the working tree uses.
        assert_eq!(
            measured
                .get(&(DEMO_SLICE.to_owned(), "sh".to_owned()))
                .copied(),
            Some(3),
            "{measured:?}"
        );
        // The repo-level dsl/mappings surface is measured from the same materialized tree.
        assert_eq!(
            measured
                .get(&(
                    gmeow_slice_quality::DSL_MAPPING_SURFACE_IRI.to_owned(),
                    "sh".to_owned()
                ))
                .copied(),
            Some(1),
            "{measured:?}"
        );
        // A slice directory that does not exist at base contributes NOTHING — the caller
        // reads that as base residue 0, never as the working tree's freshly-authored 1.
        assert!(
            !measured.contains_key(&(FRESH_SLICE.to_owned(), "sh".to_owned())),
            "{measured:?}"
        );
    }

    #[test]
    fn nothing_needed_does_no_git_work_at_all() {
        let (fx, _) = fixture_with_base_surfaces();
        let vocabularies = vec![gmeow_slice_quality::counting::shacl_vocab()];
        let owned = demo_slices(&fx.root);
        let slices: Vec<(&Path, String)> = owned
            .iter()
            .map(|(d, iri)| (d.as_path(), iri.clone()))
            .collect();
        // An unresolvable base ref would hard-fail if git were consulted; with no
        // implicated slice the whole reconstruction is skipped before that can happen.
        let measured = measure_base_residues(
            &fx.root,
            "0000000000000000000000000000000000000000",
            &vocabularies,
            &std::collections::BTreeSet::new(),
            &slices,
        )
        .unwrap();
        assert!(measured.constructs.is_empty() && measured.tree.is_none());
    }

    #[test]
    fn an_unusable_base_ref_hard_fails_rather_than_measuring_zero() {
        let (fx, _) = fixture_with_base_surfaces();
        let vocabularies = vec![gmeow_slice_quality::counting::shacl_vocab()];
        let owned = demo_slices(&fx.root);
        let slices: Vec<(&Path, String)> = owned
            .iter()
            .map(|(d, iri)| (d.as_path(), iri.clone()))
            .collect();
        let needed: std::collections::BTreeSet<String> =
            [DEMO_SLICE.to_owned()].into_iter().collect();
        assert!(
            measure_base_residues(
                &fx.root,
                "0000000000000000000000000000000000000000",
                &vocabularies,
                &needed,
                &slices,
            )
            .is_err(),
            "a base tree that cannot be materialized must HARD FAIL — a silent residue 0 \
             would grandfather freshly-authored constructs for free"
        );
    }
}

/// END-TO-END coverage of the RELOCATION-AWARE ceiling accounting, driven through the
/// real root-parameterized [`slice_quality_gate_at`] against a generated two-state git
/// repository.
///
/// The existing `base_monotonicity_git_tests` harness cannot host these: it declares no
/// `gmeow:ProjectionVocabulary` individual at all (so every ceiling/residue assertion
/// there is vacuously green), and it writes each `manifest.ttl` as the literal
/// `"# rubric slice\n"`, which declares no `gmeow:Slice` — a manifest
/// [`gmeow_slice_quality::slice_iri_of_dir`] HARD-FAILS on. This module therefore builds
/// a real fixture repository: a complete rubric (tier ladder, one axis per implemented
/// primitive, the two dated exemptions the completeness gate demands, and a guarded
/// `sh` vocabulary registry), real slice manifests declaring real `gmeow:Slice`
/// individuals, committed ceilings, authored `gmeow:CeilingRelocation` declarations, and
/// slices carrying genuine residue-producing SHACL triples.
#[cfg(test)]
mod relocation_gate_tests {
    use super::*;
    use std::process::Command;
    use std::sync::atomic::{AtomicU32, Ordering};

    const NS: &str = "https://blackcatinformatics.ca/gmeow/";
    const LOGIC_SLICE: &str = "https://blackcatinformatics.ca/gmeow/slices/logic";
    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// The slice IRI a fixture slice local name resolves to — the on-disk shape every
    /// real manifest uses, so the gate's joins behave exactly as they do in the repo.
    fn slice_iri(local: &str) -> String {
        format!("{NS}slices/{local}")
    }

    /// The term IRI a fixture shape local name resolves to — the relocation-invariant
    /// witness anchor the residue counter records for `gmeow:<local> a sh:NodeShape`.
    fn term_iri(local: &str) -> String {
        format!("{NS}{local}")
    }

    /// One fixture slice: its directory under `slices/`, its local name, and the SHACL
    /// node-shape local names its `shapes.ttl` authors (each one ungrounded residue
    /// anchored on its own term IRI). A local name of `_` authors an ANONYMOUS
    /// `[ sh:path … ]` block instead — a blank-subject construct with no named ancestor,
    /// which is [`gmeow_slice_quality::Witness::NonRelocatable`] and can never witness a
    /// relocation.
    #[derive(Clone)]
    struct SliceSpec {
        dir: String,
        local: String,
        shapes: Vec<String>,
    }

    impl SliceSpec {
        fn new(local: &str, shapes: &[&str]) -> Self {
            Self {
                dir: format!("demo/{local}"),
                local: local.to_owned(),
                shapes: shapes.iter().map(|s| (*s).to_owned()).collect(),
            }
        }
    }

    /// One authored `gmeow:CeilingRelocation` in a fixture rubric.
    #[derive(Clone)]
    struct RelocSpec {
        local: String,
        terms: Vec<String>,
        from: String,
        to: String,
    }

    impl RelocSpec {
        fn new(local: &str, terms: &[&str], from: &str, to: &str) -> Self {
            Self {
                local: local.to_owned(),
                terms: terms.iter().map(|s| (*s).to_owned()).collect(),
                from: from.to_owned(),
                to: to.to_owned(),
            }
        }
    }

    /// One whole repository state: the slices and their residue, the committed
    /// `(slice local, ceiling count)` cells, and the authored relocation declarations.
    #[derive(Clone, Default)]
    struct State {
        slices: Vec<SliceSpec>,
        ceilings: Vec<(String, u64)>,
        relocations: Vec<RelocSpec>,
    }

    impl State {
        fn ceiling(mut self, local: &str, count: u64) -> Self {
            self.ceilings.push((local.to_owned(), count));
            self
        }
        fn reloc(mut self, spec: RelocSpec) -> Self {
            self.relocations.push(spec);
            self
        }
    }

    fn state(slices: &[SliceSpec]) -> State {
        State {
            slices: slices.to_vec(),
            ..State::default()
        }
    }

    struct RepoFixture {
        root: std::path::PathBuf,
    }
    impl Drop for RepoFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn git(root: &std::path::Path, args: &[&str]) {
        let out = Command::new("git")
            .current_dir(root)
            .env("LC_ALL", "C")
            .env("HOME", root)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .args(args)
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// The fixture's `crates/` tree: ONE Rust file defining an item per implemented
    /// axis primitive, so the gate's axis→producer BINDING gate (which resolves every
    /// rubric producer to a real Rust item under `<root>/crates`) is satisfied. The two
    /// exemption producer symbols are deliberately ABSENT so the staleness gate stays
    /// silent — an exemption whose producer resolved would red.
    fn producer_stub_source() -> String {
        let mut out = String::from("// fixture producer stubs\n");
        for producer in gmeow_slice_quality::axes::IMPLEMENTED {
            out.push_str(&format!("fn {producer}() {{}}\n"));
        }
        out
    }

    /// A structurally-complete fixture rubric module: a one-rung ladder, one
    /// `gmeow:QualityAxis` per implemented primitive (each with a `0.0` threshold so
    /// nothing is floored out), the two dated exemptions the completeness gate demands
    /// for the unlanded `gmn` / `docs-panels` projection surfaces, the guarded `sh`
    /// vocabulary registry, and this state's ceiling commitments + relocation
    /// declarations.
    fn rubric_module(state: &State) -> String {
        let mut out = format!(
            r#"@prefix gmeow: <{NS}> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
gmeow:tierRegistered a gmeow:QualityTier ; gmeow:tierRank 0 .
gmeow:thr0 a gmeow:AxisThreshold ; gmeow:thresholdTier gmeow:tierRegistered ; gmeow:thresholdFloor 0.0 .
gmeow:projVocab-sh a gmeow:ProjectionVocabulary ;
    gmeow:vocabularyPrefix "sh" ;
    gmeow:vocabularyNamespace "http://www.w3.org/ns/shacl#"^^xsd:anyURI ;
    gmeow:vocabularySubsumedBy <{LOGIC_SLICE}> ;
    gmeow:vocabularyOwner <{LOGIC_SLICE}> ;
    gmeow:vocabularyCountKind "countKindShape" ;
    gmeow:vocabularyDefaultCeiling 0 ;
    gmeow:vocabularyPreservation gmeow:soundUnder .
"#
        );
        for producer in gmeow_slice_quality::axes::IMPLEMENTED {
            out.push_str(&format!(
                "gmeow:axis-{producer} a gmeow:QualityAxis ; gmeow:axisProducer \"{producer}\" ; gmeow:axisDimension gmeow:dimFixture ; gmeow:axisContextScope gmeow:scopeSliceLocal ; gmeow:axisThreshold gmeow:thr0 .\n"
            ));
        }
        // The two projection surfaces with no landed axis must each carry a dated
        // exemption naming their producer symbol, or the completeness gate reds.
        for (local, producer) in [
            ("exGmn", "GmnProjectionTarget"),
            ("exPanels", "DocMaturityPanels"),
        ] {
            out.push_str(&format!(
                "gmeow:{local} a gmeow:AxisExemption ; gmeow:exemptsAxis gmeow:axis-grounding_axis ; gmeow:exemptionReason \"the producer is genuinely unlanded in this fixture\" ; gmeow:exemptionDate \"2026-07-28\" ; gmeow:exemptionProducer \"{producer}\" .\n"
            ));
        }
        for (local, count) in &state.ceilings {
            out.push_str(&format!(
                "gmeow:pcc-{local}-sh a gmeow:ProjectionCeilingCommitment ; gmeow:ceilingSlice <{}> ; gmeow:ceilingVocabulary gmeow:projVocab-sh ; gmeow:ceilingCount {count} .\n",
                slice_iri(local)
            ));
        }
        for r in &state.relocations {
            let terms = r
                .terms
                .iter()
                .map(|t| format!("<{}>", term_iri(t)))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!(
                "gmeow:{} a gmeow:CeilingRelocation ; gmeow:relocationTerm {terms} ; gmeow:relocationFromSlice <{}> ; gmeow:relocationToSlice <{}> ; gmeow:relocationDate \"2026-07-28\" .\n",
                r.local,
                slice_iri(&r.from),
                slice_iri(&r.to)
            ));
        }
        out
    }

    /// A slice `shapes.ttl` authoring one ungrounded residue construct per local name.
    fn shapes_doc(shapes: &[String]) -> String {
        let mut out =
            format!("@prefix sh: <http://www.w3.org/ns/shacl#> .\n@prefix gmeow: <{NS}> .\n");
        for local in shapes {
            if local == "_" {
                // A blank subject with no named sh:property/sh:node ancestor — a
                // NonRelocatable construct that can never witness a relocation.
                out.push_str("[] sh:path gmeow:anonymousPath .\n");
            } else {
                out.push_str(&format!("gmeow:{local} a sh:NodeShape .\n"));
            }
        }
        out
    }

    /// Write a whole repository state onto `root` (creating every directory).
    fn write_state(root: &std::path::Path, state: &State) {
        let rubric_dir = root.join("slices/core/slice-quality-rubric");
        std::fs::create_dir_all(&rubric_dir).unwrap();
        std::fs::write(
            rubric_dir.join("manifest.ttl"),
            format!(
                "@prefix gmeow: <{NS}> .\n<{}> a gmeow:Slice .\n",
                slice_iri("slice-quality-rubric")
            ),
        )
        .unwrap();
        std::fs::write(rubric_dir.join("module.ttl"), rubric_module(state)).unwrap();
        std::fs::create_dir_all(root.join("crates")).unwrap();
        std::fs::write(root.join("crates/producers.rs"), producer_stub_source()).unwrap();

        // Rewrite every demo slice from scratch so a state transition can DELETE a
        // shape (the departure half of the relocation witness) rather than only add.
        let demo_root = root.join("slices/demo");
        let _ = std::fs::remove_dir_all(&demo_root);
        for s in &state.slices {
            let dir = root.join("slices").join(&s.dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("manifest.ttl"),
                format!(
                    "@prefix gmeow: <{NS}> .\n<{}> a gmeow:Slice .\n",
                    slice_iri(&s.local)
                ),
            )
            .unwrap();
            std::fs::write(dir.join("module.ttl"), format!("@prefix gmeow: <{NS}> .\n")).unwrap();
            std::fs::write(dir.join("shapes.ttl"), shapes_doc(&s.shapes)).unwrap();
        }
    }

    /// Build the fixture repository at `base`, commit it, point `origin/main` at the
    /// commit (the comparand [`resolve_base_ref`] resolves), then overwrite the working
    /// tree with `working`. Returns the fixture; the caller drives
    /// [`slice_quality_gate_at`] over `fixture.root`.
    fn fixture(base: &State, working: &State) -> RepoFixture {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let mut root = std::env::temp_dir();
        root.push(format!("gmeow-reloc-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let fx = RepoFixture { root: root.clone() };

        write_state(&root, base);
        git(&root, &["init", "-q"]);
        git(&root, &["config", "user.email", "test@example.com"]);
        git(&root, &["config", "user.name", "Test"]);
        git(&root, &["config", "commit.gpgsign", "false"]);
        git(&root, &["add", "-A"]);
        git(&root, &["commit", "-q", "-m", "base"]);
        // The gate diffs against `git merge-base HEAD origin/main`; in a fixture repo
        // that ref must exist or the whole comparison is a loud SKIP.
        git(&root, &["update-ref", "refs/remotes/origin/main", "HEAD"]);

        write_state(&root, working);
        fx
    }

    /// The two slices every scenario uses: `alpha` (the source) and `beta` (the
    /// destination).
    fn alpha(shapes: &[&str]) -> SliceSpec {
        SliceSpec::new("alpha", shapes)
    }
    fn beta(shapes: &[&str]) -> SliceSpec {
        SliceSpec::new("beta", shapes)
    }

    fn reloc_s1() -> RelocSpec {
        RelocSpec::new("relocS1", &["S1"], "alpha", "beta")
    }

    #[test]
    fn declared_witnessed_and_paid_transfer_is_accepted() {
        // S1 genuinely DEPARTS alpha and ARRIVES at beta; alpha's ceiling falls by
        // exactly one and beta's brand-new ceiling is pinned to its measured residue.
        // The base ceiling is re-projected through the declared relocation and the
        // unchanged lower-only comparison then holds — the gate is green.
        let base = state(&[alpha(&["S1", "S2", "S3"]), beta(&[])]).ceiling("alpha", 3);
        let working = state(&[alpha(&["S2", "S3"]), beta(&["S1"])])
            .ceiling("alpha", 2)
            .ceiling("beta", 1)
            .reloc(reloc_s1());
        let fx = fixture(&base, &working);
        assert_eq!(
            slice_quality_gate_at(&fx.root),
            0,
            "a declared, witnessed, funded, and pinned transfer must be accepted"
        );
    }

    #[test]
    fn a_copy_rather_than_a_move_is_rejected() {
        // S1 is COPIED: it stays in alpha AND appears in beta — two second sources of
        // truth, strictly worse than one. Nothing departed, so the departure half of
        // the witness is empty and the raise is unwitnessed. Alpha's ceiling is
        // unchanged (nothing left it), so nothing funds beta either.
        let base = state(&[alpha(&["S1", "S2", "S3"]), beta(&[])]).ceiling("alpha", 3);
        let working = state(&[alpha(&["S1", "S2", "S3"]), beta(&["S1"])])
            .ceiling("alpha", 3)
            .ceiling("beta", 1)
            .reloc(reloc_s1());
        let fx = fixture(&base, &working);
        assert_ne!(
            slice_quality_gate_at(&fx.root),
            0,
            "a construct COPIED into a second slice must never be netted as a transfer"
        );
    }

    #[test]
    fn a_lowering_with_no_shared_key_is_rejected() {
        // S3 genuinely DEPARTS alpha (so the declaration itself is corroborated) and
        // alpha's ceiling duly falls by one — but what beta gained is S9, freshly
        // authored there, not S3. `departed(alpha) ∩ arrived(beta)` is empty, so the
        // edge carries no capacity and beta's raise is unwitnessed.
        let base = state(&[alpha(&["S1", "S2", "S3"]), beta(&[])]).ceiling("alpha", 3);
        let working = state(&[alpha(&["S1", "S2"]), beta(&["S9"])])
            .ceiling("alpha", 2)
            .ceiling("beta", 1)
            .reloc(RelocSpec::new("relocS3", &["S3"], "alpha", "beta"));
        let fx = fixture(&base, &working);
        assert_ne!(
            slice_quality_gate_at(&fx.root),
            0,
            "a lowering that shares no witnessed key with the raise funds nothing"
        );
    }

    #[test]
    fn lowering_stale_headroom_buys_nothing() {
        // Alpha's committed ceiling is 9 against a measured residue of 3 — six units of
        // DEAD headroom. It lowers to 2 (a five-unit drop) while only ONE construct
        // actually departed, so the supply clamp (`min(lowering, |departed ∩ declared|)`)
        // caps the credit at one. Beta asks for three (S1 arrived plus two freshly
        // authored), so two units are unpaid.
        let base = state(&[alpha(&["S1", "S2", "S3"]), beta(&[])]).ceiling("alpha", 9);
        let working = state(&[alpha(&["S2", "S3"]), beta(&["S1", "N1", "N2"])])
            .ceiling("alpha", 2)
            .ceiling("beta", 3)
            .reloc(reloc_s1());
        let fx = fixture(&base, &working);
        assert_ne!(
            slice_quality_gate_at(&fx.root),
            0,
            "lowering dead headroom surrenders no authoring and must never buy live headroom"
        );
    }

    #[test]
    fn a_raise_not_pinned_to_measured_is_rejected() {
        // The transfer is fully witnessed and fully funded — S1 departs alpha, arrives
        // at beta, and alpha's ceiling falls by exactly one — but beta ALSO deletes two
        // of its own pre-existing constructs and commits 4 against a measured residue
        // of 2. The flow saturates, the aggregate total is unchanged (6 → 6), and the
        // ONLY thing wrong is that the relocation banked two units of durable surplus
        // headroom, spendable forever with no witness.
        let base = state(&[alpha(&["S1", "A2", "A3"]), beta(&["B1", "B2", "B3"])])
            .ceiling("alpha", 3)
            .ceiling("beta", 3);
        let working = state(&[alpha(&["A2", "A3"]), beta(&["S1", "B1"])])
            .ceiling("alpha", 2)
            .ceiling("beta", 4)
            .reloc(reloc_s1());
        let fx = fixture(&base, &working);
        assert_ne!(
            slice_quality_gate_at(&fx.root),
            0,
            "a raised ceiling must equal the destination's measured residue"
        );
    }

    #[test]
    fn an_undeclared_move_is_rejected() {
        // Exactly the accepted scenario with the gmeow:CeilingRelocation deleted: the
        // witness alone authorizes nothing, because the declaration is a MAINTAINER
        // decision the tool never writes.
        let base = state(&[alpha(&["S1", "S2", "S3"]), beta(&[])]).ceiling("alpha", 3);
        let working = state(&[alpha(&["S2", "S3"]), beta(&["S1"])])
            .ceiling("alpha", 2)
            .ceiling("beta", 1);
        let fx = fixture(&base, &working);
        assert_ne!(
            slice_quality_gate_at(&fx.root),
            0,
            "an undeclared move authorizes no adjustment — the tool never writes the declaration"
        );
    }

    #[test]
    fn a_stale_declaration_is_rejected() {
        // S1 already sits at beta on BOTH sides and nothing departs alpha: the
        // relocation is fully ABSORBED at the merge base. The declaration is dead and
        // must red until deleted, or declarations accumulate into standing permits.
        let base = state(&[alpha(&["S2", "S3"]), beta(&["S1"])])
            .ceiling("alpha", 2)
            .ceiling("beta", 1);
        let working = state(&[alpha(&["S2", "S3"]), beta(&["S1"])])
            .ceiling("alpha", 2)
            .ceiling("beta", 1)
            .reloc(reloc_s1());
        let fx = fixture(&base, &working);
        assert_ne!(
            slice_quality_gate_at(&fx.root),
            0,
            "a declaration whose relocation is fully absorbed at base is dead and must red"
        );
    }

    #[test]
    fn a_blank_subject_construct_cannot_witness_a_relocation() {
        // S1 genuinely DEPARTS alpha (the declaration is corroborated on its source
        // side) and alpha's ceiling falls by one — but what beta actually gained is an
        // anonymous `[ sh:path … ]` block: a blank subject with no named
        // sh:property/sh:node ancestor, hence NO cross-view identity at all. It can
        // never be the arrival half of a witness, so beta's raise is unwitnessed and
        // the refusal says so by name.
        let base = state(&[alpha(&["S1", "_"]), beta(&[])]).ceiling("alpha", 2);
        let working = state(&[alpha(&["_"]), beta(&["_"])])
            .ceiling("alpha", 1)
            .ceiling("beta", 1)
            .reloc(reloc_s1());
        let fx = fixture(&base, &working);
        assert_ne!(
            slice_quality_gate_at(&fx.root),
            0,
            "a blank-subject construct with no named anchor has no cross-view identity"
        );
    }

    #[test]
    fn one_source_cannot_fund_two_destinations() {
        // The exact case a per-destination GREEDY sum gets wrong. Alpha lowers by
        // THREE, and the three constructs it lost landed in BOTH beta and gamma, each
        // of which raises by three. Every arrival at both destinations is genuinely
        // witnessed (each key departed alpha and arrived there), so a greedy
        // per-destination accounting sees `witnessed >= demand` twice and accepts both
        // — then the aggregate conservation check reds with "Σ increased", a verdict
        // that contradicts its own audit lines and names no culprit.
        //
        // The transport solution instead saturates ONE destination, refuses the other,
        // names the blocking edge, and prints the residual demand.
        let base = state(&[
            alpha(&["S1", "S2", "S3", "A4"]),
            beta(&[]),
            SliceSpec::new("gamma", &[]),
        ])
        .ceiling("alpha", 4);
        let working = state(&[
            alpha(&["A4"]),
            beta(&["S1", "S2", "S3"]),
            SliceSpec::new("gamma", &["S1", "S2", "S3"]),
        ])
        .ceiling("alpha", 1)
        .ceiling("beta", 3)
        .ceiling("gamma", 3)
        .reloc(RelocSpec::new(
            "relocBeta",
            &["S1", "S2", "S3"],
            "alpha",
            "beta",
        ))
        .reloc(RelocSpec::new(
            "relocGamma",
            &["S1", "S2", "S3"],
            "alpha",
            "gamma",
        ));
        let fx = fixture(&base, &working);
        assert_ne!(
            slice_quality_gate_at(&fx.root),
            0,
            "one three-unit lowering can fund exactly one three-unit arrival, never two"
        );
    }

    #[test]
    fn a_relocation_into_a_brand_new_cell_passes_the_grandfather_gate() {
        // The destination cell has NO committed ceiling at base at all, so the path the
        // raise actually takes is invariant 3 (the grandfather gate), not the
        // monotonicity comparator. The same declaration, witness, and flow apply — a
        // rule that held at one ceiling gate and not the other would not be a rule.
        // (This is the accepted scenario stated from the grandfather side, with TWO
        // terms moving so the transported amount is more than a single unit.)
        let base = state(&[alpha(&["S1", "S4", "S2"]), beta(&[])]).ceiling("alpha", 3);
        let working = state(&[alpha(&["S2"]), beta(&["S1", "S4"])])
            .ceiling("alpha", 1)
            .ceiling("beta", 2)
            .reloc(RelocSpec::new("relocPair", &["S1", "S4"], "alpha", "beta"));
        let fx = fixture(&base, &working);
        assert_eq!(
            slice_quality_gate_at(&fx.root),
            0,
            "the grandfather gate honours the same relocation adjustment the monotonicity comparator does"
        );
    }

    #[test]
    fn a_legitimate_grandfathered_addition_is_not_red_by_the_conservation_check() {
        // The workflow the ratchet documentation advertises: a slice with PRE-EXISTING
        // residue commits a matching ceiling for the first time. Nothing moved and no
        // declaration exists; the addition is governed by invariant 3 alone. An
        // UNSCOPED Σ would rise here and false-red — the conservation check is scoped
        // to base ∩ working precisely so it does not.
        let base = state(&[alpha(&["S1", "S2"]), beta(&["B1"])]).ceiling("alpha", 2);
        let working = state(&[alpha(&["S1", "S2"]), beta(&["B1"])])
            .ceiling("alpha", 2)
            .ceiling("beta", 1);
        let fx = fixture(&base, &working);
        assert_eq!(
            slice_quality_gate_at(&fx.root),
            0,
            "a new ceiling grandfathering pre-existing base residue is exactly what invariant 3 permits"
        );
    }

    #[test]
    fn an_empty_declaration_set_reproduces_the_pre_relocation_behaviour() {
        // With no gmeow:CeilingRelocation anywhere, inflow is identically zero and the
        // rule degenerates to the original comparator. A hold is clean; a bare raise on
        // a shared key reds; a lowering is clean.
        let base = state(&[alpha(&["S1", "S2"]), beta(&[])]).ceiling("alpha", 2);
        let held = fixture(
            &base,
            &state(&[alpha(&["S1", "S2"]), beta(&[])]).ceiling("alpha", 2),
        );
        assert_eq!(
            slice_quality_gate_at(&held.root),
            0,
            "holding a ceiling is clean with no declarations"
        );

        let lowered_base = state(&[alpha(&["S1", "S2"]), beta(&[])]).ceiling("alpha", 2);
        let lowered = fixture(
            &lowered_base,
            &state(&[alpha(&["S1"]), beta(&[])]).ceiling("alpha", 1),
        );
        assert_eq!(
            slice_quality_gate_at(&lowered.root),
            0,
            "lowering a ceiling to the new measured residue is clean with no declarations"
        );

        let raised_base = state(&[alpha(&["S1", "S2"]), beta(&[])]).ceiling("alpha", 2);
        let raised = fixture(
            &raised_base,
            &state(&[alpha(&["S1", "S2", "S3"]), beta(&[])]).ceiling("alpha", 3),
        );
        assert_ne!(
            slice_quality_gate_at(&raised.root),
            0,
            "a bare raise on a shared key still reds with no declarations — unchanged behaviour"
        );
    }
}

/// End-to-end coverage of the ratchet gate's floor ENFORCEMENT after the
/// governance-source widening — both the gate's per-axis floor DECISION over a floor
/// authored in a NON-rubric slice, and the whole real-repository gate driven through the
/// extracted root-parameterized [`slice_quality_gate_at`].
#[cfg(test)]
mod gate_enforcement_tests {
    use super::*;
    use gmeow_slice_quality::gate::evaluate_axis_floor;
    use std::sync::atomic::{AtomicU32, Ordering};

    const NS: &str = "https://blackcatinformatics.ca/gmeow/";
    static COUNTER: AtomicU32 = AtomicU32::new(0);

    struct TempFixture {
        root: std::path::PathBuf,
    }
    impl Drop for TempFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// A minimal on-disk fixture repo (no git): a complete rubric slice plus a
    /// non-rubric demo slice authoring one `gmeow:AxisFloorCommitment` at `floor`.
    fn fixture_with_non_rubric_floor(floor: &str) -> TempFixture {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let mut root = std::env::temp_dir();
        root.push(format!("gmeow-gate-enforce-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let rubric_dir = root.join("slices/core/slice-quality-rubric");
        let demo_dir = root.join("slices/demo/demo");
        std::fs::create_dir_all(&rubric_dir).unwrap();
        std::fs::create_dir_all(&demo_dir).unwrap();
        std::fs::write(
            rubric_dir.join("module.ttl"),
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
"#
            ),
        )
        .unwrap();
        std::fs::write(rubric_dir.join("manifest.ttl"), "# rubric slice\n").unwrap();
        std::fs::write(
            demo_dir.join("module.ttl"),
            format!(
                r#"@prefix gmeow: <{NS}> .
gmeow:afc-demo a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceDemo ;
    gmeow:floorAxis gmeow:axisGmn1Coverage ;
    gmeow:floorValue {floor} .
"#
            ),
        )
        .unwrap();
        std::fs::write(demo_dir.join("manifest.ttl"), "# demo slice\n").unwrap();
        TempFixture { root }
    }

    #[test]
    fn a_non_rubric_slice_floor_is_enforced_by_the_gate_per_axis_decision() {
        // A floor committed at 1.0 in a NON-rubric slice's module.ttl.
        let fx = fixture_with_non_rubric_floor("1.0");
        let slice = format!("{NS}sliceDemo");

        // The gate reads floors through the same segregated loader; project them into the
        // (slice, axis-local) → floor map the per-axis floor pass consumes.
        let rubric = gmeow_slice_quality::load_repo_rubric(&fx.root).unwrap();
        let axis_floors = axis_floors_from_rubric(&rubric).unwrap();

        // The gate's per-axis floor RESOLUTION (`axis_floor_for`) now RESOLVES the
        // non-rubric floor — before the widening the loader never saw it, so this
        // returned None and the axis went silently unfloored.
        let resolved = axis_floor_for(&axis_floors, &slice, "axisGmn1Coverage", false)
            .unwrap()
            .expect("the non-rubric slice's committed floor is resolved by the gate");
        assert_eq!(resolved, 1.0);

        // The gate's per-axis VERDICT reds a measured score below the committed floor,
        // and passes one that meets it — exactly the decision the gate emits per grade.
        assert!(
            evaluate_axis_floor(0.5, resolved).is_failure(),
            "a measured score below the committed non-rubric floor must red the gate"
        );
        assert!(
            !evaluate_axis_floor(1.0, resolved).is_failure(),
            "a measured score meeting the floor must not red"
        );
    }

    #[test]
    fn the_real_repository_slice_quality_gate_is_green_end_to_end() {
        // Drive the WHOLE gate through the extracted root-parameterized entry against the
        // live checkout. Exit 0 confirms (a) `slice_quality_gate_at` runs end-to-end and
        // (b) the governance-source widening did not red the real gate — only the rubric
        // slice authors floors today, so the union equals the pre-widening set. This is
        // the production-surface analog of the `make check` slice-quality gate.
        assert_eq!(
            slice_quality_gate_at(&project_root()),
            0,
            "the real-repo slice-quality gate must stay green after the governance-source widening"
        );
    }
}
