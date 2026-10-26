// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Raw medium declaration observations, without registry reconstruction or RDF term decoding.

#[cfg(not(target_arch = "wasm32"))]
mod json;
use std::collections::{BTreeMap, BTreeSet};

/// Complete selected medium-axis source observations.
#[derive(Debug)]
pub struct Observations {
    /// Selected authored medium document.
    pub source_path: String,
    /// Exact original byte digest.
    pub source_digest: String,
    /// Ordered dictionary declarations.
    pub dictionaries: Vec<Node>,
    /// Ordered corpus declarations.
    pub corpora: Vec<Node>,
    /// Ordered split declarations.
    pub splits: Vec<Node>,
    /// Ordered schema declarations.
    pub schemas: Vec<Node>,
    /// Selected class keys, preserving explicit missing declarations.
    pub classes: BTreeMap<String, Option<Node>>,
    /// GMN names seen in the source.
    pub gmn_terms: BTreeSet<String>,
    /// SHACL names seen in the source.
    pub shacl_terms: BTreeSet<String>,
}

/// One observed node with its exact selected predicate inventory.
#[derive(Debug)]
pub struct Node {
    /// Original node spelling.
    pub name: Value,
    /// Ordered value vectors; an absent predicate differs from an empty vector.
    pub values: BTreeMap<String, Vec<Value>>,
}

/// Observed strings, not a lossless RDF-term serialization.
#[derive(Debug, PartialEq, Eq)]
pub enum Value {
    /// Original IRI text.
    Iri(String),
    /// Literal lexical form.
    Literal(String),
    /// Original presentation of any other term kind.
    Other(String),
}
