// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Oxigraph implementation of the purrdf SPARQL backend trait (P2d, #887).
//!
//! The RDF **text** codec role this backend once carried (`RdfParserBackend` +
//! `RdfSerializer` over the oxigraph text io) was retired in #909 / EPIC #906 S3: the
//! always-on native [`GtsCodecBackend`](crate::GtsCodecBackend) over the `gmeow-gts`
//! codecs is the single parse/serialize chokepoint now. What remains here is the
//! oxigraph in-memory **Store** SPARQL surface (scope-OUT of #909, owned by the
//! native-SPARQL EPIC children S5–S14), which is text-free — it materializes the IR
//! into a `Store` through `crate::oxigraph::store_from_dataset`, never re-parsing text.

use ::oxigraph::model::{BaseDirection, NamedOrBlankNode, Term as OxTerm, Triple};
use ::oxigraph::sparql::{QueryResults, SparqlEvaluator};
use ::oxigraph::store::Store;

use crate::{
    BlankScope, RdfDatasetBuilder, RdfDiagnostic, RdfLiteral, RdfTextDirection, SparqlEngine,
    SparqlRequest, SparqlResult, TermFactory, TermId, TermValue,
};

/// Oxigraph-backed SPARQL adapter over the in-memory `Store`.
#[derive(Debug, Clone, Copy, Default)]
pub struct OxigraphBackend;

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
            Ok(SparqlResult::Solutions {
                variables,
                rows,
                aux: RdfDatasetBuilder::new().freeze().expect("empty aux"),
            })
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let SparqlResult::Solutions {
            variables, rows, ..
        } = results
        else {
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
