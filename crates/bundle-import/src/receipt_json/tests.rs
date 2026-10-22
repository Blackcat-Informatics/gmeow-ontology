// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::{
    BUILD_FINGERPRINT, CODEC, MAX_PACK_BYTES, SCHEMA_VERSION, load_import_receipt, prune_namespace,
};
use purrdf_lex::json::{
    self,
    record::{from_slice, to_vec},
};
use std::{fs, path::Path};

const RECEIPT: &str = r#"{"schema_version":1,"action_key":"a","build_fingerprint":"b","codec":"c","source_digest":"s","source_bytes":3,"pack_digest":"p","pack_bytes":4,"dataset_quads":5,"named_graphs":6}"#;
const PRETTY: &str = r#"{
  "receipt_digest": "d",
  "receipt": {
    "schema_version": 1,
    "action_key": "a",
    "build_fingerprint": "b",
    "codec": "c",
    "source_digest": "s",
    "source_bytes": 3,
    "pack_digest": "p",
    "pack_bytes": 4,
    "dataset_quads": 5,
    "named_graphs": 6
  }
}"#;

fn sample() -> ImportReceipt {
    from_slice(RECEIPT.as_bytes()).unwrap()
}

#[test]
fn json_receipt_complete_bytes_digest_and_pretty_envelope() {
    let receipt = sample();
    assert_eq!(to_vec(&receipt), RECEIPT.as_bytes());
    assert_eq!(
        receipt.receipt_digest(),
        gmeow_action_cache::content_digest(&[
            b"gmeow:bundle-import-receipt:v1",
            RECEIPT.as_bytes()
        ])
    );
    let envelope = ReceiptEnvelope {
        receipt_digest: "d".into(),
        receipt,
    };
    assert_eq!(json::write_pretty(&envelope.to_json()), PRETTY);
    let decoded: ReceiptEnvelope = from_slice(PRETTY.as_bytes()).unwrap();
    assert_eq!(decoded.receipt_digest, "d");
    assert_eq!(decoded.receipt, sample());
    assert_eq!(
        to_vec(&decoded),
        format!(r#"{{"receipt_digest":"d","receipt":{RECEIPT}}}"#).as_bytes()
    );
}

#[test]
fn json_receipt_literal_escaping_and_unsigned_boundaries() {
    let mut receipt = sample();
    receipt.action_key = "é中😀\u{2028}\u{2029}/\"\\\n\t\u{0001}".into();
    receipt.schema_version = u32::MAX;
    receipt.source_bytes = u64::MAX;
    receipt.pack_bytes = u64::MAX;
    receipt.dataset_quads = u64::MAX;
    receipt.named_graphs = u64::MAX;
    let expected = concat!(
        "{\"schema_version\":4294967295,\"action_key\":\"é中😀\u{2028}\u{2029}/\\\"\\\\\\n\\t\\u0001\",",
        "\"build_fingerprint\":\"b\",\"codec\":\"c\",\"source_digest\":\"s\",\"source_bytes\":18446744073709551615,",
        "\"pack_digest\":\"p\",\"pack_bytes\":18446744073709551615,\"dataset_quads\":18446744073709551615,\"named_graphs\":18446744073709551615}"
    );
    assert_eq!(to_vec(&receipt), expected.as_bytes());
    assert_eq!(
        from_slice::<ImportReceipt>(expected.as_bytes()).unwrap(),
        receipt
    );
}

#[test]
fn json_receipt_all_fields_required_typed_and_known_duplicates_refused() {
    let value = sample().to_json();
    for (name, _) in value.as_object().unwrap().members() {
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(name);
        assert!(
            ImportReceipt::from_json(&missing)
                .unwrap_err()
                .to_string()
                .contains(name)
        );
        let mut null = value.clone();
        null.as_object_mut().unwrap().insert(name, Value::Null);
        assert!(
            ImportReceipt::from_json(&null)
                .unwrap_err()
                .to_string()
                .contains(&format!("/{name}"))
        );
        let duplicate = format!(r#"{{"{name}":null,{}"#, &RECEIPT[1..]);
        assert!(
            from_slice::<ImportReceipt>(duplicate.as_bytes())
                .unwrap_err()
                .to_string()
                .contains("duplicate field")
        );
    }
    for text in [
        "[]",
        "null",
        r#"{"receipt_digest":"d"}"#,
        r#"{"receipt":null}"#,
        r#"{"receipt_digest":"d","receipt_digest":"d","receipt":{}}"#,
    ] {
        assert!(
            from_slice::<ReceiptEnvelope>(text.as_bytes()).is_err(),
            "{text}"
        );
    }
    let unknown = format!(r#"{{"extra":false,"extra":true,{}"#, &RECEIPT[1..]);
    assert_eq!(
        from_slice::<ImportReceipt>(unknown.as_bytes()).unwrap(),
        sample()
    );
    let envelope = format!(r#"{{"extra":[],"receipt_digest":"d","receipt":{RECEIPT}}}"#);
    assert_eq!(
        from_slice::<ReceiptEnvelope>(envelope.as_bytes())
            .unwrap()
            .receipt,
        sample()
    );
    let envelope = ReceiptEnvelope {
        receipt_digest: "d".into(),
        receipt: sample(),
    }
    .to_json();
    for field in ["receipt_digest", "receipt"] {
        let mut missing = envelope.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            ReceiptEnvelope::from_json(&missing)
                .unwrap_err()
                .to_string()
                .contains(field)
        );
        let mut null = envelope.clone();
        null.as_object_mut().unwrap().insert(field, Value::Null);
        assert!(
            ReceiptEnvelope::from_json(&null)
                .unwrap_err()
                .to_string()
                .contains(&format!("/{field}"))
        );
        let text = json::write_compact(&envelope);
        let duplicate = format!(r#"{{"{field}":null,{}"#, &text[1..]);
        assert!(
            from_slice::<ReceiptEnvelope>(duplicate.as_bytes())
                .unwrap_err()
                .to_string()
                .contains("duplicate field")
        );
    }
    assert!(from_slice::<ImportReceipt>(b"[]").is_err());
}

#[test]
fn json_receipt_strict_unsigned_tokens_and_nested_pointer() {
    for (field, old) in [
        ("schema_version", "1"),
        ("source_bytes", "3"),
        ("pack_bytes", "4"),
        ("dataset_quads", "5"),
        ("named_graphs", "6"),
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
            let receipt = RECEIPT.replace(
                &format!(r#""{field}":{old}"#),
                &format!(r#""{field}":{invalid}"#),
            );
            let envelope = format!(r#"{{"receipt_digest":"d","receipt":{receipt}}}"#);
            let error = from_slice::<ReceiptEnvelope>(envelope.as_bytes())
                .unwrap_err()
                .to_string();
            assert!(error.contains(&format!("/receipt/{field}")), "{error}");
        }
    }
    assert!(
        from_slice::<ImportReceipt>(
            RECEIPT
                .replace("\"schema_version\":1", "\"schema_version\":4294967296")
                .as_bytes()
        )
        .is_err()
    );
    assert!(from_slice::<ReceiptEnvelope>(b"{not-json").is_err());
    assert!(from_slice::<ImportReceipt>(format!("{RECEIPT} true").as_bytes()).is_err());
}

fn admitted_receipt(action: &str) -> ImportReceipt {
    ImportReceipt {
        schema_version: SCHEMA_VERSION,
        action_key: action.into(),
        build_fingerprint: BUILD_FINGERPRINT.into(),
        codec: CODEC.into(),
        source_digest: "s".repeat(64),
        source_bytes: 3,
        pack_digest: "b".repeat(64),
        pack_bytes: 4,
        dataset_quads: 5,
        named_graphs: 1,
    }
}

fn write_root(namespace: &Path, receipt: ImportReceipt, envelope_digest: Option<&str>) {
    let path = namespace
        .join("receipts")
        .join(format!("{}.json", receipt.action_key));
    let envelope = ReceiptEnvelope {
        receipt_digest: envelope_digest.map_or_else(|| receipt.receipt_digest(), str::to_owned),
        receipt,
    };
    fs::write(path, json::write_pretty(&envelope.to_json())).unwrap();
}

#[test]
fn json_receipt_actual_loader_preserves_miss_and_identity_refusals() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("receipts")).unwrap();
    let action = "a".repeat(64);
    let load = || load_import_receipt(root.path(), &action, BUILD_FINGERPRINT, &"s".repeat(64), 3);
    assert!(load().unwrap().is_none());
    let receipt = admitted_receipt(&action);
    write_root(root.path(), receipt.clone(), None);
    assert_eq!(load().unwrap(), Some(receipt.clone()));
    for field in [
        "schema",
        "action",
        "build",
        "codec",
        "source",
        "source_bytes",
        "pack",
        "size",
    ] {
        let mut changed = receipt.clone();
        match field {
            "schema" => changed.schema_version += 1,
            "action" => changed.action_key = "c".repeat(64),
            "build" => changed.build_fingerprint.push('x'),
            "codec" => changed.codec.push('x'),
            "source" => changed.source_digest.push('x'),
            "source_bytes" => changed.source_bytes += 1,
            "pack" => changed.pack_digest = "bad".into(),
            "size" => changed.pack_bytes = MAX_PACK_BYTES + 1,
            _ => unreachable!(),
        }
        // Action mismatch must be represented at the selected original path.
        let bytes = json::write_pretty(
            &ReceiptEnvelope {
                receipt_digest: changed.receipt_digest(),
                receipt: changed,
            }
            .to_json(),
        );
        fs::write(root.path().join(format!("receipts/{action}.json")), bytes).unwrap();
        assert!(load().is_err(), "{field}");
    }
    write_root(root.path(), receipt, Some("wrong"));
    assert!(load().unwrap_err().to_string().contains("digest mismatch"));
    fs::write(root.path().join(format!("receipts/{action}.json")), b"{").unwrap();
    assert!(load().unwrap_err().to_string().contains("corrupt receipt"));
}

#[test]
fn json_receipt_actual_gc_uses_authored_roots_without_import() {
    let root = tempfile::tempdir().unwrap();
    for name in ["receipts", "blobs", "locks"] {
        fs::create_dir(root.path().join(name)).unwrap();
    }
    let action = "a".repeat(64);
    write_root(root.path(), admitted_receipt(&action), None);
    let reachable = root.path().join(format!("blobs/{}", "b".repeat(64)));
    let unreferenced = root.path().join(format!("blobs/{}", "c".repeat(64)));
    fs::write(&reachable, b"opaque authored pack bytes").unwrap();
    fs::write(&unreferenced, b"unreferenced").unwrap();
    prune_namespace(root.path(), &action).unwrap();
    assert!(reachable.is_file());
    assert!(!unreferenced.exists());
    assert!(
        prune_namespace(root.path(), &"d".repeat(64))
            .unwrap_err()
            .to_string()
            .contains("protected action")
    );
    write_root(root.path(), admitted_receipt(&action), Some("wrong"));
    assert!(
        prune_namespace(root.path(), &action)
            .unwrap_err()
            .to_string()
            .contains("corrupt GC receipt envelope")
    );
    let mut wrong = admitted_receipt(&action);
    wrong.codec.push('x');
    write_root(root.path(), wrong, None);
    assert!(
        prune_namespace(root.path(), &action)
            .unwrap_err()
            .to_string()
            .contains("identity mismatch")
    );
    fs::write(root.path().join(format!("receipts/{action}.json")), b"{").unwrap();
    assert!(
        prune_namespace(root.path(), &action)
            .unwrap_err()
            .to_string()
            .contains("corrupt GC root")
    );
}

#[test]
fn json_receipt_actual_gc_protects_selected_root_and_bounds_reachable_set() {
    let root = tempfile::tempdir().unwrap();
    for name in ["receipts", "blobs", "locks"] {
        fs::create_dir(root.path().join(name)).unwrap();
    }
    let protected = "a".repeat(64);
    for (action, pack) in [('a', 'd'), ('b', 'e'), ('c', 'f')] {
        let action = action.to_string().repeat(64);
        let mut receipt = admitted_receipt(&action);
        receipt.pack_digest = pack.to_string().repeat(64);
        fs::write(
            root.path().join("blobs").join(&receipt.pack_digest),
            b"authored opaque bytes",
        )
        .unwrap();
        write_root(root.path(), receipt, None);
    }
    prune_namespace(root.path(), &protected).unwrap();
    assert!(
        root.path()
            .join(format!("receipts/{protected}.json"))
            .is_file()
    );
    let roots = fs::read_dir(root.path().join("receipts"))
        .unwrap()
        .map(|entry| {
            let bytes = fs::read(entry.unwrap().path()).unwrap();
            from_slice::<ReceiptEnvelope>(&bytes)
                .unwrap()
                .receipt
                .pack_digest
        })
        .collect::<std::collections::BTreeSet<_>>();
    let blobs = fs::read_dir(root.path().join("blobs"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(roots.len(), crate::RETAINED_IMPORTS);
    assert_eq!(roots, blobs);
    fs::write(root.path().join("receipts/unexpected.bin"), b"{}").unwrap();
    assert!(
        prune_namespace(root.path(), &protected)
            .unwrap_err()
            .to_string()
            .contains("unknown receipt entry")
    );
}
