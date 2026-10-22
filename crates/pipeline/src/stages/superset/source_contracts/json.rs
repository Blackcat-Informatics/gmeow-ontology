// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native source-contract transport. Dispatch graph admission remains in hydration.

use super::{Definition, FanoutFamily, Observation, OwnedForm, OwnedRule};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};
use std::collections::BTreeSet;

impl ToJson for Observation {
    fn to_json(&self) -> Value {
        Object::new()
            .with("source_path", self.source_path.to_json())
            .with("source_digest", self.source_digest.to_json())
            .with("fanout_rules", self.fanout_rules.to_json())
            .with("expected_outputs", self.expected_outputs.to_json())
            .with("definitions", self.definitions.to_json())
            .into()
    }
}
impl FromJson for Observation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observation")?;
        Ok(Self {
            source_path: fields.required("source_path")?,
            source_digest: fields.required("source_digest")?,
            fanout_rules: fields.required("fanout_rules")?,
            expected_outputs: fields.required("expected_outputs")?,
            definitions: fields.required("definitions")?,
        })
    }
}
impl ToJson for Definition {
    fn to_json(&self) -> Value {
        let values: Value = match &self.values {
            Ok(values) => Object::new().with("Ok", values.to_json()).into(),
            Err(error) => Object::new().with("Err", error.to_json()).into(),
        };
        Object::new()
            .with("text", self.text.to_json())
            .with("values", values)
            .into()
    }
}
impl FromJson for Definition {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Definition")?;
        Ok(Self {
            text: fields.required("text")?,
            values: fields.required_with("values", definition_result)?,
        })
    }
}

/// Definitions record either the ordered value set or the original causal rejection.
fn definition_result(
    value: &Value,
) -> Result<Result<BTreeSet<String>, gmeow_errors::RecordedDiag>, DecodeError> {
    let (tag, _) = contract_variant(value)?;
    let mut fields = Record::new(value, "definition Result")?;
    match tag {
        "Ok" => fields.required("Ok").map(Ok),
        "Err" => fields.required("Err").map(Err),
        name => Err(DecodeError::unknown_variant(name, &["Ok", "Err"])),
    }
}
impl ToJson for OwnedRule {
    fn to_json(&self) -> Value {
        Object::new()
            .with("path", self.path.to_json())
            .with("match_prefix", self.match_prefix)
            .with("suffix", self.suffix.to_json())
            .with("family", self.family.to_json())
            .with("form", self.form.to_json())
            .into()
    }
}
impl FromJson for OwnedRule {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct OwnedRule")?;
        Ok(Self {
            path: fields.required("path")?,
            match_prefix: fields.required("match_prefix")?,
            suffix: fields.optional("suffix")?,
            family: fields.required("family")?,
            form: fields.required("form")?,
        })
    }
}

/// Externally tagged payloads must contain exactly one member, including duplicates.
fn contract_variant(value: &Value) -> Result<(&str, &Value), DecodeError> {
    let object = value
        .as_object()
        .ok_or_else(|| DecodeError::invalid_type(value, "externally tagged variant"))?;
    let mut entries = object.iter();
    let Some((tag, payload)) = entries.next() else {
        return Err(DecodeError::invalid_value(value, "one variant"));
    };
    if entries.next().is_some() {
        return Err(DecodeError::invalid_value(value, "one variant"));
    }
    Ok((tag, payload))
}

/// Unit dispatch variants retain both strings and exact singleton null objects.
fn dispatch_unit(value: &Value) -> Result<&str, DecodeError> {
    if let Some(name) = value.as_str() {
        return Ok(name);
    }
    let (name, payload) = contract_variant(value)?;
    if !matches!(payload, Value::Null) {
        return Err(DecodeError::invalid_value(
            value,
            "unit variant with null payload",
        ));
    }
    Ok(name)
}
impl ToJson for FanoutFamily {
    fn to_json(&self) -> Value {
        Value::from(match self {
            Self::RdfFanout => "RdfFanout",
            Self::Edoal => "Edoal",
            Self::Opaque => "Opaque",
            Self::HeaderDict => "HeaderDict",
        })
    }
}
impl FromJson for FanoutFamily {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match dispatch_unit(value)? {
            "RdfFanout" => Ok(Self::RdfFanout),
            "Edoal" => Ok(Self::Edoal),
            "Opaque" => Ok(Self::Opaque),
            "HeaderDict" => Ok(Self::HeaderDict),
            name => Err(DecodeError::unknown_variant(
                name,
                &["RdfFanout", "Edoal", "Opaque", "HeaderDict"],
            )),
        }
    }
}
impl ToJson for OwnedForm {
    fn to_json(&self) -> Value {
        match self {
            Self::NQuads(graph) => Object::new().with("NQuads", graph.to_json()).into(),
            Self::Turtle => Value::from("Turtle"),
            Self::NTriples => Value::from("NTriples"),
            Self::NQuadsSelf => Value::from("NQuadsSelf"),
            Self::Blob => Value::from("Blob"),
            Self::HeaderDict => Value::from("HeaderDict"),
        }
    }
}
impl FromJson for OwnedForm {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        if value.as_object().is_some() {
            let (name, _) = contract_variant(value)?;
            if name == "NQuads" {
                return Record::new(value, "NQuads variant")?
                    .required("NQuads")
                    .map(Self::NQuads);
            }
        }
        match dispatch_unit(value)? {
            "Turtle" => Ok(Self::Turtle),
            "NTriples" => Ok(Self::NTriples),
            "NQuadsSelf" => Ok(Self::NQuadsSelf),
            "Blob" => Ok(Self::Blob),
            "HeaderDict" => Ok(Self::HeaderDict),
            name => Err(DecodeError::unknown_variant(
                name,
                &[
                    "Turtle",
                    "NTriples",
                    "NQuads",
                    "NQuadsSelf",
                    "Blob",
                    "HeaderDict",
                ],
            )),
        }
    }
}

#[cfg(test)]
mod tests;
