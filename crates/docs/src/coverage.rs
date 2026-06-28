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

/// One documentation-coverage dimension: a stable machine key (for the search
/// index) plus a human display label (for the rendered docs body, literal English
/// like the rest of the term-page section headings).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoverageDimension {
    /// Stable machine key, never localized: e.g. `"usage_advice"`.
    pub key: &'static str,
    /// Human display label for the rendered page: e.g. `"Usage advice"`.
    pub label: &'static str,
}

/// The coverage dimensions in stable display order. Each mirrors a `docs/missing-*`
/// lint code, and the order matches [`TermCoverage::flags`].
pub const DIMENSIONS: [CoverageDimension; TermCoverage::TOTAL] = [
    CoverageDimension {
        key: "definition",
        label: "Definition",
    },
    CoverageDimension {
        key: "label",
        label: "Label",
    },
    CoverageDimension {
        key: "usage_advice",
        label: "Usage advice",
    },
    CoverageDimension {
        key: "example",
        label: "Example",
    },
    CoverageDimension {
        key: "scope_note",
        label: "Scope note",
    },
    CoverageDimension {
        key: "alignment",
        label: "Alignment",
    },
];

impl TermCoverage {
    /// The number of coverage dimensions.
    pub const TOTAL: usize = 6;

    /// The presence flag for each dimension, in [`DIMENSIONS`] order.
    pub fn flags(&self) -> [bool; Self::TOTAL] {
        [
            self.has_definition,
            self.has_label,
            self.has_usage_advice,
            self.has_example,
            self.has_scope_note,
            self.has_alignment,
        ]
    }

    /// How many of the [`TOTAL`](Self::TOTAL) dimensions the term carries.
    pub fn present_count(&self) -> usize {
        self.flags().iter().filter(|present| **present).count()
    }

    /// The machine keys of the dimensions the term is MISSING, in display order —
    /// the search-index facet for filtering under-documented terms.
    pub fn missing_keys(&self) -> Vec<&'static str> {
        DIMENSIONS
            .iter()
            .zip(self.flags())
            .filter(|(_, present)| !*present)
            .map(|(dim, _)| dim.key)
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
