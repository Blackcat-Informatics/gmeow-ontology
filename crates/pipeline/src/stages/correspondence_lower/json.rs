// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete original-source presence and computed-envelope observation transport.

use super::WorkedEnvelopeObservation;
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record},
};

::purrdf_lex::json_record!(impl ToJson for WorkedEnvelopeObservation {
    "observation_present" => observation_present,
    "envelope" => envelope,
});

impl FromJson for WorkedEnvelopeObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct WorkedEnvelopeObservation")?;
        Ok(Self {
            observation_present: fields.required("observation_present")?,
            envelope: fields.required("envelope")?,
        })
    }
}

#[path = "json/tests.rs"]
#[cfg(test)]
mod tests;
