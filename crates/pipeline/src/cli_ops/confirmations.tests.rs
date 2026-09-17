// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn prefixes() -> Vec<(String, String)> {
    crate::stages::superset::rdf_prefixes()
}

const SAMPLE_TTL: &str = r#"@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl:  <http://www.w3.org/2002/07/owl#> .

<http://example.org/B> a owl:Class ; rdfs:label "B" .
<http://example.org/A> a owl:Class ; rdfs:label "A" .
"#;

#[test]
fn normalize_is_idempotent() {
    let once = canonicalize_turtle(SAMPLE_TTL.as_bytes(), &prefixes()).expect("first pass");
    let twice = canonicalize_turtle(&once, &prefixes()).expect("second pass");
    assert_eq!(
        once, twice,
        "canonicalization must be a fixed point (idempotent)"
    );
    // The canonical form is non-empty valid Turtle.
    let text = String::from_utf8(once).expect("utf-8");
    assert!(text.contains("example.org/A"), "content preserved");
    assert!(text.contains("example.org/B"), "content preserved");
}

#[test]
fn up_projection_gate_audit_smoke() {
    // A minimal corpus with no lift rules: the audit runs end to end and renders a
    // Markdown report without panicking (the wiring smoke; the heavy real-corpus
    // audit is exercised by the up_projection_gates own tests).
    let corpus = vec![(
        "smoke".to_string(),
        "@prefix ex: <http://example.org/> .\nex:a ex:p ex:b .\n".to_string(),
    )];
    let (ledger, markdown) =
        up_projection_gate_audit(&[], &[], &corpus).expect("audit runs end to end");
    assert!(!markdown.trim().is_empty(), "a report is rendered");
    // Sanity: the ledger total is the partition sum of its tiers.
    assert_eq!(ledger.total(), ledger.totals.total());
}
