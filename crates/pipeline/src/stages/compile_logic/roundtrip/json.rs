// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native transport of the exact three ordered dialect observations and refusals.

use super::{DialectObservation, Observation};
use gmeow_errors::json::{read_recorded_result, recorded_result_to_json};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

struct Dialects([DialectObservation; 3]);

impl FromJson for Dialects {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let dialects = Vec::<DialectObservation>::from_json(value)?;
        dialects
            .try_into()
            .map(Self)
            .map_err(|_| DecodeError::invalid_value(value, "exactly three dialect observations"))
    }
}

struct UnitSuccess;

impl FromJson for UnitSuccess {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        gmeow_errors::json::read_null(value, "null unit success").map(|()| Self)
    }
}

impl ToJson for Observation {
    fn to_json(&self) -> Value {
        Object::new()
            .with("source", self.source.to_json())
            .with("dialects", recorded_result_to_json(&self.dialects))
            .into()
    }
}

impl FromJson for Observation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observation")?;
        let source = fields.required("source")?;
        let dialects: Result<Dialects, gmeow_errors::RecordedDiag> =
            fields.required_with("dialects", |value| {
                read_recorded_result(
                    value,
                    "Common Logic dialect outcome",
                    "one Common Logic dialect outcome variant",
                )
            })?;
        Ok(Self {
            source,
            dialects: dialects.map(|dialects| dialects.0),
        })
    }
}

impl ToJson for DialectObservation {
    fn to_json(&self) -> Value {
        // The pinned record traits have no unit codec. The success carrier is
        // exactly JSON null; failures use the complete native diagnostic owner.
        let result: Value = match &self.result {
            Ok(()) => Object::new().with("Ok", Value::Null).into(),
            Err(error) => Object::new().with("Err", error.to_json()).into(),
        };
        Object::new()
            .with("dialect", self.dialect.to_json())
            .with("result", result)
            .into()
    }
}

impl FromJson for DialectObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct DialectObservation")?;
        let dialect = fields.required("dialect")?;
        let result: Result<UnitSuccess, gmeow_errors::RecordedDiag> =
            fields.required_with("result", |value| {
                read_recorded_result(
                    value,
                    "Common Logic dialect result",
                    "one Common Logic dialect result variant",
                )
            })?;
        Ok(Self {
            dialect,
            result: result.map(|UnitSuccess| ()),
        })
    }
}
