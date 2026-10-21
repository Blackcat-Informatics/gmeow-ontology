// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete findings payload; receipt and input-key encodings are separate owners.

use super::CachedResult;
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

impl ToJson for CachedResult {
    fn to_json(&self) -> Value {
        let mut object = Object::new();
        object.insert("findings", self.findings.to_json());
        Value::Object(object)
    }
}

impl FromJson for CachedResult {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct CachedResult")?;
        Ok(Self {
            findings: record.required("findings")?,
        })
    }
}

#[cfg(test)]
mod tests;
