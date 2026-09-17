// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::reason::refute::{Decision, native::testing};
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};

const W: &str = "http://ex/w";
const XSD_NNI: &str = "http://www.w3.org/2001/XMLSchema#nonNegativeInteger";

fn quad(s: &str, p: &str, o: &str) -> RdfQuad {
    RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
}
fn typed_lit_quad(s: &str, p: &str, value: &str, dt: &str) -> RdfQuad {
    RdfQuad::new(
        RdfTerm::iri(s),
        p,
        RdfTerm::Literal(RdfLiteral::typed(value, dt)),
    )
    .in_graph(RdfTerm::iri(W))
}

fn dataset(quads: Vec<RdfQuad>) -> std::sync::Arc<RdfDataset> {
    let mut b = RdfDatasetBuilder::new();
    for q in quads {
        b.push_owned_quad(&q);
    }
    b.freeze().expect("freeze")
}

fn observations(edb: &RdfDataset) -> Vec<NativeFamilyLedger> {
    testing::fixtures(edb, true)
}
fn decide(edb: &RdfDataset) -> Option<Decision> {
    testing::decision(&observations(edb), |family| {
        family != NativeRefutationFamily::Datatype
    })
}
fn is_inconsistent(edb: &RdfDataset) -> bool {
    decide(edb) == Some(Decision::Inconsistent)
}
fn is_consistent(edb: &RdfDataset) -> bool {
    decide(edb) == Some(Decision::Consistent)
}
fn complete(edb: &RdfDataset, family: NativeRefutationFamily) -> bool {
    observations(edb)
        .iter()
        .flat_map(|ledger| &ledger.outcomes)
        .filter(|outcome| outcome.family == family)
        .all(|outcome| {
            matches!(
                outcome.completion,
                NativeFamilyCompletion::Complete | NativeFamilyCompletion::NotEngaged
            )
        })
}
fn decides_cardinality(edb: &RdfDataset) -> bool {
    complete(edb, NativeRefutationFamily::Cardinality)
}
fn decides_identity(edb: &RdfDataset) -> bool {
    complete(edb, NativeRefutationFamily::Identity)
}
fn decides_has_self(edb: &RdfDataset) -> bool {
    complete(edb, NativeRefutationFamily::HasSelf)
}
#[test]
fn ifp_literal_merges_use_native_values_with_complete_language_identity() {
    let language = |tag: &str, direction| RdfLiteral {
        lexical_form: "word".into(),
        datatype: None,
        language: Some(tag.into()),
        direction,
    };
    let cases = [
        (
            RdfLiteral::typed("1", "http://www.w3.org/2001/XMLSchema#integer"),
            RdfLiteral::typed("1.0", "http://www.w3.org/2001/XMLSchema#decimal"),
            true,
        ),
        (
            RdfLiteral::typed(
                "2026-09-09T08:00:00Z",
                "http://www.w3.org/2001/XMLSchema#dateTime",
            ),
            RdfLiteral::typed(
                "2026-09-09T09:00:00+01:00",
                "http://www.w3.org/2001/XMLSchema#dateTime",
            ),
            true,
        ),
        (language("en", None), language("fr", None), false),
        (
            language("ar", Some(purrdf::RdfTextDirection::Ltr)),
            language("ar", Some(purrdf::RdfTextDirection::Rtl)),
            false,
        ),
    ];
    for (a, b, expected) in cases {
        let input = dataset(vec![
            quad("http://ex/p", RDF_TYPE, OWL_INVERSE_FUNCTIONAL_PROPERTY),
            quad("http://ex/a", OWL_DIFFERENT_FROM, "http://ex/b"),
            RdfQuad::new(
                RdfTerm::iri("http://ex/a"),
                "http://ex/p",
                RdfTerm::literal(a),
            )
            .in_graph(RdfTerm::iri(W)),
            RdfQuad::new(
                RdfTerm::iri("http://ex/b"),
                "http://ex/p",
                RdfTerm::literal(b),
            )
            .in_graph(RdfTerm::iri(W)),
        ]);
        assert_eq!(is_inconsistent(input.as_ref()), expected);
        assert_eq!(is_consistent(input.as_ref()), !expected);
    }
}

#[test]
fn ifp_declaration_cannot_merge_subjects_in_another_world() {
    let mut definition = quad("http://ex/p", RDF_TYPE, OWL_INVERSE_FUNCTIONAL_PROPERTY);
    definition.graph_name = Some(RdfTerm::iri("http://ex/other-world"));
    let input = dataset(vec![
        definition,
        quad("http://ex/a", OWL_DIFFERENT_FROM, "http://ex/b"),
        typed_lit_quad(
            "http://ex/a",
            "http://ex/p",
            "1",
            "http://www.w3.org/2001/XMLSchema#integer",
        ),
        typed_lit_quad(
            "http://ex/b",
            "http://ex/p",
            "1.0",
            "http://www.w3.org/2001/XMLSchema#decimal",
        ),
    ]);
    assert!(is_consistent(input.as_ref()));
    assert!(!is_inconsistent(input.as_ref()));
}
fn withholds(edb: &RdfDataset) -> bool {
    decide(edb).is_none()
}

#[test]
fn empty_edb_does_not_engage() {
    let edb = RdfDatasetBuilder::new().freeze().unwrap();
    assert!(decide(edb.as_ref()).is_none());
}

#[test]
fn consistent_min_max_cardinality_certifies() {
    // C ⊑ (min 1 p), C ⊑ (max 1 p) — pure cardinality, satisfiable.
    let edb = dataset(vec![
        quad("http://ex/C", RDF_TYPE, OWL_CLASS),
        quad("http://ex/p", RDF_TYPE, OWL_OBJECT_PROPERTY),
        quad("http://ex/C", RDFS_SUBCLASSOF, "http://ex/r1"),
        quad("http://ex/C", RDFS_SUBCLASSOF, "http://ex/r2"),
        quad("http://ex/r1", RDF_TYPE, OWL_RESTRICTION),
        typed_lit_quad("http://ex/r1", OWL_MIN_CARDINALITY, "1", XSD_NNI),
        quad("http://ex/r1", OWL_ON_PROPERTY, "http://ex/p"),
        quad("http://ex/r2", RDF_TYPE, OWL_RESTRICTION),
        typed_lit_quad("http://ex/r2", OWL_MAX_CARDINALITY, "1", XSD_NNI),
        quad("http://ex/r2", OWL_ON_PROPERTY, "http://ex/p"),
    ]);
    assert!(is_consistent(edb.as_ref()));
    assert!(decides_cardinality(edb.as_ref()));
}

#[test]
fn collapsed_bound_on_populated_class_clashes() {
    // C ⊑ (min 2 p) ⊓ (max 1 p), i:C — unsatisfiable populated class.
    let edb = dataset(vec![
        quad("http://ex/C", RDF_TYPE, OWL_CLASS),
        quad("http://ex/i", RDF_TYPE, "http://ex/C"),
        quad("http://ex/C", RDFS_SUBCLASSOF, "http://ex/r1"),
        quad("http://ex/C", RDFS_SUBCLASSOF, "http://ex/r2"),
        quad("http://ex/r1", RDF_TYPE, OWL_RESTRICTION),
        typed_lit_quad("http://ex/r1", OWL_MIN_CARDINALITY, "2", XSD_NNI),
        quad("http://ex/r1", OWL_ON_PROPERTY, "http://ex/p"),
        quad("http://ex/r2", RDF_TYPE, OWL_RESTRICTION),
        typed_lit_quad("http://ex/r2", OWL_MAX_CARDINALITY, "1", XSD_NNI),
        quad("http://ex/r2", OWL_ON_PROPERTY, "http://ex/p"),
    ]);
    assert!(is_inconsistent(edb.as_ref()));
}

#[test]
fn ifp_merge_without_distinctness_is_consistent() {
    // s1 p o, s2 p o, p IFP — s1 = s2, no differentFrom ⇒ consistent.
    let edb = dataset(vec![
        quad("http://ex/p", RDF_TYPE, OWL_INVERSE_FUNCTIONAL_PROPERTY),
        quad("http://ex/s1", "http://ex/p", "http://ex/o"),
        quad("http://ex/s2", "http://ex/p", "http://ex/o"),
    ]);
    assert!(is_consistent(edb.as_ref()));
    assert!(decides_identity(edb.as_ref()));
}

#[test]
fn ifp_merge_with_differentfrom_clashes() {
    // s1 p o, s2 p o, p IFP, s1 differentFrom s2 ⇒ inconsistent (1 = 2 collapse).
    let edb = dataset(vec![
        quad("http://ex/p", RDF_TYPE, OWL_INVERSE_FUNCTIONAL_PROPERTY),
        quad("http://ex/s1", "http://ex/p", "http://ex/o"),
        quad("http://ex/s2", "http://ex/p", "http://ex/o"),
        quad("http://ex/s1", OWL_DIFFERENT_FROM, "http://ex/s2"),
    ]);
    assert!(is_inconsistent(edb.as_ref()));
}

#[test]
fn ifp_literal_merge_matches_by_value() {
    // Two subjects sharing a literal value on an IFP merge (data-valued IFP).
    let edb = dataset(vec![
        quad("http://ex/p", RDF_TYPE, OWL_INVERSE_FUNCTIONAL_PROPERTY),
        typed_lit_quad("http://ex/s1", "http://ex/p", "123", XSD_STRING),
        typed_lit_quad("http://ex/s2", "http://ex/p", "123", XSD_STRING),
        quad("http://ex/s1", OWL_DIFFERENT_FROM, "http://ex/s2"),
    ]);
    assert!(is_inconsistent(edb.as_ref()));
}

#[test]
fn ifp_with_class_construction_withholds() {
    // The same IFP but a subClassOf/disjoint construction present — outside the
    // pure assertional fragment ⇒ withhold rather than guess.
    let edb = dataset(vec![
        quad("http://ex/p", RDF_TYPE, OWL_INVERSE_FUNCTIONAL_PROPERTY),
        quad("http://ex/s1", "http://ex/p", "http://ex/o"),
        quad("http://ex/A", RDFS_SUBCLASSOF, "http://ex/B"),
    ]);
    assert!(withholds(edb.as_ref()));
    assert!(!decides_identity(edb.as_ref()));
}

#[test]
fn has_self_disjoint_self_edge_clashes() {
    // R = ∃p.Self, C disjointWith R, x p x, x:C ⇒ x ∈ Nothing.
    let edb = dataset(vec![
        quad("http://ex/C", RDF_TYPE, OWL_CLASS),
        quad("http://ex/C", OWL_DISJOINT_WITH, "http://ex/R"),
        quad("http://ex/R", RDF_TYPE, OWL_RESTRICTION),
        typed_lit_quad("http://ex/R", OWL_HAS_SELF, "true", XSD_BOOLEAN),
        quad("http://ex/R", OWL_ON_PROPERTY, "http://ex/p"),
        quad("http://ex/p", RDF_TYPE, OWL_OBJECT_PROPERTY),
        quad("http://ex/x", "http://ex/p", "http://ex/x"),
        quad("http://ex/x", RDF_TYPE, "http://ex/C"),
    ]);
    assert!(is_inconsistent(edb.as_ref()));
    assert!(decides_has_self(edb.as_ref()));
}

#[test]
fn determinism_byte_stable() {
    let edb = dataset(vec![
        quad("http://ex/p", RDF_TYPE, OWL_INVERSE_FUNCTIONAL_PROPERTY),
        quad("http://ex/s1", "http://ex/p", "http://ex/o"),
        quad("http://ex/s2", "http://ex/p", "http://ex/o"),
        quad("http://ex/s1", OWL_DIFFERENT_FROM, "http://ex/s2"),
    ]);
    let a = format!("{:?}", decide(edb.as_ref()));
    let b = format!("{:?}", decide(edb.as_ref()));
    assert_eq!(a, b);
}
