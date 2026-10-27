// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Selected projection observations, retaining every terminal byte string and cause.

use super::{Observations, ProjectionObservation};
use gmeow_errors::json::{read_recorded_result, recorded_result_to_json};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

#[cfg(test)]
mod reader;
#[cfg(test)]
pub(super) use reader::from_bytes;

pub(super) fn to_bytes(observations: &Observations) -> Vec<u8> {
    let cases: Object = observations
        .iter()
        .map(|(name, outcome)| (name.as_str(), recorded_result_to_json(outcome)))
        .collect();
    purrdf_lex::json::write_compact(&cases.into()).into_bytes()
}

impl ToJson for ProjectionObservation {
    fn to_json(&self) -> Value {
        Object::new()
            .with("diagnostics", self.diagnostics.to_json())
            .with("projections", recorded_result_to_json(&self.projections))
            .into()
    }
}
impl FromJson for ProjectionObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ProjectionObservation")?;
        Ok(Self {
            diagnostics: fields.required("diagnostics")?,
            projections: fields.required_with("projections", |value| {
                read_recorded_result(
                    value,
                    "projection output outcome",
                    "one projection output outcome variant",
                )
            })?,
        })
    }
}
