// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Data-only transport of computed envelope strings, without recomputing geometry.

use super::WorkedEnvelope;
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record, items_with},
};

::purrdf_lex::json_record!(impl ToJson for WorkedEnvelope {
    "intensity" => intensity,
    "dimensions" => dimensions,
});

impl FromJson for WorkedEnvelope {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct WorkedEnvelope")?;
        Ok(Self {
            intensity: fields.required("intensity")?,
            dimensions: fields.required_with("dimensions", |value| {
                items_with(value, |row| {
                    gmeow_errors::json::read_string_row::<2>(row)
                        .map(|[dimension, value]| (dimension, value))
                })
            })?,
        })
    }
}
