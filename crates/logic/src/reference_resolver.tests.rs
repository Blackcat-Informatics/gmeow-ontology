// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::query_ir::parse_query_program;
use crate::seam::WorldFactSnapshot;
use crate::store::WorldStore;
use gmeow_term_arena::engine::StructNodeParts;

const W: &str = "http://logic.test/world/resolver";
const PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";

// ── Helpers ───────────────────────────────────────────────────────────────

fn make_world(triples: &[(&str, &str, &str)]) -> (WorldStore, String) {
    let store = WorldStore::new();
    for (s, p, o) in triples {
        store.insert_quad(W, s, p, o);
    }
    (store, W.to_owned())
}

// ── Test 1: Non-recursive EDB lookup + single IDB rule ───────────────────

#[test]
fn non_recursive_single_rule() {
    // EDB: parentOf(alice, bob)
    // Rule: ancestorOf(X,Y) :- parentOf(X,Y).
    // Goal: ?- ancestorOf(alice, Y).
    // Expected: Y = <.../bob>

    let base = "https://example.org/";
    let (store, world_nn) = make_world(&[(
        &format!("{base}alice"),
        &format!("{base}parentOf"),
        &format!("{base}bob"),
    )]);

    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();

    let src = format!(
        ":- prefix(ex, '{base}').\n\
             ex:ancestorOf(X, Y) :- ex:parentOf(X, Y).\n\
             ?- ex:ancestorOf(ex:alice, Y).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let budget = Budget::default();
    let ans = resolve(&foreign, &world_nn, &prog, &budget).unwrap();

    assert_eq!(ans.status, BudgetStatus::Ok);
    assert_eq!(ans.bindings.len(), 1);
    assert_eq!(ans.bindings[0]["Y"], format!("<{base}bob>"));
}

// ── Test 2a: Recursive transitive closure ─────────────────────────────────

#[test]
fn recursive_transitive_closure_chain() {
    // EDB: parentOf(a,b), parentOf(b,c), parentOf(c,d)
    // Rules: ancestor(X,Y):-parentOf(X,Y). ancestor(X,Y):-parentOf(X,Z),ancestor(Z,Y).
    // Goal: ?- ancestor(a, Y).
    // Expected: Y ∈ {b, c, d}

    let base = "https://example.org/";
    let (store, world_nn) = make_world(&[
        (
            &format!("{base}a"),
            &format!("{base}parentOf"),
            &format!("{base}b"),
        ),
        (
            &format!("{base}b"),
            &format!("{base}parentOf"),
            &format!("{base}c"),
        ),
        (
            &format!("{base}c"),
            &format!("{base}parentOf"),
            &format!("{base}d"),
        ),
    ]);

    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();

    let src = format!(
        ":- prefix(ex, '{base}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let budget = Budget::default();
    let ans = resolve(&foreign, &world_nn, &prog, &budget).unwrap();

    assert_eq!(ans.status, BudgetStatus::Ok);
    let ys: Vec<&str> = ans.bindings.iter().map(|b| b["Y"].as_str()).collect();
    assert!(
        ys.contains(&format!("<{base}b>").as_str()),
        "missing b: {ys:?}"
    );
    assert!(
        ys.contains(&format!("<{base}c>").as_str()),
        "missing c: {ys:?}"
    );
    assert!(
        ys.contains(&format!("<{base}d>").as_str()),
        "missing d: {ys:?}"
    );
    assert_eq!(ans.bindings.len(), 3, "expected exactly 3 answers: {ys:?}");
}

// ── Test 2b: Cyclic EDB — seen-memo prevents infinite loop ───────────────

#[test]
fn cyclic_edb_terminates() {
    // EDB: parentOf(a,b), parentOf(b,a)  ← cycle
    // Rules: ancestor(X,Y):-parentOf(X,Y). ancestor(X,Y):-parentOf(X,Z),ancestor(Z,Y).
    // Goal: ?- ancestor(a, Y).
    // The memo must prevent infinite looping; result is {b, a} (possibly duplicates
    // filtered by the memo).

    let base = "https://example.org/";
    let (store, world_nn) = make_world(&[
        (
            &format!("{base}a"),
            &format!("{base}parentOf"),
            &format!("{base}b"),
        ),
        (
            &format!("{base}b"),
            &format!("{base}parentOf"),
            &format!("{base}a"),
        ),
    ]);

    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();

    let src = format!(
        ":- prefix(ex, '{base}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let budget = Budget {
        max_steps: Some(500), // generous but finite
        ..Default::default()
    };
    // Must terminate — no timeout needed beyond the budget.
    let ans = resolve(&foreign, &world_nn, &prog, &budget);
    assert!(ans.is_ok(), "cyclic EDB must terminate: {ans:?}");
    let ans = ans.unwrap();
    // Must have found at least b (and possibly a itself) without panicking.
    let ys: Vec<&str> = ans.bindings.iter().map(|b| b["Y"].as_str()).collect();
    assert!(
        ys.contains(&format!("<{base}b>").as_str()),
        "must find b: {ys:?}"
    );
}

// ── Test 3: Budget — max_answers ──────────────────────────────────────────

#[test]
fn budget_max_answers_partial() {
    let base = "https://example.org/";
    let (store, world_nn) = make_world(&[
        (
            &format!("{base}a"),
            &format!("{base}parentOf"),
            &format!("{base}b"),
        ),
        (
            &format!("{base}b"),
            &format!("{base}parentOf"),
            &format!("{base}c"),
        ),
        (
            &format!("{base}c"),
            &format!("{base}parentOf"),
            &format!("{base}d"),
        ),
    ]);

    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();

    let src = format!(
        ":- prefix(ex, '{base}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let budget = Budget {
        max_answers: Some(1),
        ..Default::default()
    };
    let ans = resolve(&foreign, &world_nn, &prog, &budget).unwrap();

    assert_eq!(ans.bindings.len(), 1, "exactly 1 answer with budget=1");
    assert_eq!(ans.status, BudgetStatus::Partial);
}

// ── Test 4: Cut rejection ─────────────────────────────────────────────────

#[test]
fn cut_in_rule_returns_err() {
    let base = "https://example.org/";
    let (store, world_nn) = make_world(&[(
        &format!("{base}a"),
        &format!("{base}parentOf"),
        &format!("{base}b"),
    )]);

    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();

    let src = format!(
        ":- prefix(ex, '{base}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y), !, ex:ancestor(X, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let budget = Budget::default();
    let result = resolve(&foreign, &world_nn, &prog, &budget);

    assert!(result.is_err(), "cut must return Err");
    let err = result.unwrap_err();
    assert!(
        err.message().contains("cut is procedural"),
        "error must mention 'cut is procedural': {err:?}"
    );
}

// ── Arithmetic/comparison builtins in the declarative oracle ─────────────

#[test]
fn arithmetic_builtin_list_length_resolves() {
    // Over the list l0→l1→l2→nil (rdf:rest chain), len(l0) = 3 via the
    // recursive `N is M + 1` generator, now evaluated by the oracle in body order.
    let base = "https://example.org/";
    let rdf = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
    let (store, world_nn) = make_world(&[
        (
            &format!("{base}l0"),
            &format!("{rdf}rest"),
            &format!("{base}l1"),
        ),
        (
            &format!("{base}l1"),
            &format!("{rdf}rest"),
            &format!("{base}l2"),
        ),
        (
            &format!("{base}l2"),
            &format!("{rdf}rest"),
            &format!("{rdf}nil"),
        ),
    ]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let src = format!(
        ":- prefix(ex, '{base}').\n\
             :- prefix(rdf, '{rdf}').\n\
             ex:len(rdf:nil, 0).\n\
             ex:len(L, N) :- rdf:rest(L, R), ex:len(R, M), N is M + 1.\n\
             ?- ex:len(ex:l0, N).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let ans = resolve(&foreign, &world_nn, &prog, &Budget::default()).unwrap();
    assert_eq!(ans.status, BudgetStatus::Ok);
    assert_eq!(ans.bindings.len(), 1, "one length answer: {ans:?}");
    assert_eq!(
        ans.bindings[0]["N"],
        "\"3\"^^<http://www.w3.org/2001/XMLSchema#integer>"
    );
}

#[test]
fn comparison_builtin_filters_in_oracle() {
    // A comparison over a generated value keeps or prunes the branch.  The
    // passing rule (N = 5 > 4) yields an answer; the failing rule (N = 2 > 5)
    // yields none — proving `resolve_builtin` both generates and filters.
    let base = "https://example.org/";
    let (store, world_nn) = make_world(&[(
        &format!("{base}a"),
        &format!("{base}kind"),
        &format!("{base}item"),
    )]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();

    let pass_src = format!(
        ":- prefix(ex, '{base}').\n\
             ex:pass(X, N) :- ex:kind(X, ex:item), N is 2 + 3, N > 4.\n\
             ?- ex:pass(X, N).\n"
    );
    let pass = resolve(
        &foreign,
        &world_nn,
        &parse_query_program(&pass_src).unwrap(),
        &Budget::default(),
    )
    .unwrap();
    assert_eq!(pass.bindings.len(), 1, "5 > 4 keeps the branch: {pass:?}");
    assert_eq!(pass.bindings[0]["X"], format!("<{base}a>"));
    assert_eq!(
        pass.bindings[0]["N"],
        "\"5\"^^<http://www.w3.org/2001/XMLSchema#integer>"
    );

    let fail_src = format!(
        ":- prefix(ex, '{base}').\n\
             ex:blocked(X, N) :- ex:kind(X, ex:item), N is 1 + 1, N > 5.\n\
             ?- ex:blocked(X, N).\n"
    );
    let fail = resolve(
        &foreign,
        &world_nn,
        &parse_query_program(&fail_src).unwrap(),
        &Budget::default(),
    )
    .unwrap();
    assert_eq!(fail.bindings.len(), 0, "2 > 5 prunes the branch: {fail:?}");
}

#[test]
fn exact_rational_builtin_resolves_in_body_order() {
    // The reference oracle evaluates the exact-`/` generator in body order: the
    // scalar-ℚ family flows through the ONE shared evaluator on the demand path,
    // committing the normalized rational transport surface (6/4 → 3/2).
    let base = "https://example.org/";
    let (store, world_nn) = make_world(&[(
        &format!("{base}a"),
        &format!("{base}kind"),
        &format!("{base}item"),
    )]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let src = format!(
        ":- prefix(ex, '{base}').\n\
             ex:half(X, H) :- ex:kind(X, ex:item), H is 6 / 4.\n\
             ?- ex:half(X, H).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let ans = resolve(&foreign, &world_nn, &prog, &Budget::default()).unwrap();
    assert_eq!(ans.status, BudgetStatus::Ok);
    assert_eq!(ans.bindings.len(), 1, "one exact-rational answer: {ans:?}");
    assert_eq!(ans.bindings[0]["X"], format!("<{base}a>"));
    assert_eq!(
        ans.bindings[0]["H"],
        "\"3/2\"^^<urn:gmeow:transport:rational>"
    );
}

#[test]
fn dimension_composition_resolves_in_body_order() {
    // A dimension-composition generator (`D is A * B` over two SI dimension
    // vectors fed from EDB facts) evaluates in body order through the ONE shared
    // evaluator: length (L) ⊗ time (T) = the L·T exponent vector, committed on the
    // dimension transport surface.
    let base = "https://example.org/";
    let dim_dt = "urn:gmeow:transport:dimension";
    let store = WorldStore::new();
    // length = L¹ (index 0), time = T¹ (index 2), fixed SI order.
    let length = TermValue::typed_literal("1/1,0/1,0/1,0/1,0/1,0/1,0/1", dim_dt);
    let time = TermValue::typed_literal("0/1,0/1,1/1,0/1,0/1,0/1,0/1", dim_dt);
    store
        .insert_quad_terms(
            W,
            TermValue::iri(format!("{base}a")),
            TermValue::iri(format!("{base}dimLen")),
            length,
        )
        .unwrap();
    store
        .insert_quad_terms(
            W,
            TermValue::iri(format!("{base}a")),
            TermValue::iri(format!("{base}dimTime")),
            time,
        )
        .unwrap();
    let world_nn = W.to_owned();
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
    let src = format!(
        ":- prefix(ex, '{base}').\n\
             ex:compose(X, D) :- ex:dimLen(X, A), ex:dimTime(X, B), D is A * B.\n\
             ?- ex:compose(X, D).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let ans = resolve(&foreign, &world_nn, &prog, &Budget::default()).unwrap();
    assert_eq!(ans.status, BudgetStatus::Ok);
    assert_eq!(ans.bindings.len(), 1, "one composed dimension: {ans:?}");
    assert_eq!(ans.bindings[0]["X"], format!("<{base}a>"));
    assert_eq!(
        ans.bindings[0]["D"],
        "\"1/1,0/1,1/1,0/1,0/1,0/1,0/1\"^^<urn:gmeow:transport:dimension>"
    );
}

// ── G14: a Struct argument to the reference oracle hard-fails ────────────

/// Build a `QTerm::Struct` wrapping some arbitrary node in a fresh, disposable
/// `TermDag` — the reference oracle never dereferences the wrapped node (it is
/// supposed to reject the term BEFORE any lookup), so the arena's contents are
/// irrelevant; only the term's variant matters for this guard.
fn struct_term() -> QTerm {
    let mut dag = gmeow_term_arena::engine::TermDag::new();
    let node = dag.intern_leaf(TermValue::iri("https://example.org/opaque"));
    QTerm::Struct(crate::query_ir::StructNode::wrap(node, dag.arena()))
}

#[test]
fn struct_argument_to_edb_hard_fails() {
    let base = "https://example.org/";
    let (store, world_nn) = make_world(&[(
        &format!("{base}a"),
        &format!("{base}p"),
        &format!("{base}b"),
    )]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();

    // No rules ⇒ `p` is EDB.
    let src = format!(
        ":- prefix(ex, '{base}').\n\
             ?- ex:p(ex:a, ex:b).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let budget = Budget::default();
    let mut state = ResolveState {
        foreign: &foreign,
        world: &world_nn,
        program: &prog,
        idb: BTreeSet::new(),
        budget: &budget,
        answers: Vec::new(),
        steps: 0,
        status: BudgetStatus::Ok,
    };

    let atom = QAtom {
        pred: format!("{base}p"),
        args: vec![struct_term(), QTerm::Const(format!("<{base}b>"))],
    };
    let mut seen = BTreeSet::new();
    let result = state.resolve_edb(&atom, &[], &BTreeMap::new(), &mut seen);
    assert!(
        result.is_err(),
        "a Struct EDB subject must be a typed hard-fail, not a wildcard match: {result:?}"
    );
    assert!(
        result
            .unwrap_err()
            .message()
            .contains("structured (compound) term"),
        "the error must name the structured-term guard"
    );
}

#[test]
fn struct_argument_to_idb_hard_fails() {
    let base = "https://example.org/";
    let (store, world_nn) = make_world(&[(
        &format!("{base}a"),
        &format!("{base}parentOf"),
        &format!("{base}b"),
    )]);
    let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();

    let src = format!(
        ":- prefix(ex, '{base}').\n\
             ex:ancestorOf(X, Y) :- ex:parentOf(X, Y).\n\
             ?- ex:ancestorOf(ex:a, Y).\n"
    );
    let prog = parse_query_program(&src).unwrap();
    let budget = Budget::default();
    let pred = format!("{base}ancestorOf");
    let mut state = ResolveState {
        foreign: &foreign,
        world: &world_nn,
        program: &prog,
        idb: BTreeSet::from([pred.clone()]),
        budget: &budget,
        answers: Vec::new(),
        steps: 0,
        status: BudgetStatus::Ok,
    };

    let atom = QAtom {
        pred,
        args: vec![struct_term(), QTerm::Var("Y".to_owned())],
    };
    let mut seen = BTreeSet::new();
    let result = state.resolve_idb(&atom, &[], &BTreeMap::new(), &mut seen);
    assert!(
        result.is_err(),
        "a Struct IDB call argument must be a typed hard-fail, not a wildcard memo key: {result:?}"
    );
    assert!(
        result
            .unwrap_err()
            .message()
            .contains("structured (compound) term"),
        "the error must name the structured-term guard"
    );
}
