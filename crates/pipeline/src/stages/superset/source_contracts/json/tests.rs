// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Plain authored contracts and dispatch hydration only; no selected source loader.

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};
use std::collections::BTreeSet;

const RULE: &str = r#"{"path":"z/中\n\"\\","match_prefix":true,"suffix":null,"family":"RdfFanout","form":"Turtle"}"#;
const DEFINITION_FAILURE: &str = r#"{"text":"original text","values":{"Err":{"root":{"fingerprint":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"stage":"values","grade":{"severity":"error","category":"data-shape-violation","standpoint":"binding"},"code":"authored.values","observations":[],"frames":[],"antecedents":[],"source_ctx":{"location":{},"focus":null,"term_role":null},"attributions":[],"advice":[],"labels":[],"tags":[],"knowledge":"supported","emitted_at":{"file":"authored.rs","line":1,"column":2}},"causes":[]}}}"#;
const COMPLETE: &str = r#"{"source_path":"pipeline.ttl","source_digest":"digest","fanout_rules":[{"path":"z/中\n\"\\","match_prefix":true,"suffix":null,"family":"RdfFanout","form":"Turtle"},{"path":"a","match_prefix":false,"suffix":".ttl","family":"Opaque","form":"Blob"}],"expected_outputs":["a","z"],"definitions":{"a":{"text":"values 中","values":{"Ok":["a","z"]}},"z":{"text":"empty","values":{"Ok":[]}}}}"#;

#[test]
fn json_source_contracts_literal_envelope_preserves_record_vector_and_sorted_collection_order() {
    let observation = Observation {
        source_path: "pipeline.ttl".to_owned(),
        source_digest: "digest".to_owned(),
        fanout_rules: vec![
            OwnedRule {
                path: "z/中\n\"\\".to_owned(),
                match_prefix: true,
                suffix: None,
                family: FanoutFamily::RdfFanout,
                form: OwnedForm::Turtle,
            },
            OwnedRule {
                path: "a".to_owned(),
                match_prefix: false,
                suffix: Some(".ttl".to_owned()),
                family: FanoutFamily::Opaque,
                form: OwnedForm::Blob,
            },
        ],
        expected_outputs: ["z".to_owned(), "a".to_owned()].into(),
        definitions: [
            (
                "z".to_owned(),
                Definition {
                    text: "empty".to_owned(),
                    values: Ok(BTreeSet::new()),
                },
            ),
            (
                "a".to_owned(),
                Definition {
                    text: "values 中".to_owned(),
                    values: Ok(["z".to_owned(), "a".to_owned()].into()),
                },
            ),
        ]
        .into(),
    };
    assert_eq!(to_vec(&observation.fanout_rules[0]), RULE.as_bytes());
    assert_eq!(to_vec(&observation), COMPLETE.as_bytes());
    let restored: Observation = from_slice(COMPLETE.as_bytes()).unwrap();
    assert_eq!(to_vec(&restored), COMPLETE.as_bytes());
    assert_eq!(restored.fanout_rules[0].path, "z/中\n\"\\");
    assert_eq!(restored.definitions["a"].values.as_ref().unwrap().len(), 2);
}

#[test]
fn json_source_contracts_optional_suffix_distinguishes_empty_from_missing_and_null() {
    let missing = RULE.replace("\"suffix\":null,", "");
    assert_eq!(
        to_vec(&from_slice::<OwnedRule>(missing.as_bytes()).unwrap()),
        RULE.as_bytes()
    );
    let empty = RULE.replace("\"suffix\":null", "\"suffix\":\"\"");
    assert_eq!(
        from_slice::<OwnedRule>(empty.as_bytes())
            .unwrap()
            .suffix
            .as_deref(),
        Some("")
    );
    let duplicate = RULE.replace("\"suffix\":null", "\"suffix\":null,\"suffix\":null");
    assert_eq!(
        from_slice::<OwnedRule>(duplicate.as_bytes())
            .unwrap_err()
            .pointer(),
        "/suffix"
    );
    let malformed = RULE.replace("\"suffix\":null", "\"suffix\":false");
    assert_eq!(
        from_slice::<OwnedRule>(malformed.as_bytes())
            .unwrap_err()
            .pointer(),
        "/suffix"
    );
}

#[test]
fn json_source_contracts_all_pascal_variants_retain_unit_null_objects_and_data_payloads() {
    for name in ["RdfFanout", "Edoal", "Opaque", "HeaderDict"] {
        let string = format!("\"{name}\"");
        let object = format!("{{\"{name}\":null}}");
        assert_eq!(
            to_vec(&from_slice::<FanoutFamily>(string.as_bytes()).unwrap()),
            string.as_bytes()
        );
        assert_eq!(
            to_vec(&from_slice::<FanoutFamily>(object.as_bytes()).unwrap()),
            string.as_bytes()
        );
    }
    for name in ["Turtle", "NTriples", "NQuadsSelf", "Blob", "HeaderDict"] {
        let string = format!("\"{name}\"");
        let object = format!("{{\"{name}\":null}}");
        assert_eq!(
            to_vec(&from_slice::<OwnedForm>(string.as_bytes()).unwrap()),
            string.as_bytes()
        );
        assert_eq!(
            to_vec(&from_slice::<OwnedForm>(object.as_bytes()).unwrap()),
            string.as_bytes()
        );
    }
    let graph = r#"{"NQuads":"urn:graph"}"#;
    assert_eq!(
        to_vec(&from_slice::<OwnedForm>(graph.as_bytes()).unwrap()),
        graph.as_bytes()
    );
    for invalid in [
        "\"turtle\"",
        "\"NQuads\"",
        "{}",
        "[]",
        "null",
        "{\"Turtle\":false}",
        "{\"Turtle\":null,\"Blob\":null}",
        "{\"Turtle\":null,\"Turtle\":null}",
        "{\"NQuads\":null}",
        "{\"NQuads\":7}",
    ] {
        assert!(
            from_slice::<OwnedForm>(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn json_source_contracts_definition_failure_preserves_actual_causal_record() {
    use gmeow_errors::{Diag, DiagLedger, StageId};
    let mut ledger = DiagLedger::new();
    let cause = ledger.attach(
        Diag::from(std::io::Error::other("definition source"))
            .with_focus("urn:source-contract:definition"),
        StageId::new("source"),
    );
    let rejection = ledger.record(
        Diag::from(std::io::Error::other("values refused"))
            .with_focus("urn:source-contract:values")
            .with_antecedents([cause]),
        StageId::new("values"),
    );
    assert_eq!(rejection.causes.len(), 1);
    assert_ne!(rejection.root.fingerprint, rejection.causes[0].fingerprint);
    let definition = Definition {
        text: "original text".to_owned(),
        values: Err(rejection.clone()),
    };
    let restored: Definition = from_slice(&to_vec(&definition)).unwrap();
    assert_eq!(restored.text, "original text");
    assert_eq!(restored.values.unwrap_err(), rejection);
    let literal: Definition = from_slice(DEFINITION_FAILURE.as_bytes()).unwrap();
    assert_eq!(to_vec(&literal), DEFINITION_FAILURE.as_bytes());
    assert_eq!(literal.values.unwrap_err().root.code, "authored.values");
    for invalid in [
        "{}",
        "null",
        "[]",
        "{\"Ok\":null}",
        "{\"Err\":null}",
        "{\"ok\":[]}",
        "{\"Ok\":[],\"Err\":{}}",
        "{\"Ok\":[],\"Ok\":[]}",
    ] {
        let input = format!("{{\"text\":\"t\",\"values\":{invalid}}}");
        assert!(
            from_slice::<Definition>(input.as_bytes()).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn json_source_contracts_typed_records_refuse_missing_null_duplicates_and_keep_unknowns() {
    for bad in [
        RULE.replace("\"path\":\"z/中\\n\\\"\\\\\",", ""),
        RULE.replace("\"match_prefix\":true", "\"match_prefix\":null"),
        RULE.replace(
            "\"match_prefix\":true",
            "\"match_prefix\":true,\"match_prefix\":false",
        ),
        RULE.replace("\"family\":\"RdfFanout\"", "\"family\":null"),
    ] {
        assert!(from_slice::<OwnedRule>(bad.as_bytes()).is_err(), "{bad}");
    }
    let unknown = RULE.replacen('{', "{\"unknown\":1e400,", 1);
    assert_eq!(
        to_vec(&from_slice::<OwnedRule>(unknown.as_bytes()).unwrap()),
        RULE.as_bytes()
    );
    assert!(from_slice::<Observation>(b"[]").is_err());
    assert!(from_slice::<Observation>(b"{}").is_err());
    assert!(
        from_slice::<Observation>(
            COMPLETE
                .replace("\"fanout_rules\":[", "\"fanout_rules\":null,\"ignored\":[")
                .as_bytes()
        )
        .is_err()
    );
    assert!(from_slice::<Observation>(format!("{COMPLETE} false").as_bytes()).is_err());
    assert!(from_slice::<Observation>(b"{").is_err());
    let map = r#"{"source_path":"p","source_digest":"d","fanout_rules":[],"expected_outputs":[],"definitions":{"x":{"text":"first","values":{"Ok":[]}},"x":{"text":"last","values":{"Ok":[]}}}}"#;
    assert_eq!(
        from_slice::<Observation>(map.as_bytes())
            .unwrap()
            .definitions["x"]
            .text,
        "last"
    );
    let bad_first = map.replace("\"text\":\"first\"", "\"text\":null");
    assert!(
        from_slice::<Observation>(bad_first.as_bytes()).is_err(),
        "a later duplicate map key cannot hide an invalid earlier value"
    );
}

#[test]
fn json_source_contracts_hydration_admits_only_the_supported_nquads_dispatch_graph() {
    let graph = crate::stages::superset::GRAPH_DIAGNOSTICS_IRI;
    let input = RULE.replace("\"Turtle\"", &format!("{{\"NQuads\":\"{graph}\"}}"));
    let rule: OwnedRule = from_slice(input.as_bytes()).unwrap();
    let hydrated = rule.hydrate();
    assert!(
        matches!(hydrated.form, crate::stages::superset::GraphForm::NQuads(actual) if actual == graph)
    );
    assert_eq!(hydrated.path, rule.path);
    let unsupported: OwnedRule = from_slice(
        RULE.replace("\"Turtle\"", "{\"NQuads\":\"urn:unsupported\"}")
            .as_bytes(),
    )
    .unwrap();
    assert!(std::panic::catch_unwind(|| unsupported.hydrate()).is_err());
}
