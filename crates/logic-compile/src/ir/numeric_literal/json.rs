// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Numeric JSON uses the existing exact RDF literal wire and checked range owner.
use super::NumericLiteral;
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, ToJson},
};
impl<const UNIT_INTERVAL: bool> ToJson for NumericLiteral<UNIT_INTERVAL> {
    fn to_json(&self) -> Value {
        super::super::literal_serde::single::to_json(self.literal())
    }
}
impl<const UNIT_INTERVAL: bool> FromJson for NumericLiteral<UNIT_INTERVAL> {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        Self::new(super::super::literal_serde::single::from_json(value)?)
            .map_err(DecodeError::custom)
    }
}
#[cfg(test)]
mod tests;
