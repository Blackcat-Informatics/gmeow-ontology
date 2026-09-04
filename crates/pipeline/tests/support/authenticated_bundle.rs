// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only access to the exact corpus products selected before the test runner starts.

#![allow(dead_code, unused_imports)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use purrdf::{DatasetView, RdfDataset, TermRef};

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace root")
}

/// Exact producer-selected bytes for tests that grade container or wire semantics.
pub fn source_bytes() -> &'static [u8] {
    static BYTES: OnceLock<Vec<u8>> = OnceLock::new();
    BYTES.get_or_init(|| {
        gmeow_bundle_import::load_authenticated_source_bytes(&repo_root())
            .expect("authenticated bundle bytes; tests never produce them")
    })
}

/// Graph-preserving corpus restored from the producer-created immutable pack.
pub fn dataset() -> &'static Arc<RdfDataset> {
    static DATASET: OnceLock<Arc<RdfDataset>> = OnceLock::new();
    DATASET.get_or_init(|| {
        gmeow_bundle_import::load_authenticated_repository_bundle(&repo_root())
            .expect("authenticated bundle dataset; tests never produce it")
            .dataset
    })
}

fn term_value(term: TermRef<'_>) -> String {
    match term {
        TermRef::Iri(iri) => iri.to_owned(),
        TermRef::Literal { lexical, .. } => lexical.to_owned(),
        TermRef::Blank { label, .. } => format!("_:{label}"),
        triple @ TermRef::Triple { .. } => format!("{triple:?}"),
    }
}

/// Ground triples of one named graph, preserving the historical lexical comparison surface.
pub fn graph_triples(graph_iri: &str) -> Vec<(String, String, String)> {
    graph_triples_from(dataset(), graph_iri)
}

/// Ground triples of one named graph from another authenticated stage product.
pub fn graph_triples_from(dataset: &RdfDataset, graph_iri: &str) -> Vec<(String, String, String)> {
    let scoped = dataset.project_named_graph(graph_iri);
    scoped
        .quad_refs()
        .map(|quad| (term_value(quad.s), term_value(quad.p), term_value(quad.o)))
        .collect()
}
