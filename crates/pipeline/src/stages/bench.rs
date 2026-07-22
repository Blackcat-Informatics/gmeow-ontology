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
//!     advisory tolerance band, and renders a scoreboard. It NEVER gates: the
//!     `bench-compare` bin that drives it always exits 0 and runs only in the
//!     off-gate `suite-quality` lane (Principle 18 + the AGENTS doctrine that
//!     timing numbers are evidence, not a fabricated gate).
//!
//! [`emit_baseline`] is the SINGLE producer of `bench/baseline.json` (seed and
//! every refresh go through it), subsuming the retired `scripts/bench_to_json.py`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

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

/// Committed deterministic whole-bundle SHACL validation census baseline (the single
/// source of truth for the census projection). Produced by `gmeow-dev validate
/// --emit-census`, refreshed via `make maint-validate-census`.
pub const VALIDATE_CENSUS_BASELINE_PATH: &str = "bench/validate-census.json";
/// Committed, drift-gated whole-bundle SHACL validation census artifact.
pub const VALIDATE_CENSUS_PATH: &str = "generated/bench/validate-census.md";
/// The 64-hex zero digest a SEEDED census carries (a placeholder that can never be
/// mistaken for a measured finding-graph fingerprint).
pub const CENSUS_ZERO_DIGEST: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

/// The soak window the committed record documents and the on-gate `make bench-soak`
/// lane enforces (`gmeow-bench-engines --soak N`). A single-run tally is not soak
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
#[derive(Clone, Copy, Deserialize)]
struct Estimate {
    mean_ns: u64,
    median_ns: u64,
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
    if !criterion_root.is_dir() {
        return Ok(out);
    }
    let mut files: Vec<PathBuf> = Vec::new();
    walk_estimates(criterion_root, &mut files)?;
    for path in files {
        // Components between criterion_root and the trailing `new/estimates.json`.
        let rel = path
            .strip_prefix(criterion_root)
            .expect("walked path is under root");
        let parts: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        // `<group>/<bench>/new/estimates.json` ⇒ at least 4 components, with the
        // trailing `new/estimates.json` already guaranteed by `walk_estimates`.
        if parts.len() < 4 {
            continue;
        }
        let name = parts[..parts.len() - 2].join("/");
        let bytes = std::fs::read(&path)?;
        let data: Value = serde_json::from_slice(&bytes).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!("criterion estimates parse: {e}"),
            })
        })?;
        let mean = point_estimate(&data, "mean")?;
        let median = point_estimate(&data, "median")?;
        out.insert(
            name,
            Estimate {
                mean_ns: round_ns(mean),
                median_ns: round_ns(median),
            },
        );
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
    data.get(key)
        .and_then(|v| v.get("point_estimate"))
        .and_then(Value::as_f64)
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!("criterion estimate missing {key}.point_estimate"),
            })
        })
}

/// Round a nanosecond `f64` point estimate to the nearest integer ns (`round`,
/// ties away from zero — the criterion estimates are always positive).
fn round_ns(ns: f64) -> u64 {
    if !ns.is_finite() || ns < 0.0 {
        return 0;
    }
    ns.round() as u64
}

// ── Baseline emit (THE single `bench/baseline.json` producer) ───────────────────

/// Flatten a live criterion run into the committed baseline JSON: sorted keys,
/// integer-nanosecond `mean_ns`/`median_ns`, trailing newline. This is the only
/// producer of `bench/baseline.json` (initial seed and every refresh), so there
/// is no second format to drift against.
pub fn emit_baseline(criterion_root: &Path) -> Result<String, gmeow_errors::Diag> {
    let estimates = collect_estimates(criterion_root)?;
    // BTreeMap → sorted keys; inner BTreeMap → `mean_ns` before `median_ns`.
    let mut top: BTreeMap<String, BTreeMap<&'static str, u64>> = BTreeMap::new();
    for (name, est) in estimates {
        let mut inner: BTreeMap<&'static str, u64> = BTreeMap::new();
        inner.insert("mean_ns", est.mean_ns);
        inner.insert("median_ns", est.median_ns);
        top.insert(name, inner);
    }
    let mut json = serde_json::to_string_pretty(&top).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("baseline serialize: {e}"),
        })
    })?;
    json.push('\n');
    Ok(json)
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

/// Render the report-only regression scoreboard: live criterion run vs the
/// committed baseline. Advisory — the caller always exits 0. A benchmark present
/// in both is classified on its median Δ%; baseline-only ⇒ `missing`, run-only ⇒
/// `new`.
pub fn compare_against_baseline(criterion_root: &Path, baseline_json: &str) -> String {
    // Report-only path: never gate (always exit 0), but never SWALLOW an error
    // either — surface it to stderr (the job-summary log) before degrading, so a
    // misleading all-`missing`/all-`new` board is always explained. The committed
    // `render_bench_leaderboard` path hard-fails instead; only this advisory path
    // degrades. See / Principle 18.
    let baseline: BTreeMap<String, Estimate> = if baseline_json.trim().is_empty() {
        BTreeMap::new()
    } else {
        serde_json::from_str(baseline_json).unwrap_or_else(|e| {
            tracing::warn!(
                target: "bench_compare",
                error = %e,
                "committed baseline JSON is unparsable; treating every benchmark as `new`",
            );
            BTreeMap::new()
        })
    };
    let current = collect_estimates(criterion_root).unwrap_or_else(|e| {
        tracing::warn!(
            target: "bench_compare",
            error = %e,
            "could not collect live criterion estimates; treating every baseline benchmark as `missing`",
        );
        BTreeMap::new()
    });

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
            (Some(b), Some(c)) => {
                let delta = if b.median_ns == 0 {
                    0.0
                } else {
                    (c.median_ns as f64 - b.median_ns as f64) / b.median_ns as f64 * 100.0
                };
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
    let baseline: BTreeMap<String, Estimate> = serde_json::from_slice(&bytes).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("baseline parse: {e}"),
        })
    })?;

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
#[derive(Deserialize)]
struct EnginePins {
    native: String,
    backward_reference: String,
}

/// The native engine's deterministic cost record for one bench case. `answer_count`
/// is the backward fragment's count; `derived_count` every other fragment's — exactly
/// one is present, so [`CaseRecord::count`] hard-fails if neither is.
#[derive(Deserialize)]
struct NativeCost {
    consumed_steps: u64,
    #[serde(default)]
    derived_count: Option<u64>,
    #[serde(default)]
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
    #[serde(default)]
    joined_rows: Option<u64>,
    /// Incremental-only clean-rebuild comparator, embedded in the exact descriptor.
    #[serde(default)]
    scratch: Option<ScratchCost>,
    /// Forward-only cold/warm complete-evaluation evidence for the bounded physical
    /// plan cache.
    #[serde(default)]
    plan_cache: Option<PlanCacheCost>,
    /// Forward-only bounded-provenance Record/Skip evidence over the same physical
    /// plan and fact closure.
    #[serde(default)]
    provenance: Option<ProvenanceCost>,
    /// Incremental-grounding-only deterministic work vector and explicit solver
    /// boundary.
    #[serde(default)]
    grounding: Option<IncrementalGroundingCost>,
}

#[derive(Deserialize)]
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

#[derive(Deserialize)]
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

#[derive(Deserialize)]
struct PlanEvaluationCost {
    cache_hit: bool,
    plan_builds: u64,
    planning_units: u64,
    consumed_steps: u64,
    peak_live_bytes: u64,
    closure_blake3: String,
    cost_vector: Vec<(String, String, u32, u64)>,
}

#[derive(Deserialize)]
struct ProvenanceCost {
    record: ProvenanceModeCost,
    skip: ProvenanceModeCost,
    closure_parity: bool,
    step_parity: bool,
    annotation_complete: bool,
    record_peak_overhead_bytes: i128,
    record_alloc_count_overhead: i128,
}

#[derive(Deserialize)]
struct ProvenanceModeCost {
    annotation_count: u64,
    #[serde(default)]
    max_proof_height: Option<u32>,
    consumed_steps: u64,
    fact_count: u64,
    fact_closure_blake3: String,
    alloc_count: u64,
    peak_live_bytes: u64,
}

/// The clean native rebuild paired with an incremental transaction.
#[derive(Deserialize)]
struct ScratchCost {
    consumed_steps: u64,
    derived_count: u64,
    peak_live_bytes: u64,
    cost_vector: Vec<(String, String, u32, u64)>,
    #[serde(default)]
    ground_rule_probe_rows: Option<u64>,
    #[serde(default)]
    active_ground_rules: Option<u64>,
}

/// The deterministic verdict-agreement booleans for one bench case.
#[derive(Deserialize)]
struct Agreement {
    native_vs_golden: bool,
    #[serde(default)]
    incremental_insert_vs_scratch: Option<bool>,
    #[serde(default)]
    incremental_retract_vs_scratch: Option<bool>,
}

/// One `(corpus, case)` deterministic cost + agreement record.
#[derive(Deserialize)]
struct CaseRecord {
    corpus: String,
    case: String,
    fragment: String,
    native: NativeCost,
    agreement: Agreement,
}

impl CaseRecord {
    /// The native derived / answer count (whichever the fragment carries). Hard-fails
    /// if the artifact carries neither — an absent measure is never rendered as `0`.
    fn count(&self) -> Result<u64, gmeow_errors::Diag> {
        self.native
            .derived_count
            .or(self.native.answer_count)
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::Parse {
                    message: format!(
                        "cost baseline case {}/{} carries neither derived_count nor answer_count",
                        self.corpus, self.case
                    ),
                })
            })
    }
}

/// One corpus's divergence-ledger tally (the content-addressed agreement fold).
#[derive(Deserialize)]
struct LedgerTally {
    cases: u64,
    agree: u64,
    corpus_only: u64,
    dl_gap: u64,
    finding_count: u64,
    finding_graph_blake3: String,
}

/// Scheduler-independent evidence from the permanent four-worker rule fixture.
#[derive(Deserialize)]
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
#[derive(Deserialize)]
struct CostArtifact {
    engine_pins: EnginePins,
    cases: Vec<CaseRecord>,
    rule_parallelism: RuleParallelCost,
    ledgers: BTreeMap<String, LedgerTally>,
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
    let bytes = std::fs::read(root.join(COST_BASELINE_PATH))?;
    let artifact: CostArtifact = serde_json::from_slice(&bytes).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("cost baseline parse: {e}"),
        })
    })?;
    artifact.rule_parallelism.validate()?;

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
                case.native.joined_rows.unwrap_or(0),
                case.agreement
                    .incremental_insert_vs_scratch
                    .unwrap_or(false),
                case.agreement
                    .incremental_retract_vs_scratch
                    .unwrap_or(false),
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
                case.agreement
                    .incremental_insert_vs_scratch
                    .unwrap_or(false),
                case.agreement
                    .incremental_retract_vs_scratch
                    .unwrap_or(false),
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
                provenance.record.max_proof_height.unwrap_or(0),
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
    let bytes = std::fs::read(root.join(COST_BASELINE_PATH))?;
    let artifact: CostArtifact = serde_json::from_slice(&bytes).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("cost baseline parse: {e}"),
        })
    })?;
    artifact.rule_parallelism.validate()?;

    // Assert gap-zero per corpus BEFORE rendering, and fold the invariant per-corpus
    // finding-graph digests into a single combined soak digest (blake3 over the sorted
    // `corpus\x1f<per-corpus finding_graph_blake3>` lines — a pure function of the
    // committed ledgers, so the record is byte-stable). `artifact.ledgers` is a BTreeMap,
    // so the iteration order is already sorted by corpus.
    let mut combined = blake3::Hasher::new();
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
    let combined_digest = combined.finalize().to_hex().to_string();

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

// ── Committed whole-bundle SHACL validation census (drift-gated export leaf) ─────

/// The committed deterministic whole-bundle SHACL validation census
/// (`bench/validate-census.json`). Every field is a deterministic integer count,
/// boolean verdict, or a stable finding-graph digest — NO wall-clock, NO thread
/// count, NO peak-RSS (the raw parallel-SHACL wall-time drop is PR/issue evidence,
/// never folded). The census is a property of the FINISHED bundle, so — exactly like
/// the cost baseline — it is measured OFF-GATE (`make maint-validate-census`,
/// `gmeow-dev validate --emit-census`) and committed; strict `sync` reproduces the
/// projection byte-for-byte from this committed baseline without validating anything.
///
/// `supports_parallel_focus_eval` is a STATIC engine-capability constant (the SHACL
/// engine evaluates focus nodes in parallel), NOT a runtime thread-dependent measure —
/// so it is deterministic and foldable.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct ValidateCensus {
    /// `true` while the record is an honest zero PLACEHOLDER awaiting the off-gate
    /// refresh; `false` once `gmeow-dev validate --emit-census` has measured it. A
    /// seed is constrained to the pure zero placeholder so it can NEVER masquerade as
    /// measured data (see [`ValidateCensus::validate`]).
    pub seeded: bool,
    /// The STATIC engine-capability fact that the SHACL engine evaluates focus nodes in
    /// parallel (a constant, not a runtime thread-count measure).
    pub supports_parallel_focus_eval: bool,
    /// The whole-bundle SHACL conformance verdict (`sh:conforms`).
    pub conforms: bool,
    /// The number of shapes in the enforced shape union (node shapes ∪ nested property
    /// shapes).
    pub shape_count: u64,
    /// The number of constraints across every shape in the enforced shape union.
    pub constraint_count: u64,
    /// The number of DISTINCT finding-bearing focus nodes (focus nodes that produced at
    /// least one validation result). Zero on a conforming bundle.
    pub focus_node_count: u64,
    /// The number of validation results (findings). Zero on a conforming bundle.
    pub finding_count: u64,
    /// The stable blake3 fingerprint of the sorted finding set (`CENSUS_ZERO_DIGEST`
    /// when seeded / conforming with no findings).
    pub report_blake3: String,
}

impl ValidateCensus {
    /// Reject a malformed or internally-inconsistent census (mirrors
    /// [`RuleParallelCost::validate`]): no-optionality forbids rendering a census that
    /// silently misrepresents the bundle.
    pub fn validate(&self) -> Result<(), gmeow_errors::Diag> {
        let err =
            |message: String| Err(gmeow_errors::Diag::of_kind(crate::error::Parse { message }));
        // The digest is exactly 64 lowercase-hex characters (blake3, stable form).
        let digest_ok = self.report_blake3.len() == 64
            && self
                .report_blake3
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if !digest_ok {
            return err(format!(
                "validate-census report_blake3 must be 64 lowercase-hex chars, got {:?}",
                self.report_blake3
            ));
        }
        // SHACL conformance ⟺ no findings ⟺ no finding-bearing focus nodes.
        if self.conforms && (self.finding_count != 0 || self.focus_node_count != 0) {
            return err(
                "validate-census claims conforms=true but carries findings/focus nodes".to_owned(),
            );
        }
        if !self.conforms && self.finding_count == 0 {
            return err(
                "validate-census claims conforms=false but carries zero findings".to_owned(),
            );
        }
        if self.focus_node_count > self.finding_count {
            return err(format!(
                "validate-census focus_node_count {} exceeds finding_count {} (a focus node can \
                 only be finding-bearing via a finding)",
                self.focus_node_count, self.finding_count
            ));
        }
        if self.seeded {
            // A SEED is the pure zero placeholder: it can never pose as measured data.
            let is_zero_placeholder = self.conforms
                && self.supports_parallel_focus_eval
                && self.shape_count == 0
                && self.constraint_count == 0
                && self.focus_node_count == 0
                && self.finding_count == 0
                && self.report_blake3 == CENSUS_ZERO_DIGEST;
            if !is_zero_placeholder {
                return err(
                    "a SEEDED validate-census must be the zero placeholder (conforms=true, \
                     supports_parallel_focus_eval=true, all counts 0, report_blake3 all-zero) — \
                     refresh it via `make maint-validate-census` to record measured values"
                        .to_owned(),
                );
            }
        } else {
            // A MEASURED census is over the real bundle, which always has shapes and
            // constraints — refuse a "measured" census that is vacuously empty.
            if self.shape_count == 0 || self.constraint_count == 0 {
                return err(
                    "a measured (seeded=false) validate-census must carry shape_count>0 and \
                     constraint_count>0 — an empty shape union is not a whole-bundle census"
                        .to_owned(),
                );
            }
        }
        Ok(())
    }
}

/// Read + parse + validate the committed census baseline. Hard-fails if
/// `bench/validate-census.json` is missing, malformed, or internally inconsistent
/// (no degraded fallback — mirrors [`render_cost_ledger`]).
fn load_validate_census(root: &Path) -> Result<ValidateCensus, gmeow_errors::Diag> {
    let bytes = std::fs::read(root.join(VALIDATE_CENSUS_BASELINE_PATH))?;
    let census: ValidateCensus = serde_json::from_slice(&bytes).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("validate-census baseline parse: {e}"),
        })
    })?;
    census.validate()?;
    Ok(census)
}

/// Render the deterministic whole-bundle SHACL validation census from the committed
/// baseline. PURELY deterministic — integer counts, boolean verdicts, and a stable
/// digest string, no `f64`→string formatting — so it survives the strict-sync byte gate
/// without ever validating anything. Hard-fails if the baseline is missing / malformed /
/// inconsistent (no degraded fallback).
pub(crate) fn render_validate_census(
    root: &Path,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let census = load_validate_census(root)?;

    let provenance = if census.seeded {
        "> SEEDED placeholder — the whole-bundle SHACL census has NOT yet been measured. \
Every count is zero and the digest is the zero placeholder until `make maint-validate-census` \
records the real values off-gate. This banner is the honest provenance marker (no fabricated counts)."
            .to_string()
    } else {
        "> MEASURED — the counts below are a deterministic projection of the committed census \
baseline, measured off-gate over the whole bundle."
            .to_string()
    };

    let lines: Vec<String> = vec![
        "<!-- GENERATED by `gmeow-dev sync --mode update --outputs generated` (validate-census) — DO NOT EDIT. -->".to_string(),
        String::new(),
        "# gmeow whole-bundle SHACL validation census".to_string(),
        String::new(),
        "The dogfooded `gmeow:BenchmarkObservation` record of the whole-bundle SHACL".to_string(),
        "validation: a deterministic projection of the committed census baseline".to_string(),
        "(`bench/validate-census.json`), refreshed via `make maint-validate-census`. Every".to_string(),
        "value is an integer count, a boolean verdict, or a stable finding-graph digest —".to_string(),
        "NO wall-clock, NO thread count, NO peak-RSS. The raw parallel-SHACL wall-time drop".to_string(),
        "is PR/issue evidence, NEVER folded (determinism is non-negotiable); strict `sync`".to_string(),
        "reproduces this record byte-for-byte from the committed baseline without validating".to_string(),
        "anything.".to_string(),
        String::new(),
        provenance,
        String::new(),
        format!(
            "The SHACL engine statically supports parallel focus-node evaluation: {}.",
            census.supports_parallel_focus_eval
        ),
        String::new(),
        "| metric | value |".to_string(),
        "|---|---|".to_string(),
        format!("| conforms | {} |", census.conforms),
        format!("| shapes | {} |", census.shape_count),
        format!("| constraints | {} |", census.constraint_count),
        format!("| findings | {} |", census.finding_count),
        format!("| finding-bearing focus nodes | {} |", census.focus_node_count),
        format!("| supports parallel focus eval | {} |", census.supports_parallel_focus_eval),
        format!("| finding-graph blake3 | `{}` |", census.report_blake3),
    ];
    let md = lines.join("\n") + "\n";

    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    out.insert(VALIDATE_CENSUS_PATH.to_string(), md.into_bytes());
    Ok(out)
}

/// Render the canonical `gmeow:shaclValidationCensus` individual as the exact Turtle
/// block committed in `slices/core/pipeline/module.ttl`. The off-gate refresh lane emits
/// this block for the maintainer to commit alongside the JSON baseline; the drift gate
/// ([`census_individual_block_is_committed_in_module_ttl`]) proves the committed
/// ontology individual never diverges from the committed baseline (the ontology fact and
/// the machine record are one).
#[must_use]
pub fn render_census_individual_ttl(census: &ValidateCensus) -> String {
    format!(
        "gmeow:shaclValidationCensus\n\
         \x20   a gmeow:BenchmarkObservation ;\n\
         \x20   rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/pipeline> ;\n\
         \x20   rdfs:label \"SHACL validation census\"@x-gmeow-english ;\n\
         \x20   skos:definition \"The dogfooded whole-bundle SHACL validation census: the conformance verdict, enforced shape/constraint counts, finding count, finding-bearing focus-node count, and stable finding-graph digest of the finished bundle. A deterministic, foldable observation measured off-gate (make maint-validate-census) and committed; the raw parallel-SHACL wall-time is PR/issue evidence, never folded. Kept byte-consistent with bench/validate-census.json by the pipeline drift gate.\"@x-gmeow-english ;\n\
         \x20   gmeow:observationSeeded {seeded} ;\n\
         \x20   gmeow:observedConforms {conforms} ;\n\
         \x20   gmeow:supportsParallelFocusEval {parallel} ;\n\
         \x20   gmeow:shapeCount \"{shapes}\"^^xsd:nonNegativeInteger ;\n\
         \x20   gmeow:constraintCount \"{constraints}\"^^xsd:nonNegativeInteger ;\n\
         \x20   gmeow:findingCount \"{findings}\"^^xsd:nonNegativeInteger ;\n\
         \x20   gmeow:focusNodeCount \"{focus}\"^^xsd:nonNegativeInteger ;\n\
         \x20   gmeow:reportDigest \"{digest}\" ;\n\
         \x20   gmeow:graphBoxRole gmeow:boxABox .\n",
        seeded = census.seeded,
        conforms = census.conforms,
        parallel = census.supports_parallel_focus_eval,
        shapes = census.shape_count,
        constraints = census.constraint_count,
        findings = census.finding_count,
        focus = census.focus_node_count,
        digest = census.report_blake3,
    )
}

/// Measure a `ValidateCensus` from a raw whole-bundle SHACL report and the enforced shape
/// union. Used by the OFF-GATE refresh lane (`gmeow-dev validate --emit-census`); lives
/// here so the blake3 digest and the counting rules have a single definition shared with
/// the render/validate gate. Deterministic: the digest is blake3 over the SORTED finding
/// tuples, the counts are pure structural folds, and `supports_parallel_focus_eval` is a
/// STATIC engine-capability constant. `seeded` is always `false` — a measured census is
/// never a seed.
#[must_use]
pub fn measure_validate_census(
    report: &purrdf::shapes::report::ValidationReport,
    shapes: &purrdf::shapes::shapes::Shapes,
) -> ValidateCensus {
    // Enforced-shape-union structural counts (node shapes ∪ nested property shapes).
    let mut shape_count: u64 = 0;
    let mut constraint_count: u64 = 0;
    for shape in &shapes.node_shapes {
        shape_count += 1;
        constraint_count += shape.constraints.len() as u64;
        for property in &shape.property_shapes {
            count_property_shape(property, &mut shape_count, &mut constraint_count);
        }
    }

    // Distinct finding-bearing focus nodes + a stable finding-graph digest (blake3 over the
    // SORTED finding tuples — a pure function of the finding set, so byte-reproducible).
    let mut focus_nodes: BTreeSet<String> = BTreeSet::new();
    let mut finding_lines: Vec<String> = Vec::with_capacity(report.results.len());
    for result in &report.results {
        focus_nodes.insert(result.focus_value());
        finding_lines.push(format!(
            "{}\x1f{}\x1f{}\x1f{}\x1f{}\x1f{:?}",
            result.focus_value(),
            result.source_shape,
            result.source_constraint_component.as_str(),
            result
                .result_path
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
            result
                .value
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
            result.severity,
        ));
    }
    finding_lines.sort();
    let report_blake3 = if report.results.is_empty() {
        CENSUS_ZERO_DIGEST.to_owned()
    } else {
        let mut hasher = blake3::Hasher::new();
        for line in &finding_lines {
            hasher.update(line.as_bytes());
            hasher.update(b"\n");
        }
        hasher.finalize().to_hex().to_string()
    };

    ValidateCensus {
        seeded: false,
        supports_parallel_focus_eval: true,
        conforms: report.conforms,
        shape_count,
        constraint_count,
        focus_node_count: focus_nodes.len() as u64,
        finding_count: report.results.len() as u64,
        report_blake3,
    }
}

/// Recursively fold one property shape (and its nested property shapes) into the running
/// shape / constraint counts.
fn count_property_shape(
    property: &purrdf::shapes::shapes::PropertyShape,
    shape_count: &mut u64,
    constraint_count: &mut u64,
) {
    *shape_count += 1;
    *constraint_count += property.constraints.len() as u64;
    for nested in &property.property_shapes {
        count_property_shape(nested, shape_count, constraint_count);
    }
}

/// The `stage-export-cost-ledger` export-leaf: the committed deterministic cost ledger
/// AND its longitudinal soak-window record AND the whole-bundle SHACL validation census
/// (all pure projections of committed baselines, so the single leaf renders all three —
/// the "one leaf renders many" trick that needs no new consume-edge).
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
        // v7: also render the whole-bundle SHACL validation census from its committed
        // baseline (the one leaf renders cost-ledger + soak + validate-census).
        "cost-ledger.v7"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
        // The committed cost/agreement baseline AND the committed validation-census
        // baseline are the inputs; a refresh of either busts the cache. No benchmark and
        // no validation are run here — purely deterministic projections.
        Ok(vec![
            root.join(COST_BASELINE_PATH),
            root.join(VALIDATE_CENSUS_BASELINE_PATH),
        ])
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        // The cost ledger, the soak record, and the whole-bundle SHACL validation census
        // are projections of committed baselines; render all three in the one leaf so
        // neither the soak record nor the census needs a new stage (and thus no new
        // consume-edge across carrier.rs / run.rs / module.ttl).
        let mut artifacts = render_cost_ledger(input.root)?;
        artifacts.extend(render_soak(input.root)?);
        artifacts.extend(render_validate_census(input.root)?);
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            artifacts,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    /// Repo root (the workspace, two levels up from this crate's manifest).
    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn bench_leaderboard_is_byte_identical_to_committed() {
        // The committed generated/bench/leaderboard.md must be reproduced
        // byte-for-byte from the committed bench/baseline.json (the drift gate).
        let root = repo_root();
        let arts = render_bench_leaderboard(&root).expect("render bench leaderboard");
        let built = arts
            .get(BENCH_LEADERBOARD_PATH)
            .expect("leaderboard produced");
        let committed = std::fs::read(root.join(BENCH_LEADERBOARD_PATH))
            .expect("committed generated/bench/leaderboard.md exists");
        assert_eq!(
            built,
            &committed,
            "generated/bench/leaderboard.md drifted from committed (len built {} vs committed {})",
            built.len(),
            committed.len()
        );
    }

    #[test]
    fn cost_ledger_is_byte_identical_to_committed() {
        // The committed generated/bench/cost-ledger.md must be reproduced
        // byte-for-byte from the committed bench/cost-baseline.json (the drift gate).
        let root = repo_root();
        let arts = render_cost_ledger(&root).expect("render cost ledger");
        let built = arts.get(COST_LEDGER_PATH).expect("cost ledger produced");
        let committed = std::fs::read(root.join(COST_LEDGER_PATH))
            .expect("committed generated/bench/cost-ledger.md exists");
        assert_eq!(
            built,
            &committed,
            "generated/bench/cost-ledger.md drifted from committed (len built {} vs committed {})",
            built.len(),
            committed.len()
        );
    }

    #[test]
    fn cost_ledger_returns_contextual_error_for_incomplete_grounding_evidence() {
        let root = repo_root();
        let bytes = fs::read(root.join(COST_BASELINE_PATH)).expect("read committed baseline");
        let mut artifact: serde_json::Value =
            serde_json::from_slice(&bytes).expect("parse committed baseline");
        let broken = artifact["cases"]
            .as_array_mut()
            .expect("cases array")
            .iter_mut()
            .find(|case| !case["native"]["grounding"].is_null())
            .expect("committed baseline has incremental-grounding evidence");
        let corpus = broken["corpus"].as_str().unwrap().to_owned();
        let case = broken["case"].as_str().unwrap().to_owned();
        broken["native"].as_object_mut().unwrap().remove("scratch");

        let tmp = tempdir().expect("temp root");
        fs::create_dir(tmp.path().join("bench")).expect("bench dir");
        fs::write(
            tmp.path().join(COST_BASELINE_PATH),
            serde_json::to_vec(&artifact).unwrap(),
        )
        .expect("write malformed baseline");
        let error = render_cost_ledger(tmp.path())
            .expect_err("incomplete grounding evidence must return a diagnostic");
        assert!(error.message().contains(&format!("{corpus}/{case}")));
        assert!(error.message().contains("scratch comparator"));
    }

    #[test]
    fn cost_ledger_rejects_incoherent_rule_parallel_evidence() {
        let root = repo_root();
        let bytes = fs::read(root.join(COST_BASELINE_PATH)).expect("read committed baseline");
        let mut artifact: serde_json::Value =
            serde_json::from_slice(&bytes).expect("parse committed baseline");
        artifact["rule_parallelism"]["critical_path_rows_saved"] = serde_json::json!(95);

        let tmp = tempdir().expect("temp root");
        fs::create_dir(tmp.path().join("bench")).expect("bench dir");
        fs::write(
            tmp.path().join(COST_BASELINE_PATH),
            serde_json::to_vec(&artifact).unwrap(),
        )
        .expect("write malformed baseline");
        let error = render_cost_ledger(tmp.path())
            .expect_err("incoherent rule-parallel evidence must return a diagnostic");
        assert!(
            error
                .message()
                .contains("four-worker rule-parallel evidence")
        );
    }

    #[test]
    fn soak_record_is_byte_identical_to_committed() {
        // The committed generated/bench/soak.md must be reproduced byte-for-byte from
        // the committed bench/cost-baseline.json (the soak drift gate).
        let root = repo_root();
        let arts = render_soak(&root).expect("render soak record");
        let built = arts.get(SOAK_RECORD_PATH).expect("soak record produced");
        let committed = std::fs::read(root.join(SOAK_RECORD_PATH))
            .expect("committed generated/bench/soak.md exists");
        assert_eq!(
            built,
            &committed,
            "generated/bench/soak.md drifted from committed (len built {} vs committed {})",
            built.len(),
            committed.len()
        );
    }

    #[test]
    fn soak_render_is_deterministic() {
        // Emit twice → byte-identical (the record must be byte-reproducible).
        let root = repo_root();
        let a = render_soak(&root).expect("render soak record (1)");
        let b = render_soak(&root).expect("render soak record (2)");
        assert_eq!(a, b, "the soak record must be byte-reproducible");
    }

    /// The seeded zero-placeholder census (matches the committed `bench/validate-census.json`).
    fn seed_census() -> ValidateCensus {
        ValidateCensus {
            seeded: true,
            supports_parallel_focus_eval: true,
            conforms: true,
            shape_count: 0,
            constraint_count: 0,
            focus_node_count: 0,
            finding_count: 0,
            report_blake3: CENSUS_ZERO_DIGEST.to_owned(),
        }
    }

    /// Write `census` to `bench/validate-census.json` under a fresh temp root.
    fn write_census(root: &Path, census: &ValidateCensus) {
        fs::create_dir_all(root.join("bench")).expect("bench dir");
        fs::write(
            root.join(VALIDATE_CENSUS_BASELINE_PATH),
            serde_json::to_vec_pretty(census).unwrap(),
        )
        .expect("write census baseline");
    }

    #[test]
    fn validate_census_render_is_deterministic() {
        // Reads only the committed bench/validate-census.json (not generated/), so this
        // runs without a materialized bundle. Emit twice → byte-identical.
        let root = repo_root();
        let a = render_validate_census(&root).expect("render validate-census (1)");
        let b = render_validate_census(&root).expect("render validate-census (2)");
        assert_eq!(a, b, "the validation census must be byte-reproducible");
    }

    #[test]
    fn validate_census_hard_fails_on_absent_baseline() {
        let tmp = tempdir().expect("temp root");
        render_validate_census(tmp.path())
            .expect_err("an absent census baseline must hard-fail (no degraded fallback)");
    }

    #[test]
    fn validate_census_hard_fails_on_malformed_baseline() {
        let tmp = tempdir().expect("temp root");
        fs::create_dir_all(tmp.path().join("bench")).expect("bench dir");
        fs::write(
            tmp.path().join(VALIDATE_CENSUS_BASELINE_PATH),
            b"{ not valid json ]",
        )
        .expect("write malformed baseline");
        render_validate_census(tmp.path()).expect_err("a malformed census baseline must hard-fail");
    }

    #[test]
    fn validate_census_rejects_inconsistent_conformance() {
        // conforms=true with a non-zero finding count is internally inconsistent.
        let tmp = tempdir().expect("temp root");
        let mut census = seed_census();
        census.seeded = false;
        census.shape_count = 10;
        census.constraint_count = 20;
        census.conforms = true;
        census.finding_count = 3;
        write_census(tmp.path(), &census);
        let err = render_validate_census(tmp.path())
            .expect_err("conforms=true with findings must be rejected");
        assert!(err.message().contains("conforms=true but carries findings"));
    }

    #[test]
    fn validate_census_rejects_seeded_non_placeholder() {
        // A seed that carries measured-looking counts is a lie — rejected.
        let tmp = tempdir().expect("temp root");
        let mut census = seed_census();
        census.shape_count = 42;
        write_census(tmp.path(), &census);
        let err = render_validate_census(tmp.path())
            .expect_err("a seeded census with non-zero counts must be rejected");
        assert!(
            err.message()
                .contains("SEEDED validate-census must be the zero placeholder")
        );
    }

    #[test]
    fn validate_census_rejects_measured_empty_shape_union() {
        // A "measured" census over an empty shape union is not a whole-bundle census.
        let tmp = tempdir().expect("temp root");
        let mut census = seed_census();
        census.seeded = false;
        write_census(tmp.path(), &census);
        let err = render_validate_census(tmp.path())
            .expect_err("a measured census with zero shapes must be rejected");
        assert!(err.message().contains("shape_count>0"));
    }

    #[test]
    fn census_individual_block_is_committed_in_module_ttl() {
        // The DRIFT GATE binding the two committed homes: the ontology individual
        // gmeow:shaclValidationCensus in the pipeline slice module.ttl must be EXACTLY the
        // Turtle projection of the committed bench/validate-census.json baseline, so the
        // queryable ontology fact can never silently diverge from the machine record.
        let root = repo_root();
        let census = load_validate_census(&root).expect("load committed census baseline");
        let block = render_census_individual_ttl(&census);
        let module = std::fs::read_to_string(root.join("slices/core/pipeline/module.ttl"))
            .expect("read pipeline module.ttl");
        assert!(
            module.contains(&block),
            "the committed gmeow:shaclValidationCensus individual drifted from \
             bench/validate-census.json; expected module.ttl to contain:\n{block}"
        );
    }

    #[test]
    fn validate_census_is_byte_identical_to_committed() {
        // The committed generated/bench/validate-census.md must be reproduced
        // byte-for-byte from the committed bench/validate-census.json (the drift gate).
        // Mirrors the sibling *_is_byte_identical_to_committed tests exactly: a post-sync
        // gate that requires the materialized generated/ tree (git-ignored local product).
        let root = repo_root();
        let arts = render_validate_census(&root).expect("render validate-census");
        let built = arts.get(VALIDATE_CENSUS_PATH).expect("census produced");
        let committed = std::fs::read(root.join(VALIDATE_CENSUS_PATH))
            .expect("committed generated/bench/validate-census.md exists");
        assert_eq!(
            built,
            &committed,
            "generated/bench/validate-census.md drifted from committed (len built {} vs committed {})",
            built.len(),
            committed.len()
        );
    }

    /// Write a minimal criterion `new/estimates.json` for `<group>/<bench>`.
    fn write_estimate(root: &Path, group: &str, bench: &str, mean: f64, median: f64) {
        let dir = root.join(group).join(bench).join("new");
        fs::create_dir_all(&dir).unwrap();
        let json = format!(
            "{{\"mean\":{{\"point_estimate\":{mean}}},\"median\":{{\"point_estimate\":{median}}}}}"
        );
        fs::write(dir.join("estimates.json"), &json).unwrap();
        // criterion also writes a `base/` snapshot — it must be ignored.
        let base = root.join(group).join(bench).join("base");
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("estimates.json"), &json).unwrap();
    }

    #[test]
    fn emit_baseline_is_sorted_integer_ns() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        write_estimate(root, "reason", "foundation", 1240.6, 1250.4);
        write_estimate(root, "shacl", "validate_all", 88000.9, 91500.2);
        let json = emit_baseline(root).unwrap();
        // Sorted keys (reason before shacl), integer ns, mean before median.
        assert_eq!(
            json,
            "{\n  \"reason/foundation\": {\n    \"mean_ns\": 1241,\n    \"median_ns\": 1250\n  },\n  \"shacl/validate_all\": {\n    \"mean_ns\": 88001,\n    \"median_ns\": 91500\n  }\n}\n"
        );
    }

    #[test]
    fn compare_classifies_ok_watch_regressed_missing_new() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        // current run
        write_estimate(root, "g", "steady", 100.0, 100.0); // baseline 100 → ok
        write_estimate(root, "g", "slower", 100.0, 108.0); // baseline 100 → +8% watch
        write_estimate(root, "g", "regress", 100.0, 130.0); // baseline 100 → +30% regressed
        write_estimate(root, "g", "fresh", 50.0, 50.0); // not in baseline → new
        let baseline = "{\
\"g/steady\":{\"mean_ns\":100,\"median_ns\":100},\
\"g/slower\":{\"mean_ns\":100,\"median_ns\":100},\
\"g/regress\":{\"mean_ns\":100,\"median_ns\":100},\
\"g/gone\":{\"mean_ns\":100,\"median_ns\":100}}";
        let report = compare_against_baseline(root, baseline);
        assert!(report.contains("| g/steady | 100 | 100 | +0.0 | ok |"));
        assert!(report.contains("| g/slower | 100 | 108 | +8.0 | watch |"));
        assert!(report.contains("| g/regress | 100 | 130 | +30.0 | regressed |"));
        assert!(report.contains("| g/fresh | — | 50 | — | new |"));
        assert!(report.contains("| g/gone | 100 | — | — | missing |"));
        assert!(report.contains("5 benchmark(s): 1 regressed, 1 watch"));
    }

    #[test]
    fn compare_with_empty_baseline_marks_all_new() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        write_estimate(root, "g", "a", 10.0, 10.0);
        let report = compare_against_baseline(root, "");
        assert!(report.contains("| g/a | — | 10 | — | new |"));
    }

    #[test]
    fn compare_degrades_on_malformed_baseline_without_panic() {
        // A PRESENT-but-unparseable baseline must NOT panic the report-only path
        // (it always exits 0) and must NOT be silently dropped — the warning is
        // surfaced to stderr (Gap). The current run is still classified as
        // `new` against the now-empty baseline.
        let dir = tempdir().unwrap();
        let root = dir.path();
        write_estimate(root, "g", "a", 10.0, 10.0);
        let report = compare_against_baseline(root, "{ this is not valid json ]");
        assert!(report.contains("| g/a | — | 10 | — | new |"));
        assert!(report.contains("report-only"));
    }

    #[test]
    fn compare_degrades_on_missing_criterion_root_without_panic() {
        // No live criterion tree at all: every committed baseline benchmark is
        // `missing`, the board still renders, and the call never panics.
        let dir = tempdir().unwrap();
        let root = dir.path().join("does-not-exist");
        let baseline = "{\"g/a\":{\"mean_ns\":100,\"median_ns\":100}}";
        let report = compare_against_baseline(&root, baseline);
        assert!(report.contains("| g/a | 100 | — | — | missing |"));
    }
}
