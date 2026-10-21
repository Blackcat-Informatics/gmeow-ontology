// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored timing inputs: no benchmark, Criterion or corpus execution.

use super::*;
use std::fs;
use tempfile::tempdir;

/// Decode authored bytes through the actual live measurement boundary.
fn live(text: &str) -> Result<Estimate, gmeow_errors::Diag> {
    parse_live_estimate(
        text.as_bytes(),
        Path::new("criterion/g/猫/new/estimates.json"),
        "g/猫",
    )
}

/// Write authored inputs without invoking a producer.
fn write_input(root: &Path, relative: &str, bytes: &[u8]) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

#[test]
fn json_bench_timing_literal_baseline_keeps_sorted_keys_exact_integers_unicode_and_lf() {
    let estimates = BTreeMap::from([
        (
            "猫/\"x".into(),
            Estimate {
                mean_ns: u64::MAX,
                median_ns: 0,
            },
        ),
        (
            "a/b".into(),
            Estimate {
                mean_ns: 12,
                median_ns: 13,
            },
        ),
    ]);
    assert_eq!(
        encode_timing_baseline(&estimates),
        "{\n  \"a/b\": {\n    \"mean_ns\": 12,\n    \"median_ns\": 13\n  },\n  \"猫/\\\"x\": {\n    \"mean_ns\": 18446744073709551615,\n    \"median_ns\": 0\n  }\n}\n"
    );
}

#[test]
fn json_bench_timing_typed_records_refuse_duplicates_but_outer_map_is_last_wins() {
    let parsed = parse_timing_baseline(
        br#"{"x":{"mean_ns":1,"median_ns":2},"x":{"median_ns":4,"ignored":null,"mean_ns":3}}"#,
    )
    .unwrap();
    assert_eq!(parsed["x"].mean_ns, 3);
    assert_eq!(parsed["x"].median_ns, 4);
    for input in [
        r#"{"x":{"mean_ns":1,"mean_ns":1,"median_ns":2}}"#,
        r#"{"x":{"mean_ns":1,"median_ns":2,"median_ns":2}}"#,
        r#"{"x":{"mean_ns":1}}"#,
        r#"{"x":{"median_ns":1}}"#,
        r#"{"x":null}"#,
        r#"{"x":[1,2]}"#,
        r#"[]"#,
        r#"null"#,
    ] {
        assert!(parse_timing_baseline(input.as_bytes()).is_err(), "{input}");
    }
}

#[test]
fn json_bench_timing_baseline_unsigned_tokens_keep_u64_max_and_reject_coercion() {
    let parsed = parse_timing_baseline(
        br#"{"x":{"mean_ns":18446744073709551615,"median_ns":18446744073709551615}}"#,
    )
    .unwrap();
    assert_eq!(parsed["x"].mean_ns, u64::MAX);
    assert_eq!(parsed["x"].median_ns, u64::MAX);
    for token in [
        "-0",
        "-1",
        "1.0",
        "1e0",
        "18446744073709551616",
        "true",
        "null",
        "\"1\"",
    ] {
        for field in ["mean_ns", "median_ns"] {
            let input = r#"{"x":{"mean_ns":1,"median_ns":1}}"#
                .replace(&format!(r#""{field}":1"#), &format!(r#""{field}":{token}"#));
            assert!(parse_timing_baseline(input.as_bytes()).is_err(), "{input}");
        }
    }
}

#[test]
fn json_bench_timing_live_valid_floats_keep_existing_rounding() {
    for (token, expected) in [
        ("0", 0),
        ("-0", 0),
        ("1e-400", 0),
        ("0.49", 0),
        ("0.5", 1),
        ("12.5", 13),
        ("1.25e2", 125),
    ] {
        let input = format!(
            r#"{{"mean":{{"point_estimate":{token}}},"median":{{"point_estimate":{token}}}}}"#
        );
        let parsed = live(&input).unwrap();
        assert_eq!(parsed.mean_ns, expected, "{token}");
        assert_eq!(parsed.median_ns, expected, "{token}");
    }
    assert_eq!(
        round_ns(18_446_744_073_709_549_568.0).unwrap(),
        18_446_744_073_709_549_568
    );
}

#[test]
fn json_bench_timing_live_invalid_metrics_never_become_zero_or_saturated() {
    for token in [
        "-1",
        "-1e-400",
        "1e400",
        "-1e400",
        "18446744073709551615",
        "18446744073709551616",
        "null",
        "false",
        "\"2\"",
    ] {
        for metric in ["mean", "median"] {
            let input = r#"{"mean":{"point_estimate":1},"median":{"point_estimate":1}}"#.replace(
                &format!(r#""{metric}":{{"point_estimate":1}}"#),
                &format!(r#""{metric}":{{"point_estimate":{token}}}"#),
            );
            let error = match live(&input) {
                Ok(_) => panic!("invalid measurement admitted: {input}"),
                Err(error) => error,
            };
            let detail = format!("{error:#}");
            assert!(
                detail.contains("criterion/g/猫/new/estimates.json"),
                "{detail}"
            );
            assert!(detail.contains("g/猫"), "{detail}");
            assert!(
                detail.contains(&format!("{metric}.point_estimate")),
                "{detail}"
            );
        }
    }
    for input in [
        r#"{}"#,
        r#"{"mean":{},"median":{}}"#,
        r#"{"mean":{"point_estimate":1}}"#,
        r#"{"median":{"point_estimate":1}}"#,
    ] {
        assert!(live(input).is_err(), "{input}");
    }
    assert!(round_ns(f64::NAN).is_err());
    assert!(round_ns(f64::INFINITY).is_err());
    assert!(round_ns(f64::NEG_INFINITY).is_err());
}

#[test]
fn json_bench_timing_criterion_freeform_duplicates_are_recursively_last_wins() {
    let parsed = live(r#"{"mean":{"point_estimate":999},"mean":{"point_estimate":1,"point_estimate":2.5},"median":{"point_estimate":4},"extra":{"ignored":null}}"#).unwrap();
    assert_eq!(parsed.mean_ns, 3);
    assert_eq!(parsed.median_ns, 4);
}

#[test]
fn json_bench_timing_required_documents_refuse_blank_malformed_trailing_and_non_utf8() {
    for input in [
        "",
        " ",
        "{",
        "NaN",
        "{} null",
        r#"{"x":{"mean_ns":1,"median_ns":1}}x"#,
    ] {
        assert!(parse_timing_baseline(input.as_bytes()).is_err(), "{input}");
    }
    for input in [
        "",
        " ",
        "{",
        "NaN",
        r#"{"mean":{"point_estimate":1},"median":{"point_estimate":1}} {}"#,
    ] {
        assert!(live(input).is_err(), "{input}");
    }
    assert!(parse_timing_baseline(&[0xff]).is_err());
    assert!(parse_live_estimate(&[0xff], Path::new("invalid"), "x").is_err());
}

#[test]
fn json_bench_timing_comparison_has_literal_legacy_markdown_for_valid_measurements() {
    let baseline =
        parse_timing_baseline(r#"{"g/猫":{"mean_ns":100,"median_ns":100}}"#.as_bytes()).unwrap();
    let current = BTreeMap::from([(
        "g/猫".into(),
        Estimate {
            mean_ns: 130,
            median_ns: 130,
        },
    )]);
    assert_eq!(
        render_timing_comparison(&baseline, &current),
        "# gmeow perf regression scoreboard (report-only)\n\nLive criterion run vs committed `bench/baseline.json`. Median nanoseconds, lower is better. Advisory only (watch > 5%, regressed > 15%); runner jitter is expected and never gates a PR.\n\n| benchmark | baseline (ns) | current (ns) | Δ% | status |\n|---|---|---|---|---|\n| g/猫 | 100 | 130 | +30.0 | regressed |\n\n1 benchmark(s): 1 regressed, 0 watch (report-only).\n"
    );
}

#[test]
fn json_bench_timing_thresholds_missing_new_and_zero_denominator_remain_distinct() {
    let baseline = parse_timing_baseline(br#"{"at5":{"mean_ns":100,"median_ns":100},"over5":{"mean_ns":100,"median_ns":100},"at15":{"mean_ns":100,"median_ns":100},"over15":{"mean_ns":100,"median_ns":100},"gone":{"mean_ns":1,"median_ns":1},"zero":{"mean_ns":0,"median_ns":0}}"#).unwrap();
    let current = [
        ("at5", 105),
        ("over5", 106),
        ("at15", 115),
        ("over15", 116),
        ("new", 7),
        ("zero", 100),
    ]
    .into_iter()
    .map(|(name, median_ns)| {
        (
            name.into(),
            Estimate {
                mean_ns: median_ns,
                median_ns,
            },
        )
    })
    .collect();
    let report = render_timing_comparison(&baseline, &current);
    for row in [
        "| at5 | 100 | 105 | +5.0 | ok |",
        "| over5 | 100 | 106 | +6.0 | watch |",
        "| at15 | 100 | 115 | +15.0 | watch |",
        "| over15 | 100 | 116 | +16.0 | regressed |",
        "| gone | 1 | — | — | missing |",
        "| new | — | 7 | — | new |",
        "| zero | 0 | 100 | — | unavailable |",
    ] {
        assert!(report.contains(row), "{report}");
    }
    assert!(report.contains("7 benchmark(s): 1 regressed, 2 watch"));
}

#[test]
fn json_bench_timing_explicit_empty_inventories_are_not_missing_required_inputs() {
    let directory = tempdir().unwrap();
    let baseline = r#"{"gone":{"mean_ns":1,"median_ns":1}}"#;
    assert!(
        compare_against_baseline(directory.path(), baseline)
            .unwrap()
            .contains("| gone | 1 | — | — | missing |")
    );
    assert!(
        compare_against_baseline(directory.path(), "{}")
            .unwrap()
            .contains("0 benchmark(s)")
    );
    assert!(emit_baseline(directory.path()).is_err());
    assert!(compare_against_baseline(&directory.path().join("absent"), "{}").is_err());
    assert!(emit_baseline(&directory.path().join("absent")).is_err());
    assert!(compare_against_baseline(directory.path(), "").is_err());
    assert!(compare_against_baseline(directory.path(), "{broken").is_err());
}

#[test]
fn json_bench_timing_collector_uses_only_selected_new_paths_and_keeps_invalid_context() {
    let directory = tempdir().unwrap();
    let valid = br#"{"mean":{"point_estimate":1.5},"median":{"point_estimate":2.5}}"#;
    write_input(directory.path(), "g/猫/new/estimates.json", valid);
    write_input(
        directory.path(),
        "g/猫/base/estimates.json",
        b"invalid ignored old snapshot",
    );
    write_input(
        directory.path(),
        "shallow/new/estimates.json",
        b"invalid ignored shallow input",
    );
    assert_eq!(
        emit_baseline(directory.path()).unwrap(),
        "{\n  \"g/猫\": {\n    \"mean_ns\": 2,\n    \"median_ns\": 3\n  }\n}\n"
    );
    assert!(
        compare_against_baseline(directory.path(), "{}")
            .unwrap()
            .contains("| g/猫 | — | 3 | — | new |")
    );
    write_input(
        directory.path(),
        "g/bad/new/estimates.json",
        br#"{"mean":{},"median":{}}"#,
    );
    assert!(emit_baseline(directory.path()).is_err());
    assert!(compare_against_baseline(directory.path(), "{}").is_err());
}

#[test]
fn json_bench_timing_committed_leaderboard_uses_same_typed_baseline_without_measurement() {
    let directory = tempdir().unwrap();
    write_input(
        directory.path(),
        BASELINE_PATH,
        r#"{"g/猫":{"mean_ns":18446744073709551615,"median_ns":2}}"#.as_bytes(),
    );
    let outputs = render_bench_leaderboard(directory.path()).unwrap(); // gmeow-test-input: synthetic-only
    let text = std::str::from_utf8(&outputs[BENCH_LEADERBOARD_PATH]).unwrap();
    assert_eq!(
        text,
        "<!-- GENERATED by `gmeow-dev sync --mode update --outputs generated` (bench) — DO NOT EDIT. -->\n\n# gmeow perf leaderboard: native reasoning + validation hot paths\n\nCommitted reference run (`bench/baseline.json`), refreshed via\n`make maint-bench-baseline`. Median nanoseconds per benchmark, lower is\nbetter. The off-gate `suite-quality` lane reports regressions against this\nbaseline (report-only — never gates a PR).\n\n| benchmark | mean (ns) | median (ns) |\n|---|---|---|\n| g/猫 | 18446744073709551615 | 2 |\n\n1 benchmark(s) in the committed reference run.\n"
    );
    write_input(
        directory.path(),
        BASELINE_PATH,
        r#"{"g/猫":{"mean_ns":1,"median_ns":-0}}"#.as_bytes(),
    );
    assert!(render_bench_leaderboard(directory.path()).is_err()); // gmeow-test-input: synthetic-only
}

#[cfg(unix)]
#[test]
fn json_bench_timing_non_utf8_benchmark_identity_is_refused() {
    use std::os::unix::ffi::OsStringExt as _;
    let directory = tempdir().unwrap();
    let path = directory
        .path()
        .join("g")
        .join(std::ffi::OsString::from_vec(vec![0xff]))
        .join("new");
    fs::create_dir_all(&path).unwrap();
    fs::write(
        path.join("estimates.json"),
        br#"{"mean":{"point_estimate":1},"median":{"point_estimate":1}}"#,
    )
    .unwrap();
    assert!(emit_baseline(directory.path()).is_err());
    let ignored = tempdir().unwrap();
    let shallow = ignored
        .path()
        .join(std::ffi::OsString::from_vec(vec![0xff]))
        .join("new");
    fs::create_dir_all(&shallow).unwrap();
    fs::write(
        shallow.join("estimates.json"),
        b"invalid ignored shallow document",
    )
    .unwrap();
    write_input(
        ignored.path(),
        "g/a/new/estimates.json",
        br#"{"mean":{"point_estimate":1},"median":{"point_estimate":1}}"#,
    );
    assert!(emit_baseline(ignored.path()).is_ok());
}
