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
        // The substrate YAML reader yields the JSON data model, so the YAML and JSON
        // paths converge on one validation surface (crossing as JSON text).
        InstanceFormat::Yaml => {
            let parse_error = |detail: String| {
                gmeow_errors::Diag::of_kind(crate::error::Parse {
                    detail: format!("could not parse YAML instance: {detail}"),
                })
            };
            let text = std::str::from_utf8(instance).map_err(|e| parse_error(e.to_string()))?;
            let value = purrdf_lex::yaml::read(text).map_err(|e| parse_error(e.to_string()))?;
            serde_json::from_str(&value.to_string()).map_err(|e| parse_error(e.to_string()))?
        }
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

#[path = "instance.tests.rs"]
#[cfg(test)]
mod tests;
