// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native standalone verdict products and authenticated action receipt payloads. Named records require every field, reject
//! known duplicates and ignore unknown fields; positional records are refused.

use super::{SliceSpecTaskIdentity, SliceSpecVerdict, SpecTaskVerdict};
use gmeow_errors::json::read_unsigned as unsigned;
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

impl ToJson for SliceSpecVerdict {
    fn to_json(&self) -> Value {
        Object::new()
            .with("schema_version", self.schema_version)
            .with("input_files", self.input_files)
            .with("competency_files", self.competency_files)
            .with("structural_files", self.structural_files)
            .with("conformance_files", self.conformance_files)
            .with("flagship_manifests", self.flagship_manifests)
            .with("task_receipts", self.task_receipts.to_json())
            .into()
    }
}

impl FromJson for SliceSpecVerdict {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct SliceSpecVerdict")?;
        Ok(Self {
            schema_version: record.required_with("schema_version", |v| unsigned(v, "u32"))?,
            input_files: record.required_with("input_files", |v| unsigned(v, "usize"))?,
            competency_files: record.required_with("competency_files", |v| unsigned(v, "usize"))?,
            structural_files: record.required_with("structural_files", |v| unsigned(v, "usize"))?,
            conformance_files: record
                .required_with("conformance_files", |v| unsigned(v, "usize"))?,
            flagship_manifests: record
                .required_with("flagship_manifests", |v| unsigned(v, "usize"))?,
            task_receipts: record.required("task_receipts")?,
        })
    }
}

impl ToJson for SliceSpecTaskIdentity {
    fn to_json(&self) -> Value {
        Object::new()
            .with("kind", self.kind.to_json())
            .with("path", self.path.to_json())
            .with("action_key", self.action_key.to_json())
            .with("receipt_digest", self.receipt_digest.to_json())
            .into()
    }
}

impl FromJson for SliceSpecTaskIdentity {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct SliceSpecTaskIdentity")?;
        Ok(Self {
            kind: record.required("kind")?,
            path: record.required("path")?,
            action_key: record.required("action_key")?,
            receipt_digest: record.required("receipt_digest")?,
        })
    }
}

impl ToJson for SpecTaskVerdict {
    fn to_json(&self) -> Value {
        Object::new()
            .with("schema_version", self.schema_version)
            .with("kind", self.kind.to_json())
            .with("path", self.path.to_json())
            .into()
    }
}

impl FromJson for SpecTaskVerdict {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct SpecTaskVerdict")?;
        Ok(Self {
            schema_version: record.required_with("schema_version", |v| unsigned(v, "u32"))?,
            kind: record.required("kind")?,
            path: record.required("path")?,
        })
    }
}

#[cfg(test)]
mod tests;
