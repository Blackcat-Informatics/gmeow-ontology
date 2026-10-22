// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native claim observations; graph commitments and closure evaluation stay upstream.

use super::{ClosureObservation, Observation, ShippedObservation};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

impl ToJson for ShippedObservation {
    fn to_json(&self) -> Value {
        Object::new()
            .with("graph", self.graph.to_json())
            .with("graph_digest", self.graph_digest.to_json())
            .with("asserted_quads", self.asserted_quads)
            .with("advisory_assessments", self.advisory_assessments.to_json())
            .into()
    }
}
impl FromJson for ShippedObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ShippedObservation")?;
        Ok(Self {
            graph: fields.required("graph")?,
            graph_digest: fields.required("graph_digest")?,
            asserted_quads: fields.required_with("asserted_quads", |value| {
                crate::json_counts::read_usize(value, "usize claim count")
            })?,
            advisory_assessments: fields.required("advisory_assessments")?,
        })
    }
}
impl ToJson for ClosureObservation {
    fn to_json(&self) -> Value {
        Object::new()
            .with("source_path", self.source_path.to_json())
            .with("source_digest", self.source_digest.to_json())
            .with("source_quads", self.source_quads)
            .with("closure_triples", self.closure_triples)
            .into()
    }
}
impl FromJson for ClosureObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ClosureObservation")?;
        Ok(Self {
            source_path: fields.required("source_path")?,
            source_digest: fields.required("source_digest")?,
            source_quads: fields.required_with("source_quads", |value| {
                crate::json_counts::read_usize(value, "usize claim count")
            })?,
            closure_triples: fields.required_with("closure_triples", |value| {
                crate::json_counts::read_usize(value, "usize claim count")
            })?,
        })
    }
}
impl ToJson for Observation {
    fn to_json(&self) -> Value {
        let closure: Value = match &self.closure {
            Ok(value) => Object::new().with("Ok", value.to_json()).into(),
            Err(error) => Object::new().with("Err", error.to_json()).into(),
        };
        Object::new()
            .with("claims", self.claims.to_json())
            .with("closure", closure)
            .into()
    }
}
impl FromJson for Observation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observation")?;
        Ok(Self {
            claims: fields.required("claims")?,
            closure: fields.required_with("closure", closure_result)?,
        })
    }
}

/// The closure is exactly one externally tagged success or recorded rejection.
fn closure_result(
    value: &Value,
) -> Result<Result<ClosureObservation, gmeow_errors::RecordedDiag>, DecodeError> {
    let object = value
        .as_object()
        .ok_or_else(|| DecodeError::invalid_type(value, "closure Result"))?;
    let mut entries = object.iter();
    let Some((tag, _)) = entries.next() else {
        return Err(DecodeError::invalid_value(
            value,
            "one closure Result variant",
        ));
    };
    if entries.next().is_some() {
        return Err(DecodeError::invalid_value(
            value,
            "one closure Result variant",
        ));
    }
    let mut fields = Record::new(value, "closure Result")?;
    match tag.as_str() {
        "Ok" => fields.required("Ok").map(Ok),
        "Err" => fields.required("Err").map(Err),
        name => Err(DecodeError::unknown_variant(name, &["Ok", "Err"])),
    }
}

#[cfg(test)]
mod tests;
