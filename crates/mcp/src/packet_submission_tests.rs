// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn candidate_submission_schema_keeps_explicit_formats_optional() {
    let descriptor = candidate_submission_descriptor();
    assert_eq!(
        descriptor["inputSchema"]["required"],
        json!(["formula", "kb", "standpoint"])
    );
    assert_eq!(
        descriptor["inputSchema"]["properties"]["kb_format"]["type"],
        "string"
    );
    let supplied = json!({"kb_format":"application/n-quads"});
    assert_eq!(
        optional_str_checked(&supplied, "kb_format").unwrap(),
        Some("application/n-quads")
    );
    assert_eq!(optional_str_checked(&json!({}), "kb_format").unwrap(), None);
}

#[test]
fn candidate_submission_rejects_invalid_selected_string_values() {
    for key in ["kb_format", "math_conjecture", "for_slice", "for_packet"] {
        for value in [Value::Null, json!(false), json!(3)] {
            let mut args = json!({});
            args[key] = value;
            assert!(optional_str_checked(&args, key).is_err(), "{args}");
        }
    }
}
