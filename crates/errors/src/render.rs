// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

mod finding_rdf;

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::model::{Finding, Location, RelatedLabel, Report, Rule};

/// Render a report as stable pretty JSON.
pub fn to_json(report: &Report) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&report.normalized())
}

/// Render a report as SARIF 2.1.0.
///
/// Beyond the basic results, this emits the pieces GitHub code-scanning needs to
/// navigate and de-duplicate findings: every distinct artifact (file or
/// `.gts` bundle) referenced by a finding is listed under `runs[].artifacts`,
/// each result carries `logicalLocations` + `properties` for its GTS wire
/// coordinates, and each result carries a stable `partialFingerprints` value
/// derived from the deterministic [`Finding::sort_key`] so re-runs dedupe.
///
/// When the report carries a `category` metadata key (set by the Python
/// diagnostics-output config), the run emits run-level
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

/// A stable, dependency-free `blake3` fingerprint (first 8 bytes, hex-encoded).
/// Used for SARIF `partialFingerprints` so GitHub code-scanning can dedupe a
/// finding across runs even as line numbers shift, and as the fallback subject IRI
/// for a NON-ledger finding that carries no `finding_iri`. Deterministic across
/// platforms; the SINGLE hash the diagnostics surface uses now that the FNV-1a
/// scheme is retired — ledger witnesses and this fallback both hash with `blake3`.
///
/// **v2**: incorporates canonical attribution roles + slice IRIs so that two
/// otherwise-identical findings (same severity/code/location/message) produce
/// different fingerprints when their structured attribution differs. Attributions
/// are sorted by `(role, slice_iri)` for order-independence. Separator bytes keep
/// `"ab|c"` distinct from `"a|bc"`.
fn stable_fingerprint(finding: &Finding) -> String {
    let (severity, code, location, message) = finding.sort_key();
    let mut hasher = blake3::Hasher::new();

    // The primary finding fields, each followed by a field separator.
    for part in [severity.as_str(), code, location.as_str(), message] {
        hasher.update(part.as_bytes());
        hasher.update(&[0x1f]);
    }

    // The sorted attributions (role, slice_iri) so that different attribution
    // roles on an otherwise-identical finding produce a different fingerprint;
    // sorted for order-independence, behind a primary/attribution separator.
    let mut sorted_attrs: Vec<(&str, &str)> = finding
        .attributions
        .iter()
        .map(|a| (a.role.as_str(), a.slice_iri.as_str()))
        .collect();
    sorted_attrs.sort_unstable();
    hasher.update(&[0x1e]);
    for (role, iri) in &sorted_attrs {
        for part in [*role, *iri] {
            hasher.update(part.as_bytes());
            hasher.update(&[0x1f]);
        }
        // Attribution entry separator.
        hasher.update(&[0x1d]);
    }

    let digest = hasher.finalize();
    let mut out = String::with_capacity(16);
    use std::fmt::Write;
    for byte in &digest.as_bytes()[..8] {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// The GMEOW namespace IRI prefix.
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
/// The `logic:` namespace IRI prefix — home of the `logic:FindingCategory`
/// taxonomy individuals a finding's category projects to.
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
/// The named graph the diagnostics projection lives in.
const DIAGNOSTICS_GRAPH: &str = "https://blackcatinformatics.ca/gmeow/graph/diagnostics";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const XSD_ANY_URI: &str = "http://www.w3.org/2001/XMLSchema#anyURI";
const XSD_NNI: &str = "http://www.w3.org/2001/XMLSchema#nonNegativeInteger";

/// A concise human label for a finding: `"<code>: <message>"`, with the message
/// truncated to a `char`-boundary-safe 80 characters on the nearest preceding
/// word boundary (an ellipsis marks the cut). Truncating on a word boundary
/// avoids mid-word fragments that spell-checkers flag. Findings are generated
/// A-Box instance data, so every one also carries a `skos:definition` (see
/// [`finding_definition`]) and the rest of the assertional-tier annotation
/// contract, via [`crate::abox::annotate_nquads`].
fn finding_label(code: &str, message: &str) -> String {
    const MAX: usize = 80;
    let truncated = if message.chars().count() > MAX {
        // Collect the first MAX chars, then back-track to the last word boundary
        // so the cut never falls mid-word.
        let mut s: String = message.chars().take(MAX).collect();
        if let Some(boundary) = s.rfind(|c: char| c.is_whitespace() || c == '(') {
            s.truncate(boundary);
        }
        s.push('…');
        s
    } else {
        message.to_owned()
    };
    if code.is_empty() {
        truncated
    } else if truncated.is_empty() {
        code.to_string()
    } else {
        format!("{code}: {truncated}")
    }
}

/// A `skos:definition` for a finding: the definition-equivalent companion to
/// [`finding_label`], derived purely from the finding's own severity, code, and
/// FULL (untruncated) message — never fabricated, never truncated (unlike the
/// label, a definition is expected to be read in full). Part of the
/// assertional-tier annotation contract every generated `gmeow:Finding`
/// individual satisfies via [`crate::abox::annotate_nquads`].
fn finding_definition(finding: &Finding) -> String {
    if finding.code.is_empty() {
        format!(
            "{} diagnostic: {}",
            finding.severity.as_str(),
            finding.message
        )
    } else {
        format!(
            "{} diagnostic {}: {}",
            finding.severity.as_str(),
            finding.code,
            finding.message
        )
    }
}

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

/// Escape a string literal for an N-Triples/N-Quads `STRING_LITERAL_QUOTE`:
/// backslash, double-quote, and the C0 control characters (`\n`, `\r`, `\t`,
/// and any other U+0000–U+001F as `\uXXXX`). Public so the `gmeow-validate`
/// `ComplianceAssessment` emitter (`crates/validate/src/advisory.rs`) escapes
/// its N-Quad literals through the exact same rules rather than a drifting copy.
pub fn nq_escape(value: &str) -> String {
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
            // form-feed, or VT produces a graph rdflib/oxigraph reject.
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Project a report into the `gmeow:` RDF vocabulary as N-Quads, all in the
/// `gmeow:graph/diagnostics` named graph.
///
/// Each finding becomes a `gmeow:Finding` individual carrying `gmeow:findingCode`,
/// `gmeow:findingMessage`, `gmeow:findingTool`, a `gmeow:findingSeverity`
/// pointing at the matching `gmeow:DiagnosticSeverity` individual, one
/// `gmeow:findingSuggestion` per suggestion (already sorted/deduped), an optional
/// `gmeow:findingHelpUri` from the rule registry, and one `gmeow:findingLocation`
/// blank node per location, whose GTS wire coordinates are hung on it as datatype
/// properties. This is the native in-bundle form of a report — a projection of the
/// canonical Rust model (Principle 4), SPARQL-queryable beside the data it
/// describes. N-Quads is used so the output parses in any RDF tool (oxigraph,
/// rdflib) without TriG/prefix handling. Output is deterministic: the report is
/// normalized and findings are emitted in sorted order with content-addressed
/// finding IRIs.
pub fn to_gmeow_rdf(report: &Report) -> String {
    to_gmeow_rdf_in_graph(report, DIAGNOSTICS_GRAPH)
}

/// Project a [`Report`] into `gmeow:Finding` N-Quads inside the named graph
/// `graph_iri`.
///
/// This is the single emitter [`to_gmeow_rdf`] wraps for the canonical
/// `graph/diagnostics`. Other producers of restricted Findings — e.g. the native↔
/// oracle / native↔corpus reasoning divergence ledger, which the diagnostics
/// doctrine declares ARE `gmeow:Finding`s — reuse it with their own named graph
/// (`graph/conformance`) rather than duplicating the projection.
pub fn to_gmeow_rdf_in_graph(report: &Report, graph_iri: &str) -> String {
    finding_rdf::to_text(report, graph_iri)
}

/// Project findings directly into their native diagnostics graph.
/// The report remains the single source for the native and terminal text surfaces.
///
/// # Errors
/// Returns the native builder diagnostic if the projected dataset is invalid.
pub fn to_gmeow_dataset(
    report: &Report,
) -> Result<std::sync::Arc<purrdf_core::RdfDataset>, purrdf_core::RdfDiagnostic> {
    to_gmeow_dataset_in_graph(report, DIAGNOSTICS_GRAPH)
}

/// Project findings into an explicitly selected native named graph without RDF text.
/// This emits grade coordinates; authored reasoning still owns gate verdicts.
///
/// # Errors
/// Returns the native builder diagnostic if the projected dataset is invalid.
pub fn to_gmeow_dataset_in_graph(
    report: &Report,
    graph_iri: &str,
) -> Result<std::sync::Arc<purrdf_core::RdfDataset>, purrdf_core::RdfDiagnostic> {
    let mut builder = purrdf_core::RdfDatasetBuilder::new();
    append_gmeow_findings(report, graph_iri, &mut builder);
    builder.freeze()
}

/// Append the native finding projection directly into an existing carrier builder.
/// The caller must validate/freeze the builder before publishing it; invalid term
/// identities fail that boundary. No dataset is frozen or serialized by this call.
pub fn append_gmeow_findings(
    report: &Report,
    graph_iri: &str,
    builder: &mut purrdf_core::RdfDatasetBuilder,
) {
    finding_rdf::append(report, graph_iri, builder);
}

/// Build a `BTreeMap` from rule id to `&Rule` for O(log n) lookup by finding code.
/// Built once per render call and shared across findings.
fn rule_map(report: &Report) -> BTreeMap<&str, &Rule> {
    report.rules.iter().map(|r| (r.id.as_str(), r)).collect()
}

/// Render the text lines for a single finding (message line + suggestion/help lines)
/// into `out`. Shared by [`to_text`] and [`to_text_advisories`].
fn finding_text_lines(finding: &Finding, rules: &BTreeMap<&str, &Rule>, out: &mut Vec<String>) {
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
    out.push(line);
    // The TYPED conformance-failure class the violated law declares. The headline
    // line can only carry the generic component `code` (every cardinality gate in the
    // ontology shares `shacl.MinCountConstraintComponent`), so without this line the
    // human surface can never name WHICH failure was raised.
    if let Some(class) = &finding.failure_class {
        out.push(format!("  ↳ failure class: {class}"));
    }
    // Secondary TEXT-bearing labels (Rust-compiler-style "defined here" / SHACL
    // result-path spans): one indented line each, rendering the label message
    // beside its location so the prose survives to the human text surface too.
    for label in &finding.related_labels {
        out.push(format!(
            "  ↳ note: {} ({})",
            label.message,
            label.location.display()
        ));
    }
    // Suggestions (already sorted+deduped by normalize): one indented line each.
    for suggestion in &finding.suggestions {
        out.push(format!("  ↳ suggestion: {suggestion}"));
    }
    // registry-authored remediations — the "how to fix" payload (never fabricated).
    for remediation in &finding.remediation {
        out.push(format!("  ↳ how to fix: {}", remediation.text));
        if let Some(uri) = &remediation.help_uri {
            out.push(format!("    ↳ see: {uri}"));
        }
    }
    // The single code→governing-term join, built once as `rules`: read the help
    // URI (outward catalog link), the rule-level remediation (gmeow:ruleRemediation),
    // and the governing term's usage guidance (gmeow:howToUse) for the deep surface.
    if let Some(rule) = rules.get(finding.code.as_str()) {
        if let Some(uri) = &rule.help_uri {
            out.push(format!("  ↳ help: {uri}"));
        }
        if let Some(remediation) = &rule.remediation {
            out.push(format!("  ↳ rule remediation: {remediation}"));
        }
        if let Some(how_to_use) = &rule.how_to_use {
            out.push(format!("  ↳ how to use: {how_to_use}"));
        }
    }
    // Per-term usage guidance (howToUse/useWhen/avoidWhen) joined from the bundle
    // documentation graph and projected verbatim onto the finding — never
    // fabricated, so a finding whose terms author none renders no lines.
    for guidance in &finding.guidance {
        out.push(format!(
            "  ↳ {}: {}",
            guidance.modality.label(),
            guidance.text
        ));
    }
    // Reasoner-derived meta-findings carried on the finding (present only after the
    // meta-reasoning fold has run and been read back): the shared root cause and
    // any cross-node glut edge. The 'N findings share root R' cluster grouping is a
    // report-level surface rendered once by the caller.
    if let Some(root) = &finding.root_cause {
        out.push(format!("  ↳ root cause: {root}"));
    }
    for peer in &finding.cross_node_glut_with {
        out.push(format!("  ↳ cross-node glut with: {peer}"));
    }
}

/// The report-level 'N findings share root R' cluster grouping — one line per
/// distinct reasoner-derived `gmeow:findingRootCause`, in deterministic root-IRI
/// order. Empty when the meta-reasoning fold has derived no root cause.
fn cluster_summary_lines(report: &Report) -> Vec<String> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for finding in &report.findings {
        if let Some(root) = &finding.root_cause {
            *counts.entry(root.as_str()).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .map(|(root, n)| format!("{n} finding(s) share root {root}"))
        .collect()
}

/// The `finding_iri → &Finding` index the witness-DAG walk resolves antecedents
/// against. Built ONCE per report by a renderer and shared across every finding's
/// [`derivation_lines`] call — rebuilding it per finding made `to_text`/`to_html`
/// quadratic over the report.
fn finding_index(report: &Report) -> BTreeMap<&str, &Finding> {
    report
        .findings
        .iter()
        .filter_map(|f| f.finding_iri.as_deref().map(|iri| (iri, f)))
        .collect()
}

/// The witness-DAG derivation section for a finding, reconstructed via the ONE
/// shared DAG walk engine ([`crate::dag::walk`]) over the report's finding graph
/// (keyed on `finding_iri`, edges are `antecedents`), resolving antecedents through
/// the caller-built [`finding_index`]. Returns one indented line per antecedent in
/// pre-order (DFS), naming each cited antecedent IRI and its message. Empty when
/// the finding has no antecedents or is not a ledger witness.
fn derivation_lines(by_iri: &BTreeMap<&str, &Finding>, finding: &Finding) -> Vec<String> {
    use crate::dag::walk;
    let Some(root_iri) = finding.finding_iri.as_deref() else {
        return Vec::new();
    };
    if finding.antecedents.is_empty() {
        return Vec::new();
    }
    // resolve never yields None (an antecedent absent from the report resolves to a
    // placeholder message), so the walk never hard-fails on an unresolved node; a
    // structural cycle (which the acyclic ledger never produces) degrades to no
    // section rather than a render panic.
    let tree = walk(
        root_iri.to_owned(),
        |k: &String| {
            Some(
                by_iri
                    .get(k.as_str())
                    .map(|f| f.message.clone())
                    .unwrap_or_else(|| "(antecedent not in report)".to_owned()),
            )
        },
        |k: &String, _msg: &String| {
            by_iri
                .get(k.as_str())
                .map(|f| f.antecedents.clone())
                .unwrap_or_default()
        },
    );
    let Ok(tree) = tree else {
        return Vec::new();
    };
    let mut lines = vec!["  ↳ derivation:".to_owned()];
    // Skip the root itself (depth 0); every deeper node is a cited antecedent.
    for node in tree.preorder().into_iter().filter(|n| n.depth > 0) {
        let indent = "  ".repeat(node.depth as usize + 1);
        lines.push(format!("{indent}← {} ({})", node.key, node.payload));
    }
    lines
}

/// The reasoner's explain-skeleton citation lines: one indented line per
/// `gmeow:findingDerivedFromQuad` reifier IRI this finding's verdict derives
/// from. A SEPARATE edge from the antecedent witness-DAG walked by
/// [`derivation_lines`] (finding-to-finding, keyed on fingerprint IRIs) — this
/// cites reasoned-quad reifier IRIs instead, never another finding. Empty for a
/// finding that is not the outcome of a reasoning pass.
fn derived_from_quad_lines(finding: &Finding) -> Vec<String> {
    finding
        .derived_from_quads
        .iter()
        .map(|iri| format!("  ↳ derived from: {iri}"))
        .collect()
}

/// Render a compact terminal-safe plain-text report — the FULL per-finding form
/// (one message line each, plus suggestion/help lines).
///
/// Canonical consumer: artifact / SARIF-adjacent paths and any caller that needs
/// every finding spelled out. For an interactive console gate that may surface
/// thousands of report-only findings (e.g. the coverage ratchet), prefer
/// [`to_text_summarized`], which collapses non-error findings to per-code counts.
pub fn to_text(report: &Report) -> String {
    let normalized = report.normalized();
    let rules = rule_map(&normalized);
    let by_iri = finding_index(&normalized);
    let mut lines = Vec::new();
    for finding in &normalized.findings {
        finding_text_lines(finding, &rules, &mut lines);
        // The witness-DAG derivation/explain section, walked via the one shared
        // DAG engine over the report's finding graph (index built once above).
        lines.extend(derivation_lines(&by_iri, finding));
        // The reasoner's explain-skeleton quad-derivation citations — a SEPARATE
        // edge from the antecedent witness-DAG above.
        lines.extend(derived_from_quad_lines(finding));
    }
    // The report-level 'N findings share root R' cluster grouping (empty unless the
    // meta-reasoning fold has run and been read back onto the findings).
    lines.extend(cluster_summary_lines(&normalized));
    lines.join("\n")
}

/// Render a console-digestible report: every ERROR finding in FULL (errors are
/// actionable and, on a healthy gate, few), then every non-error finding
/// (warnings/notes/info) collapsed to a single `SEVERITY code: N finding(s)` line
/// per `(severity, code)`.
///
/// Canonical consumer: the interactive `doc-lint` console (and any gate that emits
/// high-volume report-only warnings). It keeps a thousand-term coverage ratchet
/// from flooding the terminal while preserving the per-term detail in the
/// structured report consumed by [`to_text`]/[`to_json`]/[`to_sarif`].
pub fn to_text_summarized(report: &Report) -> String {
    use crate::model::Severity;

    // Errors in full — they must stay individually actionable. Clone and normalize
    // ONLY the error findings (plus the shared rules) rather than the whole report:
    // on a high-volume report-only gate the non-error findings number in the
    // thousands, and cloning them just to count them is pure allocation overhead.
    let mut error_report = Report::new(report.tool.clone());
    error_report.rules = report.rules.clone();
    error_report.findings = report
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .cloned()
        .collect();
    error_report.normalize();
    let rules = rule_map(&error_report);
    let mut lines = Vec::new();
    for finding in &error_report.findings {
        finding_text_lines(finding, &rules, &mut lines);
    }

    // Non-error findings collapse to one count line per (severity, code), counted
    // over the BORROWED originals — no clone. `normalize()` only sorts/dedups tags,
    // suggestions, locations and rules; it never drops a finding or rewrites its
    // `severity`/`code`, so a raw count is identical to a normalized one. Keying on
    // the severity/code strings keeps the order deterministic via the `BTreeMap`
    // without requiring `Severity: Ord`.
    let mut counts: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for finding in report
        .findings
        .iter()
        .filter(|f| f.severity != Severity::Error)
    {
        *counts
            .entry((finding.severity.as_str(), finding.code.as_str()))
            .or_default() += 1;
    }
    for ((severity, code), count) in &counts {
        lines.push(format!("{severity} {code}: {count} finding(s)"));
    }

    lines.join("\n")
}

/// Render ONLY the advisory (Note/Info) findings as text — the block the
/// legacy CLI appends after its error/warning lines so advisory-tier findings
///  are visible on the default `gmeow validate` surface. Reuses the same
/// per-finding rendering as `to_text` (message line + suggestion/help lines).
/// Returns an empty string when there are no advisory findings.
pub fn to_text_advisories(report: &Report) -> String {
    use crate::model::Severity;
    let normalized = report.normalized();
    let rules = rule_map(&normalized);
    let mut lines = Vec::new();
    for finding in &normalized.findings {
        if matches!(finding.severity, Severity::Note | Severity::Info) {
            finding_text_lines(finding, &rules, &mut lines);
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
    let by_iri = finding_index(&normalized);
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
        // The TYPED conformance-failure class the violated law declares — the code
        // column above can only carry the generic component name every gate of that
        // shape shares.
        if let Some(class) = &finding.failure_class {
            msg_cell.push_str(&format!(
                "<p class=\"failure-class\">failure class: {}</p>",
                escape_html(class)
            ));
        }
        if !finding.suggestions.is_empty() {
            msg_cell.push_str("<ul class=\"suggestions\">");
            for suggestion in &finding.suggestions {
                msg_cell.push_str(&format!("<li>{}</li>", escape_html(suggestion)));
            }
            msg_cell.push_str("</ul>");
        }
        // registry-authored remediations (the "how to fix" payload, never fabricated).
        for remediation in &finding.remediation {
            msg_cell.push_str(&format!(
                "<p class=\"remediation\">how to fix: {}</p>",
                escape_html(&remediation.text)
            ));
        }
        if let Some(rule) = rules.get(finding.code.as_str()) {
            if let Some(uri) = &rule.help_uri {
                msg_cell.push_str(&format!(
                    "<a class=\"help\" href=\"{}\">\u{2139} help</a>",
                    escape_html(uri)
                ));
            }
            // Per-term guidance joined once via the code→rule registry.
            if let Some(remediation) = &rule.remediation {
                msg_cell.push_str(&format!(
                    "<p class=\"remediation\">rule remediation: {}</p>",
                    escape_html(remediation)
                ));
            }
            if let Some(how_to_use) = &rule.how_to_use {
                msg_cell.push_str(&format!(
                    "<p class=\"remediation\">how to use: {}</p>",
                    escape_html(how_to_use)
                ));
            }
        }
        // Per-term usage guidance (howToUse/useWhen/avoidWhen) joined from the
        // bundle documentation graph and projected verbatim — never fabricated.
        for guidance in &finding.guidance {
            msg_cell.push_str(&format!(
                "<p class=\"remediation\">{}: {}</p>",
                escape_html(guidance.modality.label()),
                escape_html(&guidance.text)
            ));
        }
        // Reasoner-derived meta-findings carried on the finding.
        if let Some(root) = &finding.root_cause {
            msg_cell.push_str(&format!(
                "<p class=\"meta\">root cause: {}</p>",
                escape_html(root)
            ));
        }
        for peer in &finding.cross_node_glut_with {
            msg_cell.push_str(&format!(
                "<p class=\"meta\">cross-node glut with: {}</p>",
                escape_html(peer)
            ));
        }
        // The witness-DAG derivation section (walked via the one shared engine,
        // resolving against the index built once above).
        let derivation = derivation_lines(&by_iri, finding);
        if !derivation.is_empty() {
            msg_cell.push_str("<ul class=\"derivation\">");
            // Skip the leading "derivation:" label line; each remaining line is a
            // cited antecedent.
            for line in derivation.iter().skip(1) {
                msg_cell.push_str(&format!("<li>{}</li>", escape_html(line.trim())));
            }
            msg_cell.push_str("</ul>");
        }
        // The reasoner's explain-skeleton quad-derivation citations — a SEPARATE
        // edge from the antecedent witness-DAG rendered above.
        if !finding.derived_from_quads.is_empty() {
            msg_cell.push_str("<ul class=\"derived-from-quad\">");
            for iri in &finding.derived_from_quads {
                msg_cell.push_str(&format!("<li>derived from: {}</li>", escape_html(iri)));
            }
            msg_cell.push_str("</ul>");
        }
        rows.push_str(&format!("<td>{msg_cell}</td>"));

        rows.push_str(&format!("<td>{}</td>", escape_html(&location)));
        rows.push_str("</tr>\n");
    }
    if rows.is_empty() {
        rows.push_str("<tr><td colspan=\"4\">No diagnostics.</td></tr>\n");
    }

    // The report-level 'N findings share root R' cluster grouping — rendered as a
    // list above the table only when the meta-reasoning fold has derived roots.
    let cluster_block = {
        let summary = cluster_summary_lines(&normalized);
        if summary.is_empty() {
            String::new()
        } else {
            let mut block = String::from("  <ul class=\"clusters\">\n");
            for line in &summary {
                block.push_str(&format!("    <li>{}</li>\n", escape_html(line)));
            }
            block.push_str("  </ul>\n");
            block
        }
    };

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
{cluster_block}  <table>
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
        cluster_block = cluster_block,
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
    for related in &mut physical {
        if let Some(obj) = related.as_object_mut() {
            obj.remove("logicalLocations");
            obj.remove("properties");
        }
    }
    let mut related_locations: Vec<Value> = physical;
    // The text-bearing secondary labels: each rides as a related location carrying
    // a `message.text` (SARIF `location.message` is a `{ "text": ... }` object), so
    // the label prose survives into the SARIF byte artifact — the
    // DiagnosticRelatedInformation payload a code-scanning/LSP consumer reads.
    for label in &finding.related_labels {
        related_locations.push(sarif_related_label(label));
    }
    if !related_locations.is_empty() {
        result["relatedLocations"] = json!(related_locations);
    }

    // Emit result-level properties: detail text (if any) + structured
    // slice attributions (§9 / S5). Uses a single json!() call so both fields
    // land in the same "properties" object.
    let mut props = serde_json::Map::new();
    if let Some(detail) = &finding.detail {
        props.insert("detail".to_owned(), json!(detail));
    }
    // The orthogonal finding KIND (the 8-way taxonomy), guarded by Some so an
    // un-categorized finding leaves the SARIF result byte-unchanged.
    if let Some(category) = finding.category {
        props.insert("gmeow.category".to_owned(), json!(category.as_str()));
    }
    // The TYPED conformance-failure class the violated law declares, under the PINNED
    // key `gmeow.failureClass`. `ruleId` carries only the generic component name every
    // gate of that shape shares, so this is the only place the SARIF artifact can name
    // the specific failure. Guarded by Some so a class-less finding is byte-unchanged.
    if let Some(class) = &finding.failure_class {
        props.insert("gmeow.failureClass".to_owned(), json!(class));
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
    // Advisory suggestions land in properties as a plain string array — the
    // per-occurrence advice that carries no mechanical edit.
    if !finding.suggestions.is_empty() {
        props.insert("gmeow.suggestions".to_owned(), json!(finding.suggestions));
    }
    // Per-term usage guidance (howToUse/useWhen/avoidWhen), grouped by modality
    // under its PINNED SARIF key (`gmeow.howToUse`/`gmeow.useWhen`/
    // `gmeow.avoidWhen`) as a string array — `props` is a `serde_json::Map`
    // (BTreeMap-backed; this crate carries no `preserve_order` feature), so the
    // serialized key order is alphabetical regardless of insertion order,
    // keeping byte-diffed goldens deterministic. Absent when the finding carries
    // no guidance.
    let mut guidance_by_modality: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for guidance in &finding.guidance {
        guidance_by_modality
            .entry(guidance.modality.sarif_key())
            .or_default()
            .push(guidance.text.as_str());
    }
    for (key, texts) in guidance_by_modality {
        props.insert(key.to_owned(), json!(texts));
    }
    // The reasoner's explain-skeleton quad-derivation citations, under the
    // PINNED key `gmeow.derivedFromQuad` — a SEPARATE edge from the antecedent
    // witness-DAG (which rides `relatedLocations`, not `properties`).
    if !finding.derived_from_quads.is_empty() {
        props.insert(
            "gmeow.derivedFromQuad".to_owned(),
            json!(finding.derived_from_quads),
        );
    }
    if !props.is_empty() {
        result["properties"] = serde_json::Value::Object(props);
    }

    // registry-authored remediations become SARIF `fixes`: one fix per remediation,
    // its `description.text` the remediation prose, with `artifactChanges` present
    // ONLY when the remediation carries a concrete mechanical edit (an honest
    // absence otherwise — most rules are prose-only). Any artifact URI is routed
    // through the same repo-relative hygiene as every result location.
    let fixes: Vec<Value> = finding.remediation.iter().map(sarif_fix).collect();
    if !fixes.is_empty() {
        result["fixes"] = json!(fixes);
    }
    result
}

/// Render one registry-authored [`Remediation`](crate::diag::Remediation) as a SARIF
/// `fix`. The `description.text` is the remediation prose; `artifactChanges` is
/// emitted ONLY when the remediation carries a mechanical
/// [`ArtifactChange`](crate::diag::ArtifactChange) whose artifact URI passes the
/// repo-relative hygiene GitHub code-scanning requires — an honest absence
/// otherwise.
fn sarif_fix(remediation: &crate::diag::Remediation) -> Value {
    // The remediation's STANDPOINT rides as a fix-level property so the gating
    // strength of the "how to fix" guidance (advisory ⊑ perspectival ⊑ binding — the
    // leg the gate morphism reads) survives into the SARIF byte artifact, not just
    // the RDF/CLI surfaces. This is the property the annotate-by-fingerprint pass's
    // output is greppable on in the regenerated `shacl.sarif`.
    let mut fix = json!({
        "description": { "text": remediation.text },
        "properties": { "gmeow.standpoint": remediation.standpoint.as_str() },
    });
    if let Some(uri) = &remediation.help_uri {
        fix["properties"]["gmeow.helpUri"] = json!(uri);
    }
    if let Some(change) = &remediation.artifact_change {
        // Route the artifact URI through the same repo-relative validation as a
        // result location: only a bare, scheme-less, repo-relative reference is a
        // valid SARIF artifactLocation.uri.
        let loc = Location::new(Some(change.artifact_uri.clone()), None, None, None);
        if let Some(uri) = artifact_uri(&loc) {
            let mut replacement = json!({ "deletedRegion": sarif_region(&change.region) });
            replacement["insertedContent"] = json!({ "text": change.replacement });
            fix["artifactChanges"] = json!([{
                "artifactLocation": { "uri": uri },
                "replacements": [replacement],
            }]);
        }
    }
    fix
}

/// Render one text-bearing [`RelatedLabel`] as a SARIF related `location`: the
/// label's source location plus a `message.text` object carrying the label prose.
/// GitHub code-scanning requires every related location to carry a
/// `physicalLocation`, so a logical-only label location is backed by the same
/// ontology-root fallback the primary location uses; the label's logical entries
/// (a SHACL result-path / focus IRI) ride alongside, losslessly.
fn sarif_related_label(label: &RelatedLabel) -> Value {
    let mut out = sarif_location(&label.location);
    if out.get("physicalLocation").is_none() {
        out["physicalLocation"] = json!({
            "artifactLocation": { "uri": FALLBACK_ARTIFACT_URI },
        });
    }
    out["message"] = json!({ "text": label.message });
    out
}

/// The SARIF `region` object for a mechanical edit — only the coordinates present
/// are emitted (a whole-line replacement carries no column, etc.).
fn sarif_region(region: &crate::diag::Region) -> Value {
    let mut out = serde_json::Map::new();
    if let Some(v) = region.start_line {
        out.insert("startLine".to_owned(), json!(v));
    }
    if let Some(v) = region.start_column {
        out.insert("startColumn".to_owned(), json!(v));
    }
    if let Some(v) = region.end_line {
        out.insert("endLine".to_owned(), json!(v));
    }
    if let Some(v) = region.end_column {
        out.insert("endColumn".to_owned(), json!(v));
    }
    Value::Object(out)
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

#[path = "render.tests.rs"]
#[cfg(test)]
mod tests;
