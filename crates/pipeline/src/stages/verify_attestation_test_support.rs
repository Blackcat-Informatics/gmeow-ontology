// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only helpers for the owning production module.

use super::*;

/// Evaluate every selected bad-example query against the exact EDB/result pair and
/// project one deterministic quality assessment per query. This function never invokes
/// the reasoner.
#[cfg(test)]
pub(super) fn evaluate_attestation(
    edb: &RdfDataset,
    reasoning: &gmeow_logic::result::ReasoningResult,
    queries: &[(String, String)],
) -> Result<(Arc<RdfDataset>, gmeow_errors::Report), gmeow_errors::Diag> {
    let gates = test_gates();
    let verification = gmeow_logic::verify::PreparedVerification::new(queries, &gates)?;
    let report = verification
        .verify_with_reasoning_result(edb, reasoning)
        .map_err(|e| stage_err(format!("native verify: {e}")))?;
    let failed: BTreeSet<String> = report
        .findings
        .iter()
        .filter(|finding| {
            finding.severity == gmeow_errors::Severity::Error && finding.code.starts_with("verify.")
        })
        .map(|finding| finding.code["verify.".len()..].to_string())
        .collect();
    let turtle = emit_verify_attestation(queries, &failed);
    let dataset = crate::stages::carrier::parse_into_graph(
        turtle.as_bytes(),
        "text/turtle",
        crate::stages::carrier::GRAPH_VERIFY,
    )?;
    Ok((dataset, report))
}

#[cfg(test)]
pub(super) fn test_gates() -> gmeow_logic::verify::PreparedReasonedGates {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    crate::fixture::authenticated_reasoned_gates(&root)
        .expect("producer-selected native laws; tests never compile authored sources")
}
