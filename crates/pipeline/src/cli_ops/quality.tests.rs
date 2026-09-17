// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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
    let result =
        run_foops("https://w3id.org/foops/", Duration::from_secs(180)).expect("FOOPS! live call");
    assert!(result.checks_total > 0, "FOOPS! ran some checks");
}
