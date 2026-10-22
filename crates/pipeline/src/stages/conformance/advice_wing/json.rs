// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native advisory transport; confidence refusal occurs before returning bytes.

use super::{Advisory, Observation, RetainedFinding};
use purrdf_lex::json::{
    Number, Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

impl Observation {
    /// Encode the complete observation as compact UTF-8 without a trailing newline.
    ///
    /// # Errors
    /// Refuses a nonfinite confidence, identifying its advisory index, code and
    /// JSON field in the primary diagnostic. No bytes are returned on refusal.
    pub fn to_json_bytes(&self) -> gmeow_errors::Result<Vec<u8>> {
        let advisories = self
            .advisories
            .iter()
            .enumerate()
            .map(|(index, advisory)| advisory.json_value(index))
            .collect::<gmeow_errors::Result<Vec<_>>>()?;
        let value = Object::new()
            .with("profile", self.profile.to_json())
            .with("shapes_digest", self.shapes_digest.to_json())
            .with("input_digest", self.input_digest.to_json())
            .with("shape_sources", self.shape_sources.to_json())
            .with("retained_conforms", self.retained_conforms)
            .with("retained_findings", self.retained_findings.to_json())
            .with("advisories", Value::Array(advisories))
            .with("claims_nquads", self.claims_nquads.to_json());
        Ok(purrdf_lex::json::write_compact(&value.into()).into_bytes())
    }
}

impl Advisory {
    /// Build one row with a finite native number, never the float-to-null path.
    fn json_value(&self, index: usize) -> gmeow_errors::Result<Value> {
        let confidence = Number::from_f64(self.confidence).ok_or_else(|| super::fail(format!(
            "advice-wing advisory {index} ({}) /advisories/{index}/confidence requires a finite f64; got {}",
            self.code, self.confidence,
        )))?;
        Ok(Object::new()
            .with("code", self.code.to_json())
            .with("message", self.message.to_json())
            .with("severity", self.severity.to_json())
            .with("suggestions", self.suggestions.to_json())
            .with("subject_iri", self.subject_iri.to_json())
            .with("claim_code", self.claim_code.to_json())
            .with("claim_subject_iri", self.claim_subject_iri.to_json())
            .with("modality_iri", self.modality_iri.to_json())
            .with("standpoint_iri", self.standpoint_iri.to_json())
            .with("confidence", Value::Number(confidence))
            .with("verdict_iri", self.verdict_iri.to_json())
            .with("advised_proposition", self.advised_proposition.to_json())
            .into())
    }
}

impl FromJson for Observation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observation")?;
        Ok(Self {
            profile: fields.required("profile")?,
            shapes_digest: fields.required("shapes_digest")?,
            input_digest: fields.required("input_digest")?,
            shape_sources: fields.required("shape_sources")?,
            retained_conforms: fields.required("retained_conforms")?,
            retained_findings: fields.required("retained_findings")?,
            advisories: fields.required("advisories")?,
            claims_nquads: fields.required("claims_nquads")?,
        })
    }
}
impl FromJson for Advisory {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Advisory")?;
        Ok(Self {
            code: fields.required("code")?,
            message: fields.required("message")?,
            severity: fields.required("severity")?,
            suggestions: fields.required("suggestions")?,
            subject_iri: fields.optional("subject_iri")?,
            claim_code: fields.required("claim_code")?,
            claim_subject_iri: fields.optional("claim_subject_iri")?,
            modality_iri: fields.required("modality_iri")?,
            standpoint_iri: fields.required("standpoint_iri")?,
            confidence: fields.required("confidence")?,
            verdict_iri: fields.required("verdict_iri")?,
            advised_proposition: fields.required("advised_proposition")?,
        })
    }
}
impl ToJson for RetainedFinding {
    fn to_json(&self) -> Value {
        Object::new()
            .with("focus", self.focus.to_json())
            .with("component", self.component.to_json())
            .with("source_shape", self.source_shape.to_json())
            .with("severity", self.severity.to_json())
            .with("failure_class", self.failure_class.to_json())
            .into()
    }
}
impl FromJson for RetainedFinding {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct RetainedFinding")?;
        Ok(Self {
            focus: fields.required("focus")?,
            component: fields.required("component")?,
            source_shape: fields.required("source_shape")?,
            severity: fields.required("severity")?,
            failure_class: fields.optional("failure_class")?,
        })
    }
}

#[cfg(test)]
mod tests;
