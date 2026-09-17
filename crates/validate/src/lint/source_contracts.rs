// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact selected source-lint findings; no authored fixture parsing in unit tests.

use gmeow_errors::{Severity, ledger::DiagNode};
use gmeow_logic::verify::PreparedVerification;
use serde::Deserialize;
use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::OnceLock;

#[derive(Deserialize)]
struct Observations {
    profile: String,
    configuration: Configuration,
    sources: BTreeMap<String, SourceRecord>,
}

#[derive(Deserialize)]
pub(super) struct SourceRecord {
    source_digest: String,
    diagnostics: Vec<DiagNode>,
}

impl SourceRecord {
    pub(super) fn errors(&self) -> Vec<String> {
        self.diagnostics
            .iter()
            .filter(|node| node.grade.severity == Severity::Error)
            .flat_map(|node| {
                node.observations
                    .iter()
                    .map(|observation| observation.message.clone())
            })
            .collect()
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct Configuration {
    namespace: String,
    ontology_iri: String,
    selector_tokens: BTreeSet<String>,
    core_slice_iris: BTreeSet<String>,
    annotation_predicates: BTreeSet<String>,
}

pub(super) fn report(path: &str, config: &super::LintConfig) -> &'static SourceRecord {
    struct Selected {
        selector: String,
        observed: Observations,
    }
    static OBSERVED: OnceLock<gmeow_errors::Result<Selected>> = OnceLock::new();
    let selector = std::env::var(gmeow_action_cache::selection::MANIFEST_SHA256_ENV)
        .expect("source lint needs the exact producer selector");
    let selected = OBSERVED
        .get_or_init(|| {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            let bytes = gmeow_action_cache::selection::source_artifacts::load(
                &root,
                "stage-conformance",
                "pipeline/authored-lint-observations.json",
            )
            .map_err(gmeow_errors::Diag::from)?;
            let observed = serde_json::from_slice(&bytes).map_err(gmeow_errors::Diag::from)?;
            Ok(Selected {
                selector: selector.clone(),
                observed,
            })
        })
        .as_ref()
        .unwrap_or_else(|error| panic!("authenticated source lint: {error}"));
    assert_eq!(
        selected.selector, selector,
        "source lint cannot cross selector identities"
    );
    let observed = &selected.observed;
    assert_eq!(
        observed.profile,
        "structural-source-fixtures:original-native:v1"
    );
    assert_eq!(
        observed.configuration,
        Configuration {
            namespace: config.namespace.clone(),
            ontology_iri: config.ontology_iri.clone(),
            selector_tokens: config.selector_tokens.clone(),
            core_slice_iris: config.core_slice_iris.iter().cloned().collect(),
            annotation_predicates: config.annotation_predicates.iter().cloned().collect(),
        },
        "the observed lint must use the original independently specified test configuration"
    );
    assert_eq!(
        observed.sources.len(),
        41,
        "all original authored lint sources"
    );
    let source = observed
        .sources
        .get(path)
        .unwrap_or_else(|| panic!("missing selected structural source {path}"));
    assert!(
        source.source_digest.len() == 64
            && source
                .source_digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
    );
    source
}

/// The tiny synthetic homogeneity control evaluates using authenticated native
/// laws, never through the compatibility wrapper's embedded-source preparation.
pub(super) fn verification() -> Rc<PreparedVerification<'static>> {
    thread_local! {
        static VERIFY: OnceCell<gmeow_errors::Result<Rc<PreparedVerification<'static>>>> =
            const { OnceCell::new() };
    }
    let gates = crate::validate_all::verification_fixture::gates();
    VERIFY.with(|verification| {
        Rc::clone(
            verification
                .get_or_init(|| {
                    PreparedVerification::new(
                        &gmeow_logic::verify::embedded_verify_queries(),
                        gates,
                    )
                    .map(Rc::new)
                })
                .as_ref()
                .unwrap_or_else(|error| panic!("prepare native gate queries: {error}")),
        )
    })
}
