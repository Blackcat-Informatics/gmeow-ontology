// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm, TermValue};
use std::collections::BTreeSet;

const TYPE: &str = "https://blackcatinformatics.ca/logic/instanceOf";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const SUBCLASS: &str = "https://blackcatinformatics.ca/logic/subClassOf";

fn input(rows: &[(&str, &str, &str)]) -> PreparedReasoningInput {
    let mut builder = RdfDatasetBuilder::new();
    for (subject, predicate, object) in rows {
        builder.push_owned_quad(&RdfQuad::new(
            RdfTerm::iri(*subject),
            *predicate,
            RdfTerm::iri(*object),
        ));
    }
    super::super::prepare_reasoning_input(&builder.freeze().unwrap()).unwrap()
}

fn facts(result: &ReasoningResult) -> BTreeSet<(String, String, TermValue, String)> {
    result
        .inferred()
        .iter()
        .map(|row| {
            (
                row.subject.clone(),
                row.predicate.clone(),
                row.object.clone(),
                row.world.clone(),
            )
        })
        .collect()
}

#[test]
fn native_transaction_reuses_supported_proofs_and_charges_only_new_heads() {
    let base = input(&[
        ("urn:A", SUBCLASS, "urn:B"),
        ("urn:B", SUBCLASS, "urn:C"),
        ("urn:old", TYPE, "urn:A"),
    ]);
    let domains = SelectedDomains::new([]).unwrap();
    let candidate = Fact {
        subject: TermValue::iri("urn:new"),
        predicate: TYPE.to_owned(),
        object: TermValue::iri("urn:A"),
    };
    let session = NativeReasoningSession::new(
        base,
        &domains,
        vec![(
            crate::reason::rl::DEFAULT_WORLD.to_owned(),
            candidate.clone(),
        )],
    )
    .unwrap();
    let mut changed = session.input();
    changed
        .assert_fact(LogicalGraph::Default, candidate.clone())
        .unwrap();
    let scratch = crate::reason::reason_all(changed.clone(), &domains).unwrap();
    let mut retained = session.retained.clone();
    let closure = super::super::program::execute_transaction(
        &session.prepared,
        changed,
        &domains,
        None,
        &session.potential,
        Some(&mut retained),
    )
    .unwrap();
    let (updated, _, consumed) =
        super::super::result_from_closure(&session.prepared, closure, None).unwrap();
    assert!(
        retained.reused_rows() > 0,
        "base proofs must actually enter the native store"
    );
    assert!(consumed < scratch.provenance.consumed_budget.consumed);
    assert_eq!(facts(&updated), facts(&scratch));
    updated.validate().unwrap();
    let old = |result: &ReasoningResult| {
        result
            .inferred()
            .iter()
            .filter(|row| row.subject == "urn:old")
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(
        old(&updated),
        old(session.base()),
        "retention preserves existing proof bodies and origins"
    );
    assert!(!session.input().facts[crate::reason::rl::DEFAULT_WORLD].contains(&candidate));
    let cut = session
        .insert(LogicalGraph::Default, candidate, Some(0))
        .unwrap();
    assert_eq!(cut.consumed_steps, 0);
    assert_eq!(cut.status, crate::seam::BudgetStatus::Exhausted);
    assert!(
        !cut.result
            .inferred()
            .iter()
            .any(|row| row.subject == "urn:new" && row.object.as_iri() == Some("urn:B"))
    );
    assert!(
        cut.result
            .inferred()
            .iter()
            .any(|row| row.subject == "urn:old" && row.object.as_iri() == Some("urn:C"))
    );
}

#[test]
fn native_retraction_discards_unsupported_proofs_and_retains_alternative_derivations() {
    let base = input(&[
        ("urn:A", SUBCLASS, "urn:B"),
        ("urn:B", SUBCLASS, "urn:C"),
        ("urn:A", SUBCLASS, "urn:C"),
        ("urn:x", TYPE, "urn:A"),
    ]);
    let domains = SelectedDomains::new([]).unwrap();
    let potential = base
        .facts
        .iter()
        .flat_map(|(world, facts)| facts.iter().cloned().map(|fact| (world.clone(), fact)))
        .collect();
    let session = NativeReasoningSession::new(base, &domains, potential).unwrap();
    for axiom in [
        crate::reason::LeaveOneOutAxiom::new("urn:A", SUBCLASS, "urn:C"),
        crate::reason::LeaveOneOutAxiom::new("urn:x", TYPE, "urn:A"),
    ] {
        let mut changed = session.input();
        assert!(changed.retract_axiom(&axiom).unwrap());
        let scratch = crate::reason::reason_all(changed, &domains).unwrap();
        let updated = session.retract(&axiom).unwrap();
        assert_eq!(facts(&updated), facts(&scratch));
        updated.validate().unwrap();
    }
}

#[test]
fn repeated_assertion_receipt_uses_the_selected_transaction_allowance() {
    let base = input(&[("urn:A", SUBCLASS, "urn:B"), ("urn:x", TYPE, "urn:A")]);
    let domains = SelectedDomains::new([]).unwrap();
    let candidate = Fact {
        subject: TermValue::iri("urn:x"),
        predicate: TYPE.to_owned(),
        object: TermValue::iri("urn:A"),
    };
    let session = NativeReasoningSession::new(base, &domains, vec![]).unwrap();
    assert!(session.base().provenance.consumed_budget.consumed > 0);
    let transaction = session
        .insert(LogicalGraph::Default, candidate, Some(0))
        .unwrap();
    assert_eq!(transaction.consumed_steps, 0);
    assert_eq!(transaction.result.provenance.consumed_budget.consumed, 0);
    assert_eq!(
        transaction.result.provenance.consumed_budget.allowance,
        Some(0)
    );
    assert_eq!(session.base().provenance.consumed_budget.allowance, None);
    transaction.result.validate().unwrap();
}

#[test]
fn native_transactions_select_the_current_shortest_proof_after_source_promotion_or_new_route() {
    let base = input(&[
        ("urn:x", TYPE, "urn:A"),
        ("urn:A", SUBCLASS, "urn:B"),
        ("urn:B", SUBCLASS, "urn:C"),
        ("urn:D", SUBCLASS, "urn:C"),
    ]);
    let domains = SelectedDomains::new([]).unwrap();
    let candidates = ["urn:B", "urn:D"].map(|class| Fact {
        subject: TermValue::iri("urn:x"),
        predicate: TYPE.to_owned(),
        object: TermValue::iri(class),
    });
    let session = NativeReasoningSession::new(
        base,
        &domains,
        candidates
            .iter()
            .cloned()
            .map(|fact| (crate::reason::rl::DEFAULT_WORLD.to_owned(), fact))
            .collect(),
    )
    .unwrap();
    for candidate in candidates {
        let mut changed = session.input();
        changed
            .assert_fact(LogicalGraph::Default, candidate.clone())
            .unwrap();
        let fresh = crate::reason::reason_all(changed, &domains).unwrap();
        let retained = session
            .insert(LogicalGraph::Default, candidate.clone(), None)
            .unwrap()
            .result;
        assert_eq!(facts(&retained), facts(&fresh));
        let conclusion = |result: &ReasoningResult| {
            result
                .inferred()
                .iter()
                .find(|row| {
                    row.subject == "urn:x"
                        && matches!(row.predicate.as_str(), TYPE | RDF_TYPE)
                        && row.object.as_iri() == Some("urn:C")
                        && row.world == crate::reason::rl::DEFAULT_WORLD
                })
                .unwrap_or_else(|| {
                    panic!(
                        "expected urn:x type urn:C in retained native closure; inferred={:#?}",
                        result.inferred()
                    )
                })
                .clone()
        };
        let expected = conclusion(&fresh);
        assert!(
            expected
                .premises
                .iter()
                .any(|(subject, predicate, object)| {
                    subject == "urn:x"
                        && predicate == TYPE
                        && object == &crate::provenance::term_display(&candidate.object)
                }),
            "the new assertion must enable the direct proof: {expected:?}"
        );
        assert_eq!(
            conclusion(&retained),
            expected,
            "reuse must not install a longer old proof before this round's competing firing"
        );
        retained.validate().unwrap();
    }
}
