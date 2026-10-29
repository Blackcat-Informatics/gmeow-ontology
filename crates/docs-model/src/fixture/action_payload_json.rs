// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete native receipt payloads; hydration preserves stored fields verbatim.

use super::DocsActionPayload;
use gmeow_errors::json::read_unsigned;
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record},
};

::purrdf_lex::json_record! { impl ToJson for DocsActionPayload {
    "schema_version" => schema_version,
    "artifact" => artifact,
    "input_digest" => input_digest,
}}
impl FromJson for DocsActionPayload {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct DocsActionPayload")?;
        let result = Self {
            schema_version: fields
                .required_with("schema_version", |value| read_unsigned::<u32>(value, "u32"))?,
            artifact: fields.required("artifact")?,
            input_digest: fields.required("input_digest")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}
