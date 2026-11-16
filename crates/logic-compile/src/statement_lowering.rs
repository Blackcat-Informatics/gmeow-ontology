// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The RDF 1.2 statement-metadata **lowering**: an attribution's `rdf:reifies` triple term,
//! decomposed into three ordinary joinable edges.
//!
//! # What was invisible, and why it mattered
//!
//! RDF 1.2 attributes a claim to a vantage by REIFYING a statement: a reifier node carries
//! `rdf:reifies <<( s p o )>>` alongside ordinary annotations — who says it
//! (`gmeow:accordingTo`), with what polarity, with what support. The annotations are plain
//! triples and always reached the chase and the SHACL engine. The one thing neither could
//! see was the OBJECT of `rdf:reifies`, because that object is a triple TERM: the reasoner's
//! fact surface carries IRIs, blank nodes and literals, and a projected SHACL-SPARQL
//! constraint joins on ordinary triple patterns.
//!
//! That single missing edge was not a nuance. Two co-equal vantages taking opposite stances
//! on ONE statement is the canonical situation the standpoint layer exists to govern, and to
//! the reasoner it read as two unrelated nodes with unrelated opinions: nothing joined them,
//! because the only thing saying they were about the same claim was the edge it could not
//! read. A rule about contested claims could not even be WRITTEN — its two halves shared no
//! variable. The same holds for every `logic:Constraint` whose formula joins an attribution
//! to the claim it is about.
//!
//! # Where it applies
//!
//! This module is the ONE lowering, and it has exactly two consumers, both of which derive
//! a view from the authored dataset and never write that view back:
//!
//! * the reasoning world the operator CLI builds (`gmeow_logic::operator`), which pushes
//!   [`lower_reifiers`]'s rows into its single world; and
//! * the SHACL validations of authored and user data, through [`validation_view`]: the consumer
//!   `gmeow validate` path and its LSP and MCP twins, the dev authoring gate, the pipeline
//!   `stage-validate` and fixture validation, the slice conformance cells, and the
//!   slice-brief exemplar gate. A `logic:Constraint` over the lowered components is
//!   enforced only through its derived SHACL, so a validation view without these edges
//!   reports a correctly attributed statement as unattributed and never matches a
//!   forbidden pattern over two attributions.
//!
//! It lives in the compiler crate, below both the reasoning stack and the wasm-clean
//! validator, so neither carries a second copy.
//!
//! # What this is, in Principle 17 terms
//!
//! A **generated lowering**, exactly as SSSOM, EDOAL and FnO are. Canonical RDF 1.2 remains
//! the authority: the authored dataset is never mutated, the reifier side table is never
//! rewritten, and the lowering is derived from it on the way into the reasoning world or the
//! validation view. It touches neither `term_codec` (no triple term is ever encoded — it is
//! decomposed, so the codec never sees one), nor the EDB fact stream's shape (the emitted
//! rows are ordinary `(subject, predicate, object)` triples), nor join keys, nor provenance
//! minting (each lowered row is asserted and echoes like any other asserted fact). The
//! lowered edges never reach a bundle, a fanned-out artifact or the user's data, and no
//! projected shape uses one as a result path: a finding's focus node and value are the
//! authored nodes the constraint is about.
//!
//! # What it does not preserve
//!
//! Recorded narrowly as `logic:rdf12-nested-triple-term`, and mirrored in
//! `gmeow_logic::reason::refute::retained_boundaries`:
//!
//! * **A nested triple term.** A statement whose own subject or object is itself a triple
//!   term has no non-term component to decompose into. [`lower_reifiers`] emits NOTHING for
//!   it and returns it in [`StatementLowering::nested`], so the residue is named rather than
//!   flattened into a malformed IRI. [`validation_view`] carries the same residue in
//!   [`ValidationView::nested`]; a validation report names it as a warning finding, and a
//!   slice conformance cell refuses to claim conformance over it, because a constraint over
//!   the lowered components cannot match that statement and its silence is not conformance.
//! * **The statement's identity AS A TERM.** The lowering yields three edges ABOUT THE
//!   REIFIER; nothing in the fact surface denotes the statement. A rule or constraint may
//!   therefore join on the components and may not quantify over the statement itself.

use std::sync::Arc;

use purrdf::{DatasetView, GraphMatch, RdfDataset, RdfQuad, RdfTerm, RdfTriple};

use crate::graphutil::RDF_REIFIES;

/// `logic:reifiedStatementSubject` — the lowered subject component.
pub const REIFIED_STATEMENT_SUBJECT: &str =
    "https://blackcatinformatics.ca/logic/reifiedStatementSubject";
/// `logic:reifiedStatementPredicate` — the lowered predicate component, in OBJECT position
/// (the fact surface has no predicate-variable slot, which is what makes it joinable).
pub const REIFIED_STATEMENT_PREDICATE: &str =
    "https://blackcatinformatics.ca/logic/reifiedStatementPredicate";
/// `logic:reifiedStatementObject` — the lowered object component.
pub const REIFIED_STATEMENT_OBJECT: &str =
    "https://blackcatinformatics.ca/logic/reifiedStatementObject";

/// The three component edges of one lowered attribution, plus the residue.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatementLowering {
    /// The lowered rows — three per reifier, in reifier order and then
    /// subject/predicate/object order — each in the graph that holds its reifier, so a
    /// world-scoped attribution lowers inside its own world.
    pub rows: Vec<RdfQuad>,
    /// The reifiers NOT lowered because the statement they reify nests a triple term. Named
    /// rather than dropped: this is the exact residue `logic:rdf12-nested-triple-term`
    /// records, and a caller that reports "everything was reasoned over" while this is
    /// non-empty is making the blanket claim the boundary exists to replace.
    pub nested: Vec<RdfTerm>,
}

impl StatementLowering {
    /// Lower one `reifier rdf:reifies statement` declaration held in `graph`.
    fn lower(&mut self, reifier: RdfTerm, statement: RdfTriple, graph: Option<RdfTerm>) {
        if is_triple_term(&statement.subject) || is_triple_term(&statement.object) {
            // A nested triple term has no non-term component to decompose into. Emitting a
            // partial lowering here would be worse than emitting none: a rule joining on
            // subject and predicate alone would treat two different nested claims as one.
            self.nested.push(reifier);
            return;
        }
        let (subject, predicate, object, _) = statement.into_parts();
        for (component, value) in [
            (REIFIED_STATEMENT_SUBJECT, subject),
            (REIFIED_STATEMENT_PREDICATE, RdfTerm::Iri(predicate)),
            (REIFIED_STATEMENT_OBJECT, object),
        ] {
            let mut row = RdfQuad::new(reifier.clone(), component, value);
            row.graph_name.clone_from(&graph);
            self.rows.push(row);
        }
    }
}

/// True when `term` is an RDF 1.2 triple term.
fn is_triple_term(term: &RdfTerm) -> bool {
    matches!(term, RdfTerm::Triple(_))
}

/// Lower every reification of `dataset` into its three joinable component edges.
///
/// Reads both physical forms of an RDF 1.2 reification, because they are the same graph:
/// the reifier SIDE TABLE (`owned_reifiers`), where a parsed or folded dataset keeps the
/// statement layer, and an `rdf:reifies` base quad whose object is a triple term, which is
/// how a flat, un-folded dataset (`purrdf::flat_dataset_from_quads`) carries it. A dataset
/// holds one form or the other for a given declaration. The authored dataset is not
/// touched.
#[must_use]
pub fn lower_reifiers(dataset: &RdfDataset) -> StatementLowering {
    let mut out = StatementLowering::default();
    for reifier in dataset.owned_reifiers() {
        out.lower(reifier.reifier, reifier.statement, reifier.graph);
    }
    if let Some(reifies) = dataset.term_id_by_iri(RDF_REIFIES) {
        for quad in dataset.quads_for_pattern(None, Some(reifies), None, GraphMatch::Any) {
            let RdfTerm::Triple(statement) = dataset.to_owned_term(quad.o) else {
                continue;
            };
            out.lower(
                dataset.to_owned_term(quad.s),
                *statement,
                quad.g.map(|graph| dataset.to_owned_term(graph)),
            );
        }
    }
    out
}

/// The data view a SHACL validation runs on: the authored dataset plus its lowered
/// statement components, and the residue that could not be lowered.
#[derive(Debug)]
#[must_use]
pub struct ValidationView<'a> {
    /// The authored dataset the view was derived from.
    authored: &'a RdfDataset,
    /// The authored rows plus the lowered ones; `None` when there was nothing to lower,
    /// so the authored dataset is validated as it is.
    lowered: Option<Arc<RdfDataset>>,
    /// The reifiers whose statement nests a triple term, and which therefore carry no
    /// component edge in [`Self::dataset`] (`logic:rdf12-nested-triple-term`).
    pub nested: Vec<RdfTerm>,
}

impl ValidationView<'_> {
    /// The dataset to hand to the SHACL engine.
    #[must_use]
    pub fn dataset(&self) -> &RdfDataset {
        self.lowered.as_deref().unwrap_or(self.authored)
    }
}

/// Build the SHACL validation view of `dataset`: every base quad, reifier and annotation
/// in its own graph, plus the rows of [`lower_reifiers`].
///
/// This is the one place a validation view gains the lowered edges. A dataset with no
/// flat reification is used as it is. The view is a validation input only: it is never
/// serialized, folded into a bundle or returned to the caller as their data.
///
/// # Errors
///
/// Returns the builder's diagnostic if the view fails to freeze.
pub fn validation_view(dataset: &RdfDataset) -> gmeow_errors::Result<ValidationView<'_>> {
    let lowering = lower_reifiers(dataset);
    if lowering.rows.is_empty() {
        return Ok(ValidationView {
            authored: dataset,
            lowered: None,
            nested: lowering.nested,
        });
    }
    // Rebuild from owned rows so every term — blank reifiers included — is interned once,
    // in one scope, and the lowered rows join the very nodes the authored rows name.
    let mut builder = purrdf::RdfDatasetBuilder::new();
    for quad in dataset.owned_quads() {
        builder.push_owned_quad(&quad)?;
    }
    for reifier in dataset.owned_reifiers() {
        builder.push_owned_reifier(&reifier)?;
    }
    for annotation in dataset.owned_annotations() {
        builder.push_owned_annotation(&annotation)?;
    }
    for row in &lowering.rows {
        builder.push_owned_quad(row)?;
    }
    Ok(ValidationView {
        authored: dataset,
        lowered: Some(builder.freeze().map_err(gmeow_errors::Diag::from)?),
        nested: lowering.nested,
    })
}

#[path = "statement_lowering.tests.rs"]
#[cfg(test)]
mod tests;
