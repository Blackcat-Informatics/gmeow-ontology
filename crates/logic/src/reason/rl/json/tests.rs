// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};
#[test]
fn json_rl_triple_literal_keeps_rendered_strings_world_and_edb_flag() {
    let expected = r#"{"subject":"urn:s","predicate":"urn:p","object":"\"中\\n\"@xx","world":"urn:world","is_edb":false}"#;
    let row = RlTriple {
        subject: "urn:s".into(),
        predicate: "urn:p".into(),
        object: "\"中\\n\"@xx".into(),
        world: "urn:world".into(),
        is_edb: false,
    };
    assert_eq!(to_vec(&row), expected.as_bytes());
    assert_eq!(from_slice::<RlTriple>(expected.as_bytes()).unwrap(), row);
    let asserted = expected.replace("false", "true");
    assert!(from_slice::<RlTriple>(asserted.as_bytes()).unwrap().is_edb);
    let unknown = expected.replacen('{', "{\"extra\":[],", 1);
    assert_eq!(
        to_vec(&from_slice::<RlTriple>(unknown.as_bytes()).unwrap()),
        expected.as_bytes()
    );
}
#[test]
fn json_rl_triple_requires_all_five_fields_without_coercion_or_duplicate_members() {
    let valid =
        r#"{"subject":"s","predicate":"p","object":"not parsed RDF","world":"w","is_edb":true}"#;
    for field in ["subject", "predicate", "object", "world", "is_edb"] {
        let payload = if field == "is_edb" {
            "true"
        } else {
            match field {
                "subject" => "\"s\"",
                "predicate" => "\"p\"",
                "object" => "\"not parsed RDF\"",
                _ => "\"w\"",
            }
        };
        let null = valid.replace(
            &format!("\"{field}\":{payload}"),
            &format!("\"{field}\":null"),
        );
        assert_eq!(
            from_slice::<RlTriple>(null.as_bytes())
                .unwrap_err()
                .pointer(),
            format!("/{field}")
        );
        let duplicate = valid.replace(
            &format!("\"{field}\":"),
            &format!("\"{field}\":{payload},\"{field}\":"),
        );
        assert_eq!(
            from_slice::<RlTriple>(duplicate.as_bytes())
                .unwrap_err()
                .pointer(),
            format!("/{field}")
        );
    }
    for bad in [
        "{}".into(),
        "[]".into(),
        valid.replace("\"world\":\"w\",", ""),
        valid.replace("\"is_edb\":true", "\"is_edb\":1"),
        format!("{valid} null"),
    ] {
        assert!(from_slice::<RlTriple>(bad.as_bytes()).is_err(), "{bad}");
    }
}
