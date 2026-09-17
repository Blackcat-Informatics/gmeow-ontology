// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Quality and FAIR scoring via the OOPS! and FOOPS! web services.
//!
//! Both are NETWORK calls (blocking HTTP
//! over the already-vendored `ureq`), so callers gate them (they are never on a
//! build gate). OOPS! accepts inline ontology content (works pre-publication);
//! FOOPS! assesses a dereferenceable ontology URL (meaningful only once published).
//!
//! The unit tests that actually hit the network are opt-in behind the
//! `GMEOW_RUN_NETWORK` environment variable, so the default test gate never makes a
//! network call — the code itself always ships.

use std::time::Duration;

const OOPS_ENDPOINT: &str = "https://oops.linkeddata.es/rest";
const FOOPS_ENDPOINT: &str = "https://w3id.org/foops/assessOntology";

const OOPS_REQUEST_TEMPLATE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<OOPSRequest>
  <OntologyURI></OntologyURI>
  <OntologyContent><![CDATA[{content}]]></OntologyContent>
  <Pitfalls></Pitfalls>
  <OutputFormat>RDF/XML</OutputFormat>
</OOPSRequest>
"#;

fn stage_err(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "quality".to_string(),
        message: message.into(),
    })
}

/// A FOOPS! FAIR assessment summary.
#[derive(Debug, Clone, PartialEq)]
pub struct FoopsResult {
    /// The overall FAIR score.
    pub score: f64,
    /// The number of checks the assessment ran.
    pub checks_total: usize,
    /// The number of checks that passed.
    pub checks_passed: usize,
}

/// Run the OOPS! pitfall scanner on inline ontology content.
///
/// Posts the ontology (Turtle / RDF-XML) inside an OOPS! request envelope and
/// returns the evaluation as RDF/XML text.
///
/// * `ttl_content` — the ontology serialized as RDF (Turtle/RDF-XML).
/// * `timeout` — the HTTP timeout.
///
/// # Errors
///
/// - The network request fails, or the service returns a non-success status.
pub fn run_oops(ttl_content: &str, timeout: Duration) -> Result<String, gmeow_errors::Diag> {
    let body = OOPS_REQUEST_TEMPLATE.replace("{content}", ttl_content);
    let response = ureq::post(OOPS_ENDPOINT)
        .content_type("application/xml")
        .config()
        .timeout_global(Some(timeout))
        .build()
        .send(body.as_bytes())
        .map_err(|e| stage_err(format!("OOPS! request failed: {e}")))?;
    response
        .into_body()
        .read_to_string()
        .map_err(|e| stage_err(format!("OOPS! response read failed: {e}")))
}

/// Run the FOOPS! FAIR assessment on a dereferenceable ontology URL.
///
/// Posts the ontology URL as a form field and summarizes the returned FAIR score.
///
/// * `ontology_url` — the published ontology IRI/URL to assess.
/// * `timeout` — the HTTP timeout.
///
/// # Errors
///
/// - The network request fails, the service returns a non-success status, or the
///   JSON payload cannot be parsed.
pub fn run_foops(ontology_url: &str, timeout: Duration) -> Result<FoopsResult, gmeow_errors::Diag> {
    let response = ureq::post(FOOPS_ENDPOINT)
        .config()
        .timeout_global(Some(timeout))
        .build()
        .send_form([("ontologyUrl", ontology_url)])
        .map_err(|e| stage_err(format!("FOOPS! request failed: {e}")))?;
    let text = response
        .into_body()
        .read_to_string()
        .map_err(|e| stage_err(format!("FOOPS! response read failed: {e}")))?;
    parse_foops_payload(&text)
}

/// Summarize a FOOPS! JSON payload into a [`FoopsResult`]. Factored out so the
/// payload→summary reduction is unit-testable without a network call.
fn parse_foops_payload(text: &str) -> Result<FoopsResult, gmeow_errors::Diag> {
    let payload: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| stage_err(format!("FOOPS! payload not JSON: {e}")))?;
    let checks = payload
        .get("checks")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();
    let passed = checks
        .iter()
        .filter(|c| {
            c.get("status").and_then(|s| s.as_str()) == Some("ok")
                || c.get("score").and_then(serde_json::Value::as_f64) == Some(1.0)
        })
        .count();
    let score = payload
        .get("overall_score")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0);
    Ok(FoopsResult {
        score,
        checks_total: checks.len(),
        checks_passed: passed,
    })
}

#[path = "quality.tests.rs"]
#[cfg(test)]
mod tests;
