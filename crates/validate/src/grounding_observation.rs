// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Original grounding envelopes and declaration inventories shared by producer and reader.

#[cfg(not(target_arch = "wasm32"))]
mod json;
use std::collections::{BTreeMap, BTreeSet};

/// Complete original-source correspondence observations.
#[derive(Debug)]
pub struct Observations {
    /// Original source paths and exact byte digests.
    pub sources: BTreeMap<String, String>,
    /// Ordered correspondence envelopes per original catalog.
    pub catalogs: BTreeMap<String, Vec<Cell>>,
    /// Exact correspondence count or complete recorded failure.
    pub transpilation: BTreeMap<String, Result<usize, gmeow_errors::RecordedDiag>>,
    /// Named subjects declared by each selected original source.
    pub declared_subjects: BTreeMap<String, BTreeSet<String>>,
    /// Imported class declarations by source.
    pub imported_classes: BTreeMap<String, BTreeSet<String>>,
    /// Original BFO labels grouped by named subject.
    pub bfo_labels: BTreeMap<String, BTreeSet<String>>,
}

/// Extracted correspondence envelope, preserving numeric RDF identity.
#[derive(Debug, Clone)]
pub struct Cell {
    /// Correspondence provenance IRI derived by the alignment owner.
    pub iri: String,
    /// Original subject coordinate.
    pub subject: String,
    /// Original predicate coordinate.
    pub predicate: String,
    /// Original target object spelling.
    pub obj: String,
    /// Original source endpoint coordinate.
    pub source_endpoint: Option<String>,
    /// Original target endpoint coordinate.
    pub target_endpoint: Option<String>,
    /// Original correspondence document path.
    pub sssom_file: String,
    /// Original morphism class coordinate.
    pub morphism_class: Option<String>,
    /// Original morphism kind coordinate.
    pub morphism_kind: Option<String>,
    /// Original preservation coordinate.
    pub preservation: Option<String>,
    /// Optional checked confidence, preserving its numeric RDF literal identity.
    pub confidence: Option<gmeow_logic_compile::ir::UnitInterval>,
    /// Whether the extracted envelope is a grounding correspondence.
    pub grounding: bool,
}

/// A cached correspondence count or complete diagnostic, with strict count admission.
#[derive(Debug)]
pub struct TranspilationOutcome(pub Result<usize, gmeow_errors::RecordedDiag>);
