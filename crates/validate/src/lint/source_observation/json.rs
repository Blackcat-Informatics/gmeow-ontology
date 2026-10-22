// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! One native record owner for the published source-lint transport.
use super::{Configuration, Observations, SourceRecord};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};
impl ToJson for Observations {
    fn to_json(&self) -> Value {
        Object::new()
            .with("profile", self.profile.to_json())
            .with("configuration", self.configuration.to_json())
            .with("sources", self.sources.to_json())
            .into()
    }
}
impl FromJson for Observations {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observations")?;
        Ok(Self {
            profile: fields.required("profile")?,
            configuration: fields.required("configuration")?,
            sources: fields.required("sources")?,
        })
    }
}
impl ToJson for Configuration {
    fn to_json(&self) -> Value {
        Object::new()
            .with("namespace", self.namespace.to_json())
            .with("ontology_iri", self.ontology_iri.to_json())
            .with("selector_tokens", self.selector_tokens.to_json())
            .with("core_slice_iris", self.core_slice_iris.to_json())
            .with(
                "annotation_predicates",
                self.annotation_predicates.to_json(),
            )
            .into()
    }
}
impl FromJson for Configuration {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Configuration")?;
        Ok(Self {
            namespace: fields.required("namespace")?,
            ontology_iri: fields.required("ontology_iri")?,
            selector_tokens: fields.required("selector_tokens")?,
            core_slice_iris: fields.required("core_slice_iris")?,
            annotation_predicates: fields.required("annotation_predicates")?,
        })
    }
}
impl ToJson for SourceRecord {
    fn to_json(&self) -> Value {
        Object::new()
            .with("source_digest", self.source_digest.to_json())
            .with("diagnostics", self.diagnostics.to_json())
            .into()
    }
}
impl FromJson for SourceRecord {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct SourceRecord")?;
        Ok(Self {
            source_digest: fields.required("source_digest")?,
            diagnostics: fields.required("diagnostics")?,
        })
    }
}
#[cfg(test)]
mod tests;
