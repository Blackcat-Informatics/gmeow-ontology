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
//! triples and always reached the chase. The one thing it could not see was the OBJECT of
//! `rdf:reifies`, because that object is a triple TERM and the engine's fact surface carries
//! IRIs, blank nodes and literals.
//!
//! That single missing edge was not a nuance. Two co-equal vantages taking opposite stances
//! on ONE statement is the canonical situation the standpoint layer exists to govern, and to
//! the reasoner it read as two unrelated nodes with unrelated opinions: nothing joined them,
//! because the only thing saying they were about the same claim was the edge it could not
//! read. A rule about contested claims could not even be WRITTEN — its two halves shared no
//! variable.
//!
//! # What this is, in Principle 17 terms
//!
//! A **generated lowering**, exactly as SSSOM, EDOAL and FnO are. Canonical RDF 1.2 remains
//! the authority: the authored dataset is never mutated, the reifier side table is never
//! rewritten, and the lowering is derived from it on the way into the reasoning world. It
//! touches neither `term_codec` (no triple term is ever encoded — it is decomposed, so the
//! codec never sees one), nor the EDB fact stream's shape (the emitted rows are ordinary
//! `(subject, predicate, object)` triples), nor join keys, nor provenance minting (each
//! lowered row is asserted and echoes like any other asserted fact).
//!
//! # What it does not preserve
//!
//! Recorded narrowly as `logic:rdf12-nested-triple-term`, and mirrored in
//! `crate::reason::refute::retained_boundaries`:
//!
//! * **A nested triple term.** A statement whose own subject or object is itself a triple
//!   term has no non-term component to decompose into. [`lower_reifiers`] emits NOTHING for
//!   it and returns it in [`StatementLowering::nested`], so the residue is named rather than
//!   flattened into a malformed IRI.
//! * **The statement's identity AS A TERM.** The lowering yields three edges ABOUT THE
//!   REIFIER; nothing in the fact surface denotes the statement. A rule may therefore join
//!   on the components and may not quantify over the statement itself.

use purrdf::{RdfDataset, RdfTerm};

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
    /// The lowered rows, as `(subject, predicate, object)` — three per reifier, in reifier
    /// order and then subject/predicate/object order.
    pub rows: Vec<(RdfTerm, String, RdfTerm)>,
    /// The reifiers NOT lowered because the statement they reify nests a triple term. Named
    /// rather than dropped: this is the exact residue `logic:rdf12-nested-triple-term`
    /// records, and a caller that reports "everything was reasoned over" while this is
    /// non-empty is making the blanket claim the boundary exists to replace.
    pub nested: Vec<RdfTerm>,
}

/// True when `term` is (or is) an RDF 1.2 triple term.
fn is_triple_term(term: &RdfTerm) -> bool {
    matches!(term, RdfTerm::Triple(_))
}

/// Lower every reifier of `dataset` into its three joinable component edges.
///
/// Reads the reifier SIDE TABLE (`owned_reifiers`), which is where purrdf keeps the RDF 1.2
/// statement layer — it is deliberately not in the base quad table, so this is the only
/// place the reifier-to-statement pairing can be seen at all. The authored dataset is not
/// touched.
#[must_use]
pub fn lower_reifiers(dataset: &RdfDataset) -> StatementLowering {
    let mut out = StatementLowering::default();
    for reifier in dataset.owned_reifiers() {
        let statement = reifier.statement;
        if is_triple_term(&statement.subject) || is_triple_term(&statement.object) {
            // A nested triple term has no non-term component to decompose into. Emitting a
            // partial lowering here would be worse than emitting none: a rule joining on
            // subject and predicate alone would treat two different nested claims as one.
            out.nested.push(reifier.reifier);
            continue;
        }
        out.rows.push((
            reifier.reifier.clone(),
            REIFIED_STATEMENT_SUBJECT.to_owned(),
            statement.subject,
        ));
        out.rows.push((
            reifier.reifier.clone(),
            REIFIED_STATEMENT_PREDICATE.to_owned(),
            RdfTerm::Iri(statement.predicate),
        ));
        out.rows.push((
            reifier.reifier,
            REIFIED_STATEMENT_OBJECT.to_owned(),
            statement.object,
        ));
    }
    out
}

#[path = "statement_lowering.tests.rs"]
#[cfg(test)]
mod tests;
