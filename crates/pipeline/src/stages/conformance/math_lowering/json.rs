// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native structural keys and typed lowering errors, without source reinterpretation.
use super::{FiniteValueSpace, MathLoweringError, Observations, SourceObservation};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};
use std::collections::BTreeMap;

struct KeyOutcome(Result<String, MathLoweringError>);
impl FromJson for KeyOutcome {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let (tag, _) = gmeow_errors::json::read_variant(
            value,
            "structural-key Result",
            "one structural-key Result variant",
        )?;
        let mut fields = Record::new(value, "structural-key Result")?;
        match tag {
            "Ok" => fields.required("Ok").map(|value| Self(Ok(value))),
            "Err" => fields.required("Err").map(|error| Self(Err(error))),
            name => Err(DecodeError::unknown_variant(name, &["Ok", "Err"])),
        }
    }
}
impl ToJson for FiniteValueSpace {
    fn to_json(&self) -> Value {
        Object::new()
            .with("datatypes", self.datatypes.to_json())
            .with("counts", self.counts.to_json())
            .into()
    }
}
impl FromJson for FiniteValueSpace {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct FiniteValueSpace")?;
        Ok(Self {
            datatypes: fields.required("datatypes")?,
            counts: fields.required("counts")?,
        })
    }
}
impl ToJson for SourceObservation {
    fn to_json(&self) -> Value {
        let keys: Object = self
            .keys
            .iter()
            .map(|(key, outcome)| {
                let value: Value = match outcome {
                    Ok(value) => Object::new().with("Ok", value.to_json()).into(),
                    Err(error) => Object::new().with("Err", error.to_json()).into(),
                };
                (key.as_str(), value)
            })
            .collect();
        let repeated_shadow: Value = match &self.repeated_shadow {
            None => Value::Null,
            Some(Ok(value)) => Object::new().with("Ok", value.to_json()).into(),
            Some(Err(error)) => Object::new().with("Err", error.to_json()).into(),
        };
        let mut object = Object::new()
            .with("finite_value_spaces", self.finite_value_spaces.to_json())
            .with("source_path", self.source_path.to_json())
            .with("subjects", self.subjects.to_json())
            .with("authored_keys", self.authored_keys.to_json())
            .with("keys", Value::from(keys))
            .with("application_nodes", self.application_nodes)
            .with("repeated_shadow", repeated_shadow);
        if let Some(lint) = &self.source_lint {
            object.insert("source_lint", lint.to_json());
        }
        object.into()
    }
}
impl FromJson for SourceObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct SourceObservation")?;
        let finite_value_spaces = fields.required("finite_value_spaces")?;
        let source_path = fields.required("source_path")?;
        let subjects = fields.required("subjects")?;
        let authored_keys = fields.required("authored_keys")?;
        let keys: BTreeMap<String, KeyOutcome> = fields.required("keys")?;
        let application_nodes = fields.required_with("application_nodes", |value| {
            gmeow_errors::json::read_unsigned(value, "usize application count")
        })?;
        let repeated_shadow: Option<KeyOutcome> = fields.optional("repeated_shadow")?;
        let source_lint = fields.optional("source_lint")?;
        Ok(Self {
            finite_value_spaces,
            source_path,
            subjects,
            authored_keys,
            keys: keys
                .into_iter()
                .map(|(key, value)| (key, value.0))
                .collect(),
            application_nodes,
            repeated_shadow: repeated_shadow.map(|value| value.0),
            source_lint,
        })
    }
}
impl ToJson for Observations {
    fn to_json(&self) -> Value {
        Object::new()
            .with("depth_limit", self.depth_limit)
            .with("source_digests", self.source_digests.to_json())
            .with("sources", self.sources.to_json())
            .into()
    }
}
impl FromJson for Observations {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observations")?;
        Ok(Self {
            depth_limit: fields.required_with("depth_limit", |value| {
                gmeow_errors::json::read_unsigned(value, "usize depth limit")
            })?,
            source_digests: fields.required("source_digests")?,
            sources: fields.required("sources")?,
        })
    }
}
#[cfg(test)]
mod tests;
