// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::runtime::{OutcomeTag, TransitionEntry};
use std::fmt::Write;

/// Synthetic observation records only. No repository source, producer, cache,
/// corpus fixture, or reasoning closure is constructed by this helper.
fn source(boundary: &str, count: usize, operator: &str) -> String {
    let mut source = format!(
        "{SOURCE} {}",
        QUERY.replace("queryFormula ex:formula", "queryFormula ex:temporal")
    );
    let initial = blake3::hash(b"synthetic journal genesis")
        .to_hex()
        .to_string();
    write!(
        source,
        r#"
ex:temporal a logic:Formula ; logic:{operator} ex:formula .
ex:c logic:contextEnactment ex:run ; logic:contextJournal ex:journal ;
    logic:contextPosition "0"^^<http://www.w3.org/2001/XMLSchema#nonNegativeInteger> .
ex:run a logic:Enactment ; logic:enactmentJournal ex:journal .
ex:journal a logic:TransitionJournal ; logic:journalBoundary logic:{boundary} ;
    logic:journalInitialHead "blake3:{initial}" ; logic:journalHead ex:entry{} .
"#,
        count - 1
    )
    .expect("string write");
    let mut previous = initial;
    for index in 0..count {
        let delta = blake3::hash(&index.to_le_bytes()).to_hex().to_string();
        let entry = TransitionEntry::advance(&previous, &delta, OutcomeTag::Applied);
        write!(
            source,
            r#"
ex:journal logic:journalEntry ex:entry{index} .
ex:entry{index} a logic:JournalEntry ; logic:journalPrevHead "blake3:{previous}" ;
    logic:journalNewHead "blake3:{}" ; logic:journalDeltaIdentity "blake3:{delta}" ;
    logic:journalOutcomeTag logic:OutcomeApplied .
"#,
            entry.new_state_hash
        )
        .expect("string write");
        if index != 0 {
            let predecessor = index - 1;
            let current = if index == 1 {
                "ex:c".into()
            } else {
                format!("ex:c{predecessor}")
            };
            write!(
                source,
                r#"
ex:entry{index} logic:journalPredecessor ex:entry{predecessor} .
ex:c{index} a logic:AttributedContext ; logic:contextWorld ex:otherWorld ;
    logic:contextStandpoint ex:s ; logic:evidenceClosure logic:ClosedWorldClosure ;
    logic:contextEnactment ex:run ; logic:contextJournal ex:journal ;
    logic:contextPosition "{index}"^^<http://www.w3.org/2001/XMLSchema#nonNegativeInteger> .
ex:step{index} a logic:ContextSuccessorSet ; logic:successorContext {current} ;
    logic:successorAxis logic:temporallySucceeds ; logic:successorMember ex:c{index} ;
    logic:successorClosure logic:ClosedWorldClosure .
"#
            )
            .expect("string write");
        }
        previous = entry.new_state_hash;
    }
    source
}

fn assess(source: &str) -> ContextualAssessment {
    evaluate_request(&parse(source), "urn:example:request", None, None)
        .expect("synthetic temporal request")
}

fn journal_prefix(assessment: &ContextualAssessment) -> &TemporalPrefix {
    let TemporalBasis::Journal(prefix) = &assessment.temporal_prefixes[0] else {
        panic!("a journal request must never fall back to a state path");
    };
    prefix
}

#[test]
fn next_selects_the_committed_successor_and_exports_its_exact_prefix() {
    let assessment = assess(&source("OpenJournalBoundary", 2, "next"));
    assert_eq!(assessment.result.information, InformationState::Both);
    assert_eq!(assessment.temporal_prefixes.len(), 1);
    let prefix = journal_prefix(&assessment);
    assert_eq!(prefix.head, "urn:example:entry1");
    assert!(!prefix.finalized);
    for proof in [
        &assessment.result.provenance.proof,
        &assessment.result.provenance.counterproof,
    ] {
        let proof = proof.as_ref().expect("independent evidence");
        assert!(proof.cited_iris.contains(&prefix.identity));
        assert!(proof.cited_iris.contains("urn:example:entry1"));
        assert!(proof.cited_iris.contains("urn:example:step1"));
    }
    let output = crate::result_rdf::project_contextual_dataset(&assessment)
        .expect("project the valid native temporal result");
    assert!(output.contains("ObservedTemporalPrefix"));
    assert!(output.contains("OpenJournalBoundary"));
    let emitted = purrdf::parse_dataset(output.as_bytes(), "application/n-quads", None).unwrap();
    for (predicate, object) in [
        (
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
            format!("{LOGIC_NAMESPACE}ObservedTemporalPrefix"),
        ),
        (format!("{LOGIC_NAMESPACE}prefixHead"), prefix.head.clone()),
        (
            format!("{LOGIC_NAMESPACE}journalBoundary"),
            format!("{LOGIC_NAMESPACE}OpenJournalBoundary"),
        ),
    ] {
        assert!(
            emitted.owned_quads().any(|row| {
                row.subject == purrdf::RdfTerm::iri(&prefix.identity)
                    && row.predicate == predicate
                    && row.object == purrdf::RdfTerm::iri(&object)
                    && row.graph_name
                        == Some(purrdf::RdfTerm::iri(crate::result_rdf::GRAPH_REASONING))
            }),
            "the selected prefix retains its exact {predicate} output"
        );
    }
}

#[test]
fn finalization_changes_the_verdict_and_result_identity_without_inventing_an_event() {
    let open = assess(&source("OpenJournalBoundary", 1, "next"));
    let finalized = assess(&source("FinalizedJournalBoundary", 1, "next"));
    assert_eq!(open.result.information, InformationState::Undetermined);
    assert_eq!(finalized.result.information, InformationState::Opposed);
    assert_ne!(
        journal_prefix(&open).identity,
        journal_prefix(&finalized).identity
    );
    assert_ne!(
        crate::result_rdf::result_node_iri(&open.result)
            .expect("project the valid native temporal result"),
        crate::result_rdf::result_node_iri(&finalized.result)
            .expect("project the valid native temporal result")
    );
    assert_eq!(journal_prefix(&open).head, journal_prefix(&finalized).head);
}

#[test]
fn temporal_sugar_uses_the_same_source_admission_and_shared_formula_ir() {
    for operator in ["next", "eventually", "globally"] {
        let input = source("FinalizedJournalBoundary", 2, operator);
        let assessment = assess(&input);
        assert_eq!(assessment.result.evaluation, EvaluationStatus::Completed);
        let dataset = parse(&input);
        let formula = gmeow_logic_compile::frontend::reconstruct_formula_in_context(
            &dataset,
            "urn:example:temporal",
            "urn:example:c",
        )
        .expect("shared source reconstruction");
        assert!(format!("{formula:?}").contains("finite"));
    }
    let until =
        source("FinalizedJournalBoundary", 2, "until") + "ex:temporal logic:untilLeft ex:formula .";
    assert_eq!(
        assess(&until).result.information,
        InformationState::Supported
    );
    for invalid in [
        source("FinalizedJournalBoundary", 1, "until"),
        source("FinalizedJournalBoundary", 1, "next") + "ex:temporal logic:globally ex:formula .",
        source("FinalizedJournalBoundary", 1, "next")
            .replace("logic:next ex:formula", "logic:next ex:temporal"),
    ] {
        assert!(evaluate_request(&parse(&invalid), "urn:example:request", None, None).is_err());
    }
}

#[test]
fn a_temporal_request_cannot_borrow_a_hash_scope_or_observed_adjacency() {
    let input = source("FinalizedJournalBoundary", 2, "next");
    for invalid in [
        input.replace("logic:OutcomeApplied", "logic:OutcomeInvalid"),
        input.replace(
            "logic:journalHead ex:entry1",
            "logic:journalHead ex:missing",
        ),
        input.replace(
            "logic:journalEntry ex:entry0",
            "logic:unselectedEntry ex:entry0",
        ),
        input.replace(
            "logic:journalBoundary logic:FinalizedJournalBoundary",
            "logic:unselectedBoundary logic:FinalizedJournalBoundary",
        ),
        input.replace(
            "logic:enactmentJournal ex:journal",
            "logic:enactmentJournal ex:otherJournal",
        ),
        input.replace(
            "logic:successorClosure logic:ClosedWorldClosure",
            "logic:successorClosure logic:OpenWorldClosure",
        ),
        input.replace(
            "logic:successorMember ex:c1",
            "logic:successorMember ex:c1, ex:other",
        ),
    ] {
        assert!(
            evaluate_request(&parse(&invalid), "urn:example:request", None, None).is_err(),
            "accepted invalid journal observation"
        );
    }
}

#[test]
fn unrelated_legacy_journals_do_not_weaken_or_block_selected_temporal_admission() {
    let input = source("FinalizedJournalBoundary", 1, "next");
    let with_unrelated =
        format!("{input} ex:legacy a logic:TransitionJournal ; logic:journalHead ex:unknown .");
    let original = assess(&input);
    let unrelated = assess(&with_unrelated);
    assert_eq!(
        crate::result_rdf::project_contextual_dataset(&original)
            .expect("project the valid native temporal result"),
        crate::result_rdf::project_contextual_dataset(&unrelated)
            .expect("project the valid native temporal result")
    );
}

#[test]
fn journal_ownership_is_checked_before_query_order_can_choose_an_enactment() {
    let input = source("FinalizedJournalBoundary", 1, "next")
        + "ex:anotherRun a logic:Enactment ; logic:enactmentJournal ex:journal .";
    let dataset = parse(&input);
    let error = evaluate_request(&dataset, "urn:example:request", None, None)
        .expect_err("a selected journal cannot have two owning enactments");
    assert!(error.to_string().contains("ambiguous enactment ownership"));
    assert!(evaluate_requests(&dataset, None, None).is_err());
}

#[test]
fn appended_monitor_judgments_match_fresh_evaluation_and_reuse_committed_proofs() {
    use std::num::NonZeroUsize;
    for operator in ["next", "eventually", "globally", "until"] {
        let observation = |boundary, count| {
            let mut text = source(boundary, count, operator);
            if operator == "until" {
                text.push_str("ex:temporal logic:untilLeft ex:formula .");
            }
            parse(&text)
        };
        let initial = observation("OpenJournalBoundary", 1);
        let mut monitor = ContextualMonitor::compile(
            &initial,
            "urn:example:request",
            NonZeroUsize::new(128).unwrap(),
        )
        .unwrap();
        for (boundary, count) in [
            ("OpenJournalBoundary", 1),
            ("OpenJournalBoundary", 2),
            ("OpenJournalBoundary", 3),
            ("FinalizedJournalBoundary", 3),
        ] {
            let dataset = observation(boundary, count);
            let incremental = monitor.observe(&dataset, 1000, None).unwrap();
            let fresh =
                evaluate_request(&dataset, "urn:example:request", Some(1000), None).unwrap();
            assert_eq!(
                incremental.result.information, fresh.result.information,
                "{operator}/{count}/{boundary}"
            );
            assert_eq!(incremental.result.completeness, fresh.result.completeness);
            assert_eq!(incremental.result.evaluation, EvaluationStatus::Completed);
            assert_eq!(
                incremental.result.provenance.proof,
                fresh.result.provenance.proof
            );
            assert_eq!(
                incremental.result.provenance.counterproof,
                fresh.result.provenance.counterproof
            );
            assert_eq!(incremental.inferences, fresh.inferences);
            assert_eq!(incremental.anchors, fresh.anchors);
            assert_eq!(incremental.temporal_prefixes, fresh.temporal_prefixes);
            if count == 3 {
                assert!(
                    incremental.result.provenance.consumed_budget.consumed
                        < fresh.result.provenance.consumed_budget.consumed,
                    "the monitor reuses prior committed judgments"
                );
            }
        }
    }
}

#[test]
fn monitor_rejects_changed_past_or_compiled_inputs_without_advancing() {
    use std::num::NonZeroUsize;
    let initial_text = source("OpenJournalBoundary", 1, "eventually");
    let initial = parse(&initial_text);
    let mut monitor = ContextualMonitor::compile(
        &initial,
        "urn:example:request",
        NonZeroUsize::new(64).unwrap(),
    )
    .unwrap();
    monitor.observe(&initial, 1000, None).unwrap();
    for altered in [
        initial_text.replace("gmeow:supportSupported", "gmeow:supportOpposed"),
        initial_text.replace("logic:eventually ex:formula", "logic:globally ex:formula"),
        initial_text.replace("logic:queryContext ex:c", "logic:queryContext ex:other"),
    ] {
        assert!(monitor.observe(&parse(&altered), 1000, None).is_err());
    }
    let appended = parse(&source("OpenJournalBoundary", 2, "eventually"));
    let accepted = monitor.observe(&appended, 1000, None).unwrap();
    assert_eq!(journal_prefix(&accepted).head, "urn:example:entry1");
    assert!(
        monitor.observe(&initial, 1000, None).is_err(),
        "a prefix cannot retract"
    );
    let final_input = parse(&source("FinalizedJournalBoundary", 2, "eventually"));
    monitor.observe(&final_input, 1000, None).unwrap();
    assert!(
        monitor.observe(&appended, 1000, None).is_err(),
        "finalization cannot be undone"
    );
    assert!(
        monitor
            .observe(
                &parse(&source("FinalizedJournalBoundary", 3, "eventually")),
                1000,
                None
            )
            .is_err(),
        "finalized journals cannot append"
    );
}

#[test]
fn monitor_eviction_recomputes_and_cancelled_cache_hits_cannot_succeed() {
    use purrdf::sparql::CancellationFlag;
    use std::num::NonZeroUsize;
    let initial = parse(&source("OpenJournalBoundary", 1, "globally"));
    let mut small = ContextualMonitor::compile(
        &initial,
        "urn:example:request",
        NonZeroUsize::new(1).unwrap(),
    )
    .unwrap();
    let mut large = ContextualMonitor::compile(
        &initial,
        "urn:example:request",
        NonZeroUsize::new(128).unwrap(),
    )
    .unwrap();
    small.observe(&initial, 1000, None).unwrap();
    large.observe(&initial, 1000, None).unwrap();
    let appended = parse(&source("OpenJournalBoundary", 4, "globally"));
    small.observe(&appended, 1000, None).unwrap();
    large.observe(&appended, 1000, None).unwrap();
    let small_result = small.observe(&appended, 1000, None).unwrap();
    let large_result = large.observe(&appended, 1000, None).unwrap();
    assert_eq!(
        small_result.result.information,
        large_result.result.information
    );
    assert_eq!(
        small_result.result.provenance.proof,
        large_result.result.provenance.proof
    );
    assert_eq!(
        small_result.result.provenance.counterproof,
        large_result.result.provenance.counterproof
    );
    assert!(
        small_result.result.provenance.consumed_budget.consumed
            > large_result.result.provenance.consumed_budget.consumed,
        "eviction recomputes without changing semantics"
    );
    let exhausted = small.observe(&appended, 0, None).unwrap();
    assert_eq!(exhausted.interrupted, Some(IncompleteCause::StepBudget));
    assert_eq!(exhausted.result.information, InformationState::Undetermined);
    let stop = CancellationFlag::new();
    stop.cancel();
    let cancelled = large.observe(&appended, 1000, Some(&stop)).unwrap();
    assert_eq!(cancelled.interrupted, Some(IncompleteCause::Cancelled));
    assert_eq!(cancelled.result.provenance.consumed_budget.consumed, 0);
    assert!(cancelled.result.provenance.proof.is_none());
}
