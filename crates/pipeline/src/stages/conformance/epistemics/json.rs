// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Scoped probe rows and the independent size of their complete closure.
use super::{Observation, Probe};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};
use std::collections::BTreeMap;

struct ProbeOutcome(Result<Probe, gmeow_errors::RecordedDiag>);
impl FromJson for ProbeOutcome {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        gmeow_errors::json::read_recorded_result(
            value,
            "epistemics probe Result",
            "one epistemics probe Result variant",
        )
        .map(Self)
    }
}
impl ToJson for Probe {
    fn to_json(&self) -> Value {
        Object::new()
            .with("total_triples", self.total_triples)
            .with("triples", self.triples.to_json())
            .into()
    }
}
impl FromJson for Probe {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Probe")?;
        Ok(Self {
            total_triples: fields.required_with("total_triples", |value| {
                gmeow_errors::json::read_unsigned(value, "usize closure count")
            })?,
            triples: fields.required("triples")?,
        })
    }
}
impl ToJson for Observation {
    fn to_json(&self) -> Value {
        let probes: Object = self
            .probes
            .iter()
            .map(|(name, outcome)| {
                let value: Value = match outcome {
                    Ok(value) => Object::new().with("Ok", value.to_json()).into(),
                    Err(error) => Object::new().with("Err", error.to_json()).into(),
                };
                (name.as_str(), value)
            })
            .collect();
        Object::new()
            .with("universe", self.universe.to_json())
            .with("probes", Value::from(probes))
            .into()
    }
}
impl FromJson for Observation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observation")?;
        let universe = fields.required("universe")?;
        let probes: BTreeMap<String, ProbeOutcome> = fields.required("probes")?;
        Ok(Self {
            universe,
            probes: probes
                .into_iter()
                .map(|(name, value)| (name, value.0))
                .collect(),
        })
    }
}
#[cfg(test)]
mod tests;
