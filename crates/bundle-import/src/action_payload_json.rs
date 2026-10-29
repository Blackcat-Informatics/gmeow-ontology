// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete native receipt payloads; hydration preserves stored fields verbatim.

use super::CorpusArtifactPayload;
use gmeow_errors::json::read_unsigned;
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record},
};

purrdf_lex::json_record! { impl ToJson for CorpusArtifactPayload {
    "schema_version" => schema_version,
    "name" => name,
    "source_sha256" => source_sha256,
}}
impl FromJson for CorpusArtifactPayload {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct CorpusArtifactPayload")?;
        let result = Self {
            schema_version: fields
                .required_with("schema_version", |value| read_unsigned::<u32>(value, "u32"))?,
            name: fields.required("name")?,
            source_sha256: fields.required("source_sha256")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}
