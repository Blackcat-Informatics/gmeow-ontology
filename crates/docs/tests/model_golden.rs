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

use gmeow_docs::{
    DocConcern, DocExample, DocExternalTerm, DocLearningPath, DocLinkage, DocMappingSet, DocRecipe,
    DocSlice, DocTerm, DocTermCategory, DocsModel,
};
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

    // ── New (T2) collections: counts + one sample each, to lock the shapes ──
    /// Number of mapping sets.
    mapping_set_count: usize,
    /// Number of linkages (term equivalences).
    linkage_count: usize,
    /// Number of worked examples.
    example_count: usize,
    /// Number of documentation concerns.
    concern_count: usize,
    /// Number of external (non-GMEOW) terms referenced.
    external_term_count: usize,
    /// ONE fully serialized mapping set (first by IRI).
    sample_mapping_set: Option<DocMappingSet>,
    /// ONE fully serialized linkage (first by sort).
    sample_linkage: Option<DocLinkage>,
    /// ONE example (first by sort) with its `text` truncated to a small prefix
    /// so the golden stays KB-sized while still locking the field shape.
    sample_example: Option<DocExample>,
    /// ONE fully serialized concern (first by IRI).
    sample_concern: Option<DocConcern>,
    /// ONE fully serialized external term (first by IRI).
    sample_external_term: Option<DocExternalTerm>,

    // ── New (T3b) guides collections: counts + one sample each ──────────────
    /// Number of adoption recipes.
    recipe_count: usize,
    /// Number of curated learning paths.
    learning_path_count: usize,
    /// Whether the curated four-boxes prose was discovered.
    has_four_boxes: bool,
    /// ONE fully serialized recipe (first by slug).
    sample_recipe: Option<DocRecipe>,
    /// ONE fully serialized learning path (first by slug).
    sample_learning_path: Option<DocLearningPath>,
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

        // One example, with its full Turtle text truncated to a small prefix so
        // the golden stays KB-sized (the field shape is still locked).
        let sample_example = model.examples.first().cloned().map(|mut e| {
            const CAP: usize = 200;
            if e.text.len() > CAP {
                // Truncate on a char boundary.
                let mut end = CAP;
                while !e.text.is_char_boundary(end) {
                    end -= 1;
                }
                e.text.truncate(end);
                e.text.push_str("…[truncated]");
            }
            e
        });

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
            mapping_set_count: model.mapping_sets.len(),
            linkage_count: model.linkages.len(),
            example_count: model.examples.len(),
            concern_count: model.concerns.len(),
            external_term_count: model.external_terms.len(),
            sample_mapping_set: model.mapping_sets.first().cloned(),
            sample_linkage: model.linkages.first().cloned(),
            sample_example,
            sample_concern: model.concerns.first().cloned(),
            sample_external_term: model.external_terms.first().cloned(),
            recipe_count: model.recipes.len(),
            learning_path_count: model.learning_paths.len(),
            has_four_boxes: model.four_boxes.is_some(),
            sample_recipe: model.recipes.first().cloned(),
            sample_learning_path: model.learning_paths.first().cloned(),
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
