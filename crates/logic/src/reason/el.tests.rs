// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm};

const W: &str = "http://gmeow.example/w";
const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const EQUIV: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

const A: &str = "http://gmeow.example/A";
const B: &str = "http://gmeow.example/B";
const C: &str = "http://gmeow.example/C";
const D: &str = "http://gmeow.example/D";
const E: &str = "http://gmeow.example/E";
const X: &str = "http://gmeow.example/x";

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

/// Find an inferred axiom matching the given triple (any world).
///
/// `o` is the bare object IRI; match the native resource value directly.
fn find<'a>(closure: &'a ElClosure, s: &str, p: &str, o: &str) -> Option<&'a InferredAxiom> {
    closure
        .inferred
        .iter()
        .find(|a| a.subject == s && a.predicate == p && a.object.as_iri() == Some(o))
}

#[test]
fn subclass_transitivity_derives_a_subclass_c() {
    // A ⊑ B, B ⊑ C ⇒ A ⊑ C (derived, not asserted).
    let store = dataset(vec![quad(A, SUBCLASS, B), quad(B, SUBCLASS, C)]);
    let closure = el_closure(store.as_ref()).expect("EL closure should succeed");

    let ac = find(&closure, A, SUBCLASS, C).expect("A ⊑ C must be inferred");
    assert!(!ac.is_edb, "A ⊑ C is derived, must be is_edb == false");
    assert_eq!(ac.world, W, "derived axiom carries its world IRI");
    assert!(
        !ac.premises.is_empty(),
        "derived A ⊑ C must carry antecedent premises"
    );
}

#[test]
fn equivalent_class_derives_both_directions() {
    // D ≡ E ⇒ D ⊑ E and E ⊑ D.
    let store = dataset(vec![quad(D, EQUIV, E)]);
    let closure = el_closure(store.as_ref()).expect("EL closure should succeed");

    let de = find(&closure, D, SUBCLASS, E).expect("D ⊑ E must be inferred");
    let ed = find(&closure, E, SUBCLASS, D).expect("E ⊑ D must be inferred");
    assert!(!de.is_edb, "D ⊑ E is derived");
    assert!(!ed.is_edb, "E ⊑ D is derived");
}

#[test]
fn type_propagation_derives_x_type_b() {
    // x : A, A ⊑ B ⇒ x : B.
    let store = dataset(vec![quad(X, TYPE, A), quad(A, SUBCLASS, B)]);
    let closure = el_closure(store.as_ref()).expect("EL closure should succeed");

    let xb = find(&closure, X, TYPE, B).expect("x : B must be inferred");
    assert!(!xb.is_edb, "x : B is derived, must be is_edb == false");
}

/// The CANONICAL `logic:` subsumption spelling drives the fixed RDFS-vocabulary
/// EL calculus, so a taxonomy authored the way every `module.ttl` authors it is
/// live in the SHIPPED closure — not merely parsed and then dropped.
///
/// This is the EL/DL twin of `rl::tests::canonical_logic_subsumption_drives_the_rdfs_vocabulary_calculus`.
/// It is the lane that matters to a consumer: `generated/logic/inferred-closure.rdf12.ttl`,
/// the `graph/reasoning` projection folded into `gmeow.gts`, the `DlVerdict`, and
/// `gmeow entails` all fold from this chase. Without the
/// [`crate::reason::edb_predicate_spellings`] lowering in
/// [`crate::reason::build_edb_facts`], a class authored
/// `logic:subClassOf math:MathConformanceFailure` yields NO entailment at all: the
/// enforcement fires but the taxonomy is dark to anyone consuming the bundle.
#[test]
fn canonical_logic_subsumption_drives_the_rdfs_vocabulary_el_calculus() {
    let logic_subclass = gmeow_ns::LOGIC_SUB_CLASS_OF;
    let store = dataset(vec![
        quad(X, TYPE, A),
        quad(A, logic_subclass, B),
        quad(B, logic_subclass, C),
        // A MIXED chain closes as ONE taxonomy: the canonical edge and its rdfs:
        // projection are the same edge, so D ⊑ E (canonical) ⊑ ... composes.
        quad(D, SUBCLASS, E),
        quad(E, logic_subclass, A),
    ]);
    let closure = el_closure(store.as_ref()).expect("EL closure should succeed");

    let ac = find(&closure, A, SUBCLASS, C).expect("A ⊑ C must be inferred");
    assert!(!ac.is_edb, "A ⊑ C is derived from the canonical chain");
    let xb = find(&closure, X, TYPE, B).expect("x : B must be inferred");
    assert!(!xb.is_edb, "x : B is derived over logic:subClassOf");
    find(&closure, X, TYPE, C).expect("x : C must be inferred transitively");
    find(&closure, D, SUBCLASS, C)
        .expect("the rdfs: and logic: spellings compose into one taxonomy");

    // The projection ADDS the RDFS view rather than rewriting the authored edge
    // away, and it is ASSERTED — a projection of an asserted axiom is asserted,
    // not derived. (The authored `logic:` spelling itself is present in the chase
    // but filtered off THIS surface by [`SUBSUMPTION_PREDICATES`]; its survival is
    // asserted directly over the unfiltered RL encoding in
    // `rl::tests::canonical_logic_subsumption_drives_the_rdfs_vocabulary_calculus`.)
    let projected = find(&closure, A, SUBCLASS, B)
        .expect("the canonical edge is materialized under its rdfs: projection");
    assert!(
        projected.is_edb,
        "a projection of an asserted axiom is asserted, not derived"
    );
}

#[test]
fn gaps_names_the_predicate_as_symbol_limitation() {
    let store = dataset(vec![quad(A, SUBCLASS, B)]);
    let closure = el_closure(store.as_ref()).expect("EL closure should succeed");
    assert_eq!(closure.gaps.len(), 1, "exactly one profile-limit entry");
    assert!(
        closure.gaps[0].contains("property-chain")
            && closure.gaps[0].contains("predicate-as-symbol"),
        "profile limit must name domain/range + property-chain inexpressibility: {:?}",
        closure.gaps[0]
    );
}
