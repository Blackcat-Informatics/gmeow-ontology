// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native grounding records, including strict counts inside typed maps.
use super::{Cell, Observations, TranspilationOutcome};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};
use std::collections::BTreeMap;
impl ToJson for Cell {
    fn to_json(&self) -> Value {
        Object::new()
            .with("iri", self.iri.to_json())
            .with("subject", self.subject.to_json())
            .with("predicate", self.predicate.to_json())
            .with("obj", self.obj.to_json())
            .with("source_endpoint", self.source_endpoint.to_json())
            .with("target_endpoint", self.target_endpoint.to_json())
            .with("sssom_file", self.sssom_file.to_json())
            .with("morphism_class", self.morphism_class.to_json())
            .with("morphism_kind", self.morphism_kind.to_json())
            .with("preservation", self.preservation.to_json())
            .with("confidence", self.confidence.to_json())
            .with("grounding", self.grounding.to_json())
            .into()
    }
}
impl FromJson for Cell {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Cell")?;
        Ok(Self {
            iri: fields.required("iri")?,
            subject: fields.required("subject")?,
            predicate: fields.required("predicate")?,
            obj: fields.required("obj")?,
            source_endpoint: fields.optional("source_endpoint")?,
            target_endpoint: fields.optional("target_endpoint")?,
            sssom_file: fields.required("sssom_file")?,
            morphism_class: fields.optional("morphism_class")?,
            morphism_kind: fields.optional("morphism_kind")?,
            preservation: fields.optional("preservation")?,
            confidence: fields.optional("confidence")?,
            grounding: fields.required("grounding")?,
        })
    }
}
// Every duplicate map value is admitted by the native map owner before last-wins.
struct Count(usize);
impl FromJson for Count {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        gmeow_errors::json::read_unsigned(value, "unsigned correspondence count").map(Self)
    }
}
impl FromJson for TranspilationOutcome {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        gmeow_errors::json::read_recorded_result::<Count>(
            value,
            "correspondence outcome",
            "one correspondence outcome variant",
        )
        .map(|result| Self(result.map(|count| count.0)))
    }
}
impl ToJson for Observations {
    fn to_json(&self) -> Value {
        let outcomes: Object = self
            .transpilation
            .iter()
            .map(|(key, value)| {
                (
                    key.as_str(),
                    gmeow_errors::json::recorded_result_to_json(value),
                )
            })
            .collect();
        Object::new()
            .with("sources", self.sources.to_json())
            .with("catalogs", self.catalogs.to_json())
            .with("transpilation", Value::Object(outcomes))
            .with("declared_subjects", self.declared_subjects.to_json())
            .with("imported_classes", self.imported_classes.to_json())
            .with("bfo_labels", self.bfo_labels.to_json())
            .into()
    }
}
impl FromJson for Observations {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observations")?;
        Ok(Self {
            sources: fields.required("sources")?,
            catalogs: fields.required("catalogs")?,
            transpilation: fields.required_with("transpilation", |value| {
                BTreeMap::<String, TranspilationOutcome>::from_json(value).map(|values| {
                    values
                        .into_iter()
                        .map(|(key, value)| (key, value.0))
                        .collect()
                })
            })?,
            declared_subjects: fields.required("declared_subjects")?,
            imported_classes: fields.required("imported_classes")?,
            bfo_labels: fields.required("bfo_labels")?,
        })
    }
}
#[cfg(test)]
#[path = "json/confidence.tests.rs"]
mod confidence_tests;
#[cfg(test)]
mod tests;
impl ToJson for TranspilationOutcome {
    fn to_json(&self) -> Value {
        gmeow_errors::json::recorded_result_to_json(&self.0)
    }
}
