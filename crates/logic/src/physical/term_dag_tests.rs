// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The structured-term arena's invariant suite.
//!
//! The arena itself lives in the reasoner-free [`gmeow_term_arena`] crate; this suite
//! lives HERE because its load-bearing member — `dag_congruent_with_ir_content_key` — pins
//! the congruence between the DAG's netstring fold and
//! [`gmeow_logic_compile::ir::Formula::content_key`], which it can only do by running the
//! three-consumer lowering [`crate::physical::lower::lower_logic_formula`]. Moving that
//! half into the arena crate would drag the compiler IR (and its `logic:` frontend) into a
//! substrate crate that exists precisely to carry neither. The rest of the suite stays
//! with it so the arena's invariants are asserted in ONE place rather than split across
//! two crates.
//!
//! The arena's *façade* is separately exercised from outside its crate, in
//! `crates/term-arena/tests/facade.rs`.

use gmeow_logic_compile::ir::{Formula, Term};
use gmeow_term_arena::ContentKey;
use gmeow_term_arena::engine::{NodeData, NodeId, TermDag};
use proptest::prelude::*;
use purrdf::TermValue;

fn config() -> ProptestConfig {
    let cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(256);
    ProptestConfig {
        cases,
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

fn iri(s: &str) -> TermValue {
    TermValue::iri(s)
}

// ── (a) alpha-normalization / interning: same de-Bruijn structure interns once ────

#[test]
fn dag_alpha_equal_terms_intern_to_one_node_and_key() {
    // `∀_. p(bound{0,0})` built twice — via independently-constructed child nodes —
    // must intern to ONE NodeId with a byte-identical content key.
    let build = |dag: &mut TermDag| {
        let p = dag.intern_leaf(iri("https://example.org/p"));
        let bound = dag.intern_bound(0, 0);
        let body = dag.intern_app(p, vec![bound]);
        let sort = dag.intern_leaf(iri("https://example.org/Sort"));
        let forall = dag.intern_leaf(iri("https://example.org/forall"));
        dag.intern_binder(forall, vec![sort], body)
    };

    let mut dag = TermDag::new();
    let first = build(&mut dag);
    let len_after_first = dag.len();
    let second = build(&mut dag);

    assert_eq!(first, second, "alpha-equal terms must share one NodeId");
    assert_eq!(
        dag.len(),
        len_after_first,
        "re-interning the same structure must mint no new nodes"
    );
    assert_eq!(
        dag.key(first),
        dag.key(second),
        "alpha-equal terms must have byte-identical content keys"
    );
}

// ── (b) negative alpha: a binder differing only in a `sorts` child differs ─────────

#[test]
fn dag_binder_differing_in_sort_is_distinct() {
    let mut dag = TermDag::new();
    let forall = dag.intern_leaf(iri("https://example.org/forall"));
    let body = dag.intern_bound(0, 0);
    let sort_a = dag.intern_leaf(iri("https://example.org/A"));
    let sort_b = dag.intern_leaf(iri("https://example.org/B"));

    let binder_a = dag.intern_binder(forall, vec![sort_a], body);
    let binder_b = dag.intern_binder(forall, vec![sort_b], body);

    assert_ne!(
        binder_a, binder_b,
        "binders over distinct sorts must be distinct nodes"
    );
    assert_ne!(
        dag.key(binder_a),
        dag.key(binder_b),
        "binders over distinct sorts must have distinct content keys"
    );
}

// ── metavariable identity + cached free-metavar set ────────────────────────────────

#[test]
fn dag_metavars_are_identity_bearing_and_tracked() {
    let mut dag = TermDag::new();
    let (m0, n0) = dag.fresh_meta();
    let (m1, n1) = dag.fresh_meta();
    assert_ne!(m0, m1, "each fresh_meta mints a distinct metavariable");
    assert_ne!(n0, n1, "distinct metavariables are distinct nodes");
    assert_ne!(dag.key(n0), dag.key(n1), "distinct metavar keys");

    // Re-interning the SAME metavariable shares one node.
    let n0_again = dag.intern(NodeData::Meta(m0));
    assert_eq!(n0, n0_again, "the same MetaId shares one node");

    // Free-metavar sets: a leaf is empty; a metavar is its singleton; an application
    // is the union of its children's sets.
    let p = dag.intern_leaf(iri("https://example.org/p"));
    assert!(dag.free_meta(p).is_empty());
    assert!(dag.free_meta(n0).contains(m0));
    assert!(!dag.free_meta(n0).contains(m1));

    let app = dag.intern_app(p, vec![n0, n1]);
    let fm = dag.free_meta(app);
    assert_eq!(fm.len(), 2, "app over two distinct metavars has both free");
    assert!(fm.contains(m0) && fm.contains(m1));
    assert_eq!(fm.iter().collect::<Vec<_>>(), vec![m0, m1]);

    // A metavariable stays free through binder scope (occurs-check contract).
    let sort = dag.intern_leaf(iri("https://example.org/Sort"));
    let forall = dag.intern_leaf(iri("https://example.org/forall"));
    let binder = dag.intern_binder(forall, vec![sort], n0);
    assert!(
        dag.free_meta(binder).contains(m0),
        "object binders do not bind metavariables"
    );
}

// ── (c) anti-collision fuzz: distinct structure ⟺ distinct key ⟺ distinct NodeId ──

/// A generator spec that maps INJECTIVELY onto [`NodeData`] trees: two specs are
/// `PartialEq`-equal exactly when they build the same node.  The collision property
/// then reduces to: spec-equality ⟺ key-equality ⟺ NodeId-equality.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Spec {
    Leaf(String),
    Free(String),
    Bound(u32, u16),
    App(Box<Spec>, Vec<Spec>),
    Binder(Box<Spec>, Vec<Spec>, Box<Spec>),
}

fn build_spec(dag: &mut TermDag, spec: &Spec) -> NodeId {
    match spec {
        Spec::Leaf(s) => dag.intern_leaf(TermValue::simple_literal(s.clone())),
        Spec::Free(s) => dag.intern_free(TermValue::simple_literal(s.clone())),
        Spec::Bound(d, slot) => dag.intern_bound(*d, *slot),
        Spec::App(op, args) => {
            let op = build_spec(dag, op);
            let args = args.iter().map(|a| build_spec(dag, a)).collect();
            dag.intern_app(op, args)
        }
        Spec::Binder(op, sorts, body) => {
            let op = build_spec(dag, op);
            let sorts = sorts.iter().map(|s| build_spec(dag, s)).collect();
            let body = build_spec(dag, body);
            dag.intern_binder(op, sorts, body)
        }
    }
}

/// Adversarial leaf/free bytes: strings that embed the netstring separators (`:`),
/// mimic kind tags (`I`/`V`/`APP`/`BIND`), embed a decimal-then-colon length prefix,
/// or carry a NUL — exactly the bytes a bare-separator scheme would conflate.
fn arb_leaf_bytes() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just("I".to_owned()),
        Just("V".to_owned()),
        Just("APP".to_owned()),
        Just("BIND".to_owned()),
        Just("3:foo".to_owned()),
        Just("1:0".to_owned()),
        Just(":".to_owned()),
        Just("5:".to_owned()),
        Just("free_".to_owned()),
        Just("a\u{0}b".to_owned()),
        Just("\u{0}".to_owned()),
        "[0-9:IVAPPBIND\u{0}a-z]{0,6}",
    ]
}

fn arb_spec() -> impl Strategy<Value = Spec> {
    let leaf = prop_oneof![
        arb_leaf_bytes().prop_map(Spec::Leaf),
        arb_leaf_bytes().prop_map(Spec::Free),
        (0u32..3, 0u16..3).prop_map(|(d, s)| Spec::Bound(d, s)),
    ];
    leaf.prop_recursive(4, 32, 3, |inner| {
        prop_oneof![
            (inner.clone(), prop::collection::vec(inner.clone(), 0..3))
                .prop_map(|(op, args)| Spec::App(Box::new(op), args)),
            (
                inner.clone(),
                prop::collection::vec(inner.clone(), 0..3),
                inner.clone()
            )
                .prop_map(|(op, sorts, body)| Spec::Binder(
                    Box::new(op),
                    sorts,
                    Box::new(body)
                )),
        ]
    })
}

proptest! {
    #![proptest_config(config())]

    /// The core injectivity property: over adversarial structures, spec-equality,
    /// content-key equality, and `NodeId` equality all coincide.  A `s1 != s2` pair
    /// that shared a key (`k1 == k2`) would be an encoding COLLISION; a `s1 == s2`
    /// pair that got two ids would be an interning bug.  Both are caught here.
    #[test]
    fn dag_key_is_injective_and_interns(s1 in arb_spec(), s2 in arb_spec()) {
        let mut dag = TermDag::new();
        let id1 = build_spec(&mut dag, &s1);
        let id2 = build_spec(&mut dag, &s2);
        let k1 = dag.key(id1).to_owned();
        let k2 = dag.key(id2).to_owned();

        prop_assert_eq!(
            s1 == s2,
            k1 == k2,
            "structural equality must coincide with content-key equality (no collision)"
        );
        prop_assert_eq!(
            s1 == s2,
            id1 == id2,
            "structural equality must coincide with hash-consed NodeId equality"
        );
    }
}

#[test]
fn dag_meta_keys_are_injective() {
    // A focused check that identity-bearing metavariable ordinals never collide with
    // one another (the proptest corpus above deliberately omits Meta, whose id cannot
    // be pinned through `fresh_meta`).
    let mut dag = TermDag::new();
    let mut keys = std::collections::HashSet::new();
    let mut ids = std::collections::HashSet::new();
    for _ in 0..64 {
        let (_, node) = dag.fresh_meta();
        assert!(
            keys.insert(dag.key(node).to_owned()),
            "metavar keys are distinct"
        );
        assert!(ids.insert(node), "metavar nodes are distinct");
    }
}

// ── G3: contains_node validates arena identity, not just an in-range index ─────────

#[test]
fn g3_contains_node_rejects_foreign_arena() {
    // Two independent arenas that intern the SAME leaf content mint the SAME slot
    // index (0) — an index-range-only `contains_node` would wrongly alias `dag_b`'s
    // node as a member of `dag_a`. The arena brand must reject it.
    let mut dag_a = TermDag::new();
    let mut dag_b = TermDag::new();

    let node_a = dag_a.intern_leaf(iri("https://example.org/a"));
    let node_b = dag_b.intern_leaf(iri("https://example.org/a"));
    assert_eq!(
        node_a.index(),
        node_b.index(),
        "same slot index across independent arenas (test setup)"
    );
    assert_ne!(
        dag_a.arena(),
        dag_b.arena(),
        "independent TermDags mint distinct arena brands"
    );

    assert!(
        dag_a.contains_node(node_a, dag_a.arena()),
        "a node validated against the arena that minted it is accepted"
    );
    assert!(
        !dag_a.contains_node(node_b, dag_b.arena()),
        "a foreign NodeId — even one that is in-bounds and carries ITS OWN (correct) \
         arena brand — is rejected by a DIFFERENT dag: the brand check, not the bounds \
         check, is what makes membership an identity test rather than an index-range \
         coincidence"
    );
}

// ── (d) DAG ↔ ir.rs congruence ─────────────────────────────────────────────────────
//
// The `logic:` lowering the congruence corpus exercises is the promoted, non-test
// three-consumer API [`crate::physical::lower::lower_logic_formula`], which
// reproduces exactly the equivalences `ir::Formula::content_key` decides. The
// focused helper that once lived here has been removed (greenfield: one lowering, not
// a test-local duplicate).

fn tvar(name: &str) -> Term {
    Term::var(name).expect("non-empty var name")
}

fn tiri(iri: &str) -> Term {
    Term::iri(iri).expect("non-empty iri")
}

fn atom(relation: &str, args: Vec<Term>) -> Formula {
    Formula::atom(tiri(relation), args).expect("iri relation")
}

#[test]
fn dag_congruent_with_ir_content_key() {
    const P: &str = "https://example.org/p";
    const Q: &str = "https://example.org/q";
    const R: &str = "https://example.org/r";
    const A: &str = "https://example.org/a";
    const B: &str = "https://example.org/b";

    // A corpus spanning Atom / And / Or / Not / Implies / Iff / Forall / Exists / Var /
    // Iri / Literal, including alpha-variants and commutative-variants (which must
    // COLLAPSE) and sign/order/arity variants (which must stay DISTINCT).
    let lit = Term::literal("v", None).expect("literal");
    let corpus: Vec<(&str, Formula)> = vec![
        ("atom_pab", atom(P, vec![tiri(A), tiri(B)])),
        ("atom_pba", atom(P, vec![tiri(B), tiri(A)])),
        ("atom_lit", atom(P, vec![tiri(A), lit])),
        (
            "and_pq",
            Formula::And(vec![atom(P, vec![tiri(A)]), atom(Q, vec![tiri(A)])]),
        ),
        (
            "and_qp",
            Formula::And(vec![atom(Q, vec![tiri(A)]), atom(P, vec![tiri(A)])]),
        ),
        (
            "and_nested",
            Formula::And(vec![
                Formula::And(vec![atom(P, vec![tiri(A)]), atom(Q, vec![tiri(A)])]),
                atom(R, vec![tiri(A)]),
            ]),
        ),
        (
            "and_flat3",
            Formula::And(vec![
                atom(P, vec![tiri(A)]),
                atom(Q, vec![tiri(A)]),
                atom(R, vec![tiri(A)]),
            ]),
        ),
        (
            "or_pq",
            Formula::Or(vec![atom(P, vec![tiri(A)]), atom(Q, vec![tiri(A)])]),
        ),
        ("not_pa", Formula::Not(Box::new(atom(P, vec![tiri(A)])))),
        (
            "impl_pq",
            Formula::Implies(
                Box::new(atom(P, vec![tiri(A)])),
                Box::new(atom(Q, vec![tiri(A)])),
            ),
        ),
        (
            "impl_qp",
            Formula::Implies(
                Box::new(atom(Q, vec![tiri(A)])),
                Box::new(atom(P, vec![tiri(A)])),
            ),
        ),
        (
            "iff_pq",
            Formula::Iff(
                Box::new(atom(P, vec![tiri(A)])),
                Box::new(atom(Q, vec![tiri(A)])),
            ),
        ),
        (
            "iff_qp",
            Formula::Iff(
                Box::new(atom(Q, vec![tiri(A)])),
                Box::new(atom(P, vec![tiri(A)])),
            ),
        ),
        (
            "forall_x_px",
            Formula::Forall {
                vars: vec!["x".to_owned()],
                body: Box::new(atom(P, vec![tvar("x")])),
            },
        ),
        (
            "forall_y_py",
            Formula::Forall {
                vars: vec!["y".to_owned()],
                body: Box::new(atom(P, vec![tvar("y")])),
            },
        ),
        (
            "exists_x_px",
            Formula::Exists {
                vars: vec!["x".to_owned()],
                body: Box::new(atom(P, vec![tvar("x")])),
            },
        ),
        (
            "forall_xy_rxy",
            Formula::Forall {
                vars: vec!["x".to_owned(), "y".to_owned()],
                body: Box::new(atom(R, vec![tvar("x"), tvar("y")])),
            },
        ),
        (
            "forall_uv_ruv",
            Formula::Forall {
                vars: vec!["u".to_owned(), "v".to_owned()],
                body: Box::new(atom(R, vec![tvar("u"), tvar("v")])),
            },
        ),
        (
            "forall_xy_ryx",
            Formula::Forall {
                vars: vec!["x".to_owned(), "y".to_owned()],
                body: Box::new(atom(R, vec![tvar("y"), tvar("x")])),
            },
        ),
        (
            "forall_x_forall_y_rxy",
            Formula::Forall {
                vars: vec!["x".to_owned()],
                body: Box::new(Formula::Forall {
                    vars: vec!["y".to_owned()],
                    body: Box::new(atom(R, vec![tvar("x"), tvar("y")])),
                }),
            },
        ),
        (
            "forall_a_forall_b_rab",
            Formula::Forall {
                vars: vec!["a".to_owned()],
                body: Box::new(Formula::Forall {
                    vars: vec!["b".to_owned()],
                    body: Box::new(atom(R, vec![tvar("a"), tvar("b")])),
                }),
            },
        ),
        (
            "forall_free_y_pxy",
            // `x` is free (never bound), `y` is bound — free-variable identity is by
            // NAME and must NOT be alpha-collapsed.
            Formula::Forall {
                vars: vec!["y".to_owned()],
                body: Box::new(atom(R, vec![tvar("x"), tvar("y")])),
            },
        ),
    ];

    // Lower the whole corpus into ONE shared DAG so NodeId equality is comparable.
    let mut dag = TermDag::new();
    let mut lowered: Vec<(&str, NodeId, ContentKey)> = Vec::with_capacity(corpus.len());
    for (label, formula) in &corpus {
        let node = crate::physical::lower::lower_logic_formula(&mut dag, formula)
            .unwrap_or_else(|e| panic!("lowering {label} failed: {e:?}"));
        lowered.push((label, node, formula.content_key()));
    }

    // The biconditional over every ordered pair: alpha/commutative-equal ⟺ same key
    // ⟺ same NodeId.
    for (la, na, ka) in &lowered {
        for (lb, nb, kb) in &lowered {
            let ir_eq = ka == kb;
            let dag_eq = na == nb;
            assert_eq!(
                ir_eq, dag_eq,
                "congruence violated for ({la}, {lb}): ir_key_eq={ir_eq} dag_node_eq={dag_eq}\n\
                 ka={ka}\nkb={kb}"
            );
        }
    }

    // Sanity: the intended collapses and separations are actually present (guards
    // against a vacuous corpus where every pair is trivially distinct).
    let node = |name: &str| lowered.iter().find(|(l, ..)| *l == name).unwrap().1;
    assert_eq!(node("and_pq"), node("and_qp"), "And is commutative");
    assert_eq!(node("and_nested"), node("and_flat3"), "And is associative");
    assert_eq!(node("iff_pq"), node("iff_qp"), "Iff is commutative");
    assert_eq!(node("forall_x_px"), node("forall_y_py"), "alpha-equal ∀");
    assert_eq!(
        node("forall_x_forall_y_rxy"),
        node("forall_a_forall_b_rab"),
        "alpha-equal nested ∀"
    );
    assert_eq!(
        node("forall_xy_rxy"),
        node("forall_uv_ruv"),
        "alpha-equal 2-var ∀"
    );
    assert_ne!(node("atom_pab"), node("atom_pba"), "arg order matters");
    assert_ne!(node("impl_pq"), node("impl_qp"), "Implies is ordered");
    assert_ne!(
        node("forall_x_px"),
        node("exists_x_px"),
        "∀ and ∃ are distinct"
    );
    assert_ne!(
        node("forall_xy_rxy"),
        node("forall_xy_ryx"),
        "bound-var order in body matters"
    );
}
