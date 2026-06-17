// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Stratum-C counterfactual world construction (#505).
//!
//! This is the **only generative, budgeted, possibly-incomplete** stratum of the
//! logic engine. When a query carries a [`crate::query_ir::QCounterfactual`]
//! declaration, resolution does not run against the materialized base world
//! directly. Instead [`construct_and_resolve`] performs the Phase-3 protocol from
//! `LOGIC-RUNTIME.md`:
//!
//! 1. **Minimal AGM revision** — admit the antecedent `A` into a copy of the base
//!    world, retracting the *least-entrenched* conflicting facts first. The
//!    entrenchment ordering is declared data (the risk/norms/standpoint vocab read
//!    by [`crate::entrenchment`]); a **total order yields exactly one** revised
//!    world, a **genuine (incomparable) tie yields `unknown`** — never a branch.
//! 2. **Transient, isolated construction** — seed a fresh named graph `W_cf` from
//!    the revised base; the base store is never mutated, so paraconsistency is
//!    preserved and nothing leaks back.
//! 3. **Scoped resolution** — resolve the consequent `φ` inside `W_cf`.
//! 4. **Memoize or dispose** — key the constructed world by the six-tuple in
//!    [`crate::versioning::counterfactual_world_key`].
//!
//! Nested counterfactuals are nested transient graphs bounded by a **depth
//! budget**; exceeding it degrades to an incomplete/`unknown` result rather than
//! recursing without bound.

use crate::query_ir::{AnswerSet, Budget, QProgram};
use crate::store::WorldStore;

/// Default hard cap on nested-counterfactual depth when a query does not declare
/// its own `depth_budget(N)`. Chosen conservatively: counterfactuals about
/// counterfactuals are rare and unbounded nesting is the failure mode being
/// guarded against.
pub const DEFAULT_DEPTH_BUDGET: u64 = 4;

/// Return `true` iff `program` is a Stratum-C counterfactual query that must be
/// routed through [`construct_and_resolve`] rather than the plain v4 dispatcher.
///
/// This is the routing predicate the PyO3 `query` surface consults before
/// choosing between [`crate::dispatch::dispatch_query`] (v4 backward goals) and
/// counterfactual construction (v5).
pub fn is_counterfactual(program: &QProgram) -> bool {
    program.counterfactual.is_some()
}

/// Construct the counterfactual world declared by `program` and resolve its goal
/// inside it.
///
/// `store` holds the materialized base world(s) as named graphs (read-only with
/// respect to the base — the constructed `W_cf` is a *fresh* graph). `profile`
/// selects the revision/closeness mode (the default deterministic revision, or an
/// opt-in budget-capped Lewis multi-world profile). `depth` is the remaining
/// nested-counterfactual budget.
///
/// # Errors
///
/// Returns `Err(String)` on a malformed declaration, an isolation/invariant
/// violation, or an engine error.
//
// NOTE(#505 Task 1): the construction body lands in Task 3 (AGM revision +
// transient chase) and Task 4 (Lewis profile). Task 1 establishes the surface,
// the routing predicate, and the depth-budget plumbing so the PyO3 `query`
// entry can already distinguish a counterfactual program from a plain goal.
pub fn construct_and_resolve(
    _store: &WorldStore,
    program: &QProgram,
    _profile: &str,
    _budget: &Budget,
    _depth: u64,
) -> Result<AnswerSet, String> {
    let _cf = program
        .counterfactual
        .as_ref()
        .ok_or_else(|| "construct_and_resolve called on a non-counterfactual program".to_owned())?;
    Err("counterfactual construction is not yet implemented (lands in #505 Task 3)".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query_ir::parse_query_program;

    fn cf_program() -> QProgram {
        parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             :- counterfactual(ex:cf, ex:base).\n\
             :- assume(ex:a(ex:s, ex:o)).\n\
             ?- ex:p(ex:s, Y).\n",
        )
        .unwrap()
    }

    fn plain_program() -> QProgram {
        parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ?- ex:p(ex:s, Y).\n",
        )
        .unwrap()
    }

    #[test]
    fn is_counterfactual_detects_declaration() {
        assert!(is_counterfactual(&cf_program()));
        assert!(!is_counterfactual(&plain_program()));
    }

    #[test]
    fn construct_and_resolve_rejects_plain_program() {
        let store = WorldStore::new();
        let err = construct_and_resolve(
            &store,
            &plain_program(),
            "https://blackcatinformatics.ca/logic/PositiveHornProfile",
            &Budget::default(),
            DEFAULT_DEPTH_BUDGET,
        )
        .unwrap_err();
        assert!(err.contains("non-counterfactual"), "got: {err}");
    }
}
