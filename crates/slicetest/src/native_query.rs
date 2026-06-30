// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The oxigraph-free native SPARQL substrate for the slice-test harness (EPIC #906).
//!
//! Every store the harness ran over was an oxigraph in-memory `Store` queried via
//! `SparqlEvaluator`; this module replaces both with the native stack:
//!
//! * the data graph is a frozen [`Arc<RdfDataset>`](RdfDataset), built by parsing
//!   each Turtle source through the canonical native codec
//!   (`gmeow_rdf::parse_dataset`) and `RdfDataset::union`-ing them into one — the
//!   same merge `gmeow_validate::store::build_store` did, but in the IR, never an
//!   oxigraph `Store`;
//! * queries run through [`NativeSparqlEngine`] (`crates/sparql-eval`), the single
//!   required impl of the `gmeow-rdf-core` `SparqlEngine` seam;
//! * result terms are dataset-independent [`TermValue`]s, rendered to a canonical
//!   N-Triples lexical form so a competency question's expected rows compare on the
//!   SAME string both sides.
//!
//! Slice sources are Turtle (a single default graph), so the union keeps every quad
//! in the default graph — no flattening is required (unlike the GTS-bundle conformance
//! gate, whose `gmeow.gts` carries named graphs).

use std::path::PathBuf;
use std::sync::Arc;

use gmeow_rdf::parse_dataset;
use gmeow_rdf_core::ir::RdfDatasetBuilder;
use gmeow_rdf_core::{RdfDataset, SparqlEngine, SparqlRequest, SparqlResult, TermValue};
use gmeow_sparql_eval::NativeSparqlEngine;

/// One SELECT result: the projected variable names and the rows of optional terms,
/// the dataset-independent egress shape the native engine materializes.
pub struct Solutions {
    pub variables: Vec<String>,
    pub rows: Vec<Vec<Option<TermValue>>>,
}

/// Parse one Turtle file into a frozen dataset (the native codec, lenient on the
/// private-use `@x-gmeow-*` language tags, exactly as `gmeow_validate`'s `parse_file`).
///
/// # Errors
/// Returns `Err(String)` if the file cannot be read or parsed.
pub fn dataset_from_file(path: &std::path::Path) -> Result<Arc<RdfDataset>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    parse_dataset(&bytes, "text/turtle", None).map_err(|e| format!("parse {}: {e}", path.display()))
}

/// Build one merged dataset from a set of Turtle sources by parsing each and unioning
/// them — the IR-native twin of `gmeow_validate::store::build_store`.
///
/// # Errors
/// Returns `Err(String)` if any file fails to read or parse.
pub fn dataset_from_files(paths: &[PathBuf]) -> Result<Arc<RdfDataset>, String> {
    let parsed: Vec<Arc<RdfDataset>> = paths
        .iter()
        .map(|p| dataset_from_file(p))
        .collect::<Result<_, _>>()?;
    Ok(union(&parsed))
}

/// Union a set of frozen datasets into one (blank scopes are standardized apart by
/// `RdfDataset::union`). An empty input yields an empty dataset.
#[must_use]
pub fn union(datasets: &[Arc<RdfDataset>]) -> Arc<RdfDataset> {
    let refs: Vec<&RdfDataset> = datasets.iter().map(AsRef::as_ref).collect();
    Arc::new(RdfDataset::union(&refs))
}

/// Merge frozen datasets into one **preserving blank-node identity** across inputs.
///
/// Unlike [`union`] (which standardizes every input's blanks APART under a fresh
/// merge scope), this folds each dataset's already-scope-qualified owned terms into a
/// single builder at the default scope. A blank that appears in several inputs with
/// the same scope-qualified label therefore stays ONE node, and identical quads dedup
/// — the property the RDFS fixpoint depends on (re-deriving an existing quad must NOT
/// mint a fresh blank each round, which `union`'s re-scoping does, defeating
/// termination). The inputs here are all derived FROM one base dataset (its blanks
/// share a scope), so their qualified labels coincide and identity is preserved.
#[must_use]
pub fn merge_preserving_blanks(datasets: &[Arc<RdfDataset>]) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for ds in datasets {
        for quad in ds.owned_quads() {
            builder.push_owned_quad(&quad);
        }
        for reifier in ds.owned_reifiers() {
            builder.push_owned_reifier(&reifier);
        }
        for annotation in ds.owned_annotations() {
            builder.push_owned_annotation(&annotation);
        }
    }
    builder
        .freeze()
        .expect("merge of valid datasets re-freezes successfully")
}

/// Parse inline Turtle into a frozen dataset (the native codec).
///
/// # Errors
/// Returns `Err(String)` if the Turtle fails to parse.
pub fn dataset_from_turtle(ttl: &str) -> Result<Arc<RdfDataset>, String> {
    parse_dataset(ttl.as_bytes(), "text/turtle", None).map_err(|e| format!("parse turtle: {e}"))
}

/// Run a SPARQL query over `dataset`, returning the native result.
///
/// # Errors
/// Returns `Err(String)` on a parse or evaluation error (the diagnostic message).
pub fn query(dataset: &Arc<RdfDataset>, sparql: &str) -> Result<SparqlResult, String> {
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
        .map_err(|e| e.to_string())
}

/// Run a SELECT query, hard-failing if it is not a SELECT.
///
/// # Errors
/// Returns `Err(String)` on a parse/eval error or if the form is not SELECT.
pub fn select(dataset: &Arc<RdfDataset>, sparql: &str) -> Result<Solutions, String> {
    match query(dataset, sparql)? {
        SparqlResult::Solutions {
            variables, rows, ..
        } => Ok(Solutions { variables, rows }),
        SparqlResult::Boolean(_) | SparqlResult::Graph(_) => {
            Err("query must be a SELECT".to_owned())
        }
    }
}

/// Render a [`TermValue`] to a canonical N-Triples lexical form. Used on BOTH the
/// actual binding and the expected cell value so a competency row compares on the
/// same string regardless of binding/iteration order.
#[must_use]
pub fn render_term(term: &TermValue) -> String {
    match term {
        TermValue::Iri(iri) => format!("<{iri}>"),
        TermValue::Blank { label, .. } => format!("_:{label}"),
        TermValue::Literal {
            lexical_form,
            datatype,
            language,
            ..
        } => {
            let escaped = escape_literal(lexical_form);
            match language {
                Some(lang) => format!("\"{escaped}\"@{lang}"),
                None => format!("\"{escaped}\"^^<{datatype}>"),
            }
        }
        TermValue::Triple { s, p, o } => {
            format!(
                "<< {} {} {} >>",
                render_term(s),
                render_term(p),
                render_term(o)
            )
        }
    }
}

/// Escape a literal lexical form for the N-Triples quoted-string rendering used by
/// [`render_term`].
fn escape_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}
