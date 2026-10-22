// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const FINITE: &str = r#"{"datatypes":["a","z"],"counts":["2","bad","bad2"]}"#;
const SOURCE: &str = r#"{"finite_value_spaces":{"v":{"datatypes":["a","z"],"counts":["2","bad","bad2"]}},"source_path":"中.ttl","subjects":["a","z"],"authored_keys":{"a":["z","a","z"]},"keys":{"a":{"Ok":"key"},"z":{"Err":{"DuplicateArgumentSlotIndex":{"node":"n","index":-1}}}},"application_nodes":2,"repeated_shadow":null}"#;
#[test]
fn json_math_observation_literal_map_set_vector_order_and_optional_omission() {
    let finite = FiniteValueSpace {
        datatypes: ["z".into(), "a".into()].into(),
        counts: ["bad2".into(), "2".into(), "bad".into()].into(),
    };
    assert_eq!(to_vec(&finite), FINITE.as_bytes());
    let source = SourceObservation {
        finite_value_spaces: [("v".into(), finite)].into(),
        source_path: "中.ttl".into(),
        subjects: ["z".into(), "a".into()].into(),
        authored_keys: [("a".into(), vec!["z".into(), "a".into(), "z".into()])].into(),
        keys: [
            (
                "z".into(),
                Err(MathLoweringError::DuplicateArgumentSlotIndex {
                    node: "n".into(),
                    index: -1,
                }),
            ),
            ("a".into(), Ok("key".into())),
        ]
        .into(),
        application_nodes: 2,
        repeated_shadow: None,
        source_lint: None,
    };
    assert_eq!(to_vec(&source), SOURCE.as_bytes());
    let restored: SourceObservation = from_slice(SOURCE.as_bytes()).unwrap();
    assert_eq!(restored.keys, source.keys);
    assert_eq!(to_vec(&restored), SOURCE.as_bytes());
    let observations = Observations {
        depth_limit: 7,
        source_digests: [("path".into(), "digest".into())].into(),
        sources: [("path".into(), source)].into(),
    };
    let expected = format!(
        "{{\"depth_limit\":7,\"source_digests\":{{\"path\":\"digest\"}},\"sources\":{{\"path\":{SOURCE}}}}}"
    );
    assert_eq!(to_vec(&observations), expected.as_bytes());
    assert_eq!(
        to_vec(&from_slice::<Observations>(expected.as_bytes()).unwrap()),
        expected.as_bytes()
    );
}
#[test]
fn json_math_observation_optionals_and_typed_results_keep_distinct_null_policies() {
    for absent in [
        SOURCE.replace(",\"repeated_shadow\":null", ""),
        SOURCE.replace("\"source_path\":", "\"source_lint\":null,\"source_path\":"),
    ] {
        assert_eq!(
            to_vec(&from_slice::<SourceObservation>(absent.as_bytes()).unwrap()),
            SOURCE.as_bytes()
        );
    }
    for outcome in [
        r#"{"Ok":"shadow"}"#,
        r#"{"Err":{"CyclicExpressionGraph":{"node":"cycle"}}}"#,
    ] {
        let text = SOURCE.replace(
            "\"repeated_shadow\":null",
            &format!("\"repeated_shadow\":{outcome}"),
        );
        assert_eq!(
            to_vec(&from_slice::<SourceObservation>(text.as_bytes()).unwrap()),
            text.as_bytes()
        );
    }
    for field in ["repeated_shadow", "source_lint"] {
        let duplicate = SOURCE.replacen('{', &format!("{{\"{field}\":null,\"{field}\":null,"), 1);
        assert_eq!(
            from_slice::<SourceObservation>(duplicate.as_bytes())
                .unwrap_err()
                .pointer(),
            format!("/{field}")
        );
    }
    let lint = SOURCE.replace(
        "\"repeated_shadow\":null",
        r#""repeated_shadow":null,"source_lint":{"source_digest":"d","diagnostics":[]}"#,
    );
    assert_eq!(
        to_vec(&from_slice::<SourceObservation>(lint.as_bytes()).unwrap()),
        lint.as_bytes()
    );
    for outcome in [
        "null",
        r#"{"Ok":null}"#,
        r#"{"Err":null}"#,
        r#"{"Ok":"x","Ok":"y"}"#,
        r#"{"Other":"x"}"#,
    ] {
        let bad = SOURCE.replace(r#"{"Ok":"key"}"#, outcome);
        assert!(
            from_slice::<SourceObservation>(bad.as_bytes()).is_err(),
            "{bad}"
        );
    }
    let bad = SOURCE.replace(r#""a":{"Ok":"key"}"#, r#""a":{"Ok":null},"a":{"Ok":"key"}"#);
    assert_eq!(
        from_slice::<SourceObservation>(bad.as_bytes())
            .unwrap_err()
            .pointer(),
        "/keys/a/Ok"
    );
}
#[test]
fn json_math_observation_counts_required_fields_duplicates_and_nested_pointers() {
    for token in ["-0", "-1", "1.0", "1e0", "18446744073709551616", "null"] {
        let bad = SOURCE.replace(
            "\"application_nodes\":2",
            &format!("\"application_nodes\":{token}"),
        );
        assert_eq!(
            from_slice::<SourceObservation>(bad.as_bytes())
                .unwrap_err()
                .pointer(),
            "/application_nodes"
        );
        let bad = format!("{{\"depth_limit\":{token},\"source_digests\":{{}},\"sources\":{{}}}}");
        assert_eq!(
            from_slice::<Observations>(bad.as_bytes())
                .unwrap_err()
                .pointer(),
            "/depth_limit"
        );
    }
    let max = SOURCE.replace(
        "\"application_nodes\":2",
        &format!("\"application_nodes\":{}", usize::MAX),
    );
    assert_eq!(
        from_slice::<SourceObservation>(max.as_bytes())
            .unwrap()
            .application_nodes,
        usize::MAX
    );
    let max = format!(
        "{{\"depth_limit\":{},\"source_digests\":{{}},\"sources\":{{}}}}",
        usize::MAX
    );
    assert_eq!(
        from_slice::<Observations>(max.as_bytes())
            .unwrap()
            .depth_limit,
        usize::MAX
    );
    for bad in [
        "{}".into(),
        "[]".into(),
        SOURCE.replace("\"subjects\":[\"a\",\"z\"],", ""),
        SOURCE.replace("\"source_path\":", "\"source_path\":null,\"source_path\":"),
        format!("{SOURCE} true"),
    ] {
        assert!(
            from_slice::<SourceObservation>(bad.as_bytes()).is_err(),
            "{bad}"
        );
    }
    assert!(from_slice::<FiniteValueSpace>(br#"{}"#).is_err());
    assert!(from_slice::<FiniteValueSpace>(br#"{"datatypes":null,"counts":[]}"#).is_err());
    assert!(
        from_slice::<FiniteValueSpace>(br#"{"datatypes":[],"counts":[],"counts":[]}"#).is_err()
    );
    let unknown = SOURCE.replacen('{', "{\"extra\":true,", 1);
    assert_eq!(
        to_vec(&from_slice::<SourceObservation>(unknown.as_bytes()).unwrap()),
        SOURCE.as_bytes()
    );
}
