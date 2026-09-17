// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::runtime::OutcomeTag;

fn digest(label: &str) -> String {
    blake3::hash(label.as_bytes()).to_hex().to_string()
}

fn observation(count: usize) -> JournalObservation {
    let initial_head = digest("initial state");
    let mut entries: Vec<ObservedEntry> = Vec::new();
    for index in 0..count {
        let previous = entries.last();
        let transition = TransitionEntry::advance(
            previous.map_or(initial_head.as_str(), |entry| {
                &entry.transition.new_state_hash
            }),
            digest(&format!("observed delta {index}")),
            OutcomeTag::Applied,
        );
        entries.push(ObservedEntry {
            identity: format!("urn:journal:entry:{index}"),
            predecessor: previous.map(|entry| entry.identity.clone()),
            transition,
        });
    }
    JournalObservation {
        identity: "urn:journal:selected".into(),
        enactment: "urn:enactment:selected".into(),
        initial_head,
        head_entry: entries
            .last()
            .map_or("urn:journal:absent", |entry| &entry.identity)
            .into(),
        entries,
        boundary: JournalBoundary::Open,
    }
}

fn next(journal: &FiniteJournal) -> ObservedEntry {
    ObservedEntry {
        identity: format!("urn:journal:entry:{}", journal.entries().len()),
        predecessor: Some(journal.entries().last().unwrap().identity.clone()),
        transition: TransitionEntry::advance(
            journal.head_hash(),
            digest(&format!("observed delta {}", journal.entries().len())),
            OutcomeTag::Applied,
        ),
    }
}

#[test]
fn unordered_inventory_recovers_the_exact_runtime_chain() {
    let source = observation(3);
    let expected = source.entries.clone();
    let normal = FiniteJournal::admit(source.clone()).unwrap();
    let mut reversed = source;
    reversed.entries.reverse();
    let recovered = FiniteJournal::admit(reversed).unwrap();
    assert_eq!(recovered.entries(), expected);
    assert_eq!(recovered.prefix_identity(), normal.prefix_identity());
    assert_eq!(recovered.boundary(), JournalBoundary::Open);
    assert_eq!(recovered.identity(), "urn:journal:selected");
    assert_eq!(recovered.enactment(), "urn:enactment:selected");
}

#[test]
fn hash_tampering_and_a_valid_hash_with_the_wrong_predecessor_both_fail() {
    let source = observation(3);
    let mut corrupt = source.clone();
    corrupt.entries[1].transition.new_state_hash = digest("substituted head");
    assert!(matches!(
        FiniteJournal::admit(corrupt),
        Err(JournalError::Invalid(_))
    ));

    let mut wrong_link = source;
    wrong_link.entries[1].transition = TransitionEntry::advance(
        digest("another valid predecessor"),
        &wrong_link.entries[1].transition.delta_identity,
        OutcomeTag::Applied,
    );
    assert!(matches!(
        FiniteJournal::admit(wrong_link),
        Err(JournalError::Invalid(_))
    ));
}

#[test]
fn malformed_and_wrong_genesis_hashes_are_not_admitted() {
    for invalid in [
        "short".to_owned(),
        "A".repeat(64),
        digest("another genesis"),
    ] {
        let mut source = observation(1);
        source.initial_head = invalid;
        assert!(matches!(
            FiniteJournal::admit(source),
            Err(JournalError::Invalid(_))
        ));
    }
    let mut source = observation(1);
    source.entries[0].transition.delta_identity = "unverified delta label".into();
    assert!(matches!(
        FiniteJournal::admit(source),
        Err(JournalError::Invalid(_))
    ));
}

#[test]
fn selected_inventory_rejects_forks_cycles_disconnection_and_missing_heads() {
    let source = observation(3);
    let mut fork = source.clone();
    fork.entries.push(ObservedEntry {
        identity: "urn:journal:fork".into(),
        predecessor: Some(fork.entries[0].identity.clone()),
        transition: TransitionEntry::advance(
            &fork.entries[0].transition.new_state_hash,
            digest("forked operation"),
            OutcomeTag::Applied,
        ),
    });
    let mut cycle = source.clone();
    cycle.entries[0].predecessor = Some(cycle.head_entry.clone());
    let mut disconnected = source.clone();
    disconnected.entries.push(ObservedEntry {
        identity: "urn:journal:disconnected".into(),
        predecessor: None,
        transition: TransitionEntry::advance(
            &disconnected.initial_head,
            digest("another genesis operation"),
            OutcomeTag::Applied,
        ),
    });
    let mut missing = source;
    missing.head_entry = "urn:journal:missing".into();
    for malformed in [fork, cycle, disconnected, missing] {
        assert!(matches!(
            FiniteJournal::admit(malformed),
            Err(JournalError::Invalid(_))
        ));
    }
}

#[test]
fn duplicate_entry_names_and_duplicate_committed_heads_are_invalid() {
    let source = observation(2);
    let mut names = source.clone();
    names.entries[1].identity = names.entries[0].identity.clone();
    let mut heads = source;
    let mut copy = heads.entries[0].clone();
    copy.identity = "urn:journal:another-name-for-one-commit".into();
    heads.entries.push(copy);
    for malformed in [names, heads] {
        assert!(matches!(
            FiniteJournal::admit(malformed),
            Err(JournalError::Invalid(_))
        ));
    }
}

#[test]
fn incremental_append_and_fresh_admission_have_identical_prefixes() {
    let mut incremental = FiniteJournal::admit(observation(1)).unwrap();
    let first = incremental.entries()[0].clone();
    for count in 2..=8 {
        incremental.append(next(&incremental)).unwrap();
        let fresh = FiniteJournal::admit(observation(count)).unwrap();
        assert_eq!(incremental.entries(), fresh.entries());
        assert_eq!(incremental.prefix_identity(), fresh.prefix_identity());
        assert_eq!(incremental.entries()[0], first, "history is immutable");
    }
}

#[test]
fn failed_append_is_atomic_and_finalization_is_explicit_and_head_bound() {
    let mut journal = FiniteJournal::admit(observation(2)).unwrap();
    let original = journal.prefix_identity();
    let mut stale = next(&journal);
    stale.predecessor = Some(journal.entries()[0].identity.clone());
    assert!(journal.append(stale).is_err());
    assert_eq!(journal.entries().len(), 2);
    assert_eq!(journal.prefix_identity(), original);
    assert!(journal.finalize(&digest("not this head")).is_err());
    assert_eq!(journal.boundary(), JournalBoundary::Open);
    assert_eq!(journal.prefix_identity(), original);

    let head = journal.head_hash().to_owned();
    journal.finalize(&head).unwrap();
    let finalized = journal.prefix_identity();
    assert_ne!(finalized, original, "finalization contributes evidence");
    assert!(journal.append(next(&journal)).is_err());
    journal.finalize(&head).unwrap();
    assert_eq!(journal.prefix_identity(), finalized);
    let mut source = observation(2);
    source.boundary = JournalBoundary::Finalized;
    assert_eq!(
        FiniteJournal::admit(source).unwrap().prefix_identity(),
        finalized
    );
}

#[test]
fn scope_is_part_of_the_observed_prefix_identity() {
    let source = observation(1);
    let expected = FiniteJournal::admit(source.clone())
        .unwrap()
        .prefix_identity();
    let mut other_journal = source.clone();
    other_journal.identity = "urn:journal:another".into();
    let mut other_enactment = source;
    other_enactment.enactment = "urn:enactment:another".into();
    for changed in [other_journal, other_enactment] {
        assert_ne!(
            FiniteJournal::admit(changed).unwrap().prefix_identity(),
            expected
        );
    }
}

#[test]
fn empty_input_is_invalid_but_admission_exhaustion_is_not_a_semantic_verdict() {
    assert!(matches!(
        FiniteJournal::admit(observation(0)),
        Err(JournalError::Invalid(_))
    ));
    assert!(matches!(
        FiniteJournal::admit_with_limit(observation(3), 2),
        Err(JournalError::AdmissionLimit { limit: 2 }),
    ));
}
