// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Query classification and fast-path / Scryer dispatch.
//!
//! # Two-path architecture
//!
//! `dispatch_query` routes a `QProgram` to one of two engines:
//!
//! - **Fast path** (`Dispatch::Fast`): the goal contains only EDB atoms (no rule in the
//!   program defines any goal predicate). Resolved directly via a single SPARQL
//!   `SELECT DISTINCT` query against the oxigraph store — no Prolog overhead.
//!
//! - **Scryer path** (`Dispatch::Scryer`): the goal hits at least one IDB predicate
//!   (a predicate defined as a rule head). Delegated to `scryer_engine::run_scryer`
//!   with `:- table` directives for the cyclic IDB predicates.
//!
//! # Cut gate
//!
//! `dispatch_query` calls `profile_gate::check_cut_profile` first. Any program
//! containing cut that arrives under a non-procedural profile is hard-rejected before
//! any engine is invoked.

use std::collections::{BTreeMap, BTreeSet};

use oxigraph::model::NamedNode;

use crate::profile_gate;
use crate::query_ir::{AnswerSet, Budget, QProgram, QTerm};
use crate::scryer_engine;
use crate::seam::{BudgetStatus, ScryerForeign};
use crate::store::WorldStore;

// ── Dispatch enum ──────────────────────────────────────────────────────────────

/// Which engine should handle a given query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dispatch {
    /// SPARQL fast path — the goal is a pure EDB conjunction; no Prolog needed.
    Fast,
    /// Scryer Prolog — the goal involves at least one IDB (rule-defined) predicate.
    Scryer,
}

// ── IDB / cycle analysis ───────────────────────────────────────────────────────

/// Return the set of IDB predicate IRIs — predicates that appear as a rule head
/// in at least one rule.
pub fn idb_predicates(program: &QProgram) -> BTreeSet<String> {
    program.rules.iter().map(|r| r.head.pred.clone()).collect()
}

/// Return the sorted, distinct IDB predicate IRIs that lie on a dependency cycle.
///
/// A predicate P is cyclic iff P transitively depends on itself.  The dependency
/// graph has an edge P → Q for every body atom Q in a rule whose head pred is P,
/// when Q is also an IDB predicate.
///
/// These are the predicates that need `:- table P/2` in Scryer so that tabling
/// (SLG resolution) prevents infinite loops on left- or right-recursive programs.
pub fn cyclic_predicates(program: &QProgram) -> Vec<String> {
    let idb = idb_predicates(program);

    // Build adjacency: head_pred -> {body IDB preds}
    let mut adj: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for rule in &program.rules {
        let head = &rule.head.pred;
        let entry = adj.entry(head.clone()).or_default();
        for lit in &rule.body {
            if let crate::query_ir::QBodyLit::Atom(atom) = lit {
                if idb.contains(&atom.pred) {
                    entry.insert(atom.pred.clone());
                }
            }
        }
    }

    // DFS cycle detection: a pred is cyclic iff it can reach itself.
    let mut cyclic: BTreeSet<String> = BTreeSet::new();

    for start in &idb {
        if reachable_to_self(start, &adj) {
            cyclic.insert(start.clone());
        }
    }

    let mut result: Vec<String> = cyclic.into_iter().collect();
    result.sort();
    result
}

/// Return `true` if `start` can reach itself via edges in `adj` (DFS).
fn reachable_to_self(start: &str, adj: &BTreeMap<String, BTreeSet<String>>) -> bool {
    let mut visited: BTreeSet<&str> = BTreeSet::new();
    let mut stack: Vec<&str> = Vec::new();

    // Push direct neighbours; we look for a path back to `start`.
    if let Some(neighbours) = adj.get(start) {
        for n in neighbours {
            stack.push(n.as_str());
        }
    }

    while let Some(cur) = stack.pop() {
        if cur == start {
            return true;
        }
        if !visited.insert(cur) {
            continue; // already explored
        }
        if let Some(neighbours) = adj.get(cur) {
            for n in neighbours {
                stack.push(n.as_str());
            }
        }
    }
    false
}

// ── Goal classification ────────────────────────────────────────────────────────

/// Classify a program: `Fast` if every goal atom pred is EDB, `Scryer` otherwise.
pub fn classify_goal(program: &QProgram) -> Dispatch {
    let idb = idb_predicates(program);
    let needs_scryer = program.goal.atoms.iter().any(|a| idb.contains(&a.pred));
    if needs_scryer {
        Dispatch::Scryer
    } else {
        Dispatch::Fast
    }
}

// ── Fast path ─────────────────────────────────────────────────────────────────

/// Resolve the program's goal via a single SPARQL SELECT DISTINCT query against
/// the oxigraph store (EDB-only fast path).
///
/// Builds `SELECT DISTINCT ?V0 ?V1 … WHERE { GRAPH <world> { t0 . t1 . … } }`
/// from the goal atoms.  Each atom `pred(S, O)` emits a triple pattern where:
/// - A `QTerm::Const("<iri>")` is used verbatim (angle-bracket form is valid SPARQL).
/// - A `QTerm::Var(name)` becomes `?name`.
/// - The predicate bare IRI becomes `<pred>`.
///
/// Caps at `budget.max_answers` (→ `BudgetStatus::Partial`). Status is `Ok`
/// unless capped.  Result is canonicalized.
///
/// # Errors
///
/// Returns `Err(String)` if `store.select` fails (SPARQL error or `term_n3` failure).
pub fn fast_path(
    store: &WorldStore,
    world: &NamedNode,
    program: &QProgram,
    budget: &Budget,
) -> Result<AnswerSet, String> {
    // Collect the distinct variable names that appear in the goal atoms (left-to-right,
    // first-seen order) for the SELECT clause.
    let mut goal_vars: Vec<String> = Vec::new();
    for atom in &program.goal.atoms {
        for t in &atom.args {
            if let QTerm::Var(v) = t {
                if !goal_vars.contains(v) {
                    goal_vars.push(v.clone());
                }
            }
        }
    }

    // Build triple patterns.
    let patterns: Vec<String> = program
        .goal
        .atoms
        .iter()
        .map(|atom| {
            let s = term_to_sparql(&atom.args[0]);
            let p = format!("<{}>", atom.pred);
            let o = term_to_sparql(&atom.args[1]);
            format!("{s} {p} {o}")
        })
        .collect();

    let select_vars = if goal_vars.is_empty() {
        // Ground goal — still emit SELECT * to get a result row.
        "*".to_owned()
    } else {
        goal_vars
            .iter()
            .map(|v| format!("?{v}"))
            .collect::<Vec<_>>()
            .join(" ")
    };

    // Push the answer cap into the query so the store never materializes more than
    // `max_answers + 1` distinct rows. The `+ 1` lets the loop below still observe one
    // row beyond the cap and stamp `Partial` (it checks `len() >= max_a` before pushing).
    let limit_clause = budget
        .max_answers
        .map(|n| format!(" LIMIT {}", n.saturating_add(1)))
        .unwrap_or_default();
    let sparql = format!(
        "SELECT DISTINCT {select_vars} WHERE {{ GRAPH <{}> {{ {} }} }}{limit_clause}",
        world.as_str(),
        patterns.join(" . ")
    );

    let rows = store.select(&sparql)?;

    let mut bindings = Vec::new();
    for row in rows {
        if let Some(max_a) = budget.max_answers {
            if bindings.len() >= max_a {
                let mut answer = AnswerSet {
                    bindings,
                    status: BudgetStatus::Partial,
                };
                answer.canonicalize();
                return Ok(answer);
            }
        }
        // Build a Binding map containing only the goal variables.
        let mut binding = BTreeMap::new();
        for v in &goal_vars {
            if let Some(val) = row.get(v.as_str()) {
                binding.insert(v.clone(), val.clone());
            }
        }
        bindings.push(binding);
    }

    let mut answer = AnswerSet {
        bindings,
        status: BudgetStatus::Ok,
    };
    answer.canonicalize();
    Ok(answer)
}

/// Serialize a `QTerm` to a SPARQL token.
///
/// - `Const("<iri>")` → `<iri>` (the angle-bracket form is already valid SPARQL).
/// - `Var(name)` → `?name`.
fn term_to_sparql(t: &QTerm) -> String {
    match t {
        QTerm::Const(c) => c.clone(),
        QTerm::Var(v) => format!("?{v}"),
    }
}

// ── dispatch_query ─────────────────────────────────────────────────────────────

/// Resolve `program` against `world`, routing to the fast path or Scryer as
/// appropriate.
///
/// Steps:
/// 1. `profile_gate::check_cut_profile(program, profile)?` — hard-fail if cut is
///    present under a non-procedural profile.
/// 2. Compute `cyclic_predicates(program)` for `:- table` directives.
/// 3. Dispatch: `classify_goal(program) == Fast` → `fast_path`; else → `run_scryer`.
///
/// # Errors
///
/// Returns `Err(String)` from the profile gate or the chosen engine.
pub fn dispatch_query(
    foreign: &dyn ScryerForeign,
    store: &WorldStore,
    world: &NamedNode,
    program: &QProgram,
    profile: &str,
    budget: &Budget,
) -> Result<AnswerSet, String> {
    // (1) Profile gate — cut confinement.
    profile_gate::check_cut_profile(program, profile)?;

    // (2) Cyclic IDB predicates for tabling.
    let table_preds = cyclic_predicates(program);

    // (3) Dispatch.
    match classify_goal(program) {
        Dispatch::Fast => fast_path(store, world, program, budget),
        Dispatch::Scryer => {
            scryer_engine::run_scryer(foreign, world, program, &table_preds, budget)
        }
    }
}

// ── Unit tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query_ir::parse_query_program;
    use crate::seam::WorldStoreForeign;
    use crate::store::WorldStore;

    const BASE: &str = "https://example.org/";
    const W: &str = "http://logic.test/world/dispatch";
    const HORN_PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";

    fn p(local: &str) -> String {
        format!("{BASE}{local}")
    }

    // ── classify_goal ──────────────────────────────────────────────────────────

    #[test]
    fn classify_edb_only_goal_is_fast() {
        // No rules at all — pure EDB goal.
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ?- ex:parentOf(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        assert_eq!(classify_goal(&prog), Dispatch::Fast);
    }

    #[test]
    fn classify_goal_hitting_rule_head_is_scryer() {
        // ancestor is defined by rules — the goal hits an IDB predicate.
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        assert_eq!(classify_goal(&prog), Dispatch::Scryer);
    }

    // ── cyclic_predicates ──────────────────────────────────────────────────────

    #[test]
    fn cyclic_predicates_recursive_ancestor_is_cyclic() {
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let cyclic = cyclic_predicates(&prog);
        let ancestor_iri = p("ancestor");
        assert!(
            cyclic.contains(&ancestor_iri),
            "ancestor must be in cyclic predicates: {cyclic:?}"
        );
    }

    #[test]
    fn cyclic_predicates_non_recursive_is_empty() {
        // Single non-recursive rule: ancestor depends on parentOf (EDB) only.
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let cyclic = cyclic_predicates(&prog);
        assert!(
            cyclic.is_empty(),
            "non-recursive program must have no cyclic preds: {cyclic:?}"
        );
    }

    // ── fast_path correctness ──────────────────────────────────────────────────

    #[test]
    fn fast_path_returns_edb_bindings() {
        let store = WorldStore::new();
        store.insert_quad(W, &p("a"), &p("parentOf"), &p("b"));
        store.insert_quad(W, &p("a"), &p("parentOf"), &p("c"));
        let world_nn = NamedNode::new(W).unwrap();

        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ?- ex:parentOf(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget::default();
        let ans = fast_path(&store, &world_nn, &prog, &budget).unwrap();

        assert_eq!(ans.status, BudgetStatus::Ok);
        assert_eq!(ans.bindings.len(), 2, "expected 2 answers: {ans:?}");
        let ys: Vec<&str> = ans.bindings.iter().map(|b| b["Y"].as_str()).collect();
        assert!(
            ys.contains(&format!("<{BASE}b>").as_str()),
            "missing b: {ys:?}"
        );
        assert!(
            ys.contains(&format!("<{BASE}c>").as_str()),
            "missing c: {ys:?}"
        );
    }

    // ── fast == Scryer cross-check ─────────────────────────────────────────────
    //
    // Both engines must agree on the same EDB-only goal.

    #[test]
    fn fast_equals_scryer_on_edb_only_goal() {
        let store = WorldStore::new();
        store.insert_quad(W, &p("a"), &p("parentOf"), &p("b"));
        store.insert_quad(W, &p("a"), &p("parentOf"), &p("c"));
        let world_nn = NamedNode::new(W).unwrap();

        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ?- ex:parentOf(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget::default();

        let fast = fast_path(&store, &world_nn, &prog, &budget).unwrap();

        let foreign = WorldStoreForeign::from_world(&store, W, HORN_PROFILE).unwrap();
        // run_scryer with no table preds on an EDB-only goal.
        let scryer =
            crate::scryer_engine::run_scryer(&foreign, &world_nn, &prog, &[], &budget).unwrap();

        assert_eq!(
            fast.bindings, scryer.bindings,
            "fast_path and run_scryer must agree on EDB-only goal"
        );
    }

    // ── dispatch_query end-to-end (recursive ancestor, 3 answers) ─────────────

    #[test]
    fn dispatch_query_recursive_ancestor_routes_scryer() {
        let store = WorldStore::new();
        store.insert_quad(W, &p("a"), &p("parentOf"), &p("b"));
        store.insert_quad(W, &p("b"), &p("parentOf"), &p("c"));
        store.insert_quad(W, &p("c"), &p("parentOf"), &p("d"));
        let world_nn = NamedNode::new(W).unwrap();

        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget::default();

        let foreign = WorldStoreForeign::from_world(&store, W, HORN_PROFILE).unwrap();
        let ans =
            dispatch_query(&foreign, &store, &world_nn, &prog, HORN_PROFILE, &budget).unwrap();

        assert_eq!(ans.status, BudgetStatus::Ok);
        assert_eq!(
            ans.bindings.len(),
            3,
            "expected 3 transitive ancestors: {ans:?}"
        );
        let ys: Vec<&str> = ans.bindings.iter().map(|b| b["Y"].as_str()).collect();
        assert!(
            ys.contains(&format!("<{BASE}b>").as_str()),
            "missing b: {ys:?}"
        );
        assert!(
            ys.contains(&format!("<{BASE}c>").as_str()),
            "missing c: {ys:?}"
        );
        assert!(
            ys.contains(&format!("<{BASE}d>").as_str()),
            "missing d: {ys:?}"
        );
    }
}
