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

use crate::error::PipelineError;

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

fn stage_err(message: impl Into<String>) -> PipelineError {
    PipelineError::Stage {
        stage: "quality".to_string(),
        message: message.into(),
    }
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
pub fn run_oops(ttl_content: &str, timeout: Duration) -> Result<String, PipelineError> {
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
pub fn run_foops(ontology_url: &str, timeout: Duration) -> Result<FoopsResult, PipelineError> {
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
fn parse_foops_payload(text: &str) -> Result<FoopsResult, PipelineError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether the opt-in network lane is enabled.
    fn network_enabled() -> bool {
        std::env::var_os("GMEOW_RUN_NETWORK").is_some()
    }

    #[test]
    fn foops_payload_summary_counts_passing_checks() {
        let payload = r#"{
            "overall_score": 0.75,
            "checks": [
                {"status": "ok"},
                {"score": 1},
                {"status": "fail", "score": 0}
            ]
        }"#;
        let result = parse_foops_payload(payload).expect("parse");
        assert_eq!(result.checks_total, 3);
        assert_eq!(result.checks_passed, 2);
        assert!((result.score - 0.75).abs() < 1e-9);
    }

    #[test]
    fn oops_request_template_embeds_the_content() {
        let body = OOPS_REQUEST_TEMPLATE.replace("{content}", "<> a owl:Ontology .");
        assert!(body.contains("<![CDATA[<> a owl:Ontology .]]>"));
        assert!(body.contains("<OutputFormat>RDF/XML</OutputFormat>"));
    }

    /// Opt-in: actually hit the OOPS! endpoint. Skipped unless `GMEOW_RUN_NETWORK`
    /// is set, so it never runs on the default gate.
    #[test]
    fn oops_live_smoke() {
        if !network_enabled() {
            return;
        }
        let ttl = "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n<http://example.org/o> a owl:Ontology .\n";
        let out = run_oops(ttl, Duration::from_secs(120)).expect("OOPS! live call");
        assert!(!out.trim().is_empty(), "OOPS! returned an evaluation");
    }

    /// Opt-in: actually hit the FOOPS! endpoint. Skipped unless `GMEOW_RUN_NETWORK`.
    #[test]
    fn foops_live_smoke() {
        if !network_enabled() {
            return;
        }
        let result = run_foops("https://w3id.org/foops/", Duration::from_secs(180))
            .expect("FOOPS! live call");
        assert!(result.checks_total > 0, "FOOPS! ran some checks");
    }
}
