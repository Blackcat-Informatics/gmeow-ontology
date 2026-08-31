// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! JSON-Schema instance validator.
//!
//! Validates a JSON or YAML instance document against a JSON Schema (the
//! SHACL-derived `generated/schemas/gmeow.schema.json`, or
//! any user-supplied draft-2020-12 schema). The engine is the Rust authority:
//! the consumer `gmeow validate --schema` CLI is a thin PyO3 binding over
//! [`validate_instance`].
//!
//! # Engine core separation
//!
//! This module is pure Rust with no binding surface. The remote-$ref
//! resolvers of the `jsonschema` crate are disabled (`default-features = false`)
//! because the GMEOW schema is fully self-contained — every `$ref` is a local
//! `#/$defs/...` pointer, so validation never touches the network or filesystem.

use jsonschema::{Draft, Validator};
use serde_json::Value;

/// The supported instance serialization formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceFormat {
    /// A JSON document.
    Json,
    /// A YAML document (deserialized straight into a [`serde_json::Value`], so a
    /// YAML instance validates identically to its JSON twin).
    Yaml,
}

/// Parse `instance` (per `format`) and `schema` (JSON), validate the instance
/// against the schema, and return human-readable violation messages.
///
/// An empty `Vec` means the instance is valid. Each violation string carries the
/// instance path (e.g. `/@graph/3/gmeow:assertionSubject`) and the validator's
/// message, and the list is sorted for deterministic output.
///
/// Hard errors — a schema that fails to compile or an instance that fails to
/// parse — are returned as a typed diagnostic; they are not validation
/// violations but caller mistakes that must surface (no fallback).
pub fn validate_instance(
    instance: &[u8],
    format: InstanceFormat,
    schema: &[u8],
) -> gmeow_errors::Result<Vec<String>> {
    let schema_value: Value = serde_json::from_slice(schema).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            detail: format!("invalid JSON schema: {e}"),
        })
    })?;

    // Compile for draft 2020-12 (the dialect the SHACL→JSON-Schema emitter targets
    // in this validator). A compile failure (malformed schema) is a hard error.
    let validator: Validator = jsonschema::options()
        .with_draft(Draft::Draft202012)
        .build(&schema_value)
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                detail: format!("invalid JSON schema: {e}"),
            })
        })?;

    let instance_value: Value = match format {
        InstanceFormat::Json => serde_json::from_slice(instance).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                detail: format!("could not parse JSON instance: {e}"),
            })
        })?,
        // serde_yaml deserializes directly into serde_json::Value, so the YAML
        // and JSON paths converge on one validation surface.
        InstanceFormat::Yaml => serde_yaml::from_slice(instance).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                detail: format!("could not parse YAML instance: {e}"),
            })
        })?,
    };

    let mut messages: Vec<String> = validator
        .iter_errors(&instance_value)
        .map(|error| {
            let path = error.instance_path().to_string();
            // The crate renders the root instance path as the empty string; show
            // a leading slash so violations always read as JSON-pointer paths.
            let path = if path.is_empty() {
                "/".to_string()
            } else {
                path
            };
            format!("{path}: {error}")
        })
        .collect();
    messages.sort();
    Ok(messages)
}

#[cfg(test)]
mod tests {
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
}
