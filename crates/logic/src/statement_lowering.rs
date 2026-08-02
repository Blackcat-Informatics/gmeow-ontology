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

#[cfg(test)]
mod tests {
    use super::{
        REIFIED_STATEMENT_OBJECT, REIFIED_STATEMENT_PREDICATE, REIFIED_STATEMENT_SUBJECT,
        lower_reifiers,
    };
    use purrdf::RdfTerm;

    fn parse(ttl: &str) -> std::sync::Arc<purrdf::RdfDataset> {
        purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("valid RDF 1.2 Turtle")
    }

    /// The shipped shape: two reifiers over ONE statement, each lowered into three edges
    /// whose components are IDENTICAL — which is the whole point, because that identity is
    /// what a rule joins on.
    #[test]
    fn two_attributions_of_one_statement_lower_to_the_same_three_components() {
        let ds = parse(
            r#"
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/> .

ex:cmdrSays rdf:reifies <<( ex:rollback gmeow:strictlyOver ex:hotfix )>> ;
    gmeow:standpointSupportStatus gmeow:supportSupported .
ex:onCallSays rdf:reifies <<( ex:rollback gmeow:strictlyOver ex:hotfix )>> ;
    gmeow:standpointSupportStatus gmeow:supportOpposed .
"#,
        );
        let lowering = lower_reifiers(&ds);
        assert!(
            lowering.nested.is_empty(),
            "neither statement nests a triple term"
        );
        assert_eq!(
            lowering.rows.len(),
            6,
            "three component edges per reifier: {:?}",
            lowering.rows
        );
        for reifier in [
            "https://example.org/cmdrSays",
            "https://example.org/onCallSays",
        ] {
            let mine: Vec<&(RdfTerm, String, RdfTerm)> = lowering
                .rows
                .iter()
                .filter(|(s, _, _)| matches!(s, RdfTerm::Iri(iri) if iri == reifier))
                .collect();
            assert_eq!(mine.len(), 3, "{reifier} must yield all three components");
            let subject = mine
                .iter()
                .find(|(_, p, _)| p == REIFIED_STATEMENT_SUBJECT)
                .map(|(_, _, o)| o.clone());
            let predicate = mine
                .iter()
                .find(|(_, p, _)| p == REIFIED_STATEMENT_PREDICATE)
                .map(|(_, _, o)| o.clone());
            let object = mine
                .iter()
                .find(|(_, p, _)| p == REIFIED_STATEMENT_OBJECT)
                .map(|(_, _, o)| o.clone());
            assert_eq!(
                subject,
                Some(RdfTerm::Iri("https://example.org/rollback".to_owned()))
            );
            assert_eq!(
                predicate,
                Some(RdfTerm::Iri(
                    "https://blackcatinformatics.ca/gmeow/strictlyOver".to_owned()
                )),
                "the reified PREDICATE rides in object position, or the two attributions \
                 could be joined across different relations between the same endpoints"
            );
            assert_eq!(
                object,
                Some(RdfTerm::Iri("https://example.org/hotfix".to_owned()))
            );
        }
    }

    /// A NESTED triple term is named as residue, never partially lowered.
    ///
    /// A partial lowering would be worse than none: two different nested claims sharing a
    /// subject and a predicate would join as one, and the rule reading them would report a
    /// contestation nobody made.
    #[test]
    fn a_nested_triple_term_is_reported_as_residue_and_never_lowered() {
        let ds = parse(
            r#"
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/> .

ex:flat rdf:reifies <<( ex:a ex:p ex:b )>> .
ex:nested rdf:reifies <<( ex:a ex:p <<( ex:c ex:q ex:d )>> )>> .
"#,
        );
        let lowering = lower_reifiers(&ds);
        assert_eq!(
            lowering.nested,
            vec![RdfTerm::Iri("https://example.org/nested".to_owned())],
            "the nested attribution must be NAMED as residue: {:?}",
            lowering.nested
        );
        assert!(
            lowering
                .rows
                .iter()
                .all(|(s, _, _)| !matches!(s, RdfTerm::Iri(iri) if iri.ends_with("/nested"))),
            "not one component edge may be emitted for a nested statement: {:?}",
            lowering.rows
        );
        assert_eq!(
            lowering.rows.len(),
            3,
            "the FLAT attribution beside it still lowers — the residue is narrow, not a \
             blanket refusal"
        );
    }

    /// A dataset with no RDF 1.2 statement metadata lowers to nothing, and says so.
    #[test]
    fn a_dataset_with_no_reifier_lowers_to_nothing() {
        let ds = parse("@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\n");
        let lowering = lower_reifiers(&ds);
        assert!(lowering.rows.is_empty() && lowering.nested.is_empty());
    }
}
