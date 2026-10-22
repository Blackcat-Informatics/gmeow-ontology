// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native receipt products; the separate fixture selector remains a Serde owner.
//! Named records require every declared field, reject known duplicates and ignore
//! unknown fields. Positional records are refused. Field order is digest material.

use super::{ImportReceipt, ReceiptEnvelope};
use gmeow_errors::json::read_unsigned as unsigned;
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

impl ToJson for ImportReceipt {
    fn to_json(&self) -> Value {
        Object::new()
            .with("schema_version", self.schema_version)
            .with("action_key", self.action_key.to_json())
            .with("build_fingerprint", self.build_fingerprint.to_json())
            .with("codec", self.codec.to_json())
            .with("source_digest", self.source_digest.to_json())
            .with("source_bytes", self.source_bytes)
            .with("pack_digest", self.pack_digest.to_json())
            .with("pack_bytes", self.pack_bytes)
            .with("dataset_quads", self.dataset_quads)
            .with("named_graphs", self.named_graphs)
            .into()
    }
}

impl FromJson for ImportReceipt {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct ImportReceipt")?;
        Ok(Self {
            schema_version: record.required_with("schema_version", |v| unsigned(v, "u32"))?,
            action_key: record.required("action_key")?,
            build_fingerprint: record.required("build_fingerprint")?,
            codec: record.required("codec")?,
            source_digest: record.required("source_digest")?,
            source_bytes: record.required_with("source_bytes", |v| unsigned(v, "u64"))?,
            pack_digest: record.required("pack_digest")?,
            pack_bytes: record.required_with("pack_bytes", |v| unsigned(v, "u64"))?,
            dataset_quads: record.required_with("dataset_quads", |v| unsigned(v, "u64"))?,
            named_graphs: record.required_with("named_graphs", |v| unsigned(v, "u64"))?,
        })
    }
}

impl ToJson for ReceiptEnvelope {
    fn to_json(&self) -> Value {
        Object::new()
            .with("receipt_digest", self.receipt_digest.to_json())
            .with("receipt", self.receipt.to_json())
            .into()
    }
}

impl FromJson for ReceiptEnvelope {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct ReceiptEnvelope")?;
        Ok(Self {
            receipt_digest: record.required("receipt_digest")?,
            receipt: record.required("receipt")?,
        })
    }
}

#[cfg(test)]
mod tests;
