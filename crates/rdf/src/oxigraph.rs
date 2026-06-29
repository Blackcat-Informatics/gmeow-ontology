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

/// Flatten a frozen [`RdfDataset`] back into the source-faithful **flat** oxigraph
/// quad stream — base quads PLUS the RDF 1.2 statement layer re-materialized as
/// `<reifier> rdf:reifies <<( s p o )>>` rows and annotation rows.
///
/// This is the text-free replacement (#909) for re-parsing N-Quads/Turtle text into a
/// flat oxigraph quad list: a consumer that wants the un-folded quad stream
/// (`gts_compose::SnapshotBuilder`, the `py_store`/`py_gts` producer surfaces) parses
/// once via the native [`parse_dataset`](crate::parse_dataset) into the IR and then
/// flattens it here. The IR fold + this un-fold are exact inverses (set-equal to the
/// original parse), so the GTS producer's content-id stays byte-stable.
pub fn flat_oxigraph_quads_from_dataset(dataset: &RdfDataset) -> Result<Vec<Quad>, RdfDiagnostic> {
    let mut quads = Vec::new();
    for quad in dataset.owned_quads() {
        quads.push(oxigraph_quad_from_rdf(
            &quad,
            GraphPolicy::PreserveNamedGraphs,
        )?);
    }
    let rdf_reifies = NamedNode::new(RDF_REIFIES)
        .map_err(|e| RdfDiagnostic::error("oxigraph-rdf-reifies-iri", e.to_string()))?;
    for reifier in dataset.owned_reifiers() {
        quads.push(oxigraph_reifier_quad(&reifier, &rdf_reifies)?);
    }
    for annotation in dataset.owned_annotations() {
        quads.push(oxigraph_annotation_quad(&annotation)?);
    }
    Ok(quads)
}

/// Like [`flat_oxigraph_quads_from_dataset`], but every blank node label is prefixed
/// with a deterministic, collision-resistant scope derived from `scope_key`.
///
/// The native text codecs mint anonymous blank labels (`gts_<counter>`) that restart
/// at 0 on every parse, so two *different* source documents independently produce the
/// same labels. When several documents are accumulated into one store (the build's
/// per-file slice load), those distinct blanks would silently merge. Scoping by the
/// SOURCE identity keeps them disjoint — and because the prefix is a pure function of
/// `scope_key`, every stage that re-parses the same source derives the SAME labels, so
/// cross-stage blank references (reifiers, mapping atoms) stay consistent.
pub fn flat_oxigraph_quads_from_dataset_scoped(
    dataset: &RdfDataset,
    scope_key: &str,
) -> Result<Vec<Quad>, RdfDiagnostic> {
    let prefix = blank_scope_prefix(scope_key);
    Ok(flat_oxigraph_quads_from_dataset(dataset)?
        .iter()
        .map(|quad| rescope_quad_blanks(quad, &prefix))
        .collect())
}

/// A stable (FNV-1a) blank-node label prefix for a source document. Deterministic
/// across processes and stages — the same `scope_key` always yields the same prefix.
fn blank_scope_prefix(scope_key: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in scope_key.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("g{hash:016x}")
}

fn rescope_blank(node: &BlankNode, prefix: &str) -> BlankNode {
    BlankNode::new_unchecked(format!("{prefix}{}", node.as_str()))
}

fn rescope_subject(subject: &NamedOrBlankNode, prefix: &str) -> NamedOrBlankNode {
    match subject {
        NamedOrBlankNode::BlankNode(b) => rescope_blank(b, prefix).into(),
        NamedOrBlankNode::NamedNode(n) => n.clone().into(),
    }
}

fn rescope_term(term: &Term, prefix: &str) -> Term {
    match term {
        Term::BlankNode(b) => rescope_blank(b, prefix).into(),
        Term::Triple(triple) => Term::Triple(Box::new(rescope_triple(triple, prefix))),
        other => other.clone(),
    }
}

fn rescope_triple(triple: &Triple, prefix: &str) -> Triple {
    Triple::new(
        rescope_subject(&triple.subject, prefix),
        triple.predicate.clone(),
        rescope_term(&triple.object, prefix),
    )
}

fn rescope_graph(graph: &GraphName, prefix: &str) -> GraphName {
    match graph {
        GraphName::BlankNode(b) => rescope_blank(b, prefix).into(),
        other => other.clone(),
    }
}

fn rescope_quad_blanks(quad: &Quad, prefix: &str) -> Quad {
    Quad::new(
        rescope_subject(&quad.subject, prefix),
        quad.predicate.clone(),
        rescope_term(&quad.object, prefix),
        rescope_graph(&quad.graph_name, prefix),
    )
}

/// Materialize an in-memory oxigraph [`Store`] back into the frozen [`RdfDataset`]
/// IR, text-free.
///
/// This is the reverse of [`store_from_dataset`]: it iterates the store's
/// `oxigraph::model` quads (NOT `oxigraph::io`) and folds them through the SAME
/// `dataset_from_oxigraph_quads` path used by the parser ingress, so the RDF 1.2
/// statement layer (`rdf:reifies` reifiers + annotations) is reconstructed
/// identically whether the quads came from a text parse or a SPARQL/SHACL store.
///
/// # Errors
///
/// Returns an [`RdfDiagnostic`] if the store cannot be iterated or the folded
/// quads fail dataset validation.
pub fn dataset_from_store(store: &Store) -> Result<std::sync::Arc<RdfDataset>, RdfDiagnostic> {
    let quads: Vec<Quad> = store
        .iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| RdfDiagnostic::error("oxigraph-store-iter", e.to_string()))?;
    crate::dataset_from_oxigraph_quads(&quads)
        .map_err(|e| RdfDiagnostic::error("oxigraph-store-fold", e))
}

/// Convert an oxigraph [`Quad`] into the gmeow-rdf model.
///
/// Public so an oxigraph quad source can convert quads
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

    #[test]
    fn dataset_from_store_round_trips_via_native_codecs() {
        // text -> dataset -> store_from_dataset -> dataset_from_store must be
        // isomorphic to the original dataset.
        let nt = concat!(
            "<https://e/s> <https://e/p> <https://e/o> .\n",
            "<https://e/s> <https://e/p2> \"lit\"@en .\n",
            "<https://e/r> <http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies> ",
            "<<( <https://e/s> <https://e/p> <https://e/o> )>> .\n",
            "<https://e/r> <https://e/confidence> \"0.9\" .\n",
        );
        let original = crate::parse_dataset(nt.as_bytes(), "application/n-triples", None)
            .expect("parse native");
        let store = store_from_dataset(original.as_ref(), GraphPolicy::PreserveNamedGraphs)
            .expect("store from dataset");
        let round_tripped = dataset_from_store(&store).expect("dataset from store");
        assert!(
            crate::datasets_isomorphic(original.as_ref(), round_tripped.as_ref()),
            "store -> dataset round-trip must be isomorphic to the parsed dataset"
        );
    }
}
