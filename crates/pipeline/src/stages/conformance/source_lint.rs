// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMEOW structural-lint observations over original isolated source fixtures.
//! Math lowering passes its already parsed negative fixtures through this seam;
//! remaining inputs execute in independent bounded source actions.

#[cfg(test)]
#[path = "source_lint.json.tests.rs"]
mod json_tests;

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use gmeow_action_cache::{
    ActionInput, ActionStore, FileKind, STORE_FORMAT_VERSION, StoreLimits, bytes_digest,
};
use gmeow_validate::lint::{LintConfig, structural_lint_dataset};
use purrdf::RdfDataset;

mod sources;

pub(super) const CHANNEL: &str = "pipeline/authored-lint-observations.json";
const PROFILE: &str = "structural-source-fixtures:original-native:v1";

pub(super) use gmeow_validate::lint::source_observation::SourceRecord;
use gmeow_validate::lint::source_observation::{Configuration, Observations};

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
    let bytes = purrdf_lex::json::record::to_vec(&Configuration::from(&configuration()));
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
        let record = super::execution::cached_native_inputs(
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
        purrdf_lex::json::record::to_vec(&Observations {
            profile: PROFILE.to_owned(),
            configuration: Configuration::from(&configuration()),
            sources: observations,
        }),
    );
    Ok(())
}

fn fail(error: impl std::fmt::Display) -> gmeow_errors::Diag {
    super::stage_err(&error.to_string())
}
