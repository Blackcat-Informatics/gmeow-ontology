// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Frozen vector envelopes retain each completed success or recorded refusal.
use super::{Observation, PositiveObservation};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};
use std::collections::BTreeMap;

struct PositiveOutcome(Result<PositiveObservation, gmeow_errors::RecordedDiag>);
impl FromJson for PositiveOutcome {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        gmeow_errors::json::read_recorded_result(
            value,
            "positive Result",
            "one positive Result variant",
        )
        .map(Self)
    }
}
struct NegativeOutcome(Result<Option<String>, gmeow_errors::RecordedDiag>);
impl FromJson for NegativeOutcome {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        gmeow_errors::json::read_recorded_result(
            value,
            "negative Result",
            "one negative Result variant",
        )
        .map(Self)
    }
}
impl ToJson for Observation {
    fn to_json(&self) -> Value {
        let positives: Object = self
            .positives
            .iter()
            .map(|(name, outcome)| {
                let value: Value = match outcome {
                    Ok(value) => Object::new().with("Ok", value.to_json()).into(),
                    Err(error) => Object::new().with("Err", error.to_json()).into(),
                };
                (name.as_str(), value)
            })
            .collect();
        let negatives: Object = self
            .negatives
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
            .with("codebook_digest", self.codebook_digest.to_json())
            .with("declared_digests", self.declared_digests.to_json())
            .with("manifest_stems", self.manifest_stems.to_json())
            .with("positives", Value::from(positives))
            .with("expected_negatives", self.expected_negatives.to_json())
            .with("negatives", Value::from(negatives))
            .into()
    }
}
impl FromJson for Observation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observation")?;
        let codebook_digest = fields.required("codebook_digest")?;
        let declared_digests = fields.required("declared_digests")?;
        let manifest_stems = fields.required("manifest_stems")?;
        let positives: BTreeMap<String, PositiveOutcome> = fields.required("positives")?;
        let expected_negatives = fields.required("expected_negatives")?;
        let negatives: BTreeMap<String, NegativeOutcome> = fields.required("negatives")?;
        Ok(Self {
            codebook_digest,
            declared_digests,
            manifest_stems,
            positives: positives
                .into_iter()
                .map(|(key, value)| (key, value.0))
                .collect(),
            expected_negatives,
            negatives: negatives
                .into_iter()
                .map(|(key, value)| (key, value.0))
                .collect(),
        })
    }
}
#[cfg(test)]
mod tests;
