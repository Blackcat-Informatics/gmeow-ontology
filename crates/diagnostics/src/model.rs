// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Normalized diagnostic severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Note,
    Info,
}

impl Severity {
    /// Parse a user/tool supplied severity label.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "error" | "failure" | "fatal" | "violation" => Ok(Self::Error),
            "warning" | "warn" => Ok(Self::Warning),
            "note" => Ok(Self::Note),
            "info" | "information" => Ok(Self::Info),
            other => Err(format!(
                "unknown diagnostic severity `{other}`; expected error, warning, note, or info"
            )),
        }
    }

    /// Stable lowercase spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note => "note",
            Self::Info => "info",
        }
    }

    /// SARIF level spelling.
    pub fn sarif_level(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note | Self::Info => "note",
        }
    }

    fn sort_rank(self) -> u8 {
        match self {
            Self::Error => 0,
            Self::Warning => 1,
            Self::Note => 2,
            Self::Info => 3,
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialOrd for Severity {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Severity {
    fn cmp(&self, other: &Self) -> Ordering {
        self.sort_rank().cmp(&other.sort_rank())
    }
}

/// A concrete source or logical location for a diagnostic.
///
/// Beyond the file `path`/`line`/`column` and a free-form `logical` name, a
/// location can carry GTS wire coordinates (mirroring `gmeow_rdf::RdfLocation`):
/// the term-id, quad index, reifier-id, frame index, and segment index that
/// point a finding back into the exact position inside a GTS bundle. These flow
/// from the RDF/GTS adapter through the report into SARIF logical locations and
/// the `gmeow:` RDF projection so every consumer agrees on the same anchor.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Location {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logical: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gts_term_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gts_quad_index: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gts_reifier_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gts_frame_index: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gts_segment_index: Option<u64>,
}

impl Location {
    pub fn new(
        path: Option<String>,
        line: Option<u32>,
        column: Option<u32>,
        logical: Option<String>,
    ) -> Self {
        Self {
            path,
            line,
            column,
            logical,
            ..Self::default()
        }
    }

    pub fn with_gts_term(mut self, term_id: u64) -> Self {
        self.gts_term_id = Some(term_id);
        self
    }

    pub fn with_gts_quad(mut self, quad_index: u64) -> Self {
        self.gts_quad_index = Some(quad_index);
        self
    }

    pub fn with_gts_reifier(mut self, reifier_id: u64) -> Self {
        self.gts_reifier_id = Some(reifier_id);
        self
    }

    pub fn with_gts_frame(mut self, frame_index: u64) -> Self {
        self.gts_frame_index = Some(frame_index);
        self
    }

    pub fn with_gts_segment(mut self, segment_index: u64) -> Self {
        self.gts_segment_index = Some(segment_index);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.path.is_none()
            && self.line.is_none()
            && self.column.is_none()
            && self.logical.is_none()
            && self.gts_term_id.is_none()
            && self.gts_quad_index.is_none()
            && self.gts_reifier_id.is_none()
            && self.gts_frame_index.is_none()
            && self.gts_segment_index.is_none()
    }

    pub fn display(&self) -> String {
        let mut out = self
            .path
            .as_deref()
            .or(self.logical.as_deref())
            .unwrap_or("<unknown>")
            .to_owned();
        if let Some(line) = self.line {
            out.push(':');
            out.push_str(&line.to_string());
            if let Some(column) = self.column {
                out.push(':');
                out.push_str(&column.to_string());
            }
        }
        if let Some(term_id) = self.gts_term_id {
            out.push_str(&format!(" term#{term_id}"));
        }
        if let Some(quad_index) = self.gts_quad_index {
            out.push_str(&format!(" quad#{quad_index}"));
        }
        if let Some(reifier_id) = self.gts_reifier_id {
            out.push_str(&format!(" reifier#{reifier_id}"));
        }
        if let Some(frame_index) = self.gts_frame_index {
            out.push_str(&format!(" frame#{frame_index}"));
        }
        if let Some(segment_index) = self.gts_segment_index {
            out.push_str(&format!(" segment#{segment_index}"));
        }
        out
    }
}

/// Optional rule metadata for a stable diagnostic code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help_uri: Option<String>,
    pub default_severity: Severity,
}

impl Rule {
    pub fn new(id: impl Into<String>, default_severity: Severity) -> Self {
        Self {
            id: id.into(),
            title: None,
            description: None,
            help_uri: None,
            default_severity,
        }
    }
}

/// The role of a compilation unit in a diagnostic attribution (§9 / S5).
///
/// This mirrors `gmeow_rdf::provenance::AttributionRole` but uses owned strings
/// so `gmeow-diagnostics` remains dep-free of `gmeow-rdf` (the layering rule:
/// diagnostics must not import the RDF kernel). The canonical string form is
/// identical to `AttributionRole::as_str()`.
///
/// The `UnitId`→slice-IRI resolution happens in `gmeow-validate` (which has
/// access to both the provenance interners and the diagnostics model); by the
/// time an `Attribution` reaches `DiagnosticAttribution` the IRI is already
/// resolved and the numeric id is discarded (S0.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticAttribution {
    /// The public slice IRI — never a numeric unit id (S0.5).
    pub slice_iri: String,
    /// The role this slice played (mirrors `AttributionRole::as_str()`).
    ///
    /// Known values: `"assertion-origin"`, `"definition-owner"`,
    /// `"shape-owner"`, `"rule-owner"`, `"focus-origin"`, `"value-origin"`,
    /// `"derivation-support"`, `"evaluation-scope"`.
    pub role: String,
    /// Optional provenance note (human-readable; does NOT enter fingerprints).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

/// One normalized diagnostic emitted by any GMEOW tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<Location>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_locations: Vec<Location>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Structured slice attributions for this finding (§9 / S5).
    ///
    /// Records which slices (by public IRI, never numeric id) played which roles
    /// in producing this finding. Empty when no attribution context is available.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributions: Vec<DiagnosticAttribution>,
}

impl Finding {
    pub fn new(severity: Severity, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity,
            code: code.into(),
            message: message.into(),
            tool: None,
            locations: Vec::new(),
            related_locations: Vec::new(),
            suggestions: Vec::new(),
            tags: Vec::new(),
            detail: None,
            attributions: Vec::new(),
        }
    }

    pub fn with_tool(mut self, tool: impl Into<String>) -> Self {
        self.tool = Some(tool.into());
        self
    }

    pub fn add_location(&mut self, location: Location) {
        if !location.is_empty() {
            self.locations.push(location);
        }
    }

    pub fn primary_location(&self) -> Option<&Location> {
        self.locations.first()
    }

    pub fn sort_key(&self) -> (Severity, &str, String, &str) {
        (
            self.severity,
            self.code.as_str(),
            self.primary_location()
                .map(Location::display)
                .unwrap_or_default(),
            self.message.as_str(),
        )
    }

    pub fn normalize(&mut self) {
        self.tags.sort();
        self.tags.dedup();
        self.suggestions.sort();
        self.suggestions.dedup();
        self.locations.sort_by_key(Location::display);
        self.related_locations.sort_by_key(Location::display);
    }
}

/// A complete diagnostics report for one developer tool run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub tool: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<Finding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<Rule>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

impl Report {
    pub fn new(tool: impl Into<String>) -> Self {
        Self {
            tool: tool.into(),
            findings: Vec::new(),
            rules: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }

    pub fn from_legacy(
        tool: impl Into<String>,
        errors: impl IntoIterator<Item = String>,
        warnings: impl IntoIterator<Item = String>,
    ) -> Self {
        let tool = tool.into();
        let mut report = Self::new(tool.clone());
        for message in errors {
            report.add_finding(Finding::new(
                Severity::Error,
                format!("{tool}.error"),
                message,
            ));
        }
        for message in warnings {
            report.add_finding(Finding::new(
                Severity::Warning,
                format!("{tool}.warning"),
                message,
            ));
        }
        report
    }

    pub fn add_finding(&mut self, mut finding: Finding) {
        if finding.tool.is_none() {
            finding.tool = Some(self.tool.clone());
        }
        self.findings.push(finding);
    }

    pub fn add_rule(&mut self, rule: Rule) {
        self.rules.push(rule);
    }

    pub fn ok(&self) -> bool {
        !self.findings.iter().any(|f| f.severity == Severity::Error)
    }

    pub fn error_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .count()
    }

    pub fn warning_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Warning)
            .count()
    }

    /// The number of findings carrying each finding code, keyed by code (sorted).
    ///
    /// The deterministic per-code tally behind the summarized text render and the
    /// recorded coverage-ratchet baselines — a stable, low-cardinality projection
    /// of a report that may hold thousands of per-term findings.
    pub fn counts_by_code(&self) -> BTreeMap<String, usize> {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for finding in &self.findings {
            *counts.entry(finding.code.clone()).or_default() += 1;
        }
        counts
    }

    pub fn normalize(&mut self) {
        for finding in &mut self.findings {
            finding.normalize();
        }
        self.findings
            .sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
        self.rules.sort_by(|a, b| a.id.cmp(&b.id));
        self.rules.dedup_by(|a, b| a.id == b.id);
    }

    pub fn normalized(&self) -> Self {
        let mut clone = self.clone();
        clone.normalize();
        clone
    }

    pub fn legacy_errors(&self) -> Vec<String> {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .map(|f| f.message.clone())
            .collect()
    }

    pub fn legacy_warnings(&self) -> Vec<String> {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Warning)
            .map(|f| f.message.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
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
}
