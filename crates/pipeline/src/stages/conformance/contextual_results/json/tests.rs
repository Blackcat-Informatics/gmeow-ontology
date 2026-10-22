// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored metadata maps only; no dataset, selected artifact, or evaluation.

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const COMPLETE: &str = r#"{"nodes":{"a":{"properties":{"p":["<urn:a>","中\n\"\\"]}},"z":{"properties":{}}},"readiness_graphs":{"a":[null,"<urn:world>","_:world"],"z":[]}}"#;

#[test]
fn json_contextual_observation_literal_pins_record_map_set_null_and_rdf_text_order() {
    let observation = Observations {
        nodes: [
            ("z".to_owned(), Node::default()),
            (
                "a".to_owned(),
                Node {
                    properties: [(
                        "p".to_owned(),
                        ["中\n\"\\".to_owned(), "<urn:a>".to_owned()].into(),
                    )]
                    .into(),
                },
            ),
        ]
        .into(),
        readiness_graphs: [
            ("z".to_owned(), Default::default()),
            (
                "a".to_owned(),
                [
                    Some("_:world".to_owned()),
                    None,
                    Some("<urn:world>".to_owned()),
                ]
                .into(),
            ),
        ]
        .into(),
    };
    assert_eq!(to_vec(&observation), COMPLETE.as_bytes());
    let restored: Observations = from_slice(COMPLETE.as_bytes()).unwrap();
    assert_eq!(to_vec(&restored), COMPLETE.as_bytes());
    assert!(restored.readiness_graphs["a"].contains(&None));
    assert_eq!(restored.nodes["a"].properties["p"].len(), 2);
}

#[test]
fn json_contextual_observation_requires_fields_ignores_unknowns_and_refuses_known_duplicates() {
    for bad in [
        "{}",
        "[]",
        "null",
        "{\"nodes\":{},\"readiness_graphs\":null}",
        "{\"nodes\":null,\"readiness_graphs\":{}}",
        "{\"nodes\":{},\"nodes\":{},\"readiness_graphs\":{}}",
        "{\"nodes\":{\"x\":{}},\"readiness_graphs\":{}}",
        "{\"nodes\":{\"x\":{\"properties\":{},\"properties\":{}}},\"readiness_graphs\":{}}",
    ] {
        assert!(from_slice::<Observations>(bad.as_bytes()).is_err(), "{bad}");
    }
    let unknown = COMPLETE.replacen('{', "{\"unknown\":1e400,", 1);
    assert_eq!(
        to_vec(&from_slice::<Observations>(unknown.as_bytes()).unwrap()),
        COMPLETE.as_bytes()
    );
    let unknown_node = r#"{"properties":{},"unknown":false}"#;
    assert_eq!(
        to_vec(&from_slice::<Node>(unknown_node.as_bytes()).unwrap()),
        br#"{"properties":{}}"#
    );
    assert!(from_slice::<Observations>(format!("{COMPLETE} false").as_bytes()).is_err());
    assert!(from_slice::<Observations>(b"{").is_err());
}

#[test]
fn json_contextual_observation_map_duplicates_are_last_wins_after_each_value_is_admitted() {
    let map = br#"{"nodes":{"x":{"properties":{"old":[]}},"x":{"properties":{"new":[]}}},"readiness_graphs":{"x":["a","a",null,null]}}"#;
    let restored: Observations = from_slice(map).unwrap();
    assert!(restored.nodes["x"].properties.contains_key("new"));
    assert!(!restored.nodes["x"].properties.contains_key("old"));
    assert_eq!(restored.readiness_graphs["x"].len(), 2);
    let invalid =
        br#"{"nodes":{"x":{"properties":null},"x":{"properties":{}}},"readiness_graphs":{}}"#;
    assert!(from_slice::<Observations>(invalid).is_err());
}
