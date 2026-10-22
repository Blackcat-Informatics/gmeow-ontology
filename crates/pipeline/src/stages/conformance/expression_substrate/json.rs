// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact selected expression observations; no reasoning or source admission here.
use super::{Observations, Profile, Reasoned, Scene};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

impl ToJson for Profile {
    fn to_json(&self) -> Value {
        match self {
            Self::Reference => "Reference",
            Self::Twins => "Twins",
            Self::MultiGraph => "MultiGraph",
            Self::Control => "Control",
        }
        .to_json()
    }
}
impl FromJson for Profile {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "Profile")? {
            "Reference" => Ok(Self::Reference),
            "Twins" => Ok(Self::Twins),
            "MultiGraph" => Ok(Self::MultiGraph),
            "Control" => Ok(Self::Control),
            name => Err(DecodeError::unknown_variant(
                name,
                &["Reference", "Twins", "MultiGraph", "Control"],
            )),
        }
    }
}
impl ToJson for Observations {
    fn to_json(&self) -> Value {
        Object::new()
            .with("producer", self.producer.to_json())
            .with("context_path", self.context_path.to_json())
            .with(
                "context_source_digest",
                self.context_source_digest.to_json(),
            )
            .with(
                "reasoning_projection_digest",
                self.reasoning_projection_digest.to_json(),
            )
            .with("scenes", self.scenes.to_json())
            .into()
    }
}
impl FromJson for Observations {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observations")?;
        Ok(Self {
            producer: fields.required("producer")?,
            context_path: fields.required("context_path")?,
            context_source_digest: fields.required("context_source_digest")?,
            reasoning_projection_digest: fields.required("reasoning_projection_digest")?,
            scenes: fields.required("scenes")?,
        })
    }
}
impl ToJson for Scene {
    fn to_json(&self) -> Value {
        Object::new()
            .with("source_path", self.source_path.to_json())
            .with("source_digest", self.source_digest.to_json())
            .with("profile", self.profile.to_json())
            .with("declared_keys", self.declared_keys.to_json())
            .with("reasoned", self.reasoned.to_json())
            .with("multi_graph_findings", self.multi_graph_findings.to_json())
            .into()
    }
}
impl FromJson for Scene {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Scene")?;
        Ok(Self {
            source_path: fields.required("source_path")?,
            source_digest: fields.required("source_digest")?,
            profile: fields.required("profile")?,
            declared_keys: fields.required("declared_keys")?,
            reasoned: fields.optional("reasoned")?,
            multi_graph_findings: fields.optional("multi_graph_findings")?,
        })
    }
}
impl ToJson for Reasoned {
    fn to_json(&self) -> Value {
        Object::new()
            .with("alpha_classes", self.alpha_classes.to_json())
            .with(
                "operator_less_root_operator_edges",
                self.operator_less_root_operator_edges,
            )
            .with("expression_findings", self.expression_findings.to_json())
            .into()
    }
}
impl FromJson for Reasoned {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Reasoned")?;
        Ok(Self {
            alpha_classes: fields.required("alpha_classes")?,
            operator_less_root_operator_edges: fields
                .required_with("operator_less_root_operator_edges", |value| {
                    gmeow_errors::json::read_unsigned(value, "usize operator-edge count")
                })?,
            expression_findings: fields.optional("expression_findings")?,
        })
    }
}
#[cfg(test)]
mod tests;
