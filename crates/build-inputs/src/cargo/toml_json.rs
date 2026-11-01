// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native JSON projection for Cargo semantic commitments.

use crate::{Result, fail};
use purrdf_lex::json::{
    Object, Value,
    record::{self, ToJson},
};

/// Retain the pinned TOML scalar, datetime and table byte profile.
/// JSON semantic commitments require finite floating-point values.
pub(super) fn encode(value: &toml::Value) -> Result<Vec<u8>> {
    Ok(record::to_vec(&project(value)?))
}

/// Project supported TOML values while retaining nested refusal locations.
fn project(value: &toml::Value) -> Result<Value> {
    match value {
        toml::Value::String(value) => Ok(value.to_json()),
        toml::Value::Integer(value) => Ok(value.to_json()),
        toml::Value::Float(value) if value.is_finite() => Ok(value.to_json()),
        toml::Value::Float(value) => Err(fail(format!(
            "non-finite TOML float {value} cannot enter a JSON semantic commitment"
        ))),
        toml::Value::Boolean(value) => Ok(value.to_json()),
        toml::Value::Datetime(value) => Ok(Object::new()
            .with("$__toml_private_datetime", value.to_string().to_json())
            .into()),
        toml::Value::Array(values) => values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                project(value).map_err(|error| fail(format!("array index {index}: {error}")))
            })
            .collect::<Result<Vec<_>>>()
            .map(Value::Array),
        toml::Value::Table(fields) => {
            let mut result = Object::new();
            for (name, value) in fields {
                result.insert(
                    name.to_owned(),
                    project(value).map_err(|error| fail(format!("field {name:?}: {error}")))?,
                );
            }
            Ok(Value::Object(result))
        }
    }
}
