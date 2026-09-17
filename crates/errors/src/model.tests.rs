// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn legacy_report_preserves_error_warning_strings() {
    let report = Report::from_legacy(
        "validate",
        ["missing skos:definition".to_owned()],
        ["docs.md has no anchors".to_owned()],
    );

    assert!(!report.ok());
    assert_eq!(report.legacy_errors(), ["missing skos:definition"]);
    assert_eq!(report.legacy_warnings(), ["docs.md has no anchors"]);
}

#[test]
fn report_normalization_is_deterministic() {
    let mut report = Report::new("test");
    report.add_finding(Finding::new(Severity::Warning, "z", "later"));
    report.add_finding(Finding::new(Severity::Error, "a", "first"));

    let normalized = report.normalized();

    assert_eq!(normalized.findings[0].severity, Severity::Error);
    assert_eq!(normalized.findings[0].code, "a");
}

#[test]
fn location_without_wire_coords_serializes_compactly() {
    let location = Location::new(Some("a.ttl".to_owned()), Some(3), None, None);
    let json = serde_json::to_string(&location).expect("serialize");
    // skip_serializing_if keeps absent wire coords out of the wire form.
    assert!(!json.contains("gts_"), "unexpected wire keys: {json}");
    let round: Location = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(round, location);
}

#[test]
fn location_wire_coords_round_trip_and_display() {
    let location = Location::default()
        .with_gts_segment(2)
        .with_gts_quad(42)
        .with_gts_term(7);

    let json = serde_json::to_string(&location).expect("serialize");
    let round: Location = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(round, location);
    assert_eq!(round.gts_quad_index, Some(42));

    // display() participates in Finding::sort_key, so the coords must render
    // deterministically in declared order (term, quad, reifier, frame, segment).
    assert_eq!(location.display(), "<unknown> term#7 quad#42 segment#2");
    assert!(!location.is_empty());
}

#[test]
fn empty_location_stays_empty_with_wire_fields() {
    assert!(Location::default().is_empty());
}

#[test]
fn finding_category_wire_values_round_trip() {
    // Iterate the closed `ALL` set so a newly-minted category cannot escape the
    // serde-rename == as_str == parse() round-trip invariant.
    for category in FindingCategory::ALL {
        // serde rename == as_str == the kebab wire value parse() accepts.
        let json = serde_json::to_string(&category).expect("serialize");
        assert_eq!(json, format!("\"{}\"", category.as_str()));
        assert_eq!(FindingCategory::parse(category.as_str()).unwrap(), category);
        let round: FindingCategory = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(round, category);
    }
    assert!(FindingCategory::parse("not-a-category").is_err());
}

#[test]
fn finding_without_category_serializes_compactly() {
    // skip_serializing_if keeps an absent category out of the wire form, so
    // existing JSON/SARIF/RDF goldens are byte-unchanged.
    let finding = Finding::new(Severity::Error, "x.code", "msg");
    assert_eq!(finding.category, None);
    let json = serde_json::to_string(&finding).expect("serialize");
    assert!(
        !json.contains("category"),
        "unexpected category key: {json}"
    );
}

#[test]
fn with_category_attaches_and_round_trips() {
    let finding = Finding::new(
        Severity::Warning,
        "validate.deep.permitted-conflict",
        "glut",
    )
    .with_category(FindingCategory::PermittedEpistemicConflict);
    let json = serde_json::to_string(&finding).expect("serialize");
    assert!(json.contains("\"category\":\"permitted-epistemic-conflict\""));
    let round: Finding = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(
        round.category,
        Some(FindingCategory::PermittedEpistemicConflict)
    );
}

#[test]
fn category_is_not_an_ordering_axis() {
    // Two findings identical but for category must compare equal under
    // sort_key — the taxonomy never perturbs report ordering.
    let bare = Finding::new(Severity::Error, "c", "m");
    let tagged = Finding::new(Severity::Error, "c", "m")
        .with_category(FindingCategory::ContradictionWitness);
    assert_eq!(bare.sort_key(), tagged.sort_key());
}

#[test]
fn normalize_orders_and_dedups_guidance() {
    use crate::diag::{GuidanceModality, GuidanceSource};
    use crate::grade::Standpoint;

    let claim = |modality, term: &str, text: &str, source| Guidance {
        modality,
        source,
        term_iri: term.to_owned(),
        text: text.to_owned(),
        standpoint: Standpoint::Advisory,
        help_uri: None,
    };

    // Guidance can reach a finding via the ledger merge in arrival order, so
    // build it deliberately unsorted, with one duplicate that differs only in the
    // non-identity fields (`source`/`help_uri`) — it must collapse to one.
    let mut finding = Finding::new(Severity::Warning, "c", "m")
        .with_guidance(claim(
            GuidanceModality::AvoidWhen,
            "gmeow:B",
            "avoid B",
            GuidanceSource::DocumentedTerm,
        ))
        .with_guidance(claim(
            GuidanceModality::HowToUse,
            "gmeow:A",
            "use A",
            GuidanceSource::DocumentedTerm,
        ));
    // A second surfacing of the how-to-use claim from the rule-governing key,
    // carrying a help URI — same `(modality, term_iri, text)`, so a duplicate.
    finding.push_guidance(Guidance {
        help_uri: Some("https://example/anchor".to_owned()),
        source: GuidanceSource::RuleGoverningTerm,
        ..claim(
            GuidanceModality::HowToUse,
            "gmeow:A",
            "use A",
            GuidanceSource::DocumentedTerm,
        )
    });

    finding.normalize();

    // Sorted on `(modality as u8, term_iri, text)`: HowToUse(0) before
    // AvoidWhen(2), and the duplicate how-to-use claim collapsed to one.
    assert_eq!(finding.guidance.len(), 2);
    assert_eq!(finding.guidance[0].modality, GuidanceModality::HowToUse);
    assert_eq!(finding.guidance[0].term_iri, "gmeow:A");
    assert_eq!(finding.guidance[1].modality, GuidanceModality::AvoidWhen);
    assert_eq!(finding.guidance[1].term_iri, "gmeow:B");

    // Idempotent: a second normalize is a no-op — the canonical form is stable.
    let before = finding.guidance.clone();
    finding.normalize();
    assert_eq!(finding.guidance, before);
}
