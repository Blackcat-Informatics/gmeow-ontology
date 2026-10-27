// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete selected formula observations and both recorded outcome levels.

use super::FormulaObservation;
use gmeow_errors::json::{read_recorded_result, recorded_result_to_json};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

#[cfg(test)]
mod reader;
#[cfg(test)]
pub(super) use reader::from_bytes;

impl ToJson for FormulaObservation {
    fn to_json(&self) -> Value {
        Object::new()
            .with("formulas", self.formulas.to_json())
            .with("diagnostics", self.diagnostics.to_json())
            .with("selected_formulas", self.selected_formulas.to_json())
            .with("lowering", recorded_result_to_json(&self.lowering))
            .into()
    }
}
impl FromJson for FormulaObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct FormulaObservation")?;
        Ok(Self {
            formulas: fields.required("formulas")?,
            diagnostics: fields.required("diagnostics")?,
            selected_formulas: fields.required_with("selected_formulas", |value| {
                gmeow_errors::json::read_unsigned(value, "usize formula selection count")
            })?,
            lowering: fields.required_with("lowering", |value| {
                read_recorded_result(
                    value,
                    "formula lowering outcome",
                    "one formula lowering outcome variant",
                )
            })?,
        })
    }
}
