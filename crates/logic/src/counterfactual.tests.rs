// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::entrenchment::OVERRIDES;
use crate::query_ir::parse_query_program;

const HORN: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";
const BASE: &str = "http://world/base";
const CF: &str = "http://world/cf";

fn plain_program() -> QProgram {
    parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             ?- ex:p(ex:s, Y).\n",
    )
    .unwrap()
}

#[test]
fn incremental_goal_hash_is_typed_and_framed() {
    let goal = |pred: &str, term: QTerm| crate::query_ir::QGoal {
        atoms: vec![QAtom {
            pred: pred.to_owned(),
            args: vec![term],
        }],
    };

    assert_ne!(
        hash_goal(&goal("https://ex/p", QTerm::Const("1".to_owned()))),
        hash_goal(&goal("https://ex/p", QTerm::Var("1".to_owned()))),
    );
    assert_ne!(
        hash_goal(&goal("https://ex/p", QTerm::Const("1".to_owned()))),
        hash_goal(&goal("https://ex/p", QTerm::Num(1))),
    );
    assert_ne!(
        hash_goal(&goal("https://ex/a", QTerm::Const("bc".to_owned()))),
        hash_goal(&goal("https://ex/ab", QTerm::Const("c".to_owned()))),
        "length framing prevents adjacent-field boundary aliases",
    );
}

#[test]
fn incremental_base_key_canonicalizes_profile_aliases() {
    let program = plain_program();
    let facts = vec![(
        "https://ex/s".to_owned(),
        "https://ex/p".to_owned(),
        "https://ex/o".to_owned(),
    )];
    assert_eq!(
        incremental_base_key(&facts, &program, HORN),
        incremental_base_key(&facts, &program, "logic:PositiveHornProfile"),
    );
    assert_eq!(
        incremental_base_key(&facts, &program, HORN),
        incremental_base_key(&facts, &program, "PositiveHornProfile"),
    );
}

#[test]
fn is_counterfactual_detects_declaration() {
    assert!(!is_counterfactual(&plain_program()));
}

#[test]
fn construct_and_resolve_rejects_plain_program() {
    let store = WorldStore::new();
    let err = construct_and_resolve(&store, &plain_program(), HORN, &Budget::default(), 4, None)
        .unwrap_err();
    assert!(err.message().contains("non-counterfactual"), "got: {err}");
}

// ── AC-1: a counterfactual query yields the expected consequent ───────────
//
// Base: status(server, up). Antecedent overwrites it to status(server, down).
// Rule: alert(X, fired) :- status(X, down). Goal: alert(server, Z) -> {fired}.
#[test]
fn consequent_is_yielded_after_overwrite() {
    let store = WorldStore::new();
    store.insert_quad(
        BASE,
        "https://ex/server",
        "https://ex/status",
        "https://ex/up",
    );
    let prog = parse_query_program(
        ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:status(ex:server, ex:down)).\n\
             ex:alert(X, ex:fired) :- ex:status(X, ex:down).\n\
             ?- ex:alert(ex:server, Z).\n",
    )
    .unwrap();
    let ans = construct_and_resolve(&store, &prog, HORN, &Budget::default(), 4, None).unwrap();
    assert_eq!(ans.status_str(), "ok", "ans: {ans:?}");
    assert_eq!(ans.bindings.len(), 1, "exactly one consequent: {ans:?}");
    assert_eq!(ans.bindings[0]["Z"], "<https://ex/fired>");
}

#[test]
fn functional_revision_is_identical_for_incremental_and_scratch_paths() {
    let base = vec![
        (
            "https://ex/server".to_owned(),
            "https://ex/status".to_owned(),
            "https://ex/up".to_owned(),
        ),
        (
            "https://ex/other".to_owned(),
            "https://ex/kept".to_owned(),
            "https://ex/value".to_owned(),
        ),
    ];
    let admitted = vec![(
        "https://ex/server".to_owned(),
        "https://ex/status".to_owned(),
        "https://ex/down".to_owned(),
    )];
    let program = parse_query_program(
        ":- prefix(ex, 'https://ex/').\n\
             ?- ex:status(ex:server, Z).\n",
    )
    .unwrap();
    let budget = Budget::default();
    let mut cache = CfCache::new();
    let incremental = incremental_base_session(&mut cache, &base, &program, HORN, &budget)
        .unwrap()
        .expect("positive query admits a fixed-program incremental session");

    let scratch = resolve_in_world(&base, &admitted, &program, HORN, &budget, CF, None)
        .expect("scratch revision");
    let maintained = resolve_in_world(
        &base,
        &admitted,
        &program,
        HORN,
        &budget,
        CF,
        Some(&incremental),
    )
    .expect("incremental revision");

    assert_eq!(maintained, scratch);
    assert_eq!(
        maintained.0,
        BTreeSet::from([BTreeMap::from([(
            "Z".to_owned(),
            "<https://ex/down>".to_owned(),
        )])])
    );
}

// ── Native production path: recursion resolves inside the constructed world ─
//
// Each closest world's goal is resolved via `dispatch_query` (native magic-sets
// first), so a counterfactual whose consequent needs RECURSION exercises the
// promoted native path end-to-end on the counterfactual production surface: the
// assumed edge a→b joins the base chain b→c→d, so `reach(a, Y)` closes over
// {b, c, d} inside the constructed world. The reference comparison for this
// fragment lives in `physical::parity`.
#[test]
fn counterfactual_native_resolves_recursion_in_constructed_world() {
    let store = WorldStore::new();
    store.insert_quad(BASE, "https://ex/b", "https://ex/edge", "https://ex/c");
    store.insert_quad(BASE, "https://ex/c", "https://ex/edge", "https://ex/d");
    let prog = parse_query_program(
        ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:edge(ex:a, ex:b)).\n\
             ex:reach(X, Y) :- ex:edge(X, Y).\n\
             ex:reach(X, Y) :- ex:edge(X, Z), ex:reach(Z, Y).\n\
             ?- ex:reach(ex:a, Y).\n",
    )
    .unwrap();
    let mut cache = CfCache::new();
    let ans =
        construct_and_resolve_cached(&store, &prog, HORN, &Budget::default(), 4, &mut cache, None)
            .unwrap();
    assert_eq!(ans.status_str(), "ok", "ans: {ans:?}");
    let zs: BTreeSet<&str> = ans.bindings.iter().map(|b| b["Y"].as_str()).collect();
    assert_eq!(
        zs,
        BTreeSet::from(["<https://ex/b>", "<https://ex/c>", "<https://ex/d>"]),
        "native recursion inside the constructed counterfactual world: {ans:?}"
    );
    assert_eq!(
        cache.incremental_updates(),
        1,
        "the recursive counterfactual must apply one signed revision to the cached base"
    );
}

// ── AC-2: no leakage — the base store is never mutated ────────────────────
#[test]
fn no_leakage_base_store_unchanged() {
    let store = WorldStore::new();
    store.insert_quad(
        BASE,
        "https://ex/server",
        "https://ex/status",
        "https://ex/up",
    );
    let before = store.quads_in_world(BASE);
    let prog = parse_query_program(
        ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:status(ex:server, ex:down)).\n\
             ?- ex:status(ex:server, Z).\n",
    )
    .unwrap();
    let _ = construct_and_resolve(&store, &prog, HORN, &Budget::default(), 4, None).unwrap();
    // The base world still has exactly its original fact (status up), and the
    // constructed world W_cf never appears in the base store.
    let after = store.quads_in_world(BASE);
    assert_eq!(before, after, "base world must be unchanged");
    assert!(
        !store.worlds().contains(&CF.to_owned()),
        "W_cf must not leak into the base store: {:?}",
        store.worlds()
    );
    // And inside W_cf the antecedent value holds, not the base value.
    let prog2 = parse_query_program(
        ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:status(ex:server, ex:down)).\n\
             ?- ex:status(ex:server, Z).\n",
    )
    .unwrap();
    let ans = construct_and_resolve(&store, &prog2, HORN, &Budget::default(), 4, None).unwrap();
    assert_eq!(ans.bindings.len(), 1);
    assert_eq!(
        ans.bindings[0]["Z"], "<https://ex/down>",
        "overwrite applied in W_cf"
    );
}

// ── AC-3a: deterministic revision yields exactly one world ────────────────
//
// Over-determined antecedent {primary, backup} with primary ≻ backup -> primary wins.
#[test]
fn comparable_over_determination_is_deterministic() {
    let store = WorldStore::new();
    store.insert_quad(BASE, "https://ex/primary", OVERRIDES, "https://ex/backup");
    let prog = parse_query_program(
        ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:route(ex:traffic, ex:primary)).\n\
             :- assume(ex:route(ex:traffic, ex:backup)).\n\
             ?- ex:route(ex:traffic, Z).\n",
    )
    .unwrap();
    let ans = construct_and_resolve(&store, &prog, HORN, &Budget::default(), 4, None).unwrap();
    assert_eq!(ans.status_str(), "ok");
    assert_eq!(ans.bindings.len(), 1, "exactly one routed value: {ans:?}");
    assert_eq!(
        ans.bindings[0]["Z"], "<https://ex/primary>",
        "the more-entrenched value wins"
    );
}

// ── AC-3b: a genuine (incomparable) tie returns unknown ───────────────────
#[test]
fn incomparable_over_determination_is_unknown() {
    // No entrenchment edge between blue and green -> incomparable.
    let store = WorldStore::new();
    store.insert_quad(BASE, "https://ex/seed", "https://ex/p", "https://ex/o");
    let prog = parse_query_program(
        ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:flag(ex:x, ex:blue)).\n\
             :- assume(ex:flag(ex:x, ex:green)).\n\
             ?- ex:flag(ex:x, Z).\n",
    )
    .unwrap();
    let ans = construct_and_resolve(&store, &prog, HORN, &Budget::default(), 4, None).unwrap();
    assert_eq!(ans.status_str(), "unknown", "ambiguous tie must be unknown");
    assert!(ans.bindings.is_empty());
}

// ── depth budget trip ─────────────────────────────────────────────────────
#[test]
fn depth_budget_zero_is_incomplete() {
    let store = WorldStore::new();
    store.insert_quad(BASE, "https://ex/s", "https://ex/p", "https://ex/o");
    let prog = parse_query_program(
        ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:p2(ex:s, ex:o2)).\n\
             ?- ex:p(ex:s, Z).\n",
    )
    .unwrap();
    let ans = construct_and_resolve(&store, &prog, HORN, &Budget::default(), 0, None).unwrap();
    assert_eq!(ans.status_str(), "incomplete");
}

// ── memoization: identical key -> cache hit, identical answer ─────────────
#[test]
fn memoization_hit_on_identical_construction() {
    let store = WorldStore::new();
    store.insert_quad(
        BASE,
        "https://ex/server",
        "https://ex/status",
        "https://ex/up",
    );
    let prog = parse_query_program(
        ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:status(ex:server, ex:down)).\n\
             ?- ex:status(ex:server, Z).\n",
    )
    .unwrap();
    let mut cache = CfCache::new();
    let a =
        construct_and_resolve_cached(&store, &prog, HORN, &Budget::default(), 4, &mut cache, None)
            .unwrap();
    let b =
        construct_and_resolve_cached(&store, &prog, HORN, &Budget::default(), 4, &mut cache, None)
            .unwrap();
    assert_eq!(a, b, "identical construction must yield identical answers");
    assert_eq!(cache.misses(), 1, "first call is a miss");
    assert_eq!(cache.hits(), 1, "second identical call is a hit");
}

#[test]
fn cf_status_serialization() {
    assert_eq!(CfStatus::Ok.as_str(), "ok");
    assert_eq!(CfStatus::Unknown.as_str(), "unknown");
    assert_eq!(CfStatus::Incomplete.as_str(), "incomplete");
}

#[test]
fn cf_status_string_round_trips_every_cfstatus() {
    // The typed ReasoningResult is a lossless carrier: projecting it back
    // reproduces the byte-pinned conformance string exactly, so the
    // cross-engine corpus is unchanged.
    for s in [
        CfStatus::Ok,
        CfStatus::Partial,
        CfStatus::Exhausted,
        CfStatus::Unknown,
        CfStatus::Incomplete,
    ] {
        let r = cf_result(
            s,
            "http://gmeow.example/w",
            crate::result::ResultPayload::Bindings(vec![]),
        );
        assert_eq!(cf_status_string(&r), s.as_str(), "cf round-trip for {s:?}");
        assert!(
            r.validate().is_ok(),
            "cf_result must be a valid result: {s:?}"
        );
    }
}

#[test]
fn cf_unknown_is_distinct_typed_state_from_prob_unknown() {
    use crate::result::{CompletenessStatus, EvaluationStatus, InformationState};
    // A cf revision tie: completed run, completeness=unknown, no verdict.
    let r = cf_result(
        CfStatus::Unknown,
        "http://gmeow.example/w",
        crate::result::ResultPayload::Bindings(vec![]),
    );
    assert_eq!(r.evaluation, EvaluationStatus::Completed);
    assert_eq!(r.completeness, CompletenessStatus::Unknown);
    assert_eq!(r.information, InformationState::Undetermined);
    // ...which differs from prob's no-model unknown (unsupported + not-evaluated),
    // even though both project to the same "unknown" corpus string.
    assert_eq!(cf_status_string(&r), "unknown");
}

// ── Lewis multi-world profile (opt-in, budget-capped) ─────────────────────

const LEWIS_SKEPTICAL: &str = "https://blackcatinformatics.ca/logic/LewisSkepticalProfile";
const LEWIS_CREDULOUS: &str = "https://blackcatinformatics.ca/logic/LewisCredulousProfile";

fn two_world_program() -> QProgram {
    // {blue, green} are incomparable -> two closest worlds under Lewis.
    parse_query_program(
        ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:flag(ex:x, ex:blue)).\n\
             :- assume(ex:flag(ex:x, ex:green)).\n\
             ?- ex:flag(ex:x, Z).\n",
    )
    .unwrap()
}

fn seeded_base() -> WorldStore {
    let store = WorldStore::new();
    store.insert_quad(BASE, "https://ex/seed", "https://ex/p", "https://ex/o");
    store
}

#[test]
fn lewis_skeptical_intersects_closest_worlds() {
    let store = seeded_base();
    let ans = construct_and_resolve(
        &store,
        &two_world_program(),
        LEWIS_SKEPTICAL,
        &Budget::default(),
        4,
        None,
    )
    .unwrap();
    assert_eq!(ans.status_str(), "ok");
    // Z=blue holds only in the blue-world, Z=green only in the green-world:
    // the intersection is empty.
    assert!(
        ans.bindings.is_empty(),
        "skeptical: no binding holds in every closest world: {ans:?}"
    );
}

#[test]
fn lewis_credulous_unions_closest_worlds() {
    let store = seeded_base();
    let ans = construct_and_resolve(
        &store,
        &two_world_program(),
        LEWIS_CREDULOUS,
        &Budget::default(),
        4,
        None,
    )
    .unwrap();
    assert_eq!(ans.status_str(), "ok");
    // Union over both closest worlds: Z in {blue, green}.
    let zs: BTreeSet<&str> = ans.bindings.iter().map(|b| b["Z"].as_str()).collect();
    assert_eq!(
        zs,
        BTreeSet::from(["<https://ex/blue>", "<https://ex/green>"]),
        "credulous: union of both closest worlds: {ans:?}"
    );
}

#[test]
fn lewis_branch_budget_trips_to_incomplete() {
    // 5 independent binary-incomparable slots -> 2^5 = 32 closest worlds,
    // past DEFAULT_BRANCH_BUDGET (16) -> Incomplete.
    let store = seeded_base();
    let prog = parse_query_program(
        ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:a(ex:s1, ex:v1)).\n\
             :- assume(ex:a(ex:s1, ex:w1)).\n\
             :- assume(ex:a(ex:s2, ex:v2)).\n\
             :- assume(ex:a(ex:s2, ex:w2)).\n\
             :- assume(ex:a(ex:s3, ex:v3)).\n\
             :- assume(ex:a(ex:s3, ex:w3)).\n\
             :- assume(ex:a(ex:s4, ex:v4)).\n\
             :- assume(ex:a(ex:s4, ex:w4)).\n\
             :- assume(ex:a(ex:s5, ex:v5)).\n\
             :- assume(ex:a(ex:s5, ex:w5)).\n\
             ?- ex:a(ex:s1, Z).\n",
    )
    .unwrap();
    let ans =
        construct_and_resolve(&store, &prog, LEWIS_SKEPTICAL, &Budget::default(), 4, None).unwrap();
    assert_eq!(
        ans.status_str(),
        "incomplete",
        "32 worlds exceeds the branch budget"
    );
}

#[test]
fn lewis_does_not_change_deterministic_single_world() {
    // A single-valued antecedent is one world even under a Lewis profile.
    let store = WorldStore::new();
    store.insert_quad(
        BASE,
        "https://ex/server",
        "https://ex/status",
        "https://ex/up",
    );
    let prog = parse_query_program(
        ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:status(ex:server, ex:down)).\n\
             ?- ex:status(ex:server, Z).\n",
    )
    .unwrap();
    let ans =
        construct_and_resolve(&store, &prog, LEWIS_CREDULOUS, &Budget::default(), 4, None).unwrap();
    assert_eq!(ans.status_str(), "ok");
    assert_eq!(ans.bindings.len(), 1);
    assert_eq!(ans.bindings[0]["Z"], "<https://ex/down>");
}

// ── row_schema facet: declared schema is validated and attached ────────────

/// A matching schema: the result binds IRI-valued `Z`; the schema declares
/// `Required Iri` for `Z`. Schema is attached and `row_schema.is_some()`.
#[test]
fn declared_schema_matching_attaches_row_schema() {
    use gmeow_logic_compile::result_shape::{
        ColumnBinding, ColumnKind, ResultColumn, ResultShape, RowCardinality,
    };

    let store = WorldStore::new();
    store.insert_quad(
        BASE,
        "https://ex/server",
        "https://ex/status",
        "https://ex/up",
    );
    let prog = parse_query_program(
        ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:status(ex:server, ex:down)).\n\
             ?- ex:status(ex:server, Z).\n",
    )
    .unwrap();

    // Declare: one Required IRI column `Z`, any number of rows.
    let schema = ResultShape::new(
        vec![ResultColumn {
            var: "Z".to_owned(),
            kind: ColumnKind::Iri,
            binding: ColumnBinding::Required,
        }],
        RowCardinality::Contains,
    );

    let ans =
        construct_and_resolve(&store, &prog, HORN, &Budget::default(), 4, Some(schema)).unwrap();
    assert_eq!(ans.status_str(), "ok");
    assert_eq!(ans.bindings.len(), 1);
    assert!(
        ans.result.row_schema.is_some(),
        "row_schema must be attached when a declared schema matches"
    );
}

/// A mismatching schema: the result binds IRI-valued `Z`; the schema declares
/// `Required BlankNode` for `Z`. Must return Err (ContractViolation propagated).
#[test]
fn declared_schema_mismatch_returns_err() {
    use gmeow_logic_compile::result_shape::{
        ColumnBinding, ColumnKind, ResultColumn, ResultShape, RowCardinality,
    };

    let store = WorldStore::new();
    store.insert_quad(
        BASE,
        "https://ex/server",
        "https://ex/status",
        "https://ex/up",
    );
    let prog = parse_query_program(
        ":- prefix(ex, 'https://ex/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:status(ex:server, ex:down)).\n\
             ?- ex:status(ex:server, Z).\n",
    )
    .unwrap();

    // Declare: `Z` must be a blank-node — but the binding is an IRI → mismatch.
    let schema = ResultShape::new(
        vec![ResultColumn {
            var: "Z".to_owned(),
            kind: ColumnKind::BlankNode,
            binding: ColumnBinding::Required,
        }],
        RowCardinality::Contains,
    );

    let err = construct_and_resolve(&store, &prog, HORN, &Budget::default(), 4, Some(schema))
        .unwrap_err();
    assert!(
        err.message().contains("result-shape violation"),
        "ContractViolation must be propagated as Err: {err}"
    );
}
