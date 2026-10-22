// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const CONFIG: &str = r#"{"namespace":"urn:中","ontology_iri":"urn:o","selector_tokens":["a","z"],"core_slice_iris":[],"annotation_predicates":["p"]}"#;
#[test]
fn json_source_lint_literal_records_and_configuration_set_order() {
    let configuration = Configuration {
        namespace: "urn:中".into(),
        ontology_iri: "urn:o".into(),
        selector_tokens: ["z".into(), "a".into()].into(),
        core_slice_iris: Default::default(),
        annotation_predicates: ["p".into()].into(),
    };
    assert_eq!(to_vec(&configuration), CONFIG.as_bytes());
    let observation = Observations {
        profile: "profile".into(),
        configuration,
        sources: [
            (
                "z.ttl".into(),
                SourceRecord {
                    source_digest: "z".into(),
                    diagnostics: vec![],
                },
            ),
            (
                "a.ttl".into(),
                SourceRecord {
                    source_digest: "a".into(),
                    diagnostics: vec![],
                },
            ),
        ]
        .into(),
    };
    let expected = format!(
        "{{\"profile\":\"profile\",\"configuration\":{CONFIG},\"sources\":{{\"a.ttl\":{{\"source_digest\":\"a\",\"diagnostics\":[]}},\"z.ttl\":{{\"source_digest\":\"z\",\"diagnostics\":[]}}}}}}"
    );
    assert_eq!(to_vec(&observation), expected.as_bytes());
    assert_eq!(
        to_vec(&from_slice::<Observations>(expected.as_bytes()).unwrap()),
        expected.as_bytes()
    );
    let unknown = expected.replacen('{', "{\"extra\":false,", 1);
    assert_eq!(
        to_vec(&from_slice::<Observations>(unknown.as_bytes()).unwrap()),
        expected.as_bytes()
    );
}
#[test]
fn json_source_lint_records_refuse_missing_null_duplicate_and_malformed_fields() {
    for bad in [
        "{}",
        "[]",
        r#"{"source_digest":"d"}"#,
        r#"{"source_digest":null,"diagnostics":[]}"#,
        r#"{"source_digest":"d","diagnostics":null}"#,
        r#"{"source_digest":"d","diagnostics":[],"diagnostics":[]}"#,
    ] {
        assert!(from_slice::<SourceRecord>(bad.as_bytes()).is_err(), "{bad}");
    }
    for field in [
        "namespace",
        "ontology_iri",
        "selector_tokens",
        "core_slice_iris",
        "annotation_predicates",
    ] {
        let bad = CONFIG.replacen(
            &format!("\"{field}\":"),
            &format!("\"{field}\":null,\"{field}\":"),
            1,
        );
        assert_eq!(
            from_slice::<Configuration>(bad.as_bytes())
                .unwrap_err()
                .pointer(),
            format!("/{field}")
        );
    }
    let invalid_map = format!(
        "{{\"profile\":\"p\",\"configuration\":{CONFIG},\"sources\":{{\"a\":null,\"a\":{{\"source_digest\":\"d\",\"diagnostics\":[]}}}}}}"
    );
    assert_eq!(
        from_slice::<Observations>(invalid_map.as_bytes())
            .unwrap_err()
            .pointer(),
        "/sources/a"
    );
    assert!(from_slice::<Configuration>(format!("{CONFIG} null").as_bytes()).is_err());
}
#[test]
fn json_source_lint_native_diagnostics_keep_identity_and_complete_payload() {
    let recorded = gmeow_errors::DiagLedger::new().record(
        gmeow_errors::Diag::from(std::io::Error::other("native lint refusal"))
            .with_focus("urn:source-lint:refusal")
            .with_context("original lint context"),
        gmeow_errors::StageId::new("lint"),
    );
    let record = SourceRecord {
        source_digest: "source digest".into(),
        diagnostics: vec![*recorded.root],
    };
    let restored: SourceRecord = from_slice(&to_vec(&record)).unwrap();
    assert_eq!(restored.diagnostics, record.diagnostics);
    assert_eq!(restored.source_digest, record.source_digest);
}
