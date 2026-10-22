// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

#[test]
fn json_inference_observation_literal_preserves_sorted_maps_nullable_messages_and_row_order() {
    let control = Control {
        input_digest: "input".into(),
        results: vec![
            ResultMessage {
                severity: "http://www.w3.org/ns/shacl#Violation".into(),
                message: Some("中\n\"\\".into()),
            },
            ResultMessage {
                severity: "warning".into(),
                message: None,
            },
            ResultMessage {
                severity: "http://www.w3.org/ns/shacl#Violation".into(),
                message: None,
            },
        ],
    };
    let expected_control = r#"{"input_digest":"input","results":[{"severity":"http://www.w3.org/ns/shacl#Violation","message":"中\n\"\\"},{"severity":"warning","message":null},{"severity":"http://www.w3.org/ns/shacl#Violation","message":null}]}"#;
    assert_eq!(to_vec(&control), expected_control.as_bytes());
    assert_eq!(control.violations(), ["中\n\"\\", ""]);
    let observed = Observations {
        profile: "declared".into(),
        sources: [("z".into(), "Z".into()), ("a".into(), "A".into())].into(),
        controls: [
            (
                "z".into(),
                Control {
                    input_digest: "empty".into(),
                    results: vec![],
                },
            ),
            ("a".into(), control),
        ]
        .into(),
    };
    let expected = format!(
        "{{\"profile\":\"declared\",\"sources\":{{\"a\":\"A\",\"z\":\"Z\"}},\"controls\":{{\"a\":{expected_control},\"z\":{{\"input_digest\":\"empty\",\"results\":[]}}}}}}"
    );
    assert_eq!(to_vec(&observed), expected.as_bytes());
    let restored: Observations = from_slice(expected.as_bytes()).unwrap();
    assert_eq!(to_vec(&restored), expected.as_bytes());
    assert_eq!(restored.controls["a"].violations(), ["中\n\"\\", ""]);
}
#[test]
fn json_inference_observation_missing_message_null_and_empty_text_keep_distinct_output() {
    let expected = r#"{"severity":"uninterpreted","message":null}"#;
    for text in [expected, r#"{"severity":"uninterpreted"}"#] {
        assert_eq!(
            to_vec(&from_slice::<ResultMessage>(text.as_bytes()).unwrap()),
            expected.as_bytes()
        );
    }
    let present = r#"{"severity":"uninterpreted","message":""}"#;
    assert_eq!(
        to_vec(&from_slice::<ResultMessage>(present.as_bytes()).unwrap()),
        present.as_bytes()
    );
    let unknown = r#"{"severity":"uninterpreted","unknown":false,"message":null}"#;
    assert_eq!(
        to_vec(&from_slice::<ResultMessage>(unknown.as_bytes()).unwrap()),
        expected.as_bytes()
    );
    let control = Control {
        input_digest: "d".into(),
        results: vec![
            ResultMessage {
                severity: "Violation".into(),
                message: Some("not selected by shorthand".into()),
            },
            ResultMessage {
                severity: "http://www.w3.org/ns/shacl#Violation".into(),
                message: None,
            },
            ResultMessage {
                severity: "http://www.w3.org/ns/shacl#Violation".into(),
                message: Some("".into()),
            },
        ],
    };
    assert_eq!(control.violations(), ["", ""]);
}
#[test]
fn json_inference_observation_required_fields_duplicates_and_typed_maps_fail_with_pointers() {
    for bad in [
        "{}",
        "[]",
        r#"{"input_digest":"d"}"#,
        r#"{"input_digest":"d","results":null}"#,
        r#"{"input_digest":"d","results":[],"results":[]}"#,
    ] {
        assert!(from_slice::<Control>(bad.as_bytes()).is_err(), "{bad}");
    }
    for bad in [
        "{}",
        "[]",
        r#"{"severity":null}"#,
        r#"{"severity":"s","message":42}"#,
        r#"{"severity":"s","message":null,"message":null}"#,
    ] {
        assert!(
            from_slice::<ResultMessage>(bad.as_bytes()).is_err(),
            "{bad}"
        );
    }
    let malformed = r#"{"profile":"p","sources":{},"controls":{"c":{"input_digest":"d","results":[{"severity":"s","message":false}]}}}"#;
    assert_eq!(
        from_slice::<Observations>(malformed.as_bytes())
            .unwrap_err()
            .pointer(),
        "/controls/c/results/0/message"
    );
    let bad_map = r#"{"profile":"p","sources":{},"controls":{"c":null,"c":{"input_digest":"d","results":[]}}}"#;
    assert_eq!(
        from_slice::<Observations>(bad_map.as_bytes())
            .unwrap_err()
            .pointer(),
        "/controls/c"
    );
    let bad_sources = r#"{"profile":"p","sources":{"s":null,"s":"digest"},"controls":{}}"#;
    assert_eq!(
        from_slice::<Observations>(bad_sources.as_bytes())
            .unwrap_err()
            .pointer(),
        "/sources/s"
    );
    for bad in [
        "{}",
        r#"{"profile":"p","sources":{},"controls":{},"profile":"p"}"#,
        r#"{"profile":"p","sources":{},"controls":{}} null"#,
    ] {
        assert!(from_slice::<Observations>(bad.as_bytes()).is_err(), "{bad}");
    }
}
