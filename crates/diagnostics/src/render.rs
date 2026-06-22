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
///
/// When the report carries a `category` metadata key (set by the Python
/// diagnostics-output config, #662), the run emits run-level
/// `automationDetails.id` — the stable grouping key GitHub code-scanning keys
/// per-category SARIF uploads on. Absent the key, no `automationDetails` is
/// emitted (so existing single-category uploads are unchanged).
pub fn to_sarif(report: &Report) -> Result<String, serde_json::Error> {
    let normalized = report.normalized();
    let rules = sarif_rules(&normalized);
    let artifacts = sarif_artifacts(&normalized);
    let results: Vec<Value> = normalized.findings.iter().map(sarif_result).collect();
    let mut run = json!({
        "tool": {
            "driver": {
                "name": normalized.tool,
                "informationUri": "https://github.com/Blackcat-Informatics/gmeow-ontology",
                "rules": rules,
            }
        },
        "artifacts": artifacts,
        "results": results,
    });
    if let Some(category) = sarif_category(&normalized) {
        run["automationDetails"] = json!({ "id": category });
    }
    let payload = json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [run],
    });
    serde_json::to_string_pretty(&payload)
}

/// The stable code-scanning category for this report, if set: the `category`
/// metadata value when it is a non-empty JSON string. Any other shape (absent,
/// null, non-string, empty) yields `None` so the run omits `automationDetails`.
fn sarif_category(report: &Report) -> Option<&str> {
    report
        .metadata
        .get("category")
        .and_then(Value::as_str)
        .filter(|category| !category.is_empty())
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

/// Whether a string holds a URI *scheme* (`https:`, `gts:`, …) per RFC 3986
/// (`ALPHA *( ALPHA / DIGIT / "+" / "-" / "." ) ":"`). A repo-relative path
/// (`core/x.ttl`) has none.
fn has_uri_scheme(candidate: &str) -> bool {
    let bytes = candidate.as_bytes();
    if bytes.first().is_none_or(|b| !b.is_ascii_alphabetic()) {
        return false;
    }
    for (i, &b) in bytes.iter().enumerate() {
        if b == b':' {
            return i > 0;
        }
        if !(b.is_ascii_alphanumeric() || b == b'+' || b == b'-' || b == b'.') {
            return false;
        }
    }
    false
}

/// Whether a string is usable as a SARIF `artifactLocation.uri`: a **repo-relative**
/// reference — non-empty, no embedded whitespace, not a quoted RDF literal, and
/// carrying no URI scheme. GitHub code-scanning requires artifact URIs to match
/// the checkout's `file` scheme, so an absolute ontology IRI (`https://…`) is a
/// *logical* location, not a physical artifact; composite annotations such as
/// `path <…>` / `value "x"` likewise fail this and surface as logical locations.
fn is_artifact_uri(candidate: &str) -> bool {
    !candidate.is_empty()
        && !candidate.starts_with('"')
        && !candidate.chars().any(char::is_whitespace)
        && !has_uri_scheme(candidate)
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
///
/// **v2**: now incorporates canonical attribution roles + slice IRIs so that two
/// otherwise-identical findings (same severity/code/location/message) produce
/// different fingerprints when their structured attribution differs. Attributions
/// are sorted by `(role, slice_iri)` for order-independence.
fn stable_fingerprint(finding: &Finding) -> String {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let (severity, code, location, message) = finding.sort_key();
    let mut hash = FNV_OFFSET;

    // Hash the primary finding fields (same as v1).
    for part in [severity.as_str(), code, location.as_str(), message] {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        // Field separator keeps "ab|c" distinct from "a|bc".
        hash ^= 0x1f;
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    // Hash sorted attributions (role, slice_iri) so that different attribution
    // roles on an otherwise-identical finding produce a different fingerprint.
    // Sorted for order-independence.
    let mut sorted_attrs: Vec<(&str, &str)> = finding
        .attributions
        .iter()
        .map(|a| (a.role.as_str(), a.slice_iri.as_str()))
        .collect();
    sorted_attrs.sort_unstable();

    // Separator between primary fields and attribution section.
    hash ^= 0x1e;
    hash = hash.wrapping_mul(FNV_PRIME);

    for (role, iri) in &sorted_attrs {
        for part in [*role, *iri] {
            for byte in part.as_bytes() {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(FNV_PRIME);
            }
            hash ^= 0x1f;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        // Attribution entry separator.
        hash ^= 0x1d;
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
            // form-feed, or VT produces a graph rdflib/oxigraph reject (#654).
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
/// in any RDF tool (oxigraph, rdflib) without TriG/prefix handling. Output
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

/// Repo-relative anchor for findings with no specific source file (whole-ontology
/// lint warnings, legacy message-only findings). GitHub code-scanning rejects a
/// result with no location, and a location must have a repo-relative
/// `physicalLocation`, so these are attributed to the ontology root.
const FALLBACK_ARTIFACT_URI: &str = "ontology/gmeow.ttl";

fn sarif_result(finding: &Finding) -> Value {
    let mut result = json!({
        "ruleId": finding.code,
        "level": finding.severity.sarif_level(),
        "message": { "text": finding.message },
        "partialFingerprints": {
            "gmeowFindingHash/v2": stable_fingerprint(finding),
        },
    });

    // GitHub code-scanning requires every result to carry at least one location,
    // every location (primary AND related) to have a repo-relative
    // `physicalLocation`, and logical-only locations to be disallowed. So:
    // render each source location, keep the physical ones, fold all logical
    // entries (focus IRI, SHACL path/value, GTS wire coords) onto a single
    // primary location, and synthesize a fallback anchor when no file is known.
    let rendered: Vec<Value> = finding
        .locations
        .iter()
        .chain(finding.related_locations.iter())
        .map(sarif_location)
        .collect();
    let mut physical: Vec<Value> = rendered
        .iter()
        .filter(|loc| loc.get("physicalLocation").is_some())
        .cloned()
        .collect();
    let mut logical: Vec<Value> = Vec::new();
    for loc in &rendered {
        if let Some(entries) = loc.get("logicalLocations").and_then(Value::as_array) {
            for entry in entries {
                if !logical.contains(entry) {
                    logical.push(entry.clone());
                }
            }
        }
    }

    // The primary location: the first physical one, else the ontology-root
    // fallback. All gathered logical entries fold onto it.
    let mut primary = if physical.is_empty() {
        json!({ "physicalLocation": { "artifactLocation": { "uri": FALLBACK_ARTIFACT_URI } } })
    } else {
        physical.remove(0)
    };
    if logical.is_empty() {
        if let Some(obj) = primary.as_object_mut() {
            obj.remove("logicalLocations");
        }
    } else {
        primary["logicalLocations"] = json!(logical);
    }
    result["locations"] = json!([primary]);

    // Remaining physical locations ride as related locations (their logical
    // entries already folded onto the primary, so drop them to avoid duplication).
    if !physical.is_empty() {
        for related in &mut physical {
            if let Some(obj) = related.as_object_mut() {
                obj.remove("logicalLocations");
                obj.remove("properties");
            }
        }
        result["relatedLocations"] = json!(physical);
    }

    // Emit result-level properties: detail text (if any) + structured
    // slice attributions (§9 / S5). Uses a single json!() call so both fields
    // land in the same "properties" object.
    let mut props = serde_json::Map::new();
    if let Some(detail) = &finding.detail {
        props.insert("detail".to_owned(), json!(detail));
    }
    if !finding.attributions.is_empty() {
        // Sorted (role, slice_iri) for deterministic output.
        let mut sorted: Vec<_> = finding
            .attributions
            .iter()
            .map(|a| {
                let mut obj = serde_json::Map::new();
                obj.insert("sliceIri".to_owned(), json!(a.slice_iri));
                obj.insert("role".to_owned(), json!(a.role));
                if let Some(ev) = &a.evidence {
                    obj.insert("evidence".to_owned(), json!(ev));
                }
                serde_json::Value::Object(obj)
            })
            .collect();
        sorted.sort_by_key(|v| {
            (
                v["role"].as_str().unwrap_or("").to_owned(),
                v["sliceIri"].as_str().unwrap_or("").to_owned(),
            )
        });
        props.insert("gmeow.attributions".to_owned(), json!(sorted));
    }
    if !props.is_empty() {
        result["properties"] = serde_json::Value::Object(props);
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
        let annotation = strip_angle(annotation);
        if !is_artifact_uri(annotation) {
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
        // A repo-relative bundle path (the valid physical artifact URI) carrying
        // the GTS wire coordinates as logical locations.
        finding.add_location(
            Location::new(Some("bundle.gts".to_owned()), None, None, None)
                .with_gts_quad(42)
                .with_gts_segment(2),
        );
        let mut report = Report::new("validate");
        report.add_finding(finding);

        let value: Value = serde_json::from_str(&to_sarif(&report).unwrap()).unwrap();
        let result = &value["runs"][0]["results"][0];

        // partialFingerprints is stable and present for code-scanning dedup.
        let fp = result["partialFingerprints"]["gmeowFindingHash/v2"]
            .as_str()
            .expect("fingerprint present");
        assert_eq!(fp.len(), 16);

        // The physical artifact URI is the repo-relative bundle path.
        assert_eq!(
            result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
            "bundle.gts"
        );
        // The wire coordinates surface as logical locations + properties.
        let logical = &result["locations"][0]["logicalLocations"];
        assert_eq!(logical[0]["kind"], "gts:quad");
        assert_eq!(logical[0]["name"], "quad#42");
        assert_eq!(result["locations"][0]["properties"]["gts.quadIndex"], 42);
        assert_eq!(result["locations"][0]["properties"]["gts.segmentIndex"], 2);

        // The referenced artifact is listed once at the run level.
        assert_eq!(
            value["runs"][0]["artifacts"][0]["location"]["uri"],
            "bundle.gts"
        );
    }

    #[test]
    fn sarif_emits_automation_details_id_only_when_category_set() {
        let mut report = Report::new("validate");
        report.add_finding(Finding::new(Severity::Error, "x", "boom"));

        // No category metadata -> no automationDetails (existing behavior).
        let value: Value = serde_json::from_str(&to_sarif(&report).unwrap()).unwrap();
        assert!(value["runs"][0]["automationDetails"].is_null());

        // A non-empty category string -> run-level automationDetails.id.
        report
            .metadata
            .insert("category".to_owned(), json!("ontology"));
        let value: Value = serde_json::from_str(&to_sarif(&report).unwrap()).unwrap();
        assert_eq!(value["runs"][0]["automationDetails"]["id"], "ontology");

        // An empty / non-string category is ignored (omitted, not emitted blank).
        report.metadata.insert("category".to_owned(), json!(""));
        let value: Value = serde_json::from_str(&to_sarif(&report).unwrap()).unwrap();
        assert!(value["runs"][0]["automationDetails"].is_null());
        report.metadata.insert("category".to_owned(), json!(7));
        let value: Value = serde_json::from_str(&to_sarif(&report).unwrap()).unwrap();
        assert!(value["runs"][0]["automationDetails"].is_null());
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
    fn html_emits_one_row_per_finding() {
        // Well-formedness, not just substring presence: the rendered table must be
        // balanced and carry exactly one data row per finding plus the header row.
        // (Ports `tests/test_diagnostics.py::test_html_is_well_formed_with_one_row_per_finding`.)
        let mut report = Report::new("validate");
        for i in 0..3 {
            report.add_finding(Finding::new(
                Severity::Error,
                format!("code.{i}"),
                format!("message {i}"),
            ));
        }

        let html = to_html(&report);

        // Balanced, single table.
        assert_eq!(html.matches("<table>").count(), 1);
        assert_eq!(html.matches("</table>").count(), 1);
        // Balanced rows: one header row + one per finding, all closed. Count by
        // `</tr>` (close tags never carry attributes) and match the
        // attribute-tolerant `<tr` open prefix, so adding a class/style to a row
        // start tag later cannot silently break this assertion.
        let close_rows = html.matches("</tr>").count();
        assert_eq!(html.matches("<tr").count(), close_rows);
        assert_eq!(close_rows, 1 + report.findings.len());
    }

    #[test]
    fn sarif_artifact_uris_are_repo_relative_and_iris_are_logical() {
        // GitHub code-scanning requires `artifactLocation.uri` to be a repo-relative
        // file reference: it rejects angle-bracketed IRIs AND absolute schemes
        // ("scheme https did not match checkout scheme file"). So the repo `.ttl`
        // path is the physical artifact, while the focus-node IRI and a SHACL
        // `path <iri>` annotation are logical locations. Regression for #666.
        let mut finding =
            Finding::new(Severity::Error, "shacl.MinCount", "missing property").with_tool("shacl");
        // Primary: repo-relative file path (physical) carrying the focus IRI as a
        // logical anchor.
        finding.add_location(Location::new(
            Some("core/ai/examples/grounded-claim.ttl".to_owned()),
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

        // Primary physical URI is the repo-relative file, not the absolute IRI.
        assert_eq!(
            result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
            "core/ai/examples/grounded-claim.ttl"
        );
        // The focus IRI and the folded `path` annotation ride along as logical
        // locations on the primary (a logical-only related location would be
        // rejected by GitHub, so it is folded here rather than emitted).
        let names: Vec<&str> = result["locations"][0]["logicalLocations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["fullyQualifiedName"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"https://blackcatinformatics.ca/gmeow/examples/ai/claim"));
        assert!(names.contains(&"path https://blackcatinformatics.ca/gmeow/groundedIn"));

        // The only related location was logical-only, so none is emitted — and
        // every relatedLocation that IS emitted must carry a physicalLocation.
        assert!(result["relatedLocations"].is_null());

        // No emitted artifact URI carries an angle bracket OR an absolute scheme,
        // and every relatedLocation across the run has a physicalLocation.
        let serialized = to_sarif(&report).unwrap();
        assert!(
            !serialized.contains("\"uri\": \"<"),
            "angle-bracketed URI leaked"
        );
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
    fn stale_cache_shape_promotes_file_to_primary_and_folds_logicals() {
        // The exact shape a stale validation cache yields: a SHACL finding whose
        // PRIMARY location is logical-only (the focus IRI) and whose related
        // locations are the source file (physical) plus a `path <iri>` annotation
        // (logical-only). GitHub requires the primary location to be physical, so
        // the file is promoted to primary and every logical entry folds onto it.
        // Regression for #666.
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

        let value: Value = serde_json::from_str(&to_sarif(&report).unwrap()).unwrap();
        let result = &value["runs"][0]["results"][0];

        // The file is promoted to the (physical) primary location.
        assert_eq!(
            result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
            "core/ai/examples/grounded-claim.ttl"
        );
        // No physical-less location survives, so no relatedLocations are emitted.
        assert!(result["relatedLocations"].is_null());
        // Both logical entries fold onto the primary.
        let names: Vec<&str> = result["locations"][0]["logicalLocations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["fullyQualifiedName"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"path https://blackcatinformatics.ca/gmeow/groundedIn"));
        assert!(names.contains(&"https://blackcatinformatics.ca/gmeow/examples/ai/claim"));
    }

    #[test]
    fn fileless_finding_anchors_to_the_ontology_root() {
        // A message-only legacy warning (no location at all) must still get a
        // location with a repo-relative physicalLocation, else code-scanning
        // rejects it ("expected at least one location"). Regression for #666.
        let mut report = Report::new("validate");
        report.add_finding(Finding::new(
            Severity::Warning,
            "validate.warning",
            "class gmeow:Analogy is missing gmeow:howToUse",
        ));
        let value: Value = serde_json::from_str(&to_sarif(&report).unwrap()).unwrap();
        let result = &value["runs"][0]["results"][0];
        assert_eq!(
            result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
            "ontology/gmeow.ttl"
        );
        assert!(result["locations"]
            .as_array()
            .is_some_and(|a| !a.is_empty()));
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

    /// Test 3: SARIF fingerprint role sensitivity.
    ///
    /// Two otherwise-identical findings that differ only in their attribution
    /// roles must produce DIFFERENT fingerprints (v2 contract).
    #[test]
    fn sarif_fingerprint_is_role_sensitive() {
        use crate::model::DiagnosticAttribution;

        let make_finding = |role: &str| {
            let mut f = Finding::new(Severity::Error, "shacl.MinCount", "missing property");
            f.attributions.push(DiagnosticAttribution {
                slice_iri: "https://blackcatinformatics.ca/gmeow/slices/core/epistemics".to_owned(),
                role: role.to_owned(),
                evidence: None,
            });
            f
        };

        let finding_shape = make_finding("shape-owner");
        let finding_focus = make_finding("focus-origin");
        let finding_scope = make_finding("evaluation-scope");

        // Same slice IRI, same severity/code/message/location — only role differs.
        let fp_shape = stable_fingerprint(&finding_shape);
        let fp_focus = stable_fingerprint(&finding_focus);
        let fp_scope = stable_fingerprint(&finding_scope);

        assert_ne!(
            fp_shape, fp_focus,
            "shape-owner vs focus-origin must produce different fingerprints"
        );
        assert_ne!(
            fp_shape, fp_scope,
            "shape-owner vs evaluation-scope must produce different fingerprints"
        );
        assert_ne!(
            fp_focus, fp_scope,
            "focus-origin vs evaluation-scope must produce different fingerprints"
        );

        // Self-consistency: same finding always produces the same fingerprint.
        assert_eq!(
            stable_fingerprint(&finding_shape),
            stable_fingerprint(&finding_shape),
            "fingerprint must be deterministic"
        );
    }

    /// SARIF output carries `gmeow.attributions` in result properties when
    /// attributions are present.
    #[test]
    fn sarif_result_properties_carry_attributions() {
        use crate::model::DiagnosticAttribution;
        use serde_json::Value;

        let mut finding = Finding::new(Severity::Error, "shacl.MinCount", "missing property");
        finding.attributions.push(DiagnosticAttribution {
            slice_iri: "https://blackcatinformatics.ca/gmeow/slices/core/shapes".to_owned(),
            role: "shape-owner".to_owned(),
            evidence: Some("slices/core/shapes/shapes.ttl".to_owned()),
        });
        finding.attributions.push(DiagnosticAttribution {
            slice_iri: "https://blackcatinformatics.ca/gmeow/slices/ext/data".to_owned(),
            role: "focus-origin".to_owned(),
            evidence: None,
        });
        let mut report = Report::new("shacl");
        report.add_finding(finding);

        let value: Value = serde_json::from_str(&to_sarif(&report).unwrap()).unwrap();
        let result = &value["runs"][0]["results"][0];

        let attrs = result["properties"]["gmeow.attributions"]
            .as_array()
            .expect("gmeow.attributions must be present as an array");

        assert_eq!(attrs.len(), 2, "two attributions expected");

        // Sorted by (role, sliceIri): focus-origin before shape-owner.
        assert_eq!(attrs[0]["role"], "focus-origin");
        assert_eq!(
            attrs[0]["sliceIri"],
            "https://blackcatinformatics.ca/gmeow/slices/ext/data"
        );
        assert!(
            attrs[0]["evidence"].is_null(),
            "focus-origin has no evidence"
        );
        assert_eq!(attrs[1]["role"], "shape-owner");
        assert_eq!(attrs[1]["evidence"], "slices/core/shapes/shapes.ttl");
    }
}
