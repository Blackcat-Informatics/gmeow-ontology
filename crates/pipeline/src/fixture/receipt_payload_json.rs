// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete native receipt payloads; hydration preserves stored fields verbatim.

use super::SettledSourceLoadWitness;
use gmeow_errors::json::read_unsigned;
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record},
};

::purrdf_lex::json_record! { impl ToJson for SettledSourceLoadWitness {
    "schema_version" => schema_version,
    "source_action_key" => source_action_key,
    "source_receipt_digest" => source_receipt_digest,
    "source_product_digest" => source_product_digest,
    "settled_product_digest" => settled_product_digest,
}}
impl FromJson for SettledSourceLoadWitness {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct SettledSourceLoadWitness")?;
        let result = Self {
            schema_version: fields
                .required_with("schema_version", |value| read_unsigned::<u32>(value, "u32"))?,
            source_action_key: fields.required("source_action_key")?,
            source_receipt_digest: fields.required("source_receipt_digest")?,
            source_product_digest: fields.required("source_product_digest")?,
            settled_product_digest: fields.required("settled_product_digest")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}
