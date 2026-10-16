// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::{BlankScope, DatasetView, GraphMatch, TermValue};

#[test]
fn conformance_view_preserves_default_claims_and_excludes_other_worlds() {
    // gmeow-test-input: synthetic-only
    let mut builder = RdfDatasetBuilder::new();
    let subject = builder.intern_blank("claim", BlankScope(17)).unwrap();
    let predicate = builder.intern_iri(gmeow_ns::LOGIC_SUB_CLASS_OF).unwrap();
    let object = builder.intern_iri(gmeow_ns::LOGIC_THING).unwrap();
    builder.push_quad(subject, predicate, object, None).unwrap();
    let quoted_only = builder.intern_iri("urn:quoted-only").unwrap();
    let statement = builder
        .intern_triple(quoted_only, predicate, object)
        .unwrap();
    let reifier = builder.intern_iri("urn:default-claim").unwrap();
    builder
        .push_reifier_in_graph(reifier, statement, None)
        .unwrap();
    let provenance = builder.intern_iri("urn:source").unwrap();
    let source = builder.intern_iri("urn:default-source").unwrap();
    builder
        .push_annotation_in_graph(reifier, provenance, source, None)
        .unwrap();
    let other_world = builder.intern_iri("urn:other-world").unwrap();
    let other_reifier = builder.intern_iri("urn:other-claim").unwrap();
    builder
        .push_quad(subject, predicate, object, Some(other_world))
        .unwrap();
    builder
        .push_reifier_in_graph(other_reifier, statement, Some(other_world))
        .unwrap();
    builder
        .push_annotation_in_graph(other_reifier, provenance, source, Some(other_world))
        .unwrap();
    let source = builder.freeze().unwrap();

    let prepared = ConformanceOntology::new(&source).unwrap();
    let reader = prepared.reader().unwrap();
    assert!(Arc::ptr_eq(reader, prepared.reader().unwrap()));
    assert_eq!(reader.named_graphs().count(), 0);
    let subject = reader
        .term_id_by_blank("claim", BlankScope(17))
        .expect("the selected default claim keeps its scoped blank identity");
    let alias = reader
        .term_id_by_iri(gmeow_ns::RDFS_SUB_CLASS_OF)
        .expect("the canonical reader view includes the class relation");
    let top = reader.term_id_by_iri(gmeow_ns::OWL_THING).unwrap();
    assert_eq!(
        reader
            .quads_for_pattern(Some(subject), Some(alias), Some(top), GraphMatch::Default)
            .count(),
        1
    );
    let claim = reader.term_id_by_iri("urn:default-claim").unwrap();
    let provenance = reader.term_id_by_iri("urn:source").unwrap();
    assert_eq!(
        reader
            .quads_for_pattern(Some(claim), Some(provenance), None, GraphMatch::Default)
            .count(),
        1
    );
    let reifies = reader
        .term_id_by_iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies")
        .unwrap();
    assert_eq!(
        reader
            .quads_for_pattern(Some(claim), Some(reifies), None, GraphMatch::Default)
            .count(),
        1
    );
    let quoted_only = reader.term_id_by_iri("urn:quoted-only").unwrap();
    assert_eq!(
        reader
            .quads_for_pattern(Some(quoted_only), None, None, GraphMatch::Any)
            .count(),
        0,
        "a statement quoted by the selected claim never becomes asserted"
    );
    assert!(
        reader
            .as_ref()
            .term_id_by_value(&TermValue::iri("urn:other-claim"))
            .is_none()
    );
    assert_eq!(source.reifiers_with_graph().count(), 2);
    assert_eq!(source.annotations_with_graph().count(), 2);
    assert_eq!(source.named_graphs().count(), 1);
}
