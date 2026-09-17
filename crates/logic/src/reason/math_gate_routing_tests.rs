// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::BTreeSet;

use purrdf::{RdfDatasetBuilder, RdfLiteral, RdfTerm};

use super::{DIMENSION_CELL_PREDICATES, MATH_GATE_WORLD, promote_to_single_world};

#[test]
fn dimension_gate_routes_native_exponent_assertion_without_unrelated_metadata() {
    let mut builder = RdfDatasetBuilder::new();
    let cell = builder.intern_iri("urn:dimension:cell");
    let relation = builder.intern_iri("urn:dimension:asserted-relation");
    let statement = builder.intern_triple(cell, relation, cell);
    let graph = builder.intern_iri("urn:dimension:source-world");
    let numerator = builder.intern_iri(DIMENSION_CELL_PREDICATES[2]);
    let exponent = builder.intern_literal(RdfLiteral::typed(
        "-2",
        "http://www.w3.org/2001/XMLSchema#integer",
    ));
    let according_to = builder.intern_iri("https://blackcatinformatics.ca/gmeow/accordingTo");
    builder.push_reifier_in_graph(cell, statement, Some(graph));
    builder.push_annotation_in_graph(cell, numerator, exponent, Some(graph));
    builder.push_annotation_in_graph(cell, according_to, graph, Some(graph));
    let source = builder.freeze().unwrap();
    assert_eq!(source.quad_count(), 0);
    let selected = BTreeSet::from([DIMENSION_CELL_PREDICATES[2].to_owned()]);
    let output = promote_to_single_world(&source, &[], &selected).unwrap();
    let rows = output.owned_quads().collect::<Vec<_>>();
    assert_eq!(
        rows.len(),
        1,
        "only the law-read exponent enters the scratch world"
    );
    assert_eq!(rows[0].subject, RdfTerm::iri("urn:dimension:cell"));
    assert_eq!(rows[0].predicate, DIMENSION_CELL_PREDICATES[2]);
    assert_eq!(
        rows[0].object,
        RdfTerm::literal(RdfLiteral::typed(
            "-2",
            "http://www.w3.org/2001/XMLSchema#integer"
        ))
    );
    assert_eq!(rows[0].graph_name, Some(RdfTerm::iri(MATH_GATE_WORLD)));
    assert_eq!(
        source.annotations().count(),
        2,
        "source evidence remains intact"
    );
    assert_eq!(
        source.owned_named_graphs().collect::<Vec<_>>(),
        [RdfTerm::iri("urn:dimension:source-world")]
    );
}
