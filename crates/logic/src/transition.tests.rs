// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::entrenchment::OVERRIDES;
use crate::provenance::{LOGIC_NAMESPACE, NAMESPACE};

const BASE: &str = "https://example.org/state/base";
const NEXT: &str = "https://example.org/state/next";
const NEXT2: &str = "https://example.org/state/next2";
const TX: &str = "https://example.org/tx/1";
const UPDATE_INSERT: &str = "https://example.org/update/insert";
const UPDATE_DELETE: &str = "https://example.org/update/delete";
const S: &str = "https://example.org/s";
const P: &str = "https://example.org/p";
const O: &str = "https://example.org/o";

fn fact() -> TransitionFact {
    TransitionFact::iri(S, P, O).expect("valid fact")
}

#[test]
fn ins_materializes_fact_only_in_successor_state() {
    let store = WorldStore::new();
    let update = ElementaryUpdate::insert(UPDATE_INSERT, TX, fact());

    let report =
        apply_elementary_transition(&store, BASE, NEXT, &[update], BASE).expect("transition");

    assert!(
        store
            .quads_for_pattern_in_world(BASE, Some(S), Some(P), Some(O))
            .is_empty()
    );
    assert_eq!(
        store
            .quads_for_pattern_in_world(NEXT, Some(S), Some(P), Some(O))
            .len(),
        1
    );
    assert_eq!(report.carried_supports, 0);
    assert_eq!(report.inserted_supports, vec![fact().reifier().unwrap()]);
}

#[test]
fn del_retires_support_without_erasing_base_state() {
    let store = WorldStore::new();
    store.insert_quad(BASE, S, P, O);
    let support = fact().reifier().unwrap();
    let update = ElementaryUpdate::delete(UPDATE_DELETE, TX, fact());

    let report =
        apply_elementary_transition(&store, BASE, NEXT, &[update], BASE).expect("transition");

    assert_eq!(
        store
            .quads_for_pattern_in_world(BASE, Some(S), Some(P), Some(O))
            .len(),
        1,
        "base state remains append-only"
    );
    assert!(
        store
            .quads_for_pattern_in_world(NEXT, Some(S), Some(P), Some(O))
            .is_empty()
    );
    assert_eq!(report.retired_supports, vec![support.clone()]);

    assert_eq!(
        store
            .quads_for_pattern_in_world(NEXT, Some(&support), Some(ACTIVE_IN_STATE), Some(BASE))
            .len(),
        1
    );
    assert_eq!(
        store
            .quads_for_pattern_in_world(NEXT, Some(&support), Some(VALID_UNTIL_STATE), Some(NEXT))
            .len(),
        1
    );
    assert_eq!(
        store
            .quads_for_pattern_in_world(
                NEXT,
                Some(&support),
                Some(RETIRED_BY_TRANSACTION),
                Some(TX)
            )
            .len(),
        1
    );
    assert_eq!(
        store
            .quads_for_pattern_in_world(
                NEXT,
                Some(&support),
                Some(SUPERSEDED_BY),
                Some(UPDATE_DELETE)
            )
            .len(),
        1
    );
}

#[test]
fn same_fact_conflict_uses_most_entrenched_update() {
    let store = WorldStore::new();
    store.insert_quad(BASE, S, P, O);
    store.insert_quad(BASE, UPDATE_INSERT, OVERRIDES, UPDATE_DELETE);
    let insert = ElementaryUpdate::insert(UPDATE_INSERT, TX, fact());
    let delete = ElementaryUpdate::delete(UPDATE_DELETE, TX, fact());

    apply_elementary_transition(&store, BASE, NEXT, &[delete, insert], BASE)
        .expect("insert update is more entrenched");

    assert_eq!(
        store
            .quads_for_pattern_in_world(NEXT, Some(S), Some(P), Some(O))
            .len(),
        1
    );
}

#[test]
fn duplicate_conflict_candidates_do_not_create_false_ties() {
    let store = WorldStore::new();
    store.insert_quad(BASE, S, P, O);
    store.insert_quad(BASE, UPDATE_INSERT, OVERRIDES, UPDATE_DELETE);
    let insert = ElementaryUpdate::insert(UPDATE_INSERT, TX, fact());
    let delete = ElementaryUpdate::delete(UPDATE_DELETE, TX, fact());

    apply_elementary_transition(&store, BASE, NEXT, &[delete, insert.clone(), insert], BASE)
        .expect("duplicated winning update IRI is still a unique winner");

    assert_eq!(
        store
            .quads_for_pattern_in_world(NEXT, Some(S), Some(P), Some(O))
            .len(),
        1
    );
}

#[test]
fn incomparable_conflicting_updates_are_illegal() {
    let store = WorldStore::new();
    store.insert_quad(BASE, S, P, O);
    let insert = ElementaryUpdate::insert(UPDATE_INSERT, TX, fact());
    let delete = ElementaryUpdate::delete(UPDATE_DELETE, TX, fact());

    let err = apply_elementary_transition(&store, BASE, NEXT, &[delete, insert], BASE)
        .expect_err("incomparable updates must fail");
    assert!(
        err.message().contains("ambiguous elementary updates"),
        "got: {err}"
    );
    assert!(
        store
            .quads_for_pattern_in_world(NEXT, None, None, None)
            .is_empty()
    );

    store.insert_quad(BASE, UPDATE_DELETE, OVERRIDES, UPDATE_INSERT);
    apply_elementary_transition(
        &store,
        BASE,
        NEXT2,
        &[
            ElementaryUpdate::delete(UPDATE_DELETE, TX, fact()),
            ElementaryUpdate::insert(UPDATE_INSERT, TX, fact()),
        ],
        BASE,
    )
    .expect("delete update is now more entrenched");
    assert!(
        store
            .quads_for_pattern_in_world(NEXT2, Some(S), Some(P), Some(O))
            .is_empty()
    );
}

#[test]
fn del_of_absent_support_is_illegal() {
    let store = WorldStore::new();
    let update = ElementaryUpdate::delete(UPDATE_DELETE, TX, fact());
    let err = apply_elementary_transition(&store, BASE, NEXT, &[update], BASE)
        .expect_err("absent support cannot be retired");
    assert!(err.message().contains("illegal del"), "got: {err}");
    assert!(
        store
            .quads_for_pattern_in_world(NEXT, None, None, None)
            .is_empty()
    );
}

#[test]
fn successor_state_must_be_fresh() {
    let store = WorldStore::new();
    store.insert_quad(NEXT, S, P, O);
    let update = ElementaryUpdate::insert(UPDATE_INSERT, TX, fact());
    let err = apply_elementary_transition(&store, BASE, NEXT, &[update], BASE)
        .expect_err("successor is not fresh");
    assert!(
        err.message().contains("already contains quads"),
        "got: {err}"
    );
}

#[test]
fn metadata_constants_stay_in_expected_namespaces() {
    assert!(ACTIVE_IN_STATE.starts_with(LOGIC_NAMESPACE));
    assert!(VALID_UNTIL_STATE.starts_with(LOGIC_NAMESPACE));
    assert!(RETIRED_BY_TRANSACTION.starts_with(LOGIC_NAMESPACE));
    assert!(SUPERSEDED_BY.starts_with(NAMESPACE));
}
