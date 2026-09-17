// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::dispatch::dispatch_query;
use crate::query_ir::parse_query_program;
use crate::seam::WorldFactSnapshot;
use crate::store::WorldStore;

const BASE: &str = "https://example.org/";
const HORN_PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";
const WORLD: &str = "http://logic.test/world/gate";

fn cut_program() -> crate::query_ir::QProgram {
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:first(X, Y) :- ex:edge(X, Y), !.\n\
             ?- ex:first(ex:a, Y).\n"
    );
    parse_query_program(&src).unwrap()
}

fn no_cut_program() -> crate::query_ir::QProgram {
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:reach(X, Y) :- ex:edge(X, Y).\n\
             ?- ex:reach(ex:a, Y).\n"
    );
    parse_query_program(&src).unwrap()
}

// ── has_cut ────────────────────────────────────────────────────────────────

#[test]
fn has_cut_detects_cut_in_body() {
    assert!(
        has_cut(&cut_program()),
        "cut program must report has_cut=true"
    );
}

#[test]
fn has_cut_false_when_no_cut() {
    assert!(
        !has_cut(&no_cut_program()),
        "non-cut program must report has_cut=false"
    );
}

// ── retired cut syntax ────────────────────────────────────────────────────

#[test]
fn cut_is_rejected_under_every_profile() {
    let prog = cut_program();
    let error = reject_cut(&prog).expect_err("cut must be retired unconditionally");
    assert!(error.message().contains("retired cut syntax"));
}

#[test]
fn no_cut_any_profile_is_ok() {
    let prog = no_cut_program();
    assert!(reject_cut(&prog).is_ok());
}

// ── Lewis profile recognition ──────────────────────────────────────

#[test]
fn lewis_mode_recognizes_full_iri_and_short_and_prefixed() {
    assert_eq!(
        lewis_mode(LEWIS_SKEPTICAL_PROFILE),
        Some(LewisMode::Skeptical)
    );
    assert_eq!(
        lewis_mode("LewisSkepticalProfile"),
        Some(LewisMode::Skeptical)
    );
    assert_eq!(
        lewis_mode("logic:LewisCredulousProfile"),
        Some(LewisMode::Credulous)
    );
    assert_eq!(
        lewis_mode(LEWIS_CREDULOUS_PROFILE),
        Some(LewisMode::Credulous)
    );
}

#[test]
fn lewis_mode_default_profiles_are_none() {
    assert_eq!(lewis_mode(HORN_PROFILE), None);
    assert_eq!(lewis_mode("PositiveHornProfile"), None);
    assert_eq!(lewis_mode(""), None);
}

// ── Evolution-facet recognition (logic:EvolutionMode) ─────────────────────

#[test]
fn evolution_mode_recognizes_full_iri_short_and_prefixed() {
    assert_eq!(
        evolution_mode_local(STATIC_EVOLUTION),
        Some("StaticEvolution")
    );
    assert_eq!(
        evolution_mode_local("StaticEvolution"),
        Some("StaticEvolution")
    );
    assert_eq!(
        evolution_mode_local("logic:StaticEvolution"),
        Some("StaticEvolution")
    );
    assert_eq!(
        evolution_mode_local(STATE_TRANSITION_EVOLUTION),
        Some("StateTransitionEvolution")
    );
    assert_eq!(
        evolution_mode_local("logic:StateTransitionEvolution"),
        Some("StateTransitionEvolution")
    );
    assert_eq!(
        evolution_mode_local(TRANSACTION_PATH_EVOLUTION),
        Some("TransactionPathEvolution")
    );
    assert_eq!(
        evolution_mode_local("TransactionPathEvolution"),
        Some("TransactionPathEvolution")
    );
}

#[test]
fn evolution_mode_unknown_and_empty_are_none() {
    assert_eq!(evolution_mode_local(""), None);
    assert_eq!(evolution_mode_local("NotAnEvolutionMode"), None);
    assert_eq!(evolution_mode_local("logic:PositiveHornProfile"), None);
}

// ── No-write firewall ──────────────────────────────────────────────────────
//
// Reject a cut program through production dispatch and verify the store remains
// unchanged.

#[test]
fn rejected_cut_program_leaves_store_unchanged() {
    let store = WorldStore::new();
    store.insert_quad(
        WORLD,
        &format!("{BASE}a"),
        &format!("{BASE}edge"),
        &format!("{BASE}b"),
    );
    store.insert_quad(
        WORLD,
        &format!("{BASE}a"),
        &format!("{BASE}edge"),
        &format!("{BASE}c"),
    );

    let before = store.quads_in_world(WORLD).len();

    let foreign = WorldFactSnapshot::from_world(&store, WORLD, PROCEDURAL_PROLOG_PROFILE)
        .expect("from_world must succeed");

    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             ex:first(X, Y) :- ex:edge(X, Y), !.\n\
             ?- ex:first(ex:a, Y).\n"
    );
    let prog = parse_query_program(&src).unwrap();

    let error = dispatch_query(
        &foreign,
        WORLD,
        &prog,
        PROCEDURAL_PROLOG_PROFILE,
        &crate::query_ir::Budget::default(),
    )
    .expect_err("cut must be rejected even under the procedural builtin profile");
    assert!(error.message().contains("retired cut syntax"));

    let after = store.quads_in_world(WORLD).len();
    assert_eq!(
        before, after,
        "store quad count must be unchanged after the rejected query"
    );
}
