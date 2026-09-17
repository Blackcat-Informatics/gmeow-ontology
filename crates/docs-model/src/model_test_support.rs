// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only constructors for inert documentation models.

use super::*;

impl DocSlice {
    /// A bare slice carrying only an IRI and a document set — for the
    /// [`crate::source_map`] unit tests that exercise the page map over a
    /// hand-built model without the full catalog machinery.
    #[cfg(test)]
    pub(crate) fn bare_for_test(iri: &str, documents: Vec<DocMarkdownDocument>) -> Self {
        Self {
            iri: iri.to_string(),
            label: None,
            title: None,
            tier: None,
            identifier: None,
            creators: Vec::new(),
            consumers: Vec::new(),
            profiles: Vec::new(),
            depends_on: Vec::new(),
            artifacts: Vec::new(),
            has_thesis_sentence: false,
            realized_state_complete: false,
            documents,
        }
    }
}

impl DocsModel {
    /// An empty model with every collection cleared — for the [`crate::source_map`]
    /// unit tests, which populate only `slices` before exercising the page map.
    #[cfg(test)]
    pub(crate) fn empty_for_test() -> Self {
        Self {
            title: "GMEOW Ontology Documentation".to_string(),
            version: Self::VERSION.to_string(),
            slices: Vec::new(),
            terms: Vec::new(),
            dependency_edges: Vec::new(),
            mapping_sets: Vec::new(),
            linkages: Vec::new(),
            examples: Vec::new(),
            fixtures: Vec::new(),
            shapes: Vec::new(),
            seams: Vec::new(),
            competencies: Vec::new(),
            grammars: Vec::new(),
            loss_targets: Vec::new(),
            worked_instances: Vec::new(),
            concerns: Vec::new(),
            external_terms: Vec::new(),
            recipes: Vec::new(),
            learning_paths: Vec::new(),
            advice_entries: Vec::new(),
            constraint_rules: Vec::new(),
            four_boxes: None,
            concept_doi: None,
            pipeline: None,
            available_languages: vec!["english".to_string()],
            translations: Translations::default(),
            ui_catalog: UiCatalog::default(),
            reasoning: None,
            diagnostics: None,
            term_loss: None,
            schema_fragments: None,
            lang: String::new(),
        }
    }
}
