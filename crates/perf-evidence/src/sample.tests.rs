// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn resource_delta_keeps_literal_legacy_report_bytes() {
    let before = Usage {
        user_us: 1_000,
        system_us: 4_000,
        max_rss_kib: 7,
        minor_faults: 3,
        major_faults: 1,
        input_blocks: 5,
        output_blocks: 7,
        voluntary_context_switches: 11,
        involuntary_context_switches: 13,
    };
    let after = Usage {
        user_us: 3_999,
        system_us: 5_999,
        max_rss_kib: 19,
        minor_faults: 8,
        major_faults: 1,
        input_blocks: 8,
        output_blocks: 9,
        voluntary_context_switches: 15,
        involuntary_context_switches: 18,
    };
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("usage.json");
    write_json_atomic(&path, &after.delta(before)).unwrap();
    assert_eq!(
        fs::read_to_string(path).unwrap(),
        concat!(
            "{\n",
            "  \"filesystem_input_blocks\": 3,\n",
            "  \"filesystem_output_blocks\": 2,\n",
            "  \"involuntary_context_switches\": 5,\n",
            "  \"major_faults\": 0,\n",
            "  \"max_rss_kib\": 19,\n",
            "  \"minor_faults\": 5,\n",
            "  \"pre_sample_children_max_rss_kib\": 7,\n",
            "  \"scope\": \"waited command process tree via getrusage(RUSAGE_CHILDREN)\",\n",
            "  \"system_cpu_ms\": 1,\n",
            "  \"user_cpu_ms\": 2,\n",
            "  \"voluntary_context_switches\": 4\n",
            "}\n",
        )
    );
}

#[test]
fn dependency_identity_reads_authored_metadata_without_invoking_cargo() {
    let first = br#"{"packages":[{"name":"b","version":"2","source":"registry+x","checksum":"y"},{"name":"local","version":"1","source":null},{"name":"a","version":"1","source":"git+x"}]}"#;
    let reordered = br#"{"packages":[{"source":"git+x","version":"1","name":"a"},{"name":"b","version":"2","source":"registry+x","checksum":"y"},{"name":"a","version":"1","source":"git+x"}]}"#;
    assert_eq!(
        dependency_resolution_digest_from(first).unwrap(),
        dependency_resolution_digest_from(reordered).unwrap()
    );
    let changed = br#"{"packages":[{"name":"a","version":"2","source":"git+x"}]}"#;
    assert_ne!(
        dependency_resolution_digest_from(first).unwrap(),
        dependency_resolution_digest_from(changed).unwrap()
    );
    assert!(dependency_resolution_digest_from(br#"{"packages":null}"#).is_err());
    assert!(dependency_resolution_digest_from(b"not JSON").is_err());
}

#[cfg(unix)]
#[test]
fn census_refuses_non_utf8_paths_instead_of_replacing_bytes() {
    use std::os::unix::ffi::OsStringExt as _;
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join(OsString::from_vec(vec![b'a', 0xff])),
        b"x",
    )
    .unwrap();
    assert!(
        tree_census(directory.path())
            .unwrap_err()
            .to_string()
            .contains("non-UTF-8 tree path")
    );
}

fn base_args(cache_state: &str) -> Vec<OsString> {
    [
        "--pair-id",
        "pair",
        "--variant",
        "candidate",
        "--sample-index",
        "1",
        "--cache-state",
        cache_state,
        "--cargo-cache-state",
        cache_state,
        "--sync-cache-state",
        cache_state,
        "--pipeline-cache-state",
        cache_state,
        "--fixture-cache-state",
        cache_state,
        "--bundle-import-cache-state",
        cache_state,
        "--nextest-archive-cache-state",
        cache_state,
        "--node-class",
        "node",
        "--output",
        "sample.json",
    ]
    .into_iter()
    .map(OsString::from)
    .collect()
}

#[test]
fn partial_samples_require_a_named_path_and_digest() {
    let mut missing = base_args("partial");
    missing.extend([OsString::from("--"), OsString::from("true")]);
    assert!(parse_args_from(missing).is_err());

    let mut complete = base_args("partial");
    complete.extend([
        OsString::from("--partial-change"),
        OsString::from(format!("irrelevant:slices/example.ttl:{}", "a".repeat(64))),
        OsString::from("--output-root"),
        OsString::from("archive=dist/nextest"),
        OsString::from("--"),
        OsString::from("true"),
    ]);
    let parsed = parse_args_from(complete).unwrap();
    assert_eq!(parsed.cache_state, "partial");
    assert_eq!(parsed.cache_classes["pipeline"], "partial");
    assert_eq!(
        parsed.output_roots,
        vec![("archive".to_string(), PathBuf::from("dist/nextest"))]
    );
}

#[test]
fn the_six_cache_classes_are_not_collapsed_into_one_label() {
    let mut args = base_args("warm");
    let sync_option = args
        .iter()
        .position(|arg| arg == "--sync-cache-state")
        .unwrap();
    args.drain(sync_option..=sync_option + 1);
    args.extend([OsString::from("--"), OsString::from("true")]);
    let error = parse_args_from(args).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("missing required --sync-cache-state"),
        "{error}"
    );
}

#[test]
fn tree_census_binds_paths_bytes_and_content() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("nested")).unwrap();
    fs::write(directory.path().join("b"), b"two").unwrap();
    fs::write(directory.path().join("nested/a"), b"one").unwrap();
    let first = tree_census(directory.path()).unwrap();
    fs::write(directory.path().join("nested/a"), b"ONE").unwrap();
    let second = tree_census(directory.path()).unwrap();
    assert_eq!(first["file_count"], 2);
    assert_eq!(first["bytes"], 6);
    assert_ne!(first["sha256"], second["sha256"]);
}

#[test]
fn producer_receipt_identity_precedes_the_local_sync_manifest() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("dist")).unwrap();
    fs::create_dir_all(root.path().join(".cache/gmeow-sync/manifests")).unwrap();
    fs::write(
        root.path().join("dist/producer-receipt.json"),
        json::write_compact(&Value::from(Object::new().with(
            "payload",
            Value::from(Object::new().with(
                "build_identity",
                Value::from(Object::new().with("fingerprint", ("a".repeat(64)).to_json())),
            )),
        ))),
    )
    .unwrap();
    fs::write(
        root.path()
            .join(".cache/gmeow-sync/manifests/generated-default.json"),
        json::write_compact(&Value::from(
            Object::new().with("build_fingerprint", ("b".repeat(64)).to_json()),
        )),
    )
    .unwrap();
    assert_eq!(
        pipeline_build_fingerprint(root.path(), &[]).unwrap(),
        "a".repeat(64)
    );
}

#[test]
fn explicit_legacy_producer_manifest_supplies_cold_identity_outside_the_cache() {
    let root = tempfile::tempdir().unwrap();
    let authority = root.path().join("dist/baseline-generation-a.json");
    fs::create_dir_all(authority.parent().unwrap()).unwrap();
    fs::write(
        &authority,
        json::write_compact(&Value::from(
            Object::new()
                .with("version", 3.to_json())
                .with("build_fingerprint", ("c".repeat(64)).to_json()),
        )),
    )
    .unwrap();

    assert_eq!(
        pipeline_build_fingerprint(
            root.path(),
            &[(
                "producer".to_string(),
                PathBuf::from("dist/baseline-generation-a.json")
            )]
        )
        .unwrap(),
        "c".repeat(64)
    );
    assert!(
        !root
            .path()
            .join(".cache/gmeow-sync/manifests/generated-default.json")
            .exists(),
        "producer identity must not warm the live sync-manifest cache"
    );
}

#[test]
fn stage_execution_counter_normalizes_legacy_and_current_sync_telemetry() {
    let legacy = Value::from(
        Object::new().with(
            "payload",
            Value::from(
                Object::new().with(
                    "commands",
                    Value::array([Value::from(
                        Object::new()
                            .with("command", "sync".to_json())
                            .with("pipeline_runs", 1.to_json())
                            .with(
                                "timings",
                                Value::array([
                                    Value::from(
                                        Object::new()
                                            .with("phase", "stage:stage-source-load".to_json()),
                                    ),
                                    Value::from(
                                        Object::new().with(
                                            "phase",
                                            "stage:stage-source-load/parse".to_json(),
                                        ),
                                    ),
                                    Value::from(
                                        Object::new().with("phase", "stage:stage-reason".to_json()),
                                    ),
                                    Value::from(
                                        Object::new().with("phase", "pipeline-total".to_json()),
                                    ),
                                ]),
                            ),
                    )]),
                ),
            ),
        ),
    );
    let current = Value::from(
        Object::new().with(
            "payload",
            Value::from(
                Object::new().with(
                    "commands",
                    Value::array([Value::from(
                        Object::new().with("command", "sync".to_json()).with(
                            "observations",
                            Value::from(
                                Object::new()
                                    .with("pipeline_runs", 1.to_json())
                                    .with("executed_stage_count", 1.to_json()),
                            ),
                        ),
                    )]),
                ),
            ),
        ),
    );
    let fixed_point = Value::from(
        Object::new().with(
            "payload",
            Value::from(
                Object::new().with(
                    "commands",
                    Value::array([Value::from(
                        Object::new()
                            .with("command", "sync".to_json())
                            .with("pipeline_runs", 0.to_json()),
                    )]),
                ),
            ),
        ),
    );

    assert_eq!(
        causal_counters(Some(&legacy))["pipeline_stage_executions"],
        2
    );
    assert_eq!(
        causal_counters(Some(&current))["pipeline_stage_executions"],
        1
    );
    assert_eq!(
        causal_counters(Some(&fixed_point))["pipeline_stage_executions"],
        0
    );
}

#[test]
fn producer_identity_is_required_and_structurally_validated() {
    let root = tempfile::tempdir().unwrap();
    assert!(pipeline_build_fingerprint(root.path(), &[]).is_err());
    fs::create_dir_all(root.path().join(".cache/gmeow-sync/manifests")).unwrap();
    fs::write(
        root.path()
            .join(".cache/gmeow-sync/manifests/generated-default.json"),
        br#"{"build_fingerprint":"NOT-A-DIGEST"}"#,
    )
    .unwrap();
    assert!(pipeline_build_fingerprint(root.path(), &[]).is_err());
}
