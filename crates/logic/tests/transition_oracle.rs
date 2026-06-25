// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration coverage for elementary Transaction-Logic state transitions (#712).
//!
//! The full transaction-path frontend is tracked separately. This test pins the
//! Rust oracle surface that frontend will call: a base `WorldStore` state is
//! preserved, a fresh successor state is materialized, and `del(F)` records
//! supersession provenance instead of erasing historical support.

use gmeow_logic::store::WorldStore;
use gmeow_logic::transition::{
    apply_elementary_transition, ElementaryUpdate, TransitionFact, ACTIVE_IN_STATE,
    RETIRED_BY_TRANSACTION, SUPERSEDED_BY, VALID_UNTIL_STATE,
};

const BASE: &str = "https://example.org/state/base";
const NEXT: &str = "https://example.org/state/next";
const TX: &str = "https://example.org/tx/transition-1";
const UPDATE_INSERT: &str = "https://example.org/update/insert-current";
const UPDATE_DELETE: &str = "https://example.org/update/delete-obsolete";
const SUBJECT: &str = "https://example.org/resource/record";
const PREDICATE: &str = "https://example.org/schema/status";
const OLD_OBJECT: &str = "https://example.org/value/obsolete";
const NEW_OBJECT: &str = "https://example.org/value/current";

#[test]
fn elementary_transition_materializes_successor_and_retirement_provenance() {
    let store = WorldStore::new();
    store.insert_quad(BASE, SUBJECT, PREDICATE, OLD_OBJECT);

    let old_fact =
        TransitionFact::iri(SUBJECT, PREDICATE, OLD_OBJECT).expect("valid old support fact");
    let new_fact =
        TransitionFact::iri(SUBJECT, PREDICATE, NEW_OBJECT).expect("valid new support fact");
    let old_support = old_fact.reifier().expect("support IRI");
    let new_support = new_fact.reifier().expect("support IRI");

    let report = apply_elementary_transition(
        &store,
        BASE,
        NEXT,
        &[
            ElementaryUpdate::delete(UPDATE_DELETE, TX, old_fact),
            ElementaryUpdate::insert(UPDATE_INSERT, TX, new_fact),
        ],
        BASE,
    )
    .expect("elementary transition applies");

    assert_eq!(report.carried_supports, 0);
    assert_eq!(report.retired_supports, vec![old_support.clone()]);
    assert_eq!(report.inserted_supports, vec![new_support.clone()]);

    assert_eq!(
        store
            .quads_for_pattern_in_world(BASE, Some(SUBJECT), Some(PREDICATE), Some(OLD_OBJECT))
            .len(),
        1,
        "base state is preserved"
    );
    assert!(store
        .quads_for_pattern_in_world(NEXT, Some(SUBJECT), Some(PREDICATE), Some(OLD_OBJECT))
        .is_empty());
    assert_eq!(
        store
            .quads_for_pattern_in_world(NEXT, Some(SUBJECT), Some(PREDICATE), Some(NEW_OBJECT))
            .len(),
        1,
        "successor state has the inserted support"
    );

    assert_eq!(
        store
            .quads_for_pattern_in_world(NEXT, Some(&old_support), Some(ACTIVE_IN_STATE), Some(BASE))
            .len(),
        1
    );
    assert_eq!(
        store
            .quads_for_pattern_in_world(
                NEXT,
                Some(&old_support),
                Some(VALID_UNTIL_STATE),
                Some(NEXT)
            )
            .len(),
        1
    );
    assert_eq!(
        store
            .quads_for_pattern_in_world(
                NEXT,
                Some(&old_support),
                Some(SUPERSEDED_BY),
                Some(UPDATE_DELETE)
            )
            .len(),
        1
    );
    assert_eq!(
        store
            .quads_for_pattern_in_world(
                NEXT,
                Some(&old_support),
                Some(RETIRED_BY_TRANSACTION),
                Some(TX)
            )
            .len(),
        1
    );
    assert_eq!(
        store
            .quads_for_pattern_in_world(NEXT, Some(&new_support), Some(ACTIVE_IN_STATE), Some(NEXT))
            .len(),
        1
    );
}
