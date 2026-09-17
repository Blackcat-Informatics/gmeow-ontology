// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::BTreeSet;

use purrdf::{RdfDatasetBuilder, RdfTerm};

use super::{ENACTMENT_GATE_WORLD, RDF_TYPE, promote_to_single_world};

#[test]
fn enactment_gate_routes_native_lease_class_without_widening_the_law_domain() {
    let lease_class = "https://blackcatinformatics.ca/logic/ResourceLease";
    let mut builder = RdfDatasetBuilder::new();
    let lease = builder.intern_iri("urn:enactment:lease");
    let other = builder.intern_iri("urn:enactment:unrelated-record");
    let relation = builder.intern_iri("urn:enactment:asserted-relation");
    let statement = builder.intern_triple(lease, relation, lease);
    let graph = builder.intern_iri("urn:enactment:source-world");
    let rdf_type = builder.intern_iri(RDF_TYPE);
    let class = builder.intern_iri(lease_class);
    let other_class = builder.intern_iri("urn:enactment:unrelated-class");
    let according_to = builder.intern_iri("https://blackcatinformatics.ca/gmeow/accordingTo");
    builder.push_reifier_in_graph(lease, statement, Some(graph));
    builder.push_reifier_in_graph(other, statement, Some(graph));
    builder.push_annotation_in_graph(lease, rdf_type, class, Some(graph));
    builder.push_annotation_in_graph(other, rdf_type, other_class, Some(graph));
    builder.push_annotation_in_graph(lease, according_to, graph, Some(graph));
    let source = builder.freeze().unwrap();
    assert_eq!(source.quad_count(), 0);
    let output = promote_to_single_world(
        &source,
        &[],
        &BTreeSet::new(),
        &BTreeSet::from([lease_class.to_owned()]),
    )
    .unwrap();
    let rows = output.owned_quads().collect::<Vec<_>>();
    assert_eq!(
        rows.len(),
        1,
        "only the law-selected lease class is admitted"
    );
    assert_eq!(rows[0].subject, RdfTerm::iri("urn:enactment:lease"));
    assert_eq!(rows[0].predicate, RDF_TYPE);
    assert_eq!(rows[0].object, RdfTerm::iri(lease_class));
    assert_eq!(rows[0].graph_name, Some(RdfTerm::iri(ENACTMENT_GATE_WORLD)));
    let wide = promote_to_single_world(
        &source,
        &[],
        &BTreeSet::from([RDF_TYPE.to_owned()]),
        &BTreeSet::new(),
    )
    .unwrap();
    assert_eq!(
        wide.quad_count(),
        2,
        "a variable type body still selects every asserted class"
    );
    assert_eq!(
        source.annotations().count(),
        3,
        "source evidence remains intact"
    );
    assert_eq!(
        source.owned_named_graphs().collect::<Vec<_>>(),
        [RdfTerm::iri("urn:enactment:source-world")]
    );
}
