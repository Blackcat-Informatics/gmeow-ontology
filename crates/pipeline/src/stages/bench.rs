// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `bench` perf surface (#668): the committed perf leaderboard generator
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

use crate::error::PipelineError;

/// Committed reference-run input (the single source of truth for the leaderboard).
pub const BASELINE_PATH: &str = "bench/baseline.json";
/// Committed, drift-gated leaderboard artifact.
pub const BENCH_LEADERBOARD_PATH: &str = "generated/bench/leaderboard.md";

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
fn collect_estimates(criterion_root: &Path) -> Result<BTreeMap<String, Estimate>, PipelineError> {
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
        let data: Value = serde_json::from_slice(&bytes)
            .map_err(|e| PipelineError::Parse(format!("criterion estimates parse: {e}")))?;
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
fn walk_estimates(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), PipelineError> {
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
fn point_estimate(data: &Value, key: &str) -> Result<f64, PipelineError> {
    data.get(key)
        .and_then(|v| v.get("point_estimate"))
        .and_then(Value::as_f64)
        .ok_or_else(|| {
            PipelineError::Parse(format!("criterion estimate missing {key}.point_estimate"))
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
pub fn emit_baseline(criterion_root: &Path) -> Result<String, PipelineError> {
    let estimates = collect_estimates(criterion_root)?;
    // BTreeMap → sorted keys; inner BTreeMap → `mean_ns` before `median_ns`.
    let mut top: BTreeMap<String, BTreeMap<&'static str, u64>> = BTreeMap::new();
    for (name, est) in estimates {
        let mut inner: BTreeMap<&'static str, u64> = BTreeMap::new();
        inner.insert("mean_ns", est.mean_ns);
        inner.insert("median_ns", est.median_ns);
        top.insert(name, inner);
    }
    let mut json = serde_json::to_string_pretty(&top)
        .map_err(|e| PipelineError::Parse(format!("baseline serialize: {e}")))?;
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
    let baseline: BTreeMap<String, Estimate> = if baseline_json.trim().is_empty() {
        BTreeMap::new()
    } else {
        serde_json::from_str(baseline_json).unwrap_or_default()
    };
    let current = collect_estimates(criterion_root).unwrap_or_default();

    let mut names: BTreeSet<&str> = BTreeSet::new();
    names.extend(baseline.keys().map(String::as_str));
    names.extend(current.keys().map(String::as_str));

    let mut lines: Vec<String> = vec![
        "# gmeow perf regression scoreboard (report-only, #668)".to_string(),
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

// ── Committed leaderboard render + Stage impl land in Task 2 ─────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

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
}
