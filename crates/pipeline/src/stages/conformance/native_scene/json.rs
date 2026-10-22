// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Only the standalone alpha-drift transport; grounding scene payloads stay separate.
use super::{AuxiliaryObservation, AuxiliaryOutcome};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

impl ToJson for AuxiliaryObservation {
    fn to_json(&self) -> Value {
        Object::new()
            .with("source_path", self.source_path.to_json())
            .with("source_digest", self.source_digest.to_json())
            .with("findings", self.findings.to_json())
            .into()
    }
}
impl FromJson for AuxiliaryObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct AuxiliaryObservation")?;
        Ok(Self {
            source_path: fields.required("source_path")?,
            source_digest: fields.required("source_digest")?,
            findings: fields.required("findings")?,
        })
    }
}
impl ToJson for AuxiliaryOutcome {
    fn to_json(&self) -> Value {
        gmeow_errors::json::recorded_result_to_json(&self.0)
    }
}
impl FromJson for AuxiliaryOutcome {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        gmeow_errors::json::read_recorded_result(
            value,
            "auxiliary scene Result",
            "one auxiliary scene Result variant",
        )
        .map(Self)
    }
}
#[cfg(test)]
mod tests;
