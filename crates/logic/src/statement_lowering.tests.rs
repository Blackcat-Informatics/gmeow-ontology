// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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
