// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

//! SHACL Core validation engine.
//!
//! `validate` is the top-level entry point.  Target resolution and report
//! assembly arrive in Task 4; Task 3 delivers the constraint evaluator in
//! `constraints.rs` which this module delegates to.

use oxigraph::io::RdfFormat;
use oxigraph::store::Store;

use crate::report::ValidationReport;
use crate::shapes::Shapes;

/// Validate `data` against `shapes`, returning a [`ValidationReport`].
///
/// Task 3 stub: target resolution and full report assembly arrive in Task 4.
/// Returns an empty conforming report for now.
pub fn validate(_data: &Store, _shapes: &Shapes) -> ValidationReport {
    ValidationReport {
        conforms: true,
        results: Vec::new(),
    }
}

/// Validate data (N-Triples) against shapes (Turtle), returning a [`ValidationReport`].
///
/// Creates two in-memory stores, loads the respective graphs, parses shapes
/// via [`crate::shapes::from_store`], and delegates to [`validate`].
///
/// # Errors
///
/// Returns an error string if either graph fails to parse.
pub fn validate_graphs(data_nt: &str, shapes_ttl: &str) -> Result<ValidationReport, String> {
    let data = Store::new().map_err(|e| format!("data store creation failed: {e}"))?;
    if !data_nt.is_empty() {
        data.load_from_reader(RdfFormat::NTriples, data_nt.as_bytes())
            .map_err(|e| format!("N-Triples parse error: {e}"))?;
    }

    let shapes_store = Store::new().map_err(|e| format!("shapes store creation failed: {e}"))?;
    if !shapes_ttl.is_empty() {
        shapes_store
            .load_from_reader(RdfFormat::Turtle, shapes_ttl.as_bytes())
            .map_err(|e| format!("Turtle parse error: {e}"))?;
    }

    let shapes = crate::shapes::from_store(&shapes_store)?;
    Ok(validate(&data, &shapes))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_inputs_return_conforming_report() {
        let report = validate_graphs("", "").expect("empty inputs must not error");
        assert!(report.conforms, "empty report must conform");
        assert!(
            report.results.is_empty(),
            "empty report must have no results"
        );
    }

    #[test]
    fn validate_stub_always_conforms() {
        let data = Store::new().unwrap();
        let shapes = Shapes::default();
        let report = validate(&data, &shapes);
        assert!(report.conforms);
        assert!(report.results.is_empty());
    }
}
