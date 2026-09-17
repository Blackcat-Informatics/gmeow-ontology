// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_errors::{DiagLedger, StageId};

/// Project the advisory diagnostic wing through a ledger to the wire
/// [`gmeow_errors::Finding`] the renderers consume — the real intern path.
fn project_diag_finding(diag: Diag) -> gmeow_errors::Finding {
    let mut ledger = DiagLedger::new();
    ledger.attach(diag, StageId::new("validate.advisory"));
    ledger
        .findings("validate")
        .pop()
        .expect("exactly one advisory finding")
}

/// Both projection wings are produced in one call and carry the expected
/// field values — the core dual-projection-always contract.
#[test]
fn project_yields_both_graded_diag_and_claim() {
    let advisory = Advisory::note("advice.sample", "consider a more specific sortal")
        .with_suggestion("use gmeow:Kind")
        .with_help_uri("https://example.org/docs/advice#sample");

    let AdvisoryProjection { diag, claim } = advisory.project();

    // ── graded diagnostic assertions ─────────────────────────────────────
    assert_eq!(diag.grade().severity, Severity::Note);
    assert_eq!(diag.grade().category, FindingCategory::PolicyWarning);
    assert_eq!(diag.grade().standpoint, Standpoint::Advisory);
    assert_eq!(diag.message(), "consider a more specific sortal");

    let finding = project_diag_finding(diag);
    assert_eq!(finding.severity, Severity::Note);
    assert_eq!(finding.code, "advice.sample");
    assert_eq!(finding.standpoint, Some(Standpoint::Advisory));
    assert_eq!(finding.category, Some(FindingCategory::PolicyWarning));
    assert_eq!(finding.tool, Some("validate".to_owned()));
    assert_eq!(finding.suggestions, vec!["use gmeow:Kind".to_owned()]);
    assert_eq!(finding.message, "consider a more specific sortal");

    // ── claim hook assertions ────────────────────────────────────────────
    assert_eq!(claim.code, "advice.sample");
    assert_eq!(claim.standpoint_iri, BEST_PRACTICE_STANDPOINT_IRI);
    assert_eq!(claim.modality_iri, DEONTIC_RECOMMENDATION_IRI);
    assert_eq!(claim.advised_proposition, "consider a more specific sortal");
}

/// The soft Rule carries the correct default_severity and help_uri so
/// SARIF/text/HTML renderers surface the documentation link.
#[test]
fn rule_carries_help_and_note_default() {
    let advisory = Advisory::note("advice.sample", "consider a more specific sortal")
        .with_help_uri("https://example.org/docs/advice#sample");

    let rule = advisory.rule();

    assert_eq!(rule.default_severity, Severity::Note);
    assert_eq!(
        rule.help_uri,
        Some("https://example.org/docs/advice#sample".to_owned())
    );
}

/// `project()` always returns exactly ONE diagnostic and ONE claim — the 1:1
/// structural invariant of the dual-projection-always contract.
#[test]
fn one_advisory_one_claim() {
    let advisory = Advisory::note("advice.sanity", "sanity check advisory");
    let projection = advisory.project();

    // Destructure to confirm both wings exist (would not compile otherwise).
    let AdvisoryProjection { diag, claim } = projection;

    // The codes must agree — the diagnostic and claim refer to the same rule.
    let finding = project_diag_finding(diag);
    assert_eq!(finding.code, claim.code);
}

/// `Advisory::note` seeds the new (D4) fields to their documented defaults.
#[test]
fn note_seeds_default_confidence_verdict_and_no_subject() {
    let advisory = Advisory::note("advice.defaults", "defaults check");
    assert_eq!(advisory.confidence, ADVISORY_DEFAULT_CONFIDENCE);
    assert_eq!(advisory.verdict_iri, VERDICT_NOT_HELD_IRI);
    assert_eq!(advisory.subject_iri, None);

    let claim = advisory.project().claim;
    assert_eq!(claim.confidence, ADVISORY_DEFAULT_CONFIDENCE);
    assert_eq!(claim.verdict_iri, VERDICT_NOT_HELD_IRI);
    assert_eq!(claim.subject_iri, None);
}

// ── Advisory bridge: data-matched Info constraints → Note advisories ─────

fn result(severity: ShaclSeverity, shape: &str, focus: &str, message: &str) -> ValidationResult {
    use purrdf::shapes::term::{NamedNode, Term};
    ValidationResult {
        focus_node: Term::NamedNode(NamedNode::new_unchecked(focus)),
        result_path: None,
        path_structure: None,
        value: None,
        source_constraint_component: NamedNode::new_unchecked(
            "http://www.w3.org/ns/shacl#SPARQLConstraintComponent",
        ),
        source_shape: Term::NamedNode(NamedNode::new_unchecked(shape)),
        severity,
        message: Some(message.to_owned()),
        source_box_roles: Vec::new(),
        path_box_roles: Vec::new(),
        result_box_roles: Vec::new(),
        attributions: Vec::new(),
    }
}

/// An `Info`-severity result (from an advisory constraint) is lifted into a Note
/// advisory carrying the focus node as subject and the shape's `logic:formalizes`
/// provenance, its raw `shacl.*` finding SUPPRESSED; a `Violation` result is retained
/// for the hard diagnostics report. The advisory fires from the DATA MATCH.
#[test]
fn split_advisory_lifts_info_results_and_retains_violations() {
    let shapes = purrdf::parse_dataset(
        b"@prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
              @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
              <https://ex/advShape> logic:formalizes gmeow:Entity .\n",
        "text/turtle",
        None,
    )
    .expect("shapes parse");
    // The ontology carries the formalized term's positive prose: howToUse → the advisory's
    // corrective suggestion, useWhen → contextual guidance (D3 acceptance criteria).
    let ontology = purrdf::parse_dataset(
            b"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
              gmeow:Entity gmeow:howToUse \"Type each instance with its most specific sortal.\"@x-gmeow-english ;\n\
                gmeow:useWhen \"Use for a genuinely category-neutral resource.\"@x-gmeow-english .\n",
            "text/turtle",
            None,
        )
        .expect("ontology parse");
    let report = ValidationReport {
        conforms: false,
        results: vec![
            result(
                ShaclSeverity::Info,
                "https://ex/advShape",
                "https://data/thing",
                "prefer a more specific sortal than bare gmeow:Entity",
            ),
            result(
                ShaclSeverity::Violation,
                "https://ex/hardShape",
                "https://data/bad",
                "a required value is missing",
            ),
        ],
    };

    let (retained, advisories) = split_advisory_results(report, &shapes, &ontology);

    // The hard violation is retained; the Info result was suppressed.
    assert_eq!(retained.results.len(), 1);
    assert_eq!(retained.results[0].severity, ShaclSeverity::Violation);

    // Exactly one Note advisory, subject = the matched focus node, provenance tag.
    assert_eq!(advisories.len(), 1);
    let advisory = &advisories[0];
    assert_eq!(advisory.severity, Severity::Note);
    assert!(advisory.code.starts_with(crate::codes::ADVICE_FAMILY));
    // The CLI advisory finding's help URI resolves through the SAME single anchor
    // authority the MCP `advise` tool uses (`catalog_anchor_uri`), landing on the
    // rendered docs Advice section — never a dead standalone advice URL.
    assert_eq!(
        advisory.help_uri.as_deref(),
        Some(crate::rule_catalog::catalog_anchor_uri(&advisory.code).as_str())
    );
    assert!(
        advisory
            .help_uri
            .as_deref()
            .unwrap()
            .ends_with("docs/enforced-constraints#advice-"),
        "advisory help_uri must resolve to the rendered docs Advice-section anchor: {:?}",
        advisory.help_uri
    );
    assert_eq!(advisory.subject_iri.as_deref(), Some("https://data/thing"));
    // The focus node is ALSO location-bearing (not just the RDF subject_iri wing), so
    // the projected Diag carries a `logical` location every surface (CLI/SARIF/JSON/MCP)
    // resolves the tripped node from.
    assert_eq!(
        advisory.locations.iter().find_map(|l| l.logical.as_deref()),
        Some("https://data/thing"),
        "advisory must attach a Location whose logical is the focus node: {:?}",
        advisory.locations
    );
    assert_eq!(
        advisory.message,
        "prefer a more specific sortal than bare gmeow:Entity"
    );
    assert!(advisory.tags.iter().any(|t| t == "advisory-harvested"));
    assert!(
        advisory
            .tags
            .iter()
            .any(|t| t == "formalizes:https://blackcatinformatics.ca/gmeow/Entity"),
        "the advisory carries its constraint's logic:formalizes provenance: {:?}",
        advisory.tags
    );
    // howToUse populates the suggestions verbatim; useWhen is surfaced as guidance.
    assert!(
        advisory
            .suggestions
            .iter()
            .any(|s| s == "Type each instance with its most specific sortal."),
        "gmeow:howToUse must populate the advisory's suggestions: {:?}",
        advisory.suggestions
    );
    assert!(
        advisory
            .suggestions
            .iter()
            .any(|s| s == "Use when: Use for a genuinely category-neutral resource."),
        "gmeow:useWhen must surface as contextual guidance: {:?}",
        advisory.suggestions
    );

    // Its claim wing carries the deonticRecommendation modality and the subject.
    let claim = advisory.project().claim;
    assert_eq!(claim.modality_iri, DEONTIC_RECOMMENDATION_IRI);
    assert_eq!(claim.subject_iri.as_deref(), Some("https://data/thing"));
}

/// Two matches of the SAME advisory constraint at DIFFERENT focus nodes get distinct
/// injective codes (so the claim emitter never sees a duplicate) and both project.
#[test]
fn split_advisory_distinct_foci_get_distinct_codes() {
    let shapes = purrdf::parse_dataset(
            b"<https://ex/advShape> <https://blackcatinformatics.ca/logic/formalizes> <https://blackcatinformatics.ca/gmeow/Entity> .\n",
            "application/n-triples",
            None,
        )
        .expect("shapes parse");
    let report = ValidationReport {
        conforms: false,
        results: vec![
            result(
                ShaclSeverity::Info,
                "https://ex/advShape",
                "https://data/a",
                "advice",
            ),
            result(
                ShaclSeverity::Info,
                "https://ex/advShape",
                "https://data/b",
                "advice",
            ),
        ],
    };
    let (_retained, advisories) = split_advisory_results(report, &shapes, &shapes);
    assert_eq!(advisories.len(), 2);
    assert_ne!(
        advisories[0].code, advisories[1].code,
        "distinct foci → distinct codes"
    );
    // No duplicate-code panic when both distinct-focus claims project to N-Quads.
    let claims: Vec<AdvisoryClaim> = advisories.iter().map(|a| a.project().claim).collect();
    let _ = project_compliance_assessment(&claims, "https://ex/graph");
}

/// The SAME advisory constraint matching the SAME focus more than once (a shape present twice
/// in the shape union, or duplicate SPARQL solution rows) is ONE advice: the duplicate
/// `advice.<shape>.<focus-digest>` results collapse to a single advisory, so the claim emitter
/// never sees a duplicate code (which would hard-fail `project_compliance_assessment`).
#[test]
fn split_advisory_dedups_duplicate_shape_focus_matches() {
    let shapes = purrdf::parse_dataset(
            b"<https://ex/advShape> <https://blackcatinformatics.ca/logic/formalizes> <https://blackcatinformatics.ca/gmeow/Entity> .\n",
            "application/n-triples",
            None,
        )
        .expect("shapes parse");
    let report = ValidationReport {
        conforms: false,
        results: vec![
            result(
                ShaclSeverity::Info,
                "https://ex/advShape",
                "https://data/dup",
                "advice",
            ),
            result(
                ShaclSeverity::Info,
                "https://ex/advShape",
                "https://data/dup",
                "advice",
            ),
        ],
    };
    let (_retained, advisories) = split_advisory_results(report, &shapes, &shapes);
    assert_eq!(
        advisories.len(),
        1,
        "duplicate (shape, focus) Info matches must collapse to one advisory: {advisories:?}"
    );
    // And the collapsed set projects to N-Quads with no duplicate-code panic.
    let claims: Vec<AdvisoryClaim> = advisories.iter().map(|a| a.project().claim).collect();
    let _ = project_compliance_assessment(&claims, "https://ex/graph");
}

// ── (D4) project_compliance_assessment ──────────────────────────────────

const DEMO_GRAPH: &str = "https://blackcatinformatics.ca/gmeow/graph/diagnostics";

/// The demonstrator-style claim: code `advice.sample.demo`, all defaults.
fn demo_claim() -> AdvisoryClaim {
    Advisory::note(
        "advice.sample.demo",
        "prefer the active-voice recommendation phrasing",
    )
    .project()
    .claim
}

/// The emitter's output parses cleanly as N-Quads.
#[test]
fn emitter_output_parses_as_nquads() {
    let nquads = project_compliance_assessment(&[demo_claim()], DEMO_GRAPH);
    purrdf::parse_dataset(nquads.as_bytes(), "application/n-quads", None)
        .expect("emitted ComplianceAssessment N-Quads must parse cleanly");
}

/// The demonstrator claim's full expected triple shape: exactly one
/// verdict/vantage/confidence, present event/norm links, the norm's
/// deontic/issuer/partOf triples, and the event's temporal frame with NO
/// eventTime — the exact contract this module specifies.
#[test]
fn demonstrator_claim_emits_the_full_expected_shape() {
    let claim = demo_claim();
    let nquads = project_compliance_assessment(std::slice::from_ref(&claim), DEMO_GRAPH);
    purrdf::parse_dataset(nquads.as_bytes(), "application/n-quads", None)
        .expect("must parse cleanly");

    let norm = format!("<{NORM_CLAIMS_BASE_IRI}{}/norm>", claim.code);
    let event = format!("<{NORM_CLAIMS_BASE_IRI}{}/event>", claim.code);
    let assessment = format!("<{NORM_CLAIMS_BASE_IRI}{}/assessment>", claim.code);

    // Exactly one complianceVerdict, pointing at verdictNotHeld.
    let verdict_line = format!(
        "{assessment} <{GMEOW}complianceVerdict> <{VERDICT_NOT_HELD_IRI}> <{DEMO_GRAPH}> ."
    );
    assert_eq!(
        nquads.matches(&verdict_line).count(),
        1,
        "expected exactly one complianceVerdict triple:\n{nquads}"
    );

    // Exactly one vantage, pointing at the real gmeowBestPractice standpoint.
    let vantage_line =
        format!("{assessment} <{GMEOW}vantage> <{BEST_PRACTICE_STANDPOINT_IRI}> <{DEMO_GRAPH}> .");
    assert_eq!(
        nquads.matches(&vantage_line).count(),
        1,
        "expected exactly one vantage triple:\n{nquads}"
    );

    // assessedEvent / assessedNorm present.
    assert!(nquads.contains(&format!(
        "{assessment} <{GMEOW}assessedEvent> {event} <{DEMO_GRAPH}> ."
    )));
    assert!(nquads.contains(&format!(
        "{assessment} <{GMEOW}assessedNorm> {norm} <{DEMO_GRAPH}> ."
    )));

    // Exactly one confidence literal, lexical form "1.0", datatype xsd:decimal.
    let confidence_line =
        format!("{assessment} <{GMEOW}confidence> \"1.0\"^^<{XSD_DECIMAL}> <{DEMO_GRAPH}> .");
    assert_eq!(
        nquads.matches(&confidence_line).count(),
        1,
        "expected exactly one confidence triple with lexical form \"1.0\":\n{nquads}"
    );

    // The norm is typed gmeow:Norm and carries deonticModality / normIssuer / partOf.
    assert!(nquads.contains(&format!(
        "{norm} <{RDF_TYPE}> <{GMEOW}Norm> <{DEMO_GRAPH}> ."
    )));
    assert!(nquads.contains(&format!(
        "{norm} <{GMEOW}deonticModality> <{DEONTIC_RECOMMENDATION_IRI}> <{DEMO_GRAPH}> ."
    )));
    assert!(nquads.contains(&format!(
        "{norm} <{GMEOW}normIssuer> <{BEST_PRACTICE_STANDPOINT_IRI}> <{DEMO_GRAPH}> ."
    )));
    assert!(nquads.contains(&format!(
        "{norm} <{GMEOW}partOf> <{BEST_PRACTICE_NORMATIVE_SYSTEM_IRI}> <{DEMO_GRAPH}> ."
    )));

    // The event carries eventTemporalFrame + an eventType (satisfying the "type or
    // temporal placement" modeling shape) and NO eventTime (deterministic output).
    assert!(nquads.contains(&format!(
        "{event} <{GMEOW}eventTemporalFrame> <{EVENT_TEMPORAL_FRAME_IRI}> <{DEMO_GRAPH}> ."
    )));
    assert!(nquads.contains(&format!(
        "{event} <{GMEOW}eventType> <{EVENT_TYPE_IRI}> <{DEMO_GRAPH}> ."
    )));
    assert!(
        !nquads.contains("eventTime"),
        "advisory event must carry NO eventTime (deterministic output):\n{nquads}"
    );
    assert!(nquads.contains(&format!(
        "{event} <{RDF_TYPE}> <{GMEOW}Event> <{DEMO_GRAPH}> ."
    )));

    // The assessment IRI embeds the code, and is typed gmeow:ComplianceAssessment.
    assert!(assessment.contains(&claim.code));
    assert!(nquads.contains(&format!(
        "{assessment} <{RDF_TYPE}> <{GMEOW}ComplianceAssessment> <{DEMO_GRAPH}> ."
    )));
}

/// `with_verdict_iri` changes ONLY the verdict triple — a pure-function
/// proof: every other emitted line is identical between the default and
/// overridden projections.
#[test]
fn with_verdict_iri_changes_only_the_verdict_triple() {
    let base_claim = demo_claim();
    let mut overridden_claim = base_claim.clone();
    overridden_claim.verdict_iri = "https://example.org/verdict/held".to_owned();

    let base_nquads = project_compliance_assessment(&[base_claim], DEMO_GRAPH);
    let overridden_nquads = project_compliance_assessment(&[overridden_claim], DEMO_GRAPH);

    let base_lines: Vec<&str> = base_nquads.lines().collect();
    let overridden_lines: Vec<&str> = overridden_nquads.lines().collect();
    assert_eq!(base_lines.len(), overridden_lines.len());

    let mut differing = 0usize;
    for (a, b) in base_lines.iter().zip(overridden_lines.iter()) {
        if a != b {
            differing += 1;
            assert!(
                a.contains("complianceVerdict") && b.contains("complianceVerdict"),
                "the only differing line must be the complianceVerdict triple: {a:?} vs {b:?}"
            );
        }
    }
    assert_eq!(differing, 1, "exactly one line must differ");
}

/// `with_subject_iri` adds exactly one `observedFeature` triple, changing
/// nothing else.
#[test]
fn with_subject_iri_adds_exactly_one_observed_feature_triple() {
    let base_claim = demo_claim();
    let mut subject_claim = base_claim.clone();
    subject_claim.subject_iri = Some("https://blackcatinformatics.ca/gmeow/SomeTerm".to_owned());

    let base_nquads = project_compliance_assessment(&[base_claim], DEMO_GRAPH);
    let subject_nquads = project_compliance_assessment(&[subject_claim], DEMO_GRAPH);

    assert!(!base_nquads.contains("observedFeature"));
    assert_eq!(subject_nquads.matches("observedFeature").count(), 1);

    let base_lines: std::collections::BTreeSet<&str> = base_nquads.lines().collect();
    let extra_lines: Vec<&str> = subject_nquads
        .lines()
        .filter(|line| !base_lines.contains(line))
        .collect();
    assert_eq!(
        extra_lines.len(),
        1,
        "exactly one new line: {extra_lines:?}"
    );
    assert!(extra_lines[0].contains("observedFeature"));
    assert!(extra_lines[0].contains("<https://blackcatinformatics.ca/gmeow/SomeTerm>"));
}

/// Determinism: two calls on the same claims produce byte-identical
/// strings, and a 2-claim input is sorted by `code`.
#[test]
fn emitter_is_deterministic_and_sorts_by_code() {
    let claim_z = Advisory::note("advice.z.later", "z advisory")
        .project()
        .claim;
    let claim_a = Advisory::note("advice.a.first", "a advisory")
        .project()
        .claim;

    let claims = [claim_z.clone(), claim_a.clone()];
    let first = project_compliance_assessment(&claims, DEMO_GRAPH);
    let second = project_compliance_assessment(&claims, DEMO_GRAPH);
    assert_eq!(first, second, "emitter must be byte-deterministic");

    let a_pos = first
        .find("advice.a.first")
        .expect("advice.a.first present");
    let z_pos = first
        .find("advice.z.later")
        .expect("advice.z.later present");
    assert!(
        a_pos < z_pos,
        "claims must be sorted by code (a before z):\n{first}"
    );
}

/// An out-of-range confidence is a HARD FAIL: the emitter panics rather
/// than silently clamp or ship a meaningless literal.
#[test]
#[should_panic(expected = "confidence out of range")]
fn out_of_range_confidence_hard_fails() {
    let claim = Advisory::note("advice.sample.demo", "prefer the active-voice phrasing")
        .with_confidence(1.5)
        .project()
        .claim;
    let _ = project_compliance_assessment(&[claim], DEMO_GRAPH);
}

/// A code carrying an IRI-unsafe character is a HARD FAIL: it would be
/// interpolated verbatim into the content-addressed IRIs and mint an
/// invalid N-Quad, so the emitter rejects it rather than ship malformed RDF.
#[test]
#[should_panic(expected = "is not IRI-safe")]
fn non_iri_safe_code_hard_fails() {
    let claim = Advisory::note("advice tier active", "a code with a space")
        .project()
        .claim;
    let _ = project_compliance_assessment(&[claim], DEMO_GRAPH);
}

/// Two claims sharing a code is a HARD FAIL: the code keys all three IRIs,
/// so a collision would emit conflicting triples on functional properties.
#[test]
#[should_panic(expected = "duplicate code")]
fn duplicate_code_hard_fails() {
    let a = Advisory::note("advice.sample.demo", "first ruling")
        .project()
        .claim;
    let b = Advisory::note("advice.sample.demo", "second, conflicting ruling")
        .with_confidence(0.25)
        .project()
        .claim;
    let _ = project_compliance_assessment(&[a, b], DEMO_GRAPH);
}
