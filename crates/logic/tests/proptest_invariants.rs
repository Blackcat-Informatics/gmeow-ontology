// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Property-based structural invariants (T6).
//!
//! Two invariant families:
//!
//! 1. **Logic IR canonicalization** — [`LogicProgram::new`] is the canonicalizer
//!    (it sorts axioms/rules/profiles). The property: canonicalization is
//!    *idempotent* (`canon(canon(x)) == canon(x)`) and *order-independent*
//!    (building from a permuted input yields the same canonical program). Verified
//!    through the existing [`canonical_key`] content key and the [`assert_ir_isomorphic`]
//!    gate — re-used, never re-minted (Principle 5). This targets the **current**
//!    IR; full-FOL-IR idempotence is deferred.
//!
//! 2. **Entrenchment strict partial order** — [`Entrenchment`] reads a `≻` order
//!    from `gmeow:overrides` edges. The property: any successfully-built order is a
//!    strict partial order (irreflexive, asymmetric, transitive), and a cyclic
//!    input is *rejected* rather than silently collapsed. Generators are biased
//!    toward connected edge sets so the order relations are exercised
//!    non-vacuously.
//!
//! 3. **Reasoning-contract compatibility totality** (ME1) — `compat::check` is
//!    a total, deterministic, hard verdict over a [`ReasoningContract`]. The
//!    exhaustive oracle sweep in `compat.rs` pins correctness over the documented
//!    value domains; this property complements it with *robustness* over ARBITRARY
//!    facet strings (junk included): `check` never panics, is deterministic, and a
//!    contract that sets none of the documented forbidden facet values is always
//!    `Supported` — junk values never fabricate an `Unsupported` verdict.

use std::cmp::Ordering;

use gmeow_logic::entrenchment::{Entrenchment, OVERRIDES};
use gmeow_logic::store::WorldStore;
use gmeow_logic_compile::adapter::assert_ir_isomorphic;
use gmeow_logic_compile::compat::{ContractVerdict, check};
use gmeow_logic_compile::ir::{
    ContextualScope, LogicAxiom, LogicProgram, LogicRule, ReasoningContract,
};
use proptest::prelude::*;

const WORLD: &str = "http://world/base";

fn config() -> ProptestConfig {
    let cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(64);
    ProptestConfig {
        cases,
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

// ── IR generators ────────────────────────────────────────────────────────────────

fn arb_iri() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9]{0,5}".prop_map(|s| format!("https://example.org/{s}"))
}

fn arb_axiom() -> impl Strategy<Value = LogicAxiom> {
    (
        arb_iri(),
        arb_iri(),
        arb_iri(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(subject, predicate, obj, obj_is_literal, negated)| {
            // subject/predicate are always non-empty IRIs, so `new` cannot fail.
            LogicAxiom::new(
                subject,
                predicate,
                obj,
                obj_is_literal,
                negated,
                ContextualScope::default(),
            )
            .expect("generated axiom has non-empty subject/predicate")
        })
}

fn arb_rule() -> impl Strategy<Value = LogicRule> {
    (arb_axiom(), prop::collection::vec(arb_axiom(), 0..4))
        .prop_map(|(head, body)| LogicRule::new(head, body, Vec::new(), ContextualScope::default()))
}

fn arb_program_parts() -> impl Strategy<Value = (Vec<LogicAxiom>, Vec<LogicRule>)> {
    (
        prop::collection::vec(arb_axiom(), 0..6),
        prop::collection::vec(arb_rule(), 0..4),
    )
}

// ── Entrenchment generators ──────────────────────────────────────────────────────

fn node(i: usize) -> String {
    format!("https://example.org/n{i}")
}

/// Edges restricted to strictly-increasing node indices — guaranteed acyclic, so
/// [`Entrenchment::read_from_world`] always succeeds and yields a non-trivial
/// partial order to exercise asymmetry/transitivity.
fn arb_dag_edges() -> impl Strategy<Value = Vec<(usize, usize)>> {
    prop::collection::vec((0usize..6, 0usize..6), 0..12).prop_map(|pairs| {
        pairs
            .into_iter()
            .filter_map(|(a, b)| match a.cmp(&b) {
                Ordering::Less => Some((a, b)),
                Ordering::Greater => Some((b, a)),
                Ordering::Equal => None,
            })
            .collect()
    })
}

// ── Reasoning-contract generators (ME1) ──────────────────────────────────────

/// One facet-value string: a documented value drawn from across the facets mixed
/// with arbitrary junk, so generated contracts exercise both recognised and
/// unrecognised local names in every field.
fn arb_member() -> impl Strategy<Value = String> {
    prop_oneof![
        prop::sample::select(vec![
            "StableModelSemantics",
            "LeastModelSemantics",
            "EntrenchmentRevision",
            "MonotonicRevision",
            "BelnapBilattice",
            "TwoValuedBoolean",
            "AdmitAllFour",
            "ForbidGap",
            "ForbidGlut",
            "ForbidGapAndGlut",
            "OpenWorldClosure",
            "ClosedWorldClosure",
            "ProbabilisticMeasure",
            "DefaultNegation",
        ])
        .prop_map(str::to_owned),
        "[A-Za-z]{1,8}".prop_map(|s| s),
    ]
}

/// An optional single-valued facet: `None` or one [`arb_member`].
fn arb_facet() -> impl Strategy<Value = Option<String>> {
    prop::option::of(arb_member())
}

/// A fully arbitrary contract: every rule-participating facet (plus a few others)
/// populated with documented-or-junk values, sets, and a closure map.
fn arb_contract() -> impl Strategy<Value = ReasoningContract> {
    (
        arb_facet(),                                               // model_semantics
        arb_facet(),                                               // truth_algebra
        arb_facet(),                                               // revision
        arb_facet(),                                               // admissible_valuation
        arb_facet(),                                               // default_closure
        arb_facet(),                                               // formula_fragment
        prop::collection::vec(arb_member(), 0..3),                 // uncertainty_measures
        prop::collection::vec(arb_member(), 0..3),                 // negation_operators
        prop::collection::vec(("[a-z]{1,4}", arb_member()), 0..3), // closure_entries
    )
        .prop_map(|(ms, ta, rev, av, dc, ff, um, neg, ce)| {
            let mut c = ReasoningContract::new();
            c.model_semantics = ms;
            c.truth_algebra = ta;
            c.revision = rev;
            c.admissible_valuation = av;
            c.default_closure = dc;
            c.formula_fragment = ff;
            c.uncertainty_measures.extend(um);
            c.negation_operators.extend(neg);
            c.closure_entries.extend(ce);
            c
        })
}

/// A junk facet value guaranteed NOT to equal any documented forbidden-trigger
/// value: the `Junk` prefix makes a collision impossible.
fn arb_junk() -> impl Strategy<Value = Option<String>> {
    prop::option::of("[A-Za-z]{1,8}".prop_map(|s| format!("Junk{s}")))
}

/// A contract whose every facet value is junk (never a documented trigger). It can
/// match no forbidden rule, so `check` MUST rule it `Supported`.
fn arb_safe_contract() -> impl Strategy<Value = ReasoningContract> {
    (
        arb_junk(),
        arb_junk(),
        arb_junk(),
        arb_junk(),
        arb_junk(),
        prop::collection::vec("[A-Za-z]{1,8}".prop_map(|s| format!("Junk{s}")), 0..3),
        prop::collection::vec(
            (
                "[a-z]{1,4}",
                "[A-Za-z]{1,8}".prop_map(|s| format!("Junk{s}")),
            ),
            0..3,
        ),
    )
        .prop_map(|(ms, ta, rev, av, dc, um, ce)| {
            let mut c = ReasoningContract::new();
            c.model_semantics = ms;
            c.truth_algebra = ta;
            c.revision = rev;
            c.admissible_valuation = av;
            c.default_closure = dc;
            c.uncertainty_measures.extend(um);
            c.closure_entries.extend(ce);
            c
        })
}

// ── Properties ──────────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(config())]

    /// IR canonicalization is idempotent and order-independent.
    #[test]
    fn ir_canonicalization_idempotent_and_order_independent(
        (axioms, rules) in arb_program_parts()
    ) {
        let program = LogicProgram::new(axioms.clone(), rules.clone(), Vec::new(), None);

        // Idempotence: rebuilding from the already-canonical collections is a no-op.
        let rebuilt = LogicProgram::new(
            program.axioms.clone(),
            program.rules.clone(),
            program.contracts.clone(),
            program.source_iri.clone(),
        );
        prop_assert_eq!(program.canonical_key(), rebuilt.canonical_key());
        prop_assert!(assert_ir_isomorphic(&program, &rebuilt).is_ok());
        prop_assert!(program == rebuilt);

        // Order-independence: building from a reversed input is canonically equal.
        let mut rev_axioms = axioms;
        rev_axioms.reverse();
        let mut rev_rules = rules;
        rev_rules.reverse();
        let reordered = LogicProgram::new(rev_axioms, rev_rules, Vec::new(), None);
        prop_assert_eq!(program.canonical_key(), reordered.canonical_key());
        prop_assert!(assert_ir_isomorphic(&program, &reordered).is_ok());
    }

    /// An acyclic overrides graph yields a strict partial order: irreflexive,
    /// asymmetric, and transitive.
    #[test]
    fn entrenchment_is_strict_partial_order(edges in arb_dag_edges()) {
        let store = WorldStore::new();
        for (a, b) in &edges {
            // Higher-entrenched ≻ lower: node(a) overrides node(b), with a < b.
            store.insert_quad(WORLD, &node(*a), OVERRIDES, &node(*b));
        }
        let order = Entrenchment::read_from_world(&store, WORLD)
            .expect("acyclic edges must build a valid ordering");

        let entities: Vec<String> = order.entities().iter().cloned().collect();

        // Non-vacuity guard: empty/strict-relation-free edge sets (e.g. all pairs
        // collapsed by the `Ordering::Equal` filter) leave the asymmetry/transitivity
        // arms below unreached, passing the property trivially. Require at least one
        // strict `≻` relation so the order laws are actually exercised.
        let has_strict = entities.iter().any(|a| {
            entities
                .iter()
                .any(|b| order.compare(a, b) == Some(Ordering::Greater))
        });
        prop_assume!(has_strict);

        for a in &entities {
            // Irreflexive: an IRI is only ever Equal to itself, never Greater/Less.
            prop_assert_eq!(order.compare(a, a), Some(Ordering::Equal));
            for b in &entities {
                match order.compare(a, b) {
                    // Asymmetric.
                    Some(Ordering::Greater) => {
                        prop_assert_eq!(order.compare(b, a), Some(Ordering::Less));
                    }
                    Some(Ordering::Less) => {
                        prop_assert_eq!(order.compare(b, a), Some(Ordering::Greater));
                    }
                    _ => {}
                }
                // Transitive: a ≻ b ∧ b ≻ c ⟹ a ≻ c.
                for c in &entities {
                    if order.compare(a, b) == Some(Ordering::Greater)
                        && order.compare(b, c) == Some(Ordering::Greater)
                    {
                        prop_assert_eq!(order.compare(a, c), Some(Ordering::Greater));
                    }
                }
            }
        }
    }

    /// A cyclic overrides ring is rejected, never silently collapsed.
    #[test]
    fn entrenchment_cycle_is_rejected(ring_len in 2usize..6) {
        let store = WorldStore::new();
        for i in 0..ring_len {
            store.insert_quad(WORLD, &node(i), OVERRIDES, &node((i + 1) % ring_len));
        }
        let err = Entrenchment::read_from_world(&store, WORLD)
            .expect_err("a cyclic ordering must be rejected");
        prop_assert!(err.message().contains("cycle"), "unexpected error: {}", err);
    }

    /// `compat::check` is total and deterministic over ARBITRARY facet strings: it
    /// never panics on junk input, and evaluating the same contract twice yields the
    /// identical verdict. (Totality is witnessed by the test simply not panicking.)
    #[test]
    fn contract_check_is_total_and_deterministic(contract in arb_contract()) {
        let first = check(&contract);
        let second = check(&contract);
        prop_assert_eq!(first, second);
    }

    /// No-false-unsupported: a contract whose every facet value is junk (never a
    /// documented forbidden-trigger value) is always `Supported`. Junk values can
    /// never fabricate an `Unsupported` verdict — the hard verdict fires only on the
    /// declared forbidden combinations, never on unrecognised facet content.
    #[test]
    fn junk_only_contract_is_supported(contract in arb_safe_contract()) {
        prop_assert!(
            matches!(check(&contract), ContractVerdict::Supported),
            "a junk-only contract was rejected: {:?}",
            contract,
        );
    }
}
