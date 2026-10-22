// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete logic vocabulary observations with original formula ownership retained.
use super::{Endpoints, Formula, Observations};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};
use std::collections::BTreeMap;

struct Reconstructed(Result<Formula, gmeow_errors::RecordedDiag>);
impl FromJson for Reconstructed {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        gmeow_errors::json::read_recorded_result(
            value,
            "reconstruction Result",
            "one reconstruction Result variant",
        )
        .map(Self)
    }
}
impl ToJson for Endpoints {
    fn to_json(&self) -> Value {
        Object::new()
            .with("sources", self.sources.to_json())
            .with("targets", self.targets.to_json())
            .into()
    }
}
impl FromJson for Endpoints {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Endpoints")?;
        Ok(Self {
            sources: fields.required("sources")?,
            targets: fields.required("targets")?,
        })
    }
}
impl ToJson for Observations {
    fn to_json(&self) -> Value {
        let reconstructed: Object = self
            .reconstructed
            .iter()
            .map(|(name, outcome)| {
                (
                    name.as_str(),
                    gmeow_errors::json::recorded_result_to_json(outcome),
                )
            })
            .collect();
        Object::new()
            .with("producer", self.producer.to_json())
            .with("source_path", self.source_path.to_json())
            .with("source_digest", self.source_digest.to_json())
            .with("source_iri", self.source_iri.to_json())
            .with("diagnostics", self.diagnostics.to_json())
            .with(
                "declared_completeness_roots",
                self.declared_completeness_roots.to_json(),
            )
            .with("reconstructed", Value::from(reconstructed))
            .with("top_level_formulas", self.top_level_formulas.to_json())
            .with(
                "domain_axiom_predicates",
                self.domain_axiom_predicates.to_json(),
            )
            .with("grounding_laws", self.grounding_laws.to_json())
            .into()
    }
}
impl FromJson for Observations {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observations")?;
        let producer = fields.required("producer")?;
        let source_path = fields.required("source_path")?;
        let source_digest = fields.required("source_digest")?;
        let source_iri = fields.required("source_iri")?;
        let diagnostics = fields.required("diagnostics")?;
        let declared_completeness_roots = fields.required("declared_completeness_roots")?;
        let reconstructed: BTreeMap<String, Reconstructed> = fields.required("reconstructed")?;
        let top_level_formulas = fields.required("top_level_formulas")?;
        let domain_axiom_predicates = fields.required("domain_axiom_predicates")?;
        let grounding_laws = fields.required("grounding_laws")?;
        Ok(Self {
            producer,
            source_path,
            source_digest,
            source_iri,
            diagnostics,
            declared_completeness_roots,
            reconstructed: reconstructed
                .into_iter()
                .map(|(name, value)| (name, value.0))
                .collect(),
            top_level_formulas,
            domain_axiom_predicates,
            grounding_laws,
        })
    }
}
#[cfg(test)]
mod tests;
