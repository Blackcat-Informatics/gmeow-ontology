// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Golden test of the typed documentation model built from the live slice
//! catalog.
//!
//! The snapshot is a deliberately *compact structure-locking summary* of the
//! [`DocsModel`], not a full dump of all ~2k terms. Its job is to lock the
//! model's SHAPE deterministically — counts, the slice spine, and a few fully
//! serialized representatives so the `DocSlice` / `DocArtifact` / `DocTerm`
//! field shapes are pinned. Per-term *content* belongs to the slices that own
//! those terms, not to this crate's test surface, so we do not snapshot every
//! term (that produced a ~1.7 MB churn-magnet snapshot).

use std::collections::BTreeMap;
use std::path::PathBuf;

use gmeow_docs::{DocSlice, DocTerm, DocTermCategory, DocsModel};
use serde::Serialize;

/// The repo root is two levels above this crate's manifest dir
/// (`<repo>/crates/docs`).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("crate is at <repo>/crates/docs")
        .to_path_buf()
}

/// A compact, structure-locking summary of a [`DocsModel`]. Captures the
/// model's shape (counts, slice spine, category histogram) plus a handful of
/// fully serialized representatives so every nested field shape is pinned.
#[derive(Serialize)]
struct ModelSummary {
    /// `DocsModel::title`.
    title: String,
    /// `DocsModel::version`.
    version: String,
    /// Number of slices.
    slice_count: usize,
    /// Number of documented terms.
    term_count: usize,
    /// Number of cross-slice dependency edges.
    dependency_edge_count: usize,
    /// Term counts keyed by category name (`Class`/`Property`/…), sorted.
    term_count_by_category: BTreeMap<String, usize>,
    /// The sorted list of slice IRIs — small and meaningfully locks the spine.
    slice_iris: Vec<String>,
    /// ONE fully serialized slice (the first by IRI), locking the
    /// `DocSlice`/`DocArtifact` shape.
    sample_slice: Option<DocSlice>,
    /// Three fully serialized terms (first Class, first Property, first
    /// Individual by IRI), locking the `DocTerm` shape.
    sample_terms: Vec<DocTerm>,
}

impl ModelSummary {
    fn from_model(model: &DocsModel) -> Self {
        let mut term_count_by_category: BTreeMap<String, usize> = BTreeMap::new();
        for term in &model.terms {
            *term_count_by_category
                .entry(category_name(term.category).to_string())
                .or_insert(0) += 1;
        }

        // Slices are already sorted by IRI in the model; take the first.
        let sample_slice = model.slices.first().cloned();

        // Terms are already sorted by IRI in the model; pick the first by IRI
        // within each representative category.
        let first_in = |cat: DocTermCategory| -> Option<DocTerm> {
            model.terms.iter().find(|t| t.category == cat).cloned()
        };
        let sample_terms: Vec<DocTerm> = [
            first_in(DocTermCategory::Class),
            first_in(DocTermCategory::Property),
            first_in(DocTermCategory::Individual),
        ]
        .into_iter()
        .flatten()
        .collect();

        Self {
            title: model.title.clone(),
            version: model.version.clone(),
            slice_count: model.slices.len(),
            term_count: model.terms.len(),
            dependency_edge_count: model.dependency_edges.len(),
            term_count_by_category,
            slice_iris: model.slices.iter().map(|s| s.iri.clone()).collect(),
            sample_slice,
            sample_terms,
        }
    }
}

/// Stable category name used as a histogram key (matches the serde variant).
fn category_name(category: DocTermCategory) -> &'static str {
    match category {
        DocTermCategory::Class => "Class",
        DocTermCategory::Property => "Property",
        DocTermCategory::Individual => "Individual",
        DocTermCategory::Datatype => "Datatype",
        DocTermCategory::Other => "Other",
    }
}

#[test]
fn docs_model_golden() {
    let model = DocsModel::discover(&repo_root()).expect("build docs model from live slices");
    let summary = ModelSummary::from_model(&model);
    insta::assert_json_snapshot!(summary);
}
