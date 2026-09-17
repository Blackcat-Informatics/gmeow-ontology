// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Additive reader views of canonical RDF axioms.
//!
//! This structural projection uses the grounding correspondence vocabulary in
//! `gmeow_ns`. It preserves the canonical RDF surface and graph scopes; it does
//! not discharge general lens laws or project arbitrary scoped formulas.

/// Add the generated OWL/RDFS reader view of canonical `logic:` assertions while
/// retaining every original assertion and its native table. This is the carrier-side
/// projection boundary used by consumers and OWL-oriented audit readers; it never
/// becomes another authored source.
pub fn with_owl_rdfs_projection(
    dataset: &purrdf::RdfDataset,
) -> std::sync::Arc<purrdf::RdfDataset> {
    project(dataset, GraphProjection::Preserve)
}

/// Materialize the same axiom reader view directly into the SHACL data graph.
/// This selected view flattens named graphs and exposes reifiers and annotations
/// as ordinary triples, matching PurRDF's SHACL projection contract. The source
/// dataset remains the owner of graph context and statement metadata.
#[must_use]
pub fn shacl_reader_view(dataset: &purrdf::RdfDataset) -> std::sync::Arc<purrdf::RdfDataset> {
    project(dataset, GraphProjection::Shacl)
}

#[derive(Clone, Copy)]
enum GraphProjection {
    Preserve,
    Shacl,
}

impl GraphProjection {
    fn graph(self, graph: Option<purrdf::TermId>) -> Option<purrdf::TermId> {
        match self {
            Self::Preserve => graph,
            Self::Shacl => None,
        }
    }
}

#[derive(Clone, Copy)]
enum AssertionTable {
    Ordinary,
    Annotation,
}

fn project(
    dataset: &purrdf::RdfDataset,
    graphs: GraphProjection,
) -> std::sync::Arc<purrdf::RdfDataset> {
    let mut builder = purrdf::RdfDatasetBuilder::new();
    let mut remap = vec![None; dataset.term_count()];
    for quad in dataset.quads() {
        project_assertion(
            dataset,
            &mut builder,
            &mut remap,
            graphs,
            quad,
            AssertionTable::Ordinary,
        );
    }
    for (reifier, triple, graph) in dataset.reifiers_with_graph() {
        let reifier = intern_preserving_term(&mut builder, dataset, &mut remap, reifier);
        let triple = intern_preserving_term(&mut builder, dataset, &mut remap, triple);
        let graph = graphs
            .graph(graph)
            .map(|term| intern_preserving_term(&mut builder, dataset, &mut remap, term));
        match graphs {
            GraphProjection::Preserve => builder.push_reifier_in_graph(reifier, triple, graph),
            GraphProjection::Shacl => {
                let predicate =
                    builder.intern_iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies");
                builder.push_quad(reifier, predicate, triple, None);
            }
        }
    }
    for (reifier, predicate, object, graph) in dataset.annotations_with_graph() {
        project_assertion(
            dataset,
            &mut builder,
            &mut remap,
            graphs,
            purrdf::QuadIds {
                s: reifier,
                p: predicate,
                o: object,
                g: graph,
            },
            AssertionTable::Annotation,
        );
    }
    for graph in dataset
        .named_graphs()
        .filter(|_| matches!(graphs, GraphProjection::Preserve))
    {
        let graph = intern_preserving_term(&mut builder, dataset, &mut remap, graph);
        builder.declare_named_graph(graph);
    }
    // The source-to-builder id map is build-only scratch. Release it before freeze
    // materializes the immutable dataset so the two full-width tables do not overlap
    // at the projection boundary's peak-live allocation point.
    drop(remap);
    builder
        .freeze()
        .expect("OWL/RDFS projection of a valid dataset must freeze")
}

/// Project one asserted operator/marker occurrence. Quoted triple payloads are
/// re-interned unchanged; only the assertion's own predicate/object roles are read.
fn project_assertion(
    dataset: &purrdf::RdfDataset,
    builder: &mut purrdf::RdfDatasetBuilder,
    remap: &mut [Option<purrdf::TermId>],
    graphs: GraphProjection,
    quad: purrdf::QuadIds,
    table: AssertionTable,
) {
    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    let source_predicate = match dataset.resolve(quad.p) {
        purrdf::TermRef::Iri(iri) => iri,
        other => unreachable!("RDF predicate must be an IRI, got {other:?}"),
    };
    let predicate = gmeow_ns::owl_view_of_predicate(source_predicate);
    let object = match dataset.resolve(quad.o) {
        purrdf::TermRef::Iri(iri) if source_predicate == RDF_TYPE => {
            gmeow_ns::owl_view_of_type_marker(iri)
        }
        purrdf::TermRef::Iri(iri) if gmeow_ns::is_class_position_predicate(source_predicate) => {
            match iri {
                gmeow_ns::LOGIC_THING => Some(gmeow_ns::OWL_THING),
                gmeow_ns::LOGIC_NOTHING => Some(gmeow_ns::OWL_NOTHING),
                _ => None,
            }
        }
        _ => None,
    };

    let s = intern_preserving_term(builder, dataset, remap, quad.s);
    let p = intern_preserving_term(builder, dataset, remap, quad.p);
    let o = intern_preserving_term(builder, dataset, remap, quad.o);
    let g = graphs
        .graph(quad.g)
        .map(|term| intern_preserving_term(builder, dataset, remap, term));
    let projected_p = predicate.map(|iri| builder.intern_iri(iri));
    let projected_o = object.map(|iri| builder.intern_iri(iri));
    let push = |builder: &mut purrdf::RdfDatasetBuilder, predicate, object| match (graphs, table) {
        (GraphProjection::Preserve, AssertionTable::Annotation) => {
            builder.push_annotation_in_graph(s, predicate, object, g)
        }
        (GraphProjection::Preserve, AssertionTable::Ordinary)
        | (GraphProjection::Shacl, AssertionTable::Ordinary | AssertionTable::Annotation) => {
            builder.push_quad(s, predicate, object, g)
        }
    };
    match (projected_p, projected_o) {
        (Some(projected_p), Some(projected_o)) => {
            push(builder, projected_p, projected_o);
            push(builder, projected_p, o);
            push(builder, p, projected_o);
        }
        (Some(projected_p), None) => push(builder, projected_p, o),
        (None, Some(projected_o)) => push(builder, p, projected_o),
        (None, None) => {}
    }
    push(builder, p, o);
}

/// Re-intern one source term without allocating an owned quad around it. Blank scopes
/// are preserved because this is a one-source projection, not a standardize-apart union.
fn intern_preserving_term(
    builder: &mut purrdf::RdfDatasetBuilder,
    source: &purrdf::RdfDataset,
    remap: &mut [Option<purrdf::TermId>],
    source_id: purrdf::TermId,
) -> purrdf::TermId {
    if let Some(mapped) = remap[source_id.index()] {
        return mapped;
    }

    let mapped = match source.resolve(source_id) {
        purrdf::TermRef::Iri(iri) => builder.intern_iri(iri),
        purrdf::TermRef::Blank { label, scope } => builder.intern_blank(label, scope),
        purrdf::TermRef::Literal {
            lexical,
            datatype,
            language,
            direction,
        } => {
            let datatype = match source.resolve(datatype) {
                purrdf::TermRef::Iri(iri) => iri.to_owned(),
                other => unreachable!("literal datatype must be an IRI, got {other:?}"),
            };
            builder.intern_literal(purrdf::RdfLiteral {
                lexical_form: lexical.to_owned(),
                datatype: Some(datatype),
                language: language.map(str::to_owned),
                direction,
            })
        }
        purrdf::TermRef::Triple { s, p, o } => {
            let s = intern_preserving_term(builder, source, remap, s);
            let p = intern_preserving_term(builder, source, remap, p);
            let o = intern_preserving_term(builder, source, remap, o);
            builder.intern_triple(s, p, o)
        }
    };
    remap[source_id.index()] = Some(mapped);
    mapped
}

#[path = "reader_view.tests.rs"]
#[cfg(test)]
mod tests;
