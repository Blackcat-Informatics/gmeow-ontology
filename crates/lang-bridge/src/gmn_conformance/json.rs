// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact positive-vector witnesses, including null-valued successful checks.
use super::PositiveObservation;
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

struct NullWitness;
impl FromJson for NullWitness {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        gmeow_errors::json::read_null(value, "null unit witness").map(|()| Self)
    }
}
impl ToJson for PositiveObservation {
    fn to_json(&self) -> Value {
        let reconstructed: Value = match &self.reconstructed {
            Ok(value) => Object::new().with("Ok", value.to_json()).into(),
            Err(error) => Object::new().with("Err", error.to_json()).into(),
        };
        let per_claim: Value = match &self.per_claim {
            Ok(()) => Object::new().with("Ok", Value::Null).into(),
            Err(error) => Object::new().with("Err", error.to_json()).into(),
        };
        let idempotence: Value = match &self.idempotence {
            Ok(()) => Object::new().with("Ok", Value::Null).into(),
            Err(error) => Object::new().with("Err", error.to_json()).into(),
        };
        Object::new()
            .with("text", self.text.to_json())
            .with("source", self.source.to_json())
            .with("reconstructed", reconstructed)
            .with("content_digest", self.content_digest.to_json())
            .with("per_claim", per_claim)
            .with("idempotence", idempotence)
            .into()
    }
}
impl FromJson for PositiveObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct PositiveObservation")?;
        Ok(Self {
            text: fields.required("text")?,
            source: fields.required("source")?,
            reconstructed: fields.required_with("reconstructed", |value| {
                gmeow_errors::json::read_recorded_result(
                    value,
                    "reconstruction Result",
                    "one reconstruction Result variant",
                )
            })?,
            content_digest: fields.required("content_digest")?,
            per_claim: fields.required_with("per_claim", |value| {
                gmeow_errors::json::read_recorded_result::<NullWitness>(
                    value,
                    "per-claim Result",
                    "one per-claim Result variant",
                )
                .map(|result| result.map(|_| ()))
            })?,
            idempotence: fields.required_with("idempotence", |value| {
                gmeow_errors::json::read_recorded_result::<NullWitness>(
                    value,
                    "idempotence Result",
                    "one idempotence Result variant",
                )
                .map(|result| result.map(|_| ()))
            })?,
        })
    }
}
#[cfg(test)]
mod tests;
