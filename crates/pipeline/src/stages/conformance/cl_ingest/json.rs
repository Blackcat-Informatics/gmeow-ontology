// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete CL ingestion outcomes, using the native owners of terms and lineage.

use super::{Annotated, Observation, ObservationOutcome};
use gmeow_errors::json::{read_recorded_result, recorded_result_to_json};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

::purrdf_lex::json_record!(impl ToJson for Annotated {
    "certification" => certification,
    "quads" => quads,
});
impl FromJson for Annotated {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Annotated")?;
        Ok(Self {
            certification: fields.required("certification")?,
            quads: fields.required("quads")?,
        })
    }
}

impl ToJson for Observation {
    fn to_json(&self) -> Value {
        Object::new()
            .with("diagnostics", self.diagnostics.to_json())
            .with("plain", recorded_result_to_json(&self.plain))
            .with("annotated", recorded_result_to_json(&self.annotated))
            .into()
    }
}

impl FromJson for Observation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observation")?;
        Ok(Self {
            diagnostics: fields.required("diagnostics")?,
            plain: fields.required_with("plain", |value| {
                read_recorded_result(value, "CL plain result", "one CL plain result variant")
            })?,
            annotated: fields.required_with("annotated", |value| {
                read_recorded_result(
                    value,
                    "CL annotated result",
                    "one CL annotated result variant",
                )
            })?,
        })
    }
}

impl ToJson for ObservationOutcome {
    fn to_json(&self) -> Value {
        recorded_result_to_json(&self.0)
    }
}

impl FromJson for ObservationOutcome {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        read_recorded_result(
            value,
            "CL ingestion outcome",
            "one CL ingestion outcome variant",
        )
        .map(Self)
    }
}

#[cfg(test)]
mod tests;
