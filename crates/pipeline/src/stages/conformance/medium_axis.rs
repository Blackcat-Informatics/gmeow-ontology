// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Raw medium declaration cardinalities beside the shared validated native registry.
//! Only the named structural observations are exported; no RDF term dictionary,
//! corpus reconstruction or second MediumRegistry preparation is involved.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gmeow_action_cache::{ActionInput, ActionStore, FileKind, STORE_FORMAT_VERSION, StoreLimits};
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermId, TermRef};
use serde::{Deserialize, Serialize};

use crate::stages::parse_sources::SourceCatalog;

pub(super) const CHANNEL: &str = "pipeline/medium-axis-observations.json";
const SOURCE: &str = crate::medium::source_observation::SOURCE;
const GM: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const CLASSES: [&str; 19] = [
    "CompressionDictionary",
    "CompressionDictionaryRealization",
    "CorpusTrainingSplit",
    "DictionaryCorpus",
    "DictionaryStrategy",
    "DigestStratum",
    "GtsConformanceFailure",
    "Medium",
    "MediumCorpusDrift",
    "MediumDictionaryRegression",
    "MediumDigestMismatch",
    "MediumEnvelope",
    "MediumOpaqueFrame",
    "MediumSourceKind",
    "MediumUndeclaredDictionary",
    "MediumUnknownDictionary",
    "MediumUnknownSchema",
    "PayloadSchema",
    "ZstdDictMedium",
];

#[derive(Serialize, Deserialize)]
struct Observations {
    source_path: String,
    source_digest: String,
    dictionaries: Vec<Node>,
    corpora: Vec<Node>,
    splits: Vec<Node>,
    schemas: Vec<Node>,
    classes: BTreeMap<String, Option<Node>>,
    gmn_terms: BTreeSet<String>,
    shacl_terms: BTreeSet<String>,
}

#[derive(Serialize, Deserialize)]
struct Node {
    name: Value,
    values: BTreeMap<String, Vec<Value>>,
}

#[derive(Serialize, Deserialize)]
enum Value {
    Iri(String),
    Literal(String),
    Other(String),
}

fn value(term: TermRef<'_>) -> Value {
    match term {
        TermRef::Iri(iri) => Value::Iri(iri.to_owned()),
        TermRef::Literal { lexical, .. } => Value::Literal(lexical.to_owned()),
        other => Value::Other(format!("{other:?}")),
    }
}

pub(super) fn input_files(root: &Path) -> Vec<PathBuf> {
    vec![root.join(SOURCE)]
}

pub(super) fn record(
    root: &Path,
    catalog: &SourceCatalog,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let store = ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(fail)?;
    let digest = catalog.document_digest(SOURCE)?;
    let observed = super::execution::cached_inputs(
        &store,
        "medium-axis-v1:original-all-graphs",
        vec![ActionInput::Raw {
            logical_path: SOURCE.to_owned(),
            file_kind: FileKind::File,
            executable: false,
            digest: digest.to_owned(),
        }],
        || {
            let dataset = catalog.document(SOURCE)?;
            let mut gmn_terms = BTreeSet::new();
            let mut shacl_terms = BTreeSet::new();
            for quad in dataset.quad_refs() {
                for term in [quad.s, quad.p, quad.o] {
                    if let TermRef::Iri(iri) = term {
                        if iri.strip_prefix(GM).is_some_and(|local| {
                            local.starts_with("gmn") || local.starts_with("Gmn")
                        }) {
                            gmn_terms.insert(iri.to_owned());
                        }
                        if iri.starts_with("http://www.w3.org/ns/shacl#") {
                            shacl_terms.insert(iri.to_owned());
                        }
                    }
                }
            }
            Ok(Observations {
                source_path: SOURCE.to_owned(),
                source_digest: digest.to_owned(),
                dictionaries: instances(
                    dataset,
                    "CompressionDictionary",
                    &["dictionaryId", "trainsOverCorpus"],
                ),
                corpora: instances(
                    dataset,
                    "DictionaryCorpus",
                    &[
                        "corpusSelectsBlobRep",
                        "corpusSelectsGraph",
                        "corpusSelectsPathPrefix",
                        "corpusSelectsStageProduct",
                        "splitHeldOutStride",
                        "splitHeldOutOffset",
                    ],
                ),
                splits: instances(
                    dataset,
                    "CorpusTrainingSplit",
                    &["splitHeldOutStride", "splitHeldOutOffset"],
                ),
                schemas: instances(dataset, "PayloadSchema", &["payloadSchemaId"]),
                classes: CLASSES
                    .into_iter()
                    .map(|local| {
                        let iri = format!("{GM}{local}");
                        let declaration = dataset
                            .term_id_by_iri(&iri)
                            .map(|term| node(dataset, term, &["docsConcern"], true));
                        (iri, declaration)
                    })
                    .collect(),
                gmn_terms,
                shacl_terms,
            })
        },
    )?;
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&observed).map_err(fail)?,
    );
    Ok(())
}

fn instances(dataset: &RdfDataset, class: &str, properties: &[&str]) -> Vec<Node> {
    let (Some(predicate), Some(class)) = (
        dataset.term_id_by_iri(RDF_TYPE),
        dataset.term_id_by_iri(&format!("{GM}{class}")),
    ) else {
        return Vec::new();
    };
    dataset
        .quads_for_pattern(None, Some(predicate), Some(class), GraphMatch::Any)
        .map(|quad| node(dataset, quad.s, properties, false))
        .collect()
}

fn node(dataset: &RdfDataset, subject: TermId, properties: &[&str], include_types: bool) -> Node {
    let mut predicates: Vec<_> = properties
        .iter()
        .map(|local| format!("{GM}{local}"))
        .collect();
    if include_types {
        predicates.push(RDF_TYPE.to_owned());
    }
    let values = predicates
        .into_iter()
        .map(|iri| {
            let values = dataset
                .term_id_by_iri(&iri)
                .map_or_else(Vec::new, |predicate| {
                    dataset
                        .quads_for_pattern(Some(subject), Some(predicate), None, GraphMatch::Any)
                        .map(|quad| value(dataset.resolve(quad.o)))
                        .collect()
                });
            (iri, values)
        })
        .collect();
    Node {
        name: value(dataset.resolve(subject)),
        values,
    }
}

fn fail(error: impl std::fmt::Display) -> gmeow_errors::Diag {
    super::stage_err(&error.to_string())
}
