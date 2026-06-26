// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! S8 #914 parity harness: in-engine ≡ Scryer-lowered property-path evaluation.
//!
//! The acceptance criterion for #914 is that the two implementations of one
//! semantics agree on the corpus property-path shapes:
//!
//! - the **in-engine** wasm-safe evaluator (`gmeow_sparql_eval`, via the public
//!   `eval` entry over a `GraphPattern::Path`), and
//! - the **lowered** Scryer tabling engine (`gmeow_logic::sparql_path_lower`).
//!
//! For each fixture (graph + path + endpoint binding) both sides are reduced to a
//! sorted `BTreeSet<(subject, object)>` in the SAME canonical `term_n3` string
//! space (`<iri>`) and asserted equal. Fixtures cover the corpus shapes
//! (`subClassOf*`, `subPropertyOf+`, `moreSevereThan+`, list-walk `members/rest*/
//! first`, temporal `(before|^after)+`, `label|name`) plus bounded `{n,m}`, over
//! BOTH acyclic and cyclic graphs — the cyclic cases are where a naive closure or a
//! wrong `{n,m}` algorithm would diverge.
//!
//! Scope note: the lowering models the subject/object endpoints as independent, so
//! the same-variable reflexive case (`?x p ?x`) and the not-lowerable
//! negated-set/wildcard paths are exercised in `gmeow_sparql_eval`'s own unit tests
//! (in-engine only), not here.

use std::collections::BTreeSet;
use std::sync::Arc;

use gmeow_logic::sparql_path_lower::{evaluate_path_lowered, PathEnd};
use gmeow_rdf_core::{RdfDataset, RdfDatasetBuilder, TermRef};
use gmeow_sparql_algebra::{
    GraphPattern, NamedNode, PropertyPathExpression, TermPattern, Variable,
};
use gmeow_sparql_eval::{eval, EvalCtx, SolutionTerm};

const EX: &str = "https://example.org/";

fn full(local: &str) -> String {
    format!("{EX}{local}")
}

fn n3(iri: &str) -> String {
    format!("<{iri}>")
}

/// A fixture endpoint: a free variable, or a ground IRI given by its local name.
#[derive(Clone, Copy)]
enum End {
    Var(&'static str),
    Iri(&'static str),
}

fn named(local: &str) -> PropertyPathExpression {
    PropertyPathExpression::NamedNode(NamedNode::new_unchecked(full(local)))
}

fn term_pattern(e: End) -> TermPattern {
    match e {
        End::Var(n) => TermPattern::Variable(Variable::new(n)),
        End::Iri(l) => TermPattern::NamedNode(NamedNode::new_unchecked(full(l))),
    }
}

fn path_end(e: End) -> PathEnd {
    match e {
        End::Var(_) => PathEnd::Variable,
        End::Iri(l) => PathEnd::Iri(full(l)),
    }
}

/// Build the IR dataset for the in-engine evaluator from `(s, p, o)` local-name edges.
fn build_dataset(edges: &[(&str, &str, &str)]) -> Arc<RdfDataset> {
    let mut b = RdfDatasetBuilder::new();
    for (s, p, o) in edges {
        let s = b.intern_iri(full(s));
        let p = b.intern_iri(full(p));
        let o = b.intern_iri(full(o));
        b.push_quad(s, p, o, None);
    }
    b.freeze().expect("freeze")
}

/// The same edges as full-IRI triples for the lowered evaluator.
fn edges_full(edges: &[(&str, &str, &str)]) -> Vec<(String, String, String)> {
    edges
        .iter()
        .map(|(s, p, o)| (full(s), full(p), full(o)))
        .collect()
}

/// Resolve a solution cell to its canonical `term_n3` string (tests use IRIs).
fn cell_n3(ds: &RdfDataset, cell: Option<SolutionTerm>) -> Option<String> {
    match cell {
        Some(SolutionTerm::Existing(id)) => match ds.resolve(id) {
            TermRef::Iri(s) => Some(n3(s)),
            other => Some(format!("{other:?}")),
        },
        _ => None,
    }
}

/// The in-engine `(subject, object)` pair set in canonical form.
fn in_engine_pairs(
    ds: &RdfDataset,
    path: &PropertyPathExpression,
    subj: End,
    obj: End,
) -> BTreeSet<(String, String)> {
    let pattern = GraphPattern::Path {
        subject: term_pattern(subj),
        path: path.clone(),
        object: term_pattern(obj),
    };
    let mut ctx = EvalCtx::new(ds);
    let seq = eval(&pattern, &mut ctx).expect("in-engine path eval");

    let s_col = match subj {
        End::Var(n) => Some(seq.schema.index_of(&Variable::new(n)).expect("subject col")),
        End::Iri(_) => None,
    };
    let o_col = match obj {
        End::Var(n) => Some(seq.schema.index_of(&Variable::new(n)).expect("object col")),
        End::Iri(_) => None,
    };

    let mut out = BTreeSet::new();
    for row in &seq.rows {
        let s = match subj {
            End::Iri(l) => Some(n3(&full(l))),
            End::Var(_) => cell_n3(ds, row[s_col.unwrap()]),
        };
        let o = match obj {
            End::Iri(l) => Some(n3(&full(l))),
            End::Var(_) => cell_n3(ds, row[o_col.unwrap()]),
        };
        if let (Some(s), Some(o)) = (s, o) {
            out.insert((s, o));
        }
    }
    out
}

/// Assert the in-engine and lowered evaluators agree on this fixture.
fn assert_parity(edges: &[(&str, &str, &str)], path: &PropertyPathExpression, subj: End, obj: End) {
    let ds = build_dataset(edges);
    let in_engine = in_engine_pairs(&ds, path, subj, obj);
    let lowered = evaluate_path_lowered(&edges_full(edges), path, &path_end(subj), &path_end(obj))
        .expect("lowered path eval");
    assert_eq!(
        in_engine, lowered,
        "in-engine vs lowered parity mismatch for {path:?}"
    );
}

// ── Recursive closure shapes (the corpus drivers) ────────────────────────────

#[test]
fn parity_zero_or_more_subclass_acyclic() {
    // `?k rdfs:subClassOf* :Agent` shape: object ground, subject var, over a taxonomy.
    let edges = [
        ("Dog", "subClassOf", "Mammal"),
        ("Mammal", "subClassOf", "Animal"),
        ("Animal", "subClassOf", "Agent"),
    ];
    let star = PropertyPathExpression::ZeroOrMore(Box::new(named("subClassOf")));
    assert_parity(&edges, &star, End::Var("k"), End::Iri("Agent"));
}

#[test]
fn parity_one_or_more_subject_var_acyclic() {
    // `?top :moreSevereThan+ :minor` shape: object ground, subject var.
    let edges = [
        ("critical", "moreSevereThan", "major"),
        ("major", "moreSevereThan", "minor"),
    ];
    let plus = PropertyPathExpression::OneOrMore(Box::new(named("moreSevereThan")));
    assert_parity(&edges, &plus, End::Var("top"), End::Iri("minor"));
}

#[test]
fn parity_one_or_more_both_ground() {
    // `:propagatesFrom rdfs:subPropertyOf+ :wasDerivedFrom` shape: both ground (ASK).
    let edges = [
        ("propagatesFrom", "subPropertyOf", "influencedBy"),
        ("influencedBy", "subPropertyOf", "wasDerivedFrom"),
    ];
    let plus = PropertyPathExpression::OneOrMore(Box::new(named("subPropertyOf")));
    assert_parity(
        &edges,
        &plus,
        End::Iri("propagatesFrom"),
        End::Iri("wasDerivedFrom"),
    );
    // The negative case (no path) must also agree.
    assert_parity(
        &edges,
        &plus,
        End::Iri("wasDerivedFrom"),
        End::Iri("propagatesFrom"),
    );
}

#[test]
fn parity_closure_on_cycle() {
    // Cyclic graph: + and * must terminate and agree.
    let edges = [("a", "p", "b"), ("b", "p", "c"), ("c", "p", "a")];
    let plus = PropertyPathExpression::OneOrMore(Box::new(named("p")));
    assert_parity(&edges, &plus, End::Var("s"), End::Iri("a"));
    assert_parity(&edges, &plus, End::Iri("a"), End::Var("o"));
    let star = PropertyPathExpression::ZeroOrMore(Box::new(named("p")));
    assert_parity(&edges, &star, End::Iri("a"), End::Var("o"));
}

// ── Sequence / alternative / reverse (corpus non-recursive shapes) ───────────

#[test]
fn parity_sequence_list_walk() {
    // `:axiom :members/:rest*/:first ?x` — RDF-list walk.
    let edges = [
        ("axiom", "members", "l0"),
        ("l0", "first", "A"),
        ("l0", "rest", "l1"),
        ("l1", "first", "B"),
        ("l1", "rest", "l2"),
        ("l2", "first", "C"),
    ];
    let rest_star = PropertyPathExpression::ZeroOrMore(Box::new(named("rest")));
    let path = PropertyPathExpression::Sequence(
        Box::new(named("members")),
        Box::new(PropertyPathExpression::Sequence(
            Box::new(rest_star),
            Box::new(named("first")),
        )),
    );
    assert_parity(&edges, &path, End::Iri("axiom"), End::Var("x"));
}

#[test]
fn parity_alternative_label_or_name() {
    // `?r rdfs:label|:name ?t` shape.
    let edges = [("r1", "label", "t1"), ("r2", "name", "t2")];
    let alt =
        PropertyPathExpression::Alternative(Box::new(named("label")), Box::new(named("name")));
    assert_parity(&edges, &alt, End::Var("r"), End::Var("t"));
}

#[test]
fn parity_nested_alternative_inverse_plus_temporal() {
    // `(:before|^:after)+` — temporal closure with an inverse leg, on a cycle-free graph.
    let edges = [("e1", "before", "e2"), ("e3", "after", "e2")];
    let alt = PropertyPathExpression::Alternative(
        Box::new(named("before")),
        Box::new(PropertyPathExpression::Reverse(Box::new(named("after")))),
    );
    let plus = PropertyPathExpression::OneOrMore(Box::new(alt));
    assert_parity(&edges, &plus, End::Iri("e1"), End::Var("o"));
}

#[test]
fn parity_reverse_backward() {
    let edges = [("a", "p", "b"), ("c", "p", "b")];
    let rev = PropertyPathExpression::Reverse(Box::new(named("p")));
    // `:b ^:p ?o` → a, c.
    assert_parity(&edges, &rev, End::Iri("b"), End::Var("o"));
}

// ── Bounded range {n,m} (GMEOW extension), incl. on a cycle ──────────────────

#[test]
fn parity_range_bounded_acyclic() {
    let edges = [
        ("a", "p", "b"),
        ("b", "p", "c"),
        ("c", "p", "d"),
        ("d", "p", "e"),
    ];
    let rng = |min, max| PropertyPathExpression::Range {
        inner: Box::new(named("p")),
        min,
        max,
    };
    assert_parity(&edges, &rng(2, Some(2)), End::Iri("a"), End::Var("o"));
    assert_parity(&edges, &rng(0, Some(2)), End::Iri("a"), End::Var("o"));
    assert_parity(&edges, &rng(2, None), End::Iri("a"), End::Var("o"));
}

#[test]
fn parity_range_on_cycle() {
    // 2-cycle a<->b: p{2,4} reaches nodes at multiple repetition counts.
    let edges = [("a", "p", "b"), ("b", "p", "a")];
    let rng = PropertyPathExpression::Range {
        inner: Box::new(named("p")),
        min: 2,
        max: Some(4),
    };
    assert_parity(&edges, &rng, End::Iri("a"), End::Var("o"));
}

// ── Both-variable enumeration (distinct vars) ────────────────────────────────

#[test]
fn parity_both_variable_star() {
    // `?s :p* ?o` with distinct vars, incl. zero-length self-pairs over the node universe.
    let edges = [("a", "p", "b"), ("b", "p", "c"), ("x", "q", "y")];
    let star = PropertyPathExpression::ZeroOrMore(Box::new(named("p")));
    assert_parity(&edges, &star, End::Var("s"), End::Var("o"));
}
