// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Running a discovered case: load data, parse + evaluate the query.

use std::sync::Arc;

use gmeow_rdf_core::{RdfDataset, SparqlEngine, SparqlRequest, SparqlResult};
use gmeow_sparql_eval::{NativeSparqlEngine, RemoteQuerySource};

use crate::manifest::{SparqlTestCase, TestKind};

const BASE: &str = "http://gmeow.test/manifest/";

/// The outcome of running a case (before comparison against the expected result).
pub enum RunOutcome {
    /// A `QueryEvaluationTest` result.
    Eval(SparqlResult),
    /// A syntax test: did the query parse?
    Syntax { parsed_ok: bool },
}

/// Load the case's `qt:data` files into a single default-graph dataset.
///
/// Named-graph (`qt:graphData`) cases are not modeled by this default-graph
/// loader and surface as an error (recorded by the harness, never silent).
///
/// # Errors
///
/// Returns a message on a read/parse failure or an unsupported multi-graph case.
pub fn load_dataset(case: &SparqlTestCase) -> Result<Arc<RdfDataset>, String> {
    if !case.graph_data.is_empty() {
        return Err("named-graph (qt:graphData) data is not supported by this harness".to_owned());
    }
    // Concatenate the Turtle data files (Turtle permits prefix/base re-declaration)
    // and parse once into the default graph.
    let mut bytes = Vec::new();
    for data in &case.data {
        let chunk = std::fs::read(data).map_err(|e| format!("read {}: {e}", data.display()))?;
        bytes.extend_from_slice(&chunk);
        bytes.push(b'\n');
    }
    gmeow_rdf::parse_dataset(&bytes, "text/turtle", Some(BASE))
        .map_err(|e| format!("parse data for {}: {e}", case.iri))
}

/// Run `case`, optionally resolving `SERVICE` clauses through `remote`.
///
/// # Errors
///
/// Returns a message on a read/parse/evaluation failure (the harness decides
/// whether that is an expected failure).
pub fn run(
    case: &SparqlTestCase,
    remote: Option<&dyn RemoteQuerySource>,
) -> Result<RunOutcome, String> {
    let query_text = std::fs::read_to_string(&case.query)
        .map_err(|e| format!("read query {}: {e}", case.query.display()))?;

    match case.kind {
        TestKind::PositiveSyntax | TestKind::NegativeSyntax => {
            let parsed_ok = gmeow_sparql_algebra::SparqlParser::new()
                .parse_query(&query_text)
                .is_ok();
            Ok(RunOutcome::Syntax { parsed_ok })
        }
        TestKind::QueryEval => {
            let dataset = load_dataset(case)?;
            let engine = NativeSparqlEngine::new();
            let request = SparqlRequest {
                query: &query_text,
                base_iri: Some(BASE),
            };
            let result = match remote {
                Some(source) => engine.query_with_source(&dataset, request, source),
                None => engine.query(&dataset, request),
            }
            .map_err(|e| format!("evaluate {}: {e}", case.iri))?;
            Ok(RunOutcome::Eval(result))
        }
        TestKind::Unknown => Err(format!("unmodeled test type for {}", case.iri)),
    }
}
