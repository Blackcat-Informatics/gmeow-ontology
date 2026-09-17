// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Producer observations for the authored GMN signature and glyph verify queries.
//! One explicitly selected default-graph substrate is shared across all cases.
//! This is an ABox/schema query contract, not a logical assertion of source imports
//! or examples, a reasoning closure, or a correspondence rewrite certificate.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use gmeow_action_cache::{
    ActionInput, ActionStore, FileKind, STORE_FORMAT_VERSION, StoreLimits, bytes_digest,
};
use purrdf::sparql::{NativeSparqlEngine, PreparedQuery, QueryOptions};
use purrdf::{DatasetMut, MutableDataset, RdfDataset, SparqlResult, parse_dataset};
use serde::{Deserialize, Serialize};

use crate::stages::parse_sources::SourceCatalog;

mod logic;
mod math;

pub(crate) const CHANNEL: &str = "pipeline/gmn-signature-observations.json";
const MODULES: [&str; 3] = [
    "slices/grounding/lang/module.ttl",
    "slices/grounding/math/module.ttl",
    "slices/grounding/logic/module.ttl",
];

struct Case {
    name: &'static str,
    query_path: &'static str,
    injection: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct Observations {
    pub modules: BTreeMap<String, String>,
    pub cases: BTreeMap<String, CaseObservation>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct CaseObservation {
    pub query_path: String,
    pub query_digest: String,
    pub injection_digest: Option<String>,
    pub rows: Result<usize, gmeow_errors::RecordedDiag>,
}

struct QuerySource {
    text: String,
    digest: String,
    prepared: OnceLock<Arc<PreparedQuery>>,
}

fn cases() -> Vec<Case> {
    logic::cases().into_iter().chain(math::cases()).collect()
}

pub(super) fn input_files(root: &Path) -> Vec<PathBuf> {
    MODULES
        .into_iter()
        .chain(cases().into_iter().map(|case| case.query_path))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|path| root.join(path))
        .collect()
}

fn raw(path: &str, digest: String) -> ActionInput {
    ActionInput::Raw {
        logical_path: path.to_owned(),
        file_kind: FileKind::File,
        executable: false,
        digest,
    }
}

/// Preserve the former tests' selected default-graph verify substrate, using
/// original admitted native documents without another authored-source parse.
/// The union is materialized only once, only when an observation actually misses.
fn grounding_graph(catalog: &SourceCatalog) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    let mut merged = MutableDataset::new(Arc::new(RdfDataset::union(&[])));
    for path in MODULES {
        let document = catalog.document(path)?;
        for quad in document.flat_default_graph_quads() {
            merged.insert(quad).map_err(gmeow_errors::Diag::from)?;
        }
    }
    merged.freeze().map_err(gmeow_errors::Diag::from)
}

fn observe(
    engine: &NativeSparqlEngine,
    base: &Arc<RdfDataset>,
    prepared: &PreparedQuery,
    injection: Option<&str>,
) -> Result<usize, gmeow_errors::Diag> {
    let result = match injection {
        Some(text) => {
            // A control adds only its tiny delta. The frozen authored base and
            // its indexes remain shared; no complete clone, freeze or reparse.
            let extra = parse_dataset(text.as_bytes(), "text/turtle", None)
                .map_err(gmeow_errors::Diag::from)?;
            let mut overlay = MutableDataset::new(base.clone());
            for quad in extra.flat_default_graph_quads() {
                overlay.insert(quad).map_err(gmeow_errors::Diag::from)?;
            }
            let view = overlay.snapshot_view().map_err(gmeow_errors::Diag::from)?;
            engine.query_prepared_view(&view, prepared, &[], QueryOptions::EMPTY)
        }
        None => engine.query_prepared(base, prepared, &[], QueryOptions::EMPTY),
    }
    .map_err(gmeow_errors::Diag::from)?;
    match result {
        SparqlResult::Solutions { rows, .. } => Ok(rows.len()),
        other => Err(super::stage_err(&format!(
            "a GMN verify query must be a SELECT, got {other:?}"
        ))),
    }
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
    let modules = MODULES
        .into_iter()
        .map(|path| Ok((path.to_owned(), catalog.document_digest(path)?.to_owned())))
        .collect::<gmeow_errors::Result<BTreeMap<_, _>>>()?;
    let shared: Vec<_> = modules
        .iter()
        .map(|(path, digest)| raw(path, digest.clone()))
        .collect();
    let cases = cases();
    let queries = cases
        .iter()
        .map(|case| case.query_path)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(root.join(path))
                .map_err(|error| fail(format!("read {path}: {error}")))?;
            Ok((
                path,
                QuerySource {
                    digest: bytes_digest(text.as_bytes()),
                    text,
                    prepared: OnceLock::new(),
                },
            ))
        })
        .collect::<gmeow_errors::Result<BTreeMap<_, _>>>()?;
    let base = OnceLock::new();
    let engine = NativeSparqlEngine::new();
    let mut observed = BTreeMap::new();
    for case in cases {
        let query = &queries[case.query_path];
        let injection_digest = case
            .injection
            .as_ref()
            .map(|text| bytes_digest(text.as_bytes()));
        let mut inputs = shared.clone();
        inputs.push(raw(case.query_path, query.digest.clone()));
        if let Some(digest) = &injection_digest {
            inputs.push(ActionInput::Raw {
                logical_path: format!("gmn-signature-controls/{}", case.name),
                file_kind: FileKind::Aggregate,
                executable: false,
                digest: digest.clone(),
            });
        }
        let rows = super::execution::cached_inputs(
            &store,
            &format!("gmn-signature-v1:{}", case.name),
            inputs,
            || {
                let base = base.get_or_try_init(|| grounding_graph(catalog))?;
                let prepared = query.prepared.get_or_try_init(|| {
                    engine
                        .prepare_query(&query.text, None)
                        .map_err(gmeow_errors::Diag::from)
                })?;
                observe(&engine, base, prepared, case.injection.as_deref())
            },
        )
        .map_err(super::record_failure);
        if observed
            .insert(
                case.name.to_owned(),
                CaseObservation {
                    query_path: case.query_path.to_owned(),
                    query_digest: query.digest.clone(),
                    injection_digest,
                    rows,
                },
            )
            .is_some()
        {
            return Err(fail(format!("duplicate GMN signature case {}", case.name)));
        }
    }
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&Observations {
            modules,
            cases: observed,
        })
        .map_err(|error| fail(error.to_string()))?,
    );
    Ok(())
}
