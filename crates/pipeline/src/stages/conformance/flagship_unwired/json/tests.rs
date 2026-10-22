// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Independently authored wiring rows; no shape preparation or validation.

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const FINDING: &str = r#"{"focus":"中\n\"\\","path":null,"component":"component","source_shape":"shape","severity":"severity","failure_class":null,"message":null}"#;

#[test]
fn json_flagship_observation_literal_preserves_complete_order_nulls_and_vector_order() {
    let first = Finding {
        focus: "中\n\"\\".to_owned(),
        path: None,
        component: "component".to_owned(),
        source_shape: "shape".to_owned(),
        severity: "severity".to_owned(),
        failure_class: None,
        message: None,
    };
    assert_eq!(to_vec(&first), FINDING.as_bytes());
    let observation = Observation {
        profile: "profile".to_owned(),
        shapes_digest: "shapes".to_owned(),
        input_digest: "input".to_owned(),
        conforms: false,
        findings: vec![
            first,
            Finding {
                focus: "second".to_owned(),
                path: Some("".to_owned()),
                component: "c".to_owned(),
                source_shape: "s".to_owned(),
                severity: "v".to_owned(),
                failure_class: Some("urn:failure".to_owned()),
                message: Some("message".to_owned()),
            },
        ],
    };
    let expected = format!(
        "{{\"profile\":\"profile\",\"shapes_digest\":\"shapes\",\"input_digest\":\"input\",\"conforms\":false,\"findings\":[{FINDING},{{\"focus\":\"second\",\"path\":\"\",\"component\":\"c\",\"source_shape\":\"s\",\"severity\":\"v\",\"failure_class\":\"urn:failure\",\"message\":\"message\"}}]}}"
    );
    assert_eq!(to_vec(&observation), expected.as_bytes());
    assert_eq!(
        to_vec(&from_slice::<Observation>(expected.as_bytes()).unwrap()),
        expected.as_bytes()
    );
}

#[test]
fn json_flagship_observation_optionals_admit_missing_or_null_and_refuse_duplicates() {
    let absent = FINDING
        .replace("\"path\":null,", "")
        .replace("\"failure_class\":null,", "")
        .replace(",\"message\":null", "");
    assert_eq!(
        to_vec(&from_slice::<Finding>(absent.as_bytes()).unwrap()),
        FINDING.as_bytes()
    );
    for name in ["path", "failure_class", "message"] {
        let bad = FINDING.replace(
            &format!("\"{name}\":null"),
            &format!("\"{name}\":null,\"{name}\":null"),
        );
        assert_eq!(
            from_slice::<Finding>(bad.as_bytes()).unwrap_err().pointer(),
            format!("/{name}")
        );
        let bad = FINDING.replace(&format!("\"{name}\":null"), &format!("\"{name}\":7"));
        assert!(from_slice::<Finding>(bad.as_bytes()).is_err());
    }
}

#[test]
fn json_flagship_observation_required_fields_and_unknown_policy_remain_distinct() {
    let empty =
        br#"{"profile":"p","shapes_digest":"s","input_digest":"i","conforms":true,"findings":[]}"#;
    assert_eq!(to_vec(&from_slice::<Observation>(empty).unwrap()), empty);
    let unknown = FINDING.replacen('{', "{\"unknown\":null,", 1);
    assert_eq!(
        to_vec(&from_slice::<Finding>(unknown.as_bytes()).unwrap()),
        FINDING.as_bytes()
    );
    for bad in [
        "{}",
        "[]",
        "null",
        "{\"profile\":\"p\",\"shapes_digest\":\"s\",\"input_digest\":\"i\",\"conforms\":false,\"findings\":null}",
        "{\"profile\":\"p\",\"shapes_digest\":\"s\",\"input_digest\":\"i\",\"conforms\":false,\"conforms\":true,\"findings\":[]}",
    ] {
        assert!(from_slice::<Observation>(bad.as_bytes()).is_err(), "{bad}");
    }
    assert!(
        from_slice::<Finding>(
            FINDING
                .replace("\"component\":\"component\",", "")
                .as_bytes()
        )
        .is_err()
    );
    assert!(from_slice::<Finding>(format!("{FINDING} false").as_bytes()).is_err());
}
