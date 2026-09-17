// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const W: &str = "http://world/base";
const G: &str = "https://blackcatinformatics.ca/gmeow/";

fn ex(local: &str) -> String {
    format!("https://example.org/{local}")
}
fn gm(local: &str) -> String {
    format!("{G}{local}")
}

// ── overrides axis ─────────────────────────────────────────────────────────

#[test]
fn overrides_axis_orders_norms() {
    let store = WorldStore::new();
    store.insert_quad(W, &ex("constitution"), OVERRIDES, &ex("bylaw"));
    let e = Entrenchment::read_from_world(&store, W).unwrap();
    assert_eq!(
        e.compare(&ex("constitution"), &ex("bylaw")),
        Some(Ordering::Greater),
        "the overriding norm is more entrenched"
    );
    assert_eq!(
        e.compare(&ex("bylaw"), &ex("constitution")),
        Some(Ordering::Less)
    );
}

// ── strongerThan axis (levels) ─────────────────────────────────────────────

#[test]
fn stronger_than_axis_orders_levels() {
    let store = WorldStore::new();
    store.insert_quad(
        W,
        &gm("authorityAbsolute"),
        STRONGER_THAN,
        &gm("authorityHigh"),
    );
    store.insert_quad(
        W,
        &gm("authorityHigh"),
        STRONGER_THAN,
        &gm("authorityMedium"),
    );
    let e = Entrenchment::read_from_world(&store, W).unwrap();
    // Transitive: absolute ≻ medium even though only the chain is asserted.
    assert_eq!(
        e.compare(&gm("authorityAbsolute"), &gm("authorityMedium")),
        Some(Ordering::Greater)
    );
}

// ── authority-level inheritance ────────────────────────────────────────────

#[test]
fn authority_level_inheritance_orders_norms_by_level() {
    let store = WorldStore::new();
    store.insert_quad(
        W,
        &gm("authorityAbsolute"),
        STRONGER_THAN,
        &gm("authorityHigh"),
    );
    store.insert_quad(
        W,
        &ex("treaty"),
        HAS_AUTHORITY_LEVEL,
        &gm("authorityAbsolute"),
    );
    store.insert_quad(W, &ex("policy"), HAS_AUTHORITY_LEVEL, &gm("authorityHigh"));
    let e = Entrenchment::read_from_world(&store, W).unwrap();
    assert_eq!(
        e.compare(&ex("treaty"), &ex("policy")),
        Some(Ordering::Greater),
        "treaty (absolute) outranks policy (high)"
    );
}

#[test]
fn authority_inheritance_uses_strongerthan_only_not_other_edges() {
    // A non-strongerThan edge between two level individuals (here `overrides`)
    // must NOT be read as authority-level precedence: norms carrying those
    // levels stay incomparable unless a `strongerThan` chain links the levels.
    let store = WorldStore::new();
    // overrides edge directly between the *level* individuals (not strongerThan).
    store.insert_quad(W, &gm("authorityHigh"), OVERRIDES, &gm("authorityMedium"));
    store.insert_quad(W, &ex("policy"), HAS_AUTHORITY_LEVEL, &gm("authorityHigh"));
    store.insert_quad(
        W,
        &ex("guideline"),
        HAS_AUTHORITY_LEVEL,
        &gm("authorityMedium"),
    );
    let e = Entrenchment::read_from_world(&store, W).unwrap();
    // The levels themselves are ordered by the direct `overrides` edge …
    assert_eq!(
        e.compare(&gm("authorityHigh"), &gm("authorityMedium")),
        Some(Ordering::Greater)
    );
    // … but that must not synthesize precedence between the *norms*: only a
    // `strongerThan` level order may be inherited, and there is none here.
    assert_eq!(
        e.compare(&ex("policy"), &ex("guideline")),
        None,
        "overrides between levels must not leak into authority-level inheritance"
    );
}

#[test]
fn conflicting_authority_level_is_rejected() {
    let store = WorldStore::new();
    store.insert_quad(
        W,
        &ex("treaty"),
        HAS_AUTHORITY_LEVEL,
        &gm("authorityAbsolute"),
    );
    store.insert_quad(W, &ex("treaty"), HAS_AUTHORITY_LEVEL, &gm("authorityHigh"));
    let err = Entrenchment::read_from_world(&store, W).unwrap_err();
    assert!(
        err.message().contains("conflicting hasAuthorityLevel"),
        "got: {err}"
    );
}

// ── moreSevereThan axis ────────────────────────────────────────────────────

#[test]
fn more_severe_than_axis_orders_levels() {
    let store = WorldStore::new();
    store.insert_quad(W, &ex("catastrophic"), MORE_SEVERE_THAN, &ex("minor"));
    let e = Entrenchment::read_from_world(&store, W).unwrap();
    assert_eq!(
        e.compare(&ex("catastrophic"), &ex("minor")),
        Some(Ordering::Greater)
    );
}

// ── sharpens axis ──────────────────────────────────────────────────────────

#[test]
fn sharpens_axis_orders_standpoints() {
    let store = WorldStore::new();
    store.insert_quad(W, &ex("cityCouncilView"), SHARPENS, &ex("regionalView"));
    let e = Entrenchment::read_from_world(&store, W).unwrap();
    assert_eq!(
        e.compare(&ex("cityCouncilView"), &ex("regionalView")),
        Some(Ordering::Greater),
        "the sharper standpoint is more entrenched"
    );
}

// ── tie / incomparability ──────────────────────────────────────────────────

#[test]
fn incomparable_iris_are_a_tie() {
    let store = WorldStore::new();
    store.insert_quad(W, &ex("a"), OVERRIDES, &ex("b"));
    store.insert_quad(W, &ex("c"), OVERRIDES, &ex("d"));
    let e = Entrenchment::read_from_world(&store, W).unwrap();
    // a and c are in disjoint chains — incomparable.
    assert_eq!(e.compare(&ex("a"), &ex("c")), None);
    assert!(!e.is_total_over([ex("a").as_str(), ex("c").as_str()]));
    assert!(e.is_total_over([ex("a").as_str(), ex("b").as_str()]));
}

#[test]
fn least_entrenched_unique_on_total_chain() {
    let store = WorldStore::new();
    store.insert_quad(W, &ex("a"), OVERRIDES, &ex("b"));
    store.insert_quad(W, &ex("b"), OVERRIDES, &ex("c"));
    let e = Entrenchment::read_from_world(&store, W).unwrap();
    // Among {a, c}, c is strictly least entrenched (a ≻ b ≻ c).
    assert_eq!(
        e.least_entrenched(&[ex("a"), ex("c")]),
        LeastEntrenched::Unique(ex("c"))
    );
}

#[test]
fn least_entrenched_tie_on_incomparable() {
    let store = WorldStore::new();
    store.insert_quad(W, &ex("a"), OVERRIDES, &ex("b"));
    store.insert_quad(W, &ex("c"), OVERRIDES, &ex("d"));
    let e = Entrenchment::read_from_world(&store, W).unwrap();
    // b and d are both minimal but incomparable → tie.
    match e.least_entrenched(&[ex("b"), ex("d")]) {
        LeastEntrenched::Tie(t) => {
            assert_eq!(t, vec![ex("b"), ex("d")]);
        }
        other => panic!("expected Tie, got {other:?}"),
    }
}

#[test]
fn most_entrenched_unique_and_tie() {
    let store = WorldStore::new();
    store.insert_quad(W, &ex("a"), OVERRIDES, &ex("b"));
    let e = Entrenchment::read_from_world(&store, W).unwrap();
    // a ≻ b: the most entrenched is a (unique).
    assert_eq!(
        e.most_entrenched(&[ex("a"), ex("b")]),
        LeastEntrenched::Unique(ex("a"))
    );

    // Two incomparable values → tie for most entrenched.
    let store2 = WorldStore::new();
    store2.insert_quad(W, &ex("a"), OVERRIDES, &ex("b"));
    store2.insert_quad(W, &ex("c"), OVERRIDES, &ex("d"));
    let e2 = Entrenchment::read_from_world(&store2, W).unwrap();
    match e2.most_entrenched(&[ex("a"), ex("c")]) {
        LeastEntrenched::Tie(t) => assert_eq!(t, vec![ex("a"), ex("c")]),
        other => panic!("expected Tie, got {other:?}"),
    }
}

#[test]
fn least_entrenched_empty_and_singleton() {
    let e = Entrenchment::default();
    assert_eq!(e.least_entrenched(&[]), LeastEntrenched::Empty);
    assert_eq!(
        e.least_entrenched(&[ex("solo")]),
        LeastEntrenched::Unique(ex("solo"))
    );
}

// ── cycle detection ────────────────────────────────────────────────────────

#[test]
fn cycle_in_edges_is_rejected() {
    let store = WorldStore::new();
    store.insert_quad(W, &ex("a"), OVERRIDES, &ex("b"));
    store.insert_quad(W, &ex("b"), OVERRIDES, &ex("a"));
    let err = Entrenchment::read_from_world(&store, W).unwrap_err();
    assert!(err.message().contains("cycle"), "got: {err}");
}

// ── hash determinism ───────────────────────────────────────────────────────

#[test]
fn hash_is_deterministic_and_order_sensitive() {
    let store1 = WorldStore::new();
    store1.insert_quad(W, &ex("a"), OVERRIDES, &ex("b"));
    let e1 = Entrenchment::read_from_world(&store1, W).unwrap();

    // Same content, inserted again — identical hash.
    let store2 = WorldStore::new();
    store2.insert_quad(W, &ex("a"), OVERRIDES, &ex("b"));
    let e2 = Entrenchment::read_from_world(&store2, W).unwrap();
    assert_eq!(e1.hash(), e2.hash());

    // Different ordering — different hash.
    let store3 = WorldStore::new();
    store3.insert_quad(W, &ex("b"), OVERRIDES, &ex("a"));
    let e3 = Entrenchment::read_from_world(&store3, W).unwrap();
    assert_ne!(e1.hash(), e3.hash());
}

#[test]
fn empty_world_yields_empty_ordering() {
    let store = WorldStore::new();
    let e = Entrenchment::read_from_world(&store, W).unwrap();
    assert!(e.entities().is_empty());
    assert_eq!(e.compare(&ex("a"), &ex("b")), None);
}
