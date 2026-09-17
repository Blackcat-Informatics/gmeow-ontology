// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Actual native reasoned-graph verification of explicitly selected authored scenes.
//! Each bounded source action shares one parsed dataset and the producer's native law
//! and query preparation. Repairs are native single-statement deltas, not RDF reparses.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use gmeow_action_cache::{
    ActionInput, ActionStore, FileKind, STORE_FORMAT_VERSION, StoreLimits, bytes_digest,
};
use gmeow_errors::Report;
use gmeow_logic::verify::{
    GATE_SOURCES, PREPARED_GATES_CHANNEL, PreparedVerification, embedded_verify_queries,
};
use purrdf::{DatasetMut, MutableDataset, QuadValues, RdfDataset, TermValue};
use serde::{Deserialize, Serialize};

use crate::cache::BuildIdentity;
use crate::stages::parse_sources::SourceCatalog;

mod sources;

pub(super) const CHANNEL: &str = "pipeline/verify-gate-observations.json";

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Observations {
    pub producer: BuildIdentity,
    pub query_digests: BTreeMap<String, String>,
    pub query_set_digest: String,
    pub law_source_digests: BTreeMap<String, String>,
    pub sources: BTreeMap<String, SourceObservation>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct SourceObservation {
    pub source_path: String,
    pub source_digest: String,
    pub original: Report,
    pub repairs: BTreeMap<String, RepairObservation>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct RepairObservation {
    pub mutation_digest: String,
    pub removed_rows: usize,
    pub inserted_rows: usize,
    pub report: Report,
}

#[derive(Debug, Serialize)]
struct Repair {
    name: &'static str,
    subject: &'static str,
    predicate: &'static str,
    remove: Option<Object>,
    insert: Object,
}

#[derive(Debug, Serialize)]
enum Object {
    Iri(&'static str),
    String(&'static str),
}

impl Object {
    fn value(&self) -> TermValue {
        match self {
            Self::Iri(iri) => TermValue::iri(*iri),
            Self::String(text) => TermValue::simple_literal(*text),
        }
    }
}

struct Source {
    path: &'static str,
    repairs: Vec<Repair>,
}

fn raw(path: &str, digest: String) -> ActionInput {
    ActionInput::Raw {
        logical_path: path.to_owned(),
        file_kind: FileKind::File,
        executable: false,
        digest,
    }
}

pub(super) fn input_files(root: &Path) -> Vec<PathBuf> {
    sources::all()
        .into_iter()
        .map(|source| source.path)
        .chain(GATE_SOURCES.into_iter().map(|(path, _)| path))
        .map(|path| root.join(path))
        .collect()
}

pub(super) fn record(
    root: &Path,
    catalog: &SourceCatalog,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let gates = catalog.prepared_reasoned_gates().map_err(fail)?;
    artifacts.insert(
        PREPARED_GATES_CHANNEL.to_owned(),
        serde_json::to_vec(gates.as_ref()).map_err(fail)?,
    );
    let queries = embedded_verify_queries();
    let query_digests: BTreeMap<_, _> = queries
        .iter()
        .map(|(name, query)| (name.clone(), bytes_digest(query.as_bytes())))
        .collect();
    let query_set_digest = bytes_digest(&serde_json::to_vec(&query_digests).map_err(fail)?);
    let domains = gmeow_logic::reason::SelectedDomains::new([
        gmeow_logic::reason::SelectedLogicalWorld::new(
            gmeow_logic::reason::LogicalGraph::Default,
            gmeow_logic::reason::DomainProfile::NonemptyObjectDomainV1,
            "gmeow.pipeline.reasoned-verify-scenes.v1".to_owned(),
            *blake3::hash(
                b"gmeow.pipeline.reasoned-verify-scenes.v1/default/nonempty-object-domain-v1",
            )
            .as_bytes(),
        )?,
    ])?;
    let verifier = OnceLock::new();
    let store = ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(fail)?;
    let mut observations = BTreeMap::new();
    for source in sources::all() {
        let bytes = std::fs::read(root.join(source.path)).map_err(fail)?;
        let digest = bytes_digest(&bytes);
        let mut inputs = vec![
            raw(source.path, digest.clone()),
            raw("embedded-verify-query-set", query_set_digest.clone()),
        ];
        inputs.extend(
            gates
                .source_digests()
                .iter()
                .map(|(path, digest)| raw(path, digest.clone())),
        );
        for repair in &source.repairs {
            inputs.push(raw(
                repair.name,
                bytes_digest(&serde_json::to_vec(repair).map_err(fail)?),
            ));
        }
        let observed =
            super::execution::cached_inputs(&store, "reasoned-verify-scene-v1", inputs, || {
                let verifier =
                    verifier.get_or_try_init(|| PreparedVerification::new(&queries, &gates))?;
                let base = purrdf::parse_dataset(&bytes, "text/turtle", None)
                    .map_err(gmeow_errors::Diag::from)?;
                let original = verifier.verify(&base, &domains)?;
                let repairs = source
                    .repairs
                    .iter()
                    .map(|repair| {
                        observe_repair(&base, repair, verifier, &domains)
                            .map(|observed| (repair.name.to_owned(), observed))
                    })
                    .collect::<Result<_, gmeow_errors::Diag>>()?;
                Ok(SourceObservation {
                    source_path: source.path.to_owned(),
                    source_digest: digest.clone(),
                    original,
                    repairs,
                })
            })?;
        observations.insert(source.path.to_owned(), observed);
    }
    let observed = Observations {
        producer: BuildIdentity::current(),
        query_digests,
        query_set_digest,
        law_source_digests: gates.source_digests().clone(),
        sources: observations,
    };
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&observed).map_err(fail)?,
    );
    Ok(())
}

fn observe_repair(
    base: &Arc<RdfDataset>,
    repair: &Repair,
    verifier: &PreparedVerification<'_>,
    domains: &gmeow_logic::reason::SelectedDomains,
) -> Result<RepairObservation, gmeow_errors::Diag> {
    let mut edited = MutableDataset::new(base.clone());
    let quad = |object: &Object| QuadValues {
        s: TermValue::iri(repair.subject),
        p: TermValue::iri(repair.predicate),
        o: object.value(),
        g: None,
    };
    let removed_rows = usize::from(
        repair
            .remove
            .as_ref()
            .is_some_and(|value| edited.remove(&quad(value))),
    );
    let inserted_rows = usize::from(
        edited
            .insert(quad(&repair.insert))
            .map_err(gmeow_errors::Diag::from)?,
    );
    // These counts remain observations: read-only consumers independently require
    // the exact changed statement, and grade the actual complete verify report.
    let dataset = edited.freeze().map_err(gmeow_errors::Diag::from)?;
    let report = verifier.verify(&dataset, domains)?;
    Ok(RepairObservation {
        mutation_digest: bytes_digest(
            &serde_json::to_vec(repair).map_err(gmeow_errors::Diag::from)?,
        ),
        removed_rows,
        inserted_rows,
        report,
    })
}

fn fail(error: impl std::fmt::Display) -> gmeow_errors::Diag {
    super::stage_err(&error.to_string())
}
