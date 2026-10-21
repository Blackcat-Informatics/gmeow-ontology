// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native named-record transport for findings. Report metadata and renderers
//! retain their own encoding owner. Positional struct arrays are not admitted.

use super::{Finding, RelatedLabel};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

impl ToJson for RelatedLabel {
    fn to_json(&self) -> Value {
        let mut object = Object::new();
        object.insert("location", self.location.to_json());
        object.insert("message", self.message.to_json());
        Value::Object(object)
    }
}

impl FromJson for RelatedLabel {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct RelatedLabel")?;
        Ok(Self {
            location: record.required("location")?,
            message: record.required("message")?,
        })
    }
}

impl ToJson for Finding {
    fn to_json(&self) -> Value {
        let mut object = Object::new();
        object.insert("severity", self.severity.to_json());
        object.insert("code", self.code.to_json());
        object.insert("message", self.message.to_json());
        if self.source_graph.is_some() {
            object.insert(
                "source_graph",
                self.source_graph.as_ref().map(|iri| iri.as_str()).to_json(),
            );
        }
        if self.source_focus.is_some() {
            object.insert("source_focus", self.source_focus.to_json());
        }
        if self.tool.is_some() {
            object.insert("tool", self.tool.to_json());
        }
        if !self.locations.is_empty() {
            object.insert("locations", self.locations.to_json());
        }
        if !self.related_locations.is_empty() {
            object.insert("related_locations", self.related_locations.to_json());
        }
        if !self.related_labels.is_empty() {
            object.insert("related_labels", self.related_labels.to_json());
        }
        if !self.suggestions.is_empty() {
            object.insert("suggestions", self.suggestions.to_json());
        }
        if !self.advice.is_empty() {
            object.insert("advice", self.advice.to_json());
        }
        if !self.remediation.is_empty() {
            object.insert("remediation", self.remediation.to_json());
        }
        if self.finding_iri.is_some() {
            object.insert("finding_iri", self.finding_iri.to_json());
        }
        if self.anchor_iri.is_some() {
            object.insert("anchor_iri", self.anchor_iri.to_json());
        }
        if self.anchor_non_trivial {
            object.insert("anchor_non_trivial", self.anchor_non_trivial.to_json());
        }
        if !self.antecedents.is_empty() {
            object.insert("antecedents", self.antecedents.to_json());
        }
        if self.root_cause.is_some() {
            object.insert("root_cause", self.root_cause.to_json());
        }
        if self.cluster.is_some() {
            object.insert("cluster", self.cluster.to_json());
        }
        if !self.cross_node_glut_with.is_empty() {
            object.insert("cross_node_glut_with", self.cross_node_glut_with.to_json());
        }
        if !self.tags.is_empty() {
            object.insert("tags", self.tags.to_json());
        }
        if self.detail.is_some() {
            object.insert("detail", self.detail.to_json());
        }
        if self.failure_class.is_some() {
            object.insert("failure_class", self.failure_class.to_json());
        }
        if self.category.is_some() {
            object.insert("category", self.category.to_json());
        }
        if self.standpoint.is_some() {
            object.insert("standpoint", self.standpoint.to_json());
        }
        if !self.attributions.is_empty() {
            object.insert("attributions", self.attributions.to_json());
        }
        if !self.documented_terms.is_empty() {
            object.insert("documented_terms", self.documented_terms.to_json());
        }
        if !self.guidance.is_empty() {
            object.insert("guidance", self.guidance.to_json());
        }
        if !self.derived_from_quads.is_empty() {
            object.insert("derived_from_quads", self.derived_from_quads.to_json());
        }
        if !self.cited_iris.is_empty() {
            object.insert("cited_iris", self.cited_iris.to_json());
        }
        Value::Object(object)
    }
}

impl FromJson for Finding {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct Finding")?;
        Ok(Self {
            severity: record.required("severity")?,
            code: record.required("code")?,
            message: record.required("message")?,
            source_graph: record.optional_with("source_graph", |item| {
                let iri = String::from_json(item)?;
                purrdf_core::BaseIri::parse(&iri).map_err(DecodeError::custom)
            })?,
            source_focus: record.optional("source_focus")?,
            tool: record.optional("tool")?,
            locations: record.defaulted("locations")?,
            related_locations: record.defaulted("related_locations")?,
            related_labels: record.defaulted("related_labels")?,
            suggestions: record.defaulted("suggestions")?,
            advice: record.defaulted("advice")?,
            remediation: record.defaulted("remediation")?,
            finding_iri: record.optional("finding_iri")?,
            anchor_iri: record.optional("anchor_iri")?,
            anchor_non_trivial: record.defaulted("anchor_non_trivial")?,
            antecedents: record.defaulted("antecedents")?,
            root_cause: record.optional("root_cause")?,
            cluster: record.optional("cluster")?,
            cross_node_glut_with: record.defaulted("cross_node_glut_with")?,
            tags: record.defaulted("tags")?,
            detail: record.optional("detail")?,
            failure_class: record.optional("failure_class")?,
            category: record.optional("category")?,
            standpoint: record.optional("standpoint")?,
            attributions: record.defaulted("attributions")?,
            documented_terms: record.defaulted("documented_terms")?,
            guidance: record.defaulted("guidance")?,
            derived_from_quads: record.defaulted("derived_from_quads")?,
            cited_iris: record.defaulted("cited_iris")?,
        })
    }
}

#[cfg(test)]
mod tests;
