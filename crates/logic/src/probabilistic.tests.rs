// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::query_ir::parse_query_program;

const PROFILE: &str = "https://blackcatinformatics.ca/logic/ProbabilisticProfile";
const WORLD: &str = "https://example.org/prob/world";
const BASE: &str = "https://example.org/prob/";

fn const_iri(local: &str) -> String {
    format!("<{BASE}{local}>")
}

/// Find the marginal for a single-variable binding `var = <BASE+local>`.
fn marginal_for(ans: &ProbAnswer, var: &str, local: &str) -> Option<f64> {
    ans.bindings
        .iter()
        .find(|b| b.vars.get(var) == Some(&const_iri(local)))
        .map(|b| b.probability)
}

// ── AC1: independent marginals ────────────────────────────────────────────

#[test]
fn independent_or_marginal_is_noisy_or() {
    // wet :- rain.  wet :- sprinkler.   rain=0.5 (indep), sprinkler=0.5 (indep).
    // P(wet) = 1 - (1-0.5)(1-0.5) = 0.75.
    let store = WorldStore::new();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             :- probability_model(full_independence).\n\
             :- probability(ex:rain(ex:today, ex:true), 0.5).\n\
             :- probability(ex:sprinkler(ex:today, ex:true), 0.5).\n\
             ex:wet(D, ex:true) :- ex:rain(D, ex:true).\n\
             ex:wet(D, ex:true) :- ex:sprinkler(D, ex:true).\n\
             ?- ex:wet(ex:today, X).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let ans = evaluate(&store, WORLD, &prog, PROFILE, None).unwrap();
    assert_eq!(ans.status_str(), "ok");
    assert_eq!(ans.bindings.len(), 1, "exactly one binding: {ans:?}");
    assert_eq!(marginal_for(&ans, "X", "true"), Some(0.75));
}

#[test]
fn independent_and_marginal_is_product() {
    // both :- a, b.   a=0.5, b=0.4 independent → P(both) = 0.2.
    let store = WorldStore::new();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             :- probability_model(full_independence).\n\
             :- probability(ex:a(ex:s, ex:on), 0.5).\n\
             :- probability(ex:b(ex:s, ex:on), 0.4).\n\
             ex:both(S, ex:on) :- ex:a(S, ex:on), ex:b(S, ex:on).\n\
             ?- ex:both(ex:s, X).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let ans = evaluate(&store, WORLD, &prog, PROFILE, None).unwrap();
    assert_eq!(marginal_for(&ans, "X", "on"), Some(0.2));
}

// ── Dependency joint: correlated facts differ from the independence reading ─

#[test]
fn dependency_joint_marginal_uses_the_joint() {
    // a and b are perfectly correlated: joint(0.5, a, b), joint(0.5).
    // both :- a, b.  P(both) = 0.5  (vs 0.5*0.5=0.25 under independence).
    let store = WorldStore::new();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             :- probability_model(dependency).\n\
             :- joint(0.5, ex:a(ex:s, ex:on), ex:b(ex:s, ex:on)).\n\
             :- joint(0.5).\n\
             ex:both(S, ex:on) :- ex:a(S, ex:on), ex:b(S, ex:on).\n\
             ?- ex:both(ex:s, X).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let ans = evaluate(&store, WORLD, &prog, PROFILE, None).unwrap();
    assert_eq!(
        marginal_for(&ans, "X", "on"),
        Some(0.5),
        "perfectly-correlated joint gives 0.5, not the independent 0.25: {ans:?}"
    );
}

// ── AC2: confidence is NOT promoted to probability ────────────────────────

#[test]
fn confidence_is_not_a_probability_guard() {
    // diagnosis is asserted with confidence 0.9 — under ProbabilisticProfile with
    // a declared model, its marginal MUST be 1.0 (an asserted fact), NEVER 0.9.
    let store = WorldStore::new();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             :- probability_model(full_independence).\n\
             :- confidence(ex:diagnosis(ex:patient, ex:flu), 0.9).\n\
             ?- ex:diagnosis(ex:patient, X).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let ans = evaluate(&store, WORLD, &prog, PROFILE, None).unwrap();
    assert_eq!(ans.status_str(), "ok");
    let m = marginal_for(&ans, "X", "flu");
    assert_eq!(
        m,
        Some(1.0),
        "confidence must NOT become probability: {ans:?}"
    );
    assert_ne!(m, Some(0.9), "0.9 confidence leaked as a probability");
}

// ── No-model refusal guard ────────────────────────────────────────────────

#[test]
fn no_declared_model_refuses_with_unknown() {
    // A probability fact with NO probability_model → unknown (never assume independence).
    // The parser allows this; the evaluator refuses.
    let store = WorldStore::new();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             :- probability(ex:rain(ex:today, ex:true), 0.5).\n\
             ex:wet(D, ex:true) :- ex:rain(D, ex:true).\n\
             ?- ex:wet(ex:today, X).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let ans = evaluate(&store, WORLD, &prog, PROFILE, None).unwrap();
    assert_eq!(ans.status_str(), "unknown");
    assert!(
        ans.bindings.is_empty(),
        "refusal yields no marginals: {ans:?}"
    );
}

#[test]
fn prob_status_string_round_trips() {
    // The typed ReasoningResult losslessly carries the prob status.
    assert_eq!(
        prob_status_string(&prob_result(
            ProbStatus::Ok,
            WORLD,
            crate::result::ResultPayload::Marginals(vec![])
        )),
        ProbStatus::Ok.as_str()
    );
    assert_eq!(
        prob_status_string(&prob_result(
            ProbStatus::Unknown,
            WORLD,
            crate::result::ResultPayload::Marginals(vec![])
        )),
        ProbStatus::Unknown.as_str()
    );
}

#[test]
fn prob_unknown_is_unsupported_not_evaluated() {
    use crate::result::{EvaluationStatus, InformationState};
    // A no-declared-model refusal is unsupported + not-evaluated — explicitly
    // NOT the Belnap `neither`, and distinct from cf's revision-tie unknown.
    let r = prob_result(
        ProbStatus::Unknown,
        WORLD,
        crate::result::ResultPayload::Marginals(vec![]),
    );
    assert_eq!(r.evaluation, EvaluationStatus::Unsupported);
    assert_eq!(r.information, InformationState::NotEvaluated);
    assert!(r.validate().is_ok());
}

// ── Deterministic EDB facts participate with probability 1.0 ──────────────

#[test]
fn edb_fact_is_deterministic_probability_one() {
    let store = WorldStore::new();
    store.insert_quad(
        WORLD,
        &format!("{BASE}s"),
        &format!("{BASE}known"),
        &format!("{BASE}yes"),
    );
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             :- probability_model(full_independence).\n\
             ?- ex:known(ex:s, X).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let ans = evaluate(&store, WORLD, &prog, PROFILE, None).unwrap();
    assert_eq!(marginal_for(&ans, "X", "yes"), Some(1.0));
}

// ── Duplicate probabilistic fact is rejected (no double-counting) ─────────

#[test]
fn duplicate_probability_fact_is_rejected() {
    // The same fact declared twice would be counted as two independent
    // variables, corrupting the marginal — must hard-fail.
    let store = WorldStore::new();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             :- probability_model(full_independence).\n\
             :- probability(ex:rain(ex:today, ex:yes), 0.5).\n\
             :- probability(ex:rain(ex:today, ex:yes), 0.3).\n\
             ?- ex:rain(ex:today, X).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let err = evaluate(&store, WORLD, &prog, PROFILE, None).unwrap_err();
    assert!(
        err.message().contains("duplicate"),
        "unexpected error: {err}"
    );
}

// ── Too many independent facts is refused, not panicked ───────────────────

#[test]
fn too_many_independent_facts_is_rejected() {
    // 2^N enumeration is capped: over MAX_INDEPENDENT_FACTS hard-fails with a
    // clear message rather than overflowing the shift or exhausting memory.
    let store = WorldStore::new();
    let mut src = format!(
        ":- prefix(ex, '{BASE}').\n\
             :- probability_model(full_independence).\n"
    );
    for i in 0..(MAX_INDEPENDENT_FACTS + 1) {
        src.push_str(&format!(":- probability(ex:f{i}(ex:s, ex:yes), 0.5).\n"));
    }
    src.push_str("?- ex:f0(ex:s, X).\n");
    let prog = parse_query_program(&src).unwrap();
    let err = evaluate(&store, WORLD, &prog, PROFILE, None).unwrap_err();
    assert!(
        err.message().contains("too many"),
        "unexpected error: {err}"
    );
}

// ── Cut is rejected under the probabilistic profile ───────────────────────

#[test]
fn cut_is_rejected() {
    let store = WorldStore::new();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             :- probability_model(full_independence).\n\
             ex:p(X, Y) :- ex:q(X, Y), !.\n\
             ?- ex:p(ex:a, Y).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let err = evaluate(&store, WORLD, &prog, PROFILE, None).unwrap_err();
    assert!(err.message().contains("cut"), "unexpected error: {err}");
}

// ── Malformed joint table: probabilities must sum to one ──────────────────

#[test]
fn joint_probabilities_must_sum_to_one() {
    let store = WorldStore::new();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             :- probability_model(dependency).\n\
             :- joint(0.5, ex:a(ex:s, ex:on)).\n\
             :- joint(0.2).\n\
             ?- ex:a(ex:s, X).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let err = evaluate(&store, WORLD, &prog, PROFILE, None).unwrap_err();
    assert!(
        err.message().contains("sum to 1"),
        "unexpected error: {err}"
    );
}

// ── SIMD bit-identity probe: power_set_weights matches independent scalar ──
//
// Acceptance criterion: for every n in 0..=20 and every mask in 0..(1<<n),
// the SIMD-produced weight is bit-identical (f64::to_bits() equal, max_ulp == 0)
// to an independent scalar reference that re-derives the same product in the
// same i = 0..n multiply order.

#[test]
fn power_set_weights_simd_bit_identical_to_scalar() {
    // ── Independent scalar reference ──────────────────────────────────────
    // Computes, for each mask in 0..(1<<n), the product
    //   ∏_{i: bit i set} p_i · ∏_{i: bit i clear} (1 - p_i)
    // iterating i in 0..n order — the SAME multiply order the SIMD lanes use.
    // This is written from scratch (no call to power_set_weights) so that the
    // test is a genuine cross-check, not a tautology.
    let scalar_ref = |items: &[(Fact, f64)]| -> Vec<(u64, f64)> {
        let n = items.len();
        let p: Vec<f64> = items.iter().map(|(_, prob)| *prob).collect();
        let q: Vec<f64> = p.iter().map(|&pi| 1.0 - pi).collect();
        let total: u64 = 1u64 << n;
        let mut out = Vec::with_capacity(total as usize);
        for mask in 0..total {
            let mut weight = 1.0_f64;
            for i in 0..n {
                if mask & (1u64 << i) != 0 {
                    weight *= p[i];
                } else {
                    weight *= q[i];
                }
            }
            out.push((mask, weight));
        }
        out
    };

    // ── Deterministic, non-trivial, varied probabilities ──────────────────
    // p_i = 0.05 + 0.9 * ((i * 7 + 3) % 19) / 19  — all strictly in (0, 1).
    // Using 20 as the upper bound covers n = 0..=20 (2^20 ≈ 1.05 M masks).
    // n = 0 → 1 mask, n = 1 → 2 masks: both fall through to the scalar tail
    //   (total < LANES = 4 so chunks = 0).
    // n ≥ 2 → 2^n is a multiple of 4, exercising the full SIMD chunked path.
    // All n up to 20 are swept to satisfy the issue's acceptance criterion.
    for n in 0usize..=20 {
        let items: Vec<(Fact, f64)> = (0..n)
            .map(|i| {
                let raw = 0.05_f64 + 0.9_f64 * ((i * 7 + 3) % 19) as f64 / 19.0_f64;
                // Clamp defensively to strict (0, 1) — the formula already satisfies
                // this for all i, but explicit clamping documents the intent.
                let prob = raw.clamp(f64::MIN_POSITIVE, 1.0_f64 - f64::EPSILON);
                let fact: Fact = (
                    "https://example.org/simd/pred".to_string(),
                    format!("<https://example.org/simd/s{i}>"),
                    format!("<https://example.org/simd/o{i}>"),
                );
                (fact, prob)
            })
            .collect();

        let simd_result = power_set_weights(&items);
        let scalar_result = scalar_ref(&items);

        assert_eq!(
            simd_result.len(),
            scalar_result.len(),
            "n={n}: length mismatch: simd={} scalar={}",
            simd_result.len(),
            scalar_result.len()
        );

        for ((simd_mask, simd_w), (scalar_mask, scalar_w)) in
            simd_result.iter().zip(scalar_result.iter())
        {
            assert_eq!(
                simd_mask, scalar_mask,
                "n={n} mask={simd_mask}: mask ordering diverged"
            );
            assert_eq!(
                simd_w.to_bits(),
                scalar_w.to_bits(),
                "n={n} mask={simd_mask}: SIMD weight {simd_w:?} (bits={:#018x}) \
                     != scalar weight {scalar_w:?} (bits={:#018x}); max_ulp > 0",
                simd_w.to_bits(),
                scalar_w.to_bits()
            );
        }
    }
}

// ── row_schema facet: declared schema is validated and attached ────────────

/// A matching schema: the result binds IRI-valued `X`; the schema declares
/// `Required Iri` for `X`. Schema is attached and `row_schema.is_some()`.
#[test]
fn declared_schema_matching_attaches_row_schema() {
    use gmeow_logic_compile::result_shape::{
        ColumnBinding, ColumnKind, ResultColumn, ResultShape, RowCardinality,
    };

    let store = WorldStore::new();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             :- probability_model(full_independence).\n\
             :- probability(ex:rain(ex:today, ex:true), 0.5).\n\
             ex:wet(D, ex:true) :- ex:rain(D, ex:true).\n\
             ?- ex:wet(ex:today, X).\n"
    );
    let prog = parse_query_program(&src).unwrap();

    // Declare: one Required IRI column `X`, any number of rows.
    let schema = ResultShape::new(
        vec![ResultColumn {
            var: "X".to_owned(),
            kind: ColumnKind::Iri,
            binding: ColumnBinding::Required,
        }],
        RowCardinality::Contains,
    );

    let ans = evaluate(&store, WORLD, &prog, PROFILE, Some(schema)).unwrap();
    assert_eq!(ans.status_str(), "ok");
    assert!(
        ans.result.row_schema.is_some(),
        "row_schema must be attached when a declared schema matches"
    );
}

/// A mismatching schema: the result binds IRI-valued `X`; the schema declares
/// `Required BlankNode` for `X`. Must return Err (ContractViolation propagated).
#[test]
fn declared_schema_mismatch_returns_err() {
    use gmeow_logic_compile::result_shape::{
        ColumnBinding, ColumnKind, ResultColumn, ResultShape, RowCardinality,
    };

    let store = WorldStore::new();
    let src = format!(
        ":- prefix(ex, '{BASE}').\n\
             :- probability_model(full_independence).\n\
             :- probability(ex:rain(ex:today, ex:true), 0.5).\n\
             ex:wet(D, ex:true) :- ex:rain(D, ex:true).\n\
             ?- ex:wet(ex:today, X).\n"
    );
    let prog = parse_query_program(&src).unwrap();

    // Declare: `X` must be a blank-node — but the binding is an IRI → mismatch.
    let schema = ResultShape::new(
        vec![ResultColumn {
            var: "X".to_owned(),
            kind: ColumnKind::BlankNode,
            binding: ColumnBinding::Required,
        }],
        RowCardinality::Contains,
    );

    let err = evaluate(&store, WORLD, &prog, PROFILE, Some(schema)).unwrap_err();
    assert!(
        err.message().contains("result-shape violation"),
        "ContractViolation must be propagated as Err: {err}"
    );
}
