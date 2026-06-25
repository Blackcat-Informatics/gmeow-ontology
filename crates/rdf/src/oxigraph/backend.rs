// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Oxigraph implementations of the purrdf backend traits (P2d, #887).

use core::ops::ControlFlow;
use std::io::Write;

use ::oxigraph::io::{
    RdfFormat, RdfParser, RdfSerializer as OxRdfSerializer, WriterQuadSerializer,
};
use ::oxigraph::model::{
    BaseDirection, BlankNode, GraphName, NamedNode, NamedOrBlankNode, Quad, Term as OxTerm, Triple,
};
use ::oxigraph::sparql::{QueryResults, SparqlEvaluator};
use ::oxigraph::store::Store;
use gmeow_rdf_events::{
    EventError, EventQuad, EventTerm, EventTermId, EventTriple, RdfEventSink, ScopeId,
    TextDirection,
};

use crate::{
    BlankScope, RdfDataset, RdfDatasetBuilder, RdfDiagnostic, RdfLiteral, RdfParseRequest,
    RdfParserBackend, RdfSerializeRequest, RdfSerializer, RdfTextDirection, SerializeGraph,
    SparqlEngine, SparqlRequest, SparqlResult, TermFactory, TermId, TermValue,
};

use super::{GraphPolicy, RDF_REIFIES};

/// Default Oxigraph-backed parser/SPARQL/serializer adapter.
#[derive(Debug, Clone, Copy, Default)]
pub struct OxigraphBackend;

impl RdfParserBackend for OxigraphBackend {
    fn parse_into<S: RdfEventSink + ?Sized>(
        &self,
        request: RdfParseRequest<'_>,
        sink: &mut S,
    ) -> Result<(), RdfDiagnostic> {
        let format = format_from_media_type(request.media_type)?;
        let mut parser = RdfParser::from_format(format).lenient();
        if let Some(base_iri) = request.base_iri {
            parser = parser
                .with_base_iri(base_iri)
                .map_err(|e| RdfDiagnostic::error("oxigraph-parser-base-iri", e.to_string()))?;
        }

        let mut next_id = 0;
        for quad in parser.for_slice(request.bytes) {
            let quad = quad.map_err(|e| {
                let diagnostic = RdfDiagnostic::error("oxigraph-parse", e.to_string());
                match request.source_name {
                    Some(source) => diagnostic.with_detail(format!("source: {source}")),
                    None => diagnostic,
                }
            })?;
            if !emit_quad_from_oxigraph(sink, &mut next_id, &quad)? {
                return Ok(());
            }
        }
        sink.finish().map_err(event_error)
    }
}

impl SparqlEngine for OxigraphBackend {
    type Dataset = Store;

    fn query(
        &self,
        dataset: &Self::Dataset,
        request: SparqlRequest<'_>,
    ) -> Result<SparqlResult, RdfDiagnostic> {
        let evaluator = sparql_evaluator(request.base_iri)?;
        let results = evaluator
            .parse_query(request.query)
            .map_err(|e| RdfDiagnostic::error("oxigraph-sparql-query-parse", e.to_string()))?
            .on_store(dataset)
            .execute()
            .map_err(|e| RdfDiagnostic::error("oxigraph-sparql-query-eval", e.to_string()))?;
        materialize_results(results)
    }

    fn update(
        &self,
        dataset: &mut Self::Dataset,
        request: SparqlRequest<'_>,
    ) -> Result<(), RdfDiagnostic> {
        let evaluator = sparql_evaluator(request.base_iri)?;
        evaluator
            .parse_update(request.query)
            .map_err(|e| RdfDiagnostic::error("oxigraph-sparql-update-parse", e.to_string()))?
            .on_store(dataset)
            .execute()
            .map_err(|e| RdfDiagnostic::error("oxigraph-sparql-update-eval", e.to_string()))
    }
}

impl RdfSerializer for OxigraphBackend {
    fn serialize<W: Write>(
        &self,
        dataset: &RdfDataset,
        request: RdfSerializeRequest<'_>,
        output: W,
    ) -> Result<(), RdfDiagnostic> {
        let format = format_from_media_type(request.media_type)?;
        let mut serializer = OxRdfSerializer::from_format(format);
        if let Some(base_iri) = request.base_iri {
            serializer = serializer
                .with_base_iri(base_iri)
                .map_err(|e| RdfDiagnostic::error("oxigraph-serializer-base-iri", e.to_string()))?;
        }

        let mut writer = serializer.for_writer(output);
        match request.graph {
            SerializeGraph::Dataset if format.supports_datasets() => {
                serialize_dataset_quads(dataset, &mut writer)?
            }
            SerializeGraph::Dataset | SerializeGraph::DefaultGraph => {
                serialize_default_graph(dataset, &mut writer)?
            }
            SerializeGraph::Named(graph) => {
                let graph = graph_name_from_value(graph)?;
                serialize_named_graph(dataset, &graph, &mut writer)?
            }
        }
        writer.finish().map(|_| ()).map_err(serialize_error)
    }
}

/// Outcome of serializing an [`RdfDataset`] to a concrete RDF format.
#[derive(Debug, Clone)]
pub struct SerializeOutcome {
    /// The serialized document bytes.
    pub bytes: Vec<u8>,
    /// The number of RDF-1.2 statement-layer rows (reifier bindings +
    /// annotation triples) dropped because the target format cannot represent
    /// quoted triples. Zero for star-capable formats.
    pub statement_rows_dropped: usize,
}

/// Whether an [`RdfFormat`] can faithfully represent RDF-1.2 quoted triples
/// (the star layer). N-Quads, N-Triples, Turtle, and TriG are star-capable;
/// JSON-LD, RDF/XML, and N3 are not.
fn is_star_capable(format: RdfFormat) -> bool {
    matches!(
        format,
        RdfFormat::NQuads | RdfFormat::NTriples | RdfFormat::Turtle | RdfFormat::TriG
    )
}

/// Serialize the frozen IR to any oxigraph RDF format, returning the bytes and
/// the count of RDF-1.2 statement-layer rows dropped because the target format
/// cannot represent quoted triples.
///
/// Star-capable formats (Turtle, N-Triples, N-Quads, TriG) emit the full RDF-1.2
/// statement layer and report `statement_rows_dropped = 0`. Star-incapable formats
/// (JSON-LD, RDF/XML, N3) emit only the base quads and report the dropped
/// statement-row count in the outcome — the caller records this as declared loss
/// (#671 projection doctrine).
///
/// Graph selection follows the same rule as the [`RdfSerializer`] trait impl:
/// formats that support datasets (N-Quads, TriG, JSON-LD) emit all named graphs;
/// triple-only formats (Turtle, N-Triples, RDF/XML, N3) flatten to the default graph.
pub fn serialize_dataset_to_format(
    dataset: &RdfDataset,
    format: RdfFormat,
    base_iri: Option<&str>,
) -> Result<SerializeOutcome, RdfDiagnostic> {
    let mut serializer = OxRdfSerializer::from_format(format);
    if let Some(iri) = base_iri {
        serializer = serializer
            .with_base_iri(iri)
            .map_err(|e| RdfDiagnostic::error("oxigraph-serializer-base-iri", e.to_string()))?;
    }

    let mut out: Vec<u8> = Vec::new();
    let mut writer = serializer.for_writer(&mut out);

    // Emit base quads, respecting the graph-selection rule.
    if format.supports_datasets() {
        serialize_dataset_quads_no_star(dataset, &mut writer)?;
    } else {
        serialize_default_graph_no_star(dataset, &mut writer)?;
    }

    // Emit the statement layer only for star-capable formats.
    let statement_rows_dropped = if is_star_capable(format) {
        let mode = if format.supports_datasets() {
            StatementGraphMode::Quads
        } else {
            StatementGraphMode::Triples
        };
        serialize_statement_rows(dataset, &mut writer, mode)?;
        0
    } else {
        dataset.reifiers().count() + dataset.annotations().count()
    };

    writer.finish().map_err(serialize_error)?;

    Ok(SerializeOutcome {
        bytes: out,
        statement_rows_dropped,
    })
}

/// Serialize base quads for ALL graphs (dataset-capable formats), without
/// emitting the statement layer. The caller decides whether to add statement rows.
fn serialize_dataset_quads_no_star<W: Write>(
    dataset: &RdfDataset,
    serializer: &mut WriterQuadSerializer<W>,
) -> Result<(), RdfDiagnostic> {
    for (frozen_index, quad) in dataset.quads().enumerate() {
        let quad = super::oxigraph_quad_from_rdf(
            &dataset.to_owned_quad(frozen_index, quad),
            GraphPolicy::PreserveNamedGraphs,
        )?;
        serialize_quad(serializer, &quad)?;
    }
    Ok(())
}

/// Serialize only the default-graph quads (triple-only formats), without emitting
/// the statement layer. The caller decides whether to add statement rows.
fn serialize_default_graph_no_star<W: Write>(
    dataset: &RdfDataset,
    serializer: &mut WriterQuadSerializer<W>,
) -> Result<(), RdfDiagnostic> {
    for (frozen_index, quad) in dataset.quads().enumerate() {
        let ox_quad = super::oxigraph_quad_from_rdf(
            &dataset.to_owned_quad(frozen_index, quad),
            GraphPolicy::PreserveNamedGraphs,
        )?;
        if matches!(ox_quad.graph_name, GraphName::DefaultGraph) {
            serialize_triple(serializer, &ox_quad)?;
        }
    }
    Ok(())
}

fn format_from_media_type(media_type: &str) -> Result<RdfFormat, RdfDiagnostic> {
    let normalized = media_type
        .split(';')
        .next()
        .unwrap_or(media_type)
        .trim()
        .to_ascii_lowercase();
    match normalized.as_str() {
        "text/turtle" | "application/turtle" | "turtle" | "ttl" => Ok(RdfFormat::Turtle),
        "application/n-triples" | "n-triples" | "ntriples" | "nt" => Ok(RdfFormat::NTriples),
        "application/n-quads" | "n-quads" | "nquads" | "nq" => Ok(RdfFormat::NQuads),
        "application/trig" | "trig" => Ok(RdfFormat::TriG),
        other => Err(RdfDiagnostic::error(
            "oxigraph-unsupported-format",
            format!("unsupported RDF media type or format id `{other}`"),
        )),
    }
}

fn sparql_evaluator(base_iri: Option<&str>) -> Result<SparqlEvaluator, RdfDiagnostic> {
    let evaluator = SparqlEvaluator::new();
    match base_iri {
        Some(base_iri) => evaluator
            .with_base_iri(base_iri)
            .map_err(|e| RdfDiagnostic::error("oxigraph-sparql-base-iri", e.to_string())),
        None => Ok(evaluator),
    }
}

fn materialize_results(results: QueryResults<'_>) -> Result<SparqlResult, RdfDiagnostic> {
    match results {
        QueryResults::Solutions(solutions) => {
            let query_variables = solutions.variables().to_vec();
            let variables = query_variables
                .iter()
                .map(|v| v.as_str().to_owned())
                .collect::<Vec<_>>();
            let mut rows = Vec::new();
            for solution in solutions {
                let solution = solution
                    .map_err(|e| RdfDiagnostic::error("oxigraph-sparql-solution", e.to_string()))?;
                rows.push(
                    query_variables
                        .iter()
                        .map(|v| solution.get(v).map(term_value_from_oxigraph))
                        .collect(),
                );
            }
            Ok(SparqlResult::Solutions { variables, rows })
        }
        QueryResults::Graph(triples) => {
            let mut builder = RdfDatasetBuilder::new();
            for triple in triples {
                let triple = triple
                    .map_err(|e| RdfDiagnostic::error("oxigraph-sparql-graph", e.to_string()))?;
                let s = intern_subject_from_oxigraph(&mut builder, &triple.subject);
                let p = builder.intern_iri_value(triple.predicate.as_str());
                let o = intern_term_from_oxigraph(&mut builder, &triple.object);
                builder.push_quad(s, p, o, None);
            }
            let dataset = builder
                .freeze()
                .map_err(|e| RdfDiagnostic::error("oxigraph-sparql-graph-build", e.to_string()))?;
            Ok(SparqlResult::Graph(dataset))
        }
        QueryResults::Boolean(value) => Ok(SparqlResult::Boolean(value)),
    }
}

fn emit_quad_from_oxigraph<S: RdfEventSink + ?Sized>(
    sink: &mut S,
    next_id: &mut u32,
    quad: &Quad,
) -> Result<bool, RdfDiagnostic> {
    let Some(s) = emit_subject_from_oxigraph(sink, next_id, &quad.subject)? else {
        return Ok(false);
    };
    let Some(p) = emit_named_node_from_oxigraph(sink, next_id, &quad.predicate)? else {
        return Ok(false);
    };
    let Some(o) = emit_term_from_oxigraph(sink, next_id, &quad.object)? else {
        return Ok(false);
    };
    let g = match &quad.graph_name {
        GraphName::DefaultGraph => None,
        GraphName::NamedNode(node) => {
            let Some(id) = emit_named_node_from_oxigraph(sink, next_id, node)? else {
                return Ok(false);
            };
            Some(id)
        }
        GraphName::BlankNode(node) => {
            let Some(id) = emit_blank_node_from_oxigraph(sink, next_id, node)? else {
                return Ok(false);
            };
            Some(id)
        }
    };
    match sink.quad(EventQuad { s, p, o, g }).map_err(event_error)? {
        ControlFlow::Continue(()) => Ok(true),
        ControlFlow::Break(()) => Ok(false),
    }
}

fn emit_subject_from_oxigraph<S: RdfEventSink + ?Sized>(
    sink: &mut S,
    next_id: &mut u32,
    subject: &NamedOrBlankNode,
) -> Result<Option<EventTermId>, RdfDiagnostic> {
    match subject {
        NamedOrBlankNode::NamedNode(node) => emit_named_node_from_oxigraph(sink, next_id, node),
        NamedOrBlankNode::BlankNode(node) => emit_blank_node_from_oxigraph(sink, next_id, node),
    }
}

fn emit_named_node_from_oxigraph<S: RdfEventSink + ?Sized>(
    sink: &mut S,
    next_id: &mut u32,
    node: &NamedNode,
) -> Result<Option<EventTermId>, RdfDiagnostic> {
    emit_event_term(sink, next_id, EventTerm::Iri(node.as_str()))
}

fn emit_blank_node_from_oxigraph<S: RdfEventSink + ?Sized>(
    sink: &mut S,
    next_id: &mut u32,
    node: &BlankNode,
) -> Result<Option<EventTermId>, RdfDiagnostic> {
    emit_event_term(
        sink,
        next_id,
        EventTerm::Blank {
            label: node.as_str(),
            scope: ScopeId::DEFAULT,
        },
    )
}

fn emit_term_from_oxigraph<S: RdfEventSink + ?Sized>(
    sink: &mut S,
    next_id: &mut u32,
    term: &OxTerm,
) -> Result<Option<EventTermId>, RdfDiagnostic> {
    match term {
        OxTerm::NamedNode(node) => emit_named_node_from_oxigraph(sink, next_id, node),
        OxTerm::BlankNode(node) => emit_blank_node_from_oxigraph(sink, next_id, node),
        OxTerm::Literal(literal) => emit_event_term(
            sink,
            next_id,
            EventTerm::Literal {
                lexical: literal.value(),
                datatype: literal.datatype().as_str(),
                language: literal.language(),
                direction: literal.direction().map(text_direction_from_oxigraph),
            },
        ),
        OxTerm::Triple(triple) => {
            let Some(s) = emit_subject_from_oxigraph(sink, next_id, &triple.subject)? else {
                return Ok(None);
            };
            let Some(p) = emit_named_node_from_oxigraph(sink, next_id, &triple.predicate)? else {
                return Ok(None);
            };
            let Some(o) = emit_term_from_oxigraph(sink, next_id, &triple.object)? else {
                return Ok(None);
            };
            emit_event_term(sink, next_id, EventTerm::Triple(EventTriple { s, p, o }))
        }
    }
}

fn emit_event_term<S: RdfEventSink + ?Sized>(
    sink: &mut S,
    next_id: &mut u32,
    term: EventTerm<'_>,
) -> Result<Option<EventTermId>, RdfDiagnostic> {
    let id = EventTermId(*next_id);
    *next_id = next_id.checked_add(1).ok_or_else(|| {
        RdfDiagnostic::error(
            "oxigraph-parser-term-id-overflow",
            "event term id space overflowed",
        )
    })?;
    match sink.term(id, term).map_err(event_error)? {
        ControlFlow::Continue(()) => Ok(Some(id)),
        ControlFlow::Break(()) => Ok(None),
    }
}

fn serialize_dataset_quads<W: Write>(
    dataset: &RdfDataset,
    serializer: &mut WriterQuadSerializer<W>,
) -> Result<(), RdfDiagnostic> {
    for (frozen_index, quad) in dataset.quads().enumerate() {
        let quad = super::oxigraph_quad_from_rdf(
            &dataset.to_owned_quad(frozen_index, quad),
            GraphPolicy::PreserveNamedGraphs,
        )?;
        serialize_quad(serializer, &quad)?;
    }
    serialize_statement_rows(dataset, serializer, StatementGraphMode::Quads)
}

fn serialize_default_graph<W: Write>(
    dataset: &RdfDataset,
    serializer: &mut WriterQuadSerializer<W>,
) -> Result<(), RdfDiagnostic> {
    for (frozen_index, quad) in dataset.quads().enumerate() {
        let quad = super::oxigraph_quad_from_rdf(
            &dataset.to_owned_quad(frozen_index, quad),
            GraphPolicy::PreserveNamedGraphs,
        )?;
        if matches!(quad.graph_name, GraphName::DefaultGraph) {
            serialize_triple(serializer, &quad)?;
        }
    }
    serialize_statement_rows(dataset, serializer, StatementGraphMode::Triples)
}

fn serialize_named_graph<W: Write>(
    dataset: &RdfDataset,
    graph: &GraphName,
    serializer: &mut WriterQuadSerializer<W>,
) -> Result<(), RdfDiagnostic> {
    for (frozen_index, quad) in dataset.quads().enumerate() {
        let quad = super::oxigraph_quad_from_rdf(
            &dataset.to_owned_quad(frozen_index, quad),
            GraphPolicy::PreserveNamedGraphs,
        )?;
        if &quad.graph_name == graph {
            serialize_triple(serializer, &quad)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum StatementGraphMode {
    Quads,
    Triples,
}

fn serialize_statement_rows<W: Write>(
    dataset: &RdfDataset,
    serializer: &mut WriterQuadSerializer<W>,
    mode: StatementGraphMode,
) -> Result<(), RdfDiagnostic> {
    let rdf_reifies = NamedNode::new(RDF_REIFIES)
        .map_err(|e| RdfDiagnostic::error("oxigraph-rdf-reifies-iri", e.to_string()))?;
    for (reifier, triple) in dataset.reifiers() {
        let quad =
            super::oxigraph_reifier_quad(&dataset.to_owned_reifier(reifier, triple), &rdf_reifies)?;
        serialize_statement_quad(serializer, &quad, mode)?;
    }
    for (reifier, predicate, object) in dataset.annotations() {
        let quad = super::oxigraph_annotation_quad(
            &dataset.to_owned_annotation(reifier, predicate, object),
        )?;
        serialize_statement_quad(serializer, &quad, mode)?;
    }
    Ok(())
}

fn serialize_statement_quad<W: Write>(
    serializer: &mut WriterQuadSerializer<W>,
    quad: &Quad,
    mode: StatementGraphMode,
) -> Result<(), RdfDiagnostic> {
    match mode {
        StatementGraphMode::Quads => serialize_quad(serializer, quad),
        StatementGraphMode::Triples => serialize_triple(serializer, quad),
    }
}

fn serialize_quad<W: Write>(
    serializer: &mut WriterQuadSerializer<W>,
    quad: &Quad,
) -> Result<(), RdfDiagnostic> {
    serializer.serialize_quad(quad).map_err(serialize_error)
}

fn serialize_triple<W: Write>(
    serializer: &mut WriterQuadSerializer<W>,
    quad: &Quad,
) -> Result<(), RdfDiagnostic> {
    serializer
        .serialize_triple(quad.as_ref())
        .map_err(serialize_error)
}

fn serialize_error(error: std::io::Error) -> RdfDiagnostic {
    RdfDiagnostic::error("oxigraph-serialize", error.to_string())
}

fn term_value_from_oxigraph(term: &OxTerm) -> TermValue {
    match term {
        OxTerm::NamedNode(node) => TermValue::Iri(node.as_str().to_owned()),
        OxTerm::BlankNode(node) => TermValue::Blank {
            label: node.as_str().to_owned(),
            scope: BlankScope::DEFAULT,
        },
        OxTerm::Literal(literal) => TermValue::Literal {
            lexical_form: literal.value().to_owned(),
            datatype: literal.datatype().as_str().to_owned(),
            language: literal.language().map(str::to_owned),
            direction: literal.direction().map(|direction| match direction {
                BaseDirection::Ltr => RdfTextDirection::Ltr,
                BaseDirection::Rtl => RdfTextDirection::Rtl,
            }),
        },
        OxTerm::Triple(triple) => triple_value_from_oxigraph(triple),
    }
}

fn intern_term_from_oxigraph(builder: &mut RdfDatasetBuilder, term: &OxTerm) -> TermId {
    match term {
        OxTerm::NamedNode(node) => builder.intern_iri_value(node.as_str()),
        OxTerm::BlankNode(node) => builder.intern_blank_value(node.as_str(), BlankScope::DEFAULT),
        OxTerm::Literal(literal) => builder.intern_literal_value(RdfLiteral {
            lexical_form: literal.value().to_owned(),
            datatype: Some(literal.datatype().as_str().to_owned()),
            language: literal.language().map(str::to_owned),
            direction: literal.direction().map(rdf_direction_from_oxigraph),
        }),
        OxTerm::Triple(triple) => intern_triple_from_oxigraph(builder, triple),
    }
}

fn intern_subject_from_oxigraph(
    builder: &mut RdfDatasetBuilder,
    subject: &NamedOrBlankNode,
) -> TermId {
    match subject {
        NamedOrBlankNode::NamedNode(node) => builder.intern_iri_value(node.as_str()),
        NamedOrBlankNode::BlankNode(node) => {
            builder.intern_blank_value(node.as_str(), BlankScope::DEFAULT)
        }
    }
}

fn intern_triple_from_oxigraph(builder: &mut RdfDatasetBuilder, triple: &Triple) -> TermId {
    let s = intern_subject_from_oxigraph(builder, &triple.subject);
    let p = builder.intern_iri_value(triple.predicate.as_str());
    let o = intern_term_from_oxigraph(builder, &triple.object);
    builder.intern_triple_value(s, p, o)
}

fn subject_value_from_oxigraph(subject: &NamedOrBlankNode) -> TermValue {
    match subject {
        NamedOrBlankNode::NamedNode(node) => TermValue::Iri(node.as_str().to_owned()),
        NamedOrBlankNode::BlankNode(node) => TermValue::Blank {
            label: node.as_str().to_owned(),
            scope: BlankScope::DEFAULT,
        },
    }
}

fn triple_value_from_oxigraph(triple: &Triple) -> TermValue {
    TermValue::Triple {
        s: Box::new(subject_value_from_oxigraph(&triple.subject)),
        p: Box::new(TermValue::Iri(triple.predicate.as_str().to_owned())),
        o: Box::new(term_value_from_oxigraph(&triple.object)),
    }
}

fn rdf_direction_from_oxigraph(direction: BaseDirection) -> RdfTextDirection {
    match direction {
        BaseDirection::Ltr => RdfTextDirection::Ltr,
        BaseDirection::Rtl => RdfTextDirection::Rtl,
    }
}

fn text_direction_from_oxigraph(direction: BaseDirection) -> TextDirection {
    match direction {
        BaseDirection::Ltr => TextDirection::Ltr,
        BaseDirection::Rtl => TextDirection::Rtl,
    }
}

fn graph_name_from_value(value: &TermValue) -> Result<GraphName, RdfDiagnostic> {
    match value {
        TermValue::Iri(iri) => NamedNode::new(iri)
            .map(GraphName::NamedNode)
            .map_err(|e| RdfDiagnostic::error("oxigraph-graph-iri", e.to_string())),
        TermValue::Blank { label, .. } => ::oxigraph::model::BlankNode::new(label)
            .map(GraphName::BlankNode)
            .map_err(|e| RdfDiagnostic::error("oxigraph-graph-blank", e.to_string())),
        other => Err(RdfDiagnostic::error(
            "oxigraph-graph-name-unsupported",
            format!("serializer graph names must be IRI or blank, got {other:?}"),
        )),
    }
}

fn event_error(error: EventError) -> RdfDiagnostic {
    RdfDiagnostic::error("rdf-event-source", error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DatasetSink;

    fn serialization_fixture() -> std::sync::Arc<RdfDataset> {
        let mut builder = RdfDatasetBuilder::new();
        let default_s = builder.intern_iri_value("https://example.org/default-s");
        let named_s = builder.intern_iri_value("https://example.org/named-s");
        let p = builder.intern_iri_value("https://example.org/p");
        let default_o = builder.intern_iri_value("https://example.org/default-o");
        let named_o = builder.intern_iri_value("https://example.org/named-o");
        let g = builder.intern_iri_value("https://example.org/g");
        builder.push_quad(default_s, p, default_o, None);
        builder.push_quad(named_s, p, named_o, Some(g));

        let triple = builder.intern_triple_value(default_s, p, default_o);
        let reifier = builder.intern_iri_value("https://example.org/reifier");
        builder.push_reifier(reifier, triple);
        let confidence = builder.intern_iri_value("https://example.org/confidence");
        let value = builder.intern_literal_value(RdfLiteral::typed(
            "0.9",
            "http://www.w3.org/2001/XMLSchema#decimal",
        ));
        builder.push_annotation(reifier, confidence, value);

        builder.freeze().expect("fixture freezes")
    }

    fn serialize_text(dataset: &RdfDataset, media_type: &str, graph: SerializeGraph<'_>) -> String {
        let mut out = Vec::new();
        OxigraphBackend
            .serialize(
                dataset,
                RdfSerializeRequest {
                    media_type,
                    graph,
                    base_iri: None,
                },
                &mut out,
            )
            .expect("serialize");
        String::from_utf8(out).expect("utf-8")
    }

    #[test]
    fn parses_bytes_into_event_sink() {
        let backend = OxigraphBackend;
        let mut sink = DatasetSink::new();
        backend
            .parse_into(
                RdfParseRequest {
                    bytes: b"<rel> <p> <o> .",
                    media_type: "text/turtle",
                    base_iri: Some("https://example.org/"),
                    source_name: Some("inline.ttl"),
                },
                &mut sink,
            )
            .expect("parse into sink");
        let dataset = sink.into_dataset().expect("sink finished");
        assert_eq!(dataset.quad_count(), 1);
        assert!(dataset
            .term_id_by_value(&TermValue::Iri("https://example.org/rel".to_owned()))
            .is_some());
    }

    #[test]
    fn serializes_dataset_to_nquads() {
        let dataset = serialization_fixture();
        let text = serialize_text(&dataset, "application/n-quads", SerializeGraph::Dataset);
        assert!(text.contains("<https://example.org/default-s>"));
        assert!(text.contains("<https://example.org/named-s>"));
        assert!(text.contains("<https://example.org/g>"));
        assert!(text.contains("<http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies>"));
        assert!(text.contains("<https://example.org/confidence>"));
    }

    #[test]
    fn serializes_default_graph_without_named_graph_rows() {
        let dataset = serialization_fixture();
        let text = serialize_text(
            &dataset,
            "application/n-triples",
            SerializeGraph::DefaultGraph,
        );
        assert!(text.contains("<https://example.org/default-s>"));
        assert!(text.contains("<https://example.org/reifier>"));
        assert!(text.contains("<https://example.org/confidence>"));
        assert!(!text.contains("<https://example.org/named-s>"));
        assert!(!text.contains("<https://example.org/g>"));
    }

    #[test]
    fn serializes_named_graph_as_graph_local_triples() {
        let dataset = serialization_fixture();
        let graph = TermValue::Iri("https://example.org/g".to_owned());
        let text = serialize_text(
            &dataset,
            "application/n-triples",
            SerializeGraph::Named(&graph),
        );
        assert!(text.contains("<https://example.org/named-s>"));
        assert!(text.contains("<https://example.org/named-o>"));
        assert!(!text.contains("<https://example.org/g>"));
        assert!(!text.contains("<https://example.org/default-s>"));
        assert!(!text.contains("<https://example.org/reifier>"));
    }

    #[test]
    fn evaluates_query_and_update() {
        let backend = OxigraphBackend;
        let mut store = Store::new().expect("store");

        backend
            .update(
                &mut store,
                SparqlRequest {
                    query: "INSERT DATA { <https://e/s> <https://e/p> <https://e/o> }",
                    base_iri: None,
                },
            )
            .expect("update");

        let ask = backend
            .query(
                &store,
                SparqlRequest {
                    query: "ASK { <https://e/s> <https://e/p> <https://e/o> }",
                    base_iri: None,
                },
            )
            .expect("ask");
        assert!(matches!(ask, SparqlResult::Boolean(true)));

        let results = backend
            .query(
                &store,
                SparqlRequest {
                    query: "SELECT ?o WHERE { <https://e/s> <https://e/p> ?o }",
                    base_iri: None,
                },
            )
            .expect("select");
        let SparqlResult::Solutions { variables, rows } = results else {
            panic!("expected solutions");
        };
        assert_eq!(variables, vec!["o"]);
        assert_eq!(
            rows,
            vec![vec![Some(TermValue::Iri("https://e/o".to_owned()))]]
        );

        let graph = backend
            .query(
                &store,
                SparqlRequest {
                    query: "CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }",
                    base_iri: None,
                },
            )
            .expect("construct");
        let SparqlResult::Graph(dataset) = graph else {
            panic!("expected graph result");
        };
        assert_eq!(dataset.quad_count(), 1);
        assert!(dataset
            .term_id_by_value(&TermValue::Iri("https://e/s".to_owned()))
            .is_some());
    }
}

#[cfg(test)]
mod serialize_to_format_tests {
    use super::*;
    use crate::dataset_io::dataset_from_bytes;
    use ::oxigraph::io::{RdfFormat, RdfParser};
    use std::collections::BTreeSet;

    // ── helpers ──────────────────────────────────────────────────────────────────

    fn jsonld() -> RdfFormat {
        RdfFormat::JsonLd {
            profile: Default::default(),
        }
    }

    /// Parse a serialized byte slice back into a canonical set of NQ-line strings.
    /// Uses oxigraph's lenient parser for all formats.
    fn canonical_quad_strings(bytes: &[u8], format: RdfFormat) -> BTreeSet<String> {
        RdfParser::from_format(format)
            .lenient()
            .for_slice(bytes)
            .filter_map(|q| q.ok())
            .map(|q| q.to_string())
            .collect()
    }

    /// A star-free dataset: 1 default-graph quad + 1 named-graph quad.
    fn star_free_dataset() -> std::sync::Arc<RdfDataset> {
        let mut b = RdfDatasetBuilder::new();
        let s = b.intern_iri_value("https://example.org/s");
        let p = b.intern_iri_value("https://example.org/p");
        let o = b.intern_iri_value("https://example.org/o");
        let g = b.intern_iri_value("https://example.org/g");
        let s2 = b.intern_iri_value("https://example.org/s2");
        let o2 = b.intern_iri_value("https://example.org/o2");
        b.push_quad(s, p, o, None);
        b.push_quad(s2, p, o2, Some(g));
        b.freeze().expect("star_free_dataset freeze")
    }

    /// A dataset WITH one reifier (rdf:reifies binding) + one annotation.
    fn reifier_dataset() -> std::sync::Arc<RdfDataset> {
        let nq = concat!(
            "<https://e/s> <https://e/p> <https://e/o> .\n",
            "<https://e/r> <http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies> ",
            "<<( <https://e/s> <https://e/p> <https://e/o> )>> .\n",
            "<https://e/r> <https://e/confidence> \"0.9\" .\n",
        );
        dataset_from_bytes(nq.as_bytes(), RdfFormat::NTriples).expect("reifier_dataset")
    }

    // ── Test 1: star-free dataset round-trips through every format ──────────────
    #[test]
    fn star_free_nquads_roundtrip_drops_zero() {
        let ds = star_free_dataset();
        let out =
            serialize_dataset_to_format(&ds, RdfFormat::NQuads, None).expect("serialize to NQuads");
        assert_eq!(out.statement_rows_dropped, 0);
        let text = String::from_utf8(out.bytes).expect("valid utf-8");
        // Both quads must appear; named-graph IRI must be preserved in NQuads.
        assert!(
            text.contains("https://example.org/s"),
            "default-graph quad present"
        );
        assert!(
            text.contains("https://example.org/s2"),
            "named-graph quad present"
        );
        assert!(
            text.contains("https://example.org/g"),
            "named graph IRI preserved in NQuads"
        );
    }

    #[test]
    fn star_free_turtle_roundtrip_drops_zero() {
        let ds = star_free_dataset();
        let out =
            serialize_dataset_to_format(&ds, RdfFormat::Turtle, None).expect("serialize to Turtle");
        assert_eq!(out.statement_rows_dropped, 0);
        let after = canonical_quad_strings(&out.bytes, RdfFormat::Turtle);
        // The default-graph quad must appear.
        assert!(after.iter().any(|s| s.contains("https://example.org/s")));
        // Turtle is default-graph-only: the named-graph quad (s2 in graph g) is
        // NOT emitted. This is by design — triple-only formats output only the
        // default graph. The named-graph IRI must also not appear as a graph name.
        assert!(
            !after.iter().any(|s| s.contains("https://example.org/g")),
            "Turtle must not emit the named graph IRI"
        );
    }

    #[test]
    fn star_free_trig_roundtrip_drops_zero() {
        let ds = star_free_dataset();
        let out =
            serialize_dataset_to_format(&ds, RdfFormat::TriG, None).expect("serialize to TriG");
        assert_eq!(out.statement_rows_dropped, 0);
        let after = canonical_quad_strings(&out.bytes, RdfFormat::TriG);
        assert!(after.iter().any(|s| s.contains("https://example.org/s")));
        assert!(after.iter().any(|s| s.contains("https://example.org/s2")));
        // Named graph must survive in a dataset-capable format.
        assert!(after.iter().any(|s| s.contains("https://example.org/g")));
    }

    #[test]
    fn star_free_jsonld_roundtrip_drops_zero() {
        let ds = star_free_dataset();
        let out = serialize_dataset_to_format(&ds, jsonld(), None).expect("serialize to JSON-LD");
        assert_eq!(out.statement_rows_dropped, 0);
        let after = canonical_quad_strings(&out.bytes, jsonld());
        assert!(after.iter().any(|s| s.contains("https://example.org/s")));
    }

    #[test]
    fn star_free_rdfxml_roundtrip_drops_zero() {
        let ds = star_free_dataset();
        let out = serialize_dataset_to_format(&ds, RdfFormat::RdfXml, None)
            .expect("serialize to RDF/XML");
        assert_eq!(out.statement_rows_dropped, 0);
        let after = canonical_quad_strings(&out.bytes, RdfFormat::RdfXml);
        assert!(after.iter().any(|s| s.contains("https://example.org/s")));
    }

    // ── Test 2: reifier dataset → NQuads lossless ────────────────────────────────

    #[test]
    fn reifier_nquads_lossless() {
        let ds = reifier_dataset();
        assert_eq!(ds.reifiers().count(), 1);
        assert_eq!(ds.annotations().count(), 1);

        let out =
            serialize_dataset_to_format(&ds, RdfFormat::NQuads, None).expect("serialize to NQuads");
        assert_eq!(
            out.statement_rows_dropped, 0,
            "NQuads is star-capable: no rows dropped"
        );

        let text = String::from_utf8(out.bytes.clone()).expect("valid utf-8");
        assert!(
            text.contains("22-rdf-syntax-ns#reifies"),
            "rdf:reifies row present"
        );
        assert!(
            text.contains("https://e/confidence"),
            "annotation row present"
        );
        assert!(text.contains("https://e/s"), "base quad present");
    }

    // ── Test 3: reifier dataset → JSON-LD drops statement rows ──────────────────

    #[test]
    fn reifier_jsonld_drops_statement_rows() {
        let ds = reifier_dataset();
        let out = serialize_dataset_to_format(&ds, jsonld(), None).expect("serialize to JSON-LD");
        // 1 reifier + 1 annotation = 2 statement rows dropped.
        assert_eq!(out.statement_rows_dropped, 2);

        // Re-parsed base quads must include the original base triple.
        let after = canonical_quad_strings(&out.bytes, jsonld());
        assert!(
            after.iter().any(|s| s.contains("https://e/s")),
            "base quad present"
        );
        // Statement-layer triples must NOT appear.
        assert!(
            !after.iter().any(|s| s.contains("22-rdf-syntax-ns#reifies")),
            "rdf:reifies must not appear in JSON-LD output"
        );
    }

    // ── Test 4: reifier dataset → RDF/XML drops statement rows ──────────────────

    #[test]
    fn reifier_rdfxml_drops_statement_rows() {
        let ds = reifier_dataset();
        let out = serialize_dataset_to_format(&ds, RdfFormat::RdfXml, None)
            .expect("serialize to RDF/XML");
        assert_eq!(out.statement_rows_dropped, 2);

        let after = canonical_quad_strings(&out.bytes, RdfFormat::RdfXml);
        assert!(
            after.iter().any(|s| s.contains("https://e/s")),
            "base quad present in RDF/XML"
        );
        assert!(
            !after.iter().any(|s| s.contains("22-rdf-syntax-ns#reifies")),
            "rdf:reifies must not appear in RDF/XML output"
        );
    }

    // ── Test 5: named graph preserved vs flattened ───────────────────────────────

    #[test]
    fn named_graph_preserved_in_nquads_and_trig() {
        let ds = star_free_dataset();
        for format in [RdfFormat::NQuads, RdfFormat::TriG] {
            let out = serialize_dataset_to_format(&ds, format, None).expect("serialize");
            let after = canonical_quad_strings(&out.bytes, format);
            assert!(
                after.iter().any(|s| s.contains("https://example.org/g")),
                "{format}: named graph must be preserved"
            );
        }
    }

    #[test]
    fn named_graph_flattened_in_turtle_and_ntriples_and_jsonld_and_rdfxml() {
        let ds = star_free_dataset();
        // Triple-only formats (Turtle, N-Triples, RDF/XML) only emit the default
        // graph; named-graph quads are dropped entirely (the graph IRI does not
        // appear as a graph-name component, and the triple content is omitted).
        for (format, parse_format) in [
            (RdfFormat::Turtle, RdfFormat::Turtle),
            (RdfFormat::NTriples, RdfFormat::NTriples),
            (RdfFormat::RdfXml, RdfFormat::RdfXml),
        ] {
            let out = serialize_dataset_to_format(&ds, format, None).expect("serialize");
            let after = canonical_quad_strings(&out.bytes, parse_format);
            // Default-graph triple must be present.
            assert!(
                after.iter().any(|s| s.contains("https://example.org/s")),
                "{format}: default-graph quad must be present"
            );
            // Named-graph IRI must NOT appear (no graph component in triple-only
            // formats, and named-graph triples are not re-emitted as default-graph).
            assert!(
                !after.iter().any(|s| s.contains("https://example.org/g")),
                "{format}: named graph IRI must not appear"
            );
        }
        // JSON-LD supports datasets, so g SHOULD appear.
        let out = serialize_dataset_to_format(&ds, jsonld(), None).expect("serialize JSON-LD");
        let after = canonical_quad_strings(&out.bytes, jsonld());
        assert!(
            after.iter().any(|s| s.contains("https://example.org/g")),
            "JSON-LD: named graph must be preserved (dataset-capable)"
        );
    }
}
