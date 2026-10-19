// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::{ClosureEntry, CoveredTerm, GroundingMargins};
use purrdf::{DatasetView, GraphMatch, TermRef};

fn packet(term: &str, dependencies: &[&str]) -> AuthoringPacket {
    AuthoringPacket {
        packet_iri: "urn:packet:selection".into(),
        source_slice: "urn:packet:slice".into(),
        axis: None,
        batch: 0,
        digest: String::new(),
        term_count: 1,
        exemplar_shortfall: 0,
        margins: GroundingMargins::default(),
        exemplars: vec![],
        grounding: vec![],
        terms: vec![CoveredTerm {
            iri: term.into(),
            label: None,
            definition: None,
            coat: vec![],
            axioms: vec![],
            neighbors: vec![],
            content_digest: String::new(),
            closure: dependencies
                .iter()
                .map(|iri| ClosureEntry {
                    iri: (*iri).into(),
                    label: None,
                    definition: None,
                })
                .collect(),
        }],
    }
}

#[test]
fn packet_theory_keeps_source_terms_graphs_and_statement_metadata() {
    let source = purrdf::parse_dataset(
        br#"
        @prefix ex: <urn:packet:> .
        @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
        ex:Root ex:edge _:x . _:x ex:next _:x ; ex:value 7 .
        ex:Dependency ex:law ex:Rule .
        ex:Unrelated ex:unused ex:Other .
        ex:scope { ex:claim rdf:reifies <<( ex:Root ex:said ex:Value )>> ; ex:confidence "firm" . }
    "#,
        "application/trig",
        None,
    )
    .unwrap();
    let source = source.as_ref();
    let prepared = CandidatePacket::from_source(
        packet("urn:packet:Root", &["urn:packet:Dependency"]),
        source,
    )
    .unwrap();
    let kb = prepared.knowledge_base();
    let root = TermValue::iri("urn:packet:Root");
    let original = source
        .quads_for_pattern(source.term_id_by_value(&root), None, None, GraphMatch::Any)
        .next()
        .unwrap();
    let selected = kb
        .quads_for_pattern(kb.term_id_by_value(&root), None, None, GraphMatch::Any)
        .next()
        .unwrap();
    assert!(matches!(source.resolve(original.o), TermRef::Blank { .. }));
    assert_eq!(source.term_value(original.o), kb.term_value(selected.o));
    assert!(
        kb.term_id_by_value(&TermValue::iri("urn:packet:Dependency"))
            .is_some()
    );
    assert!(
        kb.term_id_by_value(&TermValue::iri("urn:packet:Unrelated"))
            .is_none()
    );
    let (_, triple, graph) = kb
        .reifiers_with_graph()
        .next()
        .expect("the selected statement's reifier");
    assert!(matches!(kb.resolve(triple), TermRef::Triple { .. }));
    assert_eq!(
        graph.map(|id| kb.term_value(id)),
        Some(TermValue::iri("urn:packet:scope"))
    );
    assert_eq!(kb.annotations_with_graph().count(), 1);
    let wire = prepared.knowledge_base_nquads().unwrap();
    let restored = purrdf::parse_dataset(wire.as_bytes(), "application/n-quads", None).unwrap();
    assert_eq!(restored.reifiers_with_graph().count(), 1);
    assert_eq!(restored.annotations_with_graph().count(), 1);
    assert!(wire.contains("urn:packet:scope"));
    assert!(wire.contains("firm"));
}

#[test]
fn packet_theory_rejects_a_term_absent_from_its_source() {
    let source = purrdf::parse_dataset(
        b"<urn:other> <urn:predicate> <urn:value> .",
        "text/turtle",
        None,
    )
    .unwrap();
    let error = CandidatePacket::from_source(packet("urn:missing", &[]), &source)
        .err()
        .expect("a foreign packet must refuse");
    assert_eq!(error.code(), crate::error::Partition::register());
}

#[test]
fn selection_packets_remain_valid_but_candidate_packets_require_a_module() {
    let fixture = tempfile::tempdir().unwrap();
    std::fs::write(
        fixture.path().join("manifest.ttl"),
        "<urn:packet:slice> a <https://blackcatinformatics.ca/gmeow/Slice> .",
    )
    .unwrap();
    let tiers = std::collections::BTreeMap::new();
    let inputs = crate::BriefInputs {
        slice_dir: fixture.path(),
        axis: None,
        batch: None,
        exemplar_tiers: &tiers,
        exemplar_target: 0,
    };
    assert_eq!(crate::assemble_packet(&inputs).unwrap().term_count, 0);
    let shapes = purrdf::shapes::engine::parse_shapes("", None).unwrap();
    assert!(
        crate::exemplar_tiers(fixture.path(), &shapes)
            .unwrap()
            .is_empty()
    );
    let error = crate::assemble_candidate_packet(&inputs)
        .err()
        .expect("a selected module may not disappear into an empty theory");
    assert!(error.to_string().contains("module.ttl"), "{error}");
}

#[test]
fn candidate_packet_uses_the_canonical_assembly_and_partition() {
    let fixture = tempfile::tempdir().unwrap();
    std::fs::write(
        fixture.path().join("manifest.ttl"),
        "<https://example.test/packet#slice> a <https://blackcatinformatics.ca/gmeow/Slice> .",
    )
    .unwrap();
    std::fs::write(
        fixture.path().join("module.ttl"),
        r#"
        @prefix ex: <https://example.test/packet#> .
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
        ex:A rdfs:isDefinedBy ex:slice ; rdfs:label "A" ; ex:definition ex:Dependency .
        ex:B rdfs:isDefinedBy ex:slice ; rdfs:label "B" .
        ex:Dependency rdfs:label "Dependency" .
    "#,
    )
    .unwrap();
    let tiers = std::collections::BTreeMap::new();
    let inputs = crate::BriefInputs {
        slice_dir: fixture.path(),
        axis: Some("A"),
        batch: Some(0),
        exemplar_tiers: &tiers,
        exemplar_target: 0,
    };
    let expected = crate::assemble_packet(&inputs).unwrap();
    let prepared = crate::assemble_candidate_packet(&inputs).unwrap();
    assert_eq!(prepared.packet, expected);
    assert_eq!(prepared.packet.term_count, 1);
    assert_eq!(
        prepared.packet.terms[0].iri,
        "https://example.test/packet#A"
    );
    assert!(
        prepared
            .knowledge_base()
            .term_id_by_value(&TermValue::iri("https://example.test/packet#Dependency"))
            .is_some()
    );
    let invalid = crate::BriefInputs {
        batch: Some(1),
        ..inputs
    };
    assert!(crate::assemble_candidate_packet(&invalid).is_err());
}
