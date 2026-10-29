// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete native receipt payloads; hydration preserves stored fields verbatim.

use super::{Payload, SourceArtifactOrigin};
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record},
};

::purrdf_lex::json_record! { impl ToJson for SourceArtifactOrigin {
    "stage" => stage,
    "action_key" => action_key,
    "receipt_digest" => receipt_digest,
    "product_digest" => product_digest,
    "implementation" => implementation,
}}
impl FromJson for SourceArtifactOrigin {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct SourceArtifactOrigin")?;
        let result = Self {
            stage: fields.required("stage")?,
            action_key: fields.required("action_key")?,
            receipt_digest: fields.required("receipt_digest")?,
            product_digest: fields.required("product_digest")?,
            implementation: fields.required("implementation")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}

::purrdf_lex::json_record! { impl ToJson for Payload {
    "source" => source,
    "artifact" => artifact,
    "digest" => digest,
    "bytes" => bytes,
}}
impl FromJson for Payload {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Payload")?;
        let result = Self {
            source: fields.required("source")?,
            artifact: fields.required("artifact")?,
            digest: fields.required("digest")?,
            bytes: fields.required_with("bytes", |value| {
                value
                    .as_u64()
                    .ok_or_else(|| DecodeError::invalid_value(value, "u64"))
            })?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}
