// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMEOW structural-lint observations over original isolated source fixtures.
//! Math lowering passes its already parsed negative fixtures through this seam;
//! remaining inputs execute in independent bounded source actions.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use gmeow_action_cache::{
    ActionInput, ActionStore, FileKind, STORE_FORMAT_VERSION, StoreLimits, bytes_digest,
};
use gmeow_errors::ledger::DiagNode;
use gmeow_validate::lint::{LintConfig, structural_lint_dataset};
use purrdf::RdfDataset;
use serde::{Deserialize, Serialize};

mod sources;

pub(super) const CHANNEL: &str = "pipeline/authored-lint-observations.json";
const PROFILE: &str = "structural-source-fixtures:original-native:v1";

#[derive(Serialize, Deserialize)]
struct Observations {
    profile: String,
    configuration: Configuration,
    sources: BTreeMap<String, SourceRecord>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct SourceRecord {
    pub source_digest: String,
    pub diagnostics: Vec<DiagNode>,
}

#[derive(Serialize, Deserialize)]
struct Configuration {
    namespace: String,
    ontology_iri: String,
    selector_tokens: BTreeSet<String>,
    core_slice_iris: BTreeSet<String>,
    annotation_predicates: BTreeSet<String>,
}

impl From<&LintConfig> for Configuration {
    fn from(config: &LintConfig) -> Self {
        Self {
            namespace: config.namespace.clone(),
            ontology_iri: config.ontology_iri.clone(),
            selector_tokens: config.selector_tokens.clone(),
            core_slice_iris: config.core_slice_iris.iter().cloned().collect(),
            annotation_predicates: config.annotation_predicates.iter().cloned().collect(),
        }
    }
}

fn configuration() -> LintConfig {
    LintConfig {
        namespace: "https://blackcatinformatics.ca/gmeow/".to_owned(),
        ontology_iri: "https://blackcatinformatics.ca/gmeow".to_owned(),
        selector_tokens: ["primary", "preferred", "default", "main"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        core_slice_iris: HashSet::new(),
        annotation_predicates: [
            "http://www.w3.org/2000/01/rdf-schema#label",
            "http://www.w3.org/2004/02/skos/core#definition",
            "http://www.w3.org/2000/01/rdf-schema#comment",
            "http://purl.org/dc/terms/title",
            "http://purl.org/dc/terms/description",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
    }
}

pub(super) fn selected(path: &str) -> bool {
    sources::SOURCES.contains(&path)
}

pub(super) fn configuration_input() -> gmeow_errors::Result<ActionInput> {
    let bytes = serde_json::to_vec(&Configuration::from(&configuration())).map_err(fail)?;
    Ok(ActionInput::Raw {
        logical_path: "structural-source-fixtures/configuration".to_owned(),
        file_kind: FileKind::Aggregate,
        executable: false,
        digest: bytes_digest(&bytes),
    })
}

/// Observe an already parsed source in its original native graph role.
pub(super) fn observe(path: &str, digest: &str, dataset: &RdfDataset) -> Option<SourceRecord> {
    selected(path).then(|| SourceRecord {
        source_digest: digest.to_owned(),
        diagnostics: structural_lint_dataset(dataset, &configuration())
            .ledger()
            .emit_sorted()
            .into_iter()
            .cloned()
            .collect(),
    })
}

pub(super) fn input_files(root: &Path) -> Vec<PathBuf> {
    sources::SOURCES
        .into_iter()
        .map(|path| root.join(path))
        .collect()
}

/// Merge native math-fixture observations without serializing/parsing a handoff.
/// An omitted selected math negative is an error, never another parsing path.
pub(super) fn record(
    root: &Path,
    mut observations: BTreeMap<String, SourceRecord>,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let store = ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(fail)?;
    let config_input = configuration_input()?;
    for path in sources::SOURCES {
        if observations.contains_key(path) {
            continue;
        }
        if path.starts_with("slices/grounding/math/tests/counter-examples/") {
            return Err(fail(format!(
                "math source preparation omitted selected structural-lint input {path}"
            )));
        }
        let bytes = std::fs::read(root.join(path)).map_err(fail)?;
        let digest = bytes_digest(&bytes);
        let record = super::execution::cached_inputs(
            &store,
            PROFILE,
            vec![
                ActionInput::Raw {
                    logical_path: path.to_owned(),
                    file_kind: FileKind::File,
                    executable: false,
                    digest: digest.clone(),
                },
                config_input.clone(),
            ],
            || {
                let dataset = purrdf::parse_dataset(&bytes, "text/turtle", None)
                    .map_err(gmeow_errors::Diag::from)?;
                observe(path, &digest, &dataset).ok_or_else(|| {
                    super::stage_err(&format!("unselected structural source {path}"))
                })
            },
        )?;
        observations.insert(path.to_owned(), record);
    }
    let expected: BTreeSet<_> = sources::SOURCES.into_iter().collect();
    if observations
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        != expected
    {
        return Err(fail(
            "structural-lint source observations do not match the exact selected inventory",
        ));
    }
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&Observations {
            profile: PROFILE.to_owned(),
            configuration: Configuration::from(&configuration()),
            sources: observations,
        })
        .map_err(fail)?,
    );
    Ok(())
}

fn fail(error: impl std::fmt::Display) -> gmeow_errors::Diag {
    super::stage_err(&error.to_string())
}
