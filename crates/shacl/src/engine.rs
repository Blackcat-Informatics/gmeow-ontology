// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

//! SHACL Core validation engine.
//!
//! Task 1 stub: `validate` always returns an empty conforming report. The real
//! constraint evaluation arrives in Task 3.

use oxigraph::io::RdfFormat;
use oxigraph::model::Term;
use oxigraph::store::Store;

use crate::report::ValidationReport;
use crate::shapes::{Shape, Shapes};

/// Validate `data` against `shapes`, returning a [`ValidationReport`].
///
/// Task 1 stub: always returns a conforming empty report.
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

/// Check whether a single focus node conforms to a single shape.
///
/// Task 1 stub: always returns `true`. Real constraint evaluation arrives
/// in Task 3.
// Task 3's constraints module will call this; the dead_code lint fires
// in Task 1 because no caller exists yet.
#[allow(dead_code)]
pub(crate) fn conforms_to_shape(_data: &Store, _focus: &Term, _shape: &Shape) -> bool {
    true
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
