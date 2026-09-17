// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Producer-owned standalone validation projections and counterexample results.
//!
//! Each selected module is compiled once from the retained native catalog. These
//! standalone scopes preserve the migration assertions: a different module cannot
//! accidentally supply a missing constraint. Tests read the exact typed shapes,
//! SHACL outputs and case results from the authenticated compile-stage action.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_logic_compile::frontend::{Diagnostic, PreparedLogicSource, derive_validation_shapes};
use gmeow_logic_compile::ir::{LogicProgram, ValidationShapeIr};
use gmeow_logic_compile::projections::shapes::{
    project_procedural_constraints, project_validation_shape_shacl,
};
use purrdf::shapes::engine::{parse_shapes, validate_dataset};
use serde::{Deserialize, Serialize};

use crate::stages::parse_sources::SourceCatalog;

mod selection;

const CHANNEL: &str = "pipeline/module-validation-fixtures.json";
const SH_HEADER: &str = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n";

#[derive(Debug, Serialize, Deserialize)]
struct ValidationFixtures {
    modules: BTreeMap<String, Result<ModuleProduct, gmeow_errors::RecordedDiag>>,
    cases: Vec<CaseObservation>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ModuleProduct {
    diagnostics: Vec<Diagnostic>,
    shapes: Vec<ShapeProduct>,
    procedural_shacl: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ShapeProduct {
    ir: ValidationShapeIr,
    shacl: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CaseObservation {
    module: String,
    input: String,
    findings: Result<Vec<(String, String)>, gmeow_errors::RecordedDiag>,
}

pub(super) fn input_files(root: &Path) -> impl Iterator<Item = PathBuf> + '_ {
    selection::CASES.iter().map(|(_, input)| root.join(input))
}

pub(super) fn record(
    catalog: &SourceCatalog,
    root: &Path,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let mut modules = BTreeMap::new();
    let mut plans = BTreeMap::new();
    for &module in selection::MODULES {
        let product = super::observed(
            catalog
                .prepare_document(module)
                .and_then(|source| compile_module(&source)),
        );
        let plan = product.as_ref().map_err(Clone::clone).and_then(|product| {
            parse_shapes(&product.procedural_shacl, None)
                .map_err(|message| super::record_failure(super::stage_err(message)))
        });
        plans.insert(module, plan);
        modules.insert(module.to_owned(), product);
    }
    let mut cases = Vec::new();
    for &(module, input) in selection::CASES {
        let findings = (|| {
            let shapes = plans
                .get(module)
                .ok_or_else(|| {
                    super::record_failure(super::stage_err(format!("unselected module {module}")))
                })?
                .as_ref()
                .map_err(Clone::clone)?;
            let bytes = std::fs::read(root.join(input)).map_err(super::record_failure)?;
            let data = purrdf::parse_dataset(&bytes, "text/turtle", None)
                .map_err(super::record_failure)?;
            let report = validate_dataset(&data, shapes)
                .map_err(|message| super::record_failure(super::stage_err(message)))?;
            Ok(report
                .results
                .iter()
                .map(|finding| {
                    (
                        finding.focus_node.to_string(),
                        finding.source_shape.to_string(),
                    )
                })
                .collect())
        })();
        cases.push(CaseObservation {
            module: module.to_owned(),
            input: input.to_owned(),
            findings,
        });
    }
    let fixtures = ValidationFixtures { modules, cases };
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&fixtures)
            .map_err(|error| super::stage_err(format!("encode validation fixtures: {error}")))?,
    );
    Ok(())
}

fn compile_module(source: &PreparedLogicSource) -> gmeow_errors::Result<ModuleProduct> {
    // Reuse the prepared ownership/formula analysis, extracting only the
    // procedural constraints this output needs. Unrelated axioms and programs
    // are already compiled by the aggregate theory and are not lowered again.
    let (constraints, diagnostics) = source.constraints();
    let program =
        LogicProgram::new(Vec::new(), Vec::new(), Vec::new(), None).with_constraints(constraints);
    let shapes = derive_validation_shapes(source.dataset())?
        .into_iter()
        .map(|ir| ShapeProduct {
            shacl: format!("{SH_HEADER}{}", project_validation_shape_shacl(&ir)),
            ir,
        })
        .collect();
    Ok(ModuleProduct {
        diagnostics,
        shapes,
        procedural_shacl: project_procedural_constraints(&program),
    })
}

#[cfg(test)]
mod corpus_tests;

#[path = "validation_fixtures.tests.rs"]
#[cfg(test)]
mod tests;
