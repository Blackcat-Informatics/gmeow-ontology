// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Ordered authored plan observations; transport does not interpret the plan.
use super::{Observation, PhaseWalk};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

impl ToJson for PhaseWalk {
    fn to_json(&self) -> Value {
        Object::new()
            .with("phases", self.phases.to_json())
            .with("iterated", self.iterated.to_json())
            .into()
    }
}
impl FromJson for PhaseWalk {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct PhaseWalk")?;
        Ok(Self {
            phases: fields.required("phases")?,
            iterated: fields.required("iterated")?,
        })
    }
}
impl ToJson for Observation {
    fn to_json(&self) -> Value {
        let walk: Value = match &self.walk {
            Ok(value) => Object::new().with("Ok", value.to_json()).into(),
            Err(error) => Object::new().with("Err", error.to_json()).into(),
        };
        Object::new()
            .with("source_path", self.source_path.to_json())
            .with("source_digest", self.source_digest.to_json())
            .with("plan_types", self.plan_types.to_json())
            .with("body_roots", self.body_roots.to_json())
            .with("walk", walk)
            .into()
    }
}
impl FromJson for Observation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observation")?;
        Ok(Self {
            source_path: fields.required("source_path")?,
            source_digest: fields.required("source_digest")?,
            plan_types: fields.required("plan_types")?,
            body_roots: fields.required("body_roots")?,
            walk: fields.required_with("walk", |value| {
                gmeow_errors::json::read_recorded_result(
                    value,
                    "phase walk Result",
                    "one phase walk Result variant",
                )
            })?,
        })
    }
}
#[cfg(test)]
mod tests;
