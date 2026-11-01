// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Typed class-source observation transport; mixed execution output keeps its owner.

use super::SourceAdmissionObservation;
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record, items_with},
};

::purrdf_lex::json_record!(impl ToJson for SourceAdmissionObservation {
    "input_blake3" => input_blake3,
    "admission" => admission,
});
impl FromJson for SourceAdmissionObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct SourceAdmissionObservation")?;
        Ok(Self {
            input_blake3: fields.required_with("input_blake3", |value| {
                let bytes: Vec<u8> = items_with(value, |byte| -> Result<_, DecodeError> {
                    gmeow_errors::json::read_unsigned(byte, "u8 input digest byte")
                })?;
                bytes
                    .try_into()
                    .map_err(|_| DecodeError::invalid_value(value, "exactly 32 input digest bytes"))
            })?,
            admission: fields.required("admission")?,
        })
    }
}

#[cfg(test)]
mod tests;
