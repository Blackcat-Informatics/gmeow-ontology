// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native authenticated receipt envelopes. Digest canonicalization is separate
//! from the declaration-order, two-space publication encoding.

use super::{ActionCacheError, ActionKey, ActionReceipt, BlobRef, ReceiptEnvelope};
use purrdf_lex::json::{
    self, Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

/// An action with no semantic receipt payload. Only JSON null is admitted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EmptyPayload;

impl ToJson for EmptyPayload {
    fn to_json(&self) -> Value {
        Value::Null
    }
}
impl FromJson for EmptyPayload {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        if matches!(value, Value::Null) {
            Ok(Self)
        } else {
            Err(DecodeError::invalid_type(value, "null"))
        }
    }
}

pub(super) fn decode_error(error: DecodeError) -> ActionCacheError {
    ActionCacheError::message(format!("action cache JSON: {error}"))
}

pub(super) fn encode_envelope<P: ToJson>(
    envelope: &ReceiptEnvelope<P>,
) -> Result<Vec<u8>, ActionCacheError> {
    let value = envelope.to_json();
    let mut resident = purrdf_lex::allocation::Resident;
    let mut memory = purrdf_lex::allocation::Memory::new(&mut resident);
    json::write_with_memory(&value, json::Format::PRETTY, &mut memory)
        .map(String::into_bytes)
        .map_err(|error| ActionCacheError::message(format!("action cache JSON: {error}")))
}

pub(super) fn schema_version(record: &mut Record<'_>) -> Result<u32, DecodeError> {
    record.required_with("schema_version", |value| {
        value
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| DecodeError::invalid_value(value, "an unsigned 32-bit schema version"))
    })
}

impl ToJson for ActionKey {
    fn to_json(&self) -> Value {
        self.0.to_json()
    }
}
impl FromJson for ActionKey {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        Ok(Self(String::from_json(value)?))
    }
}

::purrdf_lex::json_record! { impl ToJson for BlobRef {
    "digest" => digest, "bytes" => bytes
}}
impl FromJson for BlobRef {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct BlobRef")?;
        let result = Self {
            digest: fields.required("digest")?,
            bytes: fields.required_with("bytes", |value| {
                value
                    .as_u64()
                    .ok_or_else(|| DecodeError::invalid_value(value, "an unsigned byte count"))
            })?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}

impl<P: ToJson> ToJson for ActionReceipt<P> {
    fn to_json(&self) -> Value {
        Object::new()
            .with("schema_version", self.schema_version.to_json())
            .with("action_key", self.action_key.to_json())
            .with("context", self.context.to_json())
            .with("product_digest", self.product_digest.to_json())
            .with("product_blob", self.product_blob.to_json())
            .with("payload", self.payload.to_json())
            .into()
    }
}
impl<P: FromJson> FromJson for ActionReceipt<P> {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ActionReceipt")?;
        let result = Self {
            schema_version: schema_version(&mut fields)?,
            action_key: fields.required("action_key")?,
            context: fields.required("context")?,
            product_digest: fields.required("product_digest")?,
            product_blob: fields.required("product_blob")?,
            payload: fields.required("payload")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}
impl<P: ToJson> ToJson for ReceiptEnvelope<P> {
    fn to_json(&self) -> Value {
        Object::new()
            .with("receipt_digest", self.receipt_digest.to_json())
            .with("receipt", self.receipt.to_json())
            .into()
    }
}
impl<P: FromJson> FromJson for ReceiptEnvelope<P> {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ReceiptEnvelope")?;
        let result = Self {
            receipt_digest: fields.required("receipt_digest")?,
            receipt: fields.required("receipt")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}
