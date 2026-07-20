// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";
const XSD_ANY_URI: &str = "http://www.w3.org/2001/XMLSchema#anyURI";
const XSD_NNI: &str = "http://www.w3.org/2001/XMLSchema#nonNegativeInteger";

/// A concise human label for a finding: `"<code>: <message>"`, with the message
/// truncated to a `char`-boundary-safe 80 characters on the nearest preceding
/// word boundary (an ellipsis marks the cut). Truncating on a word boundary
/// avoids mid-word fragments that spell-checkers flag. Findings are generated
/// A-Box instance data, so they carry a label and provenance but no
/// `skos:definition` (assertional-tier validation contract).
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
    let normalized = report.normalized();
    let graph = format!("<{graph_iri}>");
    let mut lines: Vec<String> = Vec::new();
    let rules = rule_map(&normalized);

    let triple = |s: &str, p: &str, o: &str, lines: &mut Vec<String>| {
        lines.push(format!("{s} <{p}> {o} {graph} ."));
    };

    for (index, finding) in normalized.findings.iter().enumerate() {
        // The subject IRI is the ledger's canonical blake3 fingerprint IRI when the
        // finding is a ledger witness — the SAME IRI downstream findings' antecedent
        // edges point at (via `finding.antecedents`), so subject and
        // antecedent-object IRIs CLOSE and the meta-rules can join on them. Every
        // production finding is now a ledger witness (SHACL + compile-logic both
        // route through the DiagLedger). A NON-ledger finding (an ad-hoc report built
        // off the pipeline path) carries no fingerprint IRI and falls back to the
        // same blake3 `stable_fingerprint` content hash, disambiguated by index.
        let subject_iri = match &finding.finding_iri {
            Some(iri) => iri.clone(),
            None => format!(
                "{GMEOW}diagnostics/finding/{}-{index}",
                stable_fingerprint(finding)
            ),
        };
        let subject = format!("<{subject_iri}>");
        triple(&subject, RDF_TYPE, &format!("<{GMEOW}Finding>"), &mut lines);
        // Assertional-tier annotation: a human label, a named-graph provenance
        // anchor, and the assertional box role, so the folded bundle's generated
        // findings are self-describing instance data the validator accepts.
        triple(
            &subject,
            RDFS_LABEL,
            &format!(
                "\"{}\"",
                nq_escape(&finding_label(&finding.code, &finding.message))
            ),
            &mut lines,
        );
        triple(
            &subject,
            RDFS_IS_DEFINED_BY,
            &format!("<{DIAGNOSTICS_GRAPH}>"),
            &mut lines,
        );
        triple(
            &subject,
            &format!("{GMEOW}graphBoxRole"),
            &format!("<{GMEOW}boxABox>"),
            &mut lines,
        );
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
        // The orthogonal finding KIND, pointing at the matching logic:Finding*
        // taxonomy individual. Guarded by Some so an un-categorized finding leaves
        // the projection byte-unchanged.
        if let Some(category) = finding.category {
            triple(
                &subject,
                &format!("{GMEOW}findingCategory"),
                &format!("<{LOGIC}{}>", category.iri_local()),
                &mut lines,
            );
        }
        // The gating STANDPOINT truth-axis, pointing at the matching
        // gmeow:standpoint* individual — the leg the `logic:ruleGateFatalVerdict`
        // up-set rule (and its SHACL projection) reads alongside severity and
        // category. Guarded by Some so a finding without a standpoint leaves the
        // projection byte-unchanged.
        if let Some(standpoint) = finding.standpoint {
            triple(
                &subject,
                &format!("{GMEOW}findingStandpoint"),
                &format!("<{GMEOW}{}>", standpoint.iri_local()),
                &mut lines,
            );
        }
        // The provenance-DAG antecedent edges (gmeow:findingAntecedent): this
        // finding derives FROM each object, keyed on the antecedent's canonical
        // fingerprint IRI — the SAME IRI that antecedent's own subject carries, so
        // the graph closes and the root-cause / cluster meta-rules can walk it.
        for antecedent in &finding.antecedents {
            triple(
                &subject,
                &format!("{GMEOW}findingAntecedent"),
                &format!("<{antecedent}>"),
                &mut lines,
            );
        }
        // The code-blind source anchor (gmeow:findingAnchor) — the cross-node join
        // key two different-code findings at one source position share. Only a
        // NON-TRIVIAL anchor (a real path/focus) is typed gmeow:NonTrivialAnchor,
        // the guard that keeps the cross-node-glut join off the shared empty anchor
        // of locationless findings.
        if let Some(anchor) = &finding.anchor_iri {
            triple(
                &subject,
                &format!("{GMEOW}findingAnchor"),
                &format!("<{anchor}>"),
                &mut lines,
            );
            if finding.anchor_non_trivial {
                triple(
                    &format!("<{anchor}>"),
                    RDF_TYPE,
                    &format!("<{GMEOW}NonTrivialAnchor>"),
                    &mut lines,
                );
            }
        }
        // The registry-authored remediation payload (gmeow:findingRemediation) — the
        // "how to fix" prose, projected verbatim, never fabricated.
        for remediation in &finding.remediation {
            triple(
                &subject,
                &format!("{GMEOW}findingRemediation"),
                &format!("\"{}\"", nq_escape(&remediation.text)),
                &mut lines,
            );
        }
        // Per-term usage guidance (howToUse/useWhen/avoidWhen), joined from the
        // bundle documentation graph — projected verbatim onto its modality's
        // matching gmeow:finding{HowToUse,UseWhen,AvoidWhen} predicate, never
        // fabricated.
        for guidance in &finding.guidance {
            triple(
                &subject,
                &format!("{GMEOW}{}", guidance.modality.predicate_local()),
                &format!("\"{}\"", nq_escape(&guidance.text)),
                &mut lines,
            );
        }
        // The reasoner's explain-skeleton quad-derivation citations
        // (gmeow:findingDerivedFromQuad) — a SEPARATE edge from
        // gmeow:findingAntecedent (finding-to-finding, keyed on fingerprint IRIs):
        // this points at REASONED-QUAD reifier IRIs, so the object is an IRI, never
        // a literal.
        for quad_iri in &finding.derived_from_quads {
            triple(
                &subject,
                &format!("{GMEOW}findingDerivedFromQuad"),
                &format!("<{quad_iri}>"),
                &mut lines,
            );
        }
        // The gate verdict is NOT asserted here. It is a DERIVED predicate: the native
        // reasoner runs logic:ruleGateFatalVerdict (the authored up-set derivation
        // ↑(severityError, blockingBlocking, standpointBinding), reading the three
        // grade-axis triples emitted above) over this finding graph and materializes
        // gmeow:findingGateVerdict gmeow:gateFatal for exactly the up-set findings, into
        // the reasoned closure. This projection emits only the grade coordinates the rule
        // reads; it does not pre-compute the entailment (that would hand-assert a property
        // the ontology defines as reasoner-derived, and would invent a gateCollected value
        // no rule derives — "collected" is the honest ABSENCE of a fatal verdict).
        // Advisory: one triple per suggestion (already sorted+deduped by normalize).
        for suggestion in &finding.suggestions {
            triple(
                &subject,
                &format!("{GMEOW}findingSuggestion"),
                &format!("\"{}\"", nq_escape(suggestion)),
                &mut lines,
            );
        }
        // Advisory: help URI from the rule registry, if present.
        if let Some(rule) = rules.get(finding.code.as_str())
            && let Some(uri) = &rule.help_uri
        {
            triple(
                &subject,
                &format!("{GMEOW}findingHelpUri"),
                &format!("\"{}\"^^<{XSD_ANY_URI}>", nq_escape(uri)),
                &mut lines,
            );
        }
        for (loc_index, location) in finding.locations.iter().enumerate() {
            // An IRI (not a blank node) so the findings graph round-trips
            // through GTS fold without bnode relabeling — required for the
            // feedback bundle's snapshot content id to stay stable.
            let loc_node = format!("<{subject_iri}/location/{loc_index}>");
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
            // The source-text coordinates a span-carrying ingestion adapter recovered
            // (1-based). Carried on the RDF diagnostics surface at full fidelity so the
            // graph/diagnostics fold ships the same source line/column as the SARIF and
            // JSON projections, never a path-only truncation.
            if let Some(v) = location.line {
                int_prop(&loc_node, "findingLocationLine", u64::from(v), &mut lines);
            }
            if let Some(v) = location.column {
                int_prop(&loc_node, "findingLocationColumn", u64::from(v), &mut lines);
            }
        }
        // The text-bearing secondary labels: each rides as a gmeow:relatedLabel
        // node carrying gmeow:labelMessage (the label prose the LSP reads back to
        // emit DiagnosticRelatedInformation) plus its source-location coordinates,
        // mirroring the findingLocation shape above. A finding with no labels emits
        // no such node, so the projection stays byte-unchanged when absent.
        for (label_index, label) in finding.related_labels.iter().enumerate() {
            let label_node = format!("<{subject_iri}/relatedLabel/{label_index}>");
            triple(
                &subject,
                &format!("{GMEOW}relatedLabel"),
                &label_node,
                &mut lines,
            );
            triple(
                &label_node,
                &format!("{GMEOW}labelMessage"),
                &format!("\"{}\"", nq_escape(&label.message)),
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
            let loc = &label.location;
            if let Some(v) = loc.gts_term_id {
                int_prop(&label_node, "gtsTermId", v, &mut lines);
            }
            if let Some(v) = loc.gts_quad_index {
                int_prop(&label_node, "gtsQuadIndex", v, &mut lines);
            }
            if let Some(v) = loc.gts_reifier_id {
                int_prop(&label_node, "gtsReifierId", v, &mut lines);
            }
            if let Some(v) = loc.gts_frame_index {
                int_prop(&label_node, "gtsFrameIndex", v, &mut lines);
            }
            if let Some(v) = loc.gts_segment_index {
                int_prop(&label_node, "gtsSegmentIndex", v, &mut lines);
            }
            if let Some(path) = &loc.path {
                triple(
                    &label_node,
                    &format!("{GMEOW}findingLocationPath"),
                    &format!("\"{}\"", nq_escape(path)),
                    &mut lines,
                );
            }
            if let Some(v) = loc.line {
                int_prop(&label_node, "findingLocationLine", u64::from(v), &mut lines);
            }
            if let Some(v) = loc.column {
                int_prop(
                    &label_node,
                    "findingLocationColumn",
                    u64::from(v),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grade::Standpoint;
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
    /// `finding_iri` — the equality the Task-2 meta-rules match on.
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
        let mut finding =
            Finding::new(Severity::Error, "diag.guided", "boom").with_tool("validate");
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
        // D2c/D2b (Task 2): the three per-term Guidance modalities projected onto
        // `finding.guidance` all render, and each `finding.derived_from_quads`
        // reifier IRI renders a derivation-citation line — a SEPARATE surface from
        // the finding-fingerprint `antecedents`/`root_cause` edges, which this test
        // also asserts stay untouched by populating derived_from_quads.
        use crate::diag::{Guidance, GuidanceModality, GuidanceSource};
        let quad_iri = "https://blackcatinformatics.ca/gmeow/quad/9f2c";
        let mut finding =
            Finding::new(Severity::Error, "diag.justified", "boom").with_tool("validate");
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
}
