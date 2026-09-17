// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Producer-owned byte goldens for explicitly selected authored projection cases.
//! Native RDF outputs go directly to their terminal comparison representation;
//! they are never serialized to Turtle and reparsed to hand data to this stage.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_logic_compile::frontend::{Diagnostic, parse_logic_dataset};
use gmeow_logic_compile::ir::LogicProgram;
use gmeow_logic_compile::projections::compile_program;
use purrdf::{RdfDataset, SerializeGraph, serialize_dataset};
use serde::{Deserialize, Serialize};

const CHANNEL: &str = "pipeline/projection-fixtures.json";
const CASES: [&str; 3] = [
    "confidence-scoped-axiom",
    "kind-hierarchy",
    "relator-mediation",
];

type Observations = BTreeMap<String, Result<ProjectionObservation, gmeow_errors::RecordedDiag>>;

#[derive(Debug, Serialize, Deserialize)]
struct ProjectionObservation {
    diagnostics: Vec<Diagnostic>,
    projections: Result<BTreeMap<String, String>, gmeow_errors::RecordedDiag>,
}

pub(super) fn input_files(root: &Path) -> impl Iterator<Item = PathBuf> + '_ {
    CASES.into_iter().map(|case| {
        root.join("conformance/logic/cases/projections")
            .join(case)
            .join("input.logic.ttl")
    })
}

pub(super) fn record(
    root: &Path,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let observations: Observations = CASES
        .into_iter()
        .zip(input_files(root))
        .map(|(case, path)| {
            let observation = (|| {
                let bytes = std::fs::read(path).map_err(gmeow_errors::Diag::from)?;
                let dataset = purrdf::parse_dataset(&bytes, "text/turtle", None)
                    .map_err(gmeow_errors::Diag::from)?;
                let (program, diagnostics) =
                    parse_logic_dataset(&dataset, None).map_err(gmeow_errors::Diag::from)?;
                Ok(ProjectionObservation {
                    diagnostics,
                    projections: super::observed(project(&program)),
                })
            })();
            (case.to_owned(), super::observed(observation))
        })
        .collect();
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&observations).map_err(|error| {
            super::stage_err(format!("encode projection observations: {error}"))
        })?,
    );
    Ok(())
}

fn project(program: &LogicProgram) -> gmeow_errors::Result<BTreeMap<String, String>> {
    let artifacts = compile_program(program, gmeow_logic::correspondence_exec::program_verdicts)?;
    let mut products = BTreeMap::from([
        ("datalog".to_owned(), artifacts.datalog),
        ("n3".to_owned(), artifacts.n3),
    ]);
    for (name, output) in [
        ("owl-dl", artifacts.owl_dl),
        ("owl-el", artifacts.owl_el),
        ("gufo", artifacts.gufo),
        ("canonical-rdf12", artifacts.canonical_rdf12),
    ] {
        products.insert(name.to_owned(), snapshot(&output.dataset)?);
    }
    // The report still has a text-only public output. Its one terminal parse is
    // explicit here; none of the four native projection datasets takes this path.
    let report = purrdf::parse_dataset(artifacts.report.as_bytes(), "text/turtle", None)
        .map_err(gmeow_errors::Diag::from)?;
    products.insert("projection-report".to_owned(), snapshot(&report)?);
    Ok(products)
}

fn snapshot(dataset: &RdfDataset) -> gmeow_errors::Result<String> {
    let bytes = serialize_dataset(
        dataset,
        "application/n-triples",
        SerializeGraph::DefaultGraph,
    )
    .map_err(gmeow_errors::Diag::from)?;
    let text = String::from_utf8(bytes).map_err(gmeow_errors::Diag::from)?;
    let mut lines: Vec<_> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.strip_suffix(" .").unwrap_or(line))
        .collect();
    lines.sort_unstable();
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod corpus_tests;

#[path = "projection_fixtures.tests.rs"]
#[cfg(test)]
mod tests;
