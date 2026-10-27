// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete abductive-schema declarations and reconstruction outcomes.

use super::Observation;
use gmeow_logic_compile::ir::Formula;
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
            "abduction reconstruction outcome",
            "one abduction reconstruction outcome variant",
        )
        .map(Self)
    }
}

impl ToJson for Observation {
    fn to_json(&self) -> Value {
        let reconstructed: Object = self
            .reconstructed
            .iter()
            .map(|(name, result)| {
                (
                    name.as_str(),
                    gmeow_errors::json::recorded_result_to_json(result),
                )
            })
            .collect();
        Object::new()
            .with("diagnostics", self.diagnostics.to_json())
            .with("declared_roots", self.declared_roots.to_json())
            .with("reconstructed", Value::from(reconstructed))
            .with("asserted", self.asserted.to_json())
            .with("axiom_predicates", self.axiom_predicates.to_json())
            .into()
    }
}
impl FromJson for Observation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observation")?;
        let diagnostics = fields.required("diagnostics")?;
        let declared_roots = fields.required("declared_roots")?;
        let reconstructed: BTreeMap<String, Reconstructed> = fields.required("reconstructed")?;
        Ok(Self {
            diagnostics,
            declared_roots,
            reconstructed: reconstructed
                .into_iter()
                .map(|(name, result)| (name, result.0))
                .collect(),
            asserted: fields.required("asserted")?,
            axiom_predicates: fields.required("axiom_predicates")?,
        })
    }
}
