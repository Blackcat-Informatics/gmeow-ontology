// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3 bindings for the DSL SHACL validation seam.
//!
//! The Python-facing DSL loaders (`mapping_dsl`, `test_dsl`) used to delegate to
//! `gmeow_tools.dsl_validate`, which built the merged graph in Rust but ran the
//! SHACL engine and formatted violations in Python. This module provides the
//! canonical native entry point so the formatting and provenance enrichment live
//! in one place — the Rust side — eliminating the dual-authority bug (#937,
//! Principle 4).

use std::collections::HashMap;
use std::path::PathBuf;

use oxigraph::model::Term;
use pyo3::prelude::*;

/// Render a gmeow_shacl N-Triples term as the legacy Python seam did:
/// `<http://x>` → `http://x`; `_:b0` → `b0`; literals/plain pass through.
fn term_to_str(term: &Term) -> String {
    let s = term.to_string();
    if let Some(inner) = s.strip_prefix('<').and_then(|t| t.strip_suffix('>')) {
        inner.to_owned()
    } else if let Some(inner) = s.strip_prefix("_:") {
        inner.to_owned()
    } else {
        s
    }
}

/// Validate merged DSL Turtle sources against a SHACL shapes graph and return
/// formatted violation strings.
///
/// `dsl_paths` are processed in order; every named subject is mapped to the
/// first path it appears in. The merged graph is validated against
/// `shapes_ttl` using the native `gmeow_shacl` engine. Each non-conforming
/// result is formatted as:
///
/// ```text
/// focus=<focusNode> | path=<resultPath> | msg=<message> | source=<file>
/// ```
///
/// `path`, `msg`, and `source` are omitted when not applicable. An empty list
/// means the DSL graph conforms.
///
/// # Errors
///
/// Returns a Python `ValueError` on parse/merge/validate failures — a hard
/// fail, never a silent conformant result (P11/§11).
#[pyfunction]
pub fn validate_dsl_shacl(dsl_paths: Vec<String>, shapes_ttl: String) -> PyResult<Vec<String>> {
    if dsl_paths.is_empty() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "validate_dsl_shacl: paths to validate must not be empty",
        ));
    }

    let paths: Vec<PathBuf> = dsl_paths.iter().map(PathBuf::from).collect();
    let merge = crate::dsl::merge_with_provenance(&paths)
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    let data_store = crate::store::build_store_from_nt(&merge.data_nt)
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    let shapes = gmeow_shacl::engine::parse_shapes(&shapes_ttl)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    let report = gmeow_shacl::engine::validate(&data_store, &shapes);

    if report.conforms {
        return Ok(Vec::new());
    }

    let focus_to_file: HashMap<String, String> = merge.focus_to_file.into_iter().collect();
    let mut violations: Vec<String> = Vec::new();

    for result in &report.results {
        let mut parts: Vec<String> = Vec::new();
        let focus_str = term_to_str(&result.focus_node);
        parts.push(format!("focus={focus_str}"));

        if let Some(path) = &result.result_path {
            parts.push(format!("path={}", term_to_str(path)));
        }
        if let Some(message) = &result.message {
            parts.push(format!("msg={message}"));
        }

        // Source provenance only applies to named-IRI focus nodes.
        if let Term::NamedNode(node) = &result.focus_node {
            if let Some(source) = focus_to_file.get(node.as_str()) {
                parts.push(format!("source={source}"));
            }
        }

        violations.push(parts.join(" | "));
    }

    // Defensive: a non-conforming report with no parseable results must still
    // surface (gmeow_shacl reports conforms == results-empty, so unreachable).
    if violations.is_empty() {
        violations.push("SHACL validation failed: non-conforming with no results".to_owned());
    }

    Ok(violations)
}
