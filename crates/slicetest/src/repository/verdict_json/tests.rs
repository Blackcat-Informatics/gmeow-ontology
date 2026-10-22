// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::repository::{SliceSpecKind, SpecTask, outcome_from_entry, task_identity_from_entry};
use gmeow_action_cache::{
    ActionContext, ActionReceipt, BlobRef, ProducerIdentity, VerifiedEntry, bytes_digest,
};
use purrdf_lex::json::record::{from_slice, to_vec};
use std::{fmt::Debug, path::Path};

const IDENTITY: &str =
    r#"{"kind":"competency","path":"tiny/测试.ttl","action_key":"a","receipt_digest":"r"}"#;
const TASK: &str = r#"{"schema_version":1,"kind":"competency","path":"tiny/测试.ttl"}"#;
const VERDICT: &str = r#"{"schema_version":3,"input_files":7,"competency_files":1,"structural_files":0,"conformance_files":0,"flagship_manifests":2,"task_receipts":[{"kind":"competency","path":"tiny/测试.ttl","action_key":"a","receipt_digest":"r"}]}"#;

fn verdict() -> SliceSpecVerdict {
    from_slice(VERDICT.as_bytes()).unwrap()
}

// Inert metadata exercises the real product validators, not ActionStore admission.
// No producer, input discovery, worker or repository sweep is reachable here.
fn entry<T>(payload: T, bytes: Vec<u8>) -> VerifiedEntry<T> {
    let context = ActionContext::new(
        "authored",
        "verdict",
        ProducerIdentity::new("a".repeat(64)),
        "tiny",
        vec![],
    );
    let digest = bytes_digest(&bytes);
    VerifiedEntry {
        receipt: ActionReceipt {
            schema_version: gmeow_action_cache::RECEIPT_SCHEMA_VERSION,
            action_key: context.key(),
            context,
            product_digest: digest.clone(),
            product_blob: BlobRef {
                digest,
                bytes: u64::try_from(bytes.len()).unwrap(),
            },
            payload,
        },
        bytes,
    }
}

fn aggregate_entry(value: SliceSpecVerdict) -> VerifiedEntry<SliceSpecVerdict> {
    let bytes = to_vec(&value);
    entry(value, bytes)
}

#[test]
fn json_spec_complete_three_record_bytes_and_vector_order() {
    let identity: SliceSpecTaskIdentity = from_slice(IDENTITY.as_bytes()).unwrap();
    assert_eq!(to_vec(&identity), IDENTITY.as_bytes());
    let task: SpecTaskVerdict = from_slice(TASK.as_bytes()).unwrap();
    assert_eq!(to_vec(&task), TASK.as_bytes());
    let mut value = verdict();
    assert_eq!(to_vec(&value), VERDICT.as_bytes());
    let mut second = identity;
    second.path = "z/\"\\\n\t\u{0001}é😀".into();
    value.competency_files = 2;
    value.task_receipts.push(second);
    let expected = VERDICT.replace("\"competency_files\":1", "\"competency_files\":2")
        .replace("]}", ",{\"kind\":\"competency\",\"path\":\"z/\\\"\\\\\\n\\t\\u0001é😀\",\"action_key\":\"a\",\"receipt_digest\":\"r\"}]}");
    assert_eq!(to_vec(&value), expected.as_bytes());
    assert_eq!(
        from_slice::<SliceSpecVerdict>(expected.as_bytes()).unwrap(),
        value
    );
}

fn fields_are_strict<T: FromJson + ToJson + Debug>(literal: &str) {
    let value = from_slice::<T>(literal.as_bytes()).unwrap().to_json();
    for (field, _) in value.as_object().unwrap().members() {
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            T::from_json(&missing)
                .unwrap_err()
                .to_string()
                .contains(field)
        );
        let mut null = value.clone();
        null.as_object_mut().unwrap().insert(field, Value::Null);
        assert!(
            T::from_json(&null)
                .unwrap_err()
                .to_string()
                .contains(&format!("/{field}"))
        );
        let duplicate = format!(r#"{{"{field}":null,{}"#, &literal[1..]);
        assert!(
            from_slice::<T>(duplicate.as_bytes())
                .unwrap_err()
                .to_string()
                .contains("duplicate field")
        );
    }
    let unknown = format!(r#"{{"unknown":true,"unknown":false,{}"#, &literal[1..]);
    assert_eq!(
        to_vec(&from_slice::<T>(unknown.as_bytes()).unwrap()),
        literal.as_bytes()
    );
    for invalid in ["[]", "null", "true", "\"record\""] {
        assert!(from_slice::<T>(invalid.as_bytes()).is_err());
    }
    assert!(from_slice::<T>(format!("{literal} null").as_bytes()).is_err());
}

#[test]
fn json_spec_records_require_all_fields_and_refuse_known_duplicates() {
    fields_are_strict::<SliceSpecVerdict>(VERDICT);
    fields_are_strict::<SliceSpecTaskIdentity>(IDENTITY);
    fields_are_strict::<SpecTaskVerdict>(TASK);
}

#[test]
fn json_spec_unsigned_syntax_widths_and_nested_field_pointers() {
    for (field, old) in [
        ("schema_version", "3"),
        ("input_files", "7"),
        ("competency_files", "1"),
        ("structural_files", "0"),
        ("conformance_files", "0"),
        ("flagship_manifests", "2"),
    ] {
        for invalid in [
            "-0",
            "-1",
            "1.0",
            "1e0",
            "18446744073709551616",
            "true",
            "\"1\"",
        ] {
            let text = VERDICT.replace(
                &format!(r#""{field}":{old}"#),
                &format!(r#""{field}":{invalid}"#),
            );
            let error = from_slice::<SliceSpecVerdict>(text.as_bytes())
                .unwrap_err()
                .to_string();
            assert!(error.contains(&format!("/{field}")), "{error}");
        }
    }
    let text = VERDICT.replace("\"kind\":\"competency\"", "\"kind\":false");
    assert!(
        from_slice::<SliceSpecVerdict>(text.as_bytes())
            .unwrap_err()
            .to_string()
            .contains("/task_receipts/0/kind")
    );
    for invalid in ["-0", "1.0", "1e0", "4294967296"] {
        let text = TASK.replace(
            "\"schema_version\":1",
            &format!("\"schema_version\":{invalid}"),
        );
        assert!(
            from_slice::<SpecTaskVerdict>(text.as_bytes())
                .unwrap_err()
                .to_string()
                .contains("/schema_version")
        );
    }
    let mut value = verdict();
    value.schema_version = u32::MAX;
    value.input_files = usize::MAX;
    value.competency_files = usize::MAX;
    value.structural_files = usize::MAX;
    value.conformance_files = usize::MAX;
    value.flagship_manifests = usize::MAX;
    assert_eq!(
        from_slice::<SliceSpecVerdict>(&to_vec(&value)).unwrap(),
        value
    );
    if let Some(over) = (usize::MAX as u64).checked_add(1) {
        let text = VERDICT.replace("\"input_files\":7", &format!("\"input_files\":{over}"));
        assert!(from_slice::<SliceSpecVerdict>(text.as_bytes()).is_err());
    }
    assert!(
        from_slice::<SliceSpecVerdict>(
            VERDICT
                .replace("\"schema_version\":3", "\"schema_version\":4294967296")
                .as_bytes()
        )
        .is_err()
    );
}

#[test]
fn json_spec_actual_aggregate_replay_validates_payload_schema_census_and_digest() {
    let accepted = outcome_from_entry(aggregate_entry(verdict()), false, 7).unwrap();
    assert_eq!(accepted.verdict, verdict());
    assert!(!accepted.built);
    assert_eq!(accepted.action_key.len(), 64);
    assert_eq!(accepted.receipt_digest.len(), 64);

    let mut invalid = aggregate_entry(verdict());
    invalid.receipt.payload.input_files += 1;
    assert!(
        outcome_from_entry(invalid, false, 7)
            .unwrap_err()
            .to_string()
            .contains("payload differs")
    );
    let mut invalid = verdict();
    invalid.schema_version += 1;
    assert!(
        outcome_from_entry(aggregate_entry(invalid), false, 7)
            .unwrap_err()
            .to_string()
            .contains("schema")
    );
    assert!(
        outcome_from_entry(aggregate_entry(verdict()), false, 8)
            .unwrap_err()
            .to_string()
            .contains("input census")
    );
    let mut invalid = aggregate_entry(verdict());
    invalid.receipt.product_digest = "wrong".into();
    assert!(
        outcome_from_entry(invalid, false, 7)
            .unwrap_err()
            .to_string()
            .contains("product digest")
    );
    let mut invalid = aggregate_entry(verdict());
    invalid.bytes = b"{".to_vec();
    assert!(
        outcome_from_entry(invalid, false, 7)
            .unwrap_err()
            .to_string()
            .contains("decode slice-spec verdict product")
    );
}

#[test]
fn json_spec_actual_aggregate_replay_refuses_vacuity_count_and_duplicate_tasks() {
    let mut invalid = verdict();
    invalid.competency_files = 0;
    invalid.task_receipts.clear();
    assert!(
        outcome_from_entry(aggregate_entry(invalid), false, 7)
            .unwrap_err()
            .to_string()
            .contains("covers no")
    );
    let mut invalid = verdict();
    invalid.structural_files = 1;
    assert!(
        outcome_from_entry(aggregate_entry(invalid), false, 7)
            .unwrap_err()
            .to_string()
            .contains("task receipts for 2")
    );
    let mut invalid = verdict();
    invalid.competency_files = 2;
    invalid.task_receipts.push(invalid.task_receipts[0].clone());
    assert!(
        outcome_from_entry(aggregate_entry(invalid), false, 7)
            .unwrap_err()
            .to_string()
            .contains("duplicate task receipt identities")
    );
}

#[test]
fn json_spec_actual_aggregate_replay_refuses_both_census_overflows() {
    for (competency, structural, conformance) in [(usize::MAX, 1, 0), (usize::MAX - 1, 1, 1)] {
        let mut invalid = verdict();
        invalid.competency_files = competency;
        invalid.structural_files = structural;
        invalid.conformance_files = conformance;
        let error = outcome_from_entry(aggregate_entry(invalid), false, 7).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("declarative census overflows usize"),
            "{error}"
        );
    }
}

#[test]
fn json_spec_actual_task_replay_preserves_identity_and_refuses_conflicts() {
    let root = Path::new("/authored");
    let task = SpecTask {
        kind: SliceSpecKind::Competency,
        path: root.join("tiny/测试.ttl"),
    };
    let payload = || from_slice::<SpecTaskVerdict>(TASK.as_bytes()).unwrap();
    let valid_entry = entry(payload(), TASK.as_bytes().to_vec());
    let action_key = valid_entry.receipt.action_key.as_str().to_owned();
    let receipt_digest = valid_entry.receipt.digest();
    let actual = task_identity_from_entry(root, &task, valid_entry).unwrap();
    assert_eq!(
        actual,
        SliceSpecTaskIdentity {
            kind: "competency".into(),
            path: "tiny/测试.ttl".into(),
            action_key,
            receipt_digest
        }
    );
    for field in ["schema", "kind", "path"] {
        let mut changed = payload();
        match field {
            "schema" => changed.schema_version += 1,
            "kind" => changed.kind = "structural".into(),
            "path" => changed.path = "other.ttl".into(),
            _ => unreachable!(),
        }
        let bytes = to_vec(&changed);
        assert!(
            task_identity_from_entry(root, &task, entry(changed, bytes))
                .unwrap_err()
                .to_string()
                .contains("identity mismatch")
        );
    }
    let mut changed = entry(payload(), TASK.as_bytes().to_vec());
    changed.receipt.payload.path.push('x');
    assert!(
        task_identity_from_entry(root, &task, changed)
            .unwrap_err()
            .to_string()
            .contains("identity mismatch")
    );
    let mut changed = entry(payload(), TASK.as_bytes().to_vec());
    changed.receipt.product_digest = "wrong".into();
    assert!(
        task_identity_from_entry(root, &task, changed)
            .unwrap_err()
            .to_string()
            .contains("product digest mismatch")
    );
    let escaped = SpecTask {
        kind: SliceSpecKind::Competency,
        path: Path::new("/elsewhere/tiny.ttl").into(),
    };
    assert!(
        task_identity_from_entry(root, &escaped, entry(payload(), TASK.as_bytes().to_vec()))
            .unwrap_err()
            .to_string()
            .contains("escaped repository root")
    );
    assert!(
        task_identity_from_entry(root, &task, entry(payload(), b"{".to_vec()))
            .unwrap_err()
            .to_string()
            .contains("decode slice-spec task verdict")
    );
}
