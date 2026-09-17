// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn dataset(turtle: &str) -> std::sync::Arc<RdfDataset> {
    purrdf::parse_dataset(turtle.as_bytes(), "text/turtle", None).expect("turtle fixture parses")
}

const PREFIXES: &str = r#"
        @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
        @prefix logic: <https://blackcatinformatics.ca/logic/> .
        @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
    "#;

#[test]
fn term_guidance_reads_every_authored_modality() {
    let ds = dataset(&format!(
        r#"{PREFIXES}
            gmeow:requiresFrame
                gmeow:howToUse "Annotate the frame-carrying class." ;
                gmeow:useWhen "Use when a value is frame-relative." ;
                gmeow:avoidWhen "Avoid on frame-independent values." .
            "#
    ));
    let claims = term_guidance(
        &[ds.as_ref()],
        "https://blackcatinformatics.ca/gmeow/requiresFrame",
        GuidanceSource::RuleGoverningTerm,
        Some("https://example.test/catalog#frame".to_owned()),
    );
    assert_eq!(
        claims.len(),
        3,
        "all three modalities must be read: {claims:?}"
    );
    assert!(
        claims
            .iter()
            .all(|c| c.source == GuidanceSource::RuleGoverningTerm)
    );
    assert!(claims.iter().all(|c| c.standpoint == Standpoint::Advisory));
    assert!(
        claims
            .iter()
            .all(|c| c.help_uri.as_deref() == Some("https://example.test/catalog#frame"))
    );
    let modalities: Vec<_> = claims.iter().map(|c| c.modality).collect();
    assert!(modalities.contains(&GuidanceModality::HowToUse));
    assert!(modalities.contains(&GuidanceModality::UseWhen));
    assert!(modalities.contains(&GuidanceModality::AvoidWhen));
}

#[test]
fn term_guidance_is_honest_absence_for_an_undocumented_term() {
    let ds = dataset(&format!(
        r#"{PREFIXES}
            gmeow:someOtherTerm
                gmeow:howToUse "Unrelated guidance." .
            "#
    ));
    let claims = term_guidance(
        &[ds.as_ref()],
        "https://blackcatinformatics.ca/gmeow/undocumentedTerm",
        GuidanceSource::DocumentedTerm,
        None,
    );
    assert!(
        claims.is_empty(),
        "a term with no authored guidance must yield no fabricated claim: {claims:?}"
    );
}

#[test]
fn governing_terms_resolves_a_known_rule_code() {
    let ds = dataset(&format!(
        r#"{PREFIXES}
            gmeow:rule/discipline-frame-completeness
                a gmeow:ValidationRule ;
                gmeow:ruleCode "discipline/frame-completeness" ;
                logic:formalizes gmeow:requiresFrame ;
                gmeow:appliesToTerm gmeow:MeasuredQuantity .
            "#
    ));
    let mut terms = governing_terms(ds.as_ref(), "discipline/frame-completeness");
    terms.sort();
    assert_eq!(
        terms,
        vec![
            "https://blackcatinformatics.ca/gmeow/MeasuredQuantity".to_owned(),
            "https://blackcatinformatics.ca/gmeow/requiresFrame".to_owned(),
        ]
    );
}

#[test]
fn governing_terms_is_empty_for_an_unknown_code() {
    let ds = dataset(&format!(
        r#"{PREFIXES}
            gmeow:rule/discipline-frame-completeness
                a gmeow:ValidationRule ;
                gmeow:ruleCode "discipline/frame-completeness" ;
                logic:formalizes gmeow:requiresFrame .
            "#
    ));
    assert!(governing_terms(ds.as_ref(), "discipline/no-such-rule").is_empty());
}

/// Equivalence: the one-pass [`GuidanceIndex`] must produce the exact same
/// `term_guidance` claims and `governing_terms` set as the ground-truth
/// per-call scans above, over a small multi-rule fixture (two rules
/// sharing a term, one rule with no governing term, one undocumented
/// term) — this is the byte-identical-output guarantee the perf refactor
/// depends on.
#[test]
fn guidance_index_matches_the_ground_truth_scans() {
    let bundle = dataset(&format!(
        r#"{PREFIXES}
            gmeow:rule/discipline-frame-completeness
                a gmeow:ValidationRule ;
                gmeow:ruleCode "discipline/frame-completeness" ;
                logic:formalizes gmeow:requiresFrame ;
                gmeow:appliesToTerm gmeow:MeasuredQuantity .

            gmeow:rule/discipline-frame-completeness-2
                a gmeow:ValidationRule ;
                gmeow:ruleCode "discipline/frame-completeness" ;
                logic:formalizes gmeow:requiresUnit .

            gmeow:rule/no-governing-term
                a gmeow:ValidationRule ;
                gmeow:ruleCode "discipline/bare-rule" .

            gmeow:not-a-rule
                gmeow:ruleCode "discipline/frame-completeness" ;
                logic:formalizes gmeow:decoyTerm .

            gmeow:requiresFrame
                gmeow:howToUse "Annotate the frame-carrying class." ;
                gmeow:useWhen "Use when a value is frame-relative." ;
                gmeow:avoidWhen "Avoid on frame-independent values." .

            gmeow:requiresUnit
                gmeow:howToUse "Annotate the unit-carrying class." .
            "#
    ));
    let subject = dataset(&format!(
        r#"{PREFIXES}
            gmeow:requiresFrame
                gmeow:useWhen "Use when a value is frame-relative." ;
                gmeow:avoidWhen "Documented again in the subject graph." .
            "#
    ));

    let graphs: [&RdfDataset; 2] = [bundle.as_ref(), subject.as_ref()];
    let index = GuidanceIndex::build(&graphs);

    for code in [
        "discipline/frame-completeness",
        "discipline/bare-rule",
        "discipline/no-such-rule",
    ] {
        let mut expected = governing_terms(bundle.as_ref(), code);
        expected.sort();
        let mut actual = index.governing_terms(code).to_vec();
        actual.sort();
        assert_eq!(actual, expected, "governing_terms mismatch for {code}");
    }

    for term in [
        "https://blackcatinformatics.ca/gmeow/requiresFrame",
        "https://blackcatinformatics.ca/gmeow/requiresUnit",
        "https://blackcatinformatics.ca/gmeow/decoyTerm",
        "https://blackcatinformatics.ca/gmeow/undocumentedTerm",
    ] {
        let expected = term_guidance(
            &graphs,
            term,
            GuidanceSource::RuleGoverningTerm,
            Some("https://example.test/catalog#anchor".to_owned()),
        );
        let actual = index.term_guidance(
            term,
            GuidanceSource::RuleGoverningTerm,
            Some("https://example.test/catalog#anchor".to_owned()),
        );
        assert_eq!(actual, expected, "term_guidance mismatch for {term}");
    }
}
