// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_identity_orthogonality.py
//!
//! The centrepiece ethical invariant: the seven identity axes are ORTHOGONAL —
//! address (pronoun set, honorific), gender identity/expression,
//! sex-assigned-at-birth, and sexual/romantic orientation are independent, and
//! none may be inferred from another (CONSTITUTION P9). This file is the native
//! REGRESSION layer over the annotation-driven live enforcement
//! (`gmeow:coequalFacet true` + the `gufo::coequal_facet_orthogonality` lint):
//! the historical seven-axis matrix is pinned explicitly via `GraphStore`
//! structural asserts, and the two lint-machinery tests call the production lint
//! directly (its Python shim, `reasoning_coequal_facet_orthogonality_nt`, was a
//! thin wrapper over exactly this native function).
//!
//! Python-fn → Rust-fn mapping:
//! - `test_annotation_set_covers_the_historical_seven_axes` → [`annotation_set_covers_the_historical_seven_axes`]
//! - `test_coequal_facet_lint_holds_on_the_real_matrix` → [`coequal_facet_lint_holds_on_the_real_matrix`]
//! - `test_coequal_facet_lint_catches_seeded_violations` → [`coequal_facet_lint_catches_seeded_violations`]
//! - `test_every_axis_property_exists_with_its_own_range` → [`every_axis_property_exists_with_its_own_range`]
//! - `test_no_axis_is_inferred_from_another` → [`no_axis_is_inferred_from_another`]
//! - `test_identity_axes_are_disjoint_classes_axiom` → [`identity_axes_are_disjoint_classes_axiom`]
//! - `test_no_preferred_or_primary_identity_term` → [`no_preferred_or_primary_identity_term`]

use crate::conformance_support::*;

use std::collections::BTreeSet;
use std::sync::Arc;

use gmeow_validate::gufo::{self, GufoConfig};
use purrdf::slice::rdf_query::Object;
use purrdf::{RdfDataset, flat_dataset_from_quads, flat_rdf_quads_from_dataset, parse_dataset};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const RDFS_SUB_PROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";

const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const OWL_ANNOTATION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";
const OWL_EQUIVALENT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#equivalentProperty";
const OWL_ALL_DISJOINT_CLASSES: &str = "http://www.w3.org/2002/07/owl#AllDisjointClasses";
const OWL_MEMBERS: &str = "http://www.w3.org/2002/07/owl#members";

const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

const GMEOW_COEQUAL_FACET: &str = "https://blackcatinformatics.ca/gmeow/coequalFacet";

fn gmeow(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// The seven orthogonal axis properties and the value/facet class each ranges
/// over — the REGRESSION pin. The live axis set is the `gmeow:coequalFacet`
/// annotation set, which must always cover (at least) these seven.
const AXES: [(&str, &str); 7] = [
    ("hasPronounSet", "PronounSet"), // address (names)
    ("honorific", "Honorific"),      // address (names)
    ("hasGenderIdentity", "GenderIdentity"),
    ("hasGenderExpression", "GenderExpression"),
    ("sexAssignedAtBirth", "SexAssignedAtBirth"),
    ("hasSexualOrientation", "SexualOrientation"), // orientation (sexuality)
    ("hasRomanticOrientation", "RomanticOrientation"),
];

// ── Annotation-set coverage ───────────────────────────────────────────────────

/// Twin of `test_annotation_set_covers_the_historical_seven_axes`.
///
/// Every historical axis carries `gmeow:coequalFacet true`. Superset, not
/// equality: new co-equal facets extend the matrix and are enforced automatically.
#[gmeow_test_batch_macros::batch_test]
fn annotation_set_covers_the_historical_seven_axes() {
    let g = GraphStore::ontology();
    for (prop, _range) in AXES {
        let node = gmeow(prop);
        assert!(
            g.has_literal(&node, GMEOW_COEQUAL_FACET, "true", XSD_BOOLEAN),
            "{prop} missing gmeow:coequalFacet true"
        );
    }
}

// ── Live-lint machinery (the production `gufo::coequal_facet_orthogonality`) ────

fn cfg() -> GufoConfig {
    GufoConfig {
        namespace: GMEOW.to_owned(),
    }
}

/// Twin of `test_coequal_facet_lint_holds_on_the_real_matrix`.
///
/// The annotation-driven lint (the live enforcement, native
/// `gufo::coequal_facet_orthogonality`, of which the Python
/// `reasoning_coequal_facet_orthogonality_nt` shim was a thin wrapper) is clean
/// over the merged ontology.
#[gmeow_test_batch_macros::batch_test]
fn coequal_facet_lint_holds_on_the_real_matrix() {
    let problems = gufo::coequal_facet_orthogonality(base_ontology_dataset(), &cfg());
    assert!(
        problems.is_empty(),
        "co-equal facet lint must be clean on the real matrix; got: {:?}",
        problems.iter().map(|f| &f.message).collect::<Vec<_>>()
    );
}

/// Twin of `test_coequal_facet_lint_catches_seeded_violations`.
///
/// Each violation class is detected when seeded into a copy of the ontology: a
/// functional-property axis, a subPropertyOf bridge, and a fresh axis (with its
/// own range) missing from the joint disjointness axiom.
#[gmeow_test_batch_macros::batch_test]
fn coequal_facet_lint_catches_seeded_violations() {
    let seeded = seeded_ontology();
    let problems = gufo::coequal_facet_orthogonality(&seeded, &cfg());
    let joined = problems
        .iter()
        .map(|f| f.message.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        joined.contains("owl:FunctionalProperty"),
        "expected a functional-property finding; got: {joined}"
    );
    assert!(
        joined.contains("bridged"),
        "expected a bridge finding; got: {joined}"
    );
    assert!(
        joined.contains("not jointly declared"),
        "expected a joint-disjointness finding; got: {joined}"
    );
}

/// The merged ontology plus the three seeded violation classes (mirrors the
/// Python `seeded` graph copy): a functional-property axis, a subPropertyOf
/// bridge between two axes, and a fresh co-equal axis whose own range is absent
/// from the joint disjointness axiom.
fn seeded_ontology() -> Arc<RdfDataset> {
    let mut quads = flat_rdf_quads_from_dataset(base_ontology_dataset());
    let seeded_ttl = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

gmeow:hasGenderIdentity a owl:FunctionalProperty .
gmeow:hasGenderIdentity rdfs:subPropertyOf gmeow:hasGenderExpression .
gmeow:hasTestFacet a owl:ObjectProperty .
gmeow:hasTestFacet gmeow:coequalFacet true .
gmeow:hasTestFacet rdfs:range gmeow:TestFacetValue .
";
    let ds = parse_dataset(seeded_ttl.as_bytes(), "text/turtle", None)
        .expect("seeded Turtle must parse");
    for mut quad in flat_rdf_quads_from_dataset(&ds) {
        quad.graph_name = None;
        quads.push(quad);
    }
    flat_dataset_from_quads(&quads).expect("seeded dataset must freeze")
}

// ── Per-axis structural facts ─────────────────────────────────────────────────

/// Twin of `test_every_axis_property_exists_with_its_own_range`.
///
/// Each axis is an object/datatype property that ranges over its facet/value
/// class EXCLUSIVELY (exactly one range), and all seven ranges are distinct.
#[gmeow_test_batch_macros::batch_test]
fn every_axis_property_exists_with_its_own_range() {
    let g = GraphStore::ontology();
    let mut ranges: BTreeSet<String> = BTreeSet::new();
    for (prop, rng) in AXES {
        let node = gmeow(prop);
        assert!(
            g.has(Some(&node), Some(RDF_TYPE), Some(OWL_OBJECT_PROPERTY))
                || g.has(Some(&node), Some(RDF_TYPE), Some(OWL_DATATYPE_PROPERTY)),
            "{prop} must be defined"
        );
        let declared = g.objects(&node, RDFS_RANGE);
        let expected: BTreeSet<String> = [gmeow(rng)].into_iter().collect();
        assert_eq!(declared, expected, "{prop} must range over only {rng}");
        ranges.insert(gmeow(rng));
    }
    // All seven ranges are distinct — no two axes share a value space.
    assert_eq!(ranges.len(), AXES.len());
}

/// Twin of `test_no_axis_is_inferred_from_another`.
///
/// For every ordered pair, no subProperty/equivalence bridge in either
/// direction (hostile-absence, replicated exactly).
#[gmeow_test_batch_macros::batch_test]
fn no_axis_is_inferred_from_another() {
    let g = GraphStore::ontology();
    for (i, (pa, _)) in AXES.iter().enumerate() {
        for (pb, _) in AXES.iter().skip(i + 1) {
            let a = gmeow(pa);
            let b = gmeow(pb);
            assert!(
                !g.has(Some(&a), Some(RDFS_SUB_PROPERTY_OF), Some(&b)),
                "{pa} ⊑ {pb} forbidden"
            );
            assert!(
                !g.has(Some(&b), Some(RDFS_SUB_PROPERTY_OF), Some(&a)),
                "{pb} ⊑ {pa} forbidden"
            );
            assert!(
                !g.has(Some(&a), Some(OWL_EQUIVALENT_PROPERTY), Some(&b)),
                "{pa} ≡ {pb} forbidden"
            );
            assert!(
                !g.has(Some(&b), Some(OWL_EQUIVALENT_PROPERTY), Some(&a)),
                "{pb} ≡ {pa} forbidden"
            );
        }
    }
}

// ── Disjointness axiom ────────────────────────────────────────────────────────

/// Every `owl:AllDisjointClasses`, as the set of named class IRIs it makes
/// disjoint (walking the blank `owl:members` `rdf:List`).
fn all_disjoint_member_sets(g: &GraphStore) -> Vec<BTreeSet<String>> {
    let mut sets: Vec<BTreeSet<String>> = Vec::new();
    for axiom in g.subjects_of_type_h(OWL_ALL_DISJOINT_CLASSES) {
        let Some(members) = g.value_h(&axiom, OWL_MEMBERS) else {
            continue;
        };
        let Some(head) = GraphStore::object_as_subject(&members) else {
            continue;
        };
        let members = g
            .rdf_list_h(&head)
            .into_iter()
            .filter_map(|o| match o {
                Object::Named(iri) => Some(iri),
                _ => None,
            })
            .collect();
        sets.push(members);
    }
    sets
}

/// Twin of `test_identity_axes_are_disjoint_classes_axiom`.
///
/// The matrix is an OWL theorem: the seven axis range classes are jointly
/// disjoint via a single `owl:AllDisjointClasses`, and (the load-bearing case)
/// the four IdentityFacet siblings are jointly disjoint too.
#[gmeow_test_batch_macros::batch_test]
fn identity_axes_are_disjoint_classes_axiom() {
    let g = GraphStore::ontology();
    let member_sets = all_disjoint_member_sets(&g);

    let axis_classes: BTreeSet<String> = AXES.iter().map(|(_, rng)| gmeow(rng)).collect();
    assert!(
        member_sets
            .iter()
            .any(|s| axis_classes.iter().all(|c| s.contains(c))),
        "the seven identity axes must share one owl:AllDisjointClasses"
    );

    let facets: BTreeSet<String> = [
        "GenderIdentity",
        "GenderExpression",
        "SexualOrientation",
        "RomanticOrientation",
    ]
    .into_iter()
    .map(gmeow)
    .collect();
    assert!(
        member_sets
            .iter()
            .any(|s| facets.iter().all(|c| s.contains(c))),
        "the four IdentityFacet siblings must be jointly disjoint"
    );
}

// ── Co-equality guard ─────────────────────────────────────────────────────────

/// Twin of `test_no_preferred_or_primary_identity_term`.
///
/// Co-equality across identity axes: no preferred/primary marker anywhere (as a
/// property of any kind, or a class).
#[gmeow_test_batch_macros::batch_test]
fn no_preferred_or_primary_identity_term() {
    let g = GraphStore::ontology();
    for banned in [
        "primaryGender",
        "preferredGender",
        "primaryOrientation",
        "preferredOrientation",
        "primaryIdentity",
        "preferredIdentity",
    ] {
        let node = gmeow(banned);
        for pt in [
            OWL_OBJECT_PROPERTY,
            OWL_DATATYPE_PROPERTY,
            OWL_ANNOTATION_PROPERTY,
            OWL_CLASS,
        ] {
            assert!(
                !g.has(Some(&node), Some(RDF_TYPE), Some(pt)),
                "{banned} must not exist (as {pt})"
            );
        }
    }
}
