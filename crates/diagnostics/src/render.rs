// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::model::{Finding, Location, Report, Rule};

/// Render a report as stable pretty JSON.
pub fn to_json(report: &Report) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&report.normalized())
}

/// Render a report as SARIF 2.1.0.
///
/// Beyond the basic results, this emits the pieces GitHub code-scanning needs to
/// navigate and de-duplicate findings (#654): every distinct artifact (file or
/// `.gts` bundle) referenced by a finding is listed under `runs[].artifacts`,
/// each result carries `logicalLocations` + `properties` for its GTS wire
/// coordinates, and each result carries a stable `partialFingerprints` value
/// derived from the deterministic [`Finding::sort_key`] so re-runs dedupe.
pub fn to_sarif(report: &Report) -> Result<String, serde_json::Error> {
    let normalized = report.normalized();
    let rules = sarif_rules(&normalized);
    let artifacts = sarif_artifacts(&normalized);
    let results: Vec<Value> = normalized.findings.iter().map(sarif_result).collect();
    let payload = json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": normalized.tool,
                    "informationUri": "https://github.com/Blackcat-Informatics/gmeow-ontology",
                    "rules": rules,
                }
            },
            "artifacts": artifacts,
            "results": results,
        }]
    });
    serde_json::to_string_pretty(&payload)
}

/// Strip the angle brackets oxigraph's N-Triples `Display` wraps around IRIs.
/// SARIF `artifactLocation.uri` must be a *bare* URI: GitHub code-scanning
/// rejects the whole file when a location reads `<https://…>` ("first path
/// segment in URL cannot contain colon"). RDF terms that are not IRIs (blank
/// nodes, literals) lack the brackets and pass through unchanged.
fn strip_angle(s: &str) -> &str {
    s.strip_prefix('<')
        .and_then(|inner| inner.strip_suffix('>'))
        .unwrap_or(s)
}

/// Whether a string is usable as a SARIF `artifactLocation.uri`: a syntactically
/// valid URI reference, i.e. non-empty, no embedded whitespace, and not a quoted
/// RDF literal. Composite annotations such as `path <…>` / `value "x"` fail this
/// and are surfaced as logical locations instead of corrupting the URI.
fn is_artifact_uri(candidate: &str) -> bool {
    !candidate.is_empty()
        && !candidate.starts_with('"')
        && !candidate.chars().any(char::is_whitespace)
}

/// The artifact URI a location points at: a concrete file path when present,
/// otherwise a bare-IRI logical anchor (e.g. a `.gts` segment or focus node).
/// Returns `None` when the only candidate is a non-URI annotation, so the
/// run-level `artifacts` list never carries an invalid URI.
fn artifact_uri(location: &Location) -> Option<String> {
    location
        .path
        .as_deref()
        .or(location.logical.as_deref())
        .map(strip_angle)
        .filter(|candidate| is_artifact_uri(candidate))
        .map(str::to_owned)
}

/// Collect the distinct artifacts referenced across all findings, sorted, so the
/// `.gts` bundle and every source file appear once under `runs[].artifacts`.
fn sarif_artifacts(report: &Report) -> Vec<Value> {
    let mut uris: Vec<String> = report
        .findings
        .iter()
        .flat_map(|finding| {
            finding
                .locations
                .iter()
                .chain(finding.related_locations.iter())
        })
        .filter_map(artifact_uri)
        .collect();
    uris.sort();
    uris.dedup();
    uris.into_iter()
        .map(|uri| json!({ "location": { "uri": uri } }))
        .collect()
}

/// A stable, dependency-free 64-bit FNV-1a hash, hex-encoded. Used for SARIF
/// `partialFingerprints` so GitHub code-scanning can dedupe a finding across
/// runs even as line numbers shift. Deterministic across platforms (unlike
/// `std::hash::DefaultHasher`).
fn stable_fingerprint(finding: &Finding) -> String {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let (severity, code, location, message) = finding.sort_key();
    let mut hash = FNV_OFFSET;
    for part in [severity.as_str(), code, location.as_str(), message] {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        // Field separator keeps "ab|c" distinct from "a|bc".
        hash ^= 0x1f;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{hash:016x}")
}

/// The GMEOW namespace IRI prefix.
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
/// The named graph the diagnostics projection lives in.
const DIAGNOSTICS_GRAPH: &str = "https://blackcatinformatics.ca/gmeow/graph/diagnostics";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const XSD_NNI: &str = "http://www.w3.org/2001/XMLSchema#nonNegativeInteger";

/// The `gmeow:DiagnosticSeverity` individual IRI for a severity.
fn severity_individual(severity: crate::model::Severity) -> String {
    use crate::model::Severity;
    let local = match severity {
        Severity::Error => "severityError",
        Severity::Warning => "severityWarning",
        Severity::Note => "severityNote",
        Severity::Info => "severityInfo",
    };
    format!("{GMEOW}{local}")
}

/// Escape a string literal for N-Triples/N-Quads.
fn nq_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // Any remaining C0 control character (U+0000–U+001F) is illegal raw
            // in an N-Triples/N-Quads STRING_LITERAL_QUOTE and must be escaped as
            // \uXXXX, else a finding/SHACL message carrying e.g. NUL, backspace,
            // form-feed, or VT produces a graph rdflib/pyoxigraph reject (#654).
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Project a report into the `gmeow:` RDF vocabulary as N-Quads, all in the
/// `gmeow:graph/diagnostics` named graph (#654).
///
/// Each finding becomes a `gmeow:Finding` individual carrying `gmeow:findingCode`,
/// `gmeow:findingMessage`, `gmeow:findingTool`, a `gmeow:findingSeverity`
/// pointing at the matching `gmeow:DiagnosticSeverity` individual, and one
/// `gmeow:findingLocation` blank node per location, whose GTS wire coordinates
/// are hung on it as datatype properties. This is the native in-bundle form of a
/// report — a projection of the canonical Rust model (Principle 4), SPARQL-
/// queryable beside the data it describes. N-Quads is used so the output parses
/// in any RDF tool (pyoxigraph, oxigraph) without TriG/prefix handling. Output
/// is deterministic: the report is normalized and findings are emitted in sorted
/// order with content-addressed finding IRIs.
pub fn to_gmeow_rdf(report: &Report) -> String {
    let normalized = report.normalized();
    let graph = format!("<{DIAGNOSTICS_GRAPH}>");
    let mut lines: Vec<String> = Vec::new();

    let triple = |s: &str, p: &str, o: &str, lines: &mut Vec<String>| {
        lines.push(format!("{s} <{p}> {o} {graph} ."));
    };

    for (index, finding) in normalized.findings.iter().enumerate() {
        let fingerprint = stable_fingerprint(finding);
        let subject = format!("<{GMEOW}diagnostics/finding/{fingerprint}-{index}>");
        triple(&subject, RDF_TYPE, &format!("<{GMEOW}Finding>"), &mut lines);
        triple(
            &subject,
            &format!("{GMEOW}findingSeverity"),
            &format!("<{}>", severity_individual(finding.severity)),
            &mut lines,
        );
        triple(
            &subject,
            &format!("{GMEOW}findingCode"),
            &format!("\"{}\"", nq_escape(&finding.code)),
            &mut lines,
        );
        triple(
            &subject,
            &format!("{GMEOW}findingMessage"),
            &format!("\"{}\"", nq_escape(&finding.message)),
            &mut lines,
        );
        if let Some(tool) = &finding.tool {
            triple(
                &subject,
                &format!("{GMEOW}findingTool"),
                &format!("\"{}\"", nq_escape(tool)),
                &mut lines,
            );
        }
        for (loc_index, location) in finding.locations.iter().enumerate() {
            // An IRI (not a blank node) so the findings graph round-trips
            // through GTS fold without bnode relabeling — required for the
            // feedback bundle's snapshot content id to stay stable.
            let loc_node =
                format!("<{GMEOW}diagnostics/finding/{fingerprint}-{index}/location/{loc_index}>");
            triple(
                &subject,
                &format!("{GMEOW}findingLocation"),
                &loc_node,
                &mut lines,
            );
            let int_prop = |node: &str, local: &str, value: u64, lines: &mut Vec<String>| {
                triple(
                    node,
                    &format!("{GMEOW}{local}"),
                    &format!("\"{value}\"^^<{XSD_NNI}>"),
                    lines,
                );
            };
            if let Some(v) = location.gts_term_id {
                int_prop(&loc_node, "gtsTermId", v, &mut lines);
            }
            if let Some(v) = location.gts_quad_index {
                int_prop(&loc_node, "gtsQuadIndex", v, &mut lines);
            }
            if let Some(v) = location.gts_reifier_id {
                int_prop(&loc_node, "gtsReifierId", v, &mut lines);
            }
            if let Some(v) = location.gts_frame_index {
                int_prop(&loc_node, "gtsFrameIndex", v, &mut lines);
            }
            if let Some(v) = location.gts_segment_index {
                int_prop(&loc_node, "gtsSegmentIndex", v, &mut lines);
            }
            if let Some(path) = &location.path {
                triple(
                    &loc_node,
                    &format!("{GMEOW}findingLocationPath"),
                    &format!("\"{}\"", nq_escape(path)),
                    &mut lines,
                );
            }
        }
    }
    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// Render a compact terminal-safe plain-text report.
pub fn to_text(report: &Report) -> String {
    let normalized = report.normalized();
    let mut lines = Vec::new();
    for finding in &normalized.findings {
        let mut line = format!(
            "{} {}: {}",
            finding.severity.as_str(),
            finding.code,
            finding.message
        );
        if let Some(location) = finding.primary_location() {
            line.push_str(" (");
            line.push_str(&location.display());
            line.push(')');
        }
        lines.push(line);
    }
    lines.join("\n")
}

/// Render a self-contained static HTML report.
pub fn to_html(report: &Report) -> String {
    let normalized = report.normalized();
    let mut rows = String::new();
    for finding in &normalized.findings {
        let location = finding
            .primary_location()
            .map(Location::display)
            .unwrap_or_default();
        rows.push_str("<tr>");
        rows.push_str(&format!(
            "<td><span class=\"sev sev-{}\">{}</span></td>",
            escape_attr(finding.severity.as_str()),
            escape_html(finding.severity.as_str())
        ));
        rows.push_str(&format!("<td>{}</td>", escape_html(&finding.code)));
        rows.push_str(&format!("<td>{}</td>", escape_html(&finding.message)));
        rows.push_str(&format!("<td>{}</td>", escape_html(&location)));
        rows.push_str("</tr>\n");
    }
    if rows.is_empty() {
        rows.push_str("<tr><td colspan=\"4\">No diagnostics.</td></tr>\n");
    }

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>{tool} diagnostics</title>
  <style>
    body {{ font-family: system-ui, sans-serif; margin: 2rem; color: #17202a; }}
    h1 {{ font-size: 1.5rem; margin-bottom: 0.25rem; }}
    .summary {{ color: #4b5563; margin-bottom: 1rem; }}
    table {{ border-collapse: collapse; width: 100%; }}
    th, td {{ border-bottom: 1px solid #d8dee9; padding: 0.5rem; text-align: left; vertical-align: top; }}
    th {{ background: #f3f4f6; }}
    .sev {{ border-radius: 4px; color: white; display: inline-block; font-size: 0.8rem; min-width: 4.5rem; padding: 0.2rem 0.4rem; text-align: center; }}
    .sev-error {{ background: #b42318; }}
    .sev-warning {{ background: #b54708; }}
    .sev-note, .sev-info {{ background: #175cd3; }}
  </style>
</head>
<body>
  <h1>{tool} diagnostics</h1>
  <p class="summary">{errors} error(s), {warnings} warning(s), {total} total finding(s)</p>
  <table>
    <thead><tr><th>Severity</th><th>Code</th><th>Message</th><th>Location</th></tr></thead>
    <tbody>
{rows}    </tbody>
  </table>
</body>
</html>
"#,
        tool = escape_html(&normalized.tool),
        errors = normalized.error_count(),
        warnings = normalized.warning_count(),
        total = normalized.findings.len(),
        rows = rows,
    )
}

fn sarif_rules(report: &Report) -> Vec<Value> {
    let mut by_id: BTreeMap<String, Rule> = BTreeMap::new();
    for rule in &report.rules {
        by_id.insert(rule.id.clone(), rule.clone());
    }
    for finding in &report.findings {
        by_id
            .entry(finding.code.clone())
            .or_insert_with(|| Rule::new(finding.code.clone(), finding.severity));
    }
    by_id
        .values()
        .map(|rule| {
            let mut out = json!({
                "id": rule.id,
                "defaultConfiguration": {
                    "level": rule.default_severity.sarif_level(),
                }
            });
            if let Some(title) = &rule.title {
                out["shortDescription"] = json!({ "text": title });
            }
            if let Some(description) = &rule.description {
                out["fullDescription"] = json!({ "text": description });
            }
            if let Some(help_uri) = &rule.help_uri {
                out["helpUri"] = json!(help_uri);
            }
            out
        })
        .collect()
}

fn sarif_result(finding: &Finding) -> Value {
    let mut result = json!({
        "ruleId": finding.code,
        "level": finding.severity.sarif_level(),
        "message": { "text": finding.message },
        "partialFingerprints": {
            "gmeowFindingHash/v1": stable_fingerprint(finding),
        },
    });
    let locations: Vec<Value> = finding.locations.iter().map(sarif_location).collect();
    if !locations.is_empty() {
        result["locations"] = json!(locations);
    }
    let related: Vec<Value> = finding
        .related_locations
        .iter()
        .map(sarif_location)
        .collect();
    if !related.is_empty() {
        result["relatedLocations"] = json!(related);
    }
    if let Some(detail) = &finding.detail {
        result["properties"] = json!({ "detail": detail });
    }
    result
}

/// SARIF logical locations for whichever GTS wire coordinates are present, so a
/// result resolves to a position *inside the bundle*, not just a file.
fn sarif_logical_locations(location: &Location) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let mut push = |kind: &str, value: u64| {
        let short = kind.strip_prefix("gts:").unwrap_or(kind);
        out.push(json!({
            "name": format!("{short}#{value}"),
            "kind": kind,
            "fullyQualifiedName": format!("{kind}/{value}"),
        }));
    };
    if let Some(v) = location.gts_term_id {
        push("gts:term", v);
    }
    if let Some(v) = location.gts_quad_index {
        push("gts:quad", v);
    }
    if let Some(v) = location.gts_reifier_id {
        push("gts:reifier", v);
    }
    if let Some(v) = location.gts_frame_index {
        push("gts:frame", v);
    }
    if let Some(v) = location.gts_segment_index {
        push("gts:segment", v);
    }
    out
}

/// SARIF `properties` carrying the raw GTS wire coordinates as scalars, for
/// consumers that prefer structured fields over the logical-location names.
fn sarif_location_properties(location: &Location) -> Option<Value> {
    let mut props = serde_json::Map::new();
    if let Some(v) = location.gts_term_id {
        props.insert("gts.termId".to_owned(), json!(v));
    }
    if let Some(v) = location.gts_quad_index {
        props.insert("gts.quadIndex".to_owned(), json!(v));
    }
    if let Some(v) = location.gts_reifier_id {
        props.insert("gts.reifierId".to_owned(), json!(v));
    }
    if let Some(v) = location.gts_frame_index {
        props.insert("gts.frameIndex".to_owned(), json!(v));
    }
    if let Some(v) = location.gts_segment_index {
        props.insert("gts.segmentIndex".to_owned(), json!(v));
    }
    if props.is_empty() {
        None
    } else {
        Some(Value::Object(props))
    }
}

fn sarif_location(location: &Location) -> Value {
    let mut out = json!({});

    // Physical location: a concrete file path, or a bare-IRI logical anchor.
    // Only a valid bare URI may become `artifactLocation.uri`; angle-bracketed
    // N-Triples IRIs and composite annotations are normalised / diverted below.
    let uri = artifact_uri(location);
    let has_region = location.line.is_some() || location.column.is_some();
    if uri.is_some() || has_region {
        let mut physical = json!({
            "artifactLocation": { "uri": uri.as_deref().unwrap_or("unknown") }
        });
        if has_region {
            let mut region = json!({});
            if let Some(line) = location.line {
                region["startLine"] = json!(line);
            }
            if let Some(column) = location.column {
                region["startColumn"] = json!(column);
            }
            physical["region"] = region;
        }
        out["physicalLocation"] = physical;
    }

    // Logical locations: the GTS wire coordinates, plus any non-URI annotation
    // (e.g. a SHACL result `path`/`value`) surfaced rather than dropped.
    let mut logical = sarif_logical_locations(location);
    if let Some(annotation) = location.logical.as_deref() {
        if !is_artifact_uri(strip_angle(annotation)) {
            logical.push(json!({ "fullyQualifiedName": annotation }));
        }
    }
    if !logical.is_empty() {
        out["logicalLocations"] = json!(logical);
    }

    if let Some(properties) = sarif_location_properties(location) {
        out["properties"] = properties;
    }
    out
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn escape_attr(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Finding, Location, Report, Severity};

    #[test]
    fn sarif_has_expected_version_and_result() {
        let mut finding = Finding::new(Severity::Error, "validate.missing", "missing term");
        finding.add_location(Location::new(
            Some("slices/core/example/module.ttl".to_owned()),
            Some(12),
            Some(3),
            None,
        ));
        let mut report = Report::new("validate");
        report.add_finding(finding);

        let value: Value = serde_json::from_str(&to_sarif(&report).unwrap()).unwrap();

        assert_eq!(value["version"], "2.1.0");
        assert_eq!(value["runs"][0]["results"][0]["ruleId"], "validate.missing");
        assert_eq!(
            value["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["region"]
                ["startLine"],
            12
        );
    }

    #[test]
    fn sarif_emits_wire_coords_fingerprint_and_artifacts() {
        let mut finding =
            Finding::new(Severity::Error, "shacl.MinCount", "missing property").with_tool("shacl");
        finding.add_location(
            Location::new(None, None, None, Some("gts:quad".to_owned()))
                .with_gts_quad(42)
                .with_gts_segment(2),
        );
        let mut report = Report::new("validate");
        report.add_finding(finding);

        let value: Value = serde_json::from_str(&to_sarif(&report).unwrap()).unwrap();
        let result = &value["runs"][0]["results"][0];

        // partialFingerprints is stable and present for code-scanning dedup.
        let fp = result["partialFingerprints"]["gmeowFindingHash/v1"]
            .as_str()
            .expect("fingerprint present");
        assert_eq!(fp.len(), 16);

        // The wire coordinates surface as logical locations + properties.
        let logical = &result["locations"][0]["logicalLocations"];
        assert_eq!(logical[0]["kind"], "gts:quad");
        assert_eq!(logical[0]["name"], "quad#42");
        assert_eq!(result["locations"][0]["properties"]["gts.quadIndex"], 42);
        assert_eq!(result["locations"][0]["properties"]["gts.segmentIndex"], 2);

        // The referenced artifact is listed once at the run level.
        assert_eq!(
            value["runs"][0]["artifacts"][0]["location"]["uri"],
            "gts:quad"
        );
    }

    #[test]
    fn gmeow_rdf_projects_findings_into_the_diagnostics_graph() {
        let mut finding =
            Finding::new(Severity::Error, "shacl.MinCount", "missing property").with_tool("shacl");
        finding.add_location(
            Location::new(None, None, None, Some("gts:quad".to_owned())).with_gts_quad(42),
        );
        let mut report = Report::new("validate");
        report.add_finding(finding);

        let nquads = to_gmeow_rdf(&report);

        // Every line is in the diagnostics named graph.
        for line in nquads.lines() {
            assert!(
                line.ends_with("<https://blackcatinformatics.ca/gmeow/graph/diagnostics> ."),
                "line not in diagnostics graph: {line}"
            );
        }
        assert!(nquads.contains("<https://blackcatinformatics.ca/gmeow/Finding>"));
        assert!(nquads.contains("<https://blackcatinformatics.ca/gmeow/severityError>"));
        assert!(nquads
            .contains("<https://blackcatinformatics.ca/gmeow/findingCode> \"shacl.MinCount\""));
        // The wire coordinate rides on the location node as a typed literal.
        assert!(nquads.contains(
            "<https://blackcatinformatics.ca/gmeow/gtsQuadIndex> \"42\"^^<http://www.w3.org/2001/XMLSchema#nonNegativeInteger>"
        ));
        // Deterministic: the same report projects identically.
        assert_eq!(nquads, to_gmeow_rdf(&report));
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
        // must escape them as \uXXXX so the projection stays valid N-Quads (#654).
        let mut report = Report::new("validate");
        report.add_finding(Finding::new(
            Severity::Error,
            "ctrl",
            "nul\u{0}back\u{8}ff\u{c}vt\u{b}",
        ));
        let nquads = to_gmeow_rdf(&report);

        // The literal escapes each control as the spec-mandated \uXXXX form.
        assert!(nquads.contains("nul\\u0000back\\u0008ff\\u000Cvt\\u000B"));
        // No raw C0 control byte survives anywhere in the serialization.
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
    fn html_escapes_messages() {
        let mut report = Report::new("validate");
        report.add_finding(Finding::new(Severity::Warning, "x", "<script>"));

        let html = to_html(&report);

        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>"));
    }

    #[test]
    fn sarif_uris_are_bare_and_annotations_become_logical_locations() {
        // A focus-node IRI stored bare must emit a valid `artifactLocation.uri`
        // (no angle brackets — GitHub code-scanning rejects `<https://…>`), while
        // a non-URI annotation like `path <iri>` must surface as a logical
        // location instead of corrupting the URI. Regression for #666.
        let mut finding =
            Finding::new(Severity::Error, "shacl.MinCount", "missing property").with_tool("shacl");
        finding.add_location(Location::new(
            None,
            None,
            None,
            Some("https://blackcatinformatics.ca/gmeow/examples/ai/claim".to_owned()),
        ));
        finding.related_locations.push(Location::new(
            None,
            None,
            None,
            Some("path https://blackcatinformatics.ca/gmeow/groundedIn".to_owned()),
        ));
        let mut report = Report::new("validate");
        report.add_finding(finding);

        let value: Value = serde_json::from_str(&to_sarif(&report).unwrap()).unwrap();
        let result = &value["runs"][0]["results"][0];

        // Primary location: a bare-IRI artifact URI, no angle brackets.
        assert_eq!(
            result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
            "https://blackcatinformatics.ca/gmeow/examples/ai/claim"
        );
        // The composite annotation is a logical location, not a physical URI.
        let related = &result["relatedLocations"][0];
        assert!(
            related["physicalLocation"].is_null(),
            "non-URI annotation must not become a physical artifactLocation"
        );
        assert_eq!(
            related["logicalLocations"][0]["fullyQualifiedName"],
            "path https://blackcatinformatics.ca/gmeow/groundedIn"
        );

        // No emitted URI anywhere carries an angle bracket.
        let serialized = to_sarif(&report).unwrap();
        assert!(
            !serialized.contains("\"uri\": \"<") && !serialized.contains("\"uri\":\"<"),
            "an angle-bracketed URI leaked into the SARIF output"
        );
    }
}
