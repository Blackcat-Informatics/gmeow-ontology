// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! One canonicalization boundary retaining the actual source-term correspondence.

use std::collections::{BTreeSet, HashMap};

use purrdf::{CanonicalRelabeling, RdfDataset, TermId, TermRef, canonical_relabel_with_mapping};

use super::{Diagnostic, LogicParseError, LogicProgram};

/// An admitted native source selection, canonicalized once before extraction.
///
/// Its source mapping belongs to the exact input dataset supplied to [`Self::new`].
/// A document and an aggregate containing that document are different selections:
/// callers must map through the aggregate's own native composition before using
/// its source-term mapping. Neither local term IDs nor this relabeling operation
/// constitutes a durable source identity or a rewrite certificate.
#[derive(Debug)]
pub struct PreparedLogicSource {
    canonical: CanonicalRelabeling,
    source_graph: super::StructuralSourceGraph,
    origins: Vec<super::SourceDocumentOccurrences>,
}

impl PreparedLogicSource {
    /// Preserve source anchors while performing the frontend's strict native
    /// canonicalization. No OWL projection or literal normalization is added.
    ///
    /// # Errors
    /// Refuses empty input and every canonicalization failure.
    pub fn new(dataset: &RdfDataset) -> Result<Self, LogicParseError> {
        super::require_nonempty_source(dataset)?;
        let canonical = canonical_relabel_with_mapping(dataset).map_err(|error| {
            LogicParseError(format!("native blank-node canonicalization: {error}"))
        })?;
        let source_graph = super::StructuralSourceGraph::new(canonical.dataset());
        Ok(Self {
            canonical,
            source_graph,
            origins: Vec::new(),
        })
    }

    /// Borrow the single canonical dataset used by the compiler and shared analyses.
    #[must_use]
    pub fn dataset(&self) -> &RdfDataset {
        self.canonical.dataset()
    }

    /// Structural units and uses captured before lowering, not an execution verdict.
    #[must_use]
    pub fn source_graph(&self) -> &super::StructuralSourceGraph {
        &self.source_graph
    }

    /// Original document occurrences for this selection. An ordinary anonymous
    /// source has no file receipt; absence never claims complete source attribution.
    #[must_use]
    pub fn origins(&self) -> &[super::SourceDocumentOccurrences] {
        &self.origins
    }

    /// Attach one original document through the actual native composition and
    /// this selection's canonical mapping, never label matching.
    ///
    /// `selection_term` maps a document-local term into the exact aggregate
    /// dataset passed to [`Self::new`]. This method performs the remaining
    /// canonical mapping once and inventories every structural occurrence and
    /// assertion row without serialization or a second parse.
    ///
    /// # Errors
    /// Refuses incomplete/duplicate document identities, out-of-range source
    /// anchors, and bindings outside this selection's structural source graph.
    /// These bindings inventory provenance; they are not execution certificates.
    pub fn record_document(
        &mut self,
        document: super::SourceDocument,
        original: &RdfDataset,
        mut selection_term: impl FnMut(TermId) -> Result<TermId, LogicParseError>,
    ) -> Result<(), LogicParseError> {
        if document.path.is_empty()
            || document.content_digest.is_empty()
            || document.role.is_empty()
            || self
                .origins
                .iter()
                .any(|entry| entry.document.path == document.path)
        {
            return Err(LogicParseError(
                "incomplete or duplicate source document receipt".into(),
            ));
        }

        // Fixed-size native IDs make this a bounded per-document lookup cache.
        // On saturation, additional terms resolve directly with identical semantics.
        const MAX_REMAP_ENTRIES: usize = 65_536;
        let mut remap = HashMap::new();
        let mut bindings = BTreeSet::new();
        let mut statements = Vec::new();
        let mut map_term = |term: TermId| -> Result<TermId, LogicParseError> {
            if let Some(mapped) = remap.get(&term) {
                return Ok(*mapped);
            }
            let selected = selection_term(term)?;
            let mapped = self.source_term(selected).ok_or_else(|| {
                LogicParseError(format!(
                    "source term in {:?} has no canonical binding",
                    document.path
                ))
            })?;
            if remap.len() < MAX_REMAP_ENTRIES {
                remap.insert(term, mapped);
            }
            Ok(mapped)
        };
        let nodes = original
            .quads()
            .flat_map(|quad| {
                [
                    (quad.s, quad.g, super::SourceOccurrencePosition::Subject),
                    (quad.o, quad.g, super::SourceOccurrencePosition::Object),
                ]
            })
            .chain(
                original
                    .annotations_with_graph()
                    .flat_map(|(subject, _, object, graph)| {
                        [
                            (
                                subject,
                                graph,
                                super::SourceOccurrencePosition::AnnotationSubject,
                            ),
                            (
                                object,
                                graph,
                                super::SourceOccurrencePosition::AnnotationObject,
                            ),
                        ]
                    }),
            )
            .chain(
                original
                    .reifiers_with_graph()
                    .flat_map(|(reifier, triple, graph)| {
                        [
                            (reifier, graph, super::SourceOccurrencePosition::Reifier),
                            (
                                triple,
                                graph,
                                super::SourceOccurrencePosition::QuotedStatement,
                            ),
                        ]
                    }),
            );
        for (term, original_graph, position) in nodes {
            if matches!(original.resolve(term), TermRef::Literal { .. }) {
                continue;
            }
            let graph = original_graph.map(&mut map_term).transpose()?;
            let canonical = super::SourceNode {
                term: map_term(term)?,
                graph,
            };
            if self.source_graph.unit(canonical).is_some() {
                bindings.insert(super::SourceNodeBinding {
                    original: super::SourceNode {
                        term,
                        graph: original_graph,
                    },
                    canonical,
                    position,
                });
            }
        }
        for (quad, carrier) in original
            .quads()
            .map(|quad| (quad, super::SourceCarrier::Statement))
            .chain(original.annotations_with_graph().map(|(s, p, o, g)| {
                (
                    purrdf::QuadIds { s, p, o, g },
                    super::SourceCarrier::Annotation,
                )
            }))
        {
            statements.push(super::SourceStatementBinding {
                canonical: purrdf::QuadIds {
                    s: map_term(quad.s)?,
                    p: map_term(quad.p)?,
                    o: map_term(quad.o)?,
                    g: quad.g.map(&mut map_term).transpose()?,
                },
                carrier,
            });
        }
        drop(map_term);

        for binding in &bindings {
            if binding.original.term.index() >= original.term_count()
                || binding
                    .original
                    .graph
                    .is_some_and(|graph| graph.index() >= original.term_count())
                || self.source_graph.unit(binding.canonical).is_none()
            {
                return Err(LogicParseError(format!(
                    "invalid structural source binding for {}",
                    document.path
                )));
            }
        }
        for binding in &statements {
            let valid_canonical = [
                binding.canonical.s,
                binding.canonical.p,
                binding.canonical.o,
            ]
            .into_iter()
            .all(|term| term.index() < self.dataset().term_count())
                && binding
                    .canonical
                    .g
                    .is_none_or(|graph| graph.index() < self.dataset().term_count());
            if !valid_canonical {
                return Err(LogicParseError(format!(
                    "invalid statement source binding for {}",
                    document.path
                )));
            }
        }
        statements.sort_unstable();
        statements.dedup();
        self.origins.push(super::SourceDocumentOccurrences {
            document,
            bindings,
            statements,
        });
        Ok(())
    }

    /// Extract procedural constraints using this prepared ownership graph.
    pub fn constraints(&self) -> (Vec<super::ConstraintIr>, Vec<Diagnostic>) {
        super::extract_constraints_from_source(self.dataset(), &self.source_graph)
    }

    /// Map a term from this selection's exact original dataset into [`Self::dataset`].
    ///
    /// Unused dictionary entries have no output term. A same-index handle from a
    /// different selection is not meaningful here; callers retain selection identity
    /// alongside every source reference and never persist these runtime IDs.
    #[must_use]
    pub fn source_term(&self, original: TermId) -> Option<TermId> {
        self.canonical.map_term(original)
    }

    /// Run the standard extractors over the prepared dataset, without another
    /// parser or canonicalization pass. `source_iri` remains provenance only.
    ///
    /// Callers can record original-to-canonical source anchors before this step
    /// performs restriction lowering, formula routing and axiom deduplication.
    ///
    /// # Errors
    /// Retains every standard frontend admission and lowering error.
    pub fn compile(
        &self,
        source_iri: Option<String>,
    ) -> Result<(LogicProgram, Vec<Diagnostic>), LogicParseError> {
        let compiled = self.compile_with_sources(source_iri)?;
        Ok((compiled.program, compiled.diagnostics))
    }

    /// Retain native source anchors through formula routing, class-expression
    /// expansion, axiom deduplication and final canonical ordering. This evidence
    /// belongs to this prepared source, not a reconstructed output graph.
    ///
    /// # Errors
    /// Retains every standard frontend admission and lowering error.
    pub fn compile_with_sources(
        &self,
        source_iri: Option<String>,
    ) -> Result<super::SourceCompilation<'_>, LogicParseError> {
        let (program, diagnostics, trace) =
            super::compile_source_graph(self.dataset(), &self.source_graph, source_iri)?;
        Ok(super::SourceCompilation {
            source: self,
            program,
            diagnostics,
            trace,
        })
    }

    /// Compile once and move the immutable source and its extraction evidence
    /// into the same owned value. No program clone, reparse or second relabeling.
    ///
    /// # Errors
    /// Retains all frontend compilation errors; successful extraction may still
    /// contain explicit diagnostics and excluded source units.
    pub fn into_compiled(
        self,
        source_iri: Option<String>,
    ) -> Result<super::CompiledTheory, LogicParseError> {
        let super::SourceCompilation {
            program,
            diagnostics,
            trace,
            ..
        } = self.compile_with_sources(source_iri)?;
        Ok(super::CompiledTheory {
            source: self,
            program,
            diagnostics,
            trace,
        })
    }
}

#[path = "prepared.tests.rs"]
#[cfg(test)]
mod tests;
