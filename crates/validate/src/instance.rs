// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! JSON Schema instance validation for the native consumer CLI.
//!
//! JSON and YAML converge on the substrate's JSON value without a serialization
//! crossing. Draft 2020-12 meta-schemas are embedded; references resolve only
//! within the supplied document or those registered meta-schemas. No network or
//! filesystem resolver participates in validation.

use std::error::Error;
use std::sync::Mutex;

use gmeow_errors::{Diag, DiagKind};
use purrdf_jsonschema::{Metaschemas, OutputFormat, Registry};
use purrdf_lex::json::{self, Limits, Value};

const DOCUMENT_URI: &str = "https://blackcatinformatics.ca/gmeow/instance-schema";

const META_DOCUMENTS: [(&str, &[u8]); 9] = [
    (
        "https://json-schema.org/draft/2020-12/schema",
        include_bytes!("../vendor/json-schema-2020-12/schema.json"),
    ),
    (
        "https://json-schema.org/draft/2020-12/meta/core",
        include_bytes!("../vendor/json-schema-2020-12/meta/core.json"),
    ),
    (
        "https://json-schema.org/draft/2020-12/meta/applicator",
        include_bytes!("../vendor/json-schema-2020-12/meta/applicator.json"),
    ),
    (
        "https://json-schema.org/draft/2020-12/meta/unevaluated",
        include_bytes!("../vendor/json-schema-2020-12/meta/unevaluated.json"),
    ),
    (
        "https://json-schema.org/draft/2020-12/meta/validation",
        include_bytes!("../vendor/json-schema-2020-12/meta/validation.json"),
    ),
    (
        "https://json-schema.org/draft/2020-12/meta/meta-data",
        include_bytes!("../vendor/json-schema-2020-12/meta/meta-data.json"),
    ),
    (
        "https://json-schema.org/draft/2020-12/meta/format-annotation",
        include_bytes!("../vendor/json-schema-2020-12/meta/format-annotation.json"),
    ),
    (
        "https://json-schema.org/draft/2020-12/meta/format-assertion",
        include_bytes!("../vendor/json-schema-2020-12/meta/format-assertion.json"),
    ),
    (
        "https://json-schema.org/draft/2020-12/meta/content",
        include_bytes!("../vendor/json-schema-2020-12/meta/content.json"),
    ),
];

/// The supported instance serialization formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceFormat {
    /// A JSON document.
    Json,
    /// A YAML document, read directly into the same native JSON data model.
    Yaml,
}

/// Validate document bytes against a JSON Schema using draft 2020-12 by default.
///
/// An empty list means valid. Violations use the native engine's Basic output:
/// each message begins with its instance JSON Pointer (root displayed as `/`),
/// followed by the engine's explanation. Messages are sorted deterministically.
/// Message wording belongs to the native engine, rather than a second renderer.
///
/// Malformed documents, invalid schemas, unavailable references and evaluation
/// failures are hard diagnostics, distinct from an invalid instance verdict.
/// Parser and engine errors retain their original typed causes.
pub fn validate_instance(
    instance: &[u8],
    format: InstanceFormat,
    schema: &[u8],
) -> gmeow_errors::Result<Vec<String>> {
    let schema_value = json::read_slice(schema, Limits::DEFAULT)
        .map_err(|e| parse_error("invalid JSON schema", e))?;
    let mut registry = schema_registry()?;
    registry
        .add_resource(DOCUMENT_URI, schema_value)
        .map_err(|e| parse_error("invalid JSON schema", e))?;
    let validator = registry
        .compile(DOCUMENT_URI)
        .map_err(|e| parse_error("invalid JSON schema", e))?;

    let instance_value = match format {
        InstanceFormat::Json => json::read_slice(instance, Limits::DEFAULT)
            .map_err(|e| parse_error("could not parse JSON instance", e))?,
        InstanceFormat::Yaml => {
            let text = std::str::from_utf8(instance)
                .map_err(|e| parse_error("could not parse YAML instance", e))?;
            purrdf_lex::yaml::read(text)
                .map_err(|e| parse_error("could not parse YAML instance", e))?
        }
    };
    let output = validator
        .evaluate(&instance_value)
        .map_err(|e| engine_error("could not evaluate JSON Schema instance", e))?;
    if output.is_valid() {
        return Ok(Vec::new());
    }
    let basic = output.to_json(OutputFormat::Basic);
    let malformed_output = || {
        Diag::of_kind(crate::error::Engine {
            detail: "native JSON Schema Basic output lacks required violation fields".into(),
        })
    };
    let errors = basic
        .get("errors")
        .and_then(Value::as_array)
        .filter(|errors| !errors.is_empty())
        .ok_or_else(malformed_output)?;
    let mut messages = Vec::with_capacity(errors.len());
    for error in errors {
        let path = error
            .get("instanceLocation")
            .and_then(Value::as_str)
            .ok_or_else(malformed_output)?;
        let message = error
            .get("error")
            .and_then(Value::as_str)
            .ok_or_else(malformed_output)?;
        let path = if path.is_empty() { "/" } else { path };
        messages.push(format!("{path}: {message}"));
    }
    messages.sort();
    Ok(messages)
}

/// Compile the embedded set once; each registry shares its immutable documents
/// and validators. Release the initialization lock before compiling user input.
fn schema_registry() -> gmeow_errors::Result<Registry> {
    static METASCHEMAS: Mutex<Option<Metaschemas>> = Mutex::new(None);
    let mut shared = METASCHEMAS.lock().map_err(|_| {
        Diag::of_kind(crate::error::Engine {
            detail: "JSON Schema meta-schema initialization lock is poisoned".into(),
        })
    })?;
    if shared.is_none() {
        let documents = META_DOCUMENTS
            .iter()
            .map(|(uri, bytes)| {
                json::read_slice(bytes, Limits::DEFAULT)
                    .map(|value| (*uri, value))
                    .map_err(|e| parse_error(&format!("invalid embedded meta-schema {uri}"), e))
            })
            .collect::<gmeow_errors::Result<Vec<_>>>()?;
        *shared = Some(
            Metaschemas::new(documents)
                .map_err(|e| engine_error("could not compile embedded meta-schemas", e))?,
        );
    }
    Ok(Registry::with_metaschemas(
        shared.as_ref().expect("initialized above"),
    ))
}

/// Preserve the operation's stable classification and the original codec cause.
#[track_caller]
fn parse_error(operation: &str, source: impl Error + Send + Sync + 'static) -> Diag {
    let kind = crate::error::Parse {
        detail: format!("{operation}: {source}"),
    };
    diagnostic_with_source(kind, source)
}

/// Preserve evaluation failures as hard engine errors, never invalid verdicts.
#[track_caller]
fn engine_error(operation: &str, source: impl Error + Send + Sync + 'static) -> Diag {
    let kind = crate::error::Engine {
        detail: format!("{operation}: {source}"),
    };
    diagnostic_with_source(kind, source)
}

#[track_caller]
fn diagnostic_with_source(kind: impl DiagKind, source: impl Error + Send + Sync + 'static) -> Diag {
    let mut diag = Diag::new(kind.code(), kind.grade(), kind.to_string());
    diag.inner_mut().source = Some(Box::new(source));
    diag
}

#[path = "instance.tests.rs"]
#[cfg(test)]
mod tests;
