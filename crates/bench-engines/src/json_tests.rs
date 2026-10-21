// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
use super::*;

fn decode(text: &str) -> Value {
    json::record::read_document(text.as_bytes()).unwrap()
}

fn authored_document(cases: Vec<Value>) -> Value {
    Value::from(Object::new().with("cases", cases.to_json()))
}

fn backward_record() -> Value {
    let mut value = rec("c", "backward", 3);
    value["fragment"] = "backward".to_json();
    let native = value["native"].as_object_mut().unwrap();
    native.remove("derived_count");
    native.insert("answer_count", 3);
    native.insert("answers_fingerprint", "authored-answer-digest");
    value
}

fn incremental_record(fragment: &str) -> Value {
    let mut value = rec("c", "incremental", 3);
    value["fragment"] = fragment.to_json();
    value["native"]["rows_fingerprint"] = "inserted-rows".to_json();
    let mut retraction = Object::new().with("rows_fingerprint", "original-rows");
    if fragment == "incremental" {
        value["native"]["signed_change_count"] = 3.to_json();
        value["native"]["signed_changes_blake3"] = "inserted-signed-changes".to_json();
        retraction = retraction
            .with("derived_count", 1)
            .with("signed_change_count", 3)
            .with("signed_changes_blake3", "retracted-signed-changes");
    }
    value["native"]["retraction"] = Value::from(retraction);
    value
}

fn assert_bad_record_refused_by_both_paths(value: Value) {
    let document = authored_document(vec![value.clone()]);
    let (_, violations) = compare_baselines(&document, &document, &[], &[], &[]);
    assert!(
        !violations.is_empty(),
        "two equally malformed sides must fail"
    );
    let (_directory, path) = write_baseline_doc(&document);
    assert!(run_cost_regression_check(&path, &[value]).is_err());
}

#[test]
fn json_bench_compact_descriptor_keeps_literal_legacy_bytes() {
    let native = Object::new()
        .with("peak_live_bytes", 100)
        .with("alloc_bytes", 9_999)
        .with(
            "proof",
            Value::from(Object::new().with("z", "猫\n\"").with("a", true)),
        )
        .with("derived_count", 3)
        .with("cost_vector", Value::Array(vec![]))
        .with("consumed_steps", 7)
        .with("alloc_count", 50);
    let record = Value::from(Object::new().with("native", Value::from(native)).with(
        "agreement",
        Value::from(Object::new().with("z", true).with("a", false)),
    ));
    assert_eq!(
        cost_descriptor(&record),
        r#"native={"consumed_steps":7,"cost_vector":[],"derived_count":3,"peak_live_bytes":100,"proof":{"a":true,"z":"猫\n\""}} agreement={"a":false,"z":true}"#
    );
}

#[test]
fn json_bench_pretty_report_keeps_sorted_unicode_and_newline_policy() {
    let report = Value::from(Object::new().with("schema", "authored").with(
        "findings",
        Value::Array(vec![Value::from(
            Object::new().with("z", "é\n猫").with("a", Value::Null),
        )]),
    ));
    let expected = "{\n  \"findings\": [\n    {\n      \"a\": null,\n      \"z\": \"é\\n猫\"\n    }\n  ],\n  \"schema\": \"authored\"\n}";
    assert_eq!(pretty_json(&report), expected);
    assert_eq!(pretty_json(&report) + "\n", format!("{expected}\n"));
    assert!(!compact_json(&report).ends_with('\n'));
}

#[test]
fn json_bench_free_form_last_wins_and_terminal_parse_refusals() {
    let parsed = decode(r#"{"z":0,"nested":{"z":0,"a":"猫","z":2},"z":1,"a":[true,null]}"#);
    assert_eq!(
        compact_json(&parsed),
        r#"{"a":[true,null],"nested":{"a":"猫","z":2},"z":1}"#
    );
    for invalid in [
        "",
        "{",
        "[1,]",
        "{\"x\":NaN}",
        "{\"x\":Infinity}",
        "{} true",
        "{}{}",
    ] {
        assert!(
            json::record::read_document(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn json_bench_unsigned_selectors_keep_lexical_and_width_bounds() {
    assert_eq!(decode("18446744073709551615").as_u64(), Some(u64::MAX));
    let mut valid = rec("c", "maximum", u64::MAX);
    valid["native"]["peak_live_bytes"] = u64::MAX.to_json();
    valid["native"]["derived_count"] = u64::MAX.to_json();
    assert!(admit_case(&valid).is_ok());
    for token in [
        "18446744073709551616",
        "1e0",
        "1.0",
        "-1",
        "-0",
        "1e400",
        "null",
        "true",
        "\"1\"",
    ] {
        let value = decode(token);
        assert_eq!(value.as_u64(), None, "{token}");
        for field in ["consumed_steps", "peak_live_bytes", "derived_count"] {
            let mut record = rec("c", "invalid-number", 3);
            record["native"][field] = value.clone();
            assert_bad_record_refused_by_both_paths(record);
        }
    }
}

#[test]
fn json_bench_numeric_value_equality_does_not_weaken_exact_tokens() {
    for (left, right) in [("1", "1.0"), ("1", "1e0"), ("0", "-0")] {
        let (left, right) = (decode(left), decode(right));
        assert_eq!(left, right);
        assert_ne!(compact_json(&left), compact_json(&right));
    }
    let mut old = rec("c", "x", 3);
    old["native"]["cost_vector"] = decode("[[1]]");
    let mut new = old.clone();
    new["native"]["cost_vector"] = decode("[[1e0]]");
    assert_eq!(old["native"]["cost_vector"], new["native"]["cost_vector"]);
    assert!(
        !compare_baselines(
            &authored_document(vec![old]),
            &authored_document(vec![new]),
            &[],
            &[],
            &[]
        )
        .1
        .is_empty()
    );
}

#[test]
fn json_bench_backward_result_drift_fails_despite_lower_peak() {
    let old = backward_record();
    for (field, changed) in [
        ("answer_count", 4.to_json()),
        ("answers_fingerprint", "changed-answers".to_json()),
    ] {
        let mut new = old.clone();
        new["native"]["peak_live_bytes"] = 50.to_json();
        new["native"][field] = changed;
        let (_, violations) = compare_baselines(
            &authored_document(vec![old.clone()]),
            &authored_document(vec![new]),
            &[("c", "backward")],
            &[],
            &[],
        );
        assert!(
            violations
                .iter()
                .any(|v| v.contains("evaluation/output identity")),
            "{field}: {violations:?}"
        );
    }
}

#[test]
fn json_bench_incremental_outputs_are_exact_while_work_may_improve() {
    for fragment in ["incremental", "incremental-grounding"] {
        let old = incremental_record(fragment);
        let mut new = old.clone();
        new["native"]["peak_live_bytes"] = 50.to_json();
        new["native"]["joined_rows"] = 1.to_json();
        new["native"]["inner_iterations"] = 1.to_json();
        new["native"]["retraction"]["ground_rule_probe_rows"] = 1.to_json();
        assert!(
            compare_baselines(
                &authored_document(vec![old.clone()]),
                &authored_document(vec![new.clone()]),
                &[("c", "incremental")],
                &[],
                &[]
            )
            .1
            .is_empty()
        );
        let mut paths = vec![(
            "retraction",
            "rows_fingerprint",
            "changed-retracted-rows".to_json(),
        )];
        if fragment == "incremental" {
            for owner in ["native", "retraction"] {
                paths.push((owner, "signed_change_count", 4.to_json()));
                paths.push((
                    owner,
                    "signed_changes_blake3",
                    "changed-signed-changes".to_json(),
                ));
            }
            paths.push(("retraction", "derived_count", 2.to_json()));
        }
        for (owner, field, value) in paths {
            let mut changed = new.clone();
            if owner == "native" {
                changed["native"][field] = value;
            } else {
                changed["native"]["retraction"][field] = value;
            }
            let (_, violations) = compare_baselines(
                &authored_document(vec![old.clone()]),
                &authored_document(vec![changed]),
                &[("c", "incremental")],
                &[],
                &[],
            );
            assert!(
                violations
                    .iter()
                    .any(|line| line.contains("evaluation/output identity")),
                "{fragment} {owner}.{field}: {violations:?}"
            );
        }
    }
}

#[test]
fn json_bench_incremental_required_outputs_cannot_be_missing_on_both_sides() {
    for fragment in ["incremental", "incremental-grounding"] {
        let value = incremental_record(fragment);
        let mut missing_retraction = value.clone();
        missing_retraction["native"]
            .as_object_mut()
            .unwrap()
            .remove("retraction");
        assert_bad_record_refused_by_both_paths(missing_retraction);
        let mut paths = vec![
            ("native", "rows_fingerprint"),
            ("retraction", "rows_fingerprint"),
        ];
        if fragment == "incremental" {
            paths.extend([
                ("native", "signed_change_count"),
                ("native", "signed_changes_blake3"),
                ("retraction", "signed_change_count"),
                ("retraction", "signed_changes_blake3"),
                ("retraction", "derived_count"),
            ]);
        }
        for (owner, field) in paths {
            for remove in [true, false] {
                let mut malformed = value.clone();
                let object = if owner == "native" {
                    malformed["native"].as_object_mut().unwrap()
                } else {
                    malformed["native"]["retraction"].as_object_mut().unwrap()
                };
                if remove {
                    object.remove(field);
                } else {
                    object.insert(field, Value::Null);
                }
                assert_bad_record_refused_by_both_paths(malformed);
            }
        }
    }
}

#[test]
fn json_bench_five_wire_fragments_keep_their_distinct_minimum_outputs() {
    let existential = rec("c", "existential", 3);
    assert!(existential["native"].get("rows_fingerprint").is_none());
    let mut forward = rec("c", "forward", 3);
    forward["fragment"] = "forward".to_json();
    forward["native"]["rows_fingerprint"] = "forward-rows".to_json();
    let backward = backward_record();
    assert!(backward["native"].get("derived_count").is_none());
    let incremental = incremental_record("incremental");
    let grounding = incremental_record("incremental-grounding");
    assert!(
        grounding["native"]["retraction"]
            .get("derived_count")
            .is_none()
    );
    for value in [existential, forward, backward, incremental, grounding] {
        assert!(admit_case(&value).is_ok(), "{}", compact_json(&value));
        let document = authored_document(vec![value.clone()]);
        assert!(
            compare_baselines(&document, &document, &[], &[], &[])
                .1
                .is_empty()
        );
        let (_directory, path) = write_baseline_doc(&document);
        assert!(run_cost_regression_check(&path, &[value]).is_ok());
    }
}

#[test]
fn json_bench_reordered_keys_pass_both_comparison_paths() {
    let old = rec("c", "x", 3);
    let mut reversed = old.clone();
    let members = reversed.as_object_mut().unwrap();
    *members = Object::from(
        members
            .clone()
            .into_members()
            .into_iter()
            .rev()
            .collect::<Vec<_>>(),
    );
    let native = reversed["native"].as_object_mut().unwrap();
    *native = Object::from(
        native
            .clone()
            .into_members()
            .into_iter()
            .rev()
            .collect::<Vec<_>>(),
    );
    let old_doc = authored_document(vec![old]);
    let new_doc = authored_document(vec![reversed.clone()]);
    assert!(
        compare_baselines(&old_doc, &new_doc, &[], &[], &[])
            .1
            .is_empty()
    );
    let (_directory, path) = write_baseline_doc(&old_doc);
    assert!(run_cost_regression_check(&path, &[reversed]).is_ok());
}

#[test]
fn json_bench_missing_or_mistyped_mandatory_fields_fail_on_both_sides() {
    for field in [
        "consumed_steps",
        "peak_live_bytes",
        "derived_count",
        "cost_vector",
        "engine",
    ] {
        let mut absent = rec("c", "x", 3);
        absent["native"].as_object_mut().unwrap().remove(field);
        assert_bad_record_refused_by_both_paths(absent);
        let mut mistyped = rec("c", "x", 3);
        mistyped["native"][field] = Value::Null;
        assert_bad_record_refused_by_both_paths(mistyped);
    }
    for field in [
        "corpus",
        "case",
        "world",
        "fragment",
        "native",
        "golden_rows",
        "agreement",
    ] {
        let mut absent = rec("c", "x", 3);
        absent.as_object_mut().unwrap().remove(field);
        assert_bad_record_refused_by_both_paths(absent);
    }
    for field in ["answer_count", "answers_fingerprint"] {
        let mut absent = backward_record();
        absent["native"].as_object_mut().unwrap().remove(field);
        assert_bad_record_refused_by_both_paths(absent);
        let mut mistyped = backward_record();
        mistyped["native"][field] = Value::Null;
        assert_bad_record_refused_by_both_paths(mistyped);
    }
    assert_bad_record_refused_by_both_paths(Value::Null);
    for malformed in ["null", "[]", "{}", r#"{"cases":null}"#, r#"{"cases":{}}"#] {
        let document = decode(malformed);
        assert!(
            !compare_baselines(&document, &document, &[], &[], &[])
                .1
                .is_empty()
        );
        let (_directory, path) = write_baseline_doc(&document);
        assert!(run_cost_regression_check(&path, &[]).is_err());
    }
}

#[test]
fn json_bench_duplicate_and_empty_case_keys_are_refused() {
    let value = rec("c", "x", 3);
    let document = authored_document(vec![value.clone(), value.clone()]);
    assert!(
        !compare_baselines(&document, &document, &[], &[], &[])
            .1
            .is_empty()
    );
    let (_directory, path) = write_baseline_doc(&document);
    assert!(run_cost_regression_check(&path, std::slice::from_ref(&value)).is_err());
    let (_directory, path) = write_baseline(vec![value.clone()]);
    assert!(run_cost_regression_check(&path, &[value.clone(), value]).is_err());
    for key in ["corpus", "case", "world", "fragment"] {
        for empty in ["", " "] {
            let mut value = rec("c", "x", 3);
            value[key] = empty.to_json();
            assert_bad_record_refused_by_both_paths(value);
        }
    }
}

#[test]
fn json_bench_profile_and_output_identity_are_stable() {
    let old = rec("c", "x", 3);
    for (field, value) in [("world", "urn:other-world"), ("fragment", "backward")] {
        let mut new = old.clone();
        new[field] = value.to_json();
        if field == "fragment" {
            new["native"]
                .as_object_mut()
                .unwrap()
                .remove("derived_count");
            new["native"]["answer_count"] = 3.to_json();
            new["native"]["answers_fingerprint"] = "answers".to_json();
        }
        let (_directory, path) = write_baseline(vec![old.clone()]);
        assert!(run_cost_regression_check(&path, &[new.clone()]).is_err());
        assert!(
            !compare_baselines(
                &authored_document(vec![old.clone()]),
                &authored_document(vec![new]),
                &[],
                &[],
                &[]
            )
            .1
            .is_empty()
        );
    }
}

#[test]
fn json_bench_missing_advisory_is_unavailable_without_a_fabricated_total() {
    let old = authored_document(vec![rec("c", "x", 3)]);
    let mut new = old.clone();
    new["cases"][0]["native"]
        .as_object_mut()
        .unwrap()
        .remove("alloc_bytes");
    let (report, violations) = compare_baselines(&old, &new, &[], &[("c", "x")], &[]);
    assert!(violations.is_empty());
    assert!(
        report
            .iter()
            .any(|line| line.contains("aggregate alloc unavailable"))
    );
    assert!(!report.iter().any(|line| line.contains("aggregate alloc 0")));
}

#[test]
fn json_bench_full_u64_report_deltas_do_not_cross_the_signed_boundary() {
    let mut old = rec("c", "x", 3);
    old["native"]["peak_live_bytes"] = u64::MAX.to_json();
    old["native"]["alloc_bytes"] = u64::MAX.to_json();
    let mut new = old.clone();
    new["native"]["peak_live_bytes"] = 0.to_json();
    new["native"]["alloc_bytes"] = 0.to_json();
    let (report, violations) = compare_baselines(
        &authored_document(vec![old]),
        &authored_document(vec![new]),
        &[("c", "x")],
        &[("c", "x")],
        &[("c", "x")],
    );
    assert!(violations.is_empty(), "{violations:?}");
    for prefix in ["PEAK-WIN", "adv   c/", "NOREG"] {
        let line = report.iter().find(|line| line.starts_with(prefix)).unwrap();
        assert!(line.contains("(-18446744073709551615)"), "{line}");
    }
}

#[test]
fn json_bench_shared_ledger_ids_survive_and_missing_identities_fail() {
    let comparisons = [ExternalComparison {
        case: "authored/case::count".to_owned(),
        world: "urn:authored:world".to_owned(),
        native: "count=2".to_owned(),
        published: "count=1".to_owned(),
    }];
    let ledger = build_ledger(
        Vec::new(),
        Vec::new(),
        compare_external_corpus("authored", &comparisons),
    );
    let finding = divergence_findings(&ledger)
        .into_iter()
        .find(|finding| finding.code == "reason.divergence.corpus-only")
        .unwrap();
    let record = finding_record(&finding).unwrap();
    assert_eq!(
        record.get("finding_iri").unwrap().as_str(),
        finding.finding_iri.as_deref()
    );
    assert_eq!(
        record.get("anchor_iri").unwrap().as_str(),
        finding.anchor_iri.as_deref()
    );
    assert_eq!(
        record.get("antecedents"),
        Some(&finding.antecedents.to_json())
    );
    assert!(!record.contains_key("message"));
    let with_message = record.with("message", finding.message.to_json());
    assert_eq!(
        with_message.get("message").unwrap().as_str(),
        Some(finding.message.as_str())
    );
    for missing in [None, Some(String::new()), Some(" ".to_owned())] {
        let mut invalid = finding.clone();
        invalid.finding_iri = missing.clone();
        assert!(
            finding_record(&invalid)
                .unwrap_err()
                .to_string()
                .contains("finding_iri")
        );
        let mut invalid = finding.clone();
        invalid.anchor_iri = missing;
        assert!(
            finding_record(&invalid)
                .unwrap_err()
                .to_string()
                .contains("anchor_iri")
        );
    }
}

/// A two-case cost baseline for the `--compare-baseline` comparator: one WIN case
/// (`c/win`) and one small NOREG case (`c/small`), each with the fields the gate reads.
fn compare_doc(win_alloc: u64, win_peak: u64, small_peak: u64, steps: u64) -> Value {
    let mut winner = rec_alloc("c", "win", steps, win_alloc, 100);
    winner["native"]["peak_live_bytes"] = win_peak.to_json();
    let mut small = rec_alloc("c", "small", 1, 100, 100);
    small["native"]["peak_live_bytes"] = small_peak.to_json();
    small["native"]["derived_count"] = 1.to_json();
    Value::from(Object::new().with("cases", vec![winner, small].to_json()))
}

/// The `--compare-baseline` comparator is FALSIFIABLE: it PASSES a genuine win (the
/// authoritative `peak_live_bytes` strictly dropping on the named win case, other peaks
/// non-increased, evaluation byte-identical) and FAILS each distinct violation — a
/// peak-win that does NOT strictly drop, a peak regression on another case, and a
/// determinism drift.  It also proves `alloc_bytes` is ADVISORY: even a gross alloc
/// balloon does NOT fail as long as the deterministic peak win holds.
#[test]
fn compare_baselines_gates_win_and_regressions() {
    let peak_win: &[(&str, &str)] = &[("c", "win")];
    let win: &[(&str, &str)] = &[("c", "win")];
    let noreg: &[(&str, &str)] = &[("c", "small")];
    // `c/win` old peak 500; `c/small` old peak 300.
    let old = compare_doc(100_000, 500, 300, 3);

    // PASS: c/win peak strictly drops 500->480, c/small peak flat, determinism same.
    let (_r, v) = compare_baselines(
        &old,
        &compare_doc(98_000, 480, 300, 3),
        peak_win,
        win,
        noreg,
    );
    assert!(v.is_empty(), "a clean peak win must pass, got: {v:?}");

    // FAIL: the peak-win case's peak_live_bytes did NOT strictly drop (flat 500->500).
    let (_r, v) = compare_baselines(
        &old,
        &compare_doc(98_000, 500, 300, 3),
        peak_win,
        win,
        noreg,
    );
    assert!(
        v.iter().any(|s| s.contains("did not strictly drop")),
        "a flat peak on the win case must fail, got: {v:?}"
    );

    // ADVISORY: a gross alloc balloon (100_000->200_000) does NOT fail — alloc is not
    // gated; the peak win (500->480) still holds, so the verdict is PASS.
    let (_r, v) = compare_baselines(
        &old,
        &compare_doc(200_000, 480, 300, 3),
        peak_win,
        win,
        noreg,
    );
    assert!(
        v.is_empty(),
        "alloc_bytes is advisory: a balloon must NOT fail while the peak win holds, got: {v:?}"
    );

    // FAIL: peak_live_bytes regressed on the small (non-win) case.
    let (_r, v) = compare_baselines(
        &old,
        &compare_doc(98_000, 480, 999, 3),
        peak_win,
        win,
        noreg,
    );
    assert!(
        v.iter().any(|s| s.contains("REGRESSED")),
        "a peak-live regression must fail, got: {v:?}"
    );

    // FAIL: the evaluation moved (consumed_steps changed) — not byte-identical.
    let (_r, v) = compare_baselines(
        &old,
        &compare_doc(98_000, 480, 300, 7),
        peak_win,
        win,
        noreg,
    );
    assert!(
        v.iter().any(|s| s.contains("consumed_steps changed")),
        "a determinism drift must fail, got: {v:?}"
    );
}

/// A minimal per-case deterministic record with the fields the cost-regression
/// check compares, carrying fixed allocation scalars.
fn rec(corpus: &str, case: &str, steps: u64) -> Value {
    rec_alloc(corpus, case, steps, 10_000, 100)
}

/// Like [`rec`] but with explicit total-allocation scalars, so the tolerance-band
/// gate can be exercised at chosen `(alloc_bytes, alloc_count)` values.
fn rec_alloc(corpus: &str, case: &str, steps: u64, alloc_bytes: u64, alloc_count: u64) -> Value {
    Value::from(
        Object::new()
            .with("corpus", corpus)
            .with("case", case)
            .with("fragment", "existential")
            .with("world", "urn:gmeow:test:world")
            .with("golden_rows", 3)
            .with(
                "native",
                Value::from(
                    Object::new()
                        .with("engine", "authored-native")
                        .with("consumed_steps", steps)
                        .with("derived_count", 3)
                        .with("alloc_bytes", alloc_bytes)
                        .with("alloc_count", alloc_count)
                        .with("peak_live_bytes", 100)
                        .with("cost_vector", Value::Array(vec![])),
                ),
            )
            .with(
                "agreement",
                Value::from(
                    Object::new()
                        .with("native_vs_golden", true)
                        .with("native_vs_reference", true),
                ),
            ),
    )
}

/// Write a JSON artifact into a fresh cost-baseline temp directory.
///
/// The returned [`tempfile::TempDir`] OWNS the artifact: bind it to a live local
/// (`let (_tmp, base) = write_baseline_doc(&doc);`) for as long as the path is
/// read; dropping it removes the directory and the artifact, on success, on early
/// return, and on panic alike. The artifact keeps its `.json` name — the same
/// shape the committed cost baseline has on disk — and the enclosing directory
/// now carries the uniqueness the pid/nanos salt used to.
fn write_baseline_doc(document: &Value) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::Builder::new()
        .prefix("gmeow-cost-baseline-test-")
        .tempdir()
        .expect("create cost-baseline temp dir");
    let p = tmp.path().join("cost-baseline.json");
    std::fs::write(&p, compact_json(document)).unwrap();
    (tmp, p)
}

/// Write a minimal per-case cost baseline artifact into a fresh temp directory.
fn write_baseline(cases: Vec<Value>) -> (tempfile::TempDir, PathBuf) {
    write_baseline_doc(&Value::from(Object::new().with("cases", (cases).to_json())))
}

fn parallelism_record() -> Value {
    Value::from(
        Object::new()
            .with("fixture", ("balanced-six-rule-v1").to_json())
            .with("worker_count", (4).to_json())
            .with("rule_count", (6).to_json())
            .with("seed_rows", (24).to_json())
            .with("derived_rows", (120).to_json())
            .with("consumed_steps", (120).to_json())
            .with("parallel_rounds", (3).to_json())
            .with("rule_tasks", (18).to_json())
            .with("serial_candidate_rows", (144).to_json())
            .with("critical_path_candidate_rows", (48).to_json())
            .with("critical_path_rows_saved", (96).to_json())
            .with("max_buffered_candidate_rows", (96).to_json())
            .with("max_task_candidate_rows", (24).to_json())
            .with("budget_cases", (8).to_json())
            .with("output_parity", (true).to_json())
            .with("budget_parity", (true).to_json())
            .with("parallel_path_entered", (true).to_json())
            .with("critical_path_strictly_lower", (true).to_json())
            .with(
                "closure_blake3",
                ("6e7c8e8dfb2c3d6537ba91ce1ec2d0c22be9cb2d66d595213493506047eb54f3").to_json(),
            ),
    )
}

#[test]
fn cost_descriptor_is_stable_and_count_sensitive() {
    let a = rec("c", "x", 3);
    let b = rec("c", "x", 3);
    assert_eq!(
        cost_descriptor(&a),
        cost_descriptor(&b),
        "identical records must yield identical descriptors"
    );
    let changed = rec("c", "x", 4);
    assert_ne!(
        cost_descriptor(&a),
        cost_descriptor(&changed),
        "a changed consumed_steps count must change the descriptor"
    );
}

#[test]
fn cost_descriptor_excludes_the_nonreproducible_alloc_totals() {
    // Two records identical except for the total-allocation scalars must yield the
    // SAME exact descriptor — the totals gate through the tolerance band, not here, so
    // their run-to-run jitter can never flake the exact `::cost` comparison.
    let a = rec_alloc("c", "x", 3, 10_000, 100);
    let b = rec_alloc("c", "x", 3, 999_999, 9_999);
    assert_eq!(
        cost_descriptor(&a),
        cost_descriptor(&b),
        "alloc_bytes/alloc_count must NOT enter the exact descriptor"
    );
    // …but peak_live_bytes (deterministic) MUST still be in the descriptor.
    assert!(
        cost_descriptor(&a).contains("peak_live_bytes"),
        "the deterministic peak_live_bytes must remain in the exact descriptor"
    );
}

#[test]
fn provenance_descriptor_excludes_advisory_counts_but_keeps_exact_peak() {
    let provenance = || {
        Value::from(
            Object::new()
                .with(
                    "record",
                    Value::from(
                        Object::new()
                            .with("alloc_count", (100).to_json())
                            .with("peak_live_bytes", (900).to_json()),
                    ),
                )
                .with(
                    "skip",
                    Value::from(
                        Object::new()
                            .with("alloc_count", (80).to_json())
                            .with("peak_live_bytes", (700).to_json()),
                    ),
                )
                .with("record_alloc_count_overhead", (20).to_json())
                .with("record_peak_overhead_bytes", (200).to_json())
                .with("closure_parity", (true).to_json())
                .with("step_parity", (true).to_json()),
        )
    };
    let mut a = rec("c", "p", 3);
    a["native"]["provenance"] = provenance();
    let mut b = a.clone();
    b["native"]["provenance"]["record"]["alloc_count"] = (9_999).to_json();
    b["native"]["provenance"]["skip"]["alloc_count"] = (1).to_json();
    b["native"]["provenance"]["record_alloc_count_overhead"] = (9_998).to_json();
    assert_eq!(
        cost_descriptor(&a),
        cost_descriptor(&b),
        "per-mode allocation-count jitter must stay outside exact identity"
    );

    b["native"]["provenance"]["record"]["peak_live_bytes"] = (901).to_json();
    assert_ne!(
        cost_descriptor(&a),
        cost_descriptor(&b),
        "per-mode peak-live is deterministic and must remain exact"
    );
}

#[test]
fn incremental_grounding_descriptor_keeps_work_and_solver_honesty_exact() {
    let mut baseline = rec("c", "ground", 1);
    baseline["native"]["grounding"] = Value::from(
        Object::new()
            .with("ground_rule_probe_rows", (28).to_json())
            .with("ground_rule_changes", (1).to_json())
            .with("solver_status", ("flagged-non-incremental").to_json())
            .with("solver_reran", (true).to_json()),
    );

    let mut changed_work = baseline.clone();
    changed_work["native"]["grounding"]["ground_rule_probe_rows"] = (29).to_json();
    assert_ne!(
        cost_descriptor(&baseline),
        cost_descriptor(&changed_work),
        "grounding probe work is deterministic and must remain exact"
    );

    let mut overclaimed = baseline.clone();
    overclaimed["native"]["grounding"]["solver_status"] = ("incremental").to_json();
    assert_ne!(
        cost_descriptor(&baseline),
        cost_descriptor(&overclaimed),
        "the explicit non-incremental solver status is part of exact identity"
    );
}

#[test]
fn incremental_grounding_requires_both_changed_solver_shots_to_be_flagged() {
    assert!(changed_solver_shots_are_flagged(
        true,
        "flagged-non-incremental",
        true,
        "flagged-non-incremental",
    ));
    assert!(!changed_solver_shots_are_flagged(
        true,
        "flagged-non-incremental",
        true,
        "incremental",
    ));
    assert!(!changed_solver_shots_are_flagged(
        true,
        "flagged-non-incremental",
        false,
        "flagged-non-incremental",
    ));
}

#[test]
fn incremental_grounding_reference_agreement_is_semantic_only() {
    assert!(grounding_semantic_parity(true, true));
    assert!(!grounding_semantic_parity(true, false));
    assert!(!grounding_semantic_parity(false, true));
}

#[test]
fn alloc_band_passes_within_tolerance() {
    // Baseline alloc_count=10000/bytes=1_000_000; a fresh run 0.5% higher is inside
    // the 1% band → no regression (the sub-ε jitter the band is designed to absorb).
    let fresh = vec![rec_alloc("c", "x", 3, 1_005_000, 10_050)];
    let (_tmp, base) = write_baseline(vec![rec_alloc("c", "x", 3, 1_000_000, 10_000)]);
    let out = run_cost_regression_check(&base, &fresh);
    assert!(
        out.is_ok(),
        "a fresh alloc within the 1% band must not regress: {out:?}"
    );
}

#[test]
fn alloc_count_band_absorbs_measured_quantum_but_still_bites() {
    assert!(within_alloc_band(1_835, 1_793, ALLOC_COUNT_JITTER_FLOOR));
    assert_eq!(alloc_band_ceiling(1_793, ALLOC_COUNT_JITTER_FLOOR), 1_835);
    assert!(
        !within_alloc_band(1_836, 1_793, ALLOC_COUNT_JITTER_FLOOR),
        "one allocation beyond the measured 42-count span must fail"
    );
    assert_eq!(
        alloc_band_ceiling(1_000_000, ALLOC_COUNT_JITTER_FLOOR),
        1_010_000,
        "the 1% relative band dominates for large totals"
    );
}

#[test]
fn alloc_band_hard_fails_on_allocation_regression() {
    // FALSIFIABLE alloc gate: the committed baseline row records a SMALLER alloc_count
    // (10000) than the fresh run allocates (20000, +100% ≫ the 1% band) → the one-sided
    // tolerance band is breached → a blocking CorpusOnly cost-regression (hard fail).
    // Proves the alloc gate BITES, not just passes vacuously.
    let fresh = vec![rec_alloc("c", "x", 3, 2_000_000, 20_000)];
    let (_tmp, base) = write_baseline(vec![rec_alloc("c", "x", 3, 1_000_000, 10_000)]);
    let out = run_cost_regression_check(&base, &fresh);
    assert!(
        out.is_err(),
        "a fresh alloc_count far above the baseline band must be a cost regression"
    );
}

#[test]
fn alloc_band_hard_fails_when_baseline_lacks_alloc_fields() {
    // A stale baseline whose `native` object predates the alloc columns is a hard
    // error (regenerate hint), never a silent skip that would let alloc stop gating.
    let fresh = vec![rec_alloc("c", "x", 3, 1_000_000, 10_000)];
    let mut stale = rec_alloc("c", "x", 3, 1_000_000, 10_000);
    let native = stale.get_mut("native").unwrap().as_object_mut().unwrap();
    native.remove("alloc_bytes");
    native.remove("alloc_count");
    let (_tmp, base) = write_baseline(vec![stale]);
    let out = run_cost_regression_check(&base, &fresh);
    assert!(
        out.is_err(),
        "a baseline record missing the alloc columns must hard-fail, never skip the gate"
    );
}

#[test]
fn regression_check_passes_on_match() {
    let fresh = vec![rec("c", "x", 3)];
    let (_tmp, base) = write_baseline(vec![rec("c", "x", 3)]);
    let out = run_cost_regression_check(&base, &fresh);
    assert!(
        out.is_ok(),
        "an identical fresh run must not regress: {out:?}"
    );
}

#[test]
fn regression_check_hard_fails_on_count_divergence() {
    // Fresh run has consumed_steps=3; the committed baseline recorded 4 → a
    // deterministic-count divergence is a cost regression (hard fail).
    let fresh = vec![rec("c", "x", 3)];
    let (_tmp, base) = write_baseline(vec![rec("c", "x", 4)]);
    let out = run_cost_regression_check(&base, &fresh);
    assert!(
        out.is_err(),
        "a diverged deterministic count must be a cost regression"
    );
}

#[test]
fn soak_rejects_window_below_two() {
    // A window of 0 or 1 is a single-run tally, NOT soak evidence — the N-run
    // longitudinal reproducibility contract requires N >= 2. The guard runs before
    // any case is driven, so an empty corpus slice still surfaces the contract error.
    for n in [0usize, 1] {
        let out = run_soak(&[], n);
        assert!(
            out.is_err(),
            "a soak window of {n} must be rejected (< 2 is not a soak window)"
        );
    }
}

#[test]
fn regression_check_hard_fails_on_dropped_case() {
    // The baseline has a case the fresh run does not produce → divergence.
    let fresh = vec![rec("c", "x", 3)];
    let (_tmp, base) = write_baseline(vec![rec("c", "x", 3), rec("c", "y", 5)]);
    let out = run_cost_regression_check(&base, &fresh);
    assert!(
        out.is_err(),
        "a case present in the baseline but absent from the fresh run must regress"
    );
}

#[test]
fn rule_parallelism_regression_check_is_exact_and_bites() {
    let fresh = parallelism_record();
    let (_matching_tmp, matching) = write_baseline_doc(&Value::from(
        Object::new().with("rule_parallelism", (fresh.clone()).to_json()),
    ));
    let match_result = run_parallelism_regression_check(&matching, &fresh);
    assert!(
        match_result.is_ok(),
        "identical structural evidence must pass: {match_result:?}"
    );

    let mut drifted = fresh.clone();
    drifted["critical_path_candidate_rows"] = (49).to_json();
    let (_baseline_tmp, baseline) = write_baseline_doc(&Value::from(
        Object::new().with("rule_parallelism", (fresh.clone()).to_json()),
    ));
    let drift_result = run_parallelism_regression_check(&baseline, &drifted);
    assert!(
        drift_result.is_err(),
        "one changed structural count must produce a blocking divergence"
    );

    let (_absent_tmp, absent) = write_baseline_doc(&Value::from(
        Object::new().with("cases", Value::Array(vec![])),
    ));
    let absent_result = run_parallelism_regression_check(&absent, &fresh);
    assert!(
        absent_result.is_err(),
        "a baseline without the multi-worker record must hard-fail"
    );
}
