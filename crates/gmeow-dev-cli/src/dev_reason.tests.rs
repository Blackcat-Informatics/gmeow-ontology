// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::cell::Cell;

use super::*;

#[test]
fn reason_verify_orchestration_invokes_the_result_producer_once() {
    let dataset = purrdf::RdfDataset::union(&[]).unwrap();
    let calls = Cell::new(0usize);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let gates = gmeow_pipeline::fixture::authenticated_reasoned_gates(&root)
        .expect("producer-selected native law preparation");
    let verification = PreparedVerification::new(&[], &gates).expect("empty selected query set");
    let evaluation = evaluate_reason_verify_once(&dataset, &verification, || {
        calls.set(calls.get() + 1);
        reason_all(
            gmeow_logic::reason::prepare_reasoning_input(&dataset)?,
            &gmeow_logic::reasoning_graphs::object_level_domains()?,
        )
    })
    .expect("empty dataset reasons and verifies");

    assert_eq!(
        calls.get(),
        1,
        "the complete closure is produced exactly once"
    );
    assert!(evaluation.result.is_decided_consistent());
    assert!(evaluation.report.ok());
}

/// Profile admission uses only the explicitly selected sibling JSON and never reasons.
#[test]
fn certify_profile_admission_preserves_precedence_and_metadata_boundaries() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.logic.ttl");
    let sibling = directory.path().join("profile.json");
    assert_eq!(
        resolve_profile(&input, None).unwrap(),
        "PositiveHornProfile"
    );
    for json in [r#"{"unowned":{"metadata":true}}"#, "[]", "null"] {
        std::fs::write(&sibling, json).unwrap();
        assert_eq!(
            resolve_profile(&input, None).unwrap(),
            "PositiveHornProfile"
        );
    }
    std::fs::write(
        &sibling,
        r#"{
        "reasoning_contract":null,
        "reasoning_contract":{"preset":false,"preset":"authored é中😀","unowned":17},
        "unowned":{"metadata":true}
    }"#,
    )
    .unwrap();
    assert_eq!(resolve_profile(&input, None).unwrap(), "authored é中😀");
    assert_eq!(
        resolve_profile(&input, Some("ExplicitProfile")).unwrap(),
        "ExplicitProfile"
    );
    for json in [
        r#"{"reasoning_contract":null}"#,
        r#"{"reasoning_contract":[]}"#,
        r#"{"reasoning_contract":17}"#,
        r#"{"reasoning_contract":{}}"#,
        r#"{"reasoning_contract":{"preset":null}}"#,
        r#"{"reasoning_contract":{"preset":false}}"#,
        "not JSON",
    ] {
        std::fs::write(&sibling, json).unwrap();
        assert_eq!(resolve_profile(&input, None), Err(1), "{json}");
        assert_eq!(resolve_profile(&input, Some("")).unwrap(), "");
    }
    std::fs::write(&sibling, [0xff]).unwrap();
    assert_eq!(resolve_profile(&input, None), Err(1));
    assert_eq!(
        resolve_profile(&input, Some("OverrideUnreadable")).unwrap(),
        "OverrideUnreadable"
    );
}
