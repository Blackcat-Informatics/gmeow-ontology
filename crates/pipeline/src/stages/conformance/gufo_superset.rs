// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Source-local observations for the canonical logic ⊇ gUFO product contract.
//! Imports, module declarations and the worked example retain separate identities.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gmeow_action_cache::{ActionInput, ActionStore, FileKind, STORE_FORMAT_VERSION, StoreLimits};
use purrdf::{RdfDataset, RdfTerm};
use serde::{Deserialize, Serialize};

use crate::stages::parse_sources::SourceCatalog;

pub(crate) const CHANNEL: &str = "pipeline/gufo-superset-import-observation.json";
pub(super) const GUFO_TTL: &str = "imports/gufo.ttl";
pub(super) const MODULE_TTL: &str = "slices/grounding/logic/module.ttl";
pub(super) const EXAMPLE_TTL: &str = "slices/grounding/logic/examples/criticism-fixes.ttl";

/// The term-kind and exact terminal surface needed by the product assertions.
/// Consumers inspect these observations; they never parse them into a dataset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum ObservedTerm {
    Iri(String),
    Literal(String),
    BlankNode(String),
    Triple(String),
}

impl ObservedTerm {
    fn from_native(term: &RdfTerm) -> Self {
        match term {
            RdfTerm::Iri(iri) => Self::Iri(iri.clone()),
            RdfTerm::Literal(_) => Self::Literal(purrdf::turtle::emit_term(term)),
            RdfTerm::BlankNode(label) => Self::BlankNode(label.clone()),
            RdfTerm::Triple(_) => Self::Triple(purrdf::turtle::emit_term(term)),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(super) struct SourceObservation {
    pub source_path: String,
    pub default_quad_count: usize,
    pub subjects: BTreeSet<String>,
    pub pairs: BTreeMap<String, Vec<(String, ObservedTerm)>>,
}

pub(super) fn input_files(root: &Path) -> Vec<PathBuf> {
    [GUFO_TTL, MODULE_TTL, EXAMPLE_TTL]
        .into_iter()
        .map(|path| root.join(path))
        .collect()
}

pub(super) fn observe(path: &str, dataset: &RdfDataset) -> Option<SourceObservation> {
    matches!(path, MODULE_TTL | EXAMPLE_TTL).then(|| summarize(path, dataset))
}

fn summarize(path: &str, dataset: &RdfDataset) -> SourceObservation {
    const PREDICATES: &[&str] = &[
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#subject",
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#predicate",
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#object",
        "https://blackcatinformatics.ca/gmeow/graphBoxRole",
        "https://blackcatinformatics.ca/logic/properPartOf",
        "https://blackcatinformatics.ca/logic/instanceOf",
        "https://blackcatinformatics.ca/logic/orderedType",
        "https://blackcatinformatics.ca/logic/invokesBuiltin",
        "https://blackcatinformatics.ca/gmeow/examples/logic/validFrom",
        "https://blackcatinformatics.ca/gmeow/examples/logic/validTo",
    ];
    let mut result = SourceObservation {
        source_path: path.to_owned(),
        default_quad_count: 0,
        subjects: BTreeSet::new(),
        pairs: BTreeMap::new(),
    };
    for quad in dataset.owned_quads().filter(|q| q.graph_name.is_none()) {
        result.default_quad_count += 1;
        if let RdfTerm::Iri(subject) = quad.subject {
            result.subjects.insert(subject.clone());
            if PREDICATES.contains(&quad.predicate.as_str()) {
                result
                    .pairs
                    .entry(quad.predicate)
                    .or_default()
                    .push((subject, ObservedTerm::from_native(&quad.object)));
            }
        }
    }
    result
}

pub(super) fn record(
    root: &Path,
    catalog: &SourceCatalog,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let fail = |error: String| super::stage_err(&error);
    let store = ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(|error| fail(error.to_string()))?;
    let result = super::execution::cached_inputs(
        &store,
        "gufo-superset-import-v1",
        vec![ActionInput::Raw {
            logical_path: GUFO_TTL.to_owned(),
            file_kind: FileKind::File,
            executable: false,
            digest: catalog.document_digest(GUFO_TTL)?.to_owned(),
        }],
        || Ok(summarize(GUFO_TTL, catalog.document(GUFO_TTL)?)),
    )?;
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&result).map_err(|error| fail(error.to_string()))?,
    );
    Ok(())
}

#[cfg(test)]
mod test_support;
