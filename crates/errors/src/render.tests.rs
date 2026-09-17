// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::grade::Standpoint;
use crate::model::{DiagnosticAttribution, Finding, Location, Report, Rule, Severity};

/// F1: the typed conformance-failure class the violated law declares must reach
/// EVERY output surface, not just the struct. `code` names only the generic
/// mechanism (`shacl.MinCountConstraintComponent` is shared by every cardinality
/// gate in the ontology), so a surface that renders the code alone cannot name the
/// authored failure at all — which is the defect this field exists to close, and a
/// field nothing renders is that same defect one layer up.
#[test]
fn the_typed_failure_class_reaches_every_rendered_surface() {
    let mut finding = Finding::new(
        Severity::Error,
        "shacl.MinCountConstraintComponent",
        "SHACL constraint violated",
    )
    .with_tool("shacl")
    .with_failure_class("https://blackcatinformatics.ca/math/UntypedFreeVariable");
    finding.add_location(Location::new(
        Some("counter-examples/free-variable-untyped.ttl".to_owned()),
        None,
        None,
        Some("https://example.org/math/untypedFreeVariable".to_owned()),
    ));
    let mut report = Report::new("validate");
    report.add_finding(finding);

    const CLASS: &str = "https://blackcatinformatics.ca/math/UntypedFreeVariable";

    let json = to_json(&report).expect("JSON renders");
    assert!(
        json.contains(CLASS),
        "JSON must name the failure class: {json}"
    );
    assert!(
        json.contains("\"failure_class\""),
        "under a stable key: {json}"
    );

    let sarif = to_sarif(&report).expect("SARIF renders");
    assert!(
        sarif.contains("gmeow.failureClass") && sarif.contains(CLASS),
        "SARIF must name the failure class under its pinned property key: {sarif}"
    );

    let text = to_text(&report);
    assert!(
        text.contains(&format!("failure class: {CLASS}")),
        "the human surface must name the failure class: {text}"
    );

    let html = to_html(&report);
    assert!(
        html.contains(CLASS),
        "the HTML surface must name the failure class: {html}"
    );

    let nquads = to_gmeow_rdf(&report);
    assert!(
        nquads.contains(&format!("<{GMEOW}findingFailureClass> <{CLASS}>")),
        "the RDF projection must carry the class as an IRI-valued edge: {nquads}"
    );
}

/// A finding whose law declares no failure class leaves every surface exactly as it
/// was — the absence is honest, and no surface fabricates a class or an empty slot.
#[test]
fn a_finding_without_a_failure_class_renders_no_class_anywhere() {
    let mut finding =
        Finding::new(Severity::Error, "shacl.MinCount", "missing property").with_tool("shacl");
    finding.add_location(Location::new(Some("x.ttl".to_owned()), None, None, None));
    let mut report = Report::new("validate");
    report.add_finding(finding);
    assert!(
        !to_json(&report)
            .expect("JSON renders")
            .contains("failure_class")
    );
    assert!(
        !to_sarif(&report)
            .expect("SARIF renders")
            .contains("gmeow.failureClass")
    );
    assert!(!to_text(&report).contains("failure class:"));
    assert!(!to_html(&report).contains("failure-class"));
    assert!(!to_gmeow_rdf(&report).contains("findingFailureClass"));
}

// ── Fixtures ─────────────────────────────────────────────────────────────
//
// The renderers are pure functions of the `Report` and the fingerprint is a
// content hash (see `sarif_fingerprint_is_deterministic_and_distinct`), so
// every output is fully deterministic — the `.snap` goldens carry it verbatim
// with no redaction. One rich fixture exercises the union of structural
// features so a single whole-output snapshot per renderer subsumes the old
// field-level `assert_eq!(value["runs"][0]...)` spot-checks.

/// A multi-finding report exercising: GTS wire coordinates (quad + segment)
/// on a `.gts` bundle, a repo-relative `.ttl` with a focus-IRI logical anchor
/// plus a logical-only related `path <iri>` that folds onto the primary,
/// two attributions (sorted by role then sliceIri), a `category`
/// (yielding run-level `automationDetails.id`), and a fileless legacy warning
/// (anchored to the ontology root).
fn comprehensive_report() -> Report {
    let mut wire =
        Finding::new(Severity::Error, "shacl.MinCount", "missing property").with_tool("shacl");
    wire.add_location(
        Location::new(Some("bundle.gts".to_owned()), None, None, None)
            .with_gts_quad(42)
            .with_gts_segment(2),
    );

    let mut anchored =
        Finding::new(Severity::Error, "shacl.MinCount", "missing property").with_tool("shacl");
    anchored.add_location(Location::new(
        Some("core/ai/examples/grounded-claim.ttl".to_owned()),
        Some(12),
        Some(3),
        Some("https://blackcatinformatics.ca/gmeow/examples/ai/claim".to_owned()),
    ));
    anchored.related_locations.push(Location::new(
        None,
        None,
        None,
        Some("path https://blackcatinformatics.ca/gmeow/groundedIn".to_owned()),
    ));
    anchored.attributions.push(DiagnosticAttribution {
        slice_iri: "https://blackcatinformatics.ca/gmeow/slices/core/shapes".to_owned(),
        role: "shape-owner".to_owned(),
        evidence: Some("slices/core/shapes/shapes.ttl".to_owned()),
    });
    anchored.attributions.push(DiagnosticAttribution {
        slice_iri: "https://blackcatinformatics.ca/gmeow/slices/ext/data".to_owned(),
        role: "focus-origin".to_owned(),
        evidence: None,
    });

    let fileless = Finding::new(
        Severity::Warning,
        "validate.warning",
        "class gmeow:Analogy is missing gmeow:howToUse",
    );

    let mut report = Report::new("validate");
    report
        .metadata
        .insert("category".to_owned(), json!("ontology"));
    report.add_finding(wire);
    report.add_finding(anchored);
    report.add_finding(fileless);
    report
}

/// The exact shape a stale validation cache yields: a SHACL finding whose
/// PRIMARY location is logical-only (the focus IRI) and whose related
/// locations are the source file (physical) plus a `path <iri>` annotation
/// (logical-only). GitHub requires the primary to be physical, so the file is
/// promoted to primary and every logical entry folds onto it.
fn stale_cache_report() -> Report {
    let mut finding =
        Finding::new(Severity::Error, "shacl.MinCount", "missing property").with_tool("shacl");
    finding.add_location(Location::new(
        None,
        None,
        None,
        Some("https://blackcatinformatics.ca/gmeow/examples/ai/claim".to_owned()),
    ));
    finding.related_locations.push(Location::new(
        Some("core/ai/examples/grounded-claim.ttl".to_owned()),
        None,
        None,
        None,
    ));
    finding.related_locations.push(Location::new(
        None,
        None,
        None,
        Some("path https://blackcatinformatics.ca/gmeow/groundedIn".to_owned()),
    ));
    let mut report = Report::new("validate");
    report.add_finding(finding);
    report
}

// ── Whole-output snapshot goldens (T8) ─────────────────────────────

#[test]
fn sarif_full_snapshot() {
    let value: Value = serde_json::from_str(&to_sarif(&comprehensive_report()).unwrap()).unwrap();
    insta::assert_json_snapshot!(value);
}

#[test]
fn json_full_snapshot() {
    let value: Value = serde_json::from_str(&to_json(&comprehensive_report()).unwrap()).unwrap();
    insta::assert_json_snapshot!(value);
}

#[test]
fn gmeow_rdf_full_snapshot() {
    crate::assert_diag_snapshot!(to_gmeow_rdf(&comprehensive_report()));
}

#[test]
fn text_full_snapshot() {
    crate::assert_diag_snapshot!(to_text(&comprehensive_report()));
}

#[test]
fn html_full_snapshot() {
    crate::assert_diag_snapshot!(to_html(&comprehensive_report()));
}

#[test]
fn sarif_stale_cache_primary_promotion_snapshot() {
    let value: Value = serde_json::from_str(&to_sarif(&stale_cache_report()).unwrap()).unwrap();
    insta::assert_json_snapshot!(value);
}

#[test]
fn sarif_multi_physical_location_emits_related_locations_snapshot() {
    // A finding with two physical file locations: the first becomes the
    // primary `physicalLocation`, the rest ride as `relatedLocations`
    // (render.rs §"Remaining physical locations"). This is the only shape that
    // emits `relatedLocations`, so it pins that branch AND makes the
    // "every relatedLocation carries a physicalLocation" invariant non-vacuous.
    let mut finding =
        Finding::new(Severity::Error, "shacl.MinCount", "missing property").with_tool("shacl");
    finding.add_location(Location::new(
        Some("core/ai/examples/grounded-claim.ttl".to_owned()),
        Some(12),
        Some(3),
        None,
    ));
    finding.add_location(Location::new(
        Some("slices/core/shapes/shapes.ttl".to_owned()),
        Some(40),
        None,
        None,
    ));
    let mut report = Report::new("validate");
    report.add_finding(finding);

    let value: Value = serde_json::from_str(&to_sarif(&report).unwrap()).unwrap();

    // The emission branch fired: exactly one related location, carrying a
    // physicalLocation (the contract, here actually exercised).
    let related = value["runs"][0]["results"][0]["relatedLocations"]
        .as_array()
        .expect("relatedLocations emitted for a 2+ physical-location finding");
    assert_eq!(related.len(), 1);
    assert!(related[0].get("physicalLocation").is_some());

    insta::assert_json_snapshot!(value);
}

#[test]
fn sarif_marks_synthetic_primary_location_for_logical_only_finding() {
    let mut finding =
        Finding::new(Severity::Warning, "shacl.MinCount", "missing property").with_tool("shacl");
    finding.add_location(Location::new(
        None,
        None,
        None,
        Some("https://blackcatinformatics.ca/gmeow/example".to_owned()),
    ));
    let mut report = Report::new("validate");
    report.add_finding(finding);

    let value: Value = serde_json::from_str(&to_sarif(&report).unwrap()).unwrap();
    let location = &value["runs"][0]["results"][0]["locations"][0];

    assert_eq!(
        location["physicalLocation"]["artifactLocation"]["uri"],
        FALLBACK_ARTIFACT_URI
    );
    assert_eq!(
        location["properties"]["gmeow.syntheticPhysicalLocation"],
        true
    );
    assert_eq!(
        location["logicalLocations"][0]["fullyQualifiedName"],
        "https://blackcatinformatics.ca/gmeow/example"
    );
}

// ── Semantic invariants (properties a snapshot cannot express) ───────────

#[test]
fn sarif_emits_no_absolute_or_angle_bracket_uris() {
    // code-scanning contract, asserted as a property over the whole rich
    // report (not a single field): NO artifactLocation.uri is angle-bracketed
    // or absolute-scheme, and every emitted relatedLocation carries a
    // physicalLocation (a logical-only related location is rejected by GitHub).
    let serialized = to_sarif(&comprehensive_report()).unwrap();
    assert!(
        !serialized.contains("\"uri\": \"<"),
        "angle-bracketed URI leaked"
    );
    let value: Value = serde_json::from_str(&serialized).unwrap();
    for run in value["runs"].as_array().unwrap() {
        for kind in ["results", "artifacts"] {
            collect_uris(&run[kind]).iter().for_each(|u| {
                assert!(
                    !has_uri_scheme(u),
                    "absolute-scheme URI leaked into artifactLocation: {u}"
                );
            });
        }
        for res in run["results"].as_array().unwrap() {
            if let Some(rels) = res["relatedLocations"].as_array() {
                for rel in rels {
                    assert!(
                        rel.get("physicalLocation").is_some(),
                        "relatedLocation without physicalLocation is rejected by code-scanning"
                    );
                }
            }
        }
    }
}

#[test]
fn sarif_automation_details_omitted_without_category() {
    // The positive (category -> automationDetails.id) is locked by the SARIF
    // snapshot; here we pin the negatives a snapshot of "null" expresses
    // weakly: no metadata, empty string, and a non-string all omit the key.
    let mut report = Report::new("validate");
    report.add_finding(Finding::new(Severity::Error, "x", "boom"));
    let value: Value = serde_json::from_str(&to_sarif(&report).unwrap()).unwrap();
    assert!(value["runs"][0]["automationDetails"].is_null());

    report.metadata.insert("category".to_owned(), json!(""));
    let value: Value = serde_json::from_str(&to_sarif(&report).unwrap()).unwrap();
    assert!(value["runs"][0]["automationDetails"].is_null());

    report.metadata.insert("category".to_owned(), json!(7));
    let value: Value = serde_json::from_str(&to_sarif(&report).unwrap()).unwrap();
    assert!(value["runs"][0]["automationDetails"].is_null());
}

#[test]
fn gmeow_rdf_projects_into_the_diagnostics_graph() {
    // Every projected line lands in the diagnostics named graph, and the
    // projection is deterministic. (Specific triples are locked by the
    // purrdf snapshot; this asserts the graph-containment invariant.)
    let nquads = to_gmeow_rdf(&comprehensive_report());
    for line in nquads.lines() {
        assert!(
            line.ends_with("<https://blackcatinformatics.ca/gmeow/graph/diagnostics> ."),
            "line not in diagnostics graph: {line}"
        );
    }
    assert_eq!(nquads, to_gmeow_rdf(&comprehensive_report()));
}

#[test]
fn gmeow_rdf_emits_finding_standpoint_when_present() {
    // U1: a finding carrying a gating standpoint projects the
    // `gmeow:findingStandpoint` twin pointing at the matching gmeow:standpoint*
    // individual — the leg the logic:ruleGateFatalVerdict up-set rule (and its
    // SHACL projection) reads. A standpoint-less finding emits no such triple,
    // so existing goldens stay byte-unchanged.
    use crate::grade::Standpoint;
    let mut report = Report::new("validate");
    report.add_finding(
        Finding::new(Severity::Error, "x.binding", "binding finding")
            .with_standpoint(Standpoint::Binding),
    );
    let nquads = to_gmeow_rdf(&report);
    assert!(
        nquads.contains(
            "<https://blackcatinformatics.ca/gmeow/findingStandpoint> \
                 <https://blackcatinformatics.ca/gmeow/standpointBinding>"
        ),
        "binding-standpoint finding must project gmeow:findingStandpoint: {nquads}"
    );
    // A finding without a standpoint emits no findingStandpoint triple.
    let mut bare = Report::new("validate");
    bare.add_finding(Finding::new(Severity::Error, "x.bare", "no standpoint"));
    assert!(
        !to_gmeow_rdf(&bare).contains("findingStandpoint"),
        "standpoint-less finding must not project findingStandpoint"
    );
}

#[test]
fn gmeow_rdf_emits_grade_axes_but_never_the_derived_gate_verdict() {
    // The projection emits the three grade-axis coordinates (severity, category,
    // standpoint) the up-set rule reads, but NEVER the derived verdict itself:
    // gmeow:findingGateVerdict is materialized by the native reasoner running
    // logic:ruleGateFatalVerdict over this graph, not hand-asserted here. This keeps
    // the trust boundary honest — the shipped verdict is an entailment, and no
    // gateCollected value (which no rule derives) is invented.
    use crate::grade::Standpoint;
    use crate::model::FindingCategory;
    let mut fatal = Report::new("validate");
    fatal.add_finding(
        Finding::new(Severity::Error, "x.fatal", "up-set finding")
            .with_category(FindingCategory::DataShapeViolation)
            .with_standpoint(Standpoint::Binding),
    );
    let nq = to_gmeow_rdf(&fatal);
    // The grade coordinates the rule reads ARE projected...
    assert!(
        nq.contains("<https://blackcatinformatics.ca/gmeow/findingSeverity>")
            && nq.contains("<https://blackcatinformatics.ca/gmeow/findingStandpoint>")
            && nq.contains("<https://blackcatinformatics.ca/gmeow/findingCategory>"),
        "the three grade-axis coordinates must be projected for the reasoner: {nq}"
    );
    // ...but the DERIVED verdict is NOT hand-asserted (reasoner-derived, not projected).
    assert!(
        !nq.contains("findingGateVerdict"),
        "the projection must NOT pre-materialize the reasoner-derived gate verdict: {nq}"
    );
}

#[test]
fn gmeow_rdf_in_graph_projects_into_the_requested_graph() {
    // A non-diagnostics producer (e.g. the reasoning divergence ledger, whose
    // entries ARE restricted Findings) reuses the single emitter with its own
    // named graph; every line lands in that graph, none in graph/diagnostics.
    let graph = "https://blackcatinformatics.ca/gmeow/graph/conformance";
    let nquads = to_gmeow_rdf_in_graph(&comprehensive_report(), graph);
    assert!(!nquads.is_empty(), "report projects at least one finding");
    for line in nquads.lines() {
        assert!(
            line.ends_with(&format!("<{graph}> .")),
            "line not in the requested graph: {line}"
        );
    }
    // The bodies are identical to the diagnostics projection modulo the graph IRI.
    let diag = to_gmeow_rdf(&comprehensive_report());
    assert_eq!(
        nquads.replace(
            graph,
            "https://blackcatinformatics.ca/gmeow/graph/diagnostics"
        ),
        diag,
        "only the trailing graph IRI differs"
    );
}

#[test]
fn gmeow_rdf_escapes_literals() {
    let mut report = Report::new("validate");
    report.add_finding(Finding::new(
        Severity::Warning,
        "x",
        "quote \" and newline \n end",
    ));
    let nquads = to_gmeow_rdf(&report);
    assert!(nquads.contains("quote \\\" and newline \\n end"));
    assert!(!nquads.contains("quote \" and newline \n"));
}

#[test]
fn gmeow_rdf_escapes_c0_control_characters() {
    // A message carrying raw C0 controls (NUL, backspace, form-feed, VT)
    // must escape them as \uXXXX so the projection stays valid N-Quads.
    let mut report = Report::new("validate");
    report.add_finding(Finding::new(
        Severity::Error,
        "ctrl",
        "nul\u{0}back\u{8}ff\u{c}vt\u{b}",
    ));
    let nquads = to_gmeow_rdf(&report);
    assert!(nquads.contains("nul\\u0000back\\u0008ff\\u000Cvt\\u000B"));
    assert!(
        !nquads.chars().any(|c| (c as u32) < 0x20 && c != '\n'),
        "raw control character leaked into N-Quads output"
    );
}

#[test]
fn sarif_fingerprint_is_deterministic_and_distinct() {
    let a = Finding::new(Severity::Error, "x", "first message");
    let b = Finding::new(Severity::Error, "x", "second message");
    assert_eq!(stable_fingerprint(&a), stable_fingerprint(&a));
    assert_ne!(stable_fingerprint(&a), stable_fingerprint(&b));
}

#[test]
fn sarif_fingerprint_is_role_sensitive() {
    // Two otherwise-identical findings that differ only in attribution role
    // must produce DIFFERENT fingerprints (v2 contract).
    let make_finding = |role: &str| {
        let mut f = Finding::new(Severity::Error, "shacl.MinCount", "missing property");
        f.attributions.push(DiagnosticAttribution {
            slice_iri: "https://blackcatinformatics.ca/gmeow/slices/core/epistemics".to_owned(),
            role: role.to_owned(),
            evidence: None,
        });
        f
    };
    let fp_shape = stable_fingerprint(&make_finding("shape-owner"));
    let fp_focus = stable_fingerprint(&make_finding("focus-origin"));
    let fp_scope = stable_fingerprint(&make_finding("evaluation-scope"));
    assert_ne!(fp_shape, fp_focus);
    assert_ne!(fp_shape, fp_scope);
    assert_ne!(fp_focus, fp_scope);
}

#[test]
fn html_escapes_messages() {
    let mut report = Report::new("validate");
    report.add_finding(Finding::new(Severity::Warning, "x", "<script>"));
    let html = to_html(&report);
    assert!(html.contains("&lt;script&gt;"));
    assert!(!html.contains("<script>"));
}

#[test]
fn html_emits_one_row_per_finding() {
    // Well-formedness: the rendered table is balanced and carries exactly one
    // data row per finding plus the header row.
    let mut report = Report::new("validate");
    for i in 0..3 {
        report.add_finding(Finding::new(
            Severity::Error,
            format!("code.{i}"),
            format!("message {i}"),
        ));
    }
    let html = to_html(&report);
    assert_eq!(html.matches("<table>").count(), 1);
    assert_eq!(html.matches("</table>").count(), 1);
    let close_rows = html.matches("</tr>").count();
    assert_eq!(html.matches("<tr").count(), close_rows);
    assert_eq!(close_rows, 1 + report.findings.len());
}

/// A report with one advisory finding carrying suggestions and a rule help_uri.
/// Used to snapshot-test that `to_text` and `to_html` render these fields.
fn advisory_report() -> Report {
    let mut finding = Finding::new(
        Severity::Note,
        "advice.sample",
        "consider a more specific sortal",
    )
    .with_tool("validate");
    // Push in reverse-alphabetical order so normalize() re-sorts them, confirming
    // the renderer iterates the already-sorted slice AS-IS.
    finding
        .suggestions
        .push("use gmeow:Kind for rigid sortals".to_owned());
    finding
        .suggestions
        .push("see the modeling guide".to_owned());

    let mut rule = Rule::new("advice.sample", Severity::Note);
    rule.help_uri = Some("https://blackcatinformatics.ca/gmeow/advice#sample".to_owned());

    let mut report = Report::new("validate");
    report.add_rule(rule);
    report.add_finding(finding);
    report
}

#[test]
fn advisory_text_snapshot() {
    crate::assert_diag_snapshot!(to_text(&advisory_report()));
}

#[test]
fn to_text_advisories_renders_only_notes_and_infos() {
    // comprehensive_report() has Error + Warning findings but no Note/Info:
    // result must be empty.
    assert_eq!(
        to_text_advisories(&comprehensive_report()),
        "",
        "expected empty string for a report with no Note/Info findings"
    );

    // advisory_report() has one Note finding with suggestions and a help URI.
    let text = to_text_advisories(&advisory_report());
    assert!(
        text.contains("note advice.sample"),
        "expected 'note advice.sample' prefix line in advisory text, got: {text}"
    );
    assert!(
        text.contains("↳ suggestion:"),
        "expected suggestion lines in advisory text, got: {text}"
    );
    assert!(
        text.contains("↳ help:"),
        "expected help line in advisory text, got: {text}"
    );
    // Must NOT contain any error or warning severity prefix.
    for line in text.lines() {
        assert!(
            !line.starts_with("error ") && !line.starts_with("warning "),
            "advisory text must not contain error/warning severity lines, found: {line}"
        );
    }
}

#[test]
fn advisory_only_text_snapshot() {
    crate::assert_diag_snapshot!(to_text_advisories(&advisory_report()));
}

#[test]
fn advisory_html_snapshot() {
    crate::assert_diag_snapshot!(to_html(&advisory_report()));
}

#[test]
fn advisory_sarif_snapshot() {
    let value: Value = serde_json::from_str(&to_sarif(&advisory_report()).unwrap()).unwrap();
    insta::assert_json_snapshot!(value);
}

#[test]
fn advisory_gmeow_rdf_snapshot() {
    crate::assert_diag_snapshot!(to_gmeow_rdf(&advisory_report()));
}

#[test]
fn gmeow_rdf_escapes_suggestion_specials() {
    // A suggestion containing a double-quote and a C0 control char must be
    // escaped correctly: \" → \\\" and \u{7} → \\u0007. No raw control char
    // may survive into the N-Quads output.
    let mut finding = Finding::new(Severity::Note, "advice.escape", "escape test finding");
    finding
        .suggestions
        .push("quote \" and \u{7} bell".to_owned());
    let mut report = Report::new("validate");
    report.add_finding(finding);
    let nquads = to_gmeow_rdf(&report);
    assert!(
        nquads.contains("quote \\\" and \\u0007 bell"),
        "escaped form not found in output: {nquads}"
    );
    assert!(
        !nquads.chars().any(|c| (c as u32) < 0x20 && c != '\n'),
        "raw control character leaked into N-Quads output"
    );
}

#[test]
fn advisory_suggestions_are_properties_not_locations() {
    let sarif_str = to_sarif(&advisory_report()).unwrap();
    let value: Value = serde_json::from_str(&sarif_str).unwrap();
    let result = &value["runs"][0]["results"][0];

    // suggestions land in properties, not as locations or relatedLocations
    let suggestions = &result["properties"]["gmeow.suggestions"];
    assert!(suggestions.is_array(), "gmeow.suggestions must be an array");
    assert_eq!(
        suggestions.as_array().unwrap().len(),
        2,
        "expected 2 suggestions"
    );

    // exactly one location (the synthetic fallback for the location-less Note)
    let locations = result["locations"].as_array().unwrap();
    assert_eq!(locations.len(), 1, "expected exactly 1 location");

    // no relatedLocations key at all
    assert!(
        result.get("relatedLocations").is_none(),
        "relatedLocations must not be present"
    );

    // rule-level helpUri carried via rules array
    let rules = value["runs"][0]["tool"]["driver"]["rules"]
        .as_array()
        .unwrap();
    let advice_rule = rules
        .iter()
        .find(|r| r["id"].as_str() == Some("advice.sample"))
        .expect("advice.sample rule must be present");
    assert!(
        advice_rule.get("helpUri").is_some(),
        "advice.sample rule must carry helpUri"
    );
}

#[test]
fn summarized_text_collapses_warnings_to_per_code_counts() {
    let mut report = Report::new("gmeow-docs");
    // Two errors (rendered in full) + many warnings across two codes.
    report.add_finding(Finding::new(
        Severity::Error,
        "docs/dangling-link",
        "broken alpha",
    ));
    report.add_finding(Finding::new(
        Severity::Error,
        "docs/dangling-link",
        "broken beta",
    ));
    for i in 0..5 {
        report.add_finding(Finding::new(
            Severity::Warning,
            "docs/missing-example",
            format!("term {i}"),
        ));
    }
    for i in 0..3 {
        report.add_finding(Finding::new(
            Severity::Warning,
            "docs/missing-alignment",
            format!("aligned {i}"),
        ));
    }

    let text = to_text_summarized(&report);
    // Errors appear individually, in full.
    assert!(
        text.contains("error docs/dangling-link: broken alpha"),
        "{text}"
    );
    assert!(
        text.contains("error docs/dangling-link: broken beta"),
        "{text}"
    );
    // Warnings collapse to one count line per code — no per-term warning lines.
    assert!(
        text.contains("warning docs/missing-example: 5 finding(s)"),
        "{text}"
    );
    assert!(
        text.contains("warning docs/missing-alignment: 3 finding(s)"),
        "{text}"
    );
    assert!(
        !text.contains("term 0"),
        "individual warnings must not be listed: {text}"
    );

    // counts_by_code tallies every code (errors + warnings).
    let counts = report.counts_by_code();
    assert_eq!(counts["docs/dangling-link"], 2);
    assert_eq!(counts["docs/missing-example"], 5);
    assert_eq!(counts["docs/missing-alignment"], 3);
}

#[test]
fn category_projects_to_sarif_property_and_rdf_individual() {
    use crate::model::FindingCategory;
    let mut report = Report::new("validate");
    report.add_finding(
        Finding::new(
            Severity::Warning,
            "validate.deep.permitted-conflict",
            "disclosed glut under a glut-admitting contract",
        )
        .with_tool("validate")
        .with_category(FindingCategory::PermittedEpistemicConflict),
    );

    // SARIF: the category rides result.properties as the kebab wire value.
    let value: Value = serde_json::from_str(&to_sarif(&report).unwrap()).unwrap();
    assert_eq!(
        value["runs"][0]["results"][0]["properties"]["gmeow.category"],
        "permitted-epistemic-conflict"
    );

    // RDF: one gmeow:findingCategory triple pointing at the logic: individual.
    let nquads = to_gmeow_rdf(&report);
    assert!(
        nquads.contains(
            "<https://blackcatinformatics.ca/gmeow/findingCategory> \
                 <https://blackcatinformatics.ca/logic/FindingPermittedEpistemicConflict>"
        ),
        "findingCategory triple missing: {nquads}"
    );
}

#[test]
fn absent_category_leaves_outputs_unchanged() {
    // A finding with no category emits neither the SARIF property nor the RDF
    // triple — the byte-stability guarantee for existing goldens.
    let mut report = Report::new("validate");
    report.add_finding(Finding::new(Severity::Error, "x", "boom"));
    let sarif = to_sarif(&report).unwrap();
    assert!(!sarif.contains("gmeow.category"));
    let nquads = to_gmeow_rdf(&report);
    assert!(!nquads.contains("findingCategory"));
}

/// A parent finding and its downstream witness, both carrying the canonical
/// fingerprint IRIs a ledger projection mints, so the RDF subject/antecedent
/// join is exercised. The child's `antecedents` names the parent's own
/// `finding_iri` — the equality the declared meta-rules match on.
const PARENT_IRI: &str = "https://blackcatinformatics.ca/gmeow/diagnostics/finding/aaaa1111";
const CHILD_IRI: &str = "https://blackcatinformatics.ca/gmeow/diagnostics/finding/bbbb2222";
const ANCHOR_IRI: &str = "https://blackcatinformatics.ca/gmeow/diagnostics/anchor/cccc3333";

fn linked_report() -> Report {
    use crate::model::FindingCategory;
    let mut parent = Finding::new(Severity::Note, "diag.cause", "the root data-shape breach")
        .with_tool("validate")
        .with_category(FindingCategory::DataShapeViolation);
    parent.finding_iri = Some(PARENT_IRI.to_owned());
    parent.anchor_iri = Some(ANCHOR_IRI.to_owned());
    parent.anchor_non_trivial = true;

    let mut child = Finding::new(Severity::Error, "diag.effect", "the downstream witness")
        .with_tool("validate")
        .with_category(FindingCategory::ContradictionWitness);
    child.finding_iri = Some(CHILD_IRI.to_owned());
    child.antecedents = vec![PARENT_IRI.to_owned()];

    let mut report = Report::new("validate");
    report.add_finding(parent);
    report.add_finding(child);
    report
}

#[test]
fn rdf_subject_and_antecedent_object_close() {
    // D3: the projected diagnostic graph's subject IRI is the ledger fingerprint
    // IRI, and the child's gmeow:findingAntecedent object is textually the SAME
    // IRI the parent's subject carries — so the graph closes and the meta-rules
    // can join. The non-trivial anchor is typed gmeow:NonTrivialAnchor.
    let nquads = to_gmeow_rdf(&linked_report());
    assert!(
        nquads.contains(&format!(
            "<{PARENT_IRI}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
                 <https://blackcatinformatics.ca/gmeow/Finding>"
        )),
        "parent subject IRI must be the fingerprint IRI: {nquads}"
    );
    assert!(
        nquads.contains(&format!(
            "<{CHILD_IRI}> <https://blackcatinformatics.ca/gmeow/findingAntecedent> <{PARENT_IRI}>"
        )),
        "child antecedent edge object must equal the parent subject IRI: {nquads}"
    );
    assert!(
        nquads.contains(&format!(
            "<{PARENT_IRI}> <https://blackcatinformatics.ca/gmeow/findingAnchor> <{ANCHOR_IRI}>"
        )),
        "the anchor edge must be projected: {nquads}"
    );
    assert!(
        nquads.contains(&format!(
            "<{ANCHOR_IRI}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
                 <https://blackcatinformatics.ca/gmeow/NonTrivialAnchor>"
        )),
        "a non-trivial anchor must be typed gmeow:NonTrivialAnchor: {nquads}"
    );
}

#[test]
fn rdf_projects_finding_remediation() {
    use crate::diag::Remediation;
    let mut finding = Finding::new(Severity::Error, "diag.rem", "boom").with_tool("validate");
    finding.finding_iri = Some(CHILD_IRI.to_owned());
    finding
        .remediation
        .push(Remediation::new("attach the relator", Standpoint::Binding));
    let mut report = Report::new("validate");
    report.add_finding(finding);
    let nquads = to_gmeow_rdf(&report);
    assert!(
        nquads.contains(
            "<https://blackcatinformatics.ca/gmeow/findingRemediation> \"attach the relator\""
        ),
        "findingRemediation must be projected verbatim: {nquads}"
    );
}

#[test]
fn sarif_emits_fixes_from_remediation_and_omits_them_otherwise() {
    // D2a: a finding carrying an authored remediation renders a `fixes` array
    // whose description.text equals it (with artifactChanges from the edit); a
    // finding with no remediation renders no `fixes` key (honest absence).
    use crate::diag::{ArtifactChange, Region, Remediation};
    let mut with_fix =
        Finding::new(Severity::Error, "diag.fix", "missing mediator").with_tool("validate");
    with_fix.remediation.push(
        Remediation::new("introduce the mediating relator", Standpoint::Binding)
            .with_artifact_change(ArtifactChange {
                artifact_uri: "core/x.ttl".to_owned(),
                region: Region {
                    start_line: Some(12),
                    start_column: Some(3),
                    ..Region::default()
                },
                replacement: "gmeow:mediates ex:r .".to_owned(),
            }),
    );
    let mut report = Report::new("validate");
    report.add_finding(with_fix);
    let value: Value = serde_json::from_str(&to_sarif(&report).unwrap()).unwrap();
    let fixes = value["runs"][0]["results"][0]["fixes"]
        .as_array()
        .expect("fixes array for a finding carrying a remediation");
    assert_eq!(fixes.len(), 1);
    assert_eq!(
        fixes[0]["description"]["text"],
        "introduce the mediating relator"
    );
    let change = &fixes[0]["artifactChanges"][0];
    assert_eq!(change["artifactLocation"]["uri"], "core/x.ttl");
    assert_eq!(
        change["replacements"][0]["insertedContent"]["text"],
        "gmeow:mediates ex:r ."
    );
    assert_eq!(change["replacements"][0]["deletedRegion"]["startLine"], 12);

    // A finding with no remediation renders no `fixes` key.
    let mut bare = Report::new("validate");
    bare.add_finding(Finding::new(Severity::Error, "diag.bare", "boom"));
    let value: Value = serde_json::from_str(&to_sarif(&bare).unwrap()).unwrap();
    assert!(
        value["runs"][0]["results"][0].get("fixes").is_none(),
        "no remediation must render no fixes key"
    );
}

#[test]
fn sarif_prose_only_remediation_omits_artifact_changes() {
    // A prose-only remediation (no mechanical edit) renders a fix with a
    // description but no artifactChanges — honest absence, not an empty edit.
    use crate::diag::Remediation;
    let mut finding = Finding::new(Severity::Error, "diag.prose", "boom").with_tool("validate");
    finding.remediation.push(Remediation::new(
        "re-run the reasoner",
        Standpoint::Advisory,
    ));
    let mut report = Report::new("validate");
    report.add_finding(finding);
    let value: Value = serde_json::from_str(&to_sarif(&report).unwrap()).unwrap();
    let fix = &value["runs"][0]["results"][0]["fixes"][0];
    assert_eq!(fix["description"]["text"], "re-run the reasoner");
    assert!(
        fix.get("artifactChanges").is_none(),
        "a prose-only remediation must omit artifactChanges"
    );
}

#[test]
fn text_renders_the_witness_dag_derivation_section() {
    // D2b: a finding with a 2-level antecedent chain renders a derivation
    // section naming the antecedents, walked via the ONE shared dag::walk.
    use crate::model::FindingCategory;
    let mut root = Finding::new(Severity::Note, "diag.root", "the root cause")
        .with_tool("validate")
        .with_category(FindingCategory::DataShapeViolation);
    root.finding_iri = Some(PARENT_IRI.to_owned());

    let mid_iri = "https://blackcatinformatics.ca/gmeow/diagnostics/finding/dddd4444";
    let mut mid = Finding::new(Severity::Warning, "diag.mid", "an intermediate witness")
        .with_tool("validate");
    mid.finding_iri = Some(mid_iri.to_owned());
    mid.antecedents = vec![PARENT_IRI.to_owned()];

    let mut leaf =
        Finding::new(Severity::Error, "diag.leaf", "the surface finding").with_tool("validate");
    leaf.finding_iri = Some(CHILD_IRI.to_owned());
    leaf.antecedents = vec![mid_iri.to_owned()];

    let mut report = Report::new("validate");
    report.add_finding(root);
    report.add_finding(mid);
    report.add_finding(leaf);

    let text = to_text(&report);
    assert!(
        text.contains("derivation:"),
        "expected a derivation section: {text}"
    );
    // The 2-level chain names BOTH the immediate and the transitive antecedent.
    assert!(
        text.contains(mid_iri),
        "derivation must cite the mid antecedent: {text}"
    );
    assert!(
        text.contains(PARENT_IRI),
        "derivation must cite the transitive root antecedent: {text}"
    );
    assert!(
        text.contains("an intermediate witness") && text.contains("the root cause"),
        "derivation must name the antecedents' messages: {text}"
    );
}

#[test]
fn json_carries_the_antecedent_derivation_iris() {
    // D2b: the flat JSON explain surface carries each finding's cited antecedent
    // IRIs (the derivation is reconstructable from the antecedents fields).
    let json = to_json(&linked_report()).unwrap();
    let value: Value = serde_json::from_str(&json).unwrap();
    let child = value["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["code"] == "diag.effect")
        .expect("child finding");
    assert_eq!(child["antecedents"][0], PARENT_IRI);
}

#[test]
fn text_joins_per_term_rule_guidance_once() {
    // D2c: the renderer joins the code→governing-term rule registry ONCE and
    // reads BOTH gmeow:ruleRemediation and gmeow:howToUse for the deep surface,
    // never fabricating them when absent.
    let mut finding = Finding::new(Severity::Error, "diag.guided", "boom").with_tool("validate");
    finding.finding_iri = Some(CHILD_IRI.to_owned());
    let rule = Rule::new("diag.guided", Severity::Error)
        .with_remediation("introduce the mediating relator")
        .with_how_to_use("reference the relator via gmeow:mediates");
    let mut report = Report::new("validate");
    report.add_rule(rule);
    report.add_finding(finding);
    let text = to_text(&report);
    assert!(
        text.contains("rule remediation: introduce the mediating relator"),
        "expected ruleRemediation prose: {text}"
    );
    assert!(
        text.contains("how to use: reference the relator via gmeow:mediates"),
        "expected howToUse prose: {text}"
    );
    // A finding whose rule authors no guidance carries none (no fabrication).
    let mut bare = Finding::new(Severity::Error, "diag.unguided", "boom").with_tool("validate");
    bare.finding_iri = Some(PARENT_IRI.to_owned());
    let mut bare_report = Report::new("validate");
    bare_report.add_finding(bare);
    let bare_text = to_text(&bare_report);
    assert!(!bare_text.contains("rule remediation:"));
    assert!(!bare_text.contains("how to use:"));
}

#[test]
fn text_renders_per_term_guidance_and_derivation_citations() {
    // D2c/D2b: the three per-term Guidance modalities projected onto
    // `finding.guidance` all render, and each `finding.derived_from_quads`
    // reifier IRI renders a derivation-citation line — a SEPARATE surface from
    // the finding-fingerprint `antecedents`/`root_cause` edges, which this test
    // also asserts stay untouched by populating derived_from_quads.
    use crate::diag::{Guidance, GuidanceModality, GuidanceSource};
    let quad_iri = "https://blackcatinformatics.ca/gmeow/quad/9f2c";
    let mut finding = Finding::new(Severity::Error, "diag.justified", "boom").with_tool("validate");
    finding.finding_iri = Some(CHILD_IRI.to_owned());
    finding.push_guidance(Guidance {
        modality: GuidanceModality::HowToUse,
        source: GuidanceSource::RuleGoverningTerm,
        term_iri: "https://blackcatinformatics.ca/gmeow/mediates".to_owned(),
        text: "attach the relator via gmeow:mediates".to_owned(),
        standpoint: Standpoint::Binding,
        help_uri: None,
    });
    finding.push_guidance(Guidance {
        modality: GuidanceModality::UseWhen,
        source: GuidanceSource::DocumentedTerm,
        term_iri: "https://blackcatinformatics.ca/gmeow/Kind".to_owned(),
        text: "use gmeow:Kind for rigid sortal categories".to_owned(),
        standpoint: Standpoint::Perspectival,
        help_uri: None,
    });
    finding.push_guidance(Guidance {
        modality: GuidanceModality::AvoidWhen,
        source: GuidanceSource::DocumentedTerm,
        term_iri: "https://blackcatinformatics.ca/gmeow/Kind".to_owned(),
        text: "avoid gmeow:Kind for phase-sortals".to_owned(),
        standpoint: Standpoint::Advisory,
        help_uri: None,
    });
    finding = finding.with_derived_from_quads([quad_iri]);

    // CRITICAL namespace guard: derived_from_quads is a SEPARATE edge from the
    // finding-fingerprint antecedents/root_cause — populating it must NEVER
    // populate those fields.
    assert!(
        finding.antecedents.is_empty(),
        "derived_from_quads must not populate antecedents"
    );
    assert!(
        finding.root_cause.is_none(),
        "derived_from_quads must not populate root_cause"
    );

    let mut report = Report::new("validate");
    report.add_finding(finding);
    let text = to_text(&report);

    assert!(
        text.contains("how to use: attach the relator via gmeow:mediates"),
        "expected the HowToUse guidance line: {text}"
    );
    assert!(
        text.contains("use when: use gmeow:Kind for rigid sortal categories"),
        "expected the UseWhen guidance line: {text}"
    );
    assert!(
        text.contains("avoid when: avoid gmeow:Kind for phase-sortals"),
        "expected the AvoidWhen guidance line: {text}"
    );
    assert!(
        text.contains(&format!("derived from: {quad_iri}")),
        "expected the derivation-citation line: {text}"
    );

    // The same namespace separation must hold on the NORMALIZED report too
    // (normalize() must never fold derived_from_quads into antecedents).
    let normalized = report.normalized();
    assert!(normalized.findings[0].antecedents.is_empty());
    assert!(normalized.findings[0].root_cause.is_none());
}

#[test]
fn text_renders_reasoner_meta_findings() {
    // D3 consumer: root cause, the 'N findings share root R' cluster grouping,
    // and the cross-node glut are surfaced when present on the projected finding.
    let mut a = Finding::new(Severity::Error, "diag.a", "first").with_tool("validate");
    a.finding_iri = Some(CHILD_IRI.to_owned());
    a.root_cause = Some(PARENT_IRI.to_owned());
    a.cross_node_glut_with =
        vec!["https://blackcatinformatics.ca/gmeow/diagnostics/finding/eeee5555".to_owned()];
    let mut b = Finding::new(Severity::Error, "diag.b", "second").with_tool("validate");
    b.finding_iri =
        Some("https://blackcatinformatics.ca/gmeow/diagnostics/finding/ffff6666".to_owned());
    b.root_cause = Some(PARENT_IRI.to_owned());

    let mut report = Report::new("validate");
    report.add_finding(a);
    report.add_finding(b);
    let text = to_text(&report);
    assert!(
        text.contains(&format!("root cause: {PARENT_IRI}")),
        "expected a root-cause line: {text}"
    );
    assert!(
        text.contains(&format!("2 finding(s) share root {PARENT_IRI}")),
        "expected the 'N findings share root R' cluster grouping: {text}"
    );
    assert!(
        text.contains("cross-node glut with:"),
        "expected the cross-node glut line: {text}"
    );
}

/// Recursively collect every `"uri"` string value under a JSON node.
fn collect_uris(node: &Value) -> Vec<String> {
    let mut out = Vec::new();
    match node {
        Value::Object(map) => {
            for (k, v) in map {
                if k == "uri" {
                    if let Some(s) = v.as_str() {
                        out.push(s.to_owned());
                    }
                } else {
                    out.extend(collect_uris(v));
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|v| out.extend(collect_uris(v))),
        _ => {}
    }
    out
}
