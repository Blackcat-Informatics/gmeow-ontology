// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Documentation-coverage status — the SINGLE source of the per-term coverage
//! predicates.
//!
//! Both the lint gate ([`crate::lint`], which emits a `docs/missing-*` warning per
//! absent dimension) and the rendered docs site ([`crate::render`], which surfaces
//! coverage on each term page and the documentation-health page) read coverage
//! from here. Keeping the predicates in one place means the gate count and the
//! published page can never silently disagree about what a term is missing.

use std::collections::HashSet;

use crate::model::{DocTerm, DocsModel};

/// The set of term IRIs that are the subject of at least one external alignment
/// (term equivalence), built once from a model's linkages.
///
/// Membership is the only operation needed, so a [`HashSet`] gives O(1) per-term
/// checks rather than re-scanning the linkages for every term; iteration order is
/// never observed, so determinism is unaffected.
pub fn alignment_subjects(model: &DocsModel) -> HashSet<&str> {
    model.linkages.iter().map(|l| l.subject.as_str()).collect()
}

/// Which of the six documentation-coverage dimensions a single term carries.
///
/// A dimension is "present" exactly when the corresponding `docs/missing-*` lint
/// would NOT fire — the booleans are the negation of the lint predicates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TermCoverage {
    /// `skos:definition`/`rdfs:comment` is non-empty.
    pub has_definition: bool,
    /// `rdfs:label` is non-empty.
    pub has_label: bool,
    /// At least one of `gmeow:useWhen`/`avoidWhen`/`howToUse` is present.
    pub has_usage_advice: bool,
    /// At least one `skos:example`.
    pub has_example: bool,
    /// At least one `skos:scopeNote`.
    pub has_scope_note: bool,
    /// The term IRI is the subject of at least one external alignment.
    pub has_alignment: bool,
}

impl TermCoverage {
    /// The number of coverage dimensions.
    pub const TOTAL: usize = 6;

    /// Each dimension in stable display order, paired with the `UI_TEMPLATES` key
    /// for its human label and whether the term carries it. The renderer maps the
    /// key through [`crate::ui_string`] so the dimension labels are localized from
    /// the single UI-string source.
    pub fn dimensions(&self) -> [(&'static str, bool); Self::TOTAL] {
        [
            ("coverage_dim_definition", self.has_definition),
            ("coverage_dim_label", self.has_label),
            ("coverage_dim_usage_advice", self.has_usage_advice),
            ("coverage_dim_example", self.has_example),
            ("coverage_dim_scope_note", self.has_scope_note),
            ("coverage_dim_alignment", self.has_alignment),
        ]
    }

    /// How many of the [`TOTAL`](Self::TOTAL) dimensions the term carries.
    pub fn present_count(&self) -> usize {
        self.dimensions()
            .iter()
            .filter(|(_, present)| *present)
            .count()
    }

    /// The UI-label keys of the dimensions the term is MISSING, in display order.
    pub fn missing_keys(&self) -> Vec<&'static str> {
        self.dimensions()
            .into_iter()
            .filter(|(_, present)| !present)
            .map(|(key, _)| key)
            .collect()
    }
}

/// Compute a term's coverage against a precomputed [`alignment_subjects`] set.
pub fn term_coverage(term: &DocTerm, aligned: &HashSet<&str>) -> TermCoverage {
    TermCoverage {
        has_definition: !term.definition.as_deref().unwrap_or("").trim().is_empty(),
        has_label: !term.label.as_deref().unwrap_or("").trim().is_empty(),
        has_usage_advice: !(term.use_when.is_empty()
            && term.avoid_when.is_empty()
            && term.how_to_use.is_empty()),
        has_example: !term.examples.is_empty(),
        has_scope_note: !term.scope_notes.is_empty(),
        has_alignment: aligned.contains(term.iri.as_str()),
    }
}
