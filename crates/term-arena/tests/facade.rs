// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The curated façade, exercised from OUTSIDE the crate.
//!
//! These are integration tests on purpose: they compile only against the public surface,
//! so they prove the façade is genuinely reachable by a consumer that links no reasoning
//! runtime — and that the dense per-arena integers are NOT (nothing here can name a
//! `NodeId`).
//!
//! The arena's internal invariant suite (content-key injectivity, alpha-collapse, the
//! metavariable identity ladder, and the DAG ↔ `logic:` IR congruence) lives with the
//! lowering that exercises it, in `gmeow-logic`'s `physical::term_dag_tests` — the
//! congruence half cannot move here without dragging the compiler IR into this crate.

use gmeow_term_arena::{Arena, ContentKey, StructNode, TermArena};
use purrdf::TermValue;

fn iri(s: &str) -> TermValue {
    TermValue::iri(s)
}

/// `∀_:Sort. p(#0.0)` — built from independently-constructed child nodes each time, so
/// nothing but hash-consing can make two builds agree.
fn build_forall_p(arena: &mut TermArena) -> StructNode {
    let p = arena.intern_leaf(iri("https://example.org/p"));
    let bound = arena.intern_bound(0, 0);
    let body = arena.intern_app(p, &[bound]).expect("own nodes");
    let sort = arena.intern_leaf(iri("https://example.org/Sort"));
    let forall = arena.intern_leaf(iri("https://example.org/forall"));
    arena
        .intern_binder(forall, &[sort], body)
        .expect("own nodes")
}

/// One normalized subexpression, interned bottom-up. SIX distinct nodes when it is new and
/// zero when it is already interned — but SIX intern calls either way.
fn intern_subexpression(arena: &mut TermArena) -> StructNode {
    build_forall_p(arena)
}

/// Two alpha-equivalent terms — same de-Bruijn structure, built twice — must return the
/// SAME `StructNode` and the SAME `ContentKey` through the façade.
///
/// Bound occurrences are locally-nameless, so there is no name in the key to normalize:
/// alpha-equivalence is literally node identity.
#[test]
fn facade_alpha_equal_terms_share_one_node_and_one_key() {
    let mut arena = TermArena::new();
    let first = build_forall_p(&mut arena);
    let nodes_after_first = arena.distinct_nodes();
    let second = build_forall_p(&mut arena);

    assert_eq!(first, second, "alpha-equal terms must share one StructNode");
    assert_eq!(
        arena.distinct_nodes(),
        nodes_after_first,
        "re-interning the same structure must mint no new nodes"
    );

    let key_first = arena.key(first).expect("own node");
    let key_second = arena.key(second).expect("own node");
    assert_eq!(
        key_first, key_second,
        "alpha-equal terms must have byte-identical content keys"
    );
    // The key really is the DAG's netstring fold, not an opaque placeholder.
    assert!(
        key_first.as_str().starts_with("BIND"),
        "a binder's content key is BIND-tagged, got {key_first}"
    );
    assert_eq!(key_first.to_string(), key_first.as_str());
}

/// A binder differing ONLY in its declared sort child is a DIFFERENT term, with a
/// different key — the negative half of the alpha check (without it, a key that ignored
/// sorts would pass the test above vacuously).
#[test]
fn facade_binder_differing_in_sort_is_distinct() {
    let mut arena = TermArena::new();
    let forall = arena.intern_leaf(iri("https://example.org/forall"));
    let body = arena.intern_bound(0, 0);
    let sort_a = arena.intern_leaf(iri("https://example.org/A"));
    let sort_b = arena.intern_leaf(iri("https://example.org/B"));

    let binder_a = arena
        .intern_binder(forall, &[sort_a], body)
        .expect("own nodes");
    let binder_b = arena
        .intern_binder(forall, &[sort_b], body)
        .expect("own nodes");

    assert_ne!(binder_a, binder_b, "distinct sorts ⇒ distinct nodes");
    assert_ne!(
        arena.key(binder_a).expect("own node"),
        arena.key(binder_b).expect("own node"),
        "distinct sorts ⇒ distinct content keys"
    );
}

/// **The interning-demonstrability discharge.** Interning one normalized subexpression
/// `N` times leaves `distinct_nodes` INVARIANT in `N` while `intern_calls` grows with it:
/// fact count grows with distinct structure, not with textual repetition.
///
/// Asserted over four values of `N` so it is a TREND, not a point — a single `N` would be
/// satisfied by any constant.
#[test]
fn facade_distinct_nodes_is_invariant_across_repetition_counts() {
    const REPETITIONS: [u64; 4] = [1, 2, 5, 17];
    const NODES_PER_SUBEXPRESSION: u64 = 6;
    const CALLS_PER_SUBEXPRESSION: u64 = 6;

    let mut observed: Vec<(u64, u64, u64)> = Vec::new();
    for &n in &REPETITIONS {
        // A fresh arena per N: the measurement is over the SCOPED delta, so the mark is
        // taken on an arena that has never seen this structure.
        let mut arena = TermArena::new();
        let before = arena.snapshot();
        for _ in 0..n {
            intern_subexpression(&mut arena);
        }
        let delta = before.delta_to(&arena);
        observed.push((n, delta.distinct_nodes, delta.intern_calls));
    }

    for &(n, distinct_nodes, intern_calls) in &observed {
        assert_eq!(
            distinct_nodes, NODES_PER_SUBEXPRESSION,
            "distinct_nodes must be INVARIANT in the repetition count (N = {n}): {observed:?}"
        );
        assert_eq!(
            intern_calls,
            CALLS_PER_SUBEXPRESSION * n,
            "intern_calls must grow with the repetition count (N = {n}): {observed:?}"
        );
    }

    // The trend is strictly monotone in N on the calls axis and constant on the nodes
    // axis — stated over the whole series, not just per point.
    for pair in observed.windows(2) {
        let (n_lo, nodes_lo, calls_lo) = pair[0];
        let (n_hi, nodes_hi, calls_hi) = pair[1];
        assert!(n_lo < n_hi, "test corpus must be ordered by N");
        assert_eq!(nodes_lo, nodes_hi, "nodes axis must be flat: {observed:?}");
        assert!(
            calls_lo < calls_hi,
            "calls axis must be strictly increasing: {observed:?}"
        );
    }
}

/// The SAME invariance holds when the repetitions land in ONE arena rather than a fresh
/// one per N: the second and later lifts mint nothing, and the delta says so.
#[test]
fn facade_repeated_lifts_into_one_arena_mint_nothing_new() {
    let mut arena = TermArena::new();
    let first = arena.snapshot();
    intern_subexpression(&mut arena);
    let first_delta = first.delta_to(&arena);
    assert_eq!(first_delta.distinct_nodes, 6);
    assert_eq!(first_delta.intern_calls, 6);

    for _ in 0..3 {
        let mark = arena.snapshot();
        intern_subexpression(&mut arena);
        let delta = mark.delta_to(&arena);
        assert_eq!(
            delta.distinct_nodes, 0,
            "a repeat lift into a warm arena mints NO new nodes"
        );
        assert_eq!(
            delta.intern_calls, 6,
            "…while still doing the interning work"
        );
    }
}

/// **The scoped-snapshot guarantee.** Two independent lifts in one process report
/// INDEPENDENT deltas — neither a global counter nor a shared arena could produce this.
///
/// Covers both readings of "independent": two separate arenas, and two disjoint scopes on
/// one arena.
#[test]
fn facade_two_lifts_in_one_process_report_independent_deltas() {
    // (a) Two separate arenas, interleaved so a process-global counter would blend them.
    let mut left = TermArena::new();
    let mut right = TermArena::new();
    let left_mark = left.snapshot();
    let right_mark = right.snapshot();

    intern_subexpression(&mut left);
    // `right` does strictly less work, and a different shape.
    let r_leaf = right.intern_leaf(iri("https://example.org/q"));
    right.intern_app(r_leaf, &[]).expect("own nodes");
    intern_subexpression(&mut left);

    let left_delta = left_mark.delta_to(&left);
    let right_delta = right_mark.delta_to(&right);

    assert_eq!(left_delta.distinct_nodes, 6);
    assert_eq!(left_delta.intern_calls, 12);
    assert_eq!(right_delta.distinct_nodes, 2);
    assert_eq!(right_delta.intern_calls, 2);
    assert_ne!(
        left_delta, right_delta,
        "two arenas in one process must not share a counter"
    );

    // (b) Two disjoint scopes on ONE arena: the second scope sees only its own work.
    let mut arena = TermArena::new();
    let scope_one = arena.snapshot();
    intern_subexpression(&mut arena);
    let one = scope_one.delta_to(&arena);

    let scope_two = arena.snapshot();
    let extra = arena.intern_leaf(iri("https://example.org/z"));
    arena.intern_app(extra, &[]).expect("own nodes");
    let two = scope_two.delta_to(&arena);

    assert_eq!(one.distinct_nodes, 6);
    assert_eq!(one.intern_calls, 6);
    assert_eq!(
        two.distinct_nodes, 2,
        "the second scope must not re-report the first scope's nodes"
    );
    assert_eq!(
        two.intern_calls, 2,
        "the second scope must not re-report the first scope's calls"
    );
}

/// A `StructNode` minted by one arena is REJECTED by another — the brand check, not a
/// bounds check, is what makes membership an identity test.
///
/// The setup is adversarial: both arenas intern the same leaf FIRST, so the foreign
/// handle's numeric slot is genuinely in range on the target arena. An index-range-only
/// guard would accept it.
#[test]
fn facade_foreign_arena_node_is_rejected() {
    let mut home = TermArena::new();
    let mut away = TermArena::new();

    let home_node = home.intern_leaf(iri("https://example.org/a"));
    let away_node = away.intern_leaf(iri("https://example.org/a"));

    assert!(home.contains(home_node), "own node is a member");
    assert!(
        !home.contains(away_node),
        "a foreign node — in-bounds, same content, carrying its OWN correct brand — must \
         NOT be a member of a different arena"
    );

    // Every façade operation that consumes a handle refuses the foreign one.
    home.key(away_node)
        .expect_err("key must refuse a foreign node");
    home.intern_app(away_node, &[])
        .expect_err("intern_app must refuse a foreign operator");
    home.intern_app(home_node, &[away_node])
        .expect_err("intern_app must refuse a foreign argument");
    home.intern_binder(away_node, &[home_node], home_node)
        .expect_err("intern_binder must refuse a foreign binder symbol");
    home.intern_binder(home_node, &[away_node], home_node)
        .expect_err("intern_binder must refuse a foreign sort");
    home.intern_binder(home_node, &[home_node], away_node)
        .expect_err("intern_binder must refuse a foreign body");

    // And the refusal is total: nothing was interned by the refused calls.
    assert_eq!(home.distinct_nodes(), 1);
}

/// Two arenas that intern the SAME structure agree on the `ContentKey` (it is
/// arena-independent) while disagreeing on handle membership (a handle is not).
#[test]
fn facade_content_key_is_arena_independent_but_handles_are_not() {
    let mut left = TermArena::new();
    let mut right = TermArena::new();
    let l = build_forall_p(&mut left);
    let r = build_forall_p(&mut right);

    let left_key = left.key(l).expect("own node");
    let right_key = right.key(r).expect("own node");
    assert_eq!(
        left_key, right_key,
        "the content key crosses arena boundaries; the handle does not"
    );
    assert!(!left.contains(r));
    assert!(!right.contains(l));

    // Ord/Eq/Hash are live on the key, so it can key a map or sort a ledger.
    let mut keys = vec![
        ContentKey::new("b".to_owned()),
        left_key.clone(),
        ContentKey::new("a".to_owned()),
    ];
    keys.sort();
    assert_eq!(
        keys[0].as_str(),
        "BIND".to_owned() + &left_key.as_str()[4..]
    );
    let set: std::collections::HashSet<ContentKey> = keys.into_iter().collect();
    assert!(set.contains(&right_key));
}

/// A fresh metavariable is identity-bearing: two mints are two nodes with two keys, and
/// the façade never hands back the dense `MetaId`.
#[test]
fn facade_fresh_meta_is_identity_bearing() {
    let mut arena = TermArena::new();
    let m0 = arena.fresh_meta();
    let m1 = arena.fresh_meta();
    assert_ne!(m0, m1, "each fresh_meta mints a distinct metavariable node");
    assert_ne!(
        arena.key(m0).expect("own node"),
        arena.key(m1).expect("own node"),
        "distinct metavariables have distinct content keys"
    );
    assert_eq!(arena.distinct_nodes(), 2);
}

/// A free variable named `x` and a leaf whose display is `x` are DIFFERENT terms — the
/// `free_` framing in the key fold is what keeps them apart.
#[test]
fn facade_free_variable_is_distinct_from_a_same_named_leaf() {
    let mut arena = TermArena::new();
    let free = arena.intern_free(TermValue::simple_literal("x"));
    let leaf = arena.intern_leaf(TermValue::simple_literal("x"));
    assert_ne!(free, leaf);
    assert_ne!(
        arena.key(free).expect("own node"),
        arena.key(leaf).expect("own node")
    );
}
