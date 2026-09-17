// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Synthetic GMEOW context admission and provenance regressions; no corpus work.

use std::sync::Arc;

use purrdf::{
    RdfAnnotation, RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfReifier, RdfTerm, RdfTriple,
};

use super::*;
use crate::entail::{
    ENTAIL_RESERVED_NS, ENTAIL_WORLD, EntailmentVerdict, RDF_TYPE, RDFS_SUBCLASSOF,
    RDFS_SUBPROPERTYOF, VendorReduction, build_world_edb, dl_entails, reduce_for_vendoring,
};

fn quad(subject: &str, predicate: &str, object: &str) -> RdfQuad {
    RdfQuad::new(RdfTerm::iri(subject), predicate, RdfTerm::iri(object))
}

fn dataset(quads: impl IntoIterator<Item = RdfQuad>) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for quad in quads {
        builder.push_owned_quad(&quad);
    }
    builder.freeze().expect("synthetic dataset")
}

fn goal() -> Arc<RdfDataset> {
    dataset([quad("urn:x", RDF_TYPE, "urn:B")])
}

fn context_gap(premise: &RdfDataset, conclusion: &RdfDataset, side: &str, source: &str) {
    let EntailmentVerdict::Gap(gap) = dl_entails(premise, conclusion).expect("typed admission")
    else {
        panic!("context selection must be refused before reasoning");
    };
    assert_eq!(gap.shape, GapShape::NativeCoverage);
    assert!(gap.detail.contains(PROFILE), "{gap:?}");
    assert!(gap.detail.contains(side), "{gap:?}");
    assert!(gap.detail.contains(source), "{gap:?}");
    let VendorReduction::Gap(vendor_gap) =
        reduce_for_vendoring(premise, conclusion).expect("typed vendor admission")
    else {
        panic!("vendoring must not bypass the same context admission");
    };
    assert_eq!(vendor_gap, gap);
}

#[test]
fn separate_context_facts_cannot_form_an_unscoped_entailment() {
    let premise = dataset([
        quad("urn:x", RDF_TYPE, "urn:A").in_graph(RdfTerm::iri("urn:alice")),
        quad("urn:A", RDFS_SUBCLASSOF, "urn:B").in_graph(RdfTerm::iri("urn:bob")),
    ]);
    context_gap(&premise, &goal(), "premise", "urn:alice");
    assert_eq!(
        premise.named_graphs().count(),
        2,
        "original contexts remain"
    );
}

#[test]
fn a_default_fact_cannot_prove_a_conclusion_in_another_context() {
    let premise = goal();
    for graph in [RdfTerm::iri("urn:bob"), RdfTerm::blank_node("bob")] {
        let conclusion = dataset([quad("urn:x", RDF_TYPE, "urn:B").in_graph(graph)]);
        context_gap(&premise, &conclusion, "conclusion", "bob");
    }
}

#[test]
fn declaration_only_contexts_do_not_disappear_from_admission() {
    let mut builder = RdfDatasetBuilder::new();
    let context = builder.intern_iri("urn:unselected-context");
    builder.declare_named_graph(context);
    let declared = builder.freeze().unwrap();
    assert_eq!(declared.quad_count(), 0);
    context_gap(&declared, &goal(), "premise", "urn:unselected-context");
    context_gap(&goal(), &declared, "conclusion", "urn:unselected-context");
}

#[test]
fn native_metadata_graphs_are_subject_to_the_same_context_admission() {
    let world = RdfTerm::iri("urn:metadata-context");
    let mut reifiers = RdfDatasetBuilder::new();
    reifiers.push_owned_reifier(
        &RdfReifier::new(
            RdfTerm::iri("urn:claim"),
            RdfTriple::new(RdfTerm::iri("urn:x"), RDF_TYPE, RdfTerm::iri("urn:B")),
        )
        .in_graph(Some(world.clone())),
    );
    let mut annotations = RdfDatasetBuilder::new();
    annotations.push_owned_annotation(
        &RdfAnnotation::new(
            RdfTerm::iri("urn:claim"),
            "urn:reportedBy",
            RdfTerm::iri("urn:alice"),
        )
        .in_graph(Some(world)),
    );
    for source in [reifiers.freeze().unwrap(), annotations.freeze().unwrap()] {
        assert_eq!(source.quad_count(), 0);
        context_gap(&source, &goal(), "premise", "urn:metadata-context");
        context_gap(&goal(), &source, "conclusion", "urn:metadata-context");
    }
}

#[test]
fn asserted_and_native_annotation_standpoints_share_admission() {
    let predicate = "https://blackcatinformatics.ca/gmeow/accordingTo";
    let mut native = RdfDatasetBuilder::new();
    native.push_owned_quad(&quad("urn:x", RDF_TYPE, "urn:B"));
    native.push_owned_reifier(&RdfReifier::new(
        RdfTerm::iri("urn:claim"),
        RdfTriple::new(RdfTerm::iri("urn:x"), RDF_TYPE, RdfTerm::iri("urn:B")),
    ));
    native.push_owned_annotation(&RdfAnnotation::new(
        RdfTerm::iri("urn:claim"),
        predicate,
        RdfTerm::iri("urn:alice"),
    ));
    let native = native.freeze().unwrap();
    let ordinary = dataset([
        quad("urn:x", RDF_TYPE, "urn:B"),
        quad("urn:claim", predicate, "urn:alice"),
    ]);
    for premise in [native, ordinary] {
        context_gap(&premise, &goal(), "premise", "urn:claim");
    }
}

#[test]
fn canonical_scope_coordinates_are_not_unconditional_data() {
    for coordinate in [
        "standpoint",
        "world",
        "time",
        "path",
        "modality",
        "inModule",
    ] {
        let predicate = format!("https://blackcatinformatics.ca/logic/{coordinate}");
        let premise = dataset([
            quad("urn:x", RDF_TYPE, "urn:B"),
            quad("urn:claim", &predicate, "urn:selected-coordinate"),
        ]);
        context_gap(&premise, &goal(), "premise", &predicate);
    }
}

#[test]
fn admitted_default_context_retains_positive_and_negative_entailment() {
    let premise = dataset([
        quad("urn:x", RDF_TYPE, "urn:A"),
        quad("urn:A", RDFS_SUBCLASSOF, "urn:B"),
    ]);
    assert_eq!(
        dl_entails(&premise, &goal()).unwrap(),
        EntailmentVerdict::Entailed
    );
    let unrelated = dataset([quad("urn:x", RDF_TYPE, "urn:C")]);
    assert_eq!(
        dl_entails(&premise, &unrelated).unwrap(),
        EntailmentVerdict::NotEntailed
    );
}

#[test]
fn native_provenance_and_quoted_scope_survive_without_asserting_the_quote() {
    let mut builder = RdfDatasetBuilder::new();
    let asserted = quad("urn:x", RDF_TYPE, "urn:A")
        .with_location(purrdf::RdfLocation::logical("synthetic-source-occurrence"));
    builder.push_owned_quad(&asserted);
    builder.push_owned_quad(&quad("urn:A", RDFS_SUBCLASSOF, "urn:B"));
    let quoted = RdfTriple::new(RdfTerm::iri("urn:x"), RDF_TYPE, RdfTerm::iri("urn:C"));
    let reifier = RdfReifier::new(RdfTerm::blank_node("claim"), quoted.clone());
    builder.push_owned_reifier(&reifier);
    let provenance = RdfAnnotation::new(
        RdfTerm::blank_node("claim"),
        "https://blackcatinformatics.ca/logic/provenance",
        RdfTerm::iri("urn:source"),
    );
    let confidence = RdfAnnotation::new(
        RdfTerm::blank_node("claim"),
        CONFIDENCE,
        RdfTerm::literal(RdfLiteral::typed(
            "0.5",
            "http://www.w3.org/2001/XMLSchema#decimal",
        )),
    );
    builder.push_owned_annotation(&provenance);
    builder.push_owned_annotation(&confidence);
    // Quoting a context coordinate does not select that context for this theory.
    builder.push_owned_reifier(&RdfReifier::new(
        RdfTerm::iri("urn:scope-description"),
        RdfTriple::new(
            RdfTerm::iri("urn:other-claim"),
            "https://blackcatinformatics.ca/logic/standpoint",
            RdfTerm::iri("urn:bob"),
        ),
    ));
    let premise = builder.freeze().unwrap();
    let admitted = AdmittedDefaultGraph::new(&premise, "premise").unwrap();
    let reduced = build_world_edb(&admitted, &[]).unwrap();
    let world = RdfTerm::iri(ENTAIL_WORLD);
    assert!(
        reduced
            .owned_quads()
            .any(|row| row == asserted.clone().in_graph(world.clone()))
    );
    assert!(
        reduced
            .owned_reifiers()
            .any(|row| row == reifier.clone().in_graph(Some(world.clone())))
    );
    for annotation in [provenance, confidence] {
        assert!(
            reduced
                .owned_annotations()
                .any(|row| row == annotation.clone().in_graph(Some(world.clone())))
        );
    }
    assert!(
        !reduced
            .owned_quads()
            .any(|row| row.subject == quoted.subject
                && row.predicate == quoted.predicate
                && row.object == quoted.object)
    );
    assert_eq!(
        dl_entails(&premise, &goal()).unwrap(),
        EntailmentVerdict::Entailed
    );
    let quoted_goal = dataset([quad("urn:x", RDF_TYPE, "urn:C")]);
    assert_eq!(
        dl_entails(&premise, &quoted_goal).unwrap(),
        EntailmentVerdict::NotEntailed
    );
}

#[test]
fn native_conclusion_metadata_is_not_an_empty_tautology() {
    let mut builder = RdfDatasetBuilder::new();
    builder.push_owned_annotation(&RdfAnnotation::new(
        RdfTerm::iri("urn:claim"),
        "urn:reportedBy",
        RdfTerm::iri("urn:alice"),
    ));
    let conclusion = builder.freeze().unwrap();
    assert_eq!(conclusion.quad_count(), 0);
    assert!(matches!(
        dl_entails(&goal(), &conclusion).unwrap(),
        EntailmentVerdict::Gap(EntailmentGap {
            shape: GapShape::RoleAssertion,
            ..
        })
    ));
    assert!(matches!(
        reduce_for_vendoring(&goal(), &conclusion).unwrap(),
        VendorReduction::Gap(EntailmentGap {
            shape: GapShape::RoleAssertion,
            ..
        })
    ));
}

#[test]
fn native_property_annotations_participate_in_the_same_reachability_decision() {
    let mut builder = RdfDatasetBuilder::new();
    builder.push_owned_annotation(&RdfAnnotation::new(
        RdfTerm::iri("urn:P"),
        RDFS_SUBPROPERTYOF,
        RdfTerm::iri("urn:Q"),
    ));
    let premise = builder.freeze().unwrap();
    assert_eq!(premise.quad_count(), 0);
    let positive = dataset([quad("urn:P", RDFS_SUBPROPERTYOF, "urn:Q")]);
    let negative = dataset([quad("urn:Q", RDFS_SUBPROPERTYOF, "urn:P")]);
    assert_eq!(
        dl_entails(&premise, &positive).unwrap(),
        EntailmentVerdict::Entailed
    );
    assert_eq!(
        dl_entails(&premise, &negative).unwrap(),
        EntailmentVerdict::NotEntailed
    );
}

#[test]
fn empty_conjunction_is_only_a_tautology_even_for_unselected_premise_contexts() {
    let premise = dataset([quad("urn:x", RDF_TYPE, "urn:B").in_graph(RdfTerm::iri("urn:alice"))]);
    let empty = dataset([]);
    assert_eq!(
        dl_entails(&premise, &empty).unwrap(),
        EntailmentVerdict::Entailed
    );
    context_gap(&premise, &goal(), "premise", "urn:alice");
}

#[test]
fn reserved_minting_namespace_cannot_hide_in_native_evidence() {
    let mut builder = RdfDatasetBuilder::new();
    builder.push_owned_quad(&quad("urn:x", RDF_TYPE, "urn:B"));
    builder.push_owned_annotation(&RdfAnnotation::new(
        RdfTerm::iri("urn:claim"),
        "urn:evidence",
        RdfTerm::triple(RdfTriple::new(
            RdfTerm::iri("urn:item"),
            "urn:value",
            RdfTerm::literal(RdfLiteral::typed(
                "opaque",
                format!("{ENTAIL_RESERVED_NS}datatype"),
            )),
        )),
    ));
    let premise = builder.freeze().unwrap();
    let error = dl_entails(&premise, &goal()).unwrap_err();
    assert!(
        error.message().contains("reserved entailment IRI"),
        "{error}"
    );
    assert!(error.message().contains("datatype"), "{error}");
}
