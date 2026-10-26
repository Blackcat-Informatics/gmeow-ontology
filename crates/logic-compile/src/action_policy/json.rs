// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Strict prepared policy records; source semantics are validated separately.
use super::{Object as PolicyObject, PreparedActionPolicy, Row};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};
use std::sync::OnceLock;

impl PreparedActionPolicy {
    /// Decode the prepared transport without loading source, hydrating rows or validating policy identity.
    /// Consumers retain their own byte-size admission and call `validate` after decoding.
    ///
    /// # Errors
    /// Refuses malformed JSON, unknown or duplicate fields, wrong tags and invalid field types.
    pub fn decode_json(bytes: &[u8]) -> Result<Self, DecodeError> {
        purrdf_lex::json::record::from_slice(bytes)
    }
}
impl ToJson for PolicyObject {
    fn to_json(&self) -> Value {
        match self {
            Self::Iri(iri) => Object::new().with("Iri", iri.to_json()).into(),
            Self::Literal {
                lexical,
                datatype,
                language,
                direction,
            } => Object::new()
                .with(
                    "Literal",
                    Value::from(
                        Object::new()
                            .with("lexical", lexical.to_json())
                            .with("datatype", datatype.to_json())
                            .with("language", language.to_json())
                            .with("direction", direction.to_json()),
                    ),
                )
                .into(),
        }
    }
}
impl FromJson for PolicyObject {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let (tag, _) =
            gmeow_errors::json::read_variant(value, "policy object", "one policy object variant")?;
        let mut fields = Record::new(value, "policy object")?;
        let object = match tag {
            "Iri" => Self::Iri(fields.required("Iri")?),
            "Literal" => fields.required_with("Literal", |value| {
                let mut fields = Record::new(value, "policy literal")?;
                let literal = Self::Literal {
                    lexical: fields.required("lexical")?,
                    datatype: fields.optional("datatype")?,
                    language: fields.optional("language")?,
                    direction: fields.optional("direction")?,
                };
                fields.deny_unknown()?;
                Ok::<_, DecodeError>(literal)
            })?,
            name => return Err(DecodeError::unknown_variant(name, &["Iri", "Literal"])),
        };
        fields.deny_unknown()?;
        Ok(object)
    }
}
::purrdf_lex::json_record!(Row as "struct Row" {
    "subject" => subject: required,
    "predicate" => predicate: required,
    "object" => object: required,
});
impl ToJson for PreparedActionPolicy {
    fn to_json(&self) -> Value {
        Object::new()
            .with("schema_version", self.schema_version.to_json())
            .with("source_path", self.source_path.to_json())
            .with("source_sha256", self.source_sha256.to_json())
            .with("source_statements", self.source_statements.to_json())
            .with("rows", self.rows.to_json())
            .with("omitted_annotations", self.omitted_annotations.to_json())
            .with("nquads", self.nquads.to_json())
            .into()
    }
}
impl FromJson for PreparedActionPolicy {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct PreparedActionPolicy")?;
        let policy = Self {
            schema_version: fields.required_with("schema_version", |value| {
                gmeow_errors::json::read_unsigned(value, "unsigned u32 schema version")
            })?,
            source_path: fields.required("source_path")?,
            source_sha256: fields.required("source_sha256")?,
            source_statements: fields.required("source_statements")?,
            rows: fields.required("rows")?,
            omitted_annotations: fields.required("omitted_annotations")?,
            nquads: fields.required("nquads")?,
            dataset: OnceLock::new(),
        };
        fields.deny_unknown()?;
        Ok(policy)
    }
}
#[cfg(test)]
mod tests;
