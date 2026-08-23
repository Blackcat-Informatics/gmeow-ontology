// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The oxigraph-free native SPARQL substrate for the introspection export leaves
//! (`result_shapes`, `result_shape_composition`, `research_objects`).
//!
//! Each of those leaves used to parse Turtle sources into a transient oxigraph
//! `Store` and query it via `SparqlEvaluator`. This module replaces both with the
//! native stack:
//!
//! * the data graph is a frozen [`Arc<RdfDataset>`](RdfDataset), built by parsing
//!   each Turtle source through the canonical native codec (`purrdf::parse_dataset`)
//!   and [`RdfDataset::union`]-ing them into one (blanks standardized apart per
//!   source, the same disjointness `turtle_bytes_into_store_scoped` provided);
//! * SELECT / CONSTRUCT queries run through [`NativeSparqlEngine`] (`crates/sparql-eval`),
//!   the single required impl of the `gmeow-rdf-core` `SparqlEngine` seam;
//! * result terms are dataset-independent [`TermValue`]s, projected by column index.

use std::sync::Arc;

use purrdf::sparql::NativeSparqlEngine;
use purrdf::{RdfDataset, SparqlEngine, SparqlRequest, SparqlResult, TermValue, parse_dataset};

/// One SELECT result: the projected variable names (no leading `?`) and the rows of
/// optional terms — the dataset-independent egress shape the native engine materializes.
#[derive(Debug)]
pub struct Solutions {
    pub variables: Vec<String>,
    pub rows: Vec<Vec<Option<TermValue>>>,
}

impl Solutions {
    /// The column index of `var` in the projected variable list, if present.
    pub fn col(&self, var: &str) -> Option<usize> {
        self.variables.iter().position(|v| v == var)
    }
}

/// Parse one Turtle byte buffer into a frozen dataset (the native codec).
pub fn dataset_from_turtle(
    bytes: &[u8],
    context: &str,
) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    parse_dataset(bytes, "text/turtle", None).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("syntax error in {context}: {e}"),
        })
    })
}

/// Read + parse a set of Turtle files and union them into ONE frozen dataset
/// (each source's blanks standardized apart by [`RdfDataset::union`], the
/// IR-native twin of accumulating scope-keyed sources into one store).
pub fn dataset_from_files(
    paths: &[std::path::PathBuf],
) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    let mut parsed: Vec<Arc<RdfDataset>> = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = std::fs::read(path).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Io {
                message: e.to_string(),
            })
        })?;
        parsed.push(dataset_from_turtle(&bytes, &path.display().to_string())?);
    }
    let refs: Vec<&RdfDataset> = parsed.iter().map(AsRef::as_ref).collect();
    Ok(Arc::new(RdfDataset::union(&refs)))
}

/// Run a SPARQL query over `dataset`, returning the native result.
pub fn query(dataset: &Arc<RdfDataset>, sparql: &str) -> Result<SparqlResult, gmeow_errors::Diag> {
    let engine = NativeSparqlEngine::new();
    engine
        .query(
            dataset,
            SparqlRequest {
                query: sparql,
                base_iri: None,
                substitutions: &[],
            },
        )
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: e.to_string(),
            })
        })
}

/// Run a SELECT query, hard-failing if it is not a SELECT.
pub fn select(dataset: &Arc<RdfDataset>, sparql: &str) -> Result<Solutions, gmeow_errors::Diag> {
    match query(dataset, sparql)? {
        SparqlResult::Solutions {
            variables, rows, ..
        } => Ok(Solutions { variables, rows }),
        SparqlResult::Boolean(_) | SparqlResult::Graph(_) => {
            Err(gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: "introspection query must be a SELECT".to_owned(),
            }))
        }
    }
}

/// The IRI a binding carries, or `None` if unbound / not an IRI.
pub fn term_iri(term: Option<&TermValue>) -> Option<String> {
    match term {
        Some(TermValue::Iri(iri)) => Some(iri.clone()),
        _ => None,
    }
}

/// The lexical string a binding carries: a literal's lexical form, or an IRI's text
/// (mirrors the prior `lit()` helper which accepted either). `None` if unbound or a
/// blank / quoted triple.
pub fn term_str(term: Option<&TermValue>) -> Option<String> {
    match term {
        Some(TermValue::Literal { lexical_form, .. }) => Some(lexical_form.clone()),
        Some(TermValue::Iri(iri)) => Some(iri.clone()),
        _ => None,
    }
}
