// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const AUTHORED_SAMPLE: &str = r#"{
  "schema_version":2,"command":"perf-sample",
  "sample_identity":{"pair_id":"p","variant":"baseline","sample_index":1,
    "node_class":"n","cache_state":"cold","cache_classes":{
      "cargo":"cold","bundle_import":"absent","fixture":"absent",
      "nextest_archive":"absent","pipeline":"absent","sync_manifest":"absent"}},
  "observations":{"exit":{"success":true},"wall_ms":2},
  "deterministic_work":{"count":1}
}"#;

fn authored_sample(variant: &str) -> Sample {
    let path = PathBuf::from(format!("{variant}.json"));
    let mut sample = decode_sample(&path, AUTHORED_SAMPLE.as_bytes(), &[]).unwrap();
    sample.variant = variant.to_owned();
    sample
}

#[test]
fn sample_decoder_preserves_last_wins_and_number_lexemes() {
    let text = AUTHORED_SAMPLE
        .replace(
            "\"schema_version\":2",
            "\"schema_version\":99,\"schema_version\":2",
        )
        .replace("\"wall_ms\":2", "\"wall_ms\":1.2300e+2");
    let sample = decode_sample(Path::new("input.json"), text.as_bytes(), &[]).unwrap();
    assert_eq!(sample.wall_ms, 123.0);
    assert_eq!(
        sample
            .value
            .pointer("/observations/wall_ms")
            .unwrap()
            .as_number()
            .unwrap()
            .lexeme(),
        "1.2300e+2"
    );
    assert_eq!(sample.value["schema_version"].as_u64(), Some(2));
}

#[test]
fn sample_admission_keeps_integer_selectors_and_finite_nonnegative_observations() {
    for replacement in ["2.0", "2e0", "-2", "18446744073709551616"] {
        let text = AUTHORED_SAMPLE.replace(
            "\"schema_version\":2",
            &format!("\"schema_version\":{replacement}"),
        );
        assert!(
            decode_sample(Path::new("input.json"), text.as_bytes(), &[]).is_err(),
            "{replacement}"
        );
    }
    for replacement in ["1.0", "1e0", "-0", "-1", "18446744073709551616"] {
        let text = AUTHORED_SAMPLE.replace(
            "\"sample_index\":1",
            &format!("\"sample_index\":{replacement}"),
        );
        assert!(
            decode_sample(Path::new("input.json"), text.as_bytes(), &[]).is_err(),
            "{replacement}"
        );
    }
    let text = AUTHORED_SAMPLE.replace(
        "\"sample_index\":1",
        "\"sample_index\":18446744073709551615",
    );
    assert_eq!(
        decode_sample(Path::new("input.json"), text.as_bytes(), &[])
            .unwrap()
            .sample_index,
        u64::MAX
    );
    for replacement in ["0", "-0", "-0.0", "-1", "1e400", "\"2\"", "null"] {
        let text = AUTHORED_SAMPLE.replace("\"wall_ms\":2", &format!("\"wall_ms\":{replacement}"));
        assert!(
            decode_sample(Path::new("input.json"), text.as_bytes(), &[]).is_err(),
            "{replacement}"
        );
    }
    let counters = [("count".to_owned(), "/deterministic_work/count".to_owned())];
    for replacement in ["-0", "-1", "1e400", "null"] {
        let text = AUTHORED_SAMPLE.replace("\"count\":1", &format!("\"count\":{replacement}"));
        assert!(
            decode_sample(Path::new("input.json"), text.as_bytes(), &counters).is_err(),
            "{replacement}"
        );
    }
    let failed = AUTHORED_SAMPLE.replace("\"success\":true", "\"success\":false");
    assert!(
        decode_sample(Path::new("input.json"), failed.as_bytes(), &[])
            .unwrap_err()
            .to_string()
            .contains("failed command")
    );
}

#[test]
fn paired_identity_uses_exact_decimal_values_and_rfc6901_pointers() {
    let mut pair = Pair {
        baseline: authored_sample("baseline"),
        candidate: authored_sample("candidate"),
    };
    for (baseline, candidate) in [
        ("1", "1.0"),
        ("1", "1e0"),
        ("0", "-0"),
        (r#"{"a":1,"b":2}"#, r#"{"b":2.0,"a":1e0}"#),
    ] {
        pair.baseline.value["identity"] = json::record::read_document(baseline.as_bytes()).unwrap();
        pair.candidate.value["identity"] =
            json::record::read_document(candidate.as_bytes()).unwrap();
        assert!(equal_pointer(&pair, "/identity", "identity").is_ok());
    }
    for (baseline, candidate) in [
        ("9007199254740992", "9007199254740993"),
        ("1", "\"1\""),
        ("[1,2]", "[2,1]"),
        ("null", "false"),
    ] {
        pair.baseline.value["identity"] = json::record::read_document(baseline.as_bytes()).unwrap();
        pair.candidate.value["identity"] =
            json::record::read_document(candidate.as_bytes()).unwrap();
        assert!(equal_pointer(&pair, "/identity", "identity").is_err());
    }
    pair.baseline.value["a/b"] = Value::from(Object::new().with("~", 7));
    pair.candidate.value["a/b"] = Value::from(Object::new().with("~", 7));
    assert!(equal_pointer(&pair, "/a~1b/~0", "escaped").is_ok());
    assert!(equal_pointer(&pair, "/missing", "missing").is_err());
}

#[test]
fn pair_report_keeps_literal_legacy_bytes() {
    let mut pair = Pair {
        baseline: authored_sample("baseline"),
        candidate: authored_sample("candidate"),
    };
    pair.candidate.wall_ms = 1.0;
    pair.baseline.work.insert("rows".to_owned(), 4.0);
    pair.candidate.work.insert("rows".to_owned(), 2.0);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("pair.json");
    write_json_atomic(&path, &pair_json(&pair).unwrap()).unwrap();
    assert_eq!(
        fs::read_to_string(path).unwrap(),
        concat!(
            "{\n  \"baseline\": {\n    \"path\": \"baseline.json\",\n    \"wall_ms\": 2.0,\n    \"work\": {\n      \"rows\": 4.0\n    }\n  },\n",
            "  \"candidate\": {\n    \"path\": \"candidate.json\",\n    \"wall_ms\": 1.0,\n    \"work\": {\n      \"rows\": 2.0\n    }\n  },\n",
            "  \"pair_id\": \"p\",\n  \"sample_index\": 1,\n  \"speedup\": 2.0\n}\n",
        )
    );
}

#[cfg(unix)]
#[test]
fn pair_report_refuses_non_utf8_paths_without_lossy_substitution() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt as _;
    let mut pair = Pair {
        baseline: authored_sample("baseline"),
        candidate: authored_sample("candidate"),
    };
    pair.candidate.path = PathBuf::from(OsString::from_vec(vec![b'x', 0xff]));
    assert!(
        pair_json(&pair)
            .unwrap_err()
            .to_string()
            .contains("non-UTF-8 sample path")
    );
}

#[test]
fn timing_acceptance_has_no_implicit_speedup_or_regression_target() {
    assert!(timing_failures(0.5, 100.0, None, None).is_empty());
    assert!(timing_failures(1.2, 8.0, Some(1.1), Some(10.0)).is_empty());
    assert_eq!(timing_failures(1.2, 8.0, Some(1.3), Some(5.0)).len(), 2);
}

#[test]
fn median_handles_odd_and_even_complete_pair_counts() {
    assert_eq!(median(vec![9.0, 1.0, 5.0]), 5.0);
    assert_eq!(median(vec![9.0, 1.0, 5.0, 3.0]), 4.0);
}

#[test]
fn missing_or_changed_pair_identity_fails_closed() {
    let value = Value::from(
        Object::new().with(
            "sample_identity",
            Value::from(
                Object::new()
                    .with(
                        "cache_classes",
                        Value::from(
                            Object::new()
                                .with("bundle_import", "absent".to_json())
                                .with("cargo", "cold".to_json())
                                .with("fixture", "absent".to_json())
                                .with("nextest_archive", "absent".to_json())
                                .with("pipeline", "cold".to_json())
                                .with("sync_manifest", "cold".to_json()),
                        ),
                    )
                    .with("partial_change", Value::Null)
                    .with(
                        "measured_command",
                        Value::array(["make".to_json(), "check".to_json()]),
                    )
                    .with("dependency_resolution_sha256", "a".to_json())
                    .with("rustc_vv", "rustc".to_json())
                    .with("cargo_version", "cargo".to_json())
                    .with("nextest_version", "nextest".to_json())
                    .with("build_environment", Value::from(Object::new()))
                    .with("runner_image", "image".to_json())
                    .with(
                        "generated_tree",
                        Value::from(Object::new().with("sha256", "product".to_json())),
                    ),
            ),
        ),
    );
    let sample = |variant: &str, value: Value| Sample {
        path: PathBuf::from(format!("{variant}.json")),
        value,
        command: "perf-sample".to_string(),
        pair_id: "pair".to_string(),
        variant: variant.to_string(),
        sample_index: 1,
        node_class: "node".to_string(),
        cache_state: "cold".to_string(),
        wall_ms: 1.0,
        work: BTreeMap::new(),
    };
    let baseline = sample("baseline", value.clone());
    let mut changed = value;
    changed["sample_identity"]["runner_image"] = "other".to_json();
    let candidate = sample("candidate", changed);
    let pair = Pair {
        baseline,
        candidate,
    };
    assert!(
        verify_pair(
            &pair,
            &[(
                "generated".to_string(),
                "/sample_identity/generated_tree/sha256".to_string()
            )],
            &[],
        )
        .is_err()
    );
}

#[test]
fn cache_protocol_requires_exact_classes_and_a_protocol_witness() {
    let sample = |classes: Value| {
        Value::from(Object::new().with(
            "sample_identity",
            Value::from(Object::new().with("cache_classes", classes.to_json())),
        ))
    };
    let complete = sample(Value::from(
        Object::new()
            .with("bundle_import", "absent".to_json())
            .with("cargo", "warm".to_json())
            .with("fixture", "not-applicable".to_json())
            .with("nextest_archive", "absent".to_json())
            .with("pipeline", "partial".to_json())
            .with("sync_manifest", "warm".to_json()),
    ));
    assert!(validate_cache_protocol(&complete, "partial", Path::new("sample.json")).is_ok());
    assert!(validate_cache_protocol(&complete, "warm", Path::new("sample.json")).is_err());

    let missing = sample(Value::from(
        Object::new()
            .with("cargo", "cold".to_json())
            .with("fixture", "absent".to_json())
            .with("nextest_archive", "absent".to_json())
            .with("pipeline", "cold".to_json())
            .with("sync_manifest", "cold".to_json()),
    ));
    assert!(validate_cache_protocol(&missing, "cold", Path::new("sample.json")).is_err());

    let no_witness = sample(Value::from(
        Object::new()
            .with("bundle_import", "absent".to_json())
            .with("cargo", "absent".to_json())
            .with("fixture", "not-applicable".to_json())
            .with("nextest_archive", "absent".to_json())
            .with("pipeline", "absent".to_json())
            .with("sync_manifest", "not-applicable".to_json()),
    ));
    assert!(validate_cache_protocol(&no_witness, "cold", Path::new("sample.json")).is_err());
}

#[test]
fn paired_cache_protocol_allows_new_mechanisms_but_not_shared_state_drift() {
    let sample = |variant: &str, pipeline: &str, bundle_import: &str| Sample {
        path: PathBuf::from(format!("{variant}.json")),
        value: Value::from(
            Object::new().with(
                "sample_identity",
                Value::from(
                    Object::new().with(
                        "cache_classes",
                        Value::from(
                            Object::new()
                                .with("bundle_import", bundle_import.to_json())
                                .with("cargo", "warm".to_json())
                                .with("fixture", "absent".to_json())
                                .with("nextest_archive", "absent".to_json())
                                .with("pipeline", pipeline.to_json())
                                .with("sync_manifest", "warm".to_json()),
                        ),
                    ),
                ),
            ),
        ),
        command: "perf-sample".to_string(),
        pair_id: "pair".to_string(),
        variant: variant.to_string(),
        sample_index: 1,
        node_class: "node".to_string(),
        cache_state: "warm".to_string(),
        wall_ms: 1.0,
        work: BTreeMap::new(),
    };
    let new_mechanism = Pair {
        baseline: sample("baseline", "warm", "absent"),
        candidate: sample("candidate", "warm", "warm"),
    };
    assert!(verify_paired_cache_classes(&new_mechanism).is_ok());

    let shared_drift = Pair {
        baseline: sample("baseline", "warm", "absent"),
        candidate: sample("candidate", "partial", "warm"),
    };
    assert!(verify_paired_cache_classes(&shared_drift).is_err());
}

#[test]
fn proof_inventory_allows_additions_but_rejects_drops_and_duplicates() {
    let sample = |variant: &str, inventory: Value| Sample {
        path: PathBuf::from(format!("{variant}.json")),
        value: Value::from(Object::new().with("proofs", inventory.to_json())),
        command: "ci-run-receipt".to_string(),
        pair_id: "pair".to_string(),
        variant: variant.to_string(),
        sample_index: 1,
        node_class: "node".to_string(),
        cache_state: "cold".to_string(),
        wall_ms: 1.0,
        work: BTreeMap::new(),
    };
    let pair = Pair {
        baseline: sample("baseline", Value::array(["a".to_json(), "b".to_json()])),
        candidate: sample(
            "candidate",
            Value::array(["a".to_json(), "b".to_json(), "c".to_json()]),
        ),
    };
    assert!(baseline_is_subset(&pair, "/proofs", "required").is_ok());

    let dropped = Pair {
        baseline: sample("baseline", Value::array(["a".to_json(), "b".to_json()])),
        candidate: sample("candidate", Value::array(["a".to_json()])),
    };
    assert!(baseline_is_subset(&dropped, "/proofs", "required").is_err());

    let duplicate = Pair {
        baseline: sample("baseline", Value::array(["a".to_json()])),
        candidate: sample("candidate", Value::array(["a".to_json(), "a".to_json()])),
    };
    assert!(baseline_is_subset(&duplicate, "/proofs", "required").is_err());
}
