// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Original glossary coordinates with complete retained native object terms.
use super::{Observations, Triple};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, ToJson, Within},
};
use std::collections::BTreeMap;

impl ToJson for Triple {
    fn to_json(&self) -> Value {
        Value::Array(vec![
            self.0.to_json(),
            self.1.to_json(),
            gmeow_logic::term_serde::to_json(&self.2),
        ])
    }
}
impl FromJson for Triple {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let items = value
            .as_array()
            .ok_or_else(|| DecodeError::invalid_type(value, "three-element glossary triple"))?;
        if items.len() != 3 {
            return Err(DecodeError::invalid_value(
                value,
                "exactly three glossary triple elements",
            ));
        }
        let subject =
            String::from_json(&items[0]).map_err(|error| error.within_index_native(0, None))?;
        let predicate =
            String::from_json(&items[1]).map_err(|error| error.within_index_native(1, None))?;
        let object = gmeow_logic::term_serde::from_json(&items[2])
            .map_err(|error| error.within_index_native(2, None))?;
        Ok(Self(subject, predicate, object))
    }
}
struct SourceOutcome(Result<Vec<Triple>, gmeow_errors::RecordedDiag>);
impl FromJson for SourceOutcome {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        gmeow_errors::json::read_recorded_result(
            value,
            "glossary source Result",
            "one glossary source Result variant",
        )
        .map(Self)
    }
}
impl ToJson for Observations {
    fn to_json(&self) -> Value {
        let values: Object = self
            .0
            .iter()
            .map(|(path, outcome)| {
                let value = gmeow_errors::json::recorded_result_to_json(outcome);
                (path.as_str(), value)
            })
            .collect();
        values.into()
    }
}
impl FromJson for Observations {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let values = BTreeMap::<String, SourceOutcome>::from_json(value)?;
        Ok(Self(
            values
                .into_iter()
                .map(|(path, value)| (path, value.0))
                .collect(),
        ))
    }
}
#[cfg(test)]
mod tests;
