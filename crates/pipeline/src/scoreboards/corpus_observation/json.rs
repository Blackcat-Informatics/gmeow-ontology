// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native compact envelope, preserving schema admission and original ledger nodes.

use super::{Recorded, Stored};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

impl ToJson for Recorded<'_> {
    fn to_json(&self) -> Value {
        Object::new()
            .with("schema", self.schema)
            .with("query_rows", self.query_rows.to_json())
            .with("nodes", self.nodes.to_json())
            .with("claims", self.claims.to_json())
            .into()
    }
}
impl FromJson for Stored {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Stored")?;
        let stored = Self {
            schema: fields.required_with("schema", |value| {
                gmeow_errors::json::read_unsigned(value, "u32")
            })?,
            query_rows: fields.required("query_rows")?,
            nodes: fields.required("nodes")?,
            claims: fields.required("claims")?,
        };
        fields.deny_unknown()?;
        Ok(stored)
    }
}

#[cfg(test)]
mod tests;
