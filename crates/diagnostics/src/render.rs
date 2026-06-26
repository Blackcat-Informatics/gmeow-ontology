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

/// Build a `BTreeMap` from rule id to `&Rule` for O(log n) lookup by finding code.
/// Built once per render call and shared across findings.
fn rule_map(report: &Report) -> BTreeMap<&str, &Rule> {
    report.rules.iter().map(|r| (r.id.as_str(), r)).collect()
}

/// Render a compact terminal-safe plain-text report.
pub fn to_text(report: &Report) -> String {
    let normalized = report.normalized();
    let rules = rule_map(&normalized);
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
        // Suggestions (already sorted+deduped by normalize): one indented line each.
        for suggestion in &finding.suggestions {
            lines.push(format!("  ↳ suggestion: {suggestion}"));
        }
        // Help URI from the rule, if present.
        if let Some(rule) = rules.get(finding.code.as_str()) {
            if let Some(uri) = &rule.help_uri {
                lines.push(format!("  ↳ help: {uri}"));
            }
        }
    }
    lines.join("\n")
}

/// Whether a report has any finding with suggestions or any rule with a help_uri,
/// used to decide whether to include the `.suggestions`/`.help` CSS rules.
fn has_advisory_content(report: &Report, rules: &BTreeMap<&str, &Rule>) -> bool {
    report.findings.iter().any(|f| {
        !f.suggestions.is_empty()
            || rules
                .get(f.code.as_str())
                .and_then(|r| r.help_uri.as_deref())
                .is_some()
    })
}

/// Render a self-contained static HTML report.
pub fn to_html(report: &Report) -> String {
    let normalized = report.normalized();
    let rules = rule_map(&normalized);
    let advisory_css = if has_advisory_content(&normalized, &rules) {
        "\n    .suggestions { margin: 0.25rem 0 0 0; padding-left: 1.2rem; color: #4b5563; font-size: 0.9rem; }\n    .help { color: #175cd3; font-size: 0.85rem; margin-left: 0.4rem; text-decoration: none; }\n    .help:hover { text-decoration: underline; }"
    } else {
        ""
    };
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

        // Message cell: message text, optional suggestions list, optional help link.
        let mut msg_cell = escape_html(&finding.message);
        if !finding.suggestions.is_empty() {
            msg_cell.push_str("<ul class=\"suggestions\">");
            for suggestion in &finding.suggestions {
                msg_cell.push_str(&format!("<li>{}</li>", escape_html(suggestion)));
            }
            msg_cell.push_str("</ul>");
        }
        if let Some(rule) = rules.get(finding.code.as_str()) {
            if let Some(uri) = &rule.help_uri {
                msg_cell.push_str(&format!(
                    "<a class=\"help\" href=\"{}\">\u{2139} help</a>",
                    escape_html(uri)
                ));
            }
        }
        rows.push_str(&format!("<td>{msg_cell}</td>"));

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
    .sev-note, .sev-info {{ background: #175cd3; }}{advisory_css}
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
        advisory_css = advisory_css,
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
        json!({
            "physicalLocation": { "artifactLocation": { "uri": FALLBACK_ARTIFACT_URI } },
            "properties": {
                "gmeow.syntheticPhysicalLocation": true,
                "gmeow.syntheticPhysicalLocationReason": "logical-only diagnostic anchor"
            }
        })
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
    // Advisory suggestions land in properties as a plain string array.
    // SARIF `fixes` (with artifactChanges) is deliberately left to D5/#764
    // where suggestions become concrete edits with file mutations.
    if !finding.suggestions.is_empty() {
        props.insert("gmeow.suggestions".to_owned(), json!(finding.suggestions));
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
    use crate::model::{DiagnosticAttribution, Finding, Location, Report, Rule, Severity};

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
    /// plus a logical-only related `path <iri>` that folds onto the primary
    /// (#666), two attributions (sorted by role then sliceIri), a `category`
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
    /// promoted to primary and every logical entry folds onto it (#666).
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

    // ── Whole-output snapshot goldens (T8, #789) ─────────────────────────────

    #[test]
    fn sarif_full_snapshot() {
        let value: Value =
            serde_json::from_str(&to_sarif(&comprehensive_report()).unwrap()).unwrap();
        insta::assert_json_snapshot!(value);
    }

    #[test]
    fn json_full_snapshot() {
        let value: Value =
            serde_json::from_str(&to_json(&comprehensive_report()).unwrap()).unwrap();
        insta::assert_json_snapshot!(value);
    }

    #[test]
    fn gmeow_rdf_full_snapshot() {
        insta::assert_snapshot!(to_gmeow_rdf(&comprehensive_report()));
    }

    #[test]
    fn text_full_snapshot() {
        insta::assert_snapshot!(to_text(&comprehensive_report()));
    }

    #[test]
    fn html_full_snapshot() {
        insta::assert_snapshot!(to_html(&comprehensive_report()));
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
        // emits `relatedLocations`, so it pins that branch AND makes the #666
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
        // physicalLocation (the #666 contract, here actually exercised).
        let related = value["runs"][0]["results"][0]["relatedLocations"]
            .as_array()
            .expect("relatedLocations emitted for a 2+ physical-location finding");
        assert_eq!(related.len(), 1);
        assert!(related[0].get("physicalLocation").is_some());

        insta::assert_json_snapshot!(value);
    }

    #[test]
    fn sarif_marks_synthetic_primary_location_for_logical_only_finding() {
        let mut finding = Finding::new(Severity::Warning, "shacl.MinCount", "missing property")
            .with_tool("shacl");
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
        // #666 code-scanning contract, asserted as a property over the whole rich
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
        // gmeow_rdf snapshot; this asserts the graph-containment invariant.)
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
        insta::assert_snapshot!(to_text(&advisory_report()));
    }

    #[test]
    fn advisory_html_snapshot() {
        insta::assert_snapshot!(to_html(&advisory_report()));
    }

    #[test]
    fn advisory_sarif_snapshot() {
        let value: Value = serde_json::from_str(&to_sarif(&advisory_report()).unwrap()).unwrap();
        insta::assert_json_snapshot!(value);
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
}
