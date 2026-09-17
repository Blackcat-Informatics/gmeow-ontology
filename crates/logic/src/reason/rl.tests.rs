// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm};

const W: &str = "http://gmeow.example/w";
const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const SUBPROP: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

const A: &str = "http://gmeow.example/A";
const B: &str = "http://gmeow.example/B";
const C: &str = "http://gmeow.example/C";
const P: &str = "http://gmeow.example/p";
const P1: &str = "http://gmeow.example/p1";
const P2: &str = "http://gmeow.example/p2";
const X: &str = "http://gmeow.example/x";
const Y: &str = "http://gmeow.example/y";

fn quad(s: &str, p: &str, o: &str) -> RdfQuad {
    RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
}

fn dataset(quads: Vec<RdfQuad>) -> std::sync::Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for quad in quads {
        builder.push_owned_quad(&quad);
    }
    builder.freeze().expect("valid test dataset")
}

fn has(closure: &RlClosure, s: &str, p: &str, o: &str) -> bool {
    let obj = format!("<{o}>");
    closure
        .triples
        .iter()
        .any(|t| t.subject == s && t.predicate == p && t.object == obj)
}

#[test]
fn canonical_annotation_axioms_reach_the_named_rl_world() {
    let mut builder = RdfDatasetBuilder::new();
    let world = builder.intern_iri(W);
    let class = builder.intern_iri(A);
    let super_class = builder.intern_iri(B);
    let predicate = builder.intern_iri(gmeow_ns::LOGIC_SUB_CLASS_OF);
    let x = builder.intern_iri(X);
    let p = builder.intern_iri(P);
    let y = builder.intern_iri(Y);
    let statement = builder.intern_triple(x, p, y);
    builder.push_reifier_in_graph(class, statement, Some(world));
    builder.push_annotation_in_graph(class, predicate, super_class, Some(world));
    let source = builder.freeze().unwrap();
    assert_eq!(source.quads().count(), 0);
    assert_eq!(source.annotation_quads().count(), 1);

    let lowered = lower_edb_for_rl(&source).unwrap().unwrap();
    let rows = lowered.owned_quads().collect::<Vec<_>>();
    assert_eq!(rows.len(), 2);
    assert!(rows.contains(&quad(A, gmeow_ns::LOGIC_SUB_CLASS_OF, B)));
    assert!(rows.contains(&quad(A, SUBCLASS, B)));
    assert!(
        !rows.contains(&quad(X, P, Y)),
        "quotation must not become assertion"
    );
}

/// The canonical `logic:` subsumption spelling drives the fixed RDFS-vocabulary
/// RL calculus. GMEOW's authored `module.ttl` surface spells subsumption
/// `logic:subClassOf` / `logic:subPropertyOf` (Principle 17 — `rdfs:` is one of
/// its projections), while the W3C RL rules match `rdfs:` by specification. A
/// chase over authored sources must therefore lower the canonical edge at the
/// EDB boundary, or a re-authored taxonomy derives NOTHING — silently, with an
/// empty closure instead of an error.
#[test]
fn canonical_logic_subsumption_drives_the_rdfs_vocabulary_calculus() {
    let store = dataset(vec![
        quad(X, TYPE, A),
        quad(A, gmeow_ns::LOGIC_SUB_CLASS_OF, B),
        quad(B, gmeow_ns::LOGIC_SUB_CLASS_OF, C),
        quad(P1, gmeow_ns::LOGIC_SUB_PROPERTY_OF, P2),
        quad(X, P1, Y),
    ]);
    let c = rl_closure(store.as_ref()).expect("RL closure should succeed");
    assert!(
        has(&c, X, TYPE, B),
        "x a B via cax-sco over logic:subClassOf"
    );
    assert!(
        has(&c, X, TYPE, C),
        "x a C via cax-sco + scm-sco over logic:subClassOf"
    );
    assert!(
        has(&c, X, P2, Y),
        "x p2 y via prp-spo1 over logic:subPropertyOf"
    );
    // The projection ADDS the RDFS view; the authored canonical edge survives.
    assert!(
        has(&c, A, gmeow_ns::LOGIC_SUB_CLASS_OF, B),
        "the authored canonical edge is kept, not rewritten away"
    );
    assert!(
        has(&c, A, SUBCLASS, B),
        "the canonical edge is materialized under its rdfs: projection"
    );
}

// PurRDF owns the OWL-RL rule corpus. Retain GMEOW adapter contracts:
// canonical vocabulary lowering, selected regime, world/provenance fields
// and the rendered closure surface.

const DISJOINT_UNION_OF: &str = "http://www.w3.org/2002/07/owl#disjointUnionOf";

#[test]
fn rl_adapter_does_not_select_dl_for_disjoint_union() {
    // C disjointUnionOf ( A B ) is a DL construct: `C ≡ A ⊔ B` with A, B pairwise
    // disjoint. OWL 2 RL's scm/cls rule tables have NO clause over
    // owl:disjointUnionOf (purrdf reads it only in the Direct-Semantics lane), so
    // the member-subclass edge the retired native custom rule derived is NOT an RL
    // entailment. This asserts that boundary — and that unionOf's scm-uni does not
    // spuriously fire on disjointUnionOf — so a regression that re-added the custom
    // rule, or mis-routed disjointUnionOf into scm-uni, would flip it.
    let l0 = "http://gmeow.example/l0";
    let l1 = "http://gmeow.example/l1";
    let store = dataset(vec![
        quad(C, DISJOINT_UNION_OF, l0),
        quad(l0, FIRST, A),
        quad(l0, REST, l1),
        quad(l1, FIRST, B),
        quad(l1, REST, NIL),
    ]);
    let c = rl_closure(store.as_ref()).expect("RL closure should succeed");
    assert!(
        !has(&c, A, SUBCLASS, C),
        "A ⊑ C is not an OWL 2 RL entailment of owl:disjointUnionOf"
    );
    assert!(
        !has(&c, B, SUBCLASS, C),
        "B ⊑ C is not an OWL 2 RL entailment of owl:disjointUnionOf"
    );
}

#[test]
fn literal_objects_round_trip_through_the_closure() {
    // A hyphenated language tag (`@x-gmeow-english`), an escaped quote, and a
    // typed integer all retain exact identity through the closure. prp-spo1 must
    // carry the literal object through unchanged.
    let subprop = SUBPROP;
    let p1 = P1;
    let p2 = P2;
    let label = "http://www.w3.org/2000/01/rdf-schema#label";
    let lit_quad = RdfQuad::new(
        RdfTerm::iri(X),
        p1,
        RdfTerm::Literal(purrdf::RdfLiteral::language_tagged(
            "say \"hi\"",
            "x-gmeow-english",
        )),
    )
    .in_graph(RdfTerm::iri(W));
    let int_quad = RdfQuad::new(
        RdfTerm::iri(X),
        label,
        RdfTerm::Literal(purrdf::RdfLiteral::typed(
            "5",
            "http://www.w3.org/2001/XMLSchema#integer",
        )),
    )
    .in_graph(RdfTerm::iri(W));
    let store = dataset(vec![quad(p1, subprop, p2), lit_quad, int_quad]);
    let c = rl_closure(store.as_ref()).expect("RL closure should succeed");

    // The language literal propagates up the sub-property hierarchy unchanged.
    let derived = c
        .triples
        .iter()
        .find(|t| t.subject == X && t.predicate == p2)
        .expect("x p2 <lang-literal> must be derived via prp-spo1");
    assert_eq!(derived.object, "\"say \\\"hi\\\"\"@x-gmeow-english");
    // The typed integer literal round-trips with its datatype intact.
    assert!(
        c.triples.iter().any(|t| t.subject == X
            && t.predicate == label
            && t.object == "\"5\"^^<http://www.w3.org/2001/XMLSchema#integer>"),
        "typed integer literal must round-trip"
    );
}

#[test]
fn closure_carries_the_world_and_edb_flags() {
    let store = dataset(vec![quad(X, TYPE, A), quad(A, SUBCLASS, B)]);
    let c = rl_closure(store.as_ref()).expect("RL closure should succeed");
    let derived = c
        .triples
        .iter()
        .find(|t| t.subject == X && t.predicate == TYPE && t.object == format!("<{B}>"))
        .expect("x a B must be derived");
    assert!(!derived.is_edb, "derived triple must not be is_edb");
    assert_eq!(derived.world, W, "derived triple carries its world");
    let rule_name = c
        .rule_name(derived)
        .expect("attribution should succeed for a derived triple");
    assert_eq!(
        rule_name.as_deref(),
        Some("cax-sco"),
        "derived triple cites the firing rule"
    );
    // An asserted (EDB) triple carries no firing rule.
    let asserted = c
        .triples
        .iter()
        .find(|t| t.is_edb)
        .expect("the closure keeps the asserted triples");
    assert_eq!(
        c.rule_name(asserted).expect("attribution should succeed"),
        None,
        "an asserted triple has no firing rule"
    );
}

#[test]
fn to_ntriples_renders_blank_literal_dedups_and_sorts() {
    // The render: skolem IRI → blank-node label, literal pass-through, de-dup, and
    // byte-stable sort.
    let lit = |s: &str, p: &str, o: &str| RlTriple {
        subject: s.to_owned(),
        predicate: p.to_owned(),
        object: o.to_owned(),
        world: W.to_owned(),
        is_edb: false,
    };
    let closure = RlClosure {
        triples: vec![
            lit(B, TYPE, &format!("<{C}>")),
            lit(A, TYPE, &format!("<{B}>")),
            // Skolemized blank-node subject + a language-tagged literal object.
            lit(&format!("{SKOLEM_PREFIX}abc123"), P, "\"hi\"@en"),
            // Exact duplicate of the first row — must collapse to one line.
            lit(B, TYPE, &format!("<{C}>")),
        ],
        edb: None,
        closure: None,
    };
    let nt = closure.to_ntriples();
    let lines: Vec<&str> = nt.lines().collect();
    assert_eq!(lines.len(), 3, "deduped to 3 distinct lines: {nt:?}");
    let mut sorted = lines.clone();
    sorted.sort_unstable();
    assert_eq!(lines, sorted, "lines are sorted for determinism");
    assert!(
        nt.contains(&format!("_:babc123 <{P}> \"hi\"@en .\n")),
        "skolem IRI → blank-node label, literal preserved: {nt}"
    );
    assert!(
        nt.contains(&format!("<{A}> <{TYPE}> <{B}> .\n")),
        "named-node triple rendered verbatim: {nt}"
    );
}
