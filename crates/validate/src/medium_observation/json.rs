// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native medium records retain observation strings and original collection policies.
use super::{Node, Observations, Value as ObservedValue};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};
impl ToJson for Observations {
    fn to_json(&self) -> Value {
        Object::new()
            .with("source_path", self.source_path.to_json())
            .with("source_digest", self.source_digest.to_json())
            .with("dictionaries", self.dictionaries.to_json())
            .with("corpora", self.corpora.to_json())
            .with("splits", self.splits.to_json())
            .with("schemas", self.schemas.to_json())
            .with("classes", self.classes.to_json())
            .with("gmn_terms", self.gmn_terms.to_json())
            .with("shacl_terms", self.shacl_terms.to_json())
            .into()
    }
}
impl FromJson for Observations {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observations")?;
        Ok(Self {
            source_path: fields.required("source_path")?,
            source_digest: fields.required("source_digest")?,
            dictionaries: fields.required("dictionaries")?,
            corpora: fields.required("corpora")?,
            splits: fields.required("splits")?,
            schemas: fields.required("schemas")?,
            classes: fields.required("classes")?,
            gmn_terms: fields.required("gmn_terms")?,
            shacl_terms: fields.required("shacl_terms")?,
        })
    }
}
impl ToJson for Node {
    fn to_json(&self) -> Value {
        Object::new()
            .with("name", self.name.to_json())
            .with("values", self.values.to_json())
            .into()
    }
}
impl FromJson for Node {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Node")?;
        Ok(Self {
            name: fields.required("name")?,
            values: fields.required("values")?,
        })
    }
}
impl ToJson for ObservedValue {
    fn to_json(&self) -> Value {
        match self {
            Self::Iri(text) => Object::new().with("Iri", text.to_json()).into(),
            Self::Literal(text) => Object::new().with("Literal", text.to_json()).into(),
            Self::Other(text) => Object::new().with("Other", text.to_json()).into(),
        }
    }
}
impl FromJson for ObservedValue {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let (tag, _) = gmeow_errors::json::read_variant(
            value,
            "observed medium value",
            "one observed medium variant",
        )?;
        let mut fields = Record::new(value, "observed medium value")?;
        match tag {
            "Iri" => fields.required("Iri").map(Self::Iri),
            "Literal" => fields.required("Literal").map(Self::Literal),
            "Other" => fields.required("Other").map(Self::Other),
            name => Err(DecodeError::unknown_variant(
                name,
                &["Iri", "Literal", "Other"],
            )),
        }
    }
}
#[cfg(test)]
mod tests;
