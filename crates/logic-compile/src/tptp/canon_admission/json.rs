// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The publication envelope is a closed named record around complete observations.

use super::CanonAdmissionPublication;
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record},
};

::purrdf_lex::json_record!(impl ToJson for CanonAdmissionPublication {
    "schema_version" => schema_version,
    "admission" => admission,
});

impl FromJson for CanonAdmissionPublication {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct CanonAdmissionPublication")?;
        let publication = Self {
            schema_version: fields.required_with("schema_version", |value| {
                gmeow_errors::json::read_unsigned::<u32>(value, "an unsigned schema version")
            })?,
            admission: fields.required("admission")?,
        };
        fields.deny_unknown()?;
        Ok(publication)
    }
}
