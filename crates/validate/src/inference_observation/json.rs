// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact ordered records for the shared inference-validation observations.
use super::{Control, Observations, ResultMessage};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};
impl ToJson for Observations {
    fn to_json(&self) -> Value {
        Object::new()
            .with("profile", self.profile.to_json())
            .with("sources", self.sources.to_json())
            .with("controls", self.controls.to_json())
            .into()
    }
}
impl FromJson for Observations {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observations")?;
        Ok(Self {
            profile: fields.required("profile")?,
            sources: fields.required("sources")?,
            controls: fields.required("controls")?,
        })
    }
}
impl ToJson for Control {
    fn to_json(&self) -> Value {
        Object::new()
            .with("input_digest", self.input_digest.to_json())
            .with("results", self.results.to_json())
            .into()
    }
}
impl FromJson for Control {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Control")?;
        Ok(Self {
            input_digest: fields.required("input_digest")?,
            results: fields.required("results")?,
        })
    }
}
impl ToJson for ResultMessage {
    fn to_json(&self) -> Value {
        Object::new()
            .with("severity", self.severity.to_json())
            .with("message", self.message.to_json())
            .into()
    }
}
impl FromJson for ResultMessage {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ResultMessage")?;
        Ok(Self {
            severity: fields.required("severity")?,
            message: fields.optional("message")?,
        })
    }
}
#[cfg(test)]
mod tests;
