// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Producer-owned observations for the authored abductive advice schema.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_logic_compile::frontend::{CompiledTheory, Diagnostic, reconstruct_formula};
use gmeow_logic_compile::ir::Formula;
use purrdf::RdfTerm;
use serde::{Deserialize, Serialize};

const CHANNEL: &str = "pipeline/logic-module-abduction.json";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const REQUIRED_ROOTS: &[&str] = &[
    "relatorMediationComplete",
    "referenceFrameComplete",
    "wemiChainComplete",
    "besForall",
];

#[derive(Debug, Serialize, Deserialize)]
struct Observation {
    diagnostics: Vec<Diagnostic>,
    declared_roots: BTreeSet<String>,
    reconstructed: BTreeMap<String, Result<Formula, gmeow_errors::RecordedDiag>>,
    asserted: Vec<Formula>,
    axiom_predicates: BTreeSet<String>,
}

/// Publish the complete abductive-schema observation from the already compiled theory.
pub(super) fn record(
    theory: &CompiledTheory,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let source = theory.source().dataset();
    let program = theory.program();
    let mut schemas = BTreeSet::new();
    let mut roots: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for quad in source.owned_quads() {
        if quad.graph_name.is_some() {
            continue;
        }
        let (RdfTerm::Iri(subject), RdfTerm::Iri(object)) = (quad.subject, quad.object) else {
            continue;
        };
        if quad.predicate == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
            && object == format!("{LOGIC}AbductiveSchema")
        {
            schemas.insert(subject);
        } else if quad.predicate == format!("{LOGIC}completenessFormula") {
            roots.entry(subject).or_default().insert(object);
        }
    }
    let declared_roots: BTreeSet<_> = schemas
        .into_iter()
        .flat_map(|schema| roots.remove(&schema).unwrap_or_default())
        .collect();
    let selected: BTreeSet<_> = declared_roots
        .iter()
        .cloned()
        .chain(REQUIRED_ROOTS.iter().map(|name| format!("{LOGIC}{name}")))
        .collect();
    let observation = Observation {
        diagnostics: theory.diagnostics().to_vec(),
        declared_roots,
        reconstructed: selected
            .into_iter()
            .map(|iri| {
                let formula = reconstruct_formula(source, &iri).map_err(super::record_failure);
                (iri, formula)
            })
            .collect(),
        asserted: program.formulas.clone(),
        axiom_predicates: program
            .axioms
            .iter()
            .map(|axiom| axiom.predicate.clone())
            .collect(),
    };
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&observation)
            .map_err(|error| super::stage_err(format!("encode abduction observations: {error}")))?,
    );
    Ok(())
}

#[cfg(test)]
mod corpus_tests;
