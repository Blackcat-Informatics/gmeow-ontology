// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use ::oxigraph::model::{
    BaseDirection, BlankNode, GraphName, GraphNameRef, Literal, NamedNode, NamedOrBlankNode, Quad,
    Term, Triple,
};
use ::oxigraph::store::Store;

use crate::{
    RdfAnnotation, RdfDataset, RdfDiagnostic, RdfLiteral, RdfLocation, RdfQuad, RdfReifier,
    RdfTerm, RdfTextDirection, RdfTriple,
};

pub mod backend;

const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";

/// Named-graph policy when materializing a generic RDF store into oxigraph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum GraphPolicy {
    PreserveNamedGraphs,
    FlattenToDefaultGraph,
}

/// Materialize a frozen [`RdfDataset`] into an in-memory oxigraph store, reading the
/// IR directly.
pub fn store_from_dataset(
    dataset: &RdfDataset,
    graph_policy: GraphPolicy,
) -> Result<Store, RdfDiagnostic> {
    let store =
        Store::new().map_err(|e| RdfDiagnostic::error("oxigraph-store-create", e.to_string()))?;
    for quad in dataset.owned_quads() {
        let ox_quad = oxigraph_quad_from_rdf(&quad, graph_policy)?;
        store
            .insert(&ox_quad)
            .map_err(|e| RdfDiagnostic::error("oxigraph-store-insert", e.to_string()))?;
    }
    let rdf_reifies = NamedNode::new(RDF_REIFIES)
        .map_err(|e| RdfDiagnostic::error("oxigraph-rdf-reifies-iri", e.to_string()))?;
    for reifier in dataset.owned_reifiers() {
        let ox_quad = oxigraph_reifier_quad(&reifier, &rdf_reifies)?;
        store
            .insert(&ox_quad)
            .map_err(|e| RdfDiagnostic::error("oxigraph-store-insert", e.to_string()))?;
    }
    for annotation in dataset.owned_annotations() {
        let ox_quad = oxigraph_annotation_quad(&annotation)?;
        store
            .insert(&ox_quad)
            .map_err(|e| RdfDiagnostic::error("oxigraph-store-insert", e.to_string()))?;
    }
    Ok(store)
}

/// Convert an oxigraph [`Quad`] into the gmeow-rdf model.
///
/// Public so streaming parsers (`oxigraph::io::RdfParser`) can convert quads
/// without an intermediate `Store` — the `Store` canonicalizes typed-literal
/// lexical forms (e.g. `+00:00` → `Z`, `0.70` → `0.7`), which a faithful codec
/// must preserve.
pub fn rdf_quad_from_oxigraph(quad: &Quad) -> RdfQuad {
    let subject = match &quad.subject {
        NamedOrBlankNode::NamedNode(node) => RdfTerm::iri(node.as_str()),
        NamedOrBlankNode::BlankNode(node) => RdfTerm::blank_node(node.as_str()),
    };
    let object = rdf_term_from_oxigraph(&quad.object);
    let mut rdf_quad = RdfQuad::new(subject, quad.predicate.as_str(), object);
    rdf_quad.graph_name = match &quad.graph_name {
        GraphName::NamedNode(node) => Some(RdfTerm::iri(node.as_str())),
        GraphName::BlankNode(node) => Some(RdfTerm::blank_node(node.as_str())),
        GraphName::DefaultGraph => None,
    };
    rdf_quad
}

fn rdf_term_from_oxigraph(term: &Term) -> RdfTerm {
    match term {
        Term::NamedNode(node) => RdfTerm::iri(node.as_str()),
        Term::BlankNode(node) => RdfTerm::blank_node(node.as_str()),
        Term::Literal(literal) => RdfTerm::literal(RdfLiteral {
            lexical_form: literal.value().to_owned(),
            datatype: Some(literal.datatype().as_str().to_owned()),
            language: literal.language().map(str::to_owned),
            direction: literal.direction().map(|direction| match direction {
                BaseDirection::Ltr => RdfTextDirection::Ltr,
                BaseDirection::Rtl => RdfTextDirection::Rtl,
            }),
        }),
        Term::Triple(triple) => RdfTerm::triple(rdf_triple_from_oxigraph(triple)),
    }
}

fn rdf_triple_from_oxigraph(triple: &Triple) -> RdfTriple {
    let subject = match &triple.subject {
        NamedOrBlankNode::NamedNode(node) => RdfTerm::iri(node.as_str()),
        NamedOrBlankNode::BlankNode(node) => RdfTerm::blank_node(node.as_str()),
    };
    RdfTriple::new(
        subject,
        triple.predicate.as_str(),
        rdf_term_from_oxigraph(&triple.object),
    )
}

fn oxigraph_quad_from_rdf(
    quad: &RdfQuad,
    graph_policy: GraphPolicy,
) -> Result<Quad, RdfDiagnostic> {
    let graph_name = match graph_policy {
        GraphPolicy::FlattenToDefaultGraph => GraphNameRef::DefaultGraph.into_owned(),
        GraphPolicy::PreserveNamedGraphs => match &quad.graph_name {
            Some(graph_name) => graph_name_from_rdf(graph_name, quad.location.clone())?,
            None => GraphName::DefaultGraph,
        },
    };
    Ok(Quad::new(
        subject_from_rdf(&quad.subject, quad.location.clone())?,
        named_node_from_iri(&quad.predicate, quad.location.clone())?,
        term_from_rdf(&quad.object, quad.location.clone())?,
        graph_name,
    ))
}

fn oxigraph_reifier_quad(
    reifier: &RdfReifier,
    rdf_reifies: &NamedNode,
) -> Result<Quad, RdfDiagnostic> {
    Ok(Quad::new(
        subject_from_rdf(&reifier.reifier, reifier.location.clone())?,
        rdf_reifies.clone(),
        Term::Triple(Box::new(triple_from_rdf(
            &reifier.statement,
            reifier.location.clone(),
        )?)),
        GraphName::DefaultGraph,
    ))
}

fn oxigraph_annotation_quad(annotation: &RdfAnnotation) -> Result<Quad, RdfDiagnostic> {
    Ok(Quad::new(
        subject_from_rdf(&annotation.reifier, annotation.location.clone())?,
        named_node_from_iri(&annotation.predicate, annotation.location.clone())?,
        term_from_rdf(&annotation.object, annotation.location.clone())?,
        GraphName::DefaultGraph,
    ))
}

fn term_from_rdf(term: &RdfTerm, location: Option<RdfLocation>) -> Result<Term, RdfDiagnostic> {
    match term {
        RdfTerm::Iri(iri) => Ok(Term::NamedNode(named_node_from_iri(iri, location)?)),
        RdfTerm::BlankNode(id) => Ok(Term::BlankNode(blank_node_from_id(id, location)?)),
        RdfTerm::Literal(literal) => Ok(Term::Literal(literal_from_rdf(literal, location)?)),
        RdfTerm::Triple(triple) => Ok(Term::Triple(Box::new(triple_from_rdf(triple, location)?))),
    }
}

fn subject_from_rdf(
    term: &RdfTerm,
    location: Option<RdfLocation>,
) -> Result<NamedOrBlankNode, RdfDiagnostic> {
    match term {
        RdfTerm::Iri(iri) => Ok(NamedOrBlankNode::NamedNode(named_node_from_iri(
            iri, location,
        )?)),
        RdfTerm::BlankNode(id) => Ok(NamedOrBlankNode::BlankNode(blank_node_from_id(
            id, location,
        )?)),
        other => Err(RdfDiagnostic::error(
            "oxigraph-subject-unsupported",
            format!(
                "oxigraph subjects must be IRIs or blank nodes, got {:?}",
                other.kind()
            ),
        )
        .with_location_opt(location)),
    }
}

fn graph_name_from_rdf(
    term: &RdfTerm,
    location: Option<RdfLocation>,
) -> Result<GraphName, RdfDiagnostic> {
    match term {
        RdfTerm::Iri(iri) => Ok(GraphName::NamedNode(named_node_from_iri(iri, location)?)),
        RdfTerm::BlankNode(id) => Ok(GraphName::BlankNode(blank_node_from_id(id, location)?)),
        other => Err(RdfDiagnostic::error(
            "oxigraph-graph-name-unsupported",
            format!(
                "oxigraph graph names must be IRIs or blank nodes, got {:?}",
                other.kind()
            ),
        )
        .with_location_opt(location)),
    }
}

fn triple_from_rdf(
    triple: &RdfTriple,
    location: Option<RdfLocation>,
) -> Result<Triple, RdfDiagnostic> {
    Ok(Triple::new(
        subject_from_rdf(
            &triple.subject,
            triple.location.clone().or(location.clone()),
        )?,
        named_node_from_iri(
            &triple.predicate,
            triple.location.clone().or(location.clone()),
        )?,
        term_from_rdf(&triple.object, triple.location.clone().or(location))?,
    ))
}

fn literal_from_rdf(
    literal: &RdfLiteral,
    location: Option<RdfLocation>,
) -> Result<Literal, RdfDiagnostic> {
    if let Some(language) = &literal.language {
        return Ok(match literal.direction {
            Some(RdfTextDirection::Ltr) => {
                Literal::new_directional_language_tagged_literal_unchecked(
                    literal.lexical_form.clone(),
                    language.clone(),
                    BaseDirection::Ltr,
                )
            }
            Some(RdfTextDirection::Rtl) => {
                Literal::new_directional_language_tagged_literal_unchecked(
                    literal.lexical_form.clone(),
                    language.clone(),
                    BaseDirection::Rtl,
                )
            }
            None => Literal::new_language_tagged_literal_unchecked(
                literal.lexical_form.clone(),
                language.clone(),
            ),
        });
    }
    if let Some(datatype) = &literal.datatype {
        return Ok(Literal::new_typed_literal(
            literal.lexical_form.clone(),
            named_node_from_iri(datatype, location)?,
        ));
    }
    Ok(Literal::new_simple_literal(literal.lexical_form.clone()))
}

fn named_node_from_iri(
    iri: &str,
    location: Option<RdfLocation>,
) -> Result<NamedNode, RdfDiagnostic> {
    NamedNode::new(iri).map_err(|e| {
        RdfDiagnostic::error("oxigraph-invalid-iri", format!("invalid IRI `{iri}`"))
            .with_detail(e.to_string())
            .with_location_opt(location)
    })
}

fn blank_node_from_id(id: &str, location: Option<RdfLocation>) -> Result<BlankNode, RdfDiagnostic> {
    BlankNode::new(id).map_err(|e| {
        RdfDiagnostic::error(
            "oxigraph-invalid-blank-node",
            format!("invalid blank node id `{id}`"),
        )
        .with_detail(e.to_string())
        .with_location_opt(location)
    })
}

trait WithOptionalLocation {
    fn with_location_opt(self, location: Option<RdfLocation>) -> Self;
}

impl WithOptionalLocation for RdfDiagnostic {
    fn with_location_opt(self, location: Option<RdfLocation>) -> Self {
        match location {
            Some(location) => self.with_location(location),
            None => self,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};

    fn dataset_from_quads(quads: Vec<RdfQuad>) -> std::sync::Arc<RdfDataset> {
        let mut builder = RdfDatasetBuilder::new();
        for quad in quads {
            builder.push_owned_quad(&quad);
        }
        builder.freeze().expect("valid test dataset")
    }

    #[test]
    fn materializes_private_language_tag_without_strict_bcp47_check() {
        let source = dataset_from_quads(vec![RdfQuad::new(
            RdfTerm::iri("https://example.org/s"),
            "https://example.org/p",
            RdfTerm::literal(RdfLiteral::language_tagged("hallo", "x-gmeow-afrikaans")),
        )]);
        let store = store_from_dataset(source.as_ref(), GraphPolicy::FlattenToDefaultGraph)
            .expect("private language tags should materialize");
        assert_eq!(store.len().unwrap(), 1);
    }

    #[test]
    fn rejects_quoted_triple_subject_at_dataset_boundary() {
        let quoted = RdfTerm::triple(RdfTriple::new(
            RdfTerm::iri("https://example.org/a"),
            "https://example.org/p",
            RdfTerm::iri("https://example.org/b"),
        ));
        let mut builder = RdfDatasetBuilder::new();
        builder.push_owned_quad(&RdfQuad::new(
            quoted,
            "https://example.org/p",
            RdfTerm::iri("https://example.org/o"),
        ));
        let err = builder
            .freeze()
            .expect_err("asserted triple subjects are rejected before oxigraph materialization");
        assert_eq!(err.code, "rdf-ir-triple-subject");
    }

    #[test]
    fn store_from_dataset_materializes_reifiers_and_annotations() {
        fn quad_set(store: &Store) -> std::collections::BTreeSet<String> {
            store
                .iter()
                .map(|q| q.expect("store quad").to_string())
                .collect()
        }

        let mut b = RdfDatasetBuilder::new();
        let s = b.intern_iri("https://example.org/s".to_owned());
        let p = b.intern_iri("https://example.org/p".to_owned());
        let o = b.intern_iri("https://example.org/o".to_owned());
        b.push_quad(s, p, o, None);
        // Exercise the RDF 1.2 statement-layer path (reifier + annotation), since
        // those are the rows `store_from_dataset` resolves separately.
        let triple = b.intern_triple(s, p, o);
        let r = b.intern_iri("https://example.org/r".to_owned());
        b.push_reifier(r, triple);
        let conf = b.intern_iri("https://example.org/confidence".to_owned());
        let val = b.intern_literal(RdfLiteral::typed(
            "0.9",
            "http://www.w3.org/2001/XMLSchema#decimal",
        ));
        b.push_annotation(r, conf, val);
        let ds = b.freeze().expect("freeze");
        let dataset: &RdfDataset = &ds;

        let via_dataset =
            store_from_dataset(dataset, GraphPolicy::PreserveNamedGraphs).expect("via dataset");

        let dataset_quads = quad_set(&via_dataset);
        assert_eq!(
            dataset_quads.len(),
            3,
            "base quad, rdf:reifies row, and annotation row must materialize"
        );
        assert!(dataset_quads
            .iter()
            .any(|quad| quad.contains("22-rdf-syntax-ns#reifies")));
        assert!(dataset_quads
            .iter()
            .any(|quad| quad.contains("https://example.org/confidence")));
    }
}
