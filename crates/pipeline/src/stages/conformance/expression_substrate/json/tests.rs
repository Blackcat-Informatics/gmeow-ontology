// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const PRODUCER: &str =
    r#"{"fingerprint":"f","toolchain":"t","target":"x","profile":"p","features":["z","a","z"]}"#;
const REASONED: &str = r#"{"alpha_classes":{"a":"A","z":"Z"},"operator_less_root_operator_edges":7,"expression_findings":[{"severity":"warning","code":"authored","message":"中\n\"\\"}]}"#;
const SCENE: &str = r#"{"source_path":"scene.ttl","source_digest":"source","profile":"Reference","declared_keys":{"a":["z","a","z"],"z":[]},"reasoned":{"alpha_classes":{"a":"A","z":"Z"},"operator_less_root_operator_edges":7,"expression_findings":[{"severity":"warning","code":"authored","message":"中\n\"\\"}]},"multi_graph_findings":null}"#;
#[test]
fn json_expression_profiles_preserve_all_four_unit_spellings_and_refuse_bad_tags() {
    for (profile, name) in [
        (Profile::Reference, "Reference"),
        (Profile::Twins, "Twins"),
        (Profile::MultiGraph, "MultiGraph"),
        (Profile::Control, "Control"),
    ] {
        let text = format!("\"{name}\"");
        assert_eq!(to_vec(&profile), text.as_bytes());
        assert_eq!(from_slice::<Profile>(text.as_bytes()).unwrap(), profile);
        let object = format!("{{\"{name}\":null}}");
        assert_eq!(from_slice::<Profile>(object.as_bytes()).unwrap(), profile);
    }
    for bad in [
        "null",
        "[]",
        "{}",
        r#""reference""#,
        r#""Unknown""#,
        r#"{"Reference":false}"#,
        r#"{"Control":null,"Twins":null}"#,
        r#"{"Reference":null,"Reference":null}"#,
    ] {
        assert!(from_slice::<Profile>(bad.as_bytes()).is_err(), "{bad}");
    }
}
#[test]
fn json_expression_complete_literal_preserves_nested_findings_maps_and_declared_key_order() {
    let finding =
        gmeow_errors::Finding::new(gmeow_errors::Severity::Warning, "authored", "中\n\"\\");
    let reasoned = Reasoned {
        alpha_classes: [("z".into(), "Z".into()), ("a".into(), "A".into())].into(),
        operator_less_root_operator_edges: 7,
        expression_findings: Some(vec![finding]),
    };
    assert_eq!(to_vec(&reasoned), REASONED.as_bytes());
    let scene = Scene {
        source_path: "scene.ttl".into(),
        source_digest: "source".into(),
        profile: Profile::Reference,
        declared_keys: [
            ("z".into(), vec![]),
            ("a".into(), vec!["z".into(), "a".into(), "z".into()]),
        ]
        .into(),
        reasoned: Some(reasoned),
        multi_graph_findings: None,
    };
    assert_eq!(to_vec(&scene), SCENE.as_bytes());
    let producer = crate::cache::BuildIdentity {
        fingerprint: "f".into(),
        toolchain: "t".into(),
        target: "x".into(),
        profile: "p".into(),
        features: vec!["z".into(), "a".into(), "z".into()],
    };
    let observed = Observations {
        producer,
        context_path: "context.ttl".into(),
        context_source_digest: "context-digest".into(),
        reasoning_projection_digest: "projection-digest".into(),
        scenes: [("s".into(), scene)].into(),
    };
    let expected = format!(
        "{{\"producer\":{PRODUCER},\"context_path\":\"context.ttl\",\"context_source_digest\":\"context-digest\",\"reasoning_projection_digest\":\"projection-digest\",\"scenes\":{{\"s\":{SCENE}}}}}"
    );
    assert_eq!(to_vec(&observed), expected.as_bytes());
    assert_eq!(
        to_vec(&from_slice::<Observations>(expected.as_bytes()).unwrap()),
        expected.as_bytes()
    );
    let unknown = expected.replacen('{', "{\"extra\":null,", 1);
    assert_eq!(
        to_vec(&from_slice::<Observations>(unknown.as_bytes()).unwrap()),
        expected.as_bytes()
    );
}
#[test]
fn json_expression_optional_fields_accept_missing_or_null_and_emit_explicit_null() {
    let empty = r#"{"source_path":"s","source_digest":"d","profile":"MultiGraph","declared_keys":{},"reasoned":null,"multi_graph_findings":null}"#;
    for text in [
        empty.to_owned(),
        empty
            .replace(",\"reasoned\":null", "")
            .replace(",\"multi_graph_findings\":null", ""),
    ] {
        assert_eq!(
            to_vec(&from_slice::<Scene>(text.as_bytes()).unwrap()),
            empty.as_bytes()
        );
    }
    let reasoned =
        r#"{"alpha_classes":{},"operator_less_root_operator_edges":0,"expression_findings":null}"#;
    let missing = reasoned.replace(",\"expression_findings\":null", "");
    assert_eq!(
        to_vec(&from_slice::<Reasoned>(missing.as_bytes()).unwrap()),
        reasoned.as_bytes()
    );
    let present = empty.replace(
        "\"multi_graph_findings\":null",
        "\"multi_graph_findings\":[]",
    );
    assert_eq!(
        to_vec(&from_slice::<Scene>(present.as_bytes()).unwrap()),
        present.as_bytes()
    );
    for field in ["reasoned", "multi_graph_findings"] {
        let bad = empty.replacen(
            &format!("\"{field}\":"),
            &format!("\"{field}\":null,\"{field}\":"),
            1,
        );
        assert_eq!(
            from_slice::<Scene>(bad.as_bytes()).unwrap_err().pointer(),
            format!("/{field}")
        );
    }
    let duplicate = reasoned.replace(
        "\"expression_findings\":null",
        "\"expression_findings\":null,\"expression_findings\":null",
    );
    assert_eq!(
        from_slice::<Reasoned>(duplicate.as_bytes())
            .unwrap_err()
            .pointer(),
        "/expression_findings"
    );
}
#[test]
fn json_expression_counts_require_unsigned_host_width_with_complete_member_pointers() {
    for token in [
        "-0",
        "-1",
        "1.0",
        "1e0",
        "18446744073709551616",
        "null",
        "\"1\"",
    ] {
        let bad = SCENE.replace(
            "\"operator_less_root_operator_edges\":7",
            &format!("\"operator_less_root_operator_edges\":{token}"),
        );
        assert_eq!(
            from_slice::<Scene>(bad.as_bytes()).unwrap_err().pointer(),
            "/reasoned/operator_less_root_operator_edges"
        );
    }
    let max = REASONED.replace(
        "\"operator_less_root_operator_edges\":7",
        &format!("\"operator_less_root_operator_edges\":{}", usize::MAX),
    );
    assert_eq!(
        from_slice::<Reasoned>(max.as_bytes())
            .unwrap()
            .operator_less_root_operator_edges,
        usize::MAX
    );
    let bad = SCENE.replace(
        "\"severity\":\"warning\"",
        "\"severity\":\"not-a-severity\"",
    );
    assert_eq!(
        from_slice::<Scene>(bad.as_bytes()).unwrap_err().pointer(),
        "/reasoned/expression_findings/0/severity"
    );
}
#[test]
fn json_expression_required_records_and_typed_maps_refuse_invalid_earlier_duplicates() {
    for bad in [
        "{}".into(),
        "[]".into(),
        SCENE.replace("\"source_path\":\"scene.ttl\",", ""),
        SCENE.replace("\"source_digest\":\"source\"", "\"source_digest\":null"),
        SCENE.replace(
            "\"declared_keys\":",
            "\"declared_keys\":null,\"declared_keys\":",
        ),
        format!("{SCENE} null"),
    ] {
        assert!(from_slice::<Scene>(bad.as_bytes()).is_err(), "{bad}");
    }
    assert!(
        from_slice::<Reasoned>(br#"{"alpha_classes":{},"operator_less_root_operator_edges":null}"#)
            .is_err()
    );
    let invalid = format!(
        "{{\"producer\":{PRODUCER},\"context_path\":\"c\",\"context_source_digest\":\"d\",\"reasoning_projection_digest\":\"d\",\"scenes\":{{\"s\":null,\"s\":{SCENE}}}}}"
    );
    assert_eq!(
        from_slice::<Observations>(invalid.as_bytes())
            .unwrap_err()
            .pointer(),
        "/scenes/s"
    );
    let bad = SCENE.replace(r#""a":["z","a","z"]"#, r#""a":null,"a":["z","a","z"]"#);
    assert_eq!(
        from_slice::<Scene>(bad.as_bytes()).unwrap_err().pointer(),
        "/declared_keys/a"
    );
}
