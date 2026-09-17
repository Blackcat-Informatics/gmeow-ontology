// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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
    let error = render_cost_ledger(tmp.path()) // gmeow-test-input: synthetic-only
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
    let error = render_cost_ledger(tmp.path()) // gmeow-test-input: synthetic-only
        .expect_err("incoherent rule-parallel evidence must return a diagnostic");
    assert!(
        error
            .message()
            .contains("four-worker rule-parallel evidence")
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
