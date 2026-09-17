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
// The ONE merge-base comparand resolver in the workspace. The floor/ceiling ratchets here
// and the model-facing freeze in `crates/pipeline/tests` are both defined as a comparison
// against `git merge-base HEAD origin/main`; two copies of the resolver would be two
// notions of what this branch is being compared to.
use gmeow_pipeline::branch_base::{BaseFile, BaseRef, git_show_base, resolve_base_ref};
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
    match score_slice_with_standard(
        &dir,
        &standard,
        ScoringEnv::Repo {
            slice_dir: dir.clone(),
        },
    ) {
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
    // The assessments behind `rdf_out`, retained so the corpus header can carry the
    // `gmeow:contentDigest` fold over the grades it publishes (the record-integrity
    // witness a reader recomputes). Populated only on the RDF path.
    let mut rdf_assessments: Vec<SliceAssessment> = Vec::new();
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
                    Format::Rdf => {
                        rdf_out.push_str(&report.to_gmeow_rdf());
                        // The corpus header digests these grades, so the assessment
                        // itself is retained, not just its rendered block.
                        rdf_assessments.push(report.assessment.clone());
                    }
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
        Format::Rdf => {
            // Prefix the corpus-level freshness witness so the printed RDF is the SAME
            // document the pipeline records — a reader of either can prove which
            // authored sources produced the grades instead of guessing at its vintage.
            match gmeow_slice_quality::scored_input_fingerprint(root) {
                Ok(fingerprint) => print!(
                    "{}{rdf_out}",
                    gmeow_slice_quality::report::corpus_fingerprint_nquads(
                        &fingerprint,
                        &gmeow_slice_quality::report::corpus_content_digest(&rdf_assessments),
                    )
                ),
                Err(e) => return fail(format!("slice-quality: {e}")),
            }
        }
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
    // LOAD the recorded grade vectors rather than re-scoring every slice. The pipeline
    // already scores each discovered slice exactly once at the DAG root and projects
    // the result to `graph/quality-assessment` (on disk,
    // `generated/quality/gmeow.quality-assessment.nt`); this gate used to run the same
    // sweep, with the same rubric and the same `ScoringEnv::Repo`, a second time.
    //
    // Nothing about what the gate ASSERTS moves: every pass below — the roll-up tier
    // ratchet, the per-axis committed floors, coat distinctiveness, residue ceilings,
    // the merge-base floor diff, floor monotonicity — runs unchanged over these
    // grades. Three properties, all ENFORCED rather than assumed, make the loaded
    // vector interchangeable with a freshly-scored one:
    //
    //  * losslessness — every field round-trips exactly (the axis is a first-class
    //    `gmeow:assessmentAxis`, the score the shortest round-tripping f64 lexical, the
    //    tier an IRI resolved against this same ladder), pinned by
    //    `rdf_projection::recorded_grades_round_trip_exactly`;
    //  * completeness — a slice missing from the record, or missing an axis the rubric
    //    declares, is a HARD FAIL in the reader, so a truncated record can never read
    //    as a passing one;
    //  * freshness — `verify_fresh` recomputes the corpus fingerprint over the authored
    //    sources and hard-fails on any drift, so a stale record is an error and never a
    //    silent pass. `make slice-quality-gate` carries no `sync` prerequisite, so this
    //    check is what keeps the STANDALONE invocation as sound as the in-DAG one.
    let corpus = match gmeow_slice_quality::read::read_recorded_corpus(&root, &rubric.standard) {
        Ok(c) => c,
        Err(e) => return fail(format!("slice-quality-gate: {e}")),
    };
    if let Err(e) = corpus.verify_fresh(&root) {
        return fail(format!("slice-quality-gate: {e}"));
    }
    let mut scored: Vec<(&Path, &SliceAssessment)> = Vec::with_capacity(dirs.len());
    for dir in &dirs {
        // The slice IRI is resolved from the slice's own manifest by the SAME authority
        // the scorer used, so the join between a discovered directory and its recorded
        // assessment cannot drift from the one the projection was keyed on.
        let slice_iri = match gmeow_slice_quality::slice_iri_of_dir(dir) {
            Ok(iri) => iri,
            Err(e) => return fail(format!("slice-quality-gate: {}: {e}", dir.display())),
        };
        let assessment = match corpus.assessment(&slice_iri) {
            Ok(a) => a,
            Err(e) => return fail(format!("slice-quality-gate: {}: {e}", dir.display())),
        };
        scored.push((dir.as_path(), assessment));
    }

    let mut failures = 0usize;
    let mut checked = 0usize;
    for (dir, assessment) in &scored {
        let declared = match gmeow_slice_quality::gate::declared_tier(dir, &rubric) {
            Ok(d) => d,
            Err(e) => return fail(format!("slice-quality-gate: {e}")),
        };
        let Some(declared) = declared else { continue }; // undeclared → advisory
        checked += 1;
        let measured_rank = assessment.rollup.rank;
        let floor_rank = floors.get(&assessment.slice).map(|f| f.rank);
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
                    assessment.slice, declared.label, assessment.rollup.label
                );
            }
            RatchetVerdict::MeasuredBelowDeclared => {
                emit_error(
                    "gmeow-dev.slice-quality.gate",
                    format!(
                        "FAIL {} declared {} but measures {} — uplift the slice or lower is forbidden",
                        assessment.slice, declared.label, assessment.rollup.label
                    ),
                );
                failures += 1;
            }
            RatchetVerdict::DeclaredBelowFloor => {
                emit_error(
                    "gmeow-dev.slice-quality.gate",
                    format!(
                        "FAIL {} declares {} below its committed ratchet floor — the tier may only be raised",
                        assessment.slice, declared.label
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
    for (dir, assessment) in &scored {
        live_slices.insert(assessment.slice.clone());
        let grounding = is_grounding_slice(dir);
        // The recorded corpus carries the GRADES but not the per-term advisories that
        // produced them, so a below-floor axis re-scores THIS slice — once, lazily, and
        // only this slice — to name the terms behind the number. The passing route
        // never enters that arm, so the recorded-corpus fast path is untouched: scoring
        // one slice is the price of diagnosing an already-red gate, not a return to
        // sweeping the corpus.
        let mut advisory_source: Option<SliceReport> = None;
        for grade in &assessment.grades {
            let axis_local = axis_local_name(&grade.axis_iri);
            let floor = match axis_floor_for(&axis_floors, &assessment.slice, axis_local, grounding)
            {
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
                            // Both numbers at FULL precision, never `{:.6}`: the
                            // comparison is exact (`measured + f64::EPSILON >= floor`),
                            // so a rounded message can read "1.000000 below 1.000000"
                            // and leave the reader unable to see the regression it is
                            // reporting.
                            "FAIL {} measures {axis_local} {} — below its committed per-axis floor {floor} ({AXIS_FLOOR_PROJECTION})",
                            assessment.slice, grade.score
                        ),
                    );
                    // The failing axis already knows, per term, WHAT is wrong; print it
                    // beneath the FAIL instead of making the author rebuild the set by
                    // hand from a single aggregate line.
                    if advisory_source.is_none() {
                        let one = [dir.to_path_buf()];
                        let mut scored_one =
                            gmeow_slice_quality::score_slices_with_rubric(&root, &one, &rubric);
                        match scored_one.remove(0) {
                            Ok(r) => advisory_source = Some(r),
                            Err(e) => {
                                return fail(format!("slice-quality-gate: {}: {e}", dir.display()));
                            }
                        }
                    }
                    if let Some(report) = advisory_source.as_ref() {
                        emit_axis_floor_advisories(
                            &assessment.slice,
                            axis_local,
                            &report.advisories_for_axis(&grade.axis_iri),
                        );
                    }
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
    // Every discovered slice's (dir, IRI) pair, resolved ONCE by the corpus join above.
    // The merge-base residue reconstruction reuses it instead of re-parsing every
    // slice's manifest.ttl a second time just to discard all but the implicated few.
    let slice_dirs: Vec<(&Path, String)> = scored
        .iter()
        .map(|(dir, assessment)| (*dir, assessment.slice.clone()))
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
                    let rebalance = match ceiling_rebalance(&RebalanceInputs {
                        root: &root,
                        base: &base,
                        vocabularies,
                        slice_dirs: &slice_dirs,
                        declarations,
                        base_ceilings: &base_ceilings,
                        working_ceilings: &working_ceilings,
                        working_residues: &working_residues,
                        working_constructs: &working_constructs,
                        needed: &needed,
                    }) {
                        Ok(r) => r,
                        Err(e) => return fail(format!("slice-quality-gate: {e}")),
                    };
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

/// The finding code each WITNESS of a below-floor axis advisory is interned under —
/// the antecedent node the advisory hangs its DAG edge on. For a translation
/// advisory the witness is the catalog entry's `msgctxt`.
const AXIS_ADVISORY_WITNESS_CODE: &str = "gmeow-dev.slice-quality.axis-floor.witness";

/// Mint the failing axis's per-term advisories onto a [`gmeow_errors::DiagLedger`]
/// and project the result onto the console sink, beneath the axis's FAIL line.
///
/// The gate holds the full [`gmeow_slice_quality::SliceReport`] — including every
/// advisory the axis produced, each naming the offending term IRI (and, for
/// translation, the uncovered `(term, predicate)` and the reason) — yet printed only
/// the one-line aggregate FAIL. Reconstructing that detail by hand afterwards is
/// days of work and produces a detector that disagrees with the scorer; the scorer
/// already knows, so it says so.
///
/// Every line goes through the LEDGER, never a bare `println!` or a hand-built
/// `Finding`: each advisory becomes a content-addressed witness anchored on the term
/// it concerns (its [`gmeow_errors::Finding::documented_terms`] entry — read as DATA,
/// never re-parsed out of the message prose), with its witnesses interned as
/// ANTECEDENTS. A producer that bypasses the ledger carries no fingerprint identity,
/// no anchor, and no antecedents, so nothing can join it and it derives DARK.
///
/// The advisories are NOTE-grade here: the FAIL above already gates and is already
/// counted, so re-grading its explanation as a second error would double-count.
fn emit_axis_floor_advisories(
    slice_iri: &str,
    axis_local: &str,
    advisories: &[&gmeow_errors::Finding],
) {
    if advisories.is_empty() {
        return;
    }
    crate::dev_common::emit_report(&axis_floor_advisory_report(
        slice_iri, axis_local, advisories,
    ));
}

/// The ledger PROJECTION [`emit_axis_floor_advisories`] prints — split out so the
/// minted witnesses (their anchors, their antecedent edges, their messages) are
/// assertable without capturing a console stream.
fn axis_floor_advisory_report(
    slice_iri: &str,
    axis_local: &str,
    advisories: &[&gmeow_errors::Finding],
) -> gmeow_errors::Report {
    use gmeow_errors::{DiagLedger, StageId};
    let stage = StageId::new("slice-quality-gate");
    let mut ledger = DiagLedger::new();
    // The fallback anchor when an advisory concerns no single documented term (an
    // axis-level advice template, a whole-catalog parse error): the failing cell
    // itself, so the witness still lands somewhere joinable rather than nowhere.
    let cell_anchor = format!("{slice_iri}#{axis_local}");
    for advisory in advisories {
        let antecedents: Vec<gmeow_errors::DiagRef> = advisory
            .related_locations
            .iter()
            .filter_map(|loc| loc.logical.as_deref())
            .map(|witness| {
                let diag = gmeow_errors::Diag::note(
                    gmeow_errors::register_code(AXIS_ADVISORY_WITNESS_CODE),
                    format!("{axis_local} advisory witness: {witness}"),
                )
                .with_focus(witness.to_owned())
                .with_location(gmeow_errors::Location {
                    logical: Some(witness.to_owned()),
                    ..gmeow_errors::Location::default()
                });
                ledger.attach(diag, stage.clone())
            })
            .collect();
        // ANCHOR = the documented term the advisory concerns (the join key two
        // different-code findings about one term share); falls back to the failing
        // cell so a term-less advisory still lands somewhere joinable.
        let anchor = advisory
            .documented_terms
            .first()
            .cloned()
            .unwrap_or_else(|| cell_anchor.clone());
        // POSITION = what distinguishes THIS advisory from its same-code siblings.
        // The ledger content-addresses on (code, category, position, focus) and never
        // on the message, so without a distinct position `fr does not cover X` and
        // `cmn does not cover X` merge into one node and a whole language vanishes
        // from the report. The producer states the position (`with_position`); this
        // reads it, falling back to the anchor when there are no siblings to separate.
        let position = advisory
            .locations
            .iter()
            .find_map(|loc| loc.logical.clone())
            .unwrap_or_else(|| anchor.clone());
        let mut diag = gmeow_errors::Diag::note(
            gmeow_errors::register_code(&advisory.code),
            advisory.message.clone(),
        )
        .with_focus(anchor)
        .with_location(gmeow_errors::Location {
            logical: Some(position),
            ..gmeow_errors::Location::default()
        })
        .with_antecedents(antecedents);
        for term in &advisory.documented_terms {
            diag = diag.with_documented_term(term.clone());
        }
        ledger.attach(diag, stage.clone());
    }
    ledger.project_report("gmeow-dev").normalized()
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
/// monotonicity, grandfather) reads. The rubric loader already enforces
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
/// materialized by ONE `git archive` (see [`materialize_base_tree`]).
///
/// The extraction root is a [`tempfile::TempDir`], so it is removed on drop — on the
/// success path, on every `?` early return, and while unwinding from a panic. A
/// hand-rolled `remove_dir_all` in a `Drop` impl covered only the first two and is banned
/// repo-wide (`check_no_unmanaged_temp_dir`) for exactly that reason.
struct BaseSurfaces {
    /// The RAII guard owning the extraction root. Never read directly — [`Self::root`]
    /// is the accessor — but its lifetime is what keeps the tree on disk.
    tmp: tempfile::TempDir,
}

impl BaseSurfaces {
    /// The extraction root; base surfaces sit under it at their repo-relative paths.
    fn root(&self) -> &Path {
        self.tmp.path()
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
) -> gmeow_errors::Result<BaseSurfaces> {
    use std::process::{Command, Stdio};

    // A `pid-seq` path is guessable on a machine shared by 30+ developers: a symlink
    // pre-planted at the predicted path, combined with `create_dir_all` succeeding
    // THROUGH an existing symlink, would let `tar -x` write the base tree outside this
    // temp root (TOCTOU). `tempfile` closes that on both counts — the suffix is drawn
    // from the OS CSPRNG rather than from the pid, and the directory is created with
    // `mkdir` (which FAILS on any pre-existing path, symlink included, and never
    // follows one) and retried on collision — and it additionally removes the tree
    // while unwinding from a panic, which the hand-rolled `Drop` did not.
    let tmp = tempfile::Builder::new()
        .prefix("gmeow-ratchet-base-")
        .tempdir()
        .map_err(|e| sqe(format!("could not create base-tree temp dir: {e}")))?;
    // Construct the guard BEFORE anything else can fail, so every path below cleans up.
    let tree = BaseSurfaces { tmp };

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
        .current_dir(tree.root())
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
    tree: Option<BaseSurfaces>,
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
            gmeow_slice_quality::ratchet_dsl_surface_paths(tree.root())
        } else {
            gmeow_slice_quality::ratchet_surface_paths(&tree.root().join(rel))
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
        let paths = gmeow_slice_quality::ratchet_surface_paths(&tree.root().join(rel_dir));
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
        let paths = gmeow_slice_quality::ratchet_dsl_surface_paths(tree.root());
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
/// constructs is not conserved across the move — the three declared reason codes
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
                // Two `gmeow:CeilingRelocation` individuals may legitimately share the
                // same `(from, to, vocab)` edge — the gate itself merges witnesses and
                // declaration IRIs per edge (`network.witnesses`/`network.declarations`
                // in `gate.rs`) — so a plain `insert` here would silently DROP the
                // earlier declaration's reason codes rather than merge them. Union
                // per-anchor instead.
                let edge = out
                    .entry((
                        d.from_slice.clone(),
                        d.to_slice.clone(),
                        vocab.prefix.clone(),
                    ))
                    .or_default();
                for (anchor, codes) in scoped {
                    edge.entry(anchor).or_default().extend(codes);
                }
            }
        }
    }
    Ok(out)
}

/// The projection-ceiling REBALANCE verdict for one comparison: base residue
/// measurement (scoped to `needed`) → derived edge reasons → default-ceiling
/// projection → [`gmeow_slice_quality::gate::projection_ceiling_monotonicity`].
///
/// This is the exact tail both [`slice_quality_gate_at`] and the test-only
/// `rebalance_for` helper compose — extracted into ONE function so a change to
/// any of its steps (how the base is measured, how edge reasons are derived, how
/// the default-ceiling map is built, or which `CeilingComparison` fields feed the
/// gate) cannot silently drift between the production gate and the fixture-driven
/// test helper. A hand-duplicated copy is exactly the failure mode this closes:
/// every `assert_violation_contains` fixture would otherwise keep asserting
/// against a stale composition and silently stop covering the real gate.
///
/// `needed` decides which slices the base residue measurement bothers reading:
/// the production gate passes the implicated-only pre-filter (cells whose
/// committed ceiling is new or raised, plus every declared relocation's
/// endpoints — a repo can have hundreds of slices and most need no base read at
/// all); the test helper passes every fixture slice (fixtures are tiny, so
/// pre-filtering buys nothing and a complete measurement is easier to reason
/// about against the assertions).
///
/// The inputs [`ceiling_rebalance`] composes — grouped into one struct (mirroring
/// [`gmeow_slice_quality::gate::CeilingComparison`]'s own shape) rather than a long
/// positional argument list, so the production gate and the test helper cannot
/// silently swap two same-typed arguments past each other.
#[derive(Clone, Copy)]
struct RebalanceInputs<'a> {
    root: &'a Path,
    base: &'a str,
    vocabularies: &'a [gmeow_slice_quality::model::ProjectionVocabulary],
    slice_dirs: &'a [(&'a Path, String)],
    declarations: &'a [gmeow_slice_quality::CeilingRelocation],
    base_ceilings: &'a std::collections::BTreeMap<(String, String), u64>,
    working_ceilings: &'a std::collections::BTreeMap<(String, String), u64>,
    working_residues: &'a std::collections::BTreeMap<(String, String), u64>,
    working_constructs:
        &'a std::collections::BTreeMap<(String, String), Vec<gmeow_slice_quality::Construct>>,
    needed: &'a std::collections::BTreeSet<String>,
}

/// # Errors
/// Returns a message on a base residue measurement or edge-reason derivation
/// failure.
fn ceiling_rebalance(
    inputs: &RebalanceInputs<'_>,
) -> gmeow_errors::Result<gmeow_slice_quality::gate::CeilingRebalance> {
    let RebalanceInputs {
        root,
        base,
        vocabularies,
        slice_dirs,
        declarations,
        base_ceilings,
        working_ceilings,
        working_residues,
        working_constructs,
        needed,
    } = *inputs;
    let base_meas = measure_base_residues(root, base, vocabularies, needed, slice_dirs)?;
    let base_measured = base_meas.counts();
    let edge_reasons = derive_edge_reasons(&base_meas, declarations, vocabularies, slice_dirs)?;
    let default_ceiling_by_prefix: std::collections::BTreeMap<String, u64> = vocabularies
        .iter()
        .map(|v| (v.prefix.clone(), v.default_ceiling))
        .collect();
    Ok(gmeow_slice_quality::gate::projection_ceiling_monotonicity(
        &gmeow_slice_quality::gate::CeilingComparison {
            file_label: GOVERNANCE_SOURCE_LABEL,
            base_ceilings,
            working_ceilings,
            base_measured: &base_measured,
            working_measured: working_residues,
            base_constructs: &base_meas.constructs,
            working_constructs,
            default_ceilings: &default_ceiling_by_prefix,
            declarations,
            edge_reasons: &edge_reasons,
        },
    ))
}

#[path = "dev_slice_quality.edge_reason_merge_tests.rs"]
#[cfg(test)]
mod edge_reason_merge_tests;

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
/// (`rubric.floors.vocabularies` — the ontology-resident guarded set) and
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

    // The guarded set must be loaded (the ontology-resident registry) — an
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
/// The `demand`/supply-headroom ARITHMETIC below is the same per-cell bookkeeping
/// [`gmeow_slice_quality::gate::projection_ceiling_monotonicity`] performs before it
/// ever asks whether a raise is payable — that part is not "the solver", it is
/// deriving this single proposed move's inputs. But WHETHER the raise is payable is
/// answered by building a [`gmeow_slice_quality::gate::Transport`] (one source, one
/// destination, one witnessed edge) and calling
/// [`gmeow_slice_quality::gate::solve_transport`] on it — the EXACT function the gate
/// itself calls. `gmeow-dev-cli/src/lib.rs`'s seed-command doctrine states the
/// discipline this exists to uphold: "seed and gate can never diverge" — a second,
/// hand-rolled flow computation here could promise an acceptance the gate then
/// refuses, which is exactly the bug this delegation forecloses. (A single `--from`/
/// `--to` pair is always a one-source/one-destination network, where a greedy pass
/// and a true max flow agree — but the point is there is now only ONE algorithm that
/// could ever answer this question, not two that happen to agree today.)
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
    // The source's lowering-to-post-move-measured headroom, clamped to what
    // actually moves — the same `lowering.min(live)` supply the gate computes per
    // source cell before it ever builds a `Transport`.
    let supply = from_ceiling
        .saturating_sub(from_measured.saturating_sub(moving))
        .min(moving);
    let demand = (to_measured + moving).saturating_sub(to_ceiling);
    if demand == 0 {
        // The gate's own per-vocabulary loop `continue`s before building a
        // `Transport` at all when nothing is being raised (`gate.rs`:
        // `if network.demand.is_empty() { continue; }`) — there is nothing to
        // solve, so `credit` here is purely informational: the headroom the
        // source's lowering COULD pay, not what a flow actually delivered.
        return RelocationPlan {
            credit: supply,
            demand: 0,
            unpaid: 0,
        };
    }
    const FROM: &str = "from";
    const TO: &str = "to";
    let mut network = gmeow_slice_quality::gate::Transport::default();
    if supply > 0 {
        network.supply.insert(FROM.to_owned(), supply);
    }
    network.demand.insert(TO.to_owned(), demand);
    if moving > 0 {
        network
            .capacity
            .insert((FROM.to_owned(), TO.to_owned()), moving);
    }
    let flow = gmeow_slice_quality::gate::solve_transport(&network);
    let credit = flow
        .edges
        .get(&(FROM.to_owned(), TO.to_owned()))
        .copied()
        .unwrap_or(0);
    let unpaid = flow.residual.get(TO).copied().unwrap_or(demand);
    RelocationPlan {
        credit,
        demand,
        unpaid,
    }
}

/// How many relocatable anchor terms the preview's DISCOVERY listing prints per
/// vocabulary before it truncates. A slice can carry dozens of anchors and the listing
/// is a navigation aid, not a report — but a silent truncation would be a lie about
/// what is movable, so the cap is always accompanied by an explicit "… and N more"
/// line naming the remainder count.
const RELOCATION_ANCHOR_LISTING_CAP: usize = 20;

/// Split the requested `terms` that contributed nothing to the transport plan (none of
/// them anchors any residue construct in `from_iri`) into the two semantically distinct
/// reasons that can be:
///
/// - **absent** — the term anchors NO residue construct anywhere the preview measures
///   (neither `from_iri` nor `to_iri`). A relocation of this term genuinely moves
///   nothing; the term is likely mistyped or was never authored.
/// - **unwitnessed** — the term DOES anchor residue, just at the DESTINATION rather
///   than the source. There is no departure to pair with an arrival, so this specific
///   `from → to` declaration could never be corroborated as witnessed — mirroring the
///   departed/arrived pairing
///   [`gmeow_slice_quality::gate::projection_ceiling_monotonicity`] requires of an
///   authored `gmeow:CeilingRelocation` (`crates/slice-quality/src/gate.rs`,
///   `departed`/`arrived`). This is exactly the shape of mistake a maintainer makes
///   running the preview as its own negative control (e.g. `--from`/`--to` swapped):
///   the term is real and does carry residue, so telling them "nothing would move"
///   would be silently wrong.
///
/// Order-preserving over `terms` so the printed lists read in the order the caller
/// asked for them.
fn classify_unmoved_terms<'a>(
    terms: &'a [String],
    to_residue: &std::collections::BTreeMap<String, Vec<gmeow_slice_quality::Construct>>,
    vocabularies: &[gmeow_slice_quality::model::ProjectionVocabulary],
) -> (Vec<&'a str>, Vec<&'a str>) {
    let to_anchors: std::collections::BTreeSet<&str> = vocabularies
        .iter()
        .filter_map(|v| to_residue.get(&v.prefix))
        .flat_map(|constructs| constructs.iter())
        .filter_map(|c| c.witness.anchor())
        .collect();
    let mut unwitnessed = Vec::new();
    let mut absent = Vec::new();
    for term in terms {
        if to_anchors.contains(term.as_str()) {
            unwitnessed.push(term.as_str());
        } else {
            absent.push(term.as_str());
        }
    }
    (unwitnessed, absent)
}

/// The human-facing line for the `absent` half of `classify_unmoved_terms`'s split (a
/// term anchoring no residue construct in either slice — a relocation of it genuinely
/// moves nothing). Deliberately scoped wording, never a universal "NONE of the
/// requested terms": `absent` is a SUBSET of `terms` (the other subset is
/// `unwitnessed`, printed separately), so a mixed request must never read as if the
/// ENTIRE requested set were absent. A stand-alone, testable function so the exact
/// production wording is pinned by a test rather than merely eyeballed.
fn absent_terms_message(
    absent: &[&str],
    total_requested: usize,
    from_iri: &str,
    to_iri: &str,
) -> String {
    format!(
        "# absent: {} of {total_requested} requested term(s) anchor no residue construct in {from_iri} or {to_iri} — those would move nothing: {}",
        absent.len(),
        absent.join(", ")
    )
}

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
/// When NONE of the requested terms contributes to the transport plan, it distinguishes
/// two semantically different reasons ([`classify_unmoved_terms`]) rather than
/// conflating them into one misleading "nothing would move": a term ABSENT from both
/// slices' residue genuinely moves nothing, while a term already anchoring residue at
/// the DESTINATION is UNWITNESSED — real, but with no departure from the requested
/// source to pair with an arrival. Either way it then prints the DISCOVERY listing
/// ([`print_relocatable_anchors`]): every term in the source that DOES anchor residue,
/// per vocabulary, with the construct count each would carry.
///
/// Then, once, the AXIS-FLOOR COLLATERAL: every committed `gmeow:AxisFloorCommitment`
/// on either slice with its live measured score and headroom.
///
/// Never gates on its findings; malformed input, rubric-load and measurement errors
/// still fail non-zero (an unresolvable `--from`/`--to`, `from == to`, a rubric-load
/// or empty-registry error, and every residue/axis-floor measurement error below —
/// `return fail(...)` at every one of those sites, never a swallowed `Result`). Its
/// numbers are never fed back into a ceiling (a ceiling is lowered only by a
/// deliberate hand-edit after a genuine measured migration, and raised only through
/// an authored `gmeow:CeilingRelocation` the gate then corroborates against the
/// derived witness).
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
        let (unwitnessed, absent) = classify_unmoved_terms(terms, &to_residue, vocabularies);
        // The two cases are semantically different and must never be conflated: a
        // maintainer running this preview in the wrong direction (its own negative
        // control, e.g. --from/--to swapped) must not be told a real, existing term
        // "would move nothing" when it genuinely anchors residue — just not at the
        // requested source.
        if !unwitnessed.is_empty() {
            println!(
                "# unwitnessed: {} of {} requested term(s) already anchor residue in {to_iri} (the DESTINATION), not {from_iri} — there is no departure to pair with an arrival, so this from→to move cannot be witnessed for: {}",
                unwitnessed.len(),
                terms.len(),
                unwitnessed.join(", ")
            );
        }
        if !absent.is_empty() {
            println!(
                "{}",
                absent_terms_message(&absent, terms.len(), from_iri, to_iri)
            );
        }
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

#[path = "dev_slice_quality.relocation_preview_tests.rs"]
#[cfg(test)]
mod relocation_preview_tests;

#[path = "dev_slice_quality.min_tier_tests.rs"]
#[cfg(test)]
mod min_tier_tests;

#[path = "dev_slice_quality.floor_projection_tests.rs"]
#[cfg(test)]
mod floor_projection_tests;

#[path = "dev_slice_quality.seed_floors_tests.rs"]
#[cfg(test)]
mod seed_floors_tests;

#[path = "dev_slice_quality.base_monotonicity_git_tests.rs"]
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
mod base_monotonicity_git_tests;

#[path = "dev_slice_quality.relocation_gate_tests.rs"]
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
mod relocation_gate_tests;

#[path = "dev_slice_quality.gate_enforcement_tests.rs"]
/// End-to-end coverage of the ratchet gate's floor ENFORCEMENT after the
/// governance-source widening — both the gate's per-axis floor DECISION over a floor
/// authored in a NON-rubric slice, and the whole real-repository gate driven through the
/// extracted root-parameterized [`slice_quality_gate_at`].
#[cfg(test)]
mod gate_enforcement_tests;

#[path = "dev_slice_quality.axis_floor_diagnostics_tests.rs"]
/// The BELOW-FLOOR DIAGNOSTIC: when an axis measures under its committed floor, the
/// gate must print that axis's per-term advisories, not only the one-line aggregate.
///
/// The fixture forces a demo slice under a 1.0 floor on two axes at once —
/// `axisProseQuality` (a definition with no boundary, an example that is not a worked
/// triple) and `axisTranslationCoverage` (no catalogs at all) — and asserts the
/// LEDGER PROJECTION the gate prints names the offending term IRI, the uncovered
/// `(term, predicate)` pairs with their reasons, and carries the ledger identity
/// (`finding_iri` + anchor + antecedents) without which a reasoner pass over the
/// finding graph could not join it.
#[cfg(test)]
mod axis_floor_diagnostics_tests;

#[cfg(test)]
#[path = "dev_slice_quality_test_support.rs"]
mod test_support;
#[cfg(test)]
use test_support::load_rubric_from_ttl;
