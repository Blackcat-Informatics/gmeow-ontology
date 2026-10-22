// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored closed records only; no registry source, fixture selector or producer.

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};
use std::collections::BTreeSet;

const FAMILY: &str = r#"{"iri":"urn:family:z","name":"zè中😀","namespaces":["urn:namespace:z/","urn:namespace:a/"],"owners":["urn:owner:z","urn:owner:a"],"minimum":7}"#;
const EXEMPTION: &str = r#"{"iri":"urn:exemption","family_iri":"urn:family:z","rationale":"why\n\t\"\\中😀\u0000","row_ceiling":8}"#;
const REGISTRY: &str = r#"{"families":[{"iri":"urn:family:z","name":"zè中😀","namespaces":["urn:namespace:z/","urn:namespace:a/"],"owners":["urn:owner:z","urn:owner:a"],"minimum":7},{"iri":"urn:family:a","name":"a","namespaces":["urn:namespace:other/"],"owners":["urn:owner:other"],"minimum":0}],"exemptions":[{"iri":"urn:exemption","family_iri":"urn:family:z","rationale":"why\n\t\"\\中😀\u0000","row_ceiling":8}]}"#;
const COMPLETE: &str = r#"{"registry":{"families":[{"iri":"urn:family:z","name":"zè中😀","namespaces":["urn:namespace:z/","urn:namespace:a/"],"owners":["urn:owner:z","urn:owner:a"],"minimum":7},{"iri":"urn:family:a","name":"a","namespaces":["urn:namespace:other/"],"owners":["urn:owner:other"],"minimum":0}],"exemptions":[{"iri":"urn:exemption","family_iri":"urn:family:z","rationale":"why\n\t\"\\中😀\u0000","row_ceiling":8}]},"guarded_namespaces":["urn:guard:a","urn:guard:z"]}"#;

fn authored_observations() -> RegistryObservations {
    RegistryObservations {
        registry: CatalogRegistry {
            families: vec![
                CatalogFamily {
                    iri: "urn:family:z".to_owned(),
                    name: "zè中😀".to_owned(),
                    namespaces: vec!["urn:namespace:z/".to_owned(), "urn:namespace:a/".to_owned()],
                    owners: vec!["urn:owner:z".to_owned(), "urn:owner:a".to_owned()],
                    minimum: 7,
                },
                CatalogFamily {
                    iri: "urn:family:a".to_owned(),
                    name: "a".to_owned(),
                    namespaces: vec!["urn:namespace:other/".to_owned()],
                    owners: vec!["urn:owner:other".to_owned()],
                    minimum: 0,
                },
            ],
            exemptions: vec![ResidueExemption {
                iri: "urn:exemption".to_owned(),
                family_iri: "urn:family:z".to_owned(),
                rationale: "why\n\t\"\\中😀\0".to_owned(),
                row_ceiling: 8,
            }],
        },
        guarded_namespaces: BTreeSet::from(["urn:guard:z".to_owned(), "urn:guard:a".to_owned()]),
    }
}

#[test]
fn json_catalog_observations_literal_bytes_pin_all_record_orders_and_unicode() {
    let observations = authored_observations();
    assert_eq!(
        to_vec(&observations.registry.families[0]),
        FAMILY.as_bytes()
    );
    assert_eq!(
        to_vec(&observations.registry.exemptions[0]),
        EXEMPTION.as_bytes()
    );
    assert_eq!(to_vec(&observations.registry), REGISTRY.as_bytes());
    assert_eq!(to_vec(&observations), COMPLETE.as_bytes());
    let restored = decode_observations(COMPLETE.as_bytes()).unwrap();
    assert_eq!(restored.registry.families, observations.registry.families);
    assert_eq!(
        restored.registry.exemptions,
        observations.registry.exemptions
    );
    assert_eq!(restored.guarded_namespaces, observations.guarded_namespaces);
    assert_eq!(to_vec(&restored), COMPLETE.as_bytes());
}

#[test]
fn json_catalog_observations_empty_collections_and_zero_counts_remain_present() {
    let observations = RegistryObservations {
        registry: CatalogRegistry {
            families: Vec::new(),
            exemptions: Vec::new(),
        },
        guarded_namespaces: BTreeSet::new(),
    };
    let expected = br#"{"registry":{"families":[],"exemptions":[]},"guarded_namespaces":[]}"#;
    assert_eq!(to_vec(&observations), expected);
    assert_eq!(to_vec(&decode_observations(expected).unwrap()), expected);
    let family: CatalogFamily =
        from_slice(br#"{"iri":"","name":"","namespaces":[],"owners":[],"minimum":0}"#).unwrap();
    assert_eq!(
        to_vec(&family),
        br#"{"iri":"","name":"","namespaces":[],"owners":[],"minimum":0}"#
    );
}

fn record_refusals<T: FromJson + ToJson>(literal: &str, fields: &[&str]) {
    let value: Value = from_slice(literal.as_bytes()).unwrap();
    let original = value.as_object().unwrap();
    for &field in fields {
        let mut missing = original.clone();
        let _ = missing.remove(field);
        assert!(
            T::from_json(&Value::Object(missing)).is_err(),
            "missing {field}"
        );
        let mut null = original.clone();
        let _ = null.insert(field, Value::Null);
        assert!(T::from_json(&Value::Object(null)).is_err(), "null {field}");
        let member = purrdf_lex::json::write_compact(original.get(field).unwrap());
        let duplicate = literal.replacen('{', &format!("{{\"{field}\":{member},"), 1);
        assert!(
            from_slice::<T>(duplicate.as_bytes()).is_err(),
            "duplicate {field}"
        );
    }
    for input in ["null", "[]", "{}", "[1,2]"] {
        assert!(from_slice::<T>(input.as_bytes()).is_err(), "{input}");
    }
    let unknown = literal.replacen('{', "{\"unknown\":1e400,\"unknown\":null,", 1);
    assert_eq!(
        to_vec(&from_slice::<T>(unknown.as_bytes()).unwrap()),
        literal.as_bytes()
    );
}

#[test]
fn json_catalog_observations_all_known_fields_require_values_and_refuse_duplicates() {
    record_refusals::<CatalogFamily>(FAMILY, &["iri", "name", "namespaces", "owners", "minimum"]);
    record_refusals::<ResidueExemption>(
        EXEMPTION,
        &["iri", "family_iri", "rationale", "row_ceiling"],
    );
    record_refusals::<CatalogRegistry>(REGISTRY, &["families", "exemptions"]);
    record_refusals::<RegistryObservations>(COMPLETE, &["registry", "guarded_namespaces"]);
}

#[test]
fn json_catalog_observations_counts_retain_exact_host_width_and_strict_unsigned_tokens() {
    let maximum = COMPLETE
        .replace("\"minimum\":7", &format!("\"minimum\":{}", usize::MAX))
        .replace(
            "\"row_ceiling\":8",
            &format!("\"row_ceiling\":{}", usize::MAX),
        );
    let decoded = decode_observations(maximum.as_bytes()).unwrap();
    assert_eq!(decoded.registry.families[0].minimum, usize::MAX);
    assert_eq!(decoded.registry.exemptions[0].row_ceiling, usize::MAX);
    assert_eq!(to_vec(&decoded), maximum.as_bytes());
    for (old, field, pointer) in [
        ("\"minimum\":7", "minimum", "/registry/families/0/minimum"),
        (
            "\"row_ceiling\":8",
            "row_ceiling",
            "/registry/exemptions/0/row_ceiling",
        ),
    ] {
        for token in [
            "-0",
            "-1",
            "1.0",
            "1e0",
            "18446744073709551616",
            "\"1\"",
            "true",
        ] {
            let input = COMPLETE.replace(old, &format!("\"{field}\":{token}"));
            let error = from_slice::<RegistryObservations>(input.as_bytes()).unwrap_err();
            assert_eq!(error.pointer(), pointer);
        }
        if usize::BITS < 64 {
            let input = COMPLETE.replace(old, &format!("\"{field}\":4294967296"));
            assert!(from_slice::<RegistryObservations>(input.as_bytes()).is_err());
        }
    }
}

#[test]
fn json_catalog_observations_set_order_deduplicates_without_reordering_vectors() {
    let input = COMPLETE.replace(
        "\"guarded_namespaces\":[\"urn:guard:a\",\"urn:guard:z\"]",
        "\"guarded_namespaces\":[\"urn:guard:z\",\"urn:guard:a\",\"urn:guard:z\"]",
    );
    let decoded = decode_observations(input.as_bytes()).unwrap();
    assert_eq!(to_vec(&decoded), COMPLETE.as_bytes());
    assert_eq!(decoded.registry.families[0].iri, "urn:family:z");
    assert_eq!(
        decoded.registry.families[0].namespaces,
        ["urn:namespace:z/", "urn:namespace:a/"]
    );
    assert_eq!(
        decoded.registry.families[0].owners,
        ["urn:owner:z", "urn:owner:a"]
    );
}

#[test]
fn json_catalog_observations_wrong_nested_kinds_and_malformed_documents_refuse() {
    for (old, new) in [
        ("\"name\":\"zè中😀\"", "\"name\":false"),
        (
            "\"namespaces\":[\"urn:namespace:z/\",\"urn:namespace:a/\"]",
            "\"namespaces\":[false]",
        ),
        (
            "\"owners\":[\"urn:owner:z\",\"urn:owner:a\"]",
            "\"owners\":{}",
        ),
        ("\"rationale\":", "\"rationale\":false,\"ignored\":"),
        (
            "\"guarded_namespaces\":[\"urn:guard:a\",\"urn:guard:z\"]",
            "\"guarded_namespaces\":[null]",
        ),
    ] {
        assert!(
            decode_observations(COMPLETE.replace(old, new).as_bytes()).is_err(),
            "{old}"
        );
    }
    for input in ["", "{", "null", "[]", "{}", &format!("{COMPLETE} false")] {
        assert!(decode_observations(input.as_bytes()).is_err(), "{input}");
    }
}

#[test]
fn json_catalog_observations_actual_decoder_preserves_native_pointer_and_typed_cause() {
    let malformed = COMPLETE.replace("\"minimum\":7", "\"minimum\":-0");
    let error = decode_observations(malformed.as_bytes()).unwrap_err();
    let kind = error.downcast_ref::<crate::error::RecordedInput>().unwrap();
    assert_eq!(
        kind.operation,
        "decode authenticated catalog-family observations"
    );
    let native = kind.cause.downcast_ref::<DecodeError>().unwrap();
    assert_eq!(native.pointer(), "/registry/families/0/minimum");
    assert!(std::error::Error::source(kind).unwrap().is::<DecodeError>());
    assert!(
        error
            .to_string()
            .contains("decode authenticated catalog-family observations")
    );
    assert!(error.to_string().contains("/registry/families/0/minimum"));
}
