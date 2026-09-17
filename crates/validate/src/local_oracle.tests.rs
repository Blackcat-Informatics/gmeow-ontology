// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_errors::model::{Finding, Location, Report, Severity};

const MIN_COUNT: &str = "shacl.MinCountConstraintComponent";

fn fixture(title: &str, code: Option<&str>) -> FixtureView {
    FixtureView {
        title: title.to_string(),
        text: "<urn:s> <urn:p> <urn:o> .".to_string(),
        expected_outcome: Some("nonconforming".to_string()),
        violation_code: code.map(str::to_string),
        rationale: Some("min-count under the required floor".to_string()),
    }
}

/// The core correspondence proof: a finding whose code matches a counter-example
/// gets it (with the help URI + a positive exemplar + the term's entailments); a
/// finding whose code matches NOTHING gets a bare envelope. This proves the join
/// is by-correspondence, not a blanket attach.
#[test]
fn enrich_report_attaches_by_correspondence() {
    let term = "https://ex/prop";
    let mut counter_examples = BTreeMap::new();
    counter_examples.insert(
        MIN_COUNT.to_string(),
        fixture("bad example", Some(MIN_COUNT)),
    );
    let mut wellformed = BTreeMap::new();
    wellformed.insert(MIN_COUNT.to_string(), fixture("good example", None));
    let mut entailments = BTreeMap::new();
    entailments.insert(
        term.to_string(),
        vec![EntailmentView {
            rule: "subClassOf".to_string(),
            conclusion: "x a C".to_string(),
            premises: vec!["x a D".to_string(), "D subClassOf C".to_string()],
        }],
    );

    // A finding whose code MATCHES the counter-example and whose documented term
    // MATCHES the entailment map.
    let mut matched =
        Finding::new(Severity::Error, MIN_COUNT, "min count violated").with_documented_term(term);
    matched.add_location(Location::new(
        Some("mcp:validate_local".to_string()),
        None,
        None,
        Some("https://ex/focus".to_string()),
    ));
    // A finding whose code has NO matching fixture.
    let unmatched = Finding::new(
        Severity::Warning,
        "shacl.PatternConstraintComponent",
        "pattern mismatch",
    );

    let mut report = Report::new("mcp:validate_local");
    report.add_finding(matched);
    report.add_finding(unmatched);

    let enriched = enrich_report(&report, &counter_examples, &wellformed, &entailments);
    assert!(
        !enriched.ok,
        "an Error-severity finding makes the report not ok"
    );
    assert_eq!(enriched.findings.len(), 2);

    let m = &enriched.findings[0];
    assert_eq!(m.help_uri, help_uri_for(MIN_COUNT));
    assert_eq!(
        m.counter_example
            .as_ref()
            .and_then(|c| c.violation_code.as_deref()),
        Some(MIN_COUNT),
        "the matched finding gets the CORRESPONDING counter-example",
    );
    assert!(
        m.wellformed_exemplar.is_some(),
        "the matched finding gets a positive exemplar",
    );
    assert!(
        !m.entails.is_empty(),
        "the matched finding surfaces the documented term's entailments",
    );

    let u = &enriched.findings[1];
    assert_eq!(
        u.counter_example, None,
        "a finding whose code has no matching fixture gets NO counter-example (correspondence, not blanket-attach)",
    );
    assert!(
        u.entails.is_empty(),
        "an undocumented term surfaces no entailments"
    );
    assert_eq!(u.help_uri, help_uri_for("shacl.PatternConstraintComponent"));
}

/// A finding carrying no `finding_iri` is given a deterministic minted one, and
/// the same finding always mints the same identity.
#[test]
fn enrich_report_mints_stable_identity_when_absent() {
    let f = Finding::new(Severity::Error, MIN_COUNT, "min count violated");
    let mut report = Report::new("mcp:validate_local");
    report.add_finding(f);
    let empty = BTreeMap::new();
    let entail_empty = BTreeMap::new();

    let a = enrich_report(&report, &empty, &empty, &entail_empty);
    let b = enrich_report(&report, &empty, &empty, &entail_empty);
    let iri = a.findings[0]
        .finding_iri
        .as_deref()
        .expect("minted identity");
    assert!(
        iri.starts_with(MINTED_FINDING_BASE),
        "minted under the local finding namespace"
    );
    assert_eq!(
        a.findings[0].finding_iri, b.findings[0].finding_iri,
        "minting is deterministic",
    );
}
