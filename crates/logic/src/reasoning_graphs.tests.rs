// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::physical::{DomainProfile, LogicalGraph};

#[test]
fn meta_record_exclusion_retains_declared_empty_worlds() {
    let mut builder = RdfDatasetBuilder::new();
    let empty = builder.intern_iri(GRAPH_STATEMENTS).unwrap();
    builder.declare_named_graph(empty).unwrap();
    let blank = builder
        .intern_owned_term(&RdfTerm::blank_node("empty-world"))
        .unwrap();
    builder.declare_named_graph(blank).unwrap();
    builder
        .push_owned_quad(
            &RdfQuad::new(
                RdfTerm::iri("urn:correspondence"),
                LOGIC_SOURCE_ENDPOINT_IRI,
                RdfTerm::iri("urn:source"),
            )
            .in_graph(RdfTerm::iri(GRAPH_IMPORTS)),
        )
        .unwrap();
    builder
        .push_owned_quad(
            &RdfQuad::new(
                RdfTerm::iri("urn:correspondence"),
                "https://blackcatinformatics.ca/logic/recoveryCase",
                RdfTerm::iri("urn:recovery"),
            )
            .in_graph(RdfTerm::iri(GRAPH_LOGIC)),
        )
        .unwrap();
    let source = builder.freeze().expect("small GMEOW envelope");
    let graphs = source.owned_named_graphs().collect::<HashSet<_>>();
    for filtered in [
        exclude_grounding_correspondences(&source).expect("exclude correspondence"),
        without_recovery_case_envelopes(&source).expect("exclude recovery"),
    ] {
        assert!(filtered.quad_count() < source.quad_count());
        assert_eq!(
            filtered.owned_named_graphs().collect::<HashSet<_>>(),
            graphs
        );
    }
}

#[test]
fn snapshot_projection_selects_declarations_without_erasing_provenance_values() {
    let mut builder = RdfDatasetBuilder::new();
    let empty = builder.intern_iri(GRAPH_STATEMENTS).unwrap();
    builder.declare_named_graph(empty).unwrap();
    let former = "urn:former-source-graph";
    let former_graph = builder.intern_iri(former).unwrap();
    builder.declare_named_graph(former_graph).unwrap();
    let subject = RdfTerm::iri("urn:claim");
    let provenance = "http://www.w3.org/ns/prov#wasDerivedFrom";
    builder
        .push_owned_quad(
            &RdfQuad::new(subject.clone(), provenance, RdfTerm::iri(former))
                .in_graph(RdfTerm::iri(GRAPH_IMPORTS)),
        )
        .unwrap();
    let source = builder.freeze().expect("small source envelope");
    let projected = project_object_level_edb(&source).expect("object-level projection");
    assert_eq!(
        projected.owned_named_graphs().collect::<HashSet<_>>(),
        [RdfTerm::iri(GRAPH_STATEMENTS), RdfTerm::iri(GRAPH_IMPORTS)]
            .into_iter()
            .collect()
    );
    assert!(projected.owned_quads().any(|quad| {
        quad.subject == subject
            && quad.predicate == provenance
            && quad.object == RdfTerm::iri(former)
            && quad.graph_name == Some(RdfTerm::iri(GRAPH_IMPORTS))
    }));
    assert!(
        source
            .owned_named_graphs()
            .any(|graph| graph == RdfTerm::iri(former))
    );
}

#[test]
fn boundary_is_unique_and_excludes_meta_graphs() {
    let unique = OBJECT_LEVEL_NAMED_GRAPHS
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(unique.len(), OBJECT_LEVEL_NAMED_GRAPHS.len());
    assert!(!is_object_level_named_graph(
        "https://blackcatinformatics.ca/gmeow/graph/correspondence"
    ));
    assert!(!is_object_level_named_graph(
        "https://blackcatinformatics.ca/gmeow/graph/correspondence-laws"
    ));
    // The grounding seam registry asserts governance/policy data (which
    // cross-grounding reference channels are sanctioned), not object-level
    // axioms — excluded exactly like the correspondence-laws graph.
    assert!(!is_object_level_named_graph(
        "https://blackcatinformatics.ca/gmeow/graph/grounding-seams"
    ));

    let selected = object_level_domains().expect("the complete declared domain roles are admitted");
    let authority = "gmeow.pipeline.object-level.v1";
    let roles: Vec<_> = std::iter::once(LogicalGraph::Default)
        .chain(
            OBJECT_LEVEL_NAMED_GRAPHS
                .into_iter()
                .map(|graph| LogicalGraph::Named(purrdf::TermValue::iri(graph))),
        )
        .collect();
    let bytes = object_level_selection_bytes(authority, &roles)
        .expect("the actual encoder retains the complete ordered role envelope");
    let mut reference = include_str!("reasoning_graphs/fixtures/domain-selection-v1.txt").lines();
    let preimage = reference
        .next()
        .expect("the original reference contains its complete preimage")
        .strip_prefix("OBJECT_LEVEL_DOMAIN_PREIMAGE=")
        .expect("the original preimage is labeled");
    let digest = reference
        .next()
        .expect("the original reference contains its actual production selection")
        .strip_prefix("OBJECT_LEVEL_DOMAIN_SELECTION=")
        .expect("the original production selection is labeled");
    assert_eq!(reference.next(), None);
    assert_eq!(bytes, preimage.as_bytes());
    assert_eq!(selected.worlds().len(), roles.len());
    for world in selected.worlds() {
        assert_eq!(
            purrdf_hash::hex::Digest32::new(*world.selection()).to_hex(),
            digest
        );
        assert_eq!(world.authority(), authority);
        assert_eq!(world.profile(), DomainProfile::NonemptyObjectDomainV1);
    }
}

#[test]
fn endpoint_predicates_identify_a_correspondence_without_its_type_triple() {
    // The build-time object-level-EDB twin sees a narrowed union in which the compile
    // stage has already projected the `rdf:type logic:GroundingCorrespondence` triple into
    // the meta `graph/correspondence-laws` graph, leaving only the raw endpoint triples in
    // an admitted graph. Keying on the endpoint predicates still identifies the record, so
    // its out-of-fragment `owl:InverseFunctionalProperty` referent never reaches the EDB
    // (the reason-verify regression these leaks caused).
    let ttl = concat!(
        "@prefix logic: <https://blackcatinformatics.ca/logic/> .\n",
        "logic:corrIFP\n",
        "  logic:sourceEndpoint logic:inverseFunctionalProperty ;\n",
        "  logic:targetEndpoint <http://www.w3.org/2002/07/owl#InverseFunctionalProperty> .\n",
        "logic:Person a logic:Class .\n",
    );
    let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse ttl");

    let subjects = grounding_correspondence_subjects(&ds);
    assert!(
        subjects.contains("https://blackcatinformatics.ca/logic/corrIFP"),
        "an endpoint-only correspondence record must be detected: {subjects:?}"
    );

    let excluded = exclude_grounding_correspondences(&ds).expect("exclude correspondences");
    let leaks_ifp = excluded.owned_quads().any(|q| {
        matches!(&q.object, RdfTerm::Iri(o)
                if o == "http://www.w3.org/2002/07/owl#InverseFunctionalProperty")
    });
    assert!(
        !leaks_ifp,
        "the inverse-functional endpoint referent must be excluded from the object-level EDB"
    );
    let keeps_person = excluded.owned_quads().any(|q| {
        matches!(&q.subject, RdfTerm::Iri(s)
                if s == "https://blackcatinformatics.ca/logic/Person")
    });
    assert!(
        keeps_person,
        "ordinary object-level axioms (logic:Person a logic:Class) must be preserved"
    );
}
