// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete native receipt payloads; hydration preserves stored fields verbatim.

use super::RenderActionPayload;
use gmeow_errors::json::read_unsigned;
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record},
};

::purrdf_lex::json_record! { impl ToJson for RenderActionPayload {
    "schema_version" => schema_version,
    "artifact" => artifact,
    "language" => language,
    "model_receipt_digest" => model_receipt_digest,
}}
impl FromJson for RenderActionPayload {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct RenderActionPayload")?;
        let result = Self {
            schema_version: fields
                .required_with("schema_version", |value| read_unsigned::<u32>(value, "u32"))?,
            artifact: fields.required("artifact")?,
            language: fields.required("language")?,
            model_receipt_digest: fields.required("model_receipt_digest")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}
