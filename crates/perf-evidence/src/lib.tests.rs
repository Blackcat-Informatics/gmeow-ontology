// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::{Object, record::ToJson as _};

#[test]
fn atomic_json_replaces_complete_evidence_and_leaves_no_temporary_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("nested/report.json");
    write_json_atomic(
        &path,
        &Value::from(Object::new().with("revision", 1.to_json())),
    )
    .unwrap();
    let replacement = Value::from(
        Object::new()
            .with("revision", 2.to_json())
            .with("accepted", false.to_json()),
    );
    write_json_atomic(&path, &replacement).unwrap();
    assert_eq!(
        fs::read(&path).unwrap(),
        b"{\n  \"accepted\": false,\n  \"revision\": 2\n}\n"
    );
    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
}

#[test]
fn failed_atomic_publication_preserves_destination_and_removes_temporary_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("report.json");
    fs::create_dir(&path).unwrap();
    let prior = path.join("prior-evidence");
    fs::write(&prior, b"preserved").unwrap();
    let error = write_json_atomic(
        &path,
        &Value::from(Object::new().with("accepted", false.to_json())),
    )
    .unwrap_err();
    assert!(error.to_string().contains("publish evidence"), "{error}");
    assert!(
        error.to_string().contains(&path.display().to_string()),
        "{error}"
    );
    assert_eq!(fs::read(prior).unwrap(), b"preserved");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn publication_preserves_legacy_member_string_null_and_integer_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("report.json");
    let value = json::record::read_document(
        br#"{"z":0,"nested":{"z":true,"a":[null,"\u00e9\n\t\"\\\u0001",18446744073709551615]},"z":2,"a":false}"#,
    ).unwrap();
    write_json_atomic(&path, &value).unwrap();
    let expected = "{\n  \"a\": false,\n  \"nested\": {\n    \"a\": [\n      null,\n      \"é\\n\\t\\\"\\\\\\u0001\",\n      18446744073709551615\n    ],\n    \"z\": true\n  },\n  \"z\": 2\n}\n";
    assert_eq!(fs::read(&path).unwrap(), expected.as_bytes());
}

#[test]
fn generated_float_bytes_keep_exponent_boundaries_and_signed_zero() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("numbers.json");
    let value = Value::from(vec![1e-5_f64, 1e-6, 1e15, 1e16, 1.5e-7, -0.0]);
    write_json_atomic(&path, &value).unwrap();
    assert_eq!(
        fs::read(&path).unwrap(),
        b"[\n  0.00001,\n  1e-6,\n  1000000000000000.0,\n  1e+16,\n  1.5e-7,\n  -0.0\n]\n"
    );
}

#[test]
fn embedded_telemetry_preserves_owner_number_lexemes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("numbers.json");
    let telemetry =
        json::record::read_document(br#"{"x":1.2300e+2,"zero":-0,"huge":1e400}"#).unwrap();
    write_json_atomic(&path, &telemetry).unwrap();
    // These spellings intentionally survive instead of a binary64 re-rendering.
    assert_eq!(
        fs::read(&path).unwrap(),
        b"{\n  \"huge\": 1e400,\n  \"x\": 1.2300e+2,\n  \"zero\": -0\n}\n"
    );
}

#[test]
fn malformed_json_is_refused_before_it_becomes_evidence() {
    for bytes in [
        b"{\"x\":01}".as_slice(),
        b"[NaN]",
        b"[Infinity]",
        b"\xff",
        br#""\ud800""#,
    ] {
        assert!(json::record::read_document(bytes).is_err(), "{bytes:?}");
    }
}

#[test]
fn publication_normalizes_constructed_nested_objects_and_keeps_native_timestamp_width() {
    let mut nested = Object::new();
    nested.push("z", 0);
    nested.push("a", 1);
    nested.push("z", 2);
    let value = Value::from(
        Object::new()
            .with("timestamp", u128::from(u64::MAX))
            .with("queue", None::<u128>)
            .with("nested", Value::array([Value::from(nested)])),
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("constructed.json");
    write_json_atomic(&path, &value).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"{\n  \"nested\": [\n    {\n      \"a\": 1,\n      \"z\": 2\n    }\n  ],\n  \"queue\": null,\n  \"timestamp\": 18446744073709551615\n}\n");
}
