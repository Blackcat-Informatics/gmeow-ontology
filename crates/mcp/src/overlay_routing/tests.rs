// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use purrdf::{BlankScope, RdfDatasetBuilder, RdfLocation, RdfTerm};

use super::transient_union;
use crate::EXTERNAL_OVERLAY_GRAPH;

const SOURCE: &str = "urn:overlay:source-world";
const ACCORDING_TO: &str = "https://blackcatinformatics.ca/gmeow/accordingTo";

#[test]
fn overlay_placement_keeps_complete_claims_and_independent_canon_identity() {
    let mut canon = RdfDatasetBuilder::new();
    let canon_member = canon.intern_blank("member", BlankScope::DEFAULT);
    let predicate = canon.intern_iri("urn:overlay:canon-only");
    let object = canon.intern_iri("urn:overlay:canon-value");
    let canon_graph = canon.intern_iri("urn:overlay:canon-world");
    canon.push_quad(canon_member, predicate, object, Some(canon_graph));
    let canon = canon.freeze().unwrap();

    let mut builder = RdfDatasetBuilder::new();
    // This qualified source scope would alias the canon's rebased scope if
    // overlay rows were pushed through the unscoped owned-record API.
    let member = builder.intern_blank("member", BlankScope(1));
    let other = builder.intern_blank("member", BlankScope(9));
    let predicate = builder.intern_iri("urn:overlay:relation");
    let claim = builder.intern_blank("claim", BlankScope(1));
    let source = builder.intern_iri(SOURCE);
    let unused = builder.intern_iri("urn:overlay:empty-source-world");
    builder.declare_named_graph(unused);
    let according_to = builder.intern_iri(ACCORDING_TO);
    let row = builder.push_quad_with_handle(member, predicate, other, Some(source));
    let location = RdfLocation::logical("inline external evidence");
    builder.attach_location(row, location.clone());
    let statement = builder.intern_triple(member, predicate, other);
    builder.push_reifier_in_graph(claim, statement, Some(source));
    builder.push_annotation_in_graph(claim, according_to, source, Some(source));
    let overlay = builder.freeze().unwrap();
    let original_rows = overlay.owned_quads().collect::<Vec<_>>();

    for include_canon in [false, true] {
        let union = transient_union(include_canon.then_some(canon.as_ref()), &overlay).unwrap();
        let rows = union
            .owned_quads()
            .filter(|q| q.predicate == "urn:overlay:relation")
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 2);
        assert_ne!(rows[0].subject, rows[0].object);
        assert_eq!(rows[0].subject, rows[1].subject);
        assert_eq!(rows[0].object, rows[1].object);
        assert!(rows.iter().all(|q| q.location.as_ref() == Some(&location)));
        assert_eq!(union.reifiers().count(), 2);
        assert_eq!(union.annotations().count(), 2);
        for graph in [None, Some(RdfTerm::iri(EXTERNAL_OVERLAY_GRAPH))] {
            let row = rows.iter().find(|q| q.graph_name == graph).unwrap();
            let reifier = union.owned_reifiers().find(|r| r.graph == graph).unwrap();
            let annotation = union
                .owned_annotations()
                .find(|a| a.graph == graph)
                .unwrap();
            assert_eq!(reifier.statement.subject, row.subject);
            assert_eq!(reifier.statement.object, row.object);
            assert_eq!(annotation.reifier, reifier.reifier);
            assert_eq!(annotation.predicate, ACCORDING_TO);
            assert_eq!(annotation.object, RdfTerm::iri(SOURCE));
        }
        let graphs = union.owned_named_graphs().collect::<Vec<_>>();
        assert!(graphs.contains(&RdfTerm::iri(EXTERNAL_OVERLAY_GRAPH)));
        assert!(!graphs.contains(&RdfTerm::iri(SOURCE)));
        assert!(!graphs.contains(&RdfTerm::iri("urn:overlay:empty-source-world")));
        let canon_rows = union
            .owned_quads()
            .filter(|q| q.predicate == "urn:overlay:canon-only")
            .collect::<Vec<_>>();
        assert_eq!(canon_rows.len(), usize::from(include_canon));
        if include_canon {
            assert_ne!(canon_rows[0].subject, rows[0].subject);
            assert_eq!(
                canon_rows[0].graph_name,
                Some(RdfTerm::iri("urn:overlay:canon-world"))
            );
        }
    }
    assert_eq!(overlay.owned_quads().collect::<Vec<_>>(), original_rows);
    assert_eq!(canon.quad_count(), 1);
    assert_eq!(
        canon.owned_quads().next().unwrap().subject,
        RdfTerm::blank_node("member")
    );
}

#[test]
fn empty_overlay_still_declares_only_its_selected_external_origin() {
    let mut source = RdfDatasetBuilder::new();
    let stale = source.intern_iri(SOURCE);
    source.declare_named_graph(stale);
    let source = source.freeze().unwrap();
    let output = transient_union(None, &source).unwrap();
    assert_eq!(output.quad_count(), 0);
    assert_eq!(output.reifiers().count(), 0);
    assert_eq!(output.annotations().count(), 0);
    assert_eq!(
        output.owned_named_graphs().collect::<Vec<_>>(),
        [RdfTerm::iri(EXTERNAL_OVERLAY_GRAPH)]
    );
    assert_eq!(
        source.owned_named_graphs().collect::<Vec<_>>(),
        [RdfTerm::iri(SOURCE)]
    );
}

#[cfg(feature = "reasoning")]
#[test]
fn verification_overlay_limit_includes_statement_only_evidence() {
    let mut source = RdfDatasetBuilder::new();
    let subject = source.intern_iri("urn:overlay:subject");
    let predicate = source.intern_iri("urn:overlay:relation");
    let statement = source.intern_triple(subject, predicate, subject);
    let claim = source.intern_iri("urn:overlay:claim");
    source.push_reifier(claim, statement);
    source.push_annotation(claim, predicate, subject);
    let source = source.freeze().unwrap();
    assert_eq!(source.quad_count(), 0);
    assert!(super::verify_record_limit(&source, 2).is_ok());
    let error = super::verify_record_limit(&source, 1)
        .unwrap_err()
        .to_string();
    assert!(error.contains("2 RDF records"), "{error}");
    assert!(error.contains("exceeding the 1 quad ceiling"), "{error}");
}
