// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Terminal wiring evidence; decoding never loads or validates production shapes.

use super::{Finding, Observation};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

impl ToJson for Observation {
    fn to_json(&self) -> Value {
        Object::new()
            .with("profile", self.profile.to_json())
            .with("shapes_digest", self.shapes_digest.to_json())
            .with("input_digest", self.input_digest.to_json())
            .with("conforms", self.conforms)
            .with("findings", self.findings.to_json())
            .into()
    }
}
impl FromJson for Observation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observation")?;
        Ok(Self {
            profile: fields.required("profile")?,
            shapes_digest: fields.required("shapes_digest")?,
            input_digest: fields.required("input_digest")?,
            conforms: fields.required("conforms")?,
            findings: fields.required("findings")?,
        })
    }
}
impl ToJson for Finding {
    fn to_json(&self) -> Value {
        Object::new()
            .with("focus", self.focus.to_json())
            .with("path", self.path.to_json())
            .with("component", self.component.to_json())
            .with("source_shape", self.source_shape.to_json())
            .with("severity", self.severity.to_json())
            .with("failure_class", self.failure_class.to_json())
            .with("message", self.message.to_json())
            .into()
    }
}
impl FromJson for Finding {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Finding")?;
        Ok(Self {
            focus: fields.required("focus")?,
            path: fields.optional("path")?,
            component: fields.required("component")?,
            source_shape: fields.required("source_shape")?,
            severity: fields.required("severity")?,
            failure_class: fields.optional("failure_class")?,
            message: fields.optional("message")?,
        })
    }
}

#[cfg(test)]
mod tests;
