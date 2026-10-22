// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Verbatim gUFO term surfaces and exact two-cell subject/object rows.
use super::{ObservedTerm, SourceObservation};
use purrdf_lex::json::record::Within;
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};
use std::collections::BTreeMap;
impl ToJson for ObservedTerm {
    fn to_json(&self) -> Value {
        match self {
            Self::Iri(value) => Object::new().with("Iri", value.to_json()),
            Self::Literal(value) => Object::new().with("Literal", value.to_json()),
            Self::BlankNode(value) => Object::new().with("BlankNode", value.to_json()),
            Self::Triple(value) => Object::new().with("Triple", value.to_json()),
        }
        .into()
    }
}
impl FromJson for ObservedTerm {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let (tag, _) =
            gmeow_errors::json::read_variant(value, "ObservedTerm", "one ObservedTerm variant")?;
        let mut fields = Record::new(value, "ObservedTerm")?;
        match tag {
            "Iri" => fields.required("Iri").map(Self::Iri),
            "Literal" => fields.required("Literal").map(Self::Literal),
            "BlankNode" => fields.required("BlankNode").map(Self::BlankNode),
            "Triple" => fields.required("Triple").map(Self::Triple),
            name => Err(DecodeError::unknown_variant(
                name,
                &["Iri", "Literal", "BlankNode", "Triple"],
            )),
        }
    }
}
struct TermPair((String, ObservedTerm));
impl FromJson for TermPair {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let pair = value
            .as_array()
            .ok_or_else(|| DecodeError::invalid_type(value, "two-element term pair"))?;
        if pair.len() != 2 {
            return Err(DecodeError::invalid_value(
                value,
                "exactly two term-pair elements",
            ));
        }
        let subject =
            String::from_json(&pair[0]).map_err(|error| error.within_index_native(0, None))?;
        let object = ObservedTerm::from_json(&pair[1])
            .map_err(|error| error.within_index_native(1, None))?;
        Ok(Self((subject, object)))
    }
}
impl ToJson for SourceObservation {
    fn to_json(&self) -> Value {
        Object::new()
            .with("source_path", self.source_path.to_json())
            .with("default_quad_count", self.default_quad_count)
            .with("subjects", self.subjects.to_json())
            .with("pairs", self.pairs.to_json())
            .into()
    }
}
impl FromJson for SourceObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct SourceObservation")?;
        let source_path = fields.required("source_path")?;
        let default_quad_count = fields.required_with("default_quad_count", |value| {
            gmeow_errors::json::read_unsigned(value, "usize quad count")
        })?;
        let subjects = fields.required("subjects")?;
        let pairs: BTreeMap<String, Vec<TermPair>> = fields.required("pairs")?;
        Ok(Self {
            source_path,
            default_quad_count,
            subjects,
            pairs: pairs
                .into_iter()
                .map(|(key, pairs)| (key, pairs.into_iter().map(|pair| pair.0).collect()))
                .collect(),
        })
    }
}
#[cfg(test)]
mod tests;
