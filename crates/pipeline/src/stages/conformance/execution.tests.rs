// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn failed_observation_keeps_its_diagnostic_and_publishes_no_action() {
    let root = tempfile::tempdir().unwrap();
    let store = ActionStore::open(
        ActionStore::default_root(root.path()),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .unwrap();
    let calls = std::cell::Cell::new(0);
    // gmeow-test-input: synthetic-only; a controlled operational failure.
    let error = cached_inputs::<usize>(&store, "synthetic-failure", Vec::new(), || {
        calls.set(calls.get() + 1);
        Err(
            gmeow_errors::Diag::from(std::io::Error::other("selected input refused"))
                .with_context("prepare selected native input"),
        )
    })
    .unwrap_err();
    assert!(error.is::<std::io::Error>());
    assert_eq!(
        error.inner().context[0].label,
        "prepare selected native input"
    );
    assert_eq!(
        calls.get(),
        1,
        "a failed initializer is never retried automatically"
    );
    // gmeow-test-input: synthetic-only; a separate explicit admission attempt.
    let value = cached_inputs(&store, "synthetic-failure", Vec::new(), || {
        calls.set(calls.get() + 1);
        Ok(7usize)
    })
    .unwrap();
    assert_eq!(value, 7);
    assert_eq!(calls.get(), 2, "the failure published no reusable action");
}

#[test]
fn native_observation_reuse_binds_operation_and_input_without_rerunning_hits() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let input = root.join("synthetic.txt");
    std::fs::write(&input, "first").unwrap();
    let store = ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .unwrap();
    let calls = std::cell::Cell::new(0);
    let observe = |bytes: &[u8]| -> Result<String, gmeow_errors::Diag> {
        calls.set(calls.get() + 1);
        String::from_utf8(bytes.to_vec()).map_err(gmeow_errors::Diag::from)
    };
    // gmeow-test-input: synthetic-only; the action only reads this temporary string.
    assert_eq!(
        cached_observation(root, &input, &store, "first-operation", observe).unwrap(),
        "first"
    );
    // gmeow-test-input: synthetic-only; a hit must not execute its callback.
    assert_eq!(
        cached_observation::<String>(root, &input, &store, "first-operation", |_| panic!(
            "cache hit executed"
        ))
        .unwrap(),
        "first"
    );
    assert_eq!(calls.get(), 1);
    // gmeow-test-input: synthetic-only; a different selected operation is a miss.
    cached_observation(root, &input, &store, "other-operation", observe).unwrap();
    assert_eq!(calls.get(), 2);
    std::fs::write(&input, "second").unwrap();
    // gmeow-test-input: synthetic-only; changed execution bytes require recomputation.
    assert_eq!(
        cached_observation(root, &input, &store, "first-operation", observe).unwrap(),
        "second"
    );
    assert_eq!(calls.get(), 3);
}

#[test]
fn shared_observation_invalidates_each_authored_input_independently() {
    let directory = tempfile::tempdir().unwrap();
    let store = ActionStore::open(
        ActionStore::default_root(directory.path()),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .unwrap();
    let inputs = |logic: &str, wiring: &str| {
        vec![
            ActionInput::Raw {
                logical_path: "synthetic-logic".into(),
                file_kind: FileKind::File,
                executable: false,
                digest: bytes_digest(logic.as_bytes()),
            },
            ActionInput::Raw {
                logical_path: "synthetic-wiring".into(),
                file_kind: FileKind::File,
                executable: false,
                digest: bytes_digest(wiring.as_bytes()),
            },
        ]
    };
    let calls = std::cell::Cell::new(0);
    let observe = || -> Result<usize, gmeow_errors::Diag> {
        calls.set(calls.get() + 1);
        Ok(calls.get())
    };
    // gmeow-test-input: synthetic-only; keys bind two controlled strings.
    assert_eq!(
        cached_inputs(&store, "synthetic", inputs("a", "b"), observe).unwrap(),
        1
    );
    // gmeow-test-input: synthetic-only; unchanged inputs must never evaluate.
    assert_eq!(
        cached_inputs::<usize>(&store, "synthetic", inputs("a", "b"), || panic!(
            "hit executed"
        ))
        .unwrap(),
        1
    );
    // gmeow-test-input: synthetic-only; changing only the rule identity is a miss.
    assert_eq!(
        cached_inputs(&store, "synthetic", inputs("c", "b"), observe).unwrap(),
        2
    );
    // gmeow-test-input: synthetic-only; changing only the wiring identity is a miss.
    assert_eq!(
        cached_inputs(&store, "synthetic", inputs("a", "d"), observe).unwrap(),
        3
    );
}

#[test]
fn corpus_native_conformance_goldens() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let artifacts = crate::fixture::stage_artifacts(&root, 1, "stage-conformance")
        .expect("authenticated case observations");
    let index: BTreeSet<String> = serde_json::from_slice(&artifacts[INDEX]).expect("case index");
    let selected = selected_cases(&root).expect("required native inventory");
    assert_eq!(
        index,
        selected.iter().map(|case| case.case_id.clone()).collect()
    );
    assert!(
        index.contains("enactment/frontier-labels-are-derived"),
        "shipped-library witness remains required"
    );
    let mut failures = Vec::new();
    for case in selected {
        let bytes = &artifacts[&format!("{PREFIX}{}.json", case.case_id)];
        let observation: Observation = serde_json::from_slice(bytes).expect("case observation");
        assert_eq!(observation.case_id, case.case_id);
        match observation.outcome {
            Ok(outputs) => {
                assert_eq!(outputs.case_id, case.case_id);
                failures.extend(
                    gmeow_conformance::compare::diff_case(&case.case_dir, &outputs)
                        .into_iter()
                        .map(|diff| format!("{}: {diff}", case.case_id)),
                );
                // Preserve the actual shipped-library projection assertion,
                // independent of whether a case happens to carry this golden.
                if case.case_id == "enactment/frontier-labels-are-derived" {
                    assert!(outputs.projections.text["datalog"].contains("entryLabel"));
                }
            }
            Err(error) => failures.push(format!("{}: {error}", case.case_id)),
        }
    }
    assert!(
        failures.is_empty(),
        "{} conformance failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn case_actions_bind_execution_inputs_without_coupling_goldens_or_other_cases() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let dir = root.join(CASES).join("synthetic/selected");
    std::fs::create_dir_all(dir.join("expected")).unwrap();
    std::fs::write(dir.join("input.logic.ttl"), "<urn:a> <urn:p> <urn:b> .").unwrap();
    std::fs::write(dir.join("profile.json"), r#"{"mode":"native"}"#).unwrap();
    let case = discover::validate_case(&dir).unwrap();
    let key = context(root, &case, "library-first").unwrap().key();
    assert_eq!(key, context(root, &case, "library-second").unwrap().key());
    std::fs::write(dir.join("expected/verdicts.json"), "{}").unwrap();
    std::fs::write(root.join("unrelated.ttl"), "changed").unwrap();
    assert_eq!(key, context(root, &case, "library-first").unwrap().key());
    std::fs::write(dir.join("input.nq"), "<urn:a> <urn:p> <urn:c> <urn:w> .").unwrap();
    assert_ne!(key, context(root, &case, "library-first").unwrap().key());
    std::fs::write(
        dir.join("profile.json"),
        r#"{"mode":"native","shipped_rules":["urn:rule"]}"#,
    )
    .unwrap();
    let case = discover::validate_case(&dir).unwrap();
    assert_ne!(
        context(root, &case, "library-first").unwrap().key(),
        context(root, &case, "library-second").unwrap().key()
    );
}
