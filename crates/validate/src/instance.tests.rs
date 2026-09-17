// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// A minimal self-contained draft-2020-12 schema mirroring the shape of the
/// generated GMEOW schema: an object with one required property.
const SCHEMA: &[u8] = br#"{
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": {
            "gmeow:assertionSubject": { "type": "string" },
            "gmeow:assertionFacet": { "type": "string" }
        },
        "required": ["gmeow:assertionSubject"]
    }"#;

#[test]
fn valid_instance_passes() {
    let instance = br#"{"gmeow:assertionSubject": "ex:s"}"#;
    let errors = validate_instance(instance, InstanceFormat::Json, SCHEMA).unwrap();
    assert!(errors.is_empty(), "expected no violations, got {errors:?}");
}

#[test]
fn missing_required_property_fails() {
    let instance = br#"{"gmeow:assertionFacet": "ex:f"}"#;
    let errors = validate_instance(instance, InstanceFormat::Json, SCHEMA).unwrap();
    assert!(
        !errors.is_empty(),
        "expected a violation for the missing property"
    );
    assert!(
        errors.iter().any(|m| m.contains("gmeow:assertionSubject")),
        "violation should name the missing required property, got {errors:?}"
    );
}

#[test]
fn yaml_instance_validates_like_its_json_twin() {
    let yaml = b"gmeow:assertionSubject: ex:s\n";
    let yaml_errors = validate_instance(yaml, InstanceFormat::Yaml, SCHEMA).unwrap();
    assert!(
        yaml_errors.is_empty(),
        "valid YAML should pass, got {yaml_errors:?}"
    );

    let bad_yaml = b"gmeow:assertionFacet: ex:f\n";
    let json_twin = br#"{"gmeow:assertionFacet": "ex:f"}"#;
    let yaml_bad = validate_instance(bad_yaml, InstanceFormat::Yaml, SCHEMA).unwrap();
    let json_bad = validate_instance(json_twin, InstanceFormat::Json, SCHEMA).unwrap();
    assert_eq!(
        yaml_bad, json_bad,
        "a YAML instance must validate identically to its JSON twin"
    );
}

#[test]
fn malformed_schema_is_a_hard_error() {
    let not_json = b"this is not json";
    let err = validate_instance(br#"{}"#, InstanceFormat::Json, not_json)
        .expect_err("a non-JSON schema must be a hard error");
    assert!(err.is::<crate::error::Parse>());
    assert!(err.message().contains("invalid JSON schema"), "got {err}");
}

#[test]
fn unparsable_instance_is_a_hard_error() {
    let err = validate_instance(b"not json", InstanceFormat::Json, SCHEMA)
        .expect_err("an unparsable instance must be a hard error");
    assert!(err.is::<crate::error::Parse>());
    assert!(
        err.message().contains("could not parse JSON instance"),
        "got {err}"
    );
}
