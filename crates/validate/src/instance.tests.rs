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
    assert_eq!(err.code(), crate::error::Parse::register());
    assert!(err.is::<purrdf_lex::json::Error>());
    assert!(err.message().contains("invalid JSON schema"), "got {err}");
}

#[test]
fn unparsable_instance_is_a_hard_error() {
    let err = validate_instance(b"not json", InstanceFormat::Json, SCHEMA)
        .expect_err("an unparsable instance must be a hard error");
    assert_eq!(err.code(), crate::error::Parse::register());
    assert!(err.is::<purrdf_lex::json::Error>());
    assert!(
        err.message().contains("could not parse JSON instance"),
        "got {err}"
    );
}

#[test]
fn nested_violations_use_escaped_pointers_and_sorted_messages() {
    let schema = br#"{
        "type": "object",
        "properties": {
            "z": {"type": "integer"},
            "a/b~c": {"type": "array", "items": {"type": "string"}}
        }
    }"#;
    let instance = br#"{"z": "wrong", "a/b~c": [false]}"#;
    let errors = validate_instance(instance, InstanceFormat::Json, schema).unwrap();
    // Basic output includes applicator explanations as well as leaf failures.
    // Retain all of them and deliberately pin the native message spelling.
    assert_eq!(
        errors,
        [
            "/: properties [\"z\", \"a/b~c\"] failed their schemas",
            "/a~1b~0c/0: expected string, found boolean",
            "/a~1b~0c: items [0] failed the items schema",
            "/z: expected integer, found string",
        ]
    );
    let yaml = b"z: wrong\na/b~c: [false]\n";
    assert_eq!(
        errors,
        validate_instance(yaml, InstanceFormat::Yaml, schema).unwrap()
    );
}

#[test]
fn false_schema_is_a_root_violation_not_an_engine_failure() {
    let errors = validate_instance(b"null", InstanceFormat::Json, b"false").unwrap();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].starts_with("/: "), "{errors:?}");
    assert!(
        validate_instance(b"null", InstanceFormat::Json, b"true")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn invalid_keyword_type_is_a_typed_schema_failure() {
    let err = validate_instance(b"{}", InstanceFormat::Json, br#"{"required": 17}"#)
        .expect_err("an invalid schema must not become an instance verdict");
    assert_eq!(err.code(), crate::error::Parse::register());
    assert!(err.is::<purrdf_jsonschema::SchemaError>(), "{err:?}");
}

#[test]
fn unavailable_reference_is_a_hard_error_without_a_resolver() {
    let err = validate_instance(
        b"{}",
        InstanceFormat::Json,
        br#"{"$ref": "https://example.invalid/not-registered"}"#,
    )
    .expect_err("validation cannot fetch an unavailable schema");
    assert!(
        matches!(
            err.downcast_ref::<purrdf_jsonschema::SchemaError>(),
            Some(purrdf_jsonschema::SchemaError::UnresolvedReference { .. })
        ),
        "{err:?}"
    );
}

#[test]
fn malformed_yaml_and_non_utf8_are_hard_errors() {
    let malformed = validate_instance(b"[unterminated", InstanceFormat::Yaml, SCHEMA)
        .expect_err("malformed YAML cannot produce a verdict");
    assert_eq!(malformed.code(), crate::error::Parse::register());
    assert!(
        malformed
            .message()
            .contains("could not parse YAML instance")
    );
    let non_utf8 =
        validate_instance(&[0xff], InstanceFormat::Yaml, SCHEMA).expect_err("YAML must be UTF-8");
    assert!(non_utf8.is::<std::str::Utf8Error>());
}

#[test]
fn local_reference_and_exact_numeric_values_validate_in_both_formats() {
    let schema = br##"{
        "$defs": {"amount": {"const": 9007199254740993}},
        "properties": {"amount": {"$ref": "#/$defs/amount"}},
        "required": ["amount"]
    }"##;
    for (instance, format) in [
        (
            br#"{"amount": 9007199254740993}"#.as_slice(),
            InstanceFormat::Json,
        ),
        (
            b"amount: 9007199254740993\n".as_slice(),
            InstanceFormat::Yaml,
        ),
    ] {
        assert!(
            validate_instance(instance, format, schema)
                .unwrap()
                .is_empty()
        );
    }
    let errors = validate_instance(
        br#"{"amount": 9007199254740992}"#,
        InstanceFormat::Json,
        schema,
    )
    .unwrap();
    assert!(!errors.is_empty());
}

#[test]
fn embedded_schema_parse_failure_retains_document_and_native_cause() {
    let failure = load_metaschemas(&[("https://example.org/meta", b"not JSON")]).unwrap_err();
    let diagnostic = failure.diagnostic();
    assert_eq!(diagnostic.code(), crate::error::Parse::register());
    assert!(diagnostic.message().contains("https://example.org/meta"));
    let source = diagnostic
        .downcast_ref::<json::Error>()
        .expect("native codec cause");
    assert_eq!(
        *source,
        json::read_slice(b"not JSON", Limits::DEFAULT).unwrap_err()
    );
}

#[test]
fn embedded_schema_compile_failure_retains_native_reference_fields() {
    let failure = load_metaschemas(&[(
        "https://example.org/meta",
        br#"{"$schema":"https://example.org/unavailable"}"#,
    )])
    .unwrap_err();
    let diagnostic = failure.diagnostic();
    assert_eq!(diagnostic.code(), crate::error::Engine::register());
    assert!(
        matches!(
            diagnostic.downcast_ref::<purrdf_jsonschema::SchemaError>(),
            Some(purrdf_jsonschema::SchemaError::MissingMetaschema { metaschema, .. })
                if metaschema == "https://example.org/unavailable"
        ),
        "{diagnostic:?}"
    );
}
