// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! JSON contracts over independently constructed values; no slice assembly or corpus.

use gmeow_slice_brief::{
    Annotation, AuthoringPacket, ClosureEntry, CoveredTerm, GroundingAttribute, GroundingCell,
    GroundingMargins, ObjTerm, Triple,
};
use purrdf_lex::json::{
    self,
    record::{FromJson, ToJson},
};

const EMPTY_PACKET_JSON: &str = concat!(
    "{\"packet_iri\":\"urn:packet\",\"source_slice\":\"urn:slice\",\"axis\":null,\"batch\":0,",
    "\"digest\":\"digest\",\"term_count\":0,\"exemplar_shortfall\":0,",
    "\"margins\":{\"fr_present\":0,\"fr_absent\":0,\"zh_present\":0,\"zh_absent\":0,\"external_mapped\":0,\"external_absent\":0},",
    "\"terms\":[],\"exemplars\":[],\"grounding\":[]}",
);

/// Construct a complete minimal packet without invoking slice assembly.
fn packet() -> AuthoringPacket {
    AuthoringPacket {
        packet_iri: "urn:packet".to_owned(),
        source_slice: "urn:slice".to_owned(),
        axis: None,
        batch: 0,
        digest: "digest".to_owned(),
        term_count: 0,
        exemplar_shortfall: 0,
        margins: GroundingMargins::default(),
        terms: Vec::new(),
        exemplars: Vec::new(),
        grounding: Vec::new(),
    }
}

/// Construct an independent language cell with absent optional values.
fn cell() -> GroundingCell {
    GroundingCell {
        cell_iri: "urn:cell".to_owned(),
        term: "urn:term".to_owned(),
        attribute: GroundingAttribute::Fr,
        present: true,
        predicate: None,
        value: None,
        external_entity: None,
        external_label: None,
        align_predicate: None,
        confidence: None,
        conflict: false,
        conflict_with: None,
    }
}

#[test]
fn json_packet_complete_empty_packet_keeps_literal_wire_bytes() {
    let packet = packet();
    assert_eq!(
        json::write_compact(&ToJson::to_json(&packet)),
        EMPTY_PACKET_JSON
    );
    assert_eq!(
        packet.to_json(),
        r#"{
  "packet_iri": "urn:packet",
  "source_slice": "urn:slice",
  "axis": null,
  "batch": 0,
  "digest": "digest",
  "term_count": 0,
  "exemplar_shortfall": 0,
  "margins": {
    "fr_present": 0,
    "fr_absent": 0,
    "zh_present": 0,
    "zh_absent": 0,
    "external_mapped": 0,
    "external_absent": 0
  },
  "terms": [],
  "exemplars": [],
  "grounding": []
}"#
    );
    assert_eq!(
        json::record::from_slice::<AuthoringPacket>(EMPTY_PACKET_JSON.as_bytes()).unwrap(),
        packet
    );
}

#[test]
fn json_packet_object_variants_keep_tagged_shapes_and_literal_content() {
    for (object, expected) in [
        (ObjTerm::Iri("urn:o".to_owned()), r#"{"Iri":"urn:o"}"#),
        (ObjTerm::Blank("b0".to_owned()), r#"{"Blank":"b0"}"#),
        (
            ObjTerm::Literal {
                lexical: "é\n\"文".to_owned(),
                datatype: "urn:datatype".to_owned(),
                language: Some("fr".to_owned()),
            },
            r#"{"Literal":{"lexical":"é\n\"文","datatype":"urn:datatype","language":"fr"}}"#,
        ),
        (
            ObjTerm::Literal {
                lexical: "plain".to_owned(),
                datatype: "urn:datatype".to_owned(),
                language: None,
            },
            r#"{"Literal":{"lexical":"plain","datatype":"urn:datatype","language":null}}"#,
        ),
    ] {
        assert_eq!(json::write_compact(&object.to_json()), expected);
        assert_eq!(
            json::record::from_slice::<ObjTerm>(expected.as_bytes()).unwrap(),
            object
        );
    }
}

#[test]
fn json_packet_grounding_names_preserve_pascal_case_instead_of_rdf_tags() {
    for (attribute, expected) in [
        (GroundingAttribute::En, "En"),
        (GroundingAttribute::Fr, "Fr"),
        (GroundingAttribute::Zh, "Zh"),
        (GroundingAttribute::ExternalMapped, "ExternalMapped"),
        (GroundingAttribute::Exemplar, "Exemplar"),
    ] {
        assert_eq!(attribute.to_json().as_str(), Some(expected));
        assert_eq!(
            GroundingAttribute::from_json(&json::Value::from(expected)).unwrap(),
            attribute
        );
    }
    assert_ne!(GroundingAttribute::En.tag(), "En");
    assert!(GroundingAttribute::from_json(&json::Value::from("en")).is_err());
    assert!(json::record::from_slice::<GroundingAttribute>(br#"{"En":null}"#).is_err());
}

#[test]
fn json_packet_nested_records_round_trip_without_reordering_vectors() {
    let mut packet = packet();
    packet.terms.push(CoveredTerm {
        iri: "urn:z".to_owned(),
        label: Some("文".to_owned()),
        definition: None,
        coat: vec![Annotation {
            predicate: "urn:label".to_owned(),
            language: Some("zh".to_owned()),
            value: "文\ncoat".to_owned(),
        }],
        axioms: vec![
            Triple {
                predicate: "urn:p".to_owned(),
                object: ObjTerm::Iri("urn:b".to_owned()),
            },
            Triple {
                predicate: "urn:p".to_owned(),
                object: ObjTerm::Blank("a".to_owned()),
            },
        ],
        neighbors: vec![Triple {
            predicate: "urn:q".to_owned(),
            object: ObjTerm::Literal {
                lexical: "é".to_owned(),
                datatype: "urn:datatype".to_owned(),
                language: None,
            },
        }],
        closure: vec![ClosureEntry {
            iri: "urn:dep".to_owned(),
            label: None,
            definition: Some("dep".to_owned()),
        }],
        content_digest: "term-digest".to_owned(),
    });
    packet.exemplars = vec!["urn:z".to_owned(), "urn:a".to_owned()];
    packet.grounding.push(cell());
    let encoded = packet.to_json();
    let decoded: AuthoringPacket = json::record::from_slice(encoded.as_bytes()).unwrap();
    assert_eq!(decoded, packet);
    assert_eq!(decoded.exemplars, vec!["urn:z", "urn:a"]);
    assert_eq!(
        decoded.terms[0].axioms[0].object,
        ObjTerm::Iri("urn:b".to_owned())
    );
}

#[test]
fn json_packet_optionals_accept_missing_or_null_and_emit_explicit_nulls() {
    let missing_axis = EMPTY_PACKET_JSON.replace("\"axis\":null,", "");
    assert_eq!(
        json::record::from_slice::<AuthoringPacket>(missing_axis.as_bytes()).unwrap(),
        packet()
    );
    let text = br#"{"cell_iri":"urn:cell","term":"urn:term","attribute":"Fr","present":true,"conflict":false}"#;
    let decoded: GroundingCell = json::record::from_slice(text).unwrap();
    assert_eq!(decoded, cell());
    assert_eq!(
        json::write_compact(&decoded.to_json()),
        concat!(
            "{\"cell_iri\":\"urn:cell\",\"term\":\"urn:term\",\"attribute\":\"Fr\",\"present\":true,",
            "\"predicate\":null,\"value\":null,\"external_entity\":null,\"external_label\":null,",
            "\"align_predicate\":null,\"confidence\":null,\"conflict\":false,\"conflict_with\":null}"
        )
    );
    let literal: ObjTerm =
        json::record::from_slice(br#"{"Literal":{"lexical":"x","datatype":"urn:d"}}"#).unwrap();
    assert_eq!(
        json::write_compact(&literal.to_json()),
        r#"{"Literal":{"lexical":"x","datatype":"urn:d","language":null}}"#
    );
}

#[test]
fn json_packet_known_duplicates_fail_while_unknown_record_fields_are_ignored() {
    let unknown = EMPTY_PACKET_JSON.replacen('{', "{\"unknown\":1,\"unknown\":2,", 1);
    assert_eq!(
        json::record::from_slice::<AuthoringPacket>(unknown.as_bytes()).unwrap(),
        packet()
    );
    let duplicate = EMPTY_PACKET_JSON.replacen('{', "{\"batch\":1,", 1);
    let error = json::record::from_slice::<AuthoringPacket>(duplicate.as_bytes())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("duplicate field") && error.contains("/batch"),
        "{error}"
    );
    let duplicate_optional = EMPTY_PACKET_JSON.replacen('{', "{\"axis\":null,", 1);
    let error = json::record::from_slice::<AuthoringPacket>(duplicate_optional.as_bytes())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("duplicate field") && error.contains("/axis"),
        "{error}"
    );
    for (text, pointer) in [
        (r#"{"Iri":"urn:a","Iri":"urn:b"}"#, "/Iri"),
        (
            r#"{"Literal":{"lexical":"a","lexical":"b","datatype":"urn:d"}}"#,
            "/Literal/lexical",
        ),
    ] {
        let error = json::record::from_slice::<ObjTerm>(text.as_bytes())
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("duplicate field") && error.contains(pointer),
            "{error}"
        );
    }
    let literal = r#"{"Literal":{"lexical":"a","datatype":"urn:d","ignored":false}}"#;
    assert!(json::record::from_slice::<ObjTerm>(literal.as_bytes()).is_ok());
}

#[test]
fn json_packet_malformed_types_and_ambiguous_variants_are_refused() {
    for text in ["not JSON", "[]", "null", "{}"] {
        assert!(
            json::record::from_slice::<AuthoringPacket>(text.as_bytes()).is_err(),
            "{text}"
        );
    }
    for text in [
        r#"{}"#,
        r#"{"Iri":"urn:a","Blank":"b"}"#,
        r#"{"Other":"urn:a"}"#,
        r#"{"Iri":false}"#,
        r#"{"Literal":["x","urn:d",null]}"#,
    ] {
        assert!(
            json::record::from_slice::<ObjTerm>(text.as_bytes()).is_err(),
            "{text}"
        );
    }
}

#[test]
fn json_packet_integer_members_keep_lexical_and_width_bounds() {
    for text in ["-1", "0.0", "0e0", "4294967296"] {
        let input = EMPTY_PACKET_JSON.replace("\"batch\":0", &format!("\"batch\":{text}"));
        assert!(
            json::record::from_slice::<AuthoringPacket>(input.as_bytes()).is_err(),
            "{text}"
        );
    }
    let input = EMPTY_PACKET_JSON
        .replace("\"batch\":0", "\"batch\":4294967295")
        .replace(
            "\"term_count\":0",
            &format!("\"term_count\":{}", usize::MAX),
        );
    let decoded: AuthoringPacket = json::record::from_slice(input.as_bytes()).unwrap();
    assert_eq!(decoded.batch, u32::MAX);
    assert_eq!(decoded.term_count, usize::MAX);
    let overflow =
        EMPTY_PACKET_JSON.replace("\"term_count\":0", "\"term_count\":18446744073709551616");
    assert!(json::record::from_slice::<AuthoringPacket>(overflow.as_bytes()).is_err());
}

#[test]
fn json_packet_confidence_is_finite_and_uses_native_binary64_spelling() {
    for (input, output) in [
        ("0", "0.0"),
        ("1e0", "1.0"),
        ("-0", "-0.0"),
        ("1e-7", "1e-7"),
        ("0.00001", "0.00001"),
    ] {
        let text = concat!(
            "{\"cell_iri\":\"urn:cell\",\"term\":\"urn:term\",\"attribute\":\"Fr\",",
            "\"present\":true,\"confidence\":NUMBER,\"conflict\":false}"
        )
        .replace("NUMBER", input);
        let decoded: GroundingCell = json::record::from_slice(text.as_bytes()).unwrap();
        assert!(
            json::write_compact(&decoded.to_json()).contains(&format!("\"confidence\":{output}"))
        );
    }
    let cell = cell();
    for text in ["1e400", "-1e400", "\"0.5\"", "false"] {
        let mut value = cell.to_json();
        value["confidence"] = json::read(text).unwrap();
        let error = GroundingCell::from_json(&value).unwrap_err().to_string();
        assert!(error.contains("/confidence"), "{error}");
    }
    let overflow = EMPTY_PACKET_JSON.replace(
        "\"grounding\":[]",
        concat!(
            "\"grounding\":[{\"cell_iri\":\"urn:cell\",\"term\":\"urn:term\",\"attribute\":\"Fr\",",
            "\"present\":true,\"confidence\":1e400,\"conflict\":false}]"
        ),
    );
    let error = json::record::from_slice::<AuthoringPacket>(overflow.as_bytes())
        .unwrap_err()
        .to_string();
    assert!(error.contains("/grounding/0/confidence"), "{error}");
}

#[test]
#[should_panic(expected = "grounding cell urn:cell confidence must be finite")]
fn json_packet_public_export_refuses_manually_constructed_nan() {
    let mut packet = packet();
    let mut cell = cell();
    cell.confidence = Some(f64::NAN);
    packet.grounding.push(cell);
    let _ = packet.to_json();
}

#[test]
#[should_panic(expected = "grounding cell urn:cell confidence must be finite")]
fn json_packet_native_export_refuses_manually_constructed_positive_infinity() {
    let mut cell = cell();
    cell.confidence = Some(f64::INFINITY);
    cell.to_json();
}

#[test]
#[should_panic(expected = "grounding cell urn:cell confidence must be finite")]
fn json_packet_native_export_refuses_manually_constructed_negative_infinity() {
    let mut cell = cell();
    cell.confidence = Some(f64::NEG_INFINITY);
    cell.to_json();
}
