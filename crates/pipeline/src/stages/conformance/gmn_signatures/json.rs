// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Signature row transport preserves zero and strict unsigned token admission.
use super::{CaseObservation, Observations};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

#[derive(Debug)]
pub(super) struct RowCount(pub(super) usize);
impl ToJson for RowCount {
    fn to_json(&self) -> Value {
        self.0.to_json()
    }
}
impl FromJson for RowCount {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        gmeow_errors::json::read_unsigned(value, "usize row count").map(Self)
    }
}
impl ToJson for Observations {
    fn to_json(&self) -> Value {
        Object::new()
            .with("modules", self.modules.to_json())
            .with("cases", self.cases.to_json())
            .into()
    }
}
impl FromJson for Observations {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observations")?;
        Ok(Self {
            modules: fields.required("modules")?,
            cases: fields.required("cases")?,
        })
    }
}
impl ToJson for CaseObservation {
    fn to_json(&self) -> Value {
        let rows: Value = match &self.rows {
            Ok(value) => Object::new().with("Ok", *value).into(),
            Err(error) => Object::new().with("Err", error.to_json()).into(),
        };
        Object::new()
            .with("query_path", self.query_path.to_json())
            .with("query_digest", self.query_digest.to_json())
            .with("injection_digest", self.injection_digest.to_json())
            .with("rows", rows)
            .into()
    }
}
impl FromJson for CaseObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct CaseObservation")?;
        Ok(Self {
            query_path: fields.required("query_path")?,
            query_digest: fields.required("query_digest")?,
            injection_digest: fields.optional("injection_digest")?,
            rows: fields.required_with("rows", |value| {
                gmeow_errors::json::read_recorded_result::<RowCount>(
                    value,
                    "row Result",
                    "one row Result variant",
                )
                .map(|result| result.map(|count| count.0))
            })?,
        })
    }
}
#[cfg(test)]
mod tests;
