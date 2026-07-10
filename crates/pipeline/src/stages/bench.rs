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
//!     `check-generated` gate only reads the committed JSON.
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

use serde::Deserialize;
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
/// no `f64`→string formatting — so it survives the `check-generated` byte gate
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
        "<!-- GENERATED by `gmeow regenerate` (bench) — DO NOT EDIT. -->".to_string(),
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
    nemo_rev: String,
    scryer_branch: String,
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
    peak_live_bytes: u64,
    /// Sorted `[rule, predicate, stratum, derivations]` tuples (may be empty at seams
    /// that expose no decomposable vector — never fabricated).
    cost_vector: Vec<(String, String, u32, u64)>,
}

/// The deterministic verdict-agreement booleans for one bench case.
#[derive(Deserialize)]
struct Agreement {
    native_vs_golden: bool,
    native_vs_oracle: bool,
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

/// The committed deterministic cost/agreement artifact (`bench/cost-baseline.json`).
#[derive(Deserialize)]
struct CostArtifact {
    engine_pins: EnginePins,
    cases: Vec<CaseRecord>,
    ledgers: BTreeMap<String, LedgerTally>,
}

/// Render the committed deterministic engine-cost ledger from the committed cost
/// baseline. PURELY deterministic — integer counts and boolean verdicts rendered in
/// sorted `(corpus, case)` order, no wall-clock / peak-RSS / total-allocation scalars
/// (those are report-only in the harness) — so it survives the `check-generated` byte
/// gate without ever running a benchmark. Hard-fails if `bench/cost-baseline.json` is
/// missing or malformed (no degraded fallback).
pub(crate) fn render_cost_ledger(
    root: &Path,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let bytes = std::fs::read(root.join(COST_BASELINE_PATH))?;
    let artifact: CostArtifact = serde_json::from_slice(&bytes).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("cost baseline parse: {e}"),
        })
    })?;

    // Sort by (corpus, case) so the render is independent of the artifact's array order.
    let mut cases: Vec<&CaseRecord> = artifact.cases.iter().collect();
    cases.sort_by(|a, b| (&a.corpus, &a.case).cmp(&(&b.corpus, &b.case)));

    let mut lines: Vec<String> = vec![
        "<!-- GENERATED by `gmeow regenerate` (cost-ledger) — DO NOT EDIT. -->".to_string(),
        String::new(),
        "# gmeow deterministic engine-cost ledger".to_string(),
        String::new(),
        "Committed engine-vs-engine cost/agreement baseline (`bench/cost-baseline.json`),"
            .to_string(),
        "refreshed via `make maint-bench-cost-baseline`. Every value is an integer count".to_string(),
        "or a boolean verdict — NO wall-clock, NO peak-RSS, NO total-allocation scalars".to_string(),
        "(those are report-only in the harness). This is a drift-gated projection of the".to_string(),
        "deterministic cost artifact; `check-generated` reproduces it byte-for-byte from".to_string(),
        "the committed baseline without ever running a benchmark.".to_string(),
        String::new(),
        format!(
            "Engine pins: native `{}`, nemo `{}`, scryer `{}`.",
            artifact.engine_pins.native,
            artifact.engine_pins.nemo_rev,
            artifact.engine_pins.scryer_branch
        ),
        String::new(),
        "## Per-case deterministic cost + verdict-agreement".to_string(),
        String::new(),
        "| corpus | case | fragment | consumed_steps | derived | peak_live_bytes | native_vs_golden | native_vs_oracle |"
            .to_string(),
        "|---|---|---|---|---|---|---|---|".to_string(),
    ];
    for case in &cases {
        lines.push(format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            case.corpus,
            case.case,
            case.fragment,
            case.native.consumed_steps,
            case.count()?,
            case.native.peak_live_bytes,
            case.agreement.native_vs_golden,
            case.agreement.native_vs_oracle,
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
    }
    if vector_rows == 0 {
        lines.push("| — | — | — | — | — | — |".to_string());
    }

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

/// The `stage-export-cost-ledger` export-leaf: the committed deterministic cost ledger.
pub struct CostLedgerStage;

impl Stage for CostLedgerStage {
    fn id(&self) -> &str {
        "stage-export-cost-ledger"
    }
    fn consumes(&self) -> &[String] {
        &[]
    }
    fn impl_version(&self) -> &str {
        "cost-ledger.v1"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
        // The committed cost/agreement baseline is the only input; a baseline refresh
        // busts the cache. No benchmark is run here — purely deterministic.
        Ok(vec![root.join(COST_BASELINE_PATH)])
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            render_cost_ledger(input.root)?,
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
