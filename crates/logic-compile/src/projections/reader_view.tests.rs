// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::{RdfTerm, parse_dataset};
use std::collections::HashSet;

#[test]
fn shared_axiom_projection_preserves_context_and_quoted_claims() {
    let source = parse_dataset(
        br"@prefix ex: <https://example.org/> .
               @prefix logic: <https://blackcatinformatics.ca/logic/> .
               @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
               ex:context {
                   ex:Child logic:subClassOf logic:Thing .
                   ex:leg logic:sourceEndpoint logic:Thing, logic:Nothing .
                   ex:Empty logic:complementOf logic:Thing .
                   ex:Universal logic:complementOf logic:Nothing .
                   ex:Mixed <http://www.w3.org/2002/07/owl#complementOf> logic:Thing .
                   ex:record rdf:reifies <<(ex:Child logic:subClassOf logic:Thing)>> ;
                       logic:accordingTo ex:standpoint .
               }",
        "application/trig",
        None,
    )
    .unwrap();
    let projected = with_owl_rdfs_projection(&source);
    let quads: HashSet<_> = projected.owned_quads().collect();
    assert!(source.owned_quads().all(|quad| quads.contains(&quad)));
    assert!(quads.iter().any(
        |quad| quad.subject == RdfTerm::iri("https://example.org/Child")
            && quad.predicate == gmeow_ns::RDFS_SUB_CLASS_OF
            && quad.object == RdfTerm::iri(gmeow_ns::OWL_THING)
            && quad.graph_name == Some(RdfTerm::iri("https://example.org/context"))
    ));
    assert!(!quads.iter().any(|quad| {
        quad.subject == RdfTerm::iri("https://example.org/leg")
            && [gmeow_ns::OWL_THING, gmeow_ns::OWL_NOTHING]
                .iter()
                .any(|marker| quad.object == RdfTerm::iri(*marker))
    }));
    for (subject, target) in [
        ("Empty", gmeow_ns::OWL_THING),
        ("Universal", gmeow_ns::OWL_NOTHING),
        ("Mixed", gmeow_ns::OWL_THING),
    ] {
        assert!(
            quads.iter().any(|quad| quad.subject
                == RdfTerm::iri(format!("https://example.org/{subject}"))
                && quad.predicate == "http://www.w3.org/2002/07/owl#complementOf"
                && quad.object == RdfTerm::iri(target)
                && quad.graph_name == Some(RdfTerm::iri("https://example.org/context"))),
            "class complement must preserve its selected graph and project its actual class operand"
        );
    }
    assert_eq!(
        source.owned_reifiers().collect::<Vec<_>>(),
        projected.owned_reifiers().collect::<Vec<_>>()
    );
    assert_eq!(
        source.owned_annotations().collect::<Vec<_>>(),
        projected.owned_annotations().collect::<Vec<_>>()
    );

    let separately_projected = purrdf::shapes::engine::project_dataset(&projected).unwrap();
    let fused = shacl_reader_view(&source);
    assert_eq!(
        separately_projected.owned_quads().collect::<HashSet<_>>(),
        fused.owned_quads().collect()
    );
    assert!(fused.owned_quads().all(|quad| quad.graph_name.is_none()));
}

#[test]
fn annotation_complements_keep_native_ownership_and_project_class_operands() {
    const COMPLEMENT: &str = "https://blackcatinformatics.ca/logic/complementOf";
    const OWL_COMPLEMENT: &str = "http://www.w3.org/2002/07/owl#complementOf";
    const ENDPOINT: &str = "https://blackcatinformatics.ca/logic/sourceEndpoint";
    const PROVENANCE: &str = "http://www.w3.org/ns/prov#wasDerivedFrom";
    const STANDPOINT: &str = "https://blackcatinformatics.ca/logic/accordingTo";
    let mut builder = purrdf::RdfDatasetBuilder::new();
    let complement = builder.intern_iri(COMPLEMENT);
    let endpoint = builder.intern_iri(ENDPOINT);
    let provenance = builder.intern_iri(PROVENANCE);
    let standpoint = builder.intern_iri(STANDPOINT);
    let attributed = builder.intern_iri("urn:complement:standpoint");
    let quoted_subject = builder.intern_iri("urn:complement:quoted-only");
    let thing = builder.intern_iri(gmeow_ns::LOGIC_THING);
    let quoted = builder.intern_triple(quoted_subject, complement, thing);
    for (scope, world, operand) in [
        (71, "urn:complement:world-a", gmeow_ns::LOGIC_THING),
        (72, "urn:complement:world-b", gmeow_ns::LOGIC_NOTHING),
    ] {
        let reifier = builder.intern_blank("class", purrdf::BlankScope(scope));
        let graph = builder.intern_iri(world);
        let operand = builder.intern_iri(operand);
        builder.push_reifier_in_graph(reifier, quoted, Some(graph));
        builder.push_annotation_in_graph(reifier, complement, operand, Some(graph));
        builder.push_annotation_in_graph(reifier, endpoint, operand, Some(graph));
        builder.push_annotation_in_graph(reifier, provenance, quoted, Some(graph));
        builder.push_annotation_in_graph(reifier, standpoint, attributed, Some(graph));
    }
    let source = builder.freeze().unwrap();
    let original: Vec<_> = source.owned_annotations().collect();
    let projected = with_owl_rdfs_projection(&source);
    let annotations: Vec<_> = projected.owned_annotations().collect();
    assert_eq!(original.len(), 8);
    assert_eq!(
        annotations.len(),
        14,
        "only the two class assertions gain three projected variants each"
    );
    assert!(original.iter().all(|row| annotations.contains(row)));
    assert_eq!(
        source.owned_reifiers().collect::<Vec<_>>(),
        projected.owned_reifiers().collect::<Vec<_>>()
    );
    assert_eq!(
        projected.quads().count(),
        0,
        "annotation ownership and quoted-only claims must not become ordinary assertions"
    );
    for source_row in original.iter().filter(|row| row.predicate == COMPLEMENT) {
        let target = if source_row.object == RdfTerm::iri(gmeow_ns::LOGIC_THING) {
            gmeow_ns::OWL_THING
        } else {
            assert_eq!(source_row.object, RdfTerm::iri(gmeow_ns::LOGIC_NOTHING));
            gmeow_ns::OWL_NOTHING
        };
        let mut expected = source_row.clone();
        expected.predicate = OWL_COMPLEMENT.to_owned();
        expected.object = RdfTerm::iri(target);
        assert!(
            annotations.contains(&expected),
            "projected complement retains exact blank scope and asserting world"
        );
    }
    assert!(
        annotations
            .iter()
            .filter(|row| [ENDPOINT, PROVENANCE, STANDPOINT].contains(&row.predicate.as_str()))
            .all(|row| original.contains(row)),
        "data endpoints, standpoint and quoted provenance must not acquire aliases"
    );

    let flat = shacl_reader_view(&source);
    let flat_rows: HashSet<_> = flat.owned_quads().collect();
    for annotation in annotations {
        let expected =
            purrdf::RdfQuad::new(annotation.reifier, annotation.predicate, annotation.object);
        assert!(
            flat_rows.contains(&expected),
            "the explicit SHACL view includes every projected annotation assertion"
        );
    }
    assert!(
        flat_rows.iter().all(|row| row.graph_name.is_none()
            && row.subject != RdfTerm::iri("urn:complement:quoted-only"))
    );
    assert_eq!(flat.annotations_with_graph().count(), 0);
    assert_eq!(flat.reifiers_with_graph().count(), 0);
    assert_eq!(flat.named_graphs().count(), 0);
}
