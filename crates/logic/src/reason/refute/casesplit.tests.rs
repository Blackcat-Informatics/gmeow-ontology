// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::{RdfDatasetBuilder, RdfLiteral, RdfQuad};

const W: &str = "http://ex/w";

fn quad(s: &str, p: &str, o: &str) -> RdfQuad {
    RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
}
fn bnode_quad(s: &str, p: &str, o: RdfTerm) -> RdfQuad {
    RdfQuad::new(RdfTerm::iri(s), p, o).in_graph(RdfTerm::iri(W))
}

fn dataset(quads: Vec<RdfQuad>) -> std::sync::Arc<RdfDataset> {
    let mut b = RdfDatasetBuilder::new();
    for q in quads {
        b.push_owned_quad(&q);
    }
    b.freeze().expect("freeze")
}

fn is_inconsistent(edb: &RdfDataset) -> bool {
    matches!(
        decide(edb),
        Some(RefutationCertificate::InFragment {
            decision: Decision::Inconsistent,
            ..
        })
    )
}
fn is_consistent(edb: &RdfDataset) -> bool {
    matches!(
        decide(edb),
        Some(RefutationCertificate::InFragment {
            decision: Decision::Consistent,
            ..
        })
    )
}
fn withholds(edb: &RdfDataset) -> bool {
    matches!(
        decide(edb),
        Some(RefutationCertificate::OutOfFragment { .. })
    )
}

/// A WIDE + DEEP cyclic class expression mirroring webont-i5-26-007
/// (`_:B = B ⊓ (_:B ⊔ C)`) but with MANY individuals typed to the cyclic node.
/// The `owl:unionOf` re-offers a non-progressing `And`-disjunct branch at every
/// level; before the non-progress bound in [`search`] this recursed toward
/// [`SEARCH_DEPTH`], deep-cloning the full `State` per frame and exploding the
/// heap. The decider must WITHHOLD (`OutOfFragment`) in bounded memory rather
/// than OOM. Regression guard for the cyclic-TBox memory sink.
#[test]
fn cyclic_class_expression_withholds_without_memory_explosion() {
    let b = "http://ex/B";
    let u = "http://ex/U";
    let mut quads = vec![
        quad(b, RDF_TYPE, OWL_CLASS),
        quad(b, OWL_INTERSECTION_OF, "http://ex/il0"),
        // intersection list: [ B_named, U ]
        quad("http://ex/il0", RDF_FIRST, "http://ex/Bn"),
        quad("http://ex/il0", RDF_REST, "http://ex/il1"),
        quad("http://ex/il1", RDF_FIRST, u),
        quad("http://ex/il1", RDF_REST, RDF_NIL),
        quad(u, OWL_UNION_OF, "http://ex/ul0"),
    ];
    // union list: [ _:B, C ] — the cyclic self-reference `_:B` re-offered forever.
    quads.push(quad("http://ex/ul0", RDF_FIRST, b));
    quads.push(quad("http://ex/ul0", RDF_REST, "http://ex/ul1"));
    quads.push(quad("http://ex/ul1", RDF_FIRST, "http://ex/C"));
    quads.push(quad("http://ex/ul1", RDF_REST, RDF_NIL));
    for n in 0..300 {
        quads.push(quad(&format!("http://ex/ind{n}"), RDF_TYPE, b));
    }
    let edb = dataset(quads);
    // Bounded, sound withhold — never a decided verdict off a truncated cycle.
    assert!(withholds(edb.as_ref()));
}

#[test]
fn empty_edb_does_not_engage() {
    let edb = RdfDatasetBuilder::new().freeze().unwrap();
    assert!(decide(edb.as_ref()).is_none());
}

#[test]
fn selected_list_nil_first_is_a_source_boundary() {
    let edb = dataset(vec![
        quad("urn:selected:enumeration", OWL_ONE_OF, RDF_NIL),
        bnode_quad(RDF_NIL, RDF_FIRST, RdfTerm::blank_node("x")),
    ]);
    assert!(matches!(
        decide(edb.as_ref()),
        Some(RefutationCertificate::OutOfFragment {
            reason: FragmentBoundary::SourceAdmission { .. }
        })
    ));
}

#[test]
fn selected_list_nil_rest_is_a_source_boundary() {
    let edb = dataset(vec![
        quad("urn:selected:enumeration", OWL_ONE_OF, RDF_NIL),
        bnode_quad(RDF_NIL, RDF_REST, RdfTerm::blank_node("x")),
    ]);
    assert!(matches!(
        decide(edb.as_ref()),
        Some(RefutationCertificate::OutOfFragment {
            reason: FragmentBoundary::SourceAdmission { .. }
        })
    ));
}

#[test]
fn complement_membership_clash_is_inconsistent() {
    // x : C, x : ¬C  (via a complement node) ⇒ inconsistent.
    let edb = dataset(vec![
        quad("http://ex/x", RDF_TYPE, "http://ex/C"),
        quad("http://ex/x", RDF_TYPE, "http://ex/notC"),
        quad("http://ex/notC", OWL_COMPLEMENT_OF, "http://ex/C"),
    ]);
    assert!(is_inconsistent(edb.as_ref()));
}

/// A three-node `rdf:List` `head → [a, b] → nil` over plain IRIs, so a node's
/// subject key and its object references always coincide.
fn list2(head: &str, a: &str, b: &str, tail: &str) -> Vec<RdfQuad> {
    vec![
        quad(head, RDF_FIRST, a),
        quad(head, RDF_REST, tail),
        quad(tail, RDF_FIRST, b),
        quad(tail, RDF_REST, RDF_NIL),
    ]
}

#[test]
fn disjoint_union_with_complement_is_consistent() {
    // Child = Boy ⊎ Girl; Stewie : Child, Stewie : ¬Girl ⇒ Stewie ∈ Boy,
    // consistent (mirrors new-feature-disjointunion-001).
    let mut quads = vec![
        quad("http://ex/Child", RDF_TYPE, OWL_CLASS),
        quad("http://ex/Child", OWL_DISJOINT_UNION_OF, "http://ex/l0"),
        quad("http://ex/Stewie", RDF_TYPE, "http://ex/Child"),
        quad("http://ex/Stewie", RDF_TYPE, "http://ex/notgirl"),
        quad("http://ex/notgirl", OWL_COMPLEMENT_OF, "http://ex/Girl"),
    ];
    quads.extend(list2(
        "http://ex/l0",
        "http://ex/Boy",
        "http://ex/Girl",
        "http://ex/l1",
    ));
    let edb = dataset(quads);
    assert!(is_consistent(edb.as_ref()));
}

#[test]
fn union_disjoint_unsat_is_inconsistent() {
    // x : Test; Test ⊑ (A ⊔ B); Test ⊑ ¬A (via disjoint); Test ⊑ ¬B ⇒ every
    // branch closes ⇒ inconsistent.
    let mut quads = vec![
        quad("http://ex/x", RDF_TYPE, "http://ex/Test"),
        quad("http://ex/Test", RDFS_SUBCLASSOF, "http://ex/union"),
        quad("http://ex/union", OWL_UNION_OF, "http://ex/u0"),
        // Test disjoint with both A and B ⇒ x can be in neither.
        quad("http://ex/Test", OWL_DISJOINT_WITH, "http://ex/A"),
        quad("http://ex/Test", OWL_DISJOINT_WITH, "http://ex/B"),
    ];
    quads.extend(list2(
        "http://ex/u0",
        "http://ex/A",
        "http://ex/B",
        "http://ex/u1",
    ));
    let edb = dataset(quads);
    assert!(is_inconsistent(edb.as_ref()));
}

#[test]
fn union_disjoint_sat_is_consistent() {
    // x : Test; Test ⊑ (A ⊔ B); A disjoint B (no forced clash) ⇒ consistent.
    let mut quads = vec![
        quad("http://ex/x", RDF_TYPE, "http://ex/Test"),
        quad("http://ex/Test", RDFS_SUBCLASSOF, "http://ex/union"),
        quad("http://ex/union", OWL_UNION_OF, "http://ex/u0"),
        quad("http://ex/A", OWL_DISJOINT_WITH, "http://ex/B"),
    ];
    quads.extend(list2(
        "http://ex/u0",
        "http://ex/A",
        "http://ex/B",
        "http://ex/u1",
    ));
    let edb = dataset(quads);
    assert!(is_consistent(edb.as_ref()));
}

#[test]
fn nominal_equality_differentfrom_clash_is_inconsistent() {
    // x : {a}; y : {a}; x differentFrom y ⇒ x = a = y contradicts distinctness.
    let edb = dataset(vec![
        quad("http://ex/x", RDF_TYPE, "http://ex/oneA"),
        quad("http://ex/oneA", OWL_ONE_OF, "http://ex/la"),
        quad("http://ex/la", RDF_FIRST, "http://ex/a"),
        quad("http://ex/la", RDF_REST, RDF_NIL),
        quad("http://ex/y", RDF_TYPE, "http://ex/oneA2"),
        quad("http://ex/oneA2", OWL_ONE_OF, "http://ex/lb"),
        quad("http://ex/lb", RDF_FIRST, "http://ex/a"),
        quad("http://ex/lb", RDF_REST, RDF_NIL),
        quad("http://ex/x", OWL_DIFFERENT_FROM, "http://ex/y"),
    ]);
    assert!(is_inconsistent(edb.as_ref()));
}

#[test]
fn existential_present_blocks_consistent_withholds() {
    // A benign complement plus a someValuesFrom restriction: the complement
    // engages the decider, but the existential blocks a `Consistent` verdict
    // (no clash) ⇒ honest withhold.
    let edb = dataset(vec![
        quad("http://ex/x", RDF_TYPE, "http://ex/C"),
        quad("http://ex/notD", OWL_COMPLEMENT_OF, "http://ex/D"),
        quad("http://ex/C", RDFS_SUBCLASSOF, "http://ex/r"),
        quad("http://ex/r", RDF_TYPE, OWL_RESTRICTION),
        quad(
            "http://ex/r",
            "http://www.w3.org/2002/07/owl#onProperty",
            "http://ex/p",
        ),
        quad(
            "http://ex/r",
            "http://www.w3.org/2002/07/owl#someValuesFrom",
            "http://ex/D",
        ),
    ]);
    assert!(withholds(edb.as_ref()));
}

#[test]
fn determinism_byte_stable() {
    let edb = dataset(vec![
        quad("http://ex/x", RDF_TYPE, "http://ex/C"),
        quad("http://ex/x", RDF_TYPE, "http://ex/notC"),
        quad("http://ex/notC", OWL_COMPLEMENT_OF, "http://ex/C"),
    ]);
    let a = format!("{:?}", decide(edb.as_ref()));
    let b = format!("{:?}", decide(edb.as_ref()));
    assert_eq!(a, b);
}

#[test]
fn literal_oneof_is_not_engaged_as_nominal() {
    // A pure datatype (literal) oneOf is owned by the datatype sub-decider; the
    // case-split decider must not certify it consistent as a nominal.
    let edb = dataset(vec![
        quad("http://ex/x", RDF_TYPE, "http://ex/enum"),
        quad("http://ex/enum", OWL_ONE_OF, "http://ex/ll"),
        bnode_quad(
            "http://ex/ll",
            RDF_FIRST,
            RdfTerm::Literal(RdfLiteral::typed(
                "1",
                "http://www.w3.org/2001/XMLSchema#integer",
            )),
        ),
        quad("http://ex/ll", RDF_REST, RDF_NIL),
    ]);
    // No clash; the literal enumeration blocks `Consistent` ⇒ withhold.
    assert!(withholds(edb.as_ref()));
}
