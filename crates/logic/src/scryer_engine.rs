// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Embedded Scryer Prolog engine — backward goal resolution over the materialized EDB.
//!
//! # Role: not-yet-native fallback + conformance oracle
//!
//! The native physical engine (`crate::physical`) is the primary backward resolution
//! path. This embedded Scryer engine (and the SPARQL fast path beside it) is now the
//! not-yet-native fallback and the conformance oracle for the backward fragments the
//! native core does not yet decide: `dispatch_query` resolves a query through
//! `crate::physical::resolve_native` first and only delegates here when the native core
//! declares the query a gap (`NativeOutcome::Unsupported` — cut / arithmetic /
//! non-binary / demand-breaks-stratification). The code is retained — not deleted — so
//! it can keep checking the native engine for equivalence before the native core
//! absorbs the remaining fragments.
//!
//! # Facts-as-DB strategy
//!
//! Scryer 0.10 exposes no way to register an in-process Rust closure as a Prolog
//! foreign predicate (its only foreign path is C-ABI `.so` FFI, which cannot
//! capture `&WorldStore`). Since Phase 2 is **read-only over the already-materialized
//! EDB**, no live callback is needed: we snapshot the queried world's quads through
//! [`ScryerForeign::in_world`] and load them as ordinary Prolog **ground facts**, then
//! run the goal. This honours the seam invariant exactly — Nemo writes derived quads;
//! the Scryer layer reads them as data — and the `ScryerForeign` accessor is the single
//! source of truth for what the EDB contains.
//!
//! # Term encoding (exact oracle parity)
//!
//! Every RDF term is encoded as a Prolog **quoted atom whose content is the oracle's
//! canonical string**: `'<https://…/bob>'` for IRIs, `'"lit"^^<dt>'` for literals
//! (the [`crate::provenance::term_n3`] form). A Scryer answer binding then comes back as
//! `Term::Atom(s)` whose `s` is *verbatim* the canonical `Const` form produced by the
//! [`crate::reference_resolver`] oracle — parity by construction, no lossy transform.
//! Predicate IRIs become quoted-atom **functors**: `'https://…/parentOf'(S, O)`.
//!
//! # Termination
//!
//! - **Primary:** `library(tabling)` — the caller passes the IDB predicates that sit in
//!   a dependency cycle; each is wrapped in `:- table P/2`, which makes left/right
//!   recursion terminate (SLG resolution).
//! - **Backstop:** every goal is wrapped in `call_with_inference_limit(Goal, Limit, R)`
//!   (`library(iso_ext)`). When the cumulative inference budget is exhausted, `R` unifies
//!   with `inference_limit_exceeded`, resolution stops, and the result is stamped
//!   [`BudgetStatus::Exhausted`]. This guarantees the engine can never hang, even on a
//!   pathological non-tabled (procedural) program.
//! - **Output cap:** `max_answers` stops collection early and stamps
//!   [`BudgetStatus::Partial`] (sound, incomplete — never silently presented as complete).
//!
//! `Machine`/`QueryState` are `!Send`; the engine drives a fresh machine per query on the
//! calling thread.

use std::collections::{BTreeMap, HashSet};
use std::sync::{LazyLock, Mutex};

use oxigraph::model::{NamedNode, Term as OxTerm};
use scryer_prolog::{LeafAnswer, MachineBuilder, Term as PlTerm};

use crate::provenance::term_n3;
use crate::query_ir::{
    AnswerSet, Binding, Budget, QAtom, QBodyLit, QBuiltin, QGoal, QProgram, QTerm,
};
use crate::seam::{BudgetStatus, ScryerForeign};

/// Default per-query inference ceiling when the caller specifies no `max_steps`.
///
/// Generous enough that conformance-scale worlds always complete, low enough that a
/// runaway non-tabled recursion is caught rather than hanging the process.
const DEFAULT_INFERENCE_LIMIT: u64 = 5_000_000;

/// Prolog variable used to capture the `call_with_inference_limit/3` result.
///
/// Chosen to never collide with a query variable (query variables come from the
/// `.logic` source and never contain a double underscore prefix of this form).
const BUDGET_RESULT_VAR: &str = "ScryerBudgetResult__";

/// The atom `call_with_inference_limit/3` binds its result to when the budget is hit.
const INFERENCE_LIMIT_EXCEEDED: &str = "inference_limit_exceeded";

/// `xsd:integer` datatype IRI — the canonical type of an arithmetic answer (#1009 G2a).
///
/// A Scryer `Integer` answer is rendered as the canonical typed-literal string
/// `"N"^^<…#integer>`, matching the [`crate::provenance::literal_n3`] form so that
/// computed list lengths/indices read back identically to a materialized literal.
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

/// Serialises every Scryer machine's lifetime (`build` → `run_query` → `Drop`).
///
/// Required because `scryer-prolog` maintains process-global mutable state (the atom
/// table / wam arena) that is mutated on machine construction **and** teardown. Two
/// threads each building their own `Machine` race that global state — `Machine` being
/// `!Send` prevents sharing one machine across threads but does **not** protect the
/// shared globals — corrupting the heap into an allocation abort. This is the exact
/// analogue of [`crate::nemo_engine`]'s `CHASE_LOCK` for Nemo's global timer, and is
/// needed in the same two contexts: default-parallel `cargo test`, and PyO3 hosts that
/// drive `query()` from multiple Python threads. The Python conformance path only ever
/// held one machine at a time (GIL + sequential cases), which is why the race surfaced
/// solely on the parallel `rust` gate.
static SCRYER_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

// ── Public entry point ──────────────────────────────────────────────────────────

/// Resolve `program`'s goal against `world` using embedded Scryer Prolog.
///
/// # Arguments
///
/// - `foreign`     — blackboard accessor; its `in_world(world, None, None, None)` snapshot
///   becomes the Prolog fact DB.
/// - `world`       — the named-graph IRI identifying the world to query.
/// - `program`     — the parsed `.logic` program (rules + goal). May contain `!` (cut);
///   cut is emitted verbatim into the Prolog program (the caller is responsible for the
///   `ProceduralPrologProfile` gate — see `profile_gate`).
/// - `table_preds` — IDB predicate IRIs to wrap in `:- table P/2` (cyclic predicates).
///   Pass empty for procedural (cut) programs, where tabling and cut are incompatible.
/// - `budget`      — `max_answers` caps output (→ `Partial`); `max_steps` is the inference
///   ceiling (→ `Exhausted`); both optional.
///
/// # Returns
///
/// A canonical, sorted [`AnswerSet`] of goal-variable bindings plus a [`BudgetStatus`].
///
/// # Errors
///
/// Returns `Err(String)` on a Scryer exception/error, an EDB term that cannot be
/// canonicalized, or an answer binding of an unexpected Prolog shape.
pub fn run_scryer(
    foreign: &dyn ScryerForeign,
    world: &NamedNode,
    program: &QProgram,
    table_preds: &[String],
    budget: &Budget,
) -> Result<AnswerSet, String> {
    // Serialise Scryer's process-global state for the full machine lifetime. Declared
    // first so it is dropped last — the guard is still held while `machine` is dropped
    // below (Scryer's `Drop` also touches the global atom table). A poisoned lock means
    // a prior query panicked; recover the guard so callers are not permanently wedged.
    let _scryer_guard = SCRYER_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let module = build_module(foreign, world, program, table_preds)?;
    let goal_vars = goal_vars(&program.goal);
    let query = build_goal_query(&program.goal, budget)?;

    let mut machine = MachineBuilder::default().build();
    machine.load_module_string("user", module);

    let mut bindings: Vec<Binding> = Vec::new();
    let mut status = BudgetStatus::Ok;

    {
        let mut query_state = machine.run_query(query);
        loop {
            // Output cap: stop before pulling another solution.
            if let Some(max_a) = budget.max_answers {
                if bindings.len() >= max_a {
                    status = BudgetStatus::Partial;
                    break;
                }
            }

            match query_state.next() {
                None => break,
                Some(Err(term)) => {
                    return Err(format!("scryer resolution error: {term:?}"));
                }
                Some(Ok(leaf)) => match leaf {
                    LeafAnswer::False => break,
                    LeafAnswer::True => {
                        // Ground goal success (no query variables at all).
                        bindings.push(BTreeMap::new());
                    }
                    LeafAnswer::Exception(term) => {
                        return Err(format!("scryer uncaught exception: {term:?}"));
                    }
                    LeafAnswer::LeafAnswer { bindings: pl, .. } => {
                        // Inference-limit backstop: a solution whose result variable is
                        // `inference_limit_exceeded` is the sentinel, not a real answer.
                        if budget_exhausted(&pl) {
                            status = BudgetStatus::Exhausted;
                            break;
                        }
                        let mut bind: Binding = BTreeMap::new();
                        for v in &goal_vars {
                            match pl.get(v) {
                                Some(PlTerm::Atom(s)) => {
                                    bind.insert(v.clone(), s.clone());
                                }
                                Some(PlTerm::Integer(i)) => {
                                    // A computed arithmetic answer (e.g. a list length or
                                    // index). Render as the canonical typed integer literal,
                                    // identical to provenance::literal_n3 for xsd:integer, so
                                    // it reads back like a materialized literal (#1009 G2a).
                                    // `Integer` is arbitrary-precision; use Display.
                                    bind.insert(v.clone(), format!("\"{i}\"^^<{XSD_INTEGER}>"));
                                }
                                Some(PlTerm::Var(_)) | None => {
                                    // Unbound goal variable — omit (matches the oracle).
                                }
                                Some(other) => {
                                    return Err(format!(
                                        "scryer answer binding for {v:?} has unexpected \
                                         shape {other:?} (expected a quoted-IRI atom)"
                                    ));
                                }
                            }
                        }
                        bindings.push(bind);
                    }
                },
            }
        }
    }

    let mut answer = AnswerSet {
        bindings,
        status,
        preservation: crate::result::PreservationClaim::exact(),
    };
    answer.canonicalize();
    Ok(answer)
}

// ── Budget result inspection ─────────────────────────────────────────────────────

/// Return `true` if the `call_with_inference_limit/3` result variable signals the
/// inference budget was exhausted in this solution.
fn budget_exhausted(pl: &BTreeMap<String, PlTerm>) -> bool {
    matches!(
        pl.get(BUDGET_RESULT_VAR),
        Some(PlTerm::Atom(a)) if a == INFERENCE_LIMIT_EXCEEDED
    )
}

// ── Module assembly ──────────────────────────────────────────────────────────────

/// Build the Prolog module string: library imports, table directives, EDB facts, IDB rules.
fn build_module(
    foreign: &dyn ScryerForeign,
    world: &NamedNode,
    program: &QProgram,
    table_preds: &[String],
) -> Result<String, String> {
    let mut out = String::new();
    out.push_str(":- use_module(library(tabling)).\n");
    out.push_str(":- use_module(library(iso_ext)).\n");

    // Tabling directives for the cyclic IDB predicates (bounded recursion).
    // The arity is taken from the predicate's rule head, so n-ary IDB predicates
    // (e.g. `get/3`/`idx/3` for list indexing — #1009 G2a) are tabled at the right
    // arity, not a hardcoded `/2`.
    for pred in table_preds {
        let arity = program
            .rules
            .iter()
            .find(|r| &r.head.pred == pred)
            .map(|r| r.head.args.len())
            .unwrap_or(2);
        out.push_str(&format!(":- table({}/{arity}).\n", prolog_quote(pred)));
    }

    // EDB facts: snapshot the whole world and emit one binary fact per quad.
    // The facts are emitted in SORTED (predicate, subject, object) order so that
    // Scryer's clause-enumeration order is deterministic. This matters for
    // order-sensitive resolution — `cut` (commit-to-first) and `max_answers`
    // truncation — where the *which* answer/subset must be reproducible across runs
    // (oxigraph's quad-iteration order is not contractually stable).
    let mut facts: Vec<(String, String, String)> = Vec::new();
    for dq in foreign.in_world(world, None, None, None) {
        let s = canonical(&dq.subject)?;
        let p = dq.predicate.as_str().to_owned();
        let o = canonical(&dq.object)?;
        facts.push((p, s, o));
    }
    facts.sort();
    for (p, s, o) in &facts {
        out.push_str(&format!(
            "{}({}, {}).\n",
            prolog_quote(p),
            prolog_quote(s),
            prolog_quote(o)
        ));
    }

    // IDB rules from the query program.
    for rule in &program.rules {
        out.push_str(&serialize_rule(rule));
        out.push('\n');
    }

    Ok(out)
}

/// Serialize a `QRule` to a Prolog clause (`head :- body.` or a fact `head.`).
fn serialize_rule(rule: &crate::query_ir::QRule) -> String {
    let head = serialize_atom(&rule.head);
    if rule.body.is_empty() {
        return format!("{head}.");
    }
    let body: Vec<String> = rule
        .body
        .iter()
        .map(|lit| match lit {
            QBodyLit::Atom(a) => serialize_atom(a),
            QBodyLit::Cut => "!".to_owned(),
            QBodyLit::Builtin(b) => serialize_builtin(b),
        })
        .collect();
    format!("{head} :- {}.", body.join(", "))
}

/// Serialize a `QAtom` to a Prolog goal/head term: `'pred'(Arg0, Arg1)`.
fn serialize_atom(atom: &QAtom) -> String {
    let args: Vec<String> = atom.args.iter().map(serialize_term).collect();
    format!("{}({})", prolog_quote(&atom.pred), args.join(", "))
}

/// Serialize a `QTerm`: a `Const` becomes a quoted atom carrying its canonical string;
/// a `Var` becomes the bare Prolog variable name; a `Num` becomes BARE digits.
///
/// The `Num` arm emits unquoted digits deliberately: a quoted `'1'` is a Prolog atom,
/// not an integer, and `is`/comparisons would not evaluate it (#1009 G2a).
fn serialize_term(term: &QTerm) -> String {
    match term {
        QTerm::Const(c) => prolog_quote(c),
        QTerm::Var(v) => v.clone(),
        QTerm::Num(n) => n.to_string(),
    }
}

/// Serialize a `QBuiltin` to NATIVE Prolog infix (Scryer evaluates it directly).
///
/// - `Is{target,lhs,op,rhs}` → `target is lhs op rhs` (op ∈ `+ - * //`).
/// - `Compare{lhs,op,rhs}`   → `lhs op rhs` (op ∈ `> < >= =< =:=`).
fn serialize_builtin(b: &QBuiltin) -> String {
    match b {
        QBuiltin::Is {
            target,
            lhs,
            op,
            rhs,
        } => format!(
            "{} is {} {} {}",
            serialize_term(target),
            serialize_term(lhs),
            op.token(),
            serialize_term(rhs)
        ),
        QBuiltin::Compare { lhs, op, rhs } => format!(
            "{} {} {}",
            serialize_term(lhs),
            op.token(),
            serialize_term(rhs)
        ),
    }
}

// ── Goal query assembly ──────────────────────────────────────────────────────────

/// Build the `run_query` string: the goal conjunction wrapped in
/// `call_with_inference_limit/3` so the engine cannot hang.
fn build_goal_query(goal: &QGoal, budget: &Budget) -> Result<String, String> {
    if goal.atoms.is_empty() {
        return Err("query has an empty goal".to_owned());
    }
    let conj: Vec<String> = goal.atoms.iter().map(serialize_atom).collect();
    let limit = budget.max_steps.unwrap_or(DEFAULT_INFERENCE_LIMIT);
    // call_with_inference_limit((A, B, ...), Limit, ResultVar).
    Ok(format!(
        "call_with_inference_limit(({}), {}, {}).",
        conj.join(", "),
        limit,
        BUDGET_RESULT_VAR
    ))
}

// ── Term canonicalization + Prolog quoting ───────────────────────────────────────

/// Canonical string for an oxigraph object/subject term, identical to the oracle's
/// `Const` form: `<iri>` for IRIs, n3 form for literals.
fn canonical(term: &OxTerm) -> Result<String, String> {
    term_n3(term).map_err(|e| format!("cannot canonicalize EDB term: {e}"))
}

/// Wrap `s` as a Prolog single-quoted atom, escaping `\\` and `'`.
fn prolog_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            other => out.push(other),
        }
    }
    out.push('\'');
    out
}

/// Collect the distinct variable names appearing in the goal atoms, in first-seen order.
fn goal_vars(goal: &QGoal) -> Vec<String> {
    let mut vars: Vec<String> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for atom in &goal.atoms {
        for t in &atom.args {
            if let QTerm::Var(v) = t {
                if seen.insert(v.as_str()) {
                    vars.push(v.clone());
                }
            }
        }
    }
    vars
}

// ── Unit tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query_ir::parse_query_program;
    use crate::reference_resolver::resolve;
    use crate::seam::WorldStoreForeign;
    use crate::store::WorldStore;

    const W: &str = "http://logic.test/world/scryer";
    const PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";
    const BASE: &str = "https://example.org/";

    fn make_foreign(triples: &[(&str, &str, &str)]) -> (WorldStore, NamedNode) {
        let store = WorldStore::new();
        for (s, p, o) in triples {
            store.insert_quad(W, s, p, o);
        }
        (store, NamedNode::new(W).unwrap())
    }

    fn p(local: &str) -> String {
        format!("{BASE}{local}")
    }

    // ── prolog_quote ──────────────────────────────────────────────────────────────

    #[test]
    fn prolog_quote_wraps_and_escapes() {
        assert_eq!(prolog_quote("<https://x/a>"), "'<https://x/a>'");
        assert_eq!(prolog_quote("a'b"), "'a\\'b'");
        assert_eq!(prolog_quote("a\\b"), "'a\\\\b'");
    }

    // ── Non-recursive single rule ──────────────────────────────────────────────────

    #[test]
    fn non_recursive_single_rule() {
        let (store, world) = make_foreign(&[(&p("alice"), &p("parentOf"), &p("bob"))]);
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();

        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestorOf(X, Y) :- ex:parentOf(X, Y).\n\
             ?- ex:ancestorOf(ex:alice, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let ans = run_scryer(&foreign, &world, &prog, &[], &Budget::default()).unwrap();

        assert_eq!(ans.status, BudgetStatus::Ok);
        assert_eq!(ans.bindings.len(), 1);
        assert_eq!(ans.bindings[0]["Y"], format!("<{BASE}bob>"));
    }

    // ── Recursive transitive closure WITH tabling (bounded recursion, AC-1) ─────────

    #[test]
    fn recursive_transitive_closure_tabled() {
        let (store, world) = make_foreign(&[
            (&p("a"), &p("parentOf"), &p("b")),
            (&p("b"), &p("parentOf"), &p("c")),
            (&p("c"), &p("parentOf"), &p("d")),
        ]);
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();

        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        // Table the recursive IDB predicate so left/right recursion terminates.
        let table_preds = vec![p("ancestor")];
        let ans = run_scryer(&foreign, &world, &prog, &table_preds, &Budget::default()).unwrap();

        assert_eq!(ans.status, BudgetStatus::Ok);
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
        assert_eq!(ans.bindings.len(), 3, "expected exactly 3 answers: {ys:?}");
    }

    // ── Cyclic EDB terminates under tabling ─────────────────────────────────────────

    #[test]
    fn cyclic_edb_tabled_terminates() {
        let (store, world) = make_foreign(&[
            (&p("a"), &p("parentOf"), &p("b")),
            (&p("b"), &p("parentOf"), &p("a")),
        ]);
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let ans = run_scryer(
            &foreign,
            &world,
            &prog,
            &[p("ancestor")],
            &Budget::default(),
        )
        .unwrap();
        // Must terminate; both a and b reachable in a 2-cycle.
        let ys: Vec<&str> = ans.bindings.iter().map(|b| b["Y"].as_str()).collect();
        assert!(
            ys.contains(&format!("<{BASE}b>").as_str()),
            "must find b: {ys:?}"
        );
        assert!(
            ys.contains(&format!("<{BASE}a>").as_str()),
            "must find a: {ys:?}"
        );
    }

    // ── oracle ≡ engine (secondary differential check) ──────────────────────────────

    #[test]
    fn oracle_equals_engine_transitive_closure() {
        let (store, world) = make_foreign(&[
            (&p("a"), &p("parentOf"), &p("b")),
            (&p("b"), &p("parentOf"), &p("c")),
            (&p("c"), &p("parentOf"), &p("d")),
        ]);
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();

        let oracle = resolve(&foreign, &world, &prog, &Budget::default()).unwrap();
        let engine = run_scryer(
            &foreign,
            &world,
            &prog,
            &[p("ancestor")],
            &Budget::default(),
        )
        .unwrap();

        assert_eq!(
            oracle.bindings, engine.bindings,
            "oracle and Scryer engine must agree on the answer set"
        );
    }

    // ── Budget: max_answers → Partial ───────────────────────────────────────────────

    #[test]
    fn budget_max_answers_partial() {
        let (store, world) = make_foreign(&[
            (&p("a"), &p("parentOf"), &p("b")),
            (&p("b"), &p("parentOf"), &p("c")),
            (&p("c"), &p("parentOf"), &p("d")),
        ]);
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget {
            max_answers: Some(1),
            ..Default::default()
        };
        let ans = run_scryer(&foreign, &world, &prog, &[p("ancestor")], &budget).unwrap();
        assert_eq!(ans.bindings.len(), 1);
        assert_eq!(ans.status, BudgetStatus::Partial);
    }

    // ── Budget: inference limit → Exhausted (no-hang backstop on un-tabled recursion) ─

    #[test]
    fn budget_inference_limit_exhausted_no_hang() {
        // Left-recursive rule with NO tabling would loop forever; the inference-limit
        // backstop must catch it and stamp Exhausted (proving the engine cannot hang).
        let (store, world) = make_foreign(&[(&p("a"), &p("parentOf"), &p("b"))]);
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:loop(X, Y) :- ex:loop(X, Z), ex:parentOf(Z, Y).\n\
             ex:loop(X, Y) :- ex:parentOf(X, Y).\n\
             ?- ex:loop(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget {
            max_steps: Some(50_000), // small ceiling — must trip before any hang
            ..Default::default()
        };
        // NO tabling — relies entirely on the inference-limit backstop.
        let ans = run_scryer(&foreign, &world, &prog, &[], &budget).unwrap();
        assert_eq!(
            ans.status,
            BudgetStatus::Exhausted,
            "left-recursive un-tabled goal must trip the inference-limit backstop"
        );
    }
}
