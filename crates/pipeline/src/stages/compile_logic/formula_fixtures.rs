// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit producer selection of authored formula examples and native adapter observations.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_logic::relational_core::{FormulaLoweringInspection, inspect_formula_lowering};
use gmeow_logic_compile::frontend::{Diagnostic, parse_logic_dataset};
use gmeow_logic_compile::ir::{Formula, LogicProgram, Term};
use serde::{Deserialize, Serialize};

const CHANNEL: &str = "pipeline/formula-lowering-fixtures.json";
const CATS: &str = "slices/grounding/lang/tests/conformance-fixtures/meaning-cats-chase-mice.ttl";
const TYPED: &str = "slices/grounding/logic/examples/typed-ir.ttl";
const BETWEEN: &str = "https://blackcatinformatics.ca/gmeow/examples/logic/between";

#[derive(Debug, Serialize, Deserialize)]
struct FormulaObservation {
    formulas: Vec<Formula>,
    diagnostics: Vec<Diagnostic>,
    selected_formulas: usize,
    lowering: Result<FormulaLoweringInspection, gmeow_errors::RecordedDiag>,
}

type Observations = BTreeMap<String, Result<FormulaObservation, gmeow_errors::RecordedDiag>>;

pub(super) fn input_files(root: &Path) -> impl Iterator<Item = PathBuf> + '_ {
    [CATS, TYPED].into_iter().map(|path| root.join(path))
}

pub(super) fn record(
    root: &Path,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let observations: Observations = [(CATS, None), (TYPED, Some(BETWEEN))]
        .into_iter()
        .map(|(path, relation)| {
            let result = (|| {
                let bytes = std::fs::read(root.join(path)).map_err(gmeow_errors::Diag::from)?;
                let dataset = purrdf::parse_dataset(&bytes, "text/turtle", None)
                    .map_err(gmeow_errors::Diag::from)?;
                let (program, diagnostics) =
                    parse_logic_dataset(&dataset, None).map_err(gmeow_errors::Diag::from)?;
                Ok(observe(program, diagnostics, relation))
            })();
            (path.to_owned(), super::observed(result))
        })
        .collect();
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&observations)
            .map_err(|error| super::stage_err(format!("encode formula observations: {error}")))?,
    );
    Ok(())
}

fn observe(
    program: LogicProgram,
    diagnostics: Vec<Diagnostic>,
    relation: Option<&str>,
) -> FormulaObservation {
    let formulas = program.formulas;
    let selected = formulas
        .iter()
        .filter(|formula| {
            relation.is_none_or(|iri| {
        matches!(formula, Formula::Atom { relation: Term::Iri(candidate), .. } if candidate == iri)
    })
        })
        .cloned()
        .collect::<Vec<_>>();
    let selected_formulas = selected.len();
    let selected =
        LogicProgram::new(Vec::new(), Vec::new(), Vec::new(), None).with_formulas(selected);
    FormulaObservation {
        formulas,
        diagnostics,
        selected_formulas,
        lowering: super::observed(inspect_formula_lowering(&selected)),
    }
}

#[cfg(test)]
mod corpus_tests;

#[path = "formula_fixtures.tests.rs"]
#[cfg(test)]
mod tests;
