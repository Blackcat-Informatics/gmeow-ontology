// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Closed source-lint transport shared by publication and authenticated reading.
//! Selection, parsing and artifact authentication remain with their callers.

mod json;
use gmeow_errors::ledger::DiagNode;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Complete selected source inventory under one declared lint profile.
#[derive(Debug)]
pub struct Observations {
    /// Selected lint profile identity.
    pub profile: String,
    /// Exact configuration used to obtain these diagnostics.
    pub configuration: Configuration,
    /// Source paths and their independently observed native diagnostics.
    pub sources: BTreeMap<String, SourceRecord>,
}
/// Diagnostics and exact source identity, also embedded by grounding observations.
#[derive(Debug, Serialize, Deserialize)]
pub struct SourceRecord {
    /// Original source-byte digest.
    pub source_digest: String,
    /// Complete ordered diagnostics, retaining native identities and payloads.
    pub diagnostics: Vec<DiagNode>,
}
/// Deterministic configuration fields participating in source-action identity.
#[derive(Debug, PartialEq, Eq)]
pub struct Configuration {
    /// Namespace whose authored terms are checked.
    pub namespace: String,
    /// Ontology identity for source annotations.
    pub ontology_iri: String,
    /// Selector words subject to the lint policy.
    pub selector_tokens: BTreeSet<String>,
    /// Slice identities selected by the original configuration.
    pub core_slice_iris: BTreeSet<String>,
    /// Annotation predicates selected for native checking.
    pub annotation_predicates: BTreeSet<String>,
}
impl From<&super::LintConfig> for Configuration {
    fn from(config: &super::LintConfig) -> Self {
        Self {
            namespace: config.namespace.clone(),
            ontology_iri: config.ontology_iri.clone(),
            selector_tokens: config.selector_tokens.clone(),
            core_slice_iris: config.core_slice_iris.iter().cloned().collect(),
            annotation_predicates: config.annotation_predicates.iter().cloned().collect(),
        }
    }
}
