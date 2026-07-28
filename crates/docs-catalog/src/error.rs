// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Docs-catalog diagnostic kinds.
//!
//! Every catalog defect is a HARD failure (no-optionality): a snapshot that will not
//! fold, an absent distribution-catalog named graph, a catalog subject missing a
//! required facet, a concept node missing its extent or intent. Each is a
//! [`DiagKind`](gmeow_errors::DiagKind) minted by
//! [`define_diag_kind!`](gmeow_errors::define_diag_kind) under the `docs-catalog.*` code
//! namespace, so this reader reports on the same content-bound substrate as every other
//! crate rather than raising bare strings.
//!
//! This crate is a LEAF and has no stages, so there is deliberately no `StageFailed`-shaped
//! kind here — a pipeline stage driving a reader lifts the raised `Diag` unchanged. There
//! is NO central diagnostic-code aggregator in this workspace: [`DOCS_CATALOG_DIAG_CODES`]
//! and [`register_all`] are this crate's single, complete catalog, and the
//! self-consistency test below is what keeps the two in bijection.

use gmeow_errors::{Code, FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

define_diag_kind! {
    /// A defect reading the meta-level distribution catalog out of a materialized
    /// `gmeow.gts`: the snapshot will not fold, the `graph/distribution-catalog` named
    /// graph is absent or empty, it carries no `gmeow:DocumentationDistribution` subject,
    /// or a subject is missing a required facet. The matrix is refused rather than
    /// silently partial.
    pub struct DistributionCatalog { message: String }
    code = "docs-catalog.distribution";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "distribution catalog error: {}", message;
}

define_diag_kind! {
    /// A defect reading the formal-concept lattice out of the same catalog graph: a
    /// snapshot that will not fold, an absent catalog graph, or a subject carrying
    /// `gmeow:conceptExtent` / `gmeow:conceptIntent` without the `gmeow:FormalConcept`
    /// type — a node the reader's type filter would otherwise drop in silence.
    ///
    /// An EMPTY lattice is NOT a defect, and neither is a concept with an empty extent or
    /// an empty intent: the emitter is a separate producer, and the lattice's own bounds
    /// are one-sided by construction (see [`crate::concept_lattice`]).
    pub struct ConceptLattice { message: String }
    code = "docs-catalog.concept-lattice";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "concept lattice error: {}", message;
}

/// The complete docs-catalog diagnostic-code catalog, in registration order.
pub const DOCS_CATALOG_DIAG_CODES: &[&str] = &[DistributionCatalog::CODE, ConceptLattice::CODE];

/// Eagerly intern every docs-catalog diagnostic code (idempotent).
pub fn register_all() -> Vec<Code> {
    vec![DistributionCatalog::register(), ConceptLattice::register()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::intern_code;
    use std::collections::HashSet;

    #[test]
    fn every_docs_catalog_code_interns_with_no_collision() {
        let handles = register_all();
        assert_eq!(
            handles.len(),
            DOCS_CATALOG_DIAG_CODES.len(),
            "register_all() and DOCS_CATALOG_DIAG_CODES must enumerate the same kinds"
        );
        for code in DOCS_CATALOG_DIAG_CODES {
            assert!(
                intern_code(code).is_ok(),
                "docs-catalog code `{code}` did not intern after register_all()"
            );
        }
        let distinct_strings: HashSet<&&str> = DOCS_CATALOG_DIAG_CODES.iter().collect();
        assert_eq!(
            distinct_strings.len(),
            DOCS_CATALOG_DIAG_CODES.len(),
            "duplicate docs-catalog diagnostic code string detected"
        );
        let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
        assert_eq!(
            distinct_handles.len(),
            handles.len(),
            "two docs-catalog diagnostic kinds interned to the same code handle"
        );
    }
}
