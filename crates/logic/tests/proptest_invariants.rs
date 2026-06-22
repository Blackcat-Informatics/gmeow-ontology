// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Property-based structural invariants (#787, T6 of #781).
//!
//! Two invariant families:
//!
//! 1. **Logic IR canonicalization** — [`LogicProgram::new`] is the canonicalizer
//!    (it sorts axioms/rules/profiles). The property: canonicalization is
//!    *idempotent* (`canon(canon(x)) == canon(x)`) and *order-independent*
//!    (building from a permuted input yields the same canonical program). Verified
//!    through the existing [`canonical_key`] content key and the [`assert_ir_isomorphic`]
//!    gate — re-used, never re-minted (Principle 5). This targets the **current**
//!    IR; full-FOL-IR idempotence is deferred (#719).
//!
//! 2. **Entrenchment strict partial order** — [`Entrenchment`] reads a `≻` order
//!    from `gmeow:overrides` edges. The property: any successfully-built order is a
//!    strict partial order (irreflexive, asymmetric, transitive), and a cyclic
//!    input is *rejected* rather than silently collapsed. Generators are biased
//!    toward connected edge sets so the order relations are exercised
//!    non-vacuously.

use std::cmp::Ordering;

use gmeow_logic::compile::adapter::assert_ir_isomorphic;
use gmeow_logic::compile::ir::{ContextualScope, LogicAxiom, LogicProgram, LogicRule};
use gmeow_logic::entrenchment::{Entrenchment, OVERRIDES};
use gmeow_logic::store::WorldStore;
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
        prop_assert!(err.contains("cycle"), "unexpected error: {}", err);
    }
}
