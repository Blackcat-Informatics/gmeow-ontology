// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored evidence only: no source catalog, corpus or execution is loaded.

use super::*;
use gmeow_logic_compile::ir::PreservationKind;
use purrdf_lex::json::record::{from_slice, to_vec};

const QUAD: &str = r#"{"graph":"urn:world","subject":{"Triple":{"s":{"Blank":{"label":"same","scope":17}},"p":{"Iri":"urn:states"},"o":{"Literal":{"lexical_form":"01","datatype":"http://www.w3.org/2001/XMLSchema#integer","language":null,"direction":null}}}},"predicate":"urn:edge","object":{"Literal":{"lexical_form":"مرحبا","datatype":"http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString","language":"ar","direction":"Rtl"}},"graph_component":"urn:world","derivation_id":"urn:derived","rule_iri":"urn:rule","source_quad_ids":["urn:second","urn:first"],"profile":"urn:profile","budget_status":"Exhausted"}"#;
const DERIVATION: &str = r#"{"rule_iri":"urn:rule","sources":[{"graph":"urn:premise-world","subject":"_:same","predicate":"urn:p","object":"\"01\"^^<http://www.w3.org/2001/XMLSchema#integer>"}],"tuple_sources":[{"graph":"urn:tuple-world","relation":"urn:tuple","arguments":["<urn:z>","_:same","<urn:a>"]}],"provider_sources":[{"provider_iri":"urn:provider","artifact_generation":"generation-中","model_iri":"urn:model","request_iri":"urn:request","relation_iri":"urn:external","arguments":["<urn:z>","<urn:a>"],"annotation_dimension_iri":"urn:weight"}],"annotation":9223372036854775807}"#;

fn complete() -> String {
    format!(
        "{{\"quad\":{QUAD},\"annotation\":-9223372036854775808,\"derivations\":[{DERIVATION}]}}"
    )
}

fn retains_future_field<T: FromJson + ToJson>(record: &T) {
    let mut extended = record.to_json();
    extended
        .as_object_mut()
        .unwrap()
        .insert("future", Value::Null);
    assert_eq!(to_vec(&T::from_json(&extended).unwrap()), to_vec(record));
}

#[test]
fn native_annotated_evidence_preserves_complete_terms_and_all_lineage_sources() {
    let wire = complete();
    let value: AnnotatedQuad<i64> = from_slice(wire.as_bytes()).unwrap();
    assert_eq!(to_vec(&value), wire.as_bytes());
    retains_future_field(&value);
    retains_future_field(&value.quad);
    retains_future_field(&value.derivations[0]);
    retains_future_field(&value.derivations[0].sources[0]);
    retains_future_field(&value.derivations[0].tuple_sources[0]);
    retains_future_field(&value.derivations[0].provider_sources[0]);
    retains_future_field(&PreservationClaim::exact());
    assert_eq!(value.annotation, i64::MIN);
    assert_eq!(value.derivations[0].annotation, i64::MAX);
    assert_eq!(value.quad.source_quad_ids, ["urn:second", "urn:first"]);
    assert_eq!(
        value.derivations[0].tuple_sources[0].arguments,
        ["<urn:z>", "_:same", "<urn:a>"]
    );
    assert_eq!(
        value.derivations[0].provider_sources[0].arguments,
        ["<urn:z>", "<urn:a>"]
    );
    assert_eq!(
        value.quad.object,
        purrdf::TermValue::Literal {
            lexical_form: "مرحبا".into(),
            datatype: "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString".into(),
            language: Some("ar".into()),
            direction: Some(purrdf::RdfTextDirection::Rtl),
        }
    );
    let unknown = wire.replacen('{', "{\"future\":null,", 1);
    assert_eq!(
        to_vec(&from_slice::<AnnotatedQuad<i64>>(unknown.as_bytes()).unwrap()),
        wire.as_bytes()
    );
    let claim = br#"{"polarities":["Unsupported","Exact","Exact"],"unsupported_constructs":["urn:z","urn:a","urn:z"]}"#;
    assert_eq!(
        to_vec(&from_slice::<PreservationClaim>(claim).unwrap()),
        br#"{"polarities":["Exact","Unsupported"],"unsupported_constructs":["urn:a","urn:z"]}"#
    );
}

fn wire_variants<T: FromJson + ToJson>(names: &[&str]) {
    for name in names {
        let quoted = format!("\"{name}\"");
        assert_eq!(
            to_vec(&from_slice::<T>(quoted.as_bytes()).unwrap()),
            quoted.as_bytes()
        );
        let unit_object = format!("{{\"{name}\":null}}");
        assert_eq!(
            to_vec(&from_slice::<T>(unit_object.as_bytes()).unwrap()),
            quoted.as_bytes()
        );
        assert!(from_slice::<T>(format!("{{\"{name}\":false}}").as_bytes()).is_err());
    }
    assert!(from_slice::<T>(br#""unknown""#).is_err());
}

#[test]
fn native_annotation_enum_transport_retains_rust_tags_independently_of_display() {
    wire_variants::<BudgetStatus>(&["Ok", "Partial", "Exhausted"]);
    wire_variants::<SemiringLaw>(&[
        "AddCommutative",
        "AddAssociative",
        "AdditiveIdentity",
        "MultiplyAssociative",
        "MultiplicativeIdentity",
        "Distributive",
        "ZeroAnnihilates",
    ]);
    wire_variants::<AnnotationQueryClass>(&[
        "PositiveAcyclic",
        "PositiveRecursive",
        "PositiveNaryAcyclic",
        "PositiveNaryRecursive",
        "StratifiedNaf",
        "StratifiedAggregate",
        "StratifiedAggregateChase",
        "WellFounded",
        "StableModel",
        "ExistentialChase",
    ]);
    wire_variants::<AnnotationLineageContract>(&[
        "AllPhysicalDerivations",
        "CompleteGroupSupport",
        "SelectedPhysicalDerivation",
    ]);
    wire_variants::<PreservationKind>(&[
        "Exact",
        "SoundUnder",
        "CompleteOver",
        "ValidationOnly",
        "InconsistencyPreserving",
        "InconsistencyReflecting",
        "Unsupported",
    ]);
    assert!(from_slice::<BudgetStatus>(br#""ok""#).is_err());
    assert!(from_slice::<SemiringLaw>(br#""add-commutative""#).is_err());
    assert!(from_slice::<AnnotationQueryClass>(br#""positive-recursive""#).is_err());
    assert!(from_slice::<PreservationKind>(br#""ExactPreservation""#).is_err());
}

#[test]
fn native_evidence_admission_refuses_ambiguous_missing_and_contradictory_records() {
    let wire = complete();
    for invalid in [
        wire.replace(
            "\"graph_component\":\"urn:world\"",
            "\"graph_component\":\"urn:other\"",
        ),
        wire.replace("\"source_quad_ids\":[\"urn:second\",\"urn:first\"],", ""),
        wire.replace(
            "\"tuple_sources\":[",
            "\"tuple_sources\":[],\"tuple_sources\":[",
        ),
        wire.replace("\"artifact_generation\":\"generation-中\",", ""),
        wire.replace("\"scope\":17", "\"scope\":17,\"scope\":19"),
        format!("[{QUAD},-9223372036854775808,[{DERIVATION}]]"),
    ] {
        assert!(
            from_slice::<AnnotatedQuad<i64>>(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
    let mismatch = wire.replace(
        "\"graph_component\":\"urn:world\"",
        "\"graph_component\":\"urn:other\"",
    );
    let error = from_slice::<AnnotatedQuad<i64>>(mismatch.as_bytes())
        .unwrap_err()
        .to_string();
    assert!(error.contains("/quad/graph_component"), "{error}");
    assert!(error.contains("must equal graph"), "{error}");
    let invalid = br#"{"polarities":["ValidationOnly"],"unsupported_constructs":[]}"#;
    let error = from_slice::<PreservationClaim>(invalid)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("/polarities") && error.contains("ValidationOnly"),
        "{error}"
    );
    for token in [
        "-0",
        "1.0",
        "1e0",
        "9223372036854775808",
        "-9223372036854775809",
        "null",
        "\"1\"",
    ] {
        for (original, pointer) in [
            ("-9223372036854775808", "/annotation"),
            ("9223372036854775807", "/derivations/0/annotation"),
        ] {
            let invalid = wire.replace(original, token);
            let error = from_slice::<AnnotatedQuad<i64>>(invalid.as_bytes())
                .unwrap_err()
                .to_string();
            assert!(error.contains(pointer), "{error}");
        }
    }
}
