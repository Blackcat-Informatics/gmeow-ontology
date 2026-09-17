// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Original logic-module vocabulary and owned abductive formula observations.
//! The shared standalone compilation supplies the actual asserted formula/axiom sets;
//! reconstructing an advice root never promotes it into that asserted theory.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gmeow_action_cache::{
    ActionInput, ActionStore, FileKind, STORE_FORMAT_VERSION, StoreLimits, bytes_digest,
};
use gmeow_logic_compile::frontend::{Diagnostic, reconstruct_formula};
use gmeow_logic_compile::ir::Formula;
use purrdf::{RdfDataset, RdfTerm};
use serde::{Deserialize, Serialize};

use crate::cache::BuildIdentity;
use crate::stages::parse_sources::SourceCatalog;

pub(super) const CHANNEL: &str = "pipeline/logic-module-contract-observations.json";
pub(super) const SOURCE: &str = "slices/grounding/logic/module.ttl";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";

#[derive(Serialize, Deserialize)]
pub(super) struct Observations {
    pub producer: BuildIdentity,
    pub source_path: String,
    pub source_digest: String,
    pub source_iri: String,
    pub diagnostics: Vec<Diagnostic>,
    pub declared_completeness_roots: BTreeSet<String>,
    pub reconstructed: BTreeMap<String, Result<Formula, gmeow_errors::RecordedDiag>>,
    pub top_level_formulas: Vec<Formula>,
    pub domain_axiom_predicates: BTreeSet<String>,
    pub grounding_laws: BTreeMap<String, Endpoints>,
}

#[derive(Default, Serialize, Deserialize)]
pub(super) struct Endpoints {
    pub sources: Vec<String>,
    pub targets: Vec<String>,
}

pub(super) fn input_files(root: &Path) -> Vec<PathBuf> {
    vec![root.join(SOURCE)]
}

pub(super) fn record(
    root: &Path,
    catalog: &SourceCatalog,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let source_digest = catalog.document_digest(SOURCE)?.to_owned();
    let source_iri = gmeow_logic::verify::GATE_SOURCES[1].1;
    let store = ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(fail)?;
    let observed = super::execution::cached_inputs(
        &store,
        "logic-module-contracts-v1:original-default-graph",
        vec![
            ActionInput::Raw {
                logical_path: SOURCE.to_owned(),
                file_kind: FileKind::File,
                executable: false,
                digest: source_digest.clone(),
            },
            ActionInput::Raw {
                logical_path: "logic-module-contracts/source-iri".to_owned(),
                file_kind: FileKind::Aggregate,
                executable: false,
                digest: bytes_digest(source_iri.as_bytes()),
            },
        ],
        || {
            let dataset = catalog.document(SOURCE)?;
            let compiled = catalog.compiled_document(SOURCE, Some(source_iri.to_owned()))?;
            let (declared_completeness_roots, grounding_laws) = original_vocabulary(dataset);
            // Keep the original constraint-root ownership control beside every current
            // schema root. The schema census itself comes from the native source.
            let roots: BTreeSet<_> = declared_completeness_roots
                .iter()
                .cloned()
                .chain(
                    [
                        "relatorMediationComplete",
                        "referenceFrameComplete",
                        "wemiChainComplete",
                        "besForall",
                    ]
                    .into_iter()
                    .map(|name| format!("{LOGIC}{name}")),
                )
                .collect();
            let reconstructed = roots
                .into_iter()
                .map(|iri| {
                    let formula = reconstruct_formula(dataset, &iri).map_err(super::record_failure);
                    (iri, formula)
                })
                .collect();
            Ok(Observations {
                producer: BuildIdentity::current(),
                source_path: SOURCE.to_owned(),
                source_digest: source_digest.clone(),
                source_iri: source_iri.to_owned(),
                diagnostics: compiled.diagnostics().to_vec(),
                declared_completeness_roots,
                reconstructed,
                top_level_formulas: compiled.program().formulas.clone(),
                domain_axiom_predicates: compiled
                    .program()
                    .axioms
                    .iter()
                    .map(|axiom| axiom.predicate.clone())
                    .collect(),
                grounding_laws,
            })
        },
    )?;
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&observed).map_err(fail)?,
    );
    Ok(())
}

/// Read only original default-graph assertions, matching the former Dataset query
/// selection. Named-graph declarations never become source-level vocabulary here.
fn original_vocabulary(dataset: &RdfDataset) -> (BTreeSet<String>, BTreeMap<String, Endpoints>) {
    let mut schemas = BTreeSet::new();
    let mut laws = BTreeSet::new();
    let mut roots: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut endpoints: BTreeMap<String, Endpoints> = BTreeMap::new();
    for quad in dataset.owned_quads() {
        if quad.graph_name.is_some() {
            continue;
        }
        let (RdfTerm::Iri(subject), RdfTerm::Iri(object)) = (quad.subject, quad.object) else {
            continue;
        };
        match quad.predicate.as_str() {
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type" => match object.as_str() {
                "https://blackcatinformatics.ca/logic/AbductiveSchema" => {
                    schemas.insert(subject);
                }
                "https://blackcatinformatics.ca/logic/GroundingCorrespondence" => {
                    laws.insert(subject);
                }
                _ => {}
            },
            "https://blackcatinformatics.ca/logic/completenessFormula" => {
                roots.entry(subject).or_default().push(object)
            }
            "https://blackcatinformatics.ca/logic/sourceEndpoint" => {
                endpoints.entry(subject).or_default().sources.push(object)
            }
            "https://blackcatinformatics.ca/logic/targetEndpoint" => {
                endpoints.entry(subject).or_default().targets.push(object)
            }
            _ => {}
        }
    }
    let completeness = schemas
        .into_iter()
        .flat_map(|schema| roots.remove(&schema).unwrap_or_default())
        .collect();
    let grounding = laws
        .into_iter()
        .map(|law| {
            let values = endpoints.remove(&law).unwrap_or_default();
            (law, values)
        })
        .collect();
    (completeness, grounding)
}

fn fail(error: impl std::fmt::Display) -> gmeow_errors::Diag {
    super::stage_err(&error.to_string())
}
