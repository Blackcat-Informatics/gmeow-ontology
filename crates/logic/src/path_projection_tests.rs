// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tests for the `logic:PathShape` projection (#1010): the property-path lowering,
//! the depth-bounded Datalog scheme, the loss ledger, and the **runtime reuse** —
//! the unrolled rules running on the existing native least-model engine.
//!
//! These live in the runtime crate (not the wasm-able gmeow-logic-compile) because
//! they execute the projected Datalog through `crate::rule_ir`, the Nemo-coupled
//! evaluable engine (#732). They consume the path projection's public surface from
//! `gmeow_logic_compile::projections::paths`.

use crate::rule_ir::{least_model_of_reduct, parse_eval_rules, Fact, FactStore};
use gmeow_logic_compile::frontend::parse_logic_str;
use gmeow_logic_compile::ir::{PathBase, PathShapeIr};
use gmeow_logic_compile::projections::paths::*;
use oxigraph::model::{NamedNode as OxNamedNode, Term};

fn shape(iri: &str, base: PathBase, min: u32, max: Option<u32>, ns: Option<&str>) -> PathShapeIr {
    PathShapeIr::new(
        iri,
        base,
        min,
        max,
        ns.map(str::to_owned),
        Some("maxDepth".to_owned()),
    )
    .unwrap()
}

// ── Property-path lowering ───────────────────────────────────────────────────

#[test]
fn named_bounded_lowers_to_range() {
    let s = shape(
        "https://x/ancestors",
        PathBase::NamedPredicate("https://x/parentOf".to_owned()),
        1,
        Some(3),
        None,
    );
    assert_eq!(property_path_text(&s), "<https://x/parentOf>{1,3}");
}

#[test]
fn named_exactly_one_lowers_to_bare_step() {
    let s = shape(
        "https://x/s",
        PathBase::NamedPredicate("https://x/p".to_owned()),
        1,
        Some(1),
        None,
    );
    assert_eq!(property_path_text(&s), "<https://x/p>");
}

#[test]
fn named_unbounded_lowers_to_one_or_more() {
    let s = shape(
        "https://x/s",
        PathBase::NamedPredicate("https://x/p".to_owned()),
        1,
        None,
        None,
    );
    assert_eq!(property_path_text(&s), "<https://x/p>+");
}

#[test]
fn wildcard_namespace_bounded_lowers_to_range_over_wildcard() {
    let s = shape(
        "https://x/nearbyOrgs",
        PathBase::Wildcard,
        1,
        Some(2),
        Some("https://x/org/"),
    );
    assert_eq!(property_path_text(&s), "<any:https://x/org/>{1,2}");
}

// ── Datalog scheme + ledger ──────────────────────────────────────────────────

#[test]
fn datalog_emits_unrolled_reach_rules() {
    let s = shape(
        "https://x/ancestors",
        PathBase::NamedPredicate("https://x/parentOf".to_owned()),
        1,
        Some(2),
        None,
    );
    let dl = datalog_text(&s);
    assert!(dl.contains("<https://x/ancestors/reach/1>"));
    assert!(dl.contains("<https://x/ancestors/reach/2>"));
    assert!(dl.contains("<https://x/ancestors/reachable>"));
    // No recursion for a bounded path.
    assert!(!dl.contains("reachable>(?X, ?Y, ?W) :- <https://x/ancestors/reachable>"));
}

#[test]
fn wildcard_datalog_declares_the_prepass() {
    let s = shape(
        "https://x/nearbyOrgs",
        PathBase::Wildcard,
        1,
        Some(2),
        Some("https://x/org/"),
    );
    let dl = datalog_text(&s);
    assert!(dl.contains("materialized by a namespace-scoped pre-pass"));
    assert!(dl.contains("<https://x/nearbyOrgs/edge>"));
}

#[test]
fn ledger_row_is_property_path_sound_under_with_declared_loss() {
    let s = shape(
        "https://x/s",
        PathBase::Wildcard,
        1,
        Some(2),
        Some("https://x/org/"),
    );
    let proj = project_path_shape(&s);
    assert_eq!(proj.ledger.target, "property-path");
    assert_eq!(proj.ledger.preservation, "SoundUnderApproximation");
    assert!(
        proj.ledger
            .lossy_drops
            .iter()
            .any(|d| d.contains("beyond SPARQL 1.1 §9")),
        "ledger must declare the §9-extension exit loss: {:?}",
        proj.ledger.lossy_drops
    );
}

#[test]
fn project_path_shapes_is_invoked_over_program_shapes() {
    // R4: the projection is genuinely exercised over a parsed program, not inert.
    let ttl = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex:    <https://example.org/test/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
ex:nearbyOrgs a logic:PathShape ;
    logic:pathWildcard true ;
    logic:pathNamespaceScope \"https://example.org/org/\"^^xsd:anyURI ;
    logic:pathMinDepth 1 ; logic:pathMaxDepth 2 ; logic:pathDepthParam \"maxDepth\" .";
    let (program, _diags) = parse_logic_str(ttl, None).expect("parse");
    let projections = project_path_shapes(&program);
    assert_eq!(projections.len(), 1);
    assert!(!projections[0].property_path.is_empty());
    assert!(!projections[0].datalog.is_empty());
}

// ── Runtime reuse: the unrolled rules run on the native least-model engine ────

fn fact(s: &str, p: &str, o: &str) -> Fact {
    Fact {
        subject: Term::NamedNode(OxNamedNode::new(s.to_owned()).unwrap()),
        predicate: OxNamedNode::new(p.to_owned()).unwrap(),
        object: Term::NamedNode(OxNamedNode::new(o.to_owned()).unwrap()),
    }
}

/// `(start, reachable, node)` membership query against the engine's fact store,
/// using its N3-surface fact-key tuple `(<subject>, predicate, <object>)`.
fn reaches(store: &FactStore, start: &str, reachable_pred: &str, node: &str) -> bool {
    store.contains_key(&(
        format!("<{start}>"),
        reachable_pred.to_owned(),
        format!("<{node}>"),
    ))
}

#[test]
fn named_bounded_path_runs_on_the_native_engine() {
    // :ancestors(?maxDepth := 2) over ex:parentOf, evaluated by the EXISTING
    // least-model fixpoint — no new traversal engine.
    let s = shape(
        "https://example.org/p/ancestors",
        PathBase::NamedPredicate("https://example.org/p/parentOf".to_owned()),
        1,
        Some(2),
        None,
    );
    let rules = parse_eval_rules(&datalog_text(&s)).expect("path rules parse");

    let mut edb = FactStore::new();
    let p = "https://example.org/p/parentOf";
    edb.insert(fact(
        "https://example.org/p/a",
        p,
        "https://example.org/p/b",
    ));
    edb.insert(fact(
        "https://example.org/p/b",
        p,
        "https://example.org/p/c",
    ));
    edb.insert(fact(
        "https://example.org/p/c",
        p,
        "https://example.org/p/d",
    ));

    let res = least_model_of_reduct(&edb, &rules, &FactStore::new()).expect("engine run");
    let reachable = "https://example.org/p/ancestors/reachable";

    // Within 2 hops from `a`: b (1 hop), c (2 hops).
    assert!(reaches(
        &res.store,
        "https://example.org/p/a",
        reachable,
        "https://example.org/p/b"
    ));
    assert!(reaches(
        &res.store,
        "https://example.org/p/a",
        reachable,
        "https://example.org/p/c"
    ));
    // d is 3 hops away — NOT reachable at maxDepth := 2 (the bound bites).
    assert!(!reaches(
        &res.store,
        "https://example.org/p/a",
        reachable,
        "https://example.org/p/d"
    ));
}

// ── G3: exact unbounded closure for min_depth > 1 ───────────────────────────

#[test]
fn unbounded_min2_datalog_contains_exact_min_depth_chain_not_bare_step() {
    // G3 structural: the emitted Datalog for an unbounded shape with min_depth=2
    // must NOT define reachable directly from a single edge atom (that would admit
    // 1-hop pairs).  It must contain the auxiliary closure relation and an explicit
    // 2-hop chain body.
    let s = shape(
        "https://x/twoPlus",
        PathBase::NamedPredicate("https://x/step".to_owned()),
        2,    // min_depth = 2 — the exact form must kick in
        None, // unbounded
        None,
    );
    let dl = datalog_text(&s);

    // Must NOT admit 1-hop: the result predicate must not be defined with a body
    // consisting solely of a single edge atom with ?Y as the direct destination
    // (that would admit 1-hop pairs).  The exact form must join via the chain.
    assert!(
        !dl.contains("<https://x/twoPlus/reachable>(?X, ?Y, ?W) :- <https://x/step>(?X, ?Y, ?W)"),
        "reachable must not be defined with a bare single-step ?X->?Y body for min_depth=2:\n{dl}"
    );
    // Must use the auxiliary closure relation.
    assert!(
        dl.contains("<https://x/twoPlus/closure>"),
        "emitted Datalog must contain the closure auxiliary:\n{dl}"
    );
    // Must contain the min-depth chain (edge(X,N1), then closure(N1,Y)).
    assert!(
        dl.contains("<https://x/step>(?X, ?N1, ?W)"),
        "min-depth chain must include an explicit first-hop atom:\n{dl}"
    );
    // The approximation note must be gone.
    assert!(
        !dl.contains("approximation"),
        "no approximation note must remain in the exact min_depth>1 branch:\n{dl}"
    );
}

#[test]
fn unbounded_min2_excludes_single_hop_runtime() {
    // G3 behavioral: a graph with a single edge A->B must yield NO result for an
    // unbounded shape with min_depth=2, while A->B->C yields (A,C).
    let s = shape(
        "https://example.org/p/twoPlus",
        PathBase::NamedPredicate("https://example.org/p/step".to_owned()),
        2,    // min_depth = 2
        None, // unbounded
        None,
    );
    let rules = parse_eval_rules(&datalog_text(&s)).expect("path rules parse");
    let reachable = "https://example.org/p/twoPlus/reachable";
    let p = "https://example.org/p/step";

    // Graph with a SINGLE edge A->B only.
    let mut edb = FactStore::new();
    edb.insert(fact(
        "https://example.org/p/a",
        p,
        "https://example.org/p/b",
    ));
    let res = least_model_of_reduct(&edb, &rules, &FactStore::new()).expect("engine run");
    // min_depth=2 with one edge → NO result for A->B (only 1 hop).
    assert!(
        !reaches(
            &res.store,
            "https://example.org/p/a",
            reachable,
            "https://example.org/p/b"
        ),
        "a single-hop A->B must NOT be reachable under min_depth=2"
    );

    // Graph with two edges A->B->C.
    let mut edb2 = FactStore::new();
    edb2.insert(fact(
        "https://example.org/p/a",
        p,
        "https://example.org/p/b",
    ));
    edb2.insert(fact(
        "https://example.org/p/b",
        p,
        "https://example.org/p/c",
    ));
    let res2 = least_model_of_reduct(&edb2, &rules, &FactStore::new()).expect("engine run 2");
    // A can reach C in 2 hops → must be reachable.
    assert!(
        reaches(
            &res2.store,
            "https://example.org/p/a",
            reachable,
            "https://example.org/p/c"
        ),
        "A->B->C must be reachable under min_depth=2"
    );
    // B can reach C in 1 hop → NOT reachable (min_depth=2 still applies).
    assert!(
        !reaches(
            &res2.store,
            "https://example.org/p/b",
            reachable,
            "https://example.org/p/c"
        ),
        "B->C (1 hop) must NOT be reachable under min_depth=2"
    );
}

#[test]
fn nearby_orgs_wildcard_runs_on_the_native_engine() {
    // :nearbyOrgs(?maxDepth := 2): the headline "all nodes within n hops, any
    // predicate" case.  The wildcard's `edge` relation is the namespace-scoped
    // pre-pass output (any org: predicate); the bounded closure runs on the engine.
    let s = shape(
        "https://example.org/logic/nearbyOrgs",
        PathBase::Wildcard,
        1,
        Some(2),
        Some("https://example.org/org/"),
    );
    let rules = parse_eval_rules(&datalog_text(&s)).expect("path rules parse");
    let edge = edge_predicate_iri(&s);

    // Pre-pass output: edge(X,Y) for every (X,p,Y) with p in the org namespace.
    let mut edb = FactStore::new();
    let o = "https://example.org/org/";
    edb.insert(fact(&format!("{o}acme"), &edge, &format!("{o}beta")));
    edb.insert(fact(&format!("{o}beta"), &edge, &format!("{o}gamma")));
    edb.insert(fact(&format!("{o}acme"), &edge, &format!("{o}delta")));
    edb.insert(fact(&format!("{o}gamma"), &edge, &format!("{o}epsilon")));

    let res = least_model_of_reduct(&edb, &rules, &FactStore::new()).expect("engine run");
    let reachable = "https://example.org/logic/nearbyOrgs/reachable";
    let acme = format!("{o}acme");

    // ≤2 hops from acme, by ANY predicate: beta (1), gamma (2), delta (1).
    assert!(reaches(&res.store, &acme, reachable, &format!("{o}beta")));
    assert!(reaches(&res.store, &acme, reachable, &format!("{o}gamma")));
    assert!(reaches(&res.store, &acme, reachable, &format!("{o}delta")));
    // epsilon is 3 hops (acme→beta→gamma→epsilon) — excluded by maxDepth := 2.
    assert!(!reaches(
        &res.store,
        &acme,
        reachable,
        &format!("{o}epsilon")
    ));
}

// ── CR7: golden pin anchoring the competency CQ to the projection ────────────
//
// The competency CQ (slices/core/logic/tests/competency.ttl +
// queries/competency/named-parametric-paths.rq) exercises the bounded path
// logic:nearbyOrgs over a four-node chain.  GMEOW's native projection lowers that
// shape to the EXTENDED SPARQL property path `<...linkedTo>{1,2}`; standard SPARQL
// engines (oxigraph, which runs the competency cell) cannot parse a `{m,n}`
// quantifier — that is precisely the §9 gap issue #1010 closes — so the .rq embeds
// the licensed standard-SPARQL down-projection `(linkedTo|linkedTo/linkedTo)`.
// This golden pins the lossless extended-SPARQL projection output: if the
// projection drifts, this test fails (rust-first anti-drift), independent of the
// hand-runnable standard-SPARQL form in the .rq.

/// The lossless extended-SPARQL property path GMEOW's projection emits for the
/// bounded `nearbyOrgs` shape (the §9-extended `{1,2}` form, not the standard-SPARQL
/// down-projection the competency `.rq` executes on oxigraph).
const NEARBY_ORGS_PROJECTED_PATH: &str =
    "<https://blackcatinformatics.ca/gmeow/examples/logic/tests/linkedTo>{1,2}";

#[test]
fn nearby_orgs_projection_emits_pinned_bounded_path() {
    // nearbyOrgs: a named-predicate path over ex:linkedTo with maxDepth = 2 (the
    // shape the competency fixture path-traversal.ttl exercises).
    let s = shape(
        "https://blackcatinformatics.ca/gmeow/examples/logic/tests/nearbyOrgs",
        PathBase::NamedPredicate(
            "https://blackcatinformatics.ca/gmeow/examples/logic/tests/linkedTo".to_owned(),
        ),
        1,
        Some(2),
        None,
    );
    let projected = project_path_shape(&s).property_path;
    assert_eq!(
        projected, NEARBY_ORGS_PROJECTED_PATH,
        "the logic:nearbyOrgs projection must emit the pinned bounded extended-SPARQL \
         path; if this drifts, the competency .rq's standard-SPARQL down-projection \
         must be re-derived to stay semantically equivalent"
    );
}
