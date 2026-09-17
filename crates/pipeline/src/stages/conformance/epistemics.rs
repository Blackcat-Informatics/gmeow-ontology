// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Producer-owned epistemics instance probes over the original native module.
//! Each selected ABox is independently cached; expected memberships remain solely
//! in the consumer. No source parse or cumulative closure cache is introduced.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use gmeow_action_cache::{ActionInput, ActionStore, FileKind, STORE_FORMAT_VERSION, StoreLimits};
use gmeow_logic::reason::{RlTriple, rl_closure};
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm, TermRef};
use serde::{Deserialize, Serialize};

use crate::stages::parse_sources::SourceCatalog;

pub(crate) const CHANNEL: &str = "pipeline/epistemics-instance-observations.json";
const SOURCE: &str = "slices/core/epistemics/module.ttl";
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const EX: &str = "https://example.org/test/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";

#[derive(Serialize, Deserialize)]
pub(super) struct Observation {
    pub universe: BTreeSet<String>,
    pub probes: BTreeMap<String, Result<Probe, gmeow_errors::RecordedDiag>>,
}

/// The complete probe answer domain plus the size of its actual full closure.
/// Retaining untouched module facts five times would inflate the intermediate
/// without adding any evidence to the selected instance queries.
#[derive(Serialize, Deserialize)]
pub(super) struct Probe {
    pub total_triples: usize,
    pub triples: Vec<RlTriple>,
}

fn class_universe(dataset: &RdfDataset) -> BTreeSet<String> {
    let mut classes = BTreeSet::new();
    for quad in dataset.quads() {
        let selected = match dataset.resolve(quad.p) {
            TermRef::Iri(RDFS_SUBCLASS_OF) => Some(quad.s),
            TermRef::Iri(RDF_FIRST) => Some(quad.o),
            _ => None,
        };
        if let Some(term) = selected
            && let TermRef::Iri(iri) = dataset.resolve(term)
            && iri.starts_with(GMEOW)
        {
            classes.insert(iri.to_owned());
        }
    }
    classes
}

fn iri_quad(subject: &str, predicate: &str, object: &str) -> RdfQuad {
    RdfQuad::new(RdfTerm::iri(subject), predicate, RdfTerm::iri(object))
}

pub(super) fn record(
    root: &Path,
    catalog: &SourceCatalog,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let source = catalog.document(SOURCE)?;
    let universe = class_universe(source);
    let store = ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(|error| super::stage_err(&error.to_string()))?;
    let mut selections = BTreeMap::new();
    selections.insert(
        "knowsThat",
        vec![iri_quad(
            &format!("{EX}llm"),
            &format!("{GMEOW}knowsThat"),
            &format!("{EX}propRecall"),
        )],
    );
    for sibling in ["knowsThatIn", "claimsToKnowThat", "takesAsKnown"] {
        selections.insert(
            sibling,
            vec![iri_quad(
                &format!("{EX}agentFor"),
                &format!("{GMEOW}{sibling}"),
                &format!("{EX}propFor"),
            )],
        );
    }
    selections.insert(
        "class-universe",
        universe
            .iter()
            .enumerate()
            .map(|(index, class)| iri_quad(&format!("{EX}probe{index}"), RDF_TYPE, class))
            .collect(),
    );
    let mut probes = BTreeMap::new();
    for (name, abox) in selections {
        let observed = super::execution::cached_inputs(
            &store,
            &format!("epistemics-instance-v1:{name}"),
            vec![ActionInput::Raw {
                logical_path: SOURCE.to_owned(),
                file_kind: FileKind::File,
                executable: false,
                digest: catalog.document_digest(SOURCE)?.to_owned(),
            }],
            || {
                let mut builder = RdfDatasetBuilder::new();
                // Native arena transfer carries reifiers, annotations and graph identity
                // as well as triples. Only this operation's small probe is appended.
                builder.push_dataset(source);
                for quad in &abox {
                    builder.push_owned_quad(quad);
                }
                let dataset = builder.freeze().map_err(gmeow_errors::Diag::from)?;
                let subjects: BTreeSet<_> = abox
                    .iter()
                    .filter_map(|quad| match &quad.subject {
                        RdfTerm::Iri(iri) => Some(iri.as_str()),
                        _ => None,
                    })
                    .collect();
                rl_closure(&dataset).map(|result| Probe {
                    total_triples: result.triples.len(),
                    triples: result
                        .triples
                        .into_iter()
                        .filter(|triple| subjects.contains(triple.subject.as_str()))
                        .collect(),
                })
            },
        )
        .map_err(super::record_failure);
        probes.insert(name.to_owned(), observed);
    }
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&Observation { universe, probes })
            .map_err(|error| super::stage_err(&error.to_string()))?,
    );
    Ok(())
}
