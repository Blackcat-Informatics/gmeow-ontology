// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native GMN vector observations shared by the producer and public CLI.
//! Executing a vector and comparing its frozen bytes are separate operations:
//! changing an expectation never requires recompiling its source model.

use serde::{Deserialize, Serialize};

use crate::{
    Gmn0Model, GmnDictionary, gmn1_read, gmn1_write, idempotence_check, per_claim_round_trip_check,
};

/// Actual output and independent witnesses from one positive vector execution.
#[derive(Debug, Serialize, Deserialize)]
pub struct PositiveObservation {
    /// Complete emitted surface, compared against the independently frozen bytes.
    pub text: String,
    /// Canonical source graph, including its RDF 1.2 statement layer.
    pub source: String,
    /// Canonical reconstruction, or the actual decoder failure.
    pub reconstructed: Result<String, gmeow_errors::RecordedDiag>,
    /// Digest computed from the input model.
    pub content_digest: String,
    /// Per-claim inversion witness from the complete native document.
    pub per_claim: Result<(), gmeow_errors::RecordedDiag>,
    /// Re-encoding witness, including the native reference table.
    pub idempotence: Result<(), gmeow_errors::RecordedDiag>,
}

impl PositiveObservation {
    /// Compare an observed execution against a separately supplied golden.
    /// The checks do not parse, lower, encode or materialize anything.
    #[must_use]
    pub fn failures(&self, frozen: &[u8]) -> Vec<String> {
        let mut failures = Vec::new();
        if self.text.as_bytes() != frozen {
            failures.push("writer output differs from frozen vector (byte mismatch)".to_owned());
        }
        match &self.reconstructed {
            Ok(back) if back == &self.source => {}
            Ok(_) => failures
                .push("reconstructed model is not canonically equal to the source".to_owned()),
            Err(error) => failures.push(format!("reader failed: {error}")),
        }
        if let Err(error) = &self.per_claim {
            failures.push(format!("per-claim inversion witness failed: {error}"));
        }
        if let Err(error) = &self.idempotence {
            failures.push(format!("idempotence witness failed: {error}"));
        }
        failures
    }
}

/// Execute a selected native model once and retain the independent witnesses.
///
/// # Errors
/// Returns the actual writer failure; later witness failures remain in the observation.
pub fn observe_positive(
    model: &Gmn0Model,
    dictionary: &GmnDictionary,
) -> gmeow_errors::Result<PositiveObservation> {
    let document = gmn1_write(model, dictionary)?;
    let source = model.canonical_nquads();
    let content_digest = crate::gmn1_digest::canonical_content_digest(&source);
    Ok(PositiveObservation {
        source,
        reconstructed: gmn1_read(&document, dictionary)
            .map(|back| back.canonical_nquads())
            .map_err(record_failure),
        content_digest,
        per_claim: per_claim_round_trip_check(model, dictionary).map_err(record_failure),
        idempotence: idempotence_check(&document, dictionary).map_err(record_failure),
        text: document.text,
    })
}

fn record_failure(error: impl Into<gmeow_errors::Diag>) -> gmeow_errors::RecordedDiag {
    gmeow_errors::DiagLedger::new().record(error.into(), gmeow_errors::StageId::new("gmn1-vector"))
}

#[path = "gmn_conformance.tests.rs"]
#[cfg(test)]
mod tests;
