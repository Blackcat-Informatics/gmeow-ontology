// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact native cache tuples; their source roles remain separate from asserted data.
use super::{Cell, Declarations};
use gmeow_validate::grounding_observation::TranspilationOutcome;
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, ToJson, Within},
};

/// Original catalog envelope and optional transpilation, in the original tuple order.
#[derive(Debug)]
pub(super) struct Catalog(pub (Vec<Cell>, Option<Result<usize, gmeow_errors::RecordedDiag>>));
impl ToJson for Catalog {
    fn to_json(&self) -> Value {
        Value::Array(vec![
            self.0.0.to_json(),
            self.0
                .1
                .as_ref()
                .map(gmeow_errors::json::recorded_result_to_json)
                .unwrap_or(Value::Null),
        ])
    }
}
impl FromJson for Catalog {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let cells = value
            .as_array()
            .ok_or_else(|| DecodeError::invalid_type(value, "catalog cache pair"))?;
        if cells.len() != 2 {
            return Err(DecodeError::invalid_value(
                value,
                "exactly two catalog cache cells",
            ));
        }
        let catalog = Vec::<Cell>::from_json(&cells[0])
            .map_err(|error| error.within_index_native(0, None))?;
        let outcome = Option::<TranspilationOutcome>::from_json(&cells[1])
            .map_err(|error| error.within_index_native(1, None))?;
        Ok(Self((catalog, outcome.map(|outcome| outcome.0))))
    }
}
/// Named subjects, class names and label sets extracted from the same source.
#[derive(Debug)]
pub(super) struct Declaration(pub Declarations);
impl ToJson for Declaration {
    fn to_json(&self) -> Value {
        self.0.to_json()
    }
}
impl FromJson for Declaration {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let cells = value
            .as_array()
            .ok_or_else(|| DecodeError::invalid_type(value, "declaration cache triple"))?;
        if cells.len() != 3 {
            return Err(DecodeError::invalid_value(
                value,
                "exactly three declaration cache cells",
            ));
        }
        Ok(Self((
            FromJson::from_json(&cells[0])
                .map_err(|error: DecodeError| error.within_index_native(0, None))?,
            FromJson::from_json(&cells[1])
                .map_err(|error: DecodeError| error.within_index_native(1, None))?,
            FromJson::from_json(&cells[2])
                .map_err(|error: DecodeError| error.within_index_native(2, None))?,
        )))
    }
}
#[cfg(test)]
mod tests;
