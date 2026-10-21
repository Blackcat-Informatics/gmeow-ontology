// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `bench` perf surface: the committed perf leaderboard generator
//! plus the report-only regression scoreboard.
//!
//! Timings are non-deterministic, so the load-bearing line is between two
//! halves:
//!
//!   * **Committed, drift-gated** — [`render_bench_leaderboard`] reads the
//!     committed reference run (`bench/baseline.json`) and renders a *purely
//!     deterministic* Markdown leaderboard (`generated/bench/leaderboard.md`).
//!     The values are recorded as integer nanoseconds, so there is no
//!     `f64`→string formatting to drift. No benchmarking runs here; the
//!     strict-sync gate only reads the committed JSON.
//!   * **Report-only** — [`compare_against_baseline`] reads a LIVE criterion
//!     run (`target/criterion/**/new/estimates.json`) and the committed
//!     baseline, classifies each benchmark `ok | watch | regressed` against an
//!     advisory tolerance band, and renders a scoreboard. Valid regressions never
//!     fail the command; invalid required inputs do. The off-gate `suite-quality`
//!     lane records timing evidence without turning runner jitter into a gate.
//!
//! [`emit_baseline`] is the SINGLE producer of `bench/baseline.json` (seed and
//! every refresh go through it), subsuming the retired `scripts/bench_to_json.py`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use purrdf_lex::json::{
    self, Object, Value,
    record::{DecodeError, FromJson, Record, Within as _},
};

use crate::node::{Stage, StageInput, StageOutput, StageProduct};

/// Committed reference-run input (the single source of truth for the leaderboard).
pub const BASELINE_PATH: &str = "bench/baseline.json";
/// Committed, drift-gated leaderboard artifact.
pub const BENCH_LEADERBOARD_PATH: &str = "generated/bench/leaderboard.md";

/// Committed deterministic engine-cost/agreement baseline (the single source of
/// truth for the cost ledger). Produced by `gmeow-bench-engines --emit-cost`,
/// refreshed via `make maint-bench-cost-baseline`.
pub const COST_BASELINE_PATH: &str = "bench/cost-baseline.json";
/// Committed, drift-gated cost-ledger artifact.
pub const COST_LEDGER_PATH: &str = "generated/bench/cost-ledger.md";
/// Committed, drift-gated soak-window record (the longitudinal gap-zero claim).
pub const SOAK_RECORD_PATH: &str = "generated/bench/soak.md";

/// The soak window the committed record documents and the `make bench-soak` lane
/// enforces (`gmeow-bench-engines --soak N`) on the required CI `make heavy` lane — a
/// repeat-for-confidence loop does not belong on a per-commit local gate, but it still
/// runs on every pull request. A single-run tally is not soak
/// evidence; the window is the number of deterministic native-vs-published agreement
/// runs whose finding-graph fingerprint must stay byte-identical. Kept in lock-step
/// with the `bench-soak` Make target and the `--soak` default.
pub const SOAK_WINDOW: u64 = 3;

/// Advisory tolerance: a median slowdown over this (but under [`REGRESS_PCT`])
/// is flagged `watch`. Runner jitter on shared CI is expected — these bands are
/// evidence, never a gate.
const WATCH_PCT: f64 = 5.0;
/// Advisory tolerance: a median slowdown over this is flagged `regressed`.
const REGRESS_PCT: f64 = 15.0;

/// One benchmark's point estimates, in integer nanoseconds (`f64`→`u64` at the
/// emit boundary so the committed JSON and the render are formatting-stable).
#[derive(Clone, Copy)]
struct Estimate {
    mean_ns: u64,
    median_ns: u64,
}

impl FromJson for Estimate {
    /// Admit required unsigned integer tokens, preserving known duplicate refusals.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "timing baseline estimate object")?;
        let unsigned = |value: &Value| {
            value.as_u64().ok_or_else(|| {
                DecodeError::invalid_value(value, "an unsigned integer token within u64 range")
            })
        };
        Ok(Self {
            mean_ns: fields.required_with("mean_ns", unsigned)?,
            median_ns: fields.required_with("median_ns", unsigned)?,
        })
    }
}

/// Decode the required typed baseline without normalizing away duplicate record fields.
fn parse_timing_baseline(bytes: &[u8]) -> Result<BTreeMap<String, Estimate>, gmeow_errors::Diag> {
    json::record::from_slice(bytes).map_err(|error| {
        gmeow_errors::Diag::from(error).with_context(format!("parse required {BASELINE_PATH}"))
    })
}

// ── Criterion estimate collection (subsumes `bench_to_json.py::collect`) ────────

/// Walk `criterion_root/**/new/estimates.json` and map `"<group>/<bench>"` to its
/// point estimates in integer nanoseconds. Only files whose parent directory is
/// `new` count (criterion also writes a `base/` snapshot we ignore), and the
/// relative path must have at least `<group>/<bench>/new/estimates.json` depth —
/// exactly the constraint the old `**/new/estimates.json` glob encoded.
fn collect_estimates(
    criterion_root: &Path,
) -> Result<BTreeMap<String, Estimate>, gmeow_errors::Diag> {
    let mut out = BTreeMap::new();
    let metadata = std::fs::metadata(criterion_root).map_err(|error| {
        gmeow_errors::Diag::from(error).with_context(format!(
            "read required criterion directory {}",
            criterion_root.display(),
        ))
    })?;
    if !metadata.is_dir() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!(
                "criterion input is not a directory: {}",
                criterion_root.display()
            ),
        }));
    }
    let mut files: Vec<PathBuf> = Vec::new();
    walk_estimates(criterion_root, &mut files)?;
    for path in files {
        // Components between criterion_root and the trailing `new/estimates.json`.
        let rel = path
            .strip_prefix(criterion_root)
            .expect("walked path is under root");
        let components: Vec<_> = rel.components().collect();
        // Shallower paths are outside the selected measurement surface.
        if components.len() < 4 {
            continue;
        }
        let parts: Vec<&str> = components
            .iter()
            .map(|component| {
                component.as_os_str().to_str().ok_or_else(|| {
                    gmeow_errors::Diag::of_kind(crate::error::Parse {
                        message: format!(
                            "criterion benchmark path is not UTF-8: {}",
                            path.display()
                        ),
                    })
                })
            })
            .collect::<Result<_, _>>()?;
        let name = parts[..parts.len() - 2].join("/");
        let bytes = std::fs::read(&path).map_err(|error| {
            gmeow_errors::Diag::from(error).with_context(format!(
                "read criterion benchmark {name} at {}",
                path.display(),
            ))
        })?;
        out.insert(name.clone(), parse_live_estimate(&bytes, &path, &name)?);
    }
    Ok(out)
}

/// Recursively collect every `estimates.json` whose parent directory is `new`.
fn walk_estimates(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), gmeow_errors::Diag> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_estimates(&path, out)?;
        } else if path.file_name().and_then(|n| n.to_str()) == Some("estimates.json")
            && path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                == Some("new")
        {
            out.push(path);
        }
    }
    Ok(())
}

/// Read `data[key].point_estimate` as `f64` (criterion's estimate shape).
fn point_estimate(data: &Value, key: &str) -> Result<f64, gmeow_errors::Diag> {
    let number = data
        .get(key)
        .and_then(|v| v.get("point_estimate"))
        .and_then(Value::as_number)
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!("criterion estimate requires numeric {key}.point_estimate"),
            })
        })?;
    if number.decimal().is_negative() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("criterion {key}.point_estimate must be nonnegative"),
        }));
    }
    Ok(number.as_f64())
}

/// Round finite nonnegative nanoseconds, refusing overflow instead of saturating.
fn round_ns(ns: f64) -> Result<u64, gmeow_errors::Diag> {
    let rounded = ns.round();
    // u64::MAX rounds up to 2^64 as binary64; that endpoint cannot be cast safely.
    if !ns.is_finite() || ns < 0.0 || rounded >= 18_446_744_073_709_551_616.0 {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: "criterion point estimate must be finite, nonnegative and round within u64 nanoseconds".into(),
        }));
    }
    Ok(rounded as u64)
}

/// Decode one free-form Criterion document and locate each metric failure precisely.
fn parse_live_estimate(
    bytes: &[u8],
    path: &Path,
    name: &str,
) -> Result<Estimate, gmeow_errors::Diag> {
    let data = json::record::read_document(bytes).map_err(|error| {
        gmeow_errors::Diag::from(error).with_context(format!(
            "parse criterion benchmark {name} at {}",
            path.display(),
        ))
    })?;
    let metric = |key: &str| {
        point_estimate(&data, key)
            .and_then(round_ns)
            .map_err(|error| {
                error.with_context(format!(
                    "criterion benchmark {name} at {}: {key}.point_estimate",
                    path.display(),
                ))
            })
    };
    Ok(Estimate {
        mean_ns: metric("mean")?,
        median_ns: metric("median")?,
    })
}

// ── Baseline emit (THE single `bench/baseline.json` producer) ───────────────────

/// Flatten a live criterion run into the committed baseline JSON: sorted keys,
/// integer-nanosecond `mean_ns`/`median_ns`, trailing newline. This is the only
/// producer of `bench/baseline.json` (initial seed and every refresh), so there
/// is no second format to drift against.
pub fn emit_baseline(criterion_root: &Path) -> Result<String, gmeow_errors::Diag> {
    let estimates = collect_estimates(criterion_root)?;
    if estimates.is_empty() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!(
                "cannot emit a measured baseline: no criterion estimates under {}",
                criterion_root.display(),
            ),
        }));
    }
    Ok(encode_timing_baseline(&estimates))
}

/// Emit sorted benchmark names and mean/median members as pretty integer JSON plus one LF.
fn encode_timing_baseline(estimates: &BTreeMap<String, Estimate>) -> String {
    let mut top = Object::new();
    for (name, estimate) in estimates {
        top.insert(
            name,
            Object::new()
                .with("mean_ns", estimate.mean_ns)
                .with("median_ns", estimate.median_ns),
        );
    }
    let mut output = json::write_pretty(&top.into());
    output.push('\n');
    output
}

// ── Report-only regression scoreboard ───────────────────────────────────────────

/// Per-benchmark classification against the baseline.
enum Status {
    /// Within the watch band (or faster).
    Ok,
    /// Slower than [`WATCH_PCT`] but under [`REGRESS_PCT`].
    Watch,
    /// Slower than [`REGRESS_PCT`].
    Regressed,
    /// Percentage comparison is undefined because the baseline median is zero.
    Unavailable,
    /// In the baseline but absent from this run.
    Missing,
    /// In this run but not the baseline.
    New,
}

impl Status {
    fn label(&self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Watch => "watch",
            Status::Regressed => "regressed",
            Status::Unavailable => "unavailable",
            Status::Missing => "missing",
            Status::New => "new",
        }
    }
}

/// Classify a current vs baseline median (lower is better).
fn classify(delta_pct: f64) -> Status {
    if delta_pct > REGRESS_PCT {
        Status::Regressed
    } else if delta_pct > WATCH_PCT {
        Status::Watch
    } else {
        Status::Ok
    }
}

/// Render a valid live timing comparison; regressions remain advisory.
///
/// # Errors
/// Refuses missing/malformed baselines and missing/invalid live measurement inputs.
pub fn compare_against_baseline(
    criterion_root: &Path,
    baseline_json: &str,
) -> Result<String, gmeow_errors::Diag> {
    let baseline = parse_timing_baseline(baseline_json.as_bytes())?;
    let current = collect_estimates(criterion_root)?;
    Ok(render_timing_comparison(&baseline, &current))
}

/// Compare admitted inventories without treating missing measurements as zero.
fn render_timing_comparison(
    baseline: &BTreeMap<String, Estimate>,
    current: &BTreeMap<String, Estimate>,
) -> String {
    let mut names: BTreeSet<&str> = BTreeSet::new();
    names.extend(baseline.keys().map(String::as_str));
    names.extend(current.keys().map(String::as_str));

    let mut lines: Vec<String> = vec![
        "# gmeow perf regression scoreboard (report-only)".to_string(),
        String::new(),
        format!(
            "Live criterion run vs committed `{BASELINE_PATH}`. Median nanoseconds, \
lower is better. Advisory only (watch > {WATCH_PCT:.0}%, regressed > {REGRESS_PCT:.0}%); \
runner jitter is expected and never gates a PR."
        ),
        String::new(),
        "| benchmark | baseline (ns) | current (ns) | Δ% | status |".to_string(),
        "|---|---|---|---|---|".to_string(),
    ];

    let mut regressed = 0usize;
    let mut watch = 0usize;
    for name in &names {
        let base = baseline.get(*name);
        let cur = current.get(*name);
        let (base_cell, cur_cell, delta_cell, status) = match (base, cur) {
            (Some(b), Some(c)) if b.median_ns == 0 => (
                b.median_ns.to_string(),
                c.median_ns.to_string(),
                "—".to_string(),
                Status::Unavailable,
            ),
            (Some(b), Some(c)) => {
                let delta = (c.median_ns as f64 - b.median_ns as f64) / b.median_ns as f64 * 100.0;
                let status = classify(delta);
                match status {
                    Status::Regressed => regressed += 1,
                    Status::Watch => watch += 1,
                    _ => {}
                }
                (
                    b.median_ns.to_string(),
                    c.median_ns.to_string(),
                    format!("{delta:+.1}"),
                    status,
                )
            }
            (Some(b), None) => (
                b.median_ns.to_string(),
                "—".to_string(),
                "—".to_string(),
                Status::Missing,
            ),
            (None, Some(c)) => (
                "—".to_string(),
                c.median_ns.to_string(),
                "—".to_string(),
                Status::New,
            ),
            (None, None) => unreachable!("name came from one of the two maps"),
        };
        lines.push(format!(
            "| {name} | {base_cell} | {cur_cell} | {delta_cell} | {} |",
            status.label()
        ));
    }

    lines.push(String::new());
    lines.push(format!(
        "{} benchmark(s): {regressed} regressed, {watch} watch (report-only).",
        names.len()
    ));
    lines.join("\n") + "\n"
}

// ── Committed perf leaderboard (drift-gated export leaf) ─────────────────────────

/// Render the committed perf leaderboard from the committed baseline reference
/// run. PURELY deterministic — integer-ns values rendered in BTreeMap key order,
/// no `f64`→string formatting — so it survives the strict-sync byte gate
/// without ever running a benchmark. Hard-fails if `bench/baseline.json` is
/// missing or malformed (no degraded fallback).
pub(crate) fn render_bench_leaderboard(
    root: &Path,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let bytes = std::fs::read(root.join(BASELINE_PATH))?;
    let baseline = parse_timing_baseline(&bytes)?;

    let mut lines: Vec<String> = vec![
        "<!-- GENERATED by `gmeow-dev sync --mode update --outputs generated` (bench) — DO NOT EDIT. -->".to_string(),
        String::new(),
        "# gmeow perf leaderboard: native reasoning + validation hot paths".to_string(),
        String::new(),
        "Committed reference run (`bench/baseline.json`), refreshed via".to_string(),
        "`make maint-bench-baseline`. Median nanoseconds per benchmark, lower is".to_string(),
        "better. The off-gate `suite-quality` lane reports regressions against this".to_string(),
        "baseline (report-only — never gates a PR).".to_string(),
        String::new(),
        "| benchmark | mean (ns) | median (ns) |".to_string(),
        "|---|---|---|".to_string(),
    ];
    for (name, est) in &baseline {
        lines.push(format!("| {name} | {} | {} |", est.mean_ns, est.median_ns));
    }
    lines.push(String::new());
    lines.push(format!(
        "{} benchmark(s) in the committed reference run.",
        baseline.len()
    ));
    let md = lines.join("\n") + "\n";

    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    out.insert(BENCH_LEADERBOARD_PATH.to_string(), md.into_bytes());
    Ok(out)
}

/// The `stage-export-bench` export-leaf: the committed perf leaderboard.
pub struct BenchLeaderboardStage;

impl Stage for BenchLeaderboardStage {
    fn id(&self) -> &str {
        "stage-export-bench"
    }
    fn consumes(&self) -> &[String] {
        &[]
    }
    fn impl_version(&self) -> &str {
        "bench.v1"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
        // The committed reference run is the only input; a baseline refresh
        // busts the cache. No criterion run is read here — purely deterministic.
        Ok(vec![root.join(BASELINE_PATH)])
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            render_bench_leaderboard(input.root)?,
        )))
    }
}

// ── Committed deterministic cost ledger (drift-gated export leaf) ────────────────

/// The pinned engine revisions the cost baseline is attributable to.
struct EnginePins {
    native: String,
    backward_reference: String,
}

/// The native engine's deterministic cost record for one bench case. `answer_count`
/// is the backward fragment's count; `derived_count` every other fragment's — exactly
/// one is present, so [`CaseRecord::count`] hard-fails if neither is.
struct NativeCost {
    consumed_steps: u64,
    derived_count: Option<u64>,
    answer_count: Option<u64>,
    /// Total bytes allocated during the run — gated through the one-sided tolerance band
    /// (`fresh ≤ baseline·(1+ε)`) in the harness `--check-cost` lane; rendered here as a
    /// committed, drift-gated integer column (this projection reproduces the committed
    /// baseline byte-for-byte, so it is stable even though the live measure jitters).
    alloc_bytes: u64,
    /// Total allocation count during the run — gated through the same tolerance band.
    alloc_count: u64,
    peak_live_bytes: u64,
    /// Sorted `[rule, predicate, stratum, derivations]` tuples (may be empty at seams
    /// that expose no decomposable vector — never fabricated).
    cost_vector: Vec<(String, String, u32, u64)>,
    /// Incremental-only deterministic join-work count.
    joined_rows: Option<u64>,
    /// Incremental-only clean-rebuild comparator, embedded in the exact descriptor.
    scratch: Option<ScratchCost>,
    /// Forward-only cold/warm complete-evaluation evidence for the bounded physical
    /// plan cache.
    plan_cache: Option<PlanCacheCost>,
    /// Forward-only bounded-provenance Record/Skip evidence over the same physical
    /// plan and fact closure.
    provenance: Option<ProvenanceCost>,
    /// Incremental-grounding-only deterministic work vector and explicit solver
    /// boundary.
    grounding: Option<IncrementalGroundingCost>,
}

struct IncrementalGroundingCost {
    edb_changes: u64,
    ground_rule_changes: u64,
    universe_changes: u64,
    universe_joined_rows: u64,
    ground_rule_joined_rows: u64,
    ground_rule_probe_rows: u64,
    active_ground_rules: u64,
    solver: String,
    solver_status: String,
    solver_reran: bool,
}

struct PlanCacheCost {
    solver_version: String,
    rule_hash: String,
    cold: PlanEvaluationCost,
    warm: PlanEvaluationCost,
    same_executable: bool,
    repeat_parity: bool,
    warm_alloc_count_strictly_lower: bool,
    warm_peak_live_strictly_lower: bool,
}

struct PlanEvaluationCost {
    cache_hit: bool,
    plan_builds: u64,
    planning_units: u64,
    consumed_steps: u64,
    peak_live_bytes: u64,
    closure_blake3: String,
    cost_vector: Vec<(String, String, u32, u64)>,
}

struct ProvenanceCost {
    record: ProvenanceModeCost,
    skip: ProvenanceModeCost,
    closure_parity: bool,
    step_parity: bool,
    annotation_complete: bool,
    record_peak_overhead_bytes: i128,
    record_alloc_count_overhead: i128,
}

struct ProvenanceModeCost {
    annotation_count: u64,
    max_proof_height: Option<u32>,
    consumed_steps: u64,
    fact_count: u64,
    fact_closure_blake3: String,
    alloc_count: u64,
    peak_live_bytes: u64,
}

/// The clean native rebuild paired with an incremental transaction.
struct ScratchCost {
    consumed_steps: u64,
    derived_count: u64,
    peak_live_bytes: u64,
    cost_vector: Vec<(String, String, u32, u64)>,
    ground_rule_probe_rows: Option<u64>,
    active_ground_rules: Option<u64>,
}

/// The deterministic verdict-agreement booleans for one bench case.
struct Agreement {
    native_vs_golden: bool,
    incremental_insert_vs_scratch: Option<bool>,
    incremental_retract_vs_scratch: Option<bool>,
}

/// One `(corpus, case)` deterministic cost + agreement record.
struct CaseRecord {
    corpus: String,
    case: String,
    fragment: String,
    native: NativeCost,
    agreement: Agreement,
}

impl CaseRecord {
    /// Select exactly the count owned by this producer fragment.
    fn count(&self) -> Result<u64, gmeow_errors::Diag> {
        match (
            self.fragment.as_str(),
            self.native.derived_count,
            self.native.answer_count,
        ) {
            ("backward", None, Some(count)) => Ok(count),
            (
                "forward" | "existential" | "incremental" | "incremental-grounding",
                Some(count),
                None,
            ) => Ok(count),
            _ => Err(self
                .evidence_error("exactly one fragment-appropriate derived_count or answer_count")),
        }
    }

    /// Locate an absent selected measurement within its producer case.
    fn evidence_error(&self, missing: &str) -> gmeow_errors::Diag {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!(
                "cost baseline case {}/{} ({}) requires {missing}",
                self.corpus, self.case, self.fragment
            ),
        })
    }

    /// Require the paired rebuild, join measurement and both actual parity observations.
    fn incremental_evidence(&self) -> Result<(u64, bool, bool), gmeow_errors::Diag> {
        self.native
            .scratch
            .as_ref()
            .ok_or_else(|| self.evidence_error("the scratch comparator"))?;
        Ok((
            self.native
                .joined_rows
                .ok_or_else(|| self.evidence_error("joined_rows"))?,
            self.agreement
                .incremental_insert_vs_scratch
                .ok_or_else(|| self.evidence_error("incremental_insert_vs_scratch"))?,
            self.agreement
                .incremental_retract_vs_scratch
                .ok_or_else(|| self.evidence_error("incremental_retract_vs_scratch"))?,
        ))
    }
}

/// One corpus's divergence-ledger tally (the content-addressed agreement fold).
struct LedgerTally {
    cases: u64,
    agree: u64,
    corpus_only: u64,
    dl_gap: u64,
    finding_count: u64,
    finding_graph_blake3: String,
}

/// Scheduler-independent evidence from the permanent four-worker rule fixture.
struct RuleParallelCost {
    fixture: String,
    worker_count: u64,
    rule_count: u64,
    seed_rows: u64,
    derived_rows: u64,
    consumed_steps: u64,
    parallel_rounds: u64,
    rule_tasks: u64,
    serial_candidate_rows: u64,
    critical_path_candidate_rows: u64,
    critical_path_rows_saved: u64,
    max_buffered_candidate_rows: u64,
    max_task_candidate_rows: u64,
    budget_cases: u64,
    output_parity: bool,
    budget_parity: bool,
    parallel_path_entered: bool,
    critical_path_strictly_lower: bool,
    closure_blake3: String,
}

impl RuleParallelCost {
    /// Refuse to render a committed parallelism claim unless the evidence is
    /// non-vacuous, internally coherent, and records the promised four-worker path.
    fn validate(&self) -> Result<(), gmeow_errors::Diag> {
        let rows_saved = self
            .serial_candidate_rows
            .checked_sub(self.critical_path_candidate_rows);
        let digest_is_blake3 = self.closure_blake3.len() == 64
            && self
                .closure_blake3
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit());
        if self.fixture.is_empty()
            || self.worker_count != 4
            || self.rule_count < 2
            || self.seed_rows == 0
            || self.derived_rows == 0
            || self.consumed_steps == 0
            || self.parallel_rounds == 0
            || self.rule_tasks < self.parallel_rounds
            || rows_saved != Some(self.critical_path_rows_saved)
            || self.critical_path_rows_saved == 0
            || self.max_buffered_candidate_rows < self.max_task_candidate_rows
            || self.budget_cases == 0
            || !self.output_parity
            || !self.budget_parity
            || !self.parallel_path_entered
            || !self.critical_path_strictly_lower
            || !digest_is_blake3
        {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!(
                    "cost baseline carries incomplete or inconsistent four-worker rule-parallel evidence for fixture `{}`",
                    self.fixture
                ),
            }));
        }
        Ok(())
    }
}

/// The committed deterministic cost/agreement artifact (`bench/cost-baseline.json`).
struct CostArtifact {
    schema: String,
    case_count: u64,
    engine_pins: EnginePins,
    cases: Vec<CaseRecord>,
    rule_parallelism: RuleParallelCost,
    ledgers: BTreeMap<String, LedgerTally>,
}

/// Admit an exact unsigned integer token; signed zero is not unsigned evidence.
fn cost_unsigned(value: &Value) -> Result<u64, DecodeError> {
    value
        .as_u64()
        .ok_or_else(|| DecodeError::invalid_value(value, "an unsigned u64 integer token"))
}

/// Admit a bounded unsigned stratum/height token without truncation.
fn cost_stratum(value: &Value) -> Result<u32, DecodeError> {
    cost_unsigned(value).and_then(|number| {
        u32::try_from(number)
            .map_err(|_| DecodeError::invalid_value(value, "an unsigned u32 integer token"))
    })
}

/// Admit signed integer overheads exactly, retaining the signed owner's zero semantics.
fn cost_signed(value: &Value) -> Result<i128, DecodeError> {
    value
        .as_number()
        .and_then(|number| number.as_i128())
        .ok_or_else(|| DecodeError::invalid_value(value, "a signed i128 integer token"))
}

/// Decode ordered cost tuples with exact arity and native nested index pointers.
fn cost_vector(value: &Value) -> Result<Vec<(String, String, u32, u64)>, DecodeError> {
    json::record::items_with(value, |row| {
        let fields = row
            .as_array()
            .filter(|fields| fields.len() == 4)
            .ok_or_else(|| DecodeError::invalid_value(row, "a four-element cost tuple"))?;
        Ok((
            String::from_json(&fields[0]).map_err(|error| error.within("0"))?,
            String::from_json(&fields[1]).map_err(|error| error.within("1"))?,
            cost_stratum(&fields[2]).map_err(|error| error.within("2"))?,
            cost_unsigned(&fields[3]).map_err(|error| error.within("3"))?,
        ))
    })
}

impl FromJson for EnginePins {
    /// Read the native and backward-reference version identities.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "EnginePins object")?;
        Ok(Self {
            native: fields.required("native")?,
            backward_reference: fields.required("backward_reference")?,
        })
    }
}

impl FromJson for NativeCost {
    /// Require common measurements while retaining fragment-specific optional panels.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "NativeCost object")?;
        Ok(Self {
            consumed_steps: fields.required_with("consumed_steps", cost_unsigned)?,
            derived_count: fields.optional_with("derived_count", cost_unsigned)?,
            answer_count: fields.optional_with("answer_count", cost_unsigned)?,
            alloc_bytes: fields.required_with("alloc_bytes", cost_unsigned)?,
            alloc_count: fields.required_with("alloc_count", cost_unsigned)?,
            peak_live_bytes: fields.required_with("peak_live_bytes", cost_unsigned)?,
            cost_vector: fields.required_with("cost_vector", cost_vector)?,
            joined_rows: fields.optional_with("joined_rows", cost_unsigned)?,
            scratch: fields.optional("scratch")?,
            plan_cache: fields.optional("plan_cache")?,
            provenance: fields.optional("provenance")?,
            grounding: fields.optional("grounding")?,
        })
    }
}

impl FromJson for IncrementalGroundingCost {
    /// Read the incremental grounding work counts and explicit solver outcome.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "IncrementalGroundingCost object")?;
        Ok(Self {
            edb_changes: fields.required_with("edb_changes", cost_unsigned)?,
            ground_rule_changes: fields.required_with("ground_rule_changes", cost_unsigned)?,
            universe_changes: fields.required_with("universe_changes", cost_unsigned)?,
            universe_joined_rows: fields.required_with("universe_joined_rows", cost_unsigned)?,
            ground_rule_joined_rows: fields
                .required_with("ground_rule_joined_rows", cost_unsigned)?,
            ground_rule_probe_rows: fields
                .required_with("ground_rule_probe_rows", cost_unsigned)?,
            active_ground_rules: fields.required_with("active_ground_rules", cost_unsigned)?,
            solver: fields.required("solver")?,
            solver_status: fields.required("solver_status")?,
            solver_reran: fields.required("solver_reran")?,
        })
    }
}

impl FromJson for PlanCacheCost {
    /// Read paired cold/warm evaluations and their declared reuse comparisons.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "PlanCacheCost object")?;
        Ok(Self {
            solver_version: fields.required("solver_version")?,
            rule_hash: fields.required("rule_hash")?,
            cold: fields.required("cold")?,
            warm: fields.required("warm")?,
            same_executable: fields.required("same_executable")?,
            repeat_parity: fields.required("repeat_parity")?,
            warm_alloc_count_strictly_lower: fields.required("warm_alloc_count_strictly_lower")?,
            warm_peak_live_strictly_lower: fields.required("warm_peak_live_strictly_lower")?,
        })
    }
}

impl FromJson for PlanEvaluationCost {
    /// Read one complete evaluation's costs, closure identity and ordered vector.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "PlanEvaluationCost object")?;
        Ok(Self {
            cache_hit: fields.required("cache_hit")?,
            plan_builds: fields.required_with("plan_builds", cost_unsigned)?,
            planning_units: fields.required_with("planning_units", cost_unsigned)?,
            consumed_steps: fields.required_with("consumed_steps", cost_unsigned)?,
            peak_live_bytes: fields.required_with("peak_live_bytes", cost_unsigned)?,
            closure_blake3: fields.required("closure_blake3")?,
            cost_vector: fields.required_with("cost_vector", cost_vector)?,
        })
    }
}

impl FromJson for ProvenanceCost {
    /// Read Record/Skip observations, parity and exact signed overheads.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "ProvenanceCost object")?;
        Ok(Self {
            record: fields.required("record")?,
            skip: fields.required("skip")?,
            closure_parity: fields.required("closure_parity")?,
            step_parity: fields.required("step_parity")?,
            annotation_complete: fields.required("annotation_complete")?,
            record_peak_overhead_bytes: fields
                .required_with("record_peak_overhead_bytes", cost_signed)?,
            record_alloc_count_overhead: fields
                .required_with("record_alloc_count_overhead", cost_signed)?,
        })
    }
}

impl FromJson for ProvenanceModeCost {
    /// Read a provenance mode's costs and nullable proof-height observation.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "ProvenanceModeCost object")?;
        Ok(Self {
            annotation_count: fields.required_with("annotation_count", cost_unsigned)?,
            max_proof_height: fields.optional_with("max_proof_height", cost_stratum)?,
            consumed_steps: fields.required_with("consumed_steps", cost_unsigned)?,
            fact_count: fields.required_with("fact_count", cost_unsigned)?,
            fact_closure_blake3: fields.required("fact_closure_blake3")?,
            alloc_count: fields.required_with("alloc_count", cost_unsigned)?,
            peak_live_bytes: fields.required_with("peak_live_bytes", cost_unsigned)?,
        })
    }
}

impl FromJson for ScratchCost {
    /// Read the paired clean rebuild and its optional grounding measurements.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "ScratchCost object")?;
        Ok(Self {
            consumed_steps: fields.required_with("consumed_steps", cost_unsigned)?,
            derived_count: fields.required_with("derived_count", cost_unsigned)?,
            peak_live_bytes: fields.required_with("peak_live_bytes", cost_unsigned)?,
            cost_vector: fields.required_with("cost_vector", cost_vector)?,
            ground_rule_probe_rows: fields
                .optional_with("ground_rule_probe_rows", cost_unsigned)?,
            active_ground_rules: fields.optional_with("active_ground_rules", cost_unsigned)?,
        })
    }
}

impl FromJson for Agreement {
    /// Read native/golden agreement and optional incremental parity observations.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "Agreement object")?;
        Ok(Self {
            native_vs_golden: fields.required("native_vs_golden")?,
            incremental_insert_vs_scratch: fields.optional("incremental_insert_vs_scratch")?,
            incremental_retract_vs_scratch: fields.optional("incremental_retract_vs_scratch")?,
        })
    }
}

impl FromJson for CaseRecord {
    /// Read one case identity, fragment, measurements and agreement record.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "CaseRecord object")?;
        Ok(Self {
            corpus: fields.required("corpus")?,
            case: fields.required("case")?,
            fragment: fields.required("fragment")?,
            native: fields.required("native")?,
            agreement: fields.required("agreement")?,
        })
    }
}

impl FromJson for LedgerTally {
    /// Read comparison tallies and their stored finding-graph identity.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "LedgerTally object")?;
        Ok(Self {
            cases: fields.required_with("cases", cost_unsigned)?,
            agree: fields.required_with("agree", cost_unsigned)?,
            corpus_only: fields.required_with("corpus_only", cost_unsigned)?,
            dl_gap: fields.required_with("dl_gap", cost_unsigned)?,
            finding_count: fields.required_with("finding_count", cost_unsigned)?,
            finding_graph_blake3: fields.required("finding_graph_blake3")?,
        })
    }
}

impl FromJson for RuleParallelCost {
    /// Read all required four-worker structural evidence without invented defaults.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "RuleParallelCost object")?;
        Ok(Self {
            fixture: fields.required("fixture")?,
            worker_count: fields.required_with("worker_count", cost_unsigned)?,
            rule_count: fields.required_with("rule_count", cost_unsigned)?,
            seed_rows: fields.required_with("seed_rows", cost_unsigned)?,
            derived_rows: fields.required_with("derived_rows", cost_unsigned)?,
            consumed_steps: fields.required_with("consumed_steps", cost_unsigned)?,
            parallel_rounds: fields.required_with("parallel_rounds", cost_unsigned)?,
            rule_tasks: fields.required_with("rule_tasks", cost_unsigned)?,
            serial_candidate_rows: fields.required_with("serial_candidate_rows", cost_unsigned)?,
            critical_path_candidate_rows: fields
                .required_with("critical_path_candidate_rows", cost_unsigned)?,
            critical_path_rows_saved: fields
                .required_with("critical_path_rows_saved", cost_unsigned)?,
            max_buffered_candidate_rows: fields
                .required_with("max_buffered_candidate_rows", cost_unsigned)?,
            max_task_candidate_rows: fields
                .required_with("max_task_candidate_rows", cost_unsigned)?,
            budget_cases: fields.required_with("budget_cases", cost_unsigned)?,
            output_parity: fields.required("output_parity")?,
            budget_parity: fields.required("budget_parity")?,
            parallel_path_entered: fields.required("parallel_path_entered")?,
            critical_path_strictly_lower: fields.required("critical_path_strictly_lower")?,
            closure_blake3: fields.required("closure_blake3")?,
        })
    }
}

impl FromJson for CostArtifact {
    /// Read the versioned envelope and every required case, ledger and evidence owner.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "CostArtifact object")?;
        Ok(Self {
            schema: fields.required("schema")?,
            case_count: fields.required_with("case_count", cost_unsigned)?,
            engine_pins: fields.required("engine_pins")?,
            cases: fields.required("cases")?,
            rule_parallelism: fields.required("rule_parallelism")?,
            ledgers: fields.required("ledgers")?,
        })
    }
}

/// Admit the complete cost artifact once for both report projections.
fn parse_cost_artifact(bytes: &[u8]) -> Result<CostArtifact, gmeow_errors::Diag> {
    let artifact: CostArtifact = json::record::from_slice(bytes).map_err(|error| {
        gmeow_errors::Diag::from(error).with_context(format!("parse required {COST_BASELINE_PATH}"))
    })?;
    artifact.validate()?;
    Ok(artifact)
}

impl CostArtifact {
    /// Refuse unsupported, vacuous or ambiguous artifact identities before reporting.
    fn validate(&self) -> Result<(), gmeow_errors::Diag> {
        let invalid = |message: &str| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!("cost baseline {message}"),
            })
        };
        if self.schema != "gmeow.bench-engines.cost/1" {
            return Err(invalid("requires schema gmeow.bench-engines.cost/1"));
        }
        if self.cases.is_empty() || self.ledgers.is_empty() {
            return Err(invalid("requires nonempty cases and ledgers"));
        }
        if u64::try_from(self.cases.len()).ok() != Some(self.case_count) {
            return Err(invalid("case_count does not match cases"));
        }
        if self.engine_pins.native.is_empty() || self.engine_pins.backward_reference.is_empty() {
            return Err(invalid(
                "requires nonempty native and backward-reference engine pins",
            ));
        }
        let mut identities = BTreeSet::new();
        let mut corpora = BTreeSet::new();
        for case in &self.cases {
            if case.corpus.is_empty() || case.case.is_empty() {
                return Err(invalid("requires nonempty corpus and case identities"));
            }
            if !identities.insert((&case.corpus, &case.case)) {
                return Err(invalid(&format!(
                    "has duplicate case {}/{}",
                    case.corpus, case.case
                )));
            }
            corpora.insert(case.corpus.as_str());
            case.count()?;
            let incremental = matches!(
                case.fragment.as_str(),
                "incremental" | "incremental-grounding"
            );
            if incremental || case.native.scratch.is_some() {
                case.incremental_evidence()?;
            }
            if case.fragment == "incremental-grounding" || case.native.grounding.is_some() {
                case.native
                    .grounding
                    .as_ref()
                    .ok_or_else(|| case.evidence_error("the grounding record"))?;
                let scratch = case
                    .native
                    .scratch
                    .as_ref()
                    .ok_or_else(|| case.evidence_error("the scratch comparator"))?;
                scratch
                    .ground_rule_probe_rows
                    .ok_or_else(|| case.evidence_error("scratch ground_rule_probe_rows"))?;
                scratch
                    .active_ground_rules
                    .ok_or_else(|| case.evidence_error("scratch active_ground_rules"))?;
            }
        }
        for (corpus, tally) in &self.ledgers {
            if corpus.is_empty() || tally.finding_graph_blake3.is_empty() || tally.cases == 0 {
                return Err(invalid(&format!(
                    "ledger {corpus:?} requires a nonempty corpus identity, finding-graph identity and nonzero comparison count"
                )));
            }
        }
        let ledger_corpora: BTreeSet<_> = self.ledgers.keys().map(String::as_str).collect();
        if corpora != ledger_corpora {
            return Err(invalid(&format!(
                "ledger corpus identities must match case corpora: expected {corpora:?}, found {ledger_corpora:?}"
            )));
        }
        self.rule_parallelism.validate()
    }
}

/// Render the committed deterministic engine-cost ledger from the committed cost
/// baseline. PURELY deterministic — integer counts, the three allocation scalars, and
/// boolean verdicts rendered in sorted `(corpus, case)` order, no wall-clock / peak-RSS
/// (those are report-only in the harness) — so it survives the strict-sync byte
/// gate without ever running a benchmark. The `alloc_bytes` / `alloc_count` columns are a
/// projection of the COMMITTED baseline (byte-stable here even though the live measure
/// jitters; the jitter is absorbed by the harness tolerance-band gate). Hard-fails if
/// `bench/cost-baseline.json` is missing or malformed (no degraded fallback).
pub(crate) fn render_cost_ledger(
    root: &Path,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let path = root.join(COST_BASELINE_PATH);
    let bytes = std::fs::read(&path).map_err(|error| {
        gmeow_errors::Diag::from(error).with_context(format!("read required {}", path.display()))
    })?;
    let artifact = parse_cost_artifact(&bytes)?;

    // Sort by (corpus, case) so the render is independent of the artifact's array order.
    let mut cases: Vec<&CaseRecord> = artifact.cases.iter().collect();
    cases.sort_by(|a, b| (&a.corpus, &a.case).cmp(&(&b.corpus, &b.case)));

    let mut lines: Vec<String> = vec![
        "<!-- GENERATED by `gmeow-dev sync --mode update --outputs generated` (cost-ledger) — DO NOT EDIT. -->".to_string(),
        String::new(),
        "# gmeow deterministic engine-cost ledger".to_string(),
        String::new(),
        "Committed engine/reference cost/agreement baseline (`bench/cost-baseline.json`),"
            .to_string(),
        "refreshed via `make maint-bench-cost-baseline`. Every measured performance value is".to_string(),
        "an integer count or boolean verdict — NO wall-clock, NO peak-RSS (those are report-only in the".to_string(),
        "harness). The three allocation scalars GATE: `peak_live_bytes` by exact drift-match,".to_string(),
        "and `alloc_bytes`/`alloc_count` through one-sided tolerance bands: bytes use 1%,".to_string(),
        "while counts use the greater of 1% and the measured 42-allocation quantized floor. This".to_string(),
        "is a drift-gated projection of the deterministic cost artifact; strict `sync`".to_string(),
        "reproduces it byte-for-byte from the committed baseline without running a benchmark.".to_string(),
        String::new(),
        format!(
            "Engine/reference pins: native `{}`, backward `{}`.",
            artifact.engine_pins.native,
            artifact.engine_pins.backward_reference
        ),
        String::new(),
        "## Per-case deterministic cost + verdict-agreement".to_string(),
        String::new(),
        "| corpus | case | fragment | consumed_steps | derived | alloc_bytes | alloc_count | peak_live_bytes | native_vs_golden |"
            .to_string(),
        "|---|---|---|---|---|---|---|---|---|".to_string(),
    ];
    for case in &cases {
        lines.push(format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            case.corpus,
            case.case,
            case.fragment,
            case.native.consumed_steps,
            case.count()?,
            case.native.alloc_bytes,
            case.native.alloc_count,
            case.native.peak_live_bytes,
            case.agreement.native_vs_golden,
        ));
    }

    // Decomposable cost vectors, in the same sorted (corpus, case) case order and the
    // artifact's already-sorted CostKey tuple order.
    lines.push(String::new());
    lines.push("## Decomposable cost vectors (rule × predicate × stratum)".to_string());
    lines.push(String::new());
    lines.push("| corpus | case | rule | predicate | stratum | derivations |".to_string());
    lines.push("|---|---|---|---|---|---|".to_string());
    let mut vector_rows = 0usize;
    for case in &cases {
        for (rule, predicate, stratum, derivations) in &case.native.cost_vector {
            lines.push(format!(
                "| {} | {} | {} | {} | {} | {} |",
                case.corpus, case.case, rule, predicate, stratum, derivations
            ));
            vector_rows += 1;
        }
        if let Some(scratch) = &case.native.scratch {
            for (rule, predicate, stratum, derivations) in &scratch.cost_vector {
                lines.push(format!(
                    "| {} | {} (scratch rebuild) | {} | {} | {} | {} |",
                    case.corpus, case.case, rule, predicate, stratum, derivations
                ));
                vector_rows += 1;
            }
        }
    }
    if vector_rows == 0 {
        lines.push("| — | — | — | — | — | — |".to_string());
    }

    // The incremental lane's paired native rebuild is retained inside the exact
    // descriptor. Surface the raw deterministic delta explicitly: no wall-clock and
    // no inferred percentage, just the committed counts/high-water marks.
    let incremental_cases: Vec<&&CaseRecord> = cases
        .iter()
        .filter(|case| case.native.scratch.is_some())
        .collect();
    if !incremental_cases.is_empty() {
        lines.push(String::new());
        lines.push("## Incremental transaction vs clean native rebuild".to_string());
        lines.push(String::new());
        lines.push(
            "| corpus | case | incremental steps | scratch steps | steps saved | derived rows | incremental peak_live_bytes | scratch peak_live_bytes | peak bytes saved | joined delta rows | insert parity | retract parity |"
                .to_string(),
        );
        lines.push("|---|---|---|---|---|---|---|---|---|---|---|---|".to_string());
        for case in incremental_cases {
            let (joined_rows, insert_parity, retract_parity) = case.incremental_evidence()?;
            let scratch = case
                .native
                .scratch
                .as_ref()
                .expect("filtered to incremental scratch cases");
            lines.push(format!(
                "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                case.corpus,
                case.case,
                case.native.consumed_steps,
                scratch.consumed_steps,
                i128::from(scratch.consumed_steps) - i128::from(case.native.consumed_steps),
                scratch.derived_count,
                case.native.peak_live_bytes,
                scratch.peak_live_bytes,
                i128::from(scratch.peak_live_bytes) - i128::from(case.native.peak_live_bytes),
                joined_rows,
                insert_parity,
                retract_parity,
            ));
        }
    }

    let grounding_cases: Vec<&&CaseRecord> = cases
        .iter()
        .filter(|case| case.native.grounding.is_some())
        .collect();
    if !grounding_cases.is_empty() {
        lines.push(String::new());
        lines.push("## Incremental non-monotone grounding".to_string());
        lines.push(String::new());
        lines.push(
            "Ground-rule commits and candidate probes are deterministic. The maintained ground program is incremental; the named WFS/stable-model solver remains explicitly from scratch whenever its complete slice changes."
                .to_string(),
        );
        lines.push(String::new());
        lines.push(
            "| corpus | case | incremental ground commits | scratch ground commits | commits saved | incremental ground probes | scratch ground probes | probes saved | active ground rules | EDB changes | universe changes | universe delta rows | ground-rule delta rows | solver | solver status | solver reran | insert parity | retract parity |"
                .to_string(),
        );
        lines.push(
            "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|".to_string(),
        );
        for case in grounding_cases {
            let (_, insert_parity, retract_parity) = case.incremental_evidence()?;
            let evidence_error = |missing: &str| {
                gmeow_errors::Diag::of_kind(crate::error::Parse {
                    message: format!(
                        "cost baseline case `{}/{}` carries incremental-grounding evidence but is missing {missing}",
                        case.corpus, case.case
                    ),
                })
            };
            let grounding = case
                .native
                .grounding
                .as_ref()
                .ok_or_else(|| evidence_error("the grounding record"))?;
            let scratch = case
                .native
                .scratch
                .as_ref()
                .ok_or_else(|| evidence_error("the scratch comparator"))?;
            let scratch_probes = scratch
                .ground_rule_probe_rows
                .ok_or_else(|| evidence_error("scratch ground_rule_probe_rows"))?;
            let scratch_rules = scratch
                .active_ground_rules
                .ok_or_else(|| evidence_error("scratch active_ground_rules"))?;
            lines.push(format!(
                "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                case.corpus,
                case.case,
                grounding.ground_rule_changes,
                scratch_rules,
                i128::from(scratch_rules) - i128::from(grounding.ground_rule_changes),
                grounding.ground_rule_probe_rows,
                scratch_probes,
                i128::from(scratch_probes) - i128::from(grounding.ground_rule_probe_rows),
                grounding.active_ground_rules,
                grounding.edb_changes,
                grounding.universe_changes,
                grounding.universe_joined_rows,
                grounding.ground_rule_joined_rows,
                grounding.solver,
                grounding.solver_status,
                grounding.solver_reran,
                insert_parity,
                retract_parity,
            ));
        }
    }

    // Complete cold/warm evaluations over identical EDB+rules. The second run must
    // consume the same immutable plan, do zero planning work, preserve the closure and
    // decomposable cost vector, and strictly reduce allocation count + peak live bytes.
    let planned_cases: Vec<&&CaseRecord> = cases
        .iter()
        .filter(|case| case.native.plan_cache.is_some())
        .collect();
    if !planned_cases.is_empty() {
        lines.push(String::new());
        lines.push("## Cold vs warm physical-plan reuse".to_string());
        lines.push(String::new());
        lines.push(
            "Each row is two complete materializations over identical inputs; parsing/EDB loading/certification are outside both measured regions."
                .to_string(),
        );
        lines.push(String::new());
        lines.push(
            "| corpus | case | solver | rule hash | cold hit | warm hit | cold builds | warm builds | cold planning units | warm planning units | cold steps | warm steps | cold peak_live_bytes | warm peak_live_bytes | peak bytes saved | same plan | closure+cost parity | warm alloc count lower | warm peak lower |"
                .to_string(),
        );
        lines.push(
            "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"
                .to_string(),
        );
        for case in planned_cases {
            let plan = case
                .native
                .plan_cache
                .as_ref()
                .expect("filtered to plan-cache cases");
            let closure_and_cost_parity = plan.repeat_parity
                && plan.cold.closure_blake3 == plan.warm.closure_blake3
                && plan.cold.cost_vector == plan.warm.cost_vector;
            lines.push(format!(
                "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                case.corpus,
                case.case,
                plan.solver_version,
                plan.rule_hash,
                plan.cold.cache_hit,
                plan.warm.cache_hit,
                plan.cold.plan_builds,
                plan.warm.plan_builds,
                plan.cold.planning_units,
                plan.warm.planning_units,
                plan.cold.consumed_steps,
                plan.warm.consumed_steps,
                plan.cold.peak_live_bytes,
                plan.warm.peak_live_bytes,
                i128::from(plan.cold.peak_live_bytes) - i128::from(plan.warm.peak_live_bytes),
                plan.same_executable,
                closure_and_cost_parity,
                plan.warm_alloc_count_strictly_lower,
                plan.warm_peak_live_strictly_lower,
            ));
        }
    }

    // Bounded provenance overhead: Record and Skip execute the same complete plan.
    // The fact digest and steps are exact laws; peak-live is the deterministic
    // overhead signal. Total allocation counts are retained as advisory corroboration
    // and excluded from the harness's exact descriptor.
    let provenance_cases: Vec<&&CaseRecord> = cases
        .iter()
        .filter(|case| case.native.provenance.is_some())
        .collect();
    if !provenance_cases.is_empty() {
        lines.push(String::new());
        lines.push("## Record vs Skip bounded-provenance overhead".to_string());
        lines.push(String::new());
        lines.push(
            "Each row executes the same warm physical plan over identical EDB/rules. Fact-closure and committed-step parity are hard laws; peak-live is exact, while allocation-count deltas are advisory."
                .to_string(),
        );
        lines.push(String::new());
        lines.push(
            "| corpus | case | facts | annotations | max proof height | Record steps | Skip steps | Record peak_live_bytes | Skip peak_live_bytes | Record peak overhead | Record alloc_count | Skip alloc_count | alloc-count overhead (advisory) | fact-closure parity | step parity | annotation complete |"
                .to_string(),
        );
        lines.push("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|".to_string());
        for case in provenance_cases {
            let provenance = case
                .native
                .provenance
                .as_ref()
                .expect("filtered to provenance cases");
            let closure_parity = provenance.closure_parity
                && provenance.record.fact_closure_blake3 == provenance.skip.fact_closure_blake3;
            lines.push(format!(
                "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                case.corpus,
                case.case,
                provenance.record.fact_count,
                provenance.record.annotation_count,
                provenance
                    .record
                    .max_proof_height
                    .map_or_else(|| "—".to_string(), |height| height.to_string()),
                provenance.record.consumed_steps,
                provenance.skip.consumed_steps,
                provenance.record.peak_live_bytes,
                provenance.skip.peak_live_bytes,
                provenance.record_peak_overhead_bytes,
                provenance.record.alloc_count,
                provenance.skip.alloc_count,
                provenance.record_alloc_count_overhead,
                closure_parity,
                provenance.step_parity,
                provenance.annotation_complete
                    && provenance.skip.annotation_count == 0
                    && provenance.record.annotation_count == provenance.skip.fact_count,
            ));
        }
    }

    let parallel = &artifact.rule_parallelism;
    lines.push(String::new());
    lines.push("## Four-worker rule-parallel structural evidence".to_string());
    lines.push(String::new());
    lines.push(
        "The permanent balanced fixture runs in a real four-worker Rayon pool and is compared with forced-sequential execution. Candidate rows are counted after rule-local deduplication and before the deterministic merge; the critical-path count sums the largest task in each sequential round. These are exact structural row counts, not wall-clock speedup or byte-level memory claims."
            .to_string(),
    );
    lines.push(String::new());
    lines.push(
        "| fixture | workers | rules | seed rows | derived rows | consumed steps | parallel rounds | rule tasks | budget cuts |"
            .to_string(),
    );
    lines.push("|---|---|---|---|---|---|---|---|---|".to_string());
    lines.push(format!(
        "| {} | {} | {} | {} | {} | {} | {} | {} | {} |",
        parallel.fixture,
        parallel.worker_count,
        parallel.rule_count,
        parallel.seed_rows,
        parallel.derived_rows,
        parallel.consumed_steps,
        parallel.parallel_rounds,
        parallel.rule_tasks,
        parallel.budget_cases,
    ));
    lines.push(String::new());
    lines.push(
        "| serial candidate-row sum | critical-path candidate-row sum | structural row gap | max merge-barrier rows | max task rows | output + provenance parity | budget parity | parallel path entered | strict critical-path reduction | closure blake3 |"
            .to_string(),
    );
    lines.push("|---|---|---|---|---|---|---|---|---|---|".to_string());
    lines.push(format!(
        "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
        parallel.serial_candidate_rows,
        parallel.critical_path_candidate_rows,
        parallel.critical_path_rows_saved,
        parallel.max_buffered_candidate_rows,
        parallel.max_task_candidate_rows,
        parallel.output_parity,
        parallel.budget_parity,
        parallel.parallel_path_entered,
        parallel.critical_path_strictly_lower,
        parallel.closure_blake3,
    ));

    // Per-corpus divergence-ledger tally (the content-addressed agreement fold): the
    // finding graph blake3 is a pure function of the comparisons, so it is a stable
    // drift signal.
    lines.push(String::new());
    lines.push("## Per-corpus divergence-ledger tally".to_string());
    lines.push(String::new());
    lines.push(
        "| corpus | cases | agree | corpus_only | dl_gap | findings | finding_graph_blake3 |"
            .to_string(),
    );
    lines.push("|---|---|---|---|---|---|---|".to_string());
    for (corpus, tally) in &artifact.ledgers {
        lines.push(format!(
            "| {} | {} | {} | {} | {} | {} | {} |",
            corpus,
            tally.cases,
            tally.agree,
            tally.corpus_only,
            tally.dl_gap,
            tally.finding_count,
            tally.finding_graph_blake3,
        ));
    }

    lines.push(String::new());
    lines.push(format!(
        "{} case(s) across {} corpora in the committed cost baseline.",
        cases.len(),
        artifact.ledgers.len()
    ));
    let md = lines.join("\n") + "\n";

    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    out.insert(COST_LEDGER_PATH.to_string(), md.into_bytes());
    Ok(out)
}

/// Render the committed soak-window record from the committed cost baseline. This is
/// the LONGITUDINAL twin of [`render_cost_ledger`]: where the cost ledger reports one
/// run's per-corpus tally, the soak record projects the ledger's INVARIANT finding-graph
/// digest and asserts gap-zero (`corpus_only == 0 && dl_gap == 0`) per corpus — the
/// checkable claim behind "ledger gap-zero over a soak window". Because the finding-graph
/// blake3 is a pure function of the committed comparisons, this record is byte-stable and
/// strict `sync` reproduces it without running a benchmark; the live N-run
/// reproducibility+gap-zero assertion is enforced on-gate by
/// `gmeow-bench-engines --soak <N>` (the `make bench-soak` lane).
///
/// Hard-fails if the baseline is missing/malformed, OR if ANY corpus is NOT gap-zero:
/// committing a soak record over a corpus with a live divergence would be a false
/// gap-zero claim, so a non-zero `corpus_only`/`dl_gap` is a hard error here, never a
/// silently-rendered "held" (no-optionality).
pub(crate) fn render_soak(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let path = root.join(COST_BASELINE_PATH);
    let bytes = std::fs::read(&path).map_err(|error| {
        gmeow_errors::Diag::from(error).with_context(format!("read required {}", path.display()))
    })?;
    let artifact = parse_cost_artifact(&bytes)?;

    // Assert gap-zero per corpus BEFORE rendering, and fold the invariant per-corpus
    // finding-graph digests into a single combined soak digest (blake3 over the sorted
    // `corpus\x1f<per-corpus finding_graph_blake3>` lines — a pure function of the
    // committed ledgers, so the record is byte-stable). `artifact.ledgers` is a BTreeMap,
    // so the iteration order is already sorted by corpus.
    let mut combined = purrdf_hash::blake3::Hasher::new();
    for (corpus, tally) in &artifact.ledgers {
        if tally.corpus_only != 0 || tally.dl_gap != 0 {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!(
                    "soak record refuses to render a gap-zero claim: corpus `{corpus}` has \
                     corpus_only={} + dl_gap={} in the committed cost baseline — a soak record \
                     over a diverging corpus would be a false claim; regenerate the baseline \
                     via `make maint-bench-cost-baseline` and resolve the divergence first",
                    tally.corpus_only, tally.dl_gap
                ),
            }));
        }
        combined.update(corpus.as_bytes());
        combined.update(b"\x1f");
        combined.update(tally.finding_graph_blake3.as_bytes());
        combined.update(b"\n");
    }
    let combined_digest = purrdf_hash::hex::encode(combined.finalize().as_bytes());

    let mut lines: Vec<String> = vec![
        "<!-- GENERATED by `gmeow-dev sync --mode update --outputs generated` (soak) — DO NOT EDIT. -->".to_string(),
        String::new(),
        "# gmeow divergence-ledger soak-window record".to_string(),
        String::new(),
        "The checkable form of \"ledger gap-zero over a soak window\": a single-run tally is"
            .to_string(),
        "not soak evidence. This record projects the committed cost/agreement baseline".to_string(),
        "(`bench/cost-baseline.json`) and states, per corpus, that the divergence ledger is"
            .to_string(),
        "GAP-ZERO (`corpus_only == 0 && dl_gap == 0`) together with the INVARIANT finding-graph"
            .to_string(),
        "blake3 digest — a pure function of the committed comparisons, so this record is"
            .to_string(),
        "byte-stable and strict `sync` reproduces it without running a benchmark.".to_string(),
        String::new(),
        format!(
            "The live invariant is enforced on-gate by `gmeow-bench-engines --soak {SOAK_WINDOW}` \
             (the `make bench-soak` lane): it re-runs the DETERMINISTIC native-vs-published \
             agreement check {SOAK_WINDOW} times over the committed mini corpora and hard-fails \
             unless EVERY run is gap-zero AND its finding-graph fingerprint is byte-identical \
             across all {SOAK_WINDOW} runs (a drifting fingerprint is itself a divergence finding \
             — reproducibility is the soak invariant)."
        ),
        String::new(),
        format!("Soak window: {SOAK_WINDOW} runs."),
        String::new(),
        "## Per-corpus gap-zero + invariant finding-graph digest".to_string(),
        String::new(),
        "| corpus | cases | agree | corpus_only | dl_gap | gap_zero | finding_graph_blake3 |"
            .to_string(),
        "|---|---|---|---|---|---|---|".to_string(),
    ];
    for (corpus, tally) in &artifact.ledgers {
        lines.push(format!(
            "| {} | {} | {} | {} | {} | {} | {} |",
            corpus,
            tally.cases,
            tally.agree,
            tally.corpus_only,
            tally.dl_gap,
            true,
            tally.finding_graph_blake3,
        ));
    }

    lines.push(String::new());
    lines.push(format!(
        "Combined soak digest (blake3 over the sorted per-corpus finding-graph digests): `{combined_digest}`."
    ));
    lines.push(String::new());
    lines.push(format!(
        "Gap-zero HELD across {} corpora at soak window {SOAK_WINDOW}; pins native `{}`, \
         backward reference `{}`.",
        artifact.ledgers.len(),
        artifact.engine_pins.native,
        artifact.engine_pins.backward_reference,
    ));
    let md = lines.join("\n") + "\n";

    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    out.insert(SOAK_RECORD_PATH.to_string(), md.into_bytes());
    Ok(out)
}

/// The `stage-export-cost-ledger` export-leaf: the committed deterministic cost ledger
/// AND its longitudinal soak-window record (both pure projections of the same committed
/// cost baseline, so the single leaf renders both).
pub struct CostLedgerStage;

impl Stage for CostLedgerStage {
    fn id(&self) -> &str {
        "stage-export-cost-ledger"
    }
    fn consumes(&self) -> &[String] {
        &[]
    }
    fn impl_version(&self) -> &str {
        // v6: surface exact four-worker structural evidence in the cost ledger.
        "cost-ledger.v6"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
        // The committed cost/agreement baseline is the only input; a baseline refresh
        // busts the cache. No benchmark is run here — purely deterministic.
        Ok(vec![root.join(COST_BASELINE_PATH)])
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        // Both the cost ledger and the soak record are projections of the same committed
        // baseline; render both in the one leaf so the soak record needs no new stage
        // (and thus no new consume-edge across carrier.rs / run.rs / module.ttl).
        let mut artifacts = render_cost_ledger(input.root)?;
        artifacts.extend(render_soak(input.root)?);
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            artifacts,
        )))
    }
}

#[path = "bench.tests.rs"]
#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "bench.timing_json.tests.rs"]
mod timing_json_tests;

#[cfg(test)]
#[path = "bench.cost_json.tests.rs"]
mod cost_json_tests;
