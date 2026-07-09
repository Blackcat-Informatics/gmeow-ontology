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
//!   decides — INCLUDING the closed arithmetic/comparison builtin set (`+ - * //`,
//!   `> < >= =< =:=`), evaluated as a post-join constraint stage. A
//!   `NativeOutcome::Unsupported` is a declared gap that falls through to the router
//!   below; the demotion class is now the RESIDUAL — cut, a non-binary atom, demand-
//!   breaks-stratification, and the residual arithmetic modes native cannot compute
//!   (an unbound operand, division by zero, or i64 overflow). A binary arithmetic
//!   program in a supported mode is decided here and never reaches the router.
//!   The native core now HONOURS `budget.max_steps`: its semi-naive governor stamps
//!   `BudgetStatus::Exhausted` at the step ceiling (a sound partial answer, never a
//!   wrong verdict), so a step-budgeted query is NOT demoted for lack of a step
//!   governor — it runs native for the fragments native decides. `max_answers` and
//!   `max_steps` budgets both stay native; only a declared native gap falls through.
//!
//! - **Fast path** (`Dispatch::Fast`): the goal contains only EDB atoms (no rule in the
//!   program defines any goal predicate). Resolved directly via a single SPARQL
//!   `SELECT DISTINCT` query against the native store — no Prolog overhead. Reached
//!   ONLY on the fallback (after a declared native gap) and ONLY without a step budget:
//!   `fast_path` honours just `max_answers`, so a step-budgeted goal that fell through
//!   is routed to Scryer instead (which does honour the step ceiling). A step-budgeted
//!   pure-EDB goal never falls through — native decides it (settled stratum 0).
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

use crate::oracle::{BackwardOracle, backward_oracle};
use crate::profile_gate;
use crate::query_ir::{AnswerSet, Budget, QBodyLit, QProgram, QTerm};
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
            if let crate::query_ir::QBodyLit::Atom(atom) = lit
                && idb.contains(&atom.pred)
            {
                entry.insert(atom.pred.clone());
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
/// builtin.
///
/// The native core decides the binary arithmetic fragment directly, so a
/// supported-mode builtin program is answered natively and never reaches this
/// fallback router. This detector remains as the router's SAFETY NET: a builtin
/// program that fell through (a residual native gap — an unbound operand, ÷0, or
/// overflow) must still route to Scryer, never the arithmetic-blind SPARQL fast
/// path.
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

/// Classify a program for the FALLBACK router (reached only after the native core
/// declared a gap): `Fast` only if every goal atom is a binary EDB atom and the
/// program carries no builtin; `Scryer` otherwise.
///
/// Forcing `Scryer` for a residual builtin program and for non-binary goal atoms is
/// a hard invariant: the SPARQL fast path neither evaluates arithmetic nor handles
/// arity ≠ 2. A supported-mode binary arithmetic program is decided by the native
/// core upstream and never reaches this router.
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
/// the native store (EDB-only fast path).
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
/// Returns `Err` if `store.select` fails (SPARQL error or `term_n3` failure).
pub fn fast_path(
    store: &WorldStore,
    world: &str,
    program: &QProgram,
    budget: &Budget,
) -> gmeow_errors::Result<AnswerSet> {
    // Collect the distinct variable names that appear in the goal atoms (left-to-right,
    // first-seen order) for the SELECT clause.
    let mut goal_vars: Vec<String> = Vec::new();
    let mut goal_vars_seen: HashSet<&str> = HashSet::new();
    for atom in &program.goal.atoms {
        for t in &atom.args {
            if let QTerm::Var(v) = t
                && goal_vars_seen.insert(v.as_str())
            {
                goal_vars.push(v.clone());
            }
        }
    }

    // Defensive guard: the fast path forms binary triple patterns by indexing
    // `args[0]`/`args[1]`. A non-binary goal atom must never reach here (classify_goal
    // routes those to Scryer), but fail with a clear error rather than panic-index if it
    // somehow does.
    if let Some(bad) = program.goal.atoms.iter().find(|a| a.args.len() != 2) {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Reason {
            detail: format!(
                "fast_path requires binary goal atoms; {:?} has arity {} \
                 (non-binary atoms must route to Scryer)",
                bad.pred,
                bad.args.len()
            ),
        }));
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
        world,
        patterns.join(" . ")
    );

    let rows = store.select(&sparql)?;

    let mut bindings = Vec::new();
    for row in rows {
        if let Some(max_a) = budget.max_answers
            && bindings.len() >= max_a
        {
            let mut answer = AnswerSet {
                bindings,
                status: BudgetStatus::Partial,
                preservation: crate::result::PreservationClaim::exact(),
                // The fast-path SPARQL projection over the materialized EDB does not run
                // the native governor, so it carries no stratum frontier.
                frontier: crate::query_ir::CompletionFrontier::empty(),
            };
            answer.canonicalize();
            return Ok(answer);
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
        // The fast-path SPARQL projection over the materialized EDB does not run the
        // native governor, so it carries no stratum frontier.
        frontier: crate::query_ir::CompletionFrontier::empty(),
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
        QTerm::Num(n) => crate::physical::emit_integer_surface(*n),
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
/// Returns `Err` from the profile gate or the chosen engine.
pub fn dispatch_query(
    foreign: &dyn ScryerForeign,
    store: &WorldStore,
    world: &str,
    program: &QProgram,
    profile: &str,
    budget: &Budget,
) -> gmeow_errors::Result<AnswerSet> {
    // (1) Profile gate — cut + arithmetic-builtin confinement.
    profile_gate::check_cut_profile(program, profile)?;
    profile_gate::check_builtin_profile(program, profile)?;

    // (2) Native physical core first — the primary backward path. The magic-sets engine
    // (`crate::physical::resolve_native`) answers the query fragment it decides by bottom-up
    // demand evaluation and is AUTHORITATIVE for that fragment. A `NativeOutcome::Unsupported`
    // is native declaring an honest DECLINE (incomplete-but-never-wrong), never a fallback
    // masking a case native could decide. The residual routed here to `backward_oracle()` is
    // EXACTLY this enumerated `UnsupportedKind` taxonomy — the closed set of gaps the backward
    // core cannot yet soundly decide, for which the Scryer oracle is the sanctioned decider:
    //
    //   - `Cut`                     — a `!` control construct. Constitutionally oracle-only
    //                                 (P17): cut is procedural, with no declarative bottom-up
    //                                 meaning, so it can only be discharged by the oracle.
    //                                 Produced at `physical/magic.rs` (whole-program + per-rule)
    //                                 and `physical/magic_generic.rs` (n-ary lowering).
    //   - `Floundering`             — NAF over a variable no positive body atom range-restricts.
    //                                 Deciding it natively would test one partial grounding, not
    //                                 the intended universal absence — an UNSOUND answer — so
    //                                 native refuses it. Produced at `physical/magic.rs`.
    //   - `NonStratifiable`         — a negative dependency edge inside a cycle: no stratification
    //                                 exists (or n-ary negation, unsupported on the generic leg).
    //                                 The demand/magic transform cannot give it a least model, so
    //                                 native declines. Produced at `physical/seminaive.rs` and
    //                                 `physical/magic_generic.rs`.
    //   - `NonTerminatingArithmetic`— a value-generating `is` inside an IDB cycle with no finite
    //                                 driver and no `max_steps` budget: an unbounded Herbrand
    //                                 stream. Refused STATICALLY rather than hang (with a step
    //                                 budget the governor cuts it and native decides it, so it
    //                                 never reaches here). Produced at `physical/magic.rs`.
    //   - `NonBinaryAtom`           — an arity the served evaluators cannot query (a non-binary
    //                                 shape that is not the reserved `triple/4` the generic leg
    //                                 serves). Emitting its empty demand slice as `Decided` would
    //                                 be a silent wrong answer; native declines instead. Produced
    //                                 at `physical/magic.rs` and `physical/magic_generic.rs`.
    //   - `Arithmetic`              — a residual arithmetic MODE the closed native builtin set
    //                                 cannot compute (an unbound operand, ÷0, or i64 overflow), or
    //                                 a builtin on the n-ary generic leg. A single such residual
    //                                 re-demotes the whole program rather than return a wrong or
    //                                 truncated answer. Produced at `physical/seminaive.rs` and
    //                                 `physical/magic_generic.rs`.
    //
    // (`NonTerminatingExistential` is a FORWARD-chase-only gap — produced in `physical/chase.rs`,
    // reached only via `materialize` / `reason`, never through this backward `resolve_native`.)
    //
    // The native engine now HONOURS `max_steps` (its semi-naive governor stamps
    // `BudgetStatus::Exhausted` at the step ceiling — a sound partial answer, never a
    // wrong verdict), so a step-budgeted query is NO LONGER demoted for lack of a step
    // governor: it runs native for the fragments native decides.  Only a declared native
    // gap still falls through, where the step-honouring Scryer fallback takes over.
    match crate::physical::resolve_native(foreign, world, program, budget)? {
        crate::physical::NativeOutcome::Decided(answer) => return Ok(answer),
        crate::physical::NativeOutcome::Unsupported(_) => {}
    }

    // (3) Fallback: cyclic IDB predicates for tabling, then the legacy router.
    let table_preds = cyclic_predicates(program);

    match classify_goal(program) {
        // This arm is only reached when native declared a gap (`Unsupported`). `fast_path`
        // is a single-pass EDB SPARQL optimisation that honours only `max_answers`; it has
        // no step governor. A step-budgeted goal that fell through here must therefore
        // bypass it and go to Scryer, which wraps the goal in `call_with_inference_limit`
        // and stamps `BudgetStatus::Exhausted` on the step ceiling. (A step-budgeted
        // pure-EDB goal no longer reaches here: native decides it — settled stratum 0 —
        // and returns a complete `Ok` answer, the frontier win at the query surface.)
        Dispatch::Fast if budget.max_steps.is_none() => fast_path(store, world, program, budget),
        Dispatch::Fast | Dispatch::Scryer => {
            backward_oracle().solve(foreign, world, program, &table_preds, budget)
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
    fn list_world() -> (WorldStore, &'static str) {
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
        (store, W)
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

        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ?- ex:parentOf(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget::default();
        let ans = fast_path(&store, W, &prog, &budget).unwrap();

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

        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ?- ex:parentOf(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget::default();

        let fast = fast_path(&store, W, &prog, &budget).unwrap();

        let foreign = WorldStoreForeign::from_world(&store, W, HORN_PROFILE).unwrap();
        // Backward oracle with no table preds on an EDB-only goal.
        let scryer = backward_oracle()
            .solve(&foreign, W, &prog, &[], &budget)
            .unwrap();

        assert_eq!(
            fast.bindings, scryer.bindings,
            "fast_path and the backward oracle must agree on EDB-only goal"
        );
    }

    // ── dispatch_query end-to-end (recursive ancestor, 3 answers) ─────────────

    #[test]
    fn dispatch_query_recursive_ancestor_routes_scryer() {
        let store = WorldStore::new();
        store.insert_quad(W, &p("a"), &p("parentOf"), &p("b"));
        store.insert_quad(W, &p("b"), &p("parentOf"), &p("c"));
        store.insert_quad(W, &p("c"), &p("parentOf"), &p("d"));

        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget::default();

        let foreign = WorldStoreForeign::from_world(&store, W, HORN_PROFILE).unwrap();
        let ans = dispatch_query(&foreign, &store, W, &prog, HORN_PROFILE, &budget).unwrap();

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

    // ── Arithmetic-builtin list functions (G2a) ─────────────────────────
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
            world,
            &prog,
            PROCEDURAL_PROFILE,
            &Budget::default(),
        )
        .unwrap();
        assert_eq!(ans.status, BudgetStatus::Ok);
        assert_eq!(ans.bindings.len(), 1, "exactly one length answer: {ans:?}");
        assert_eq!(ans.bindings[0]["N"], format!("\"3\"^^<{XSD_INT}>"));
    }

    /// The binary arithmetic list-length program is DECIDED by the native core,
    /// so `dispatch_query` returns at the native arm and never reaches the
    /// `classify_goal` / Scryer fallback router.  Probing `resolve_native`
    /// directly proves the demotion class no longer contains this program.
    #[test]
    fn binary_arithmetic_is_decided_by_native_not_demoted() {
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
        // Native decides it directly — not an Unsupported gap that would demote.
        let outcome =
            crate::physical::resolve_native(&foreign, world, &prog, &Budget::default()).unwrap();
        let crate::physical::NativeOutcome::Decided(answer) = outcome else {
            panic!("binary arithmetic must be decided natively, not demoted: {outcome:?}");
        };
        assert_eq!(answer.bindings.len(), 1);
        assert_eq!(answer.bindings[0]["N"], format!("\"3\"^^<{XSD_INT}>"));
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
            world,
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
            world,
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
            world,
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
        let world_nn = W.to_owned();

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

    // ── N-ary predicate-as-data `triple/4` on the PARSED production surface ──────────

    /// The canonical predicate-as-data `triple/4` shape, driven end-to-end through the
    /// REAL production surface (`parse_query_program` → `dispatch_query`), DECIDES with
    /// non-empty correct bindings.
    ///
    /// This is the parser-driven twin of the hand-built-IR unit tests in
    /// `physical::magic_generic`: it proves the reserved bare `triple` relation now
    /// parses (previously `parse_query_program` rejected it with
    /// `cannot resolve predicate IRI "triple"`), routes through the arity-generic
    /// evaluator, and agrees with the generic-triple EDB's `push_fact("triple", …)`.
    #[test]
    fn dispatch_query_parsed_triple4_decides_nary_goal() {
        // A single <p1> edge x→y; the sub-property rule derives x <p2> y.
        let store = WorldStore::new();
        store.insert_quad(W, &p("x"), &p("p1"), &p("y"));
        let world_nn = W.to_owned();

        // The reserved bare `triple` relation with the property pinned in the DATA
        // position — the shape the binary store cannot express.
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             triple(S, ex:p2, O, Wg) :- triple(S, ex:p1, O, Wg).\n\
             ?- triple(S, ex:p2, O, Wg).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        // The parser carried the reserved relation VERBATIM (bare, un-resolved).
        assert_eq!(prog.goal.atoms[0].pred, "triple");
        assert_eq!(prog.goal.atoms[0].args.len(), 4, "arity 4 ⇒ n-ary path");

        let budget = Budget::default();
        let foreign = WorldStoreForeign::from_world(&store, W, HORN_PROFILE).unwrap();
        let ans =
            dispatch_query(&foreign, &store, &world_nn, &prog, HORN_PROFILE, &budget).unwrap();
        assert_eq!(ans.status, BudgetStatus::Ok);
        assert_eq!(
            ans.bindings.len(),
            1,
            "exactly one derived <p2> edge (non-empty): {ans:?}"
        );
        let b = &ans.bindings[0];
        assert_eq!(b["S"], format!("<{BASE}x>"), "subject binding");
        assert_eq!(b["O"], format!("<{BASE}y>"), "object binding");
        assert_eq!(b["Wg"], format!("<{W}>"), "world binding");
    }

    /// An n-ary shape the generic evaluator CANNOT serve — an arity-3 IDB over a binary
    /// EDB predicate (`edge`) that the generic-triple EDB never loads — must NOT be a
    /// silent-empty `Ok`.  Native declares an honest `Unsupported(NonBinaryAtom)` gap and
    /// `dispatch_query` routes it to the oracle, which decides it CORRECTLY (non-empty).
    /// This closes the F2 silent-wrong-answer defect on the parsed production surface.
    #[test]
    fn dispatch_query_parsed_nary_over_binary_edb_not_silent_empty() {
        let store = WorldStore::new();
        store.insert_quad(W, &p("a"), &p("edge"), &p("b"));
        store.insert_quad(W, &p("b"), &p("edge"), &p("c"));
        let world_nn = W.to_owned();

        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:tri(X, Y, Z) :- ex:edge(X, Y), ex:edge(Y, Z).\n\
             ?- ex:tri(ex:a, Y, Z).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget::default();

        // Native MUST declare the gap (never a silent-empty `Decided`): the generic
        // evaluator cannot load the binary `edge` EDB, so it is `NonBinaryAtom`.
        let native = crate::physical::resolve_native(
            &WorldStoreForeign::from_world(&store, W, HORN_PROFILE).unwrap(),
            &world_nn,
            &prog,
            &budget,
        )
        .unwrap();
        assert!(
            matches!(
                native,
                crate::physical::NativeOutcome::Unsupported(
                    crate::physical::UnsupportedKind::NonBinaryAtom
                )
            ),
            "an un-servable n-ary shape must be a declared gap, not silent-empty: {native:?}"
        );

        // dispatch_query routes the gap to the oracle, which decides it non-empty.
        let foreign = WorldStoreForeign::from_world(&store, W, HORN_PROFILE).unwrap();
        let ans =
            dispatch_query(&foreign, &store, &world_nn, &prog, HORN_PROFILE, &budget).unwrap();
        assert_eq!(
            ans.bindings.len(),
            1,
            "the oracle decides tri(a,Y,Z) non-empty (Y=b, Z=c): {ans:?}"
        );
        assert_eq!(ans.bindings[0]["Y"], format!("<{BASE}b>"));
        assert_eq!(ans.bindings[0]["Z"], format!("<{BASE}c>"));
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
        let world_nn = W.to_owned();

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
            world,
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
            msg.message().contains("builtin") && msg.message().contains(HORN_PROFILE),
            "error must name the offending profile: {msg:?}"
        );
    }

    // ── Budget: max_steps runs NATIVE (no demotion) ──────────────────────────────

    #[test]
    fn dispatch_budget_max_steps_runs_native_and_matches_reference() {
        // Build a chain: a→b→c→d (3 EDB parentOf edges), transitive-closure program.
        // The native engine now HONOURS `max_steps`: a step-budgeted query runs native
        // (no demotion) and stamps `Exhausted` at the ceiling. At a zero-step budget the
        // IDB goal derives nothing, so native and the reference oracle agree byte-for-byte
        // (both `Exhausted`, both empty); at an ample budget native completes with `Ok`.
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
        let world_nn = W.to_owned();
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

        // Zero-step budget: the native governor stops before the first ancestor
        // derivation → `Exhausted` with no bindings. The reference oracle also exhausts at
        // its first budget check (steps=0 >= 0) with no bindings. Because the IDB goal
        // derives nothing at budget 0, the two engines agree byte-for-byte here.
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
            "a zero-step budget must be Exhausted (native honours it, no wrong Ok)"
        );
        assert!(
            dispatched.bindings.is_empty(),
            "a zero-step budget derives no ancestor ⇒ no bindings"
        );

        // At budget 0 the native path and the reference oracle agree byte-for-byte (both
        // Exhausted, both empty) — the completion boundary where the two engines coincide.
        let reference =
            crate::reference_resolver::resolve(&foreign, &world_nn, &prog, &tight_budget).unwrap();
        assert_eq!(
            dispatched.status, reference.status,
            "native dispatch status must match the reference oracle at budget 0"
        );
        assert_eq!(
            dispatched.bindings, reference.bindings,
            "native dispatch bindings must match the reference oracle at budget 0 (both empty)"
        );
        // GAP A: the completion frontier crosses the PUBLIC `AnswerSet` boundary out of
        // `dispatch_query`. A zero-step cut leaves the single (magic-transformed) stratum
        // unsaturated — the caller reads `completed < total` to tell that from a complete
        // result.
        assert_eq!(
            dispatched.frontier.completed, 0,
            "a zero-step cut saturates no stratum: {:?}",
            dispatched.frontier
        );
        assert_eq!(
            dispatched.frontier.total, 1,
            "one stratum in the ancestor program: {:?}",
            dispatched.frontier
        );

        // An ample step budget completes on native with `Ok` and the full 3 answers —
        // native is NOT demoted for carrying a step budget.
        let ample = Budget {
            max_steps: Some(1_000_000),
            max_answers: None,
        };
        let completed =
            dispatch_query(&foreign, &store, &world_nn, &prog, HORN_PROFILE, &ample).unwrap();
        assert_eq!(completed.status, BudgetStatus::Ok, "ample budget completes");
        assert_eq!(
            completed.bindings.len(),
            3,
            "an ample step budget yields all 3 ancestors on the native path"
        );
        // GAP A: an ample budget saturates the stratum, so the public frontier reports a
        // complete run and a positive committed-derivation count.
        assert_eq!(
            completed.frontier.completed, completed.frontier.total,
            "an ample budget saturates the whole program: {:?}",
            completed.frontier
        );
        assert!(
            completed.frontier.consumed_steps >= 1,
            "deriving the 3 ancestors commits at least one derivation: {:?}",
            completed.frontier
        );
    }

    #[test]
    fn dispatch_budget_max_steps_pure_edb_goal_completes_native() {
        // A single binary EDB atom classifies as `Dispatch::Fast`, but the native engine
        // now runs first: a pure-EDB goal is the settled stratum 0, so it derives NOTHING
        // and native returns the COMPLETE answer with `Ok` under ANY step budget, including
        // 0. This is the frontier win at the query surface — more correct than the
        // reference oracle, which counts the EDB lookup as a step and stamps `Exhausted`
        // at 0. The two engines intentionally DIVERGE on the pure-EDB path (different step
        // units), so no cross-engine status parity is asserted here.
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
        let world_nn = W.to_owned();
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

        // Zero-step budget: native decides the pure-EDB goal without any derivation, so it
        // returns the COMPLETE answer (both children) with `Ok` — no inference was needed.
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
            BudgetStatus::Ok,
            "a pure-EDB goal needs no derivation ⇒ complete `Ok` under any step budget"
        );
        assert_eq!(
            dispatched.bindings.len(),
            2,
            "the complete pure-EDB answer (both children) is returned under budget 0"
        );

        // The reference oracle, by contrast, counts the EDB lookup as a step and stamps
        // Exhausted at budget 0 — the documented, intended divergence (different step
        // units). Native's complete answer is the more faithful verdict.
        let reference =
            crate::reference_resolver::resolve(&foreign, &world_nn, &prog, &tight_budget).unwrap();
        assert_eq!(
            reference.status,
            BudgetStatus::Exhausted,
            "the reference oracle exhausts at budget 0 — native intentionally diverges"
        );
    }
}
