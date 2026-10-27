// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete validation fixture products, selected source bytes and recorded refusals.

use super::{CaseObservation, ModuleProduct, ShapeProduct, ValidationFixtures};
use gmeow_errors::json::{read_recorded_result, recorded_result_to_json};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson, items_with},
};
use std::collections::BTreeMap;

struct ModuleOutcome(Result<ModuleProduct, gmeow_errors::RecordedDiag>);
impl FromJson for ModuleOutcome {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        read_recorded_result(
            value,
            "validation module outcome",
            "one validation module outcome variant",
        )
        .map(Self)
    }
}
struct InputBytes(Vec<u8>);
impl FromJson for InputBytes {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        items_with(value, |byte| {
            gmeow_errors::json::read_unsigned(byte, "u8 source input byte")
        })
        .map(Self)
    }
}
struct Findings(Vec<(String, String)>);
impl FromJson for Findings {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        items_with(value, |row| -> Result<_, DecodeError> {
            let [focus, shape] = gmeow_errors::json::read_string_row(row)?;
            Ok((focus, shape))
        })
        .map(Self)
    }
}
impl ToJson for ValidationFixtures {
    fn to_json(&self) -> Value {
        Object::new()
            .with(
                "modules",
                gmeow_errors::json::recorded_map_to_json(&self.modules),
            )
            .with("inputs", self.inputs.to_json())
            .with("cases", self.cases.to_json())
            .with(
                "diagnostics_migration",
                recorded_result_to_json(&self.diagnostics_migration),
            )
            .into()
    }
}
impl FromJson for ValidationFixtures {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ValidationFixtures")?;
        let modules: BTreeMap<String, ModuleOutcome> = fields.required("modules")?;
        let inputs: BTreeMap<String, InputBytes> = fields.required("inputs")?;
        Ok(Self {
            modules: modules
                .into_iter()
                .map(|(name, outcome)| (name, outcome.0))
                .collect(),
            inputs: inputs
                .into_iter()
                .map(|(name, bytes)| (name, bytes.0))
                .collect(),
            cases: fields.required("cases")?,
            diagnostics_migration: fields.required_with("diagnostics_migration", |value| {
                read_recorded_result(
                    value,
                    "validation migration outcome",
                    "one validation migration outcome variant",
                )
            })?,
        })
    }
}
::purrdf_lex::json_record!(impl ToJson for ModuleProduct {
    "source_rdf" => source_rdf,
    "diagnostics" => diagnostics,
    "shapes" => shapes,
    "procedural_shacl" => procedural_shacl,
});
impl FromJson for ModuleProduct {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ModuleProduct")?;
        Ok(Self {
            source_rdf: fields.required("source_rdf")?,
            diagnostics: fields.required("diagnostics")?,
            shapes: fields.required("shapes")?,
            procedural_shacl: fields.required("procedural_shacl")?,
        })
    }
}
::purrdf_lex::json_record!(impl ToJson for ShapeProduct {
    "ir" => ir,
    "shacl" => shacl,
});
impl FromJson for ShapeProduct {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ShapeProduct")?;
        Ok(Self {
            ir: fields.required("ir")?,
            shacl: fields.required("shacl")?,
        })
    }
}
impl ToJson for CaseObservation {
    fn to_json(&self) -> Value {
        Object::new()
            .with("module", self.module.to_json())
            .with("input", self.input.to_json())
            .with("findings", recorded_result_to_json(&self.findings))
            .into()
    }
}
impl FromJson for CaseObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct CaseObservation")?;
        let module = fields.required("module")?;
        let input = fields.required("input")?;
        let findings: Result<Findings, gmeow_errors::RecordedDiag> =
            fields.required_with("findings", |value| {
                read_recorded_result(
                    value,
                    "validation findings outcome",
                    "one validation findings outcome variant",
                )
            })?;
        Ok(Self {
            module,
            input,
            findings: findings.map(|findings| findings.0),
        })
    }
}

#[cfg(test)]
mod tests;
