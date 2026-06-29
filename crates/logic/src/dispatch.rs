// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Query classification and native / fast-path / Scryer dispatch.
//!
//! # Primary path + fallback router
//!
//! `dispatch_query` resolves a `QProgram` native-first, falling back to the legacy
//! two-path router only for a declared native gap:
//!
//! - **Native physical core** (the primary path): `crate::physical::resolve_native`
//!   magic-transforms the query and evaluates it bottom-up over the columnar
//!   `RelationStore`. It is authoritative for the binary positive query fragment it
//!   decides; a `NativeOutcome::Unsupported` (cut / arithmetic / non-binary / demand-
//!   breaks-stratification) is a declared gap that falls through to the router below.
//!
//! - **Fast path** (`Dispatch::Fast`): the goal contains only EDB atoms (no rule in the
//!   program defines any goal predicate). Resolved directly via a single SPARQL
//!   `SELECT DISTINCT` query against the oxigraph store — no Prolog overhead.
//!
//! - **Scryer path** (`Dispatch::Scryer`): the goal hits at least one IDB predicate
//!   (a predicate defined as a rule head). Delegated to `scryer_engine::run_scryer`
//!   with `:- table` directives for the cyclic IDB predicates.
//!
//! The fast-path / Scryer router is the not-yet-native fallback and conformance oracle
//! for the fragments the native core does not yet decide; `classify_goal` is ONLY the
//! fallback router.
//!
//! # Cut gate
//!
//! `dispatch_query` calls `profile_gate::check_cut_profile` first. Any program
//! containing cut that arrives under a non-procedural profile is hard-rejected before
//! any engine is invoked.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use oxigraph::model::NamedNode;

use crate::profile_gate;
use crate::query_ir::{AnswerSet, Budget, QBodyLit, QProgram, QTerm};
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

/// Return `true` if any rule body in `program` contains an arithmetic/comparison
/// builtin (#1009 G2a). Such a program MUST be resolved by Scryer (the SPARQL fast
/// path cannot evaluate arithmetic), never the EDB fast path.
pub fn program_has_builtin(program: &QProgram) -> bool {
    program.rules.iter().any(|rule| {
        rule.body
            .iter()
            .any(|lit| matches!(lit, QBodyLit::Builtin(_)))
    })
}

/// Return `true` if any goal atom has arity ≠ 2. The SPARQL fast path indexes
/// `atom.args[0]`/`atom.args[1]`; a non-binary goal atom (e.g. `get/3`) must route
/// to Scryer rather than panic-indexing.
fn goal_has_non_binary_atom(program: &QProgram) -> bool {
    program.goal.atoms.iter().any(|a| a.args.len() != 2)
}

/// Classify a program: `Fast` only if every goal atom is a binary EDB atom and the
/// program contains no arithmetic builtin; `Scryer` otherwise.
///
/// Forcing `Scryer` for builtin programs and for non-binary goal atoms is a hard
/// invariant: the fast path neither evaluates arithmetic nor handles arity ≠ 2.
pub fn classify_goal(program: &QProgram) -> Dispatch {
    let idb = idb_predicates(program);
    let needs_scryer = program.goal.atoms.iter().any(|a| idb.contains(&a.pred))
        || program_has_builtin(program)
        || goal_has_non_binary_atom(program);
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
    let mut goal_vars_seen: HashSet<&str> = HashSet::new();
    for atom in &program.goal.atoms {
        for t in &atom.args {
            if let QTerm::Var(v) = t {
                if goal_vars_seen.insert(v.as_str()) {
                    goal_vars.push(v.clone());
                }
            }
        }
    }

    // Defensive guard: the fast path forms binary triple patterns by indexing
    // `args[0]`/`args[1]`. A non-binary goal atom must never reach here (classify_goal
    // routes those to Scryer), but fail with a clear error rather than panic-index if it
    // somehow does.
    if let Some(bad) = program.goal.atoms.iter().find(|a| a.args.len() != 2) {
        return Err(format!(
            "fast_path requires binary goal atoms; {:?} has arity {} \
             (non-binary atoms must route to Scryer)",
            bad.pred,
            bad.args.len()
        ));
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
                    preservation: crate::result::PreservationClaim::exact(),
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
        preservation: crate::result::PreservationClaim::exact(),
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
        // Defensive: a numeric operand in a fast-path goal is not normally reached
        // (builtin programs route to Scryer), but emit the canonical typed literal so
        // the SPARQL is still well-formed rather than producing a malformed token.
        QTerm::Num(n) => format!("\"{n}\"^^<http://www.w3.org/2001/XMLSchema#integer>"),
    }
}

// ── dispatch_query ─────────────────────────────────────────────────────────────

/// Resolve `program` against `world`, routing to the fast path or Scryer as
/// appropriate.
///
/// Steps:
/// 1. `profile_gate::check_cut_profile(program, profile)?` — hard-fail if cut is
///    present under a non-procedural profile.
/// 2. Native physical core first (`crate::physical::resolve_native`): return its answer
///    when it decides; fall through on a declared gap.
/// 3. Fallback router: compute `cyclic_predicates(program)` for `:- table`, then
///    `classify_goal(program) == Fast` → `fast_path`; else → `run_scryer`.
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
    // (1) Profile gate — cut + arithmetic-builtin confinement.
    profile_gate::check_cut_profile(program, profile)?;
    profile_gate::check_builtin_profile(program, profile)?;

    // (2) Native physical core first — the primary backward path. The magic-sets engine
    // (`crate::physical::resolve_native`) answers the binary positive query fragment by
    // bottom-up demand evaluation; it is authoritative for what it decides. A declared
    // gap (`NativeOutcome::Unsupported` — cut / arithmetic / non-binary / demand-breaks-
    // stratification) falls through to the demoted fast-path / Scryer fallback below.
    //
    // Step-budget demotion: the native engine runs to fixpoint and has no post-hoc step
    // governor; it cannot stamp `BudgetStatus::Exhausted` or honour `max_steps`. When
    // the caller supplies a step limit, route to the Scryer/fast-path fallback (which
    // does honour it) rather than silently running unbounded and reporting the wrong
    // status. `max_answers`-only budgets are genuinely handled natively and are NOT
    // demoted. A query carrying BOTH fields is demoted (max_steps takes precedence).
    if budget.max_steps.is_none() {
        match crate::physical::resolve_native(foreign, world, program, budget)? {
            crate::physical::NativeOutcome::Decided(answer) => return Ok(answer),
            crate::physical::NativeOutcome::Unsupported(_) => {}
        }
    }

    // (3) Fallback: cyclic IDB predicates for tabling, then the legacy router.
    let table_preds = cyclic_predicates(program);

    match classify_goal(program) {
        // `fast_path` is a single-pass EDB SPARQL optimisation that honours only
        // `max_answers`; it has no step governor. A step-budgeted query must therefore
        // bypass it and go to Scryer, which wraps the goal in `call_with_inference_limit`
        // and stamps `BudgetStatus::Exhausted` on the step ceiling (matching the reference
        // oracle). Without this, a step-budgeted pure-EDB goal would silently report `Ok`.
        Dispatch::Fast if budget.max_steps.is_none() => fast_path(store, world, program, budget),
        Dispatch::Fast | Dispatch::Scryer => {
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
    const PROCEDURAL_PROFILE: &str = "https://blackcatinformatics.ca/logic/ProceduralPrologProfile";
    const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";

    fn p(local: &str) -> String {
        format!("{BASE}{local}")
    }

    fn rdf(local: &str) -> String {
        format!("{RDF}{local}")
    }

    /// Build a 3-element RDF list (x y z) at l0 → l1 → l2 → rdf:nil in a fresh world.
    fn list_world() -> (WorldStore, NamedNode) {
        let store = WorldStore::new();
        let first = rdf("first");
        let rest = rdf("rest");
        let nil = rdf("nil");
        store.insert_quad(W, &p("l0"), &first, &p("x"));
        store.insert_quad(W, &p("l0"), &rest, &p("l1"));
        store.insert_quad(W, &p("l1"), &first, &p("y"));
        store.insert_quad(W, &p("l1"), &rest, &p("l2"));
        store.insert_quad(W, &p("l2"), &first, &p("z"));
        store.insert_quad(W, &p("l2"), &rest, &nil);
        (store, NamedNode::new(W).unwrap())
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

    // ── Arithmetic-builtin list functions (#1009 G2a) ─────────────────────────
    //
    // Over the list (x y z): l0 →first x, →rest l1; l1 →first y, →rest l2;
    // l2 →first z, →rest rdf:nil. Each runs via dispatch_query under the
    // ProceduralPrologProfile (the builtin gate licenses arithmetic there).

    const XSD_INT: &str = "http://www.w3.org/2001/XMLSchema#integer";

    #[test]
    fn list_length_via_arithmetic_builtin() {
        let (store, world) = list_world();
        let foreign = WorldStoreForeign::from_world(&store, W, PROCEDURAL_PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- prefix(rdf, '{RDF}').\n\
             ex:len(rdf:nil, 0).\n\
             ex:len(L, N) :- rdf:rest(L, R), ex:len(R, M), N is M + 1.\n\
             ?- ex:len(ex:l0, N).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let ans = dispatch_query(
            &foreign,
            &store,
            &world,
            &prog,
            PROCEDURAL_PROFILE,
            &Budget::default(),
        )
        .unwrap();
        assert_eq!(ans.status, BudgetStatus::Ok);
        assert_eq!(ans.bindings.len(), 1, "exactly one length answer: {ans:?}");
        assert_eq!(ans.bindings[0]["N"], format!("\"3\"^^<{XSD_INT}>"));
    }

    #[test]
    fn list_get_via_comparison_and_arithmetic() {
        let (store, world) = list_world();
        let foreign = WorldStoreForeign::from_world(&store, W, PROCEDURAL_PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- prefix(rdf, '{RDF}').\n\
             ex:get(L, 0, X) :- rdf:first(L, X).\n\
             ex:get(L, N, X) :- N > 0, rdf:rest(L, R), M is N - 1, ex:get(R, M, X).\n\
             ?- ex:get(ex:l0, 1, X).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let ans = dispatch_query(
            &foreign,
            &store,
            &world,
            &prog,
            PROCEDURAL_PROFILE,
            &Budget::default(),
        )
        .unwrap();
        assert_eq!(ans.status, BudgetStatus::Ok);
        assert_eq!(ans.bindings.len(), 1, "exactly one get answer: {ans:?}");
        assert_eq!(ans.bindings[0]["X"], format!("<{BASE}y>"));
    }

    #[test]
    fn list_index_of_via_arithmetic_builtin() {
        let (store, world) = list_world();
        let foreign = WorldStoreForeign::from_world(&store, W, PROCEDURAL_PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- prefix(rdf, '{RDF}').\n\
             ex:idx(L, X, 0) :- rdf:first(L, X).\n\
             ex:idx(L, X, N) :- rdf:rest(L, R), ex:idx(R, X, M), N is M + 1.\n\
             ?- ex:idx(ex:l0, ex:z, N).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let ans = dispatch_query(
            &foreign,
            &store,
            &world,
            &prog,
            PROCEDURAL_PROFILE,
            &Budget::default(),
        )
        .unwrap();
        assert_eq!(ans.status, BudgetStatus::Ok);
        assert_eq!(ans.bindings.len(), 1, "exactly one indexOf answer: {ans:?}");
        assert_eq!(ans.bindings[0]["N"], format!("\"2\"^^<{XSD_INT}>"));
    }

    #[test]
    fn comparison_only_builtin_filters_answers() {
        // pick/2 binds N to each list index (0,1,2) then keeps only N > 0.
        let (store, world) = list_world();
        let foreign = WorldStoreForeign::from_world(&store, W, PROCEDURAL_PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- prefix(rdf, '{RDF}').\n\
             ex:idx(L, X, 0) :- rdf:first(L, X).\n\
             ex:idx(L, X, N) :- rdf:rest(L, R), ex:idx(R, X, M), N is M + 1.\n\
             ex:positive(X, N) :- ex:idx(ex:l0, X, N), N > 0.\n\
             ?- ex:positive(X, N).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let ans = dispatch_query(
            &foreign,
            &store,
            &world,
            &prog,
            PROCEDURAL_PROFILE,
            &Budget::default(),
        )
        .unwrap();
        // Indices 1 (y) and 2 (z) survive the N > 0 filter; index 0 (x) is dropped.
        assert_eq!(
            ans.bindings.len(),
            2,
            "expected 2 positive-index answers: {ans:?}"
        );
        let xs: Vec<&str> = ans.bindings.iter().map(|b| b["X"].as_str()).collect();
        assert!(
            xs.contains(&format!("<{BASE}y>").as_str()),
            "missing y: {xs:?}"
        );
        assert!(
            xs.contains(&format!("<{BASE}z>").as_str()),
            "missing z: {xs:?}"
        );
    }

    // ── Native-first backward wiring ─────────────────────────────────────────────

    /// An IDB (recursive) program is resolved by the native physical core
    /// (`crate::physical::resolve_native`) — the primary backward path — not Scryer.
    /// The native magic-sets engine decides the binary positive fragment, so the
    /// transitive-ancestor answers come back native-authoritative. We assert the full
    /// answer set (a→b, a→c, a→d) to pin that the native path actually answered.
    #[test]
    fn dispatch_query_idb_resolved_by_native() {
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

        // The native core MUST decide this binary positive query directly.
        let native = crate::physical::resolve_native(
            &WorldStoreForeign::from_world(&store, W, HORN_PROFILE).unwrap(),
            &world_nn,
            &prog,
            &budget,
        )
        .unwrap();
        assert!(
            matches!(native, crate::physical::NativeOutcome::Decided(_)),
            "native core must decide an IDB binary positive query: {native:?}"
        );

        // dispatch_query routes through the native core and returns the same answers.
        let foreign = WorldStoreForeign::from_world(&store, W, HORN_PROFILE).unwrap();
        let ans =
            dispatch_query(&foreign, &store, &world_nn, &prog, HORN_PROFILE, &budget).unwrap();
        assert_eq!(ans.status, BudgetStatus::Ok);
        let ys: BTreeSet<String> = ans.bindings.iter().map(|b| b["Y"].clone()).collect();
        let want: BTreeSet<String> = ["b", "c", "d"]
            .into_iter()
            .map(|x| format!("<{BASE}{x}>"))
            .collect();
        assert_eq!(ys, want, "native-resolved transitive ancestors: {ys:?}");
    }

    /// A declared native gap falls through to the demoted fallback router. A cut under
    /// the procedural profile is `NativeOutcome::Unsupported(Cut)` for the native core,
    /// so `dispatch_query` must fall through to Scryer (the procedural cut engine) and
    /// still answer — the fallback is exercised, not bypassed.
    #[test]
    fn dispatch_query_cut_falls_back_to_scryer() {
        let store = WorldStore::new();
        store.insert_quad(W, &p("a"), &p("parentOf"), &p("b"));
        store.insert_quad(W, &p("a"), &p("parentOf"), &p("c"));
        let world_nn = NamedNode::new(W).unwrap();

        // first/2: a cut prunes after the first parentOf solution (procedural semantics).
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:first(X, Y) :- ex:parentOf(X, Y), !.\n\
             ?- ex:first(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget::default();

        // The native core declares cut a gap.
        let native = crate::physical::resolve_native(
            &WorldStoreForeign::from_world(&store, W, PROCEDURAL_PROFILE).unwrap(),
            &world_nn,
            &prog,
            &budget,
        )
        .unwrap();
        assert!(
            matches!(
                native,
                crate::physical::NativeOutcome::Unsupported(crate::physical::UnsupportedKind::Cut)
            ),
            "cut must be a declared native gap: {native:?}"
        );

        // dispatch_query falls through to Scryer (the procedural cut engine) and answers.
        let foreign = WorldStoreForeign::from_world(&store, W, PROCEDURAL_PROFILE).unwrap();
        let ans = dispatch_query(
            &foreign,
            &store,
            &world_nn,
            &prog,
            PROCEDURAL_PROFILE,
            &budget,
        )
        .unwrap();
        // The cut keeps exactly one answer (the first parentOf binding).
        assert_eq!(
            ans.bindings.len(),
            1,
            "cut must prune to one answer via the Scryer fallback: {ans:?}"
        );
    }

    #[test]
    fn builtin_under_non_procedural_profile_is_rejected() {
        let (store, world) = list_world();
        let foreign = WorldStoreForeign::from_world(&store, W, HORN_PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- prefix(rdf, '{RDF}').\n\
             ex:len(rdf:nil, 0).\n\
             ex:len(L, N) :- rdf:rest(L, R), ex:len(R, M), N is M + 1.\n\
             ?- ex:len(ex:l0, N).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let result = dispatch_query(
            &foreign,
            &store,
            &world,
            &prog,
            HORN_PROFILE,
            &Budget::default(),
        );
        assert!(
            result.is_err(),
            "arithmetic builtin under PositiveHornProfile must be rejected"
        );
        let msg = result.unwrap_err();
        assert!(
            msg.contains("builtin") && msg.contains(HORN_PROFILE),
            "error must name the offending profile: {msg:?}"
        );
    }

    // ── Budget: max_steps demotion parity ─────────────────────────────────────────

    #[test]
    fn dispatch_budget_max_steps_demotes_native_and_matches_reference() {
        // Build a chain: a→b→c→d (3 EDB parentOf edges), transitive-closure program.
        // With a very tight max_steps budget (1), the Scryer fallback exhausts before
        // fixpoint and reports Exhausted.  The native engine has no step governor and
        // would silently return all 3 answers with status Ok — the demotion guard must
        // prevent it from firing at all.
        let store = WorldStore::new();
        let base = "https://example.org/";
        store.insert_quad(
            W,
            &format!("{base}a"),
            &format!("{base}parentOf"),
            &format!("{base}b"),
        );
        store.insert_quad(
            W,
            &format!("{base}b"),
            &format!("{base}parentOf"),
            &format!("{base}c"),
        );
        store.insert_quad(
            W,
            &format!("{base}c"),
            &format!("{base}parentOf"),
            &format!("{base}d"),
        );
        let world_nn = NamedNode::new(W).unwrap();
        let foreign = crate::seam::WorldStoreForeign::from_world(&store, W, HORN_PROFILE).unwrap();

        let src = format!(
            ":- prefix(ex, '{base}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();

        // Unbudgeted dispatch goes through native and returns all 3 ancestors.
        let full = dispatch_query(
            &foreign,
            &store,
            &world_nn,
            &prog,
            HORN_PROFILE,
            &Budget::default(),
        )
        .unwrap();
        assert_eq!(
            full.bindings.len(),
            3,
            "unbudgeted should yield all 3 ancestors"
        );
        assert_eq!(
            full.status,
            BudgetStatus::Ok,
            "unbudgeted status must be Ok"
        );

        // Zero-step budget: reference_resolver exhausts at the very first budget_exceeded()
        // check (steps=0 >= 0) in resolve_conjunct, reporting Exhausted with no bindings.
        // Scryer's inference limit of 0 also fires immediately.  The native engine has no
        // step governor and would silently return all 3 answers with status Ok; the
        // demotion guard must prevent it from being invoked at all.
        let tight_budget = Budget {
            max_steps: Some(0),
            max_answers: None,
        };
        let dispatched = dispatch_query(
            &foreign,
            &store,
            &world_nn,
            &prog,
            HORN_PROFILE,
            &tight_budget,
        )
        .unwrap();

        // Core invariant: the native engine was NOT invoked.  If it had been, it would
        // have returned Ok with 3 bindings; the Scryer fallback honours the step budget
        // and returns Exhausted before collecting any answer.
        assert_eq!(
            dispatched.status,
            BudgetStatus::Exhausted,
            "a 1-step budget must be Exhausted (native would wrongly return Ok with 3 answers)"
        );
        assert!(
            dispatched.bindings.len() < 3,
            "demoted path must not deliver all 3 native answers"
        );

        // Cross-check the reference oracle under the same budget: reference_resolver hits
        // budget_exceeded() (steps=0 >= 0) at the top of the first resolve_conjunct call
        // and stamps Exhausted immediately.  Bindings must also match (both empty).
        let reference =
            crate::reference_resolver::resolve(&foreign, &world_nn, &prog, &tight_budget).unwrap();
        assert_eq!(
            reference.status,
            BudgetStatus::Exhausted,
            "reference oracle must also stamp Exhausted under a zero-step budget"
        );
        assert_eq!(
            dispatched.status, reference.status,
            "demoted dispatch status must match reference oracle under max_steps budget"
        );
        assert_eq!(
            dispatched.bindings, reference.bindings,
            "demoted dispatch bindings must match reference oracle under max_steps budget"
        );
    }

    #[test]
    fn dispatch_budget_max_steps_pure_edb_goal_honours_budget() {
        // A single binary EDB atom classifies as `Dispatch::Fast`. `fast_path` honours
        // only `max_answers`, so a step budget routed there would silently report `Ok`.
        // The router must instead send step-budgeted Fast goals to Scryer, which stamps
        // Exhausted on the step ceiling — matching the reference oracle.
        let store = WorldStore::new();
        store.insert_quad(
            W,
            &format!("{BASE}a"),
            &format!("{BASE}parentOf"),
            &format!("{BASE}b"),
        );
        store.insert_quad(
            W,
            &format!("{BASE}a"),
            &format!("{BASE}parentOf"),
            &format!("{BASE}c"),
        );
        let world_nn = NamedNode::new(W).unwrap();
        let foreign = WorldStoreForeign::from_world(&store, W, HORN_PROFILE).unwrap();

        // Pure-EDB goal (no IDB predicate) → classify_goal == Fast.
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ?- ex:parentOf(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        assert_eq!(
            classify_goal(&prog),
            Dispatch::Fast,
            "single binary EDB atom must classify as Fast"
        );

        // Unbudgeted: fast_path returns both children with status Ok.
        let full = dispatch_query(
            &foreign,
            &store,
            &world_nn,
            &prog,
            HORN_PROFILE,
            &Budget::default(),
        )
        .unwrap();
        assert_eq!(
            full.bindings.len(),
            2,
            "unbudgeted Fast goal yields both children"
        );
        assert_eq!(full.status, BudgetStatus::Ok);

        // Zero-step budget: must NOT take fast_path (which would report Ok); routes to
        // Scryer and stamps Exhausted, matching the reference oracle.
        let tight_budget = Budget {
            max_steps: Some(0),
            max_answers: None,
        };
        let dispatched = dispatch_query(
            &foreign,
            &store,
            &world_nn,
            &prog,
            HORN_PROFILE,
            &tight_budget,
        )
        .unwrap();
        assert_eq!(
            dispatched.status,
            BudgetStatus::Exhausted,
            "a step-budgeted pure-EDB goal must report Exhausted, not silent Ok via fast_path"
        );

        let reference =
            crate::reference_resolver::resolve(&foreign, &world_nn, &prog, &tight_budget).unwrap();
        assert_eq!(
            dispatched.status, reference.status,
            "step-budgeted Fast goal status must match the reference oracle"
        );
        assert_eq!(
            dispatched.bindings, reference.bindings,
            "step-budgeted Fast goal bindings must match the reference oracle"
        );
    }
}
