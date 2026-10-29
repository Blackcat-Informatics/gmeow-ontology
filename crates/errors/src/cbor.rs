// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete diagnostic Report CBOR with a selected closed record schema.
//! Writers retain the existing sparse, declaration-ordered wire. Readers reject
//! unknown and duplicate fields intentionally; they do not normalize findings.

use std::collections::BTreeMap;
use std::fmt;
use std::io::{self, Write};

use purrdf_core::BaseIri;
use purrdf_lex::cbor::{self, Value, head};

use crate::diag::{
    Advice, ArtifactChange, Guidance, GuidanceModality, GuidanceSource, Region, Remediation,
};
use crate::{
    DiagnosticAttribution, Finding, FindingCategory, Location, RelatedLabel, Report, Rule,
    Severity, Standpoint,
};

/// The existing persistent decoder permits 256 nested containers. For this
/// closed wire, typed record/sequence maps and metadata containers account for
/// the same open-container levels as the original Ciborium recursion budget.
/// Enum text keys are shallow and their temporary enum visitor is released
/// before the associated report value is decoded. Tags are not in this schema.
pub const MAX_DIAGNOSTIC_CBOR_DEPTH: usize = 256;

/// A native byte-decoding or typed report-admission failure.
/// Its original native cause remains available through the standard error chain.
#[derive(Debug)]
pub struct DiagnosticCborError {
    message: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl DiagnosticCborError {
    /// Refuse a malformed value in the selected diagnostic record schema.
    pub fn invalid(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }

    /// Retain a native parsing or target-width failure without replacing its cause.
    fn caused_by(
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Attribute a typed refusal to its containing record field.
    fn at(mut self, field: &str) -> Self {
        self.message = format!("{field}: {}", self.message);
        self
    }
}

impl From<cbor::DecodeError> for DiagnosticCborError {
    fn from(error: cbor::DecodeError) -> Self {
        Self::caused_by("native diagnostic CBOR", error)
    }
}

impl fmt::Display for DiagnosticCborError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)?;
        if let Some(source) = &self.source {
            write!(formatter, ": {source}")?;
        }
        Ok(())
    }
}

impl std::error::Error for DiagnosticCborError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|source| source.as_ref() as &(dyn std::error::Error + 'static))
    }
}

/// Admit only a native map; record and metadata owners check its keys separately.
fn read_map(value: &Value) -> Result<&[(Value, Value)], DiagnosticCborError> {
    match value {
        Value::Map(entries) => Ok(entries),
        _ => Err(DiagnosticCborError::invalid("expected diagnostic map")),
    }
}

/// Admit only an array without coercing another CBOR container into a sequence.
fn read_array(value: &Value) -> Result<&[Value], DiagnosticCborError> {
    match value {
        Value::Array(values) => Ok(values),
        _ => Err(DiagnosticCborError::invalid("expected diagnostic array")),
    }
}

/// Borrow a text value without rendering or parsing another value kind.
fn read_text(value: &Value) -> Result<&str, DiagnosticCborError> {
    match value {
        Value::Text(text) => Ok(text),
        _ => Err(DiagnosticCborError::invalid("expected diagnostic text")),
    }
}

/// Admit the original absolute source-graph IRI while retaining its native refusal.
fn read_base_iri(value: &Value) -> Result<BaseIri, DiagnosticCborError> {
    BaseIri::parse(read_text(value)?)
        .map_err(|error| DiagnosticCborError::caused_by("invalid diagnostic source graph", error))
}

/// Admit a nonnegative integer before checking the owning field's target width.
fn read_u64(value: &Value) -> Result<u64, DiagnosticCborError> {
    match value {
        Value::Integer(integer) => u64::try_from(*integer).map_err(|error| {
            DiagnosticCborError::caused_by("expected diagnostic unsigned integer", error)
        }),
        _ => Err(DiagnosticCborError::invalid(
            "expected diagnostic unsigned integer",
        )),
    }
}

/// Admit an actual boolean rather than an integer or textual surrogate.
fn read_bool(value: &Value) -> Result<bool, DiagnosticCborError> {
    match value {
        Value::Bool(value) => Ok(*value),
        _ => Err(DiagnosticCborError::invalid("expected diagnostic boolean")),
    }
}

impl Severity {
    /// Stream the exact existing diagnostic enum spelling.
    ///
    /// # Errors
    /// Returns the sink's original failure.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_text(
            out,
            match self {
                Self::Error => "error",
                Self::Warning => "warning",
                Self::Note => "note",
                Self::Info => "info",
            },
        )
    }
}

/// Admit only the selected Severity wire names.
fn read_severity(value: &Value) -> Result<Severity, DiagnosticCborError> {
    match read_text(value)? {
        "error" => Ok(Severity::Error),
        "warning" => Ok(Severity::Warning),
        "note" => Ok(Severity::Note),
        "info" => Ok(Severity::Info),
        name => Err(DiagnosticCborError::invalid(format!(
            "unknown Severity variant {name}"
        ))),
    }
}

impl FindingCategory {
    /// Stream the exact existing diagnostic enum spelling.
    ///
    /// # Errors
    /// Returns the sink's original failure.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_text(
            out,
            match self {
                Self::DataShapeViolation => "data-shape-violation",
                Self::ModelingDisciplineViolation => "modeling-discipline-violation",
                Self::ContradictionWitness => "contradiction-witness",
                Self::PermittedEpistemicConflict => "permitted-epistemic-conflict",
                Self::UnsupportedSemanticFeature => "unsupported-semantic-feature",
                Self::IncompleteCheck => "incomplete-check",
                Self::ProjectionLoss => "projection-loss",
                Self::PolicyWarning => "policy-warning",
                Self::Corroboration => "corroboration",
                Self::Transient => "transient-chatter",
            },
        )
    }
}

/// Admit only the selected FindingCategory wire names.
fn read_finding_category(value: &Value) -> Result<FindingCategory, DiagnosticCborError> {
    match read_text(value)? {
        "data-shape-violation" => Ok(FindingCategory::DataShapeViolation),
        "modeling-discipline-violation" => Ok(FindingCategory::ModelingDisciplineViolation),
        "contradiction-witness" => Ok(FindingCategory::ContradictionWitness),
        "permitted-epistemic-conflict" => Ok(FindingCategory::PermittedEpistemicConflict),
        "unsupported-semantic-feature" => Ok(FindingCategory::UnsupportedSemanticFeature),
        "incomplete-check" => Ok(FindingCategory::IncompleteCheck),
        "projection-loss" => Ok(FindingCategory::ProjectionLoss),
        "policy-warning" => Ok(FindingCategory::PolicyWarning),
        "corroboration" => Ok(FindingCategory::Corroboration),
        "transient-chatter" => Ok(FindingCategory::Transient),
        name => Err(DiagnosticCborError::invalid(format!(
            "unknown FindingCategory variant {name}"
        ))),
    }
}

impl Standpoint {
    /// Stream the exact existing diagnostic enum spelling.
    ///
    /// # Errors
    /// Returns the sink's original failure.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_text(
            out,
            match self {
                Self::Advisory => "advisory",
                Self::Perspectival => "perspectival",
                Self::Binding => "binding",
            },
        )
    }
}

/// Admit only the selected Standpoint wire names.
fn read_standpoint(value: &Value) -> Result<Standpoint, DiagnosticCborError> {
    match read_text(value)? {
        "advisory" => Ok(Standpoint::Advisory),
        "perspectival" => Ok(Standpoint::Perspectival),
        "binding" => Ok(Standpoint::Binding),
        name => Err(DiagnosticCborError::invalid(format!(
            "unknown Standpoint variant {name}"
        ))),
    }
}

impl GuidanceModality {
    /// Stream the exact existing diagnostic enum spelling.
    ///
    /// # Errors
    /// Returns the sink's original failure.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_text(
            out,
            match self {
                Self::HowToUse => "HowToUse",
                Self::UseWhen => "UseWhen",
                Self::AvoidWhen => "AvoidWhen",
            },
        )
    }
}

/// Admit only the selected GuidanceModality wire names.
fn read_guidance_modality(value: &Value) -> Result<GuidanceModality, DiagnosticCborError> {
    match read_text(value)? {
        "HowToUse" => Ok(GuidanceModality::HowToUse),
        "UseWhen" => Ok(GuidanceModality::UseWhen),
        "AvoidWhen" => Ok(GuidanceModality::AvoidWhen),
        name => Err(DiagnosticCborError::invalid(format!(
            "unknown GuidanceModality variant {name}"
        ))),
    }
}

impl GuidanceSource {
    /// Stream the exact existing diagnostic enum spelling.
    ///
    /// # Errors
    /// Returns the sink's original failure.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        head::write_text(
            out,
            match self {
                Self::RuleGoverningTerm => "RuleGoverningTerm",
                Self::DocumentedTerm => "DocumentedTerm",
            },
        )
    }
}

/// Admit only the selected GuidanceSource wire names.
fn read_guidance_source(value: &Value) -> Result<GuidanceSource, DiagnosticCborError> {
    match read_text(value)? {
        "RuleGoverningTerm" => Ok(GuidanceSource::RuleGoverningTerm),
        "DocumentedTerm" => Ok(GuidanceSource::DocumentedTerm),
        name => Err(DiagnosticCborError::invalid(format!(
            "unknown GuidanceSource variant {name}"
        ))),
    }
}

impl Report {
    /// Stream the complete Report in its original sparse declaration order.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    pub fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        let length = 1_usize
            + usize::from(!self.findings.is_empty())
            + usize::from(!self.rules.is_empty())
            + usize::from(!self.metadata.is_empty());
        head::write_head(out, head::MAP, length as u64)?;
        head::write_text(out, "tool")?;
        head::write_text(out, &self.tool)?;
        if !self.findings.is_empty() {
            head::write_text(out, "findings")?;
            head::write_head(out, head::ARRAY, self.findings.len() as u64)?;
            for value in &self.findings {
                value.write_cbor(out)?;
            }
        }
        if !self.rules.is_empty() {
            head::write_text(out, "rules")?;
            head::write_head(out, head::ARRAY, self.rules.len() as u64)?;
            for value in &self.rules {
                value.write_cbor(out)?;
            }
        }
        if !self.metadata.is_empty() {
            head::write_text(out, "metadata")?;
            write_metadata_map(&self.metadata, out)?;
        }
        Ok(())
    }

    /// Admit the complete selected report schema from an already native CBOR value.
    /// Unknown/duplicate fields and unsupported metadata are intentionally refused.
    /// The publication byte decoder enforces the enclosing 256-container bound.
    ///
    /// # Errors
    /// Returns a typed field refusal, retaining native IRI and width errors.
    pub fn read_cbor(value: &Value) -> Result<Self, DiagnosticCborError> {
        read_report(value)
    }
}

/// Admit the complete Report record without normalization or ignored fields.
fn read_report(value: &Value) -> Result<Report, DiagnosticCborError> {
    let mut tool = None;
    let mut findings = None;
    let mut rules = None;
    let mut metadata = None;
    for (key, value) in read_map(value)? {
        match read_text(key)? {
            "tool" => {
                if tool.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Report.tool"));
                }
                tool = Some(
                    (read_text(value).map(str::to_owned))
                        .map_err(|error| error.at("Report.tool"))?,
                );
            }
            "findings" => {
                if findings.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Report.findings"));
                }
                findings = Some(
                    (read_array(value).and_then(|values| {
                        values
                            .iter()
                            .map(read_finding)
                            .collect::<Result<Vec<_>, _>>()
                    }))
                    .map_err(|error| error.at("Report.findings"))?,
                );
            }
            "rules" => {
                if rules.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Report.rules"));
                }
                rules = Some(
                    (read_array(value).and_then(|values| {
                        values.iter().map(read_rule).collect::<Result<Vec<_>, _>>()
                    }))
                    .map_err(|error| error.at("Report.rules"))?,
                );
            }
            "metadata" => {
                if metadata.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Report.metadata"));
                }
                metadata =
                    Some((read_metadata_map(value)).map_err(|error| error.at("Report.metadata"))?);
            }
            name => {
                return Err(DiagnosticCborError::invalid(format!(
                    "unknown Report field {name}"
                )));
            }
        }
    }
    Ok(Report {
        tool: tool.ok_or_else(|| DiagnosticCborError::invalid("missing Report.tool"))?,
        findings: findings.unwrap_or_default(),
        rules: rules.unwrap_or_default(),
        metadata: metadata.unwrap_or_default(),
    })
}

impl Finding {
    /// Stream the complete Finding in its original sparse declaration order.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        let length = 3_usize
            + usize::from(self.source_graph.is_some())
            + usize::from(self.source_focus.is_some())
            + usize::from(self.tool.is_some())
            + usize::from(!self.locations.is_empty())
            + usize::from(!self.related_locations.is_empty())
            + usize::from(!self.related_labels.is_empty())
            + usize::from(!self.suggestions.is_empty())
            + usize::from(!self.advice.is_empty())
            + usize::from(!self.remediation.is_empty())
            + usize::from(self.finding_iri.is_some())
            + usize::from(self.anchor_iri.is_some())
            + usize::from(self.anchor_non_trivial)
            + usize::from(!self.antecedents.is_empty())
            + usize::from(self.root_cause.is_some())
            + usize::from(self.cluster.is_some())
            + usize::from(!self.cross_node_glut_with.is_empty())
            + usize::from(!self.tags.is_empty())
            + usize::from(self.detail.is_some())
            + usize::from(self.failure_class.is_some())
            + usize::from(self.category.is_some())
            + usize::from(self.standpoint.is_some())
            + usize::from(!self.attributions.is_empty())
            + usize::from(!self.documented_terms.is_empty())
            + usize::from(!self.guidance.is_empty())
            + usize::from(!self.derived_from_quads.is_empty())
            + usize::from(!self.cited_iris.is_empty());
        head::write_head(out, head::MAP, length as u64)?;
        head::write_text(out, "severity")?;
        self.severity.write_cbor(out)?;
        head::write_text(out, "code")?;
        head::write_text(out, &self.code)?;
        head::write_text(out, "message")?;
        head::write_text(out, &self.message)?;
        if let Some(value) = &self.source_graph {
            head::write_text(out, "source_graph")?;
            head::write_text(out, value.as_str())?;
        }
        if let Some(value) = &self.source_focus {
            head::write_text(out, "source_focus")?;
            head::write_text(out, value)?;
        }
        if let Some(value) = &self.tool {
            head::write_text(out, "tool")?;
            head::write_text(out, value)?;
        }
        if !self.locations.is_empty() {
            head::write_text(out, "locations")?;
            head::write_head(out, head::ARRAY, self.locations.len() as u64)?;
            for value in &self.locations {
                value.write_cbor(out)?;
            }
        }
        if !self.related_locations.is_empty() {
            head::write_text(out, "related_locations")?;
            head::write_head(out, head::ARRAY, self.related_locations.len() as u64)?;
            for value in &self.related_locations {
                value.write_cbor(out)?;
            }
        }
        if !self.related_labels.is_empty() {
            head::write_text(out, "related_labels")?;
            head::write_head(out, head::ARRAY, self.related_labels.len() as u64)?;
            for value in &self.related_labels {
                value.write_cbor(out)?;
            }
        }
        if !self.suggestions.is_empty() {
            head::write_text(out, "suggestions")?;
            head::write_head(out, head::ARRAY, self.suggestions.len() as u64)?;
            for value in &self.suggestions {
                head::write_text(out, value)?;
            }
        }
        if !self.advice.is_empty() {
            head::write_text(out, "advice")?;
            head::write_head(out, head::ARRAY, self.advice.len() as u64)?;
            for value in &self.advice {
                value.write_cbor(out)?;
            }
        }
        if !self.remediation.is_empty() {
            head::write_text(out, "remediation")?;
            head::write_head(out, head::ARRAY, self.remediation.len() as u64)?;
            for value in &self.remediation {
                value.write_cbor(out)?;
            }
        }
        if let Some(value) = &self.finding_iri {
            head::write_text(out, "finding_iri")?;
            head::write_text(out, value)?;
        }
        if let Some(value) = &self.anchor_iri {
            head::write_text(out, "anchor_iri")?;
            head::write_text(out, value)?;
        }
        if self.anchor_non_trivial {
            head::write_text(out, "anchor_non_trivial")?;
            cbor::write(&Value::Bool(self.anchor_non_trivial), out)?;
        }
        if !self.antecedents.is_empty() {
            head::write_text(out, "antecedents")?;
            head::write_head(out, head::ARRAY, self.antecedents.len() as u64)?;
            for value in &self.antecedents {
                head::write_text(out, value)?;
            }
        }
        if let Some(value) = &self.root_cause {
            head::write_text(out, "root_cause")?;
            head::write_text(out, value)?;
        }
        if let Some(value) = &self.cluster {
            head::write_text(out, "cluster")?;
            head::write_text(out, value)?;
        }
        if !self.cross_node_glut_with.is_empty() {
            head::write_text(out, "cross_node_glut_with")?;
            head::write_head(out, head::ARRAY, self.cross_node_glut_with.len() as u64)?;
            for value in &self.cross_node_glut_with {
                head::write_text(out, value)?;
            }
        }
        if !self.tags.is_empty() {
            head::write_text(out, "tags")?;
            head::write_head(out, head::ARRAY, self.tags.len() as u64)?;
            for value in &self.tags {
                head::write_text(out, value)?;
            }
        }
        if let Some(value) = &self.detail {
            head::write_text(out, "detail")?;
            head::write_text(out, value)?;
        }
        if let Some(value) = &self.failure_class {
            head::write_text(out, "failure_class")?;
            head::write_text(out, value)?;
        }
        if let Some(value) = &self.category {
            head::write_text(out, "category")?;
            value.write_cbor(out)?;
        }
        if let Some(value) = &self.standpoint {
            head::write_text(out, "standpoint")?;
            value.write_cbor(out)?;
        }
        if !self.attributions.is_empty() {
            head::write_text(out, "attributions")?;
            head::write_head(out, head::ARRAY, self.attributions.len() as u64)?;
            for value in &self.attributions {
                value.write_cbor(out)?;
            }
        }
        if !self.documented_terms.is_empty() {
            head::write_text(out, "documented_terms")?;
            head::write_head(out, head::ARRAY, self.documented_terms.len() as u64)?;
            for value in &self.documented_terms {
                head::write_text(out, value)?;
            }
        }
        if !self.guidance.is_empty() {
            head::write_text(out, "guidance")?;
            head::write_head(out, head::ARRAY, self.guidance.len() as u64)?;
            for value in &self.guidance {
                value.write_cbor(out)?;
            }
        }
        if !self.derived_from_quads.is_empty() {
            head::write_text(out, "derived_from_quads")?;
            head::write_head(out, head::ARRAY, self.derived_from_quads.len() as u64)?;
            for value in &self.derived_from_quads {
                head::write_text(out, value)?;
            }
        }
        if !self.cited_iris.is_empty() {
            head::write_text(out, "cited_iris")?;
            head::write_head(out, head::ARRAY, self.cited_iris.len() as u64)?;
            for value in &self.cited_iris {
                head::write_text(out, value)?;
            }
        }
        Ok(())
    }
}

/// Admit the complete Finding record without normalization or ignored fields.
fn read_finding(value: &Value) -> Result<Finding, DiagnosticCborError> {
    let mut severity = None;
    let mut code = None;
    let mut message = None;
    let mut source_graph = None;
    let mut source_focus = None;
    let mut tool = None;
    let mut locations = None;
    let mut related_locations = None;
    let mut related_labels = None;
    let mut suggestions = None;
    let mut advice = None;
    let mut remediation = None;
    let mut finding_iri = None;
    let mut anchor_iri = None;
    let mut anchor_non_trivial = None;
    let mut antecedents = None;
    let mut root_cause = None;
    let mut cluster = None;
    let mut cross_node_glut_with = None;
    let mut tags = None;
    let mut detail = None;
    let mut failure_class = None;
    let mut category = None;
    let mut standpoint = None;
    let mut attributions = None;
    let mut documented_terms = None;
    let mut guidance = None;
    let mut derived_from_quads = None;
    let mut cited_iris = None;
    for (key, value) in read_map(value)? {
        match read_text(key)? {
            "severity" => {
                if severity.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Finding.severity"));
                }
                severity =
                    Some((read_severity(value)).map_err(|error| error.at("Finding.severity"))?);
            }
            "code" => {
                if code.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Finding.code"));
                }
                code = Some(
                    (read_text(value).map(str::to_owned))
                        .map_err(|error| error.at("Finding.code"))?,
                );
            }
            "message" => {
                if message.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Finding.message"));
                }
                message = Some(
                    (read_text(value).map(str::to_owned))
                        .map_err(|error| error.at("Finding.message"))?,
                );
            }
            "source_graph" => {
                if source_graph.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate Finding.source_graph",
                    ));
                }
                source_graph = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_base_iri(value)).map(Some)
                    })
                    .map_err(|error| error.at("Finding.source_graph"))?,
                );
            }
            "source_focus" => {
                if source_focus.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate Finding.source_focus",
                    ));
                }
                source_focus = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_text(value).map(str::to_owned)).map(Some)
                    })
                    .map_err(|error| error.at("Finding.source_focus"))?,
                );
            }
            "tool" => {
                if tool.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Finding.tool"));
                }
                tool = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_text(value).map(str::to_owned)).map(Some)
                    })
                    .map_err(|error| error.at("Finding.tool"))?,
                );
            }
            "locations" => {
                if locations.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Finding.locations"));
                }
                locations = Some(
                    (read_array(value).and_then(|values| {
                        values
                            .iter()
                            .map(read_location)
                            .collect::<Result<Vec<_>, _>>()
                    }))
                    .map_err(|error| error.at("Finding.locations"))?,
                );
            }
            "related_locations" => {
                if related_locations.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate Finding.related_locations",
                    ));
                }
                related_locations = Some(
                    (read_array(value).and_then(|values| {
                        values
                            .iter()
                            .map(read_location)
                            .collect::<Result<Vec<_>, _>>()
                    }))
                    .map_err(|error| error.at("Finding.related_locations"))?,
                );
            }
            "related_labels" => {
                if related_labels.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate Finding.related_labels",
                    ));
                }
                related_labels = Some(
                    (read_array(value).and_then(|values| {
                        values
                            .iter()
                            .map(read_related_label)
                            .collect::<Result<Vec<_>, _>>()
                    }))
                    .map_err(|error| error.at("Finding.related_labels"))?,
                );
            }
            "suggestions" => {
                if suggestions.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate Finding.suggestions",
                    ));
                }
                suggestions = Some(
                    (read_array(value).and_then(|values| {
                        values
                            .iter()
                            .map(|value| read_text(value).map(str::to_owned))
                            .collect::<Result<Vec<_>, _>>()
                    }))
                    .map_err(|error| error.at("Finding.suggestions"))?,
                );
            }
            "advice" => {
                if advice.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Finding.advice"));
                }
                advice = Some(
                    (read_array(value).and_then(|values| {
                        values
                            .iter()
                            .map(read_advice)
                            .collect::<Result<Vec<_>, _>>()
                    }))
                    .map_err(|error| error.at("Finding.advice"))?,
                );
            }
            "remediation" => {
                if remediation.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate Finding.remediation",
                    ));
                }
                remediation = Some(
                    (read_array(value).and_then(|values| {
                        values
                            .iter()
                            .map(read_remediation)
                            .collect::<Result<Vec<_>, _>>()
                    }))
                    .map_err(|error| error.at("Finding.remediation"))?,
                );
            }
            "finding_iri" => {
                if finding_iri.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate Finding.finding_iri",
                    ));
                }
                finding_iri = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_text(value).map(str::to_owned)).map(Some)
                    })
                    .map_err(|error| error.at("Finding.finding_iri"))?,
                );
            }
            "anchor_iri" => {
                if anchor_iri.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Finding.anchor_iri"));
                }
                anchor_iri = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_text(value).map(str::to_owned)).map(Some)
                    })
                    .map_err(|error| error.at("Finding.anchor_iri"))?,
                );
            }
            "anchor_non_trivial" => {
                if anchor_non_trivial.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate Finding.anchor_non_trivial",
                    ));
                }
                anchor_non_trivial = Some(
                    (read_bool(value)).map_err(|error| error.at("Finding.anchor_non_trivial"))?,
                );
            }
            "antecedents" => {
                if antecedents.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate Finding.antecedents",
                    ));
                }
                antecedents = Some(
                    (read_array(value).and_then(|values| {
                        values
                            .iter()
                            .map(|value| read_text(value).map(str::to_owned))
                            .collect::<Result<Vec<_>, _>>()
                    }))
                    .map_err(|error| error.at("Finding.antecedents"))?,
                );
            }
            "root_cause" => {
                if root_cause.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Finding.root_cause"));
                }
                root_cause = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_text(value).map(str::to_owned)).map(Some)
                    })
                    .map_err(|error| error.at("Finding.root_cause"))?,
                );
            }
            "cluster" => {
                if cluster.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Finding.cluster"));
                }
                cluster = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_text(value).map(str::to_owned)).map(Some)
                    })
                    .map_err(|error| error.at("Finding.cluster"))?,
                );
            }
            "cross_node_glut_with" => {
                if cross_node_glut_with.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate Finding.cross_node_glut_with",
                    ));
                }
                cross_node_glut_with = Some(
                    (read_array(value).and_then(|values| {
                        values
                            .iter()
                            .map(|value| read_text(value).map(str::to_owned))
                            .collect::<Result<Vec<_>, _>>()
                    }))
                    .map_err(|error| error.at("Finding.cross_node_glut_with"))?,
                );
            }
            "tags" => {
                if tags.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Finding.tags"));
                }
                tags = Some(
                    (read_array(value).and_then(|values| {
                        values
                            .iter()
                            .map(|value| read_text(value).map(str::to_owned))
                            .collect::<Result<Vec<_>, _>>()
                    }))
                    .map_err(|error| error.at("Finding.tags"))?,
                );
            }
            "detail" => {
                if detail.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Finding.detail"));
                }
                detail = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_text(value).map(str::to_owned)).map(Some)
                    })
                    .map_err(|error| error.at("Finding.detail"))?,
                );
            }
            "failure_class" => {
                if failure_class.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate Finding.failure_class",
                    ));
                }
                failure_class = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_text(value).map(str::to_owned)).map(Some)
                    })
                    .map_err(|error| error.at("Finding.failure_class"))?,
                );
            }
            "category" => {
                if category.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Finding.category"));
                }
                category = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_finding_category(value)).map(Some)
                    })
                    .map_err(|error| error.at("Finding.category"))?,
                );
            }
            "standpoint" => {
                if standpoint.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Finding.standpoint"));
                }
                standpoint = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_standpoint(value)).map(Some)
                    })
                    .map_err(|error| error.at("Finding.standpoint"))?,
                );
            }
            "attributions" => {
                if attributions.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate Finding.attributions",
                    ));
                }
                attributions = Some(
                    (read_array(value).and_then(|values| {
                        values
                            .iter()
                            .map(read_diagnostic_attribution)
                            .collect::<Result<Vec<_>, _>>()
                    }))
                    .map_err(|error| error.at("Finding.attributions"))?,
                );
            }
            "documented_terms" => {
                if documented_terms.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate Finding.documented_terms",
                    ));
                }
                documented_terms = Some(
                    (read_array(value).and_then(|values| {
                        values
                            .iter()
                            .map(|value| read_text(value).map(str::to_owned))
                            .collect::<Result<Vec<_>, _>>()
                    }))
                    .map_err(|error| error.at("Finding.documented_terms"))?,
                );
            }
            "guidance" => {
                if guidance.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Finding.guidance"));
                }
                guidance = Some(
                    (read_array(value).and_then(|values| {
                        values
                            .iter()
                            .map(read_guidance)
                            .collect::<Result<Vec<_>, _>>()
                    }))
                    .map_err(|error| error.at("Finding.guidance"))?,
                );
            }
            "derived_from_quads" => {
                if derived_from_quads.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate Finding.derived_from_quads",
                    ));
                }
                derived_from_quads = Some(
                    (read_array(value).and_then(|values| {
                        values
                            .iter()
                            .map(|value| read_text(value).map(str::to_owned))
                            .collect::<Result<Vec<_>, _>>()
                    }))
                    .map_err(|error| error.at("Finding.derived_from_quads"))?,
                );
            }
            "cited_iris" => {
                if cited_iris.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Finding.cited_iris"));
                }
                cited_iris = Some(
                    (read_array(value).and_then(|values| {
                        values
                            .iter()
                            .map(|value| read_text(value).map(str::to_owned))
                            .collect::<Result<Vec<_>, _>>()
                    }))
                    .map_err(|error| error.at("Finding.cited_iris"))?,
                );
            }
            name => {
                return Err(DiagnosticCborError::invalid(format!(
                    "unknown Finding field {name}"
                )));
            }
        }
    }
    Ok(Finding {
        severity: severity
            .ok_or_else(|| DiagnosticCborError::invalid("missing Finding.severity"))?,
        code: code.ok_or_else(|| DiagnosticCborError::invalid("missing Finding.code"))?,
        message: message.ok_or_else(|| DiagnosticCborError::invalid("missing Finding.message"))?,
        source_graph: source_graph.unwrap_or_default(),
        source_focus: source_focus.unwrap_or_default(),
        tool: tool.unwrap_or_default(),
        locations: locations.unwrap_or_default(),
        related_locations: related_locations.unwrap_or_default(),
        related_labels: related_labels.unwrap_or_default(),
        suggestions: suggestions.unwrap_or_default(),
        advice: advice.unwrap_or_default(),
        remediation: remediation.unwrap_or_default(),
        finding_iri: finding_iri.unwrap_or_default(),
        anchor_iri: anchor_iri.unwrap_or_default(),
        anchor_non_trivial: anchor_non_trivial.unwrap_or_default(),
        antecedents: antecedents.unwrap_or_default(),
        root_cause: root_cause.unwrap_or_default(),
        cluster: cluster.unwrap_or_default(),
        cross_node_glut_with: cross_node_glut_with.unwrap_or_default(),
        tags: tags.unwrap_or_default(),
        detail: detail.unwrap_or_default(),
        failure_class: failure_class.unwrap_or_default(),
        category: category.unwrap_or_default(),
        standpoint: standpoint.unwrap_or_default(),
        attributions: attributions.unwrap_or_default(),
        documented_terms: documented_terms.unwrap_or_default(),
        guidance: guidance.unwrap_or_default(),
        derived_from_quads: derived_from_quads.unwrap_or_default(),
        cited_iris: cited_iris.unwrap_or_default(),
    })
}

impl Location {
    /// Stream the complete Location in its original sparse declaration order.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        let length = usize::from(self.path.is_some())
            + usize::from(self.line.is_some())
            + usize::from(self.column.is_some())
            + usize::from(self.logical.is_some())
            + usize::from(self.gts_term_id.is_some())
            + usize::from(self.gts_quad_index.is_some())
            + usize::from(self.gts_reifier_id.is_some())
            + usize::from(self.gts_frame_index.is_some())
            + usize::from(self.gts_segment_index.is_some());
        head::write_head(out, head::MAP, length as u64)?;
        if let Some(value) = &self.path {
            head::write_text(out, "path")?;
            head::write_text(out, value)?;
        }
        if let Some(value) = &self.line {
            head::write_text(out, "line")?;
            head::write_head(out, head::UNSIGNED, *value)?;
        }
        if let Some(value) = &self.column {
            head::write_text(out, "column")?;
            head::write_head(out, head::UNSIGNED, u64::from(*value))?;
        }
        if let Some(value) = &self.logical {
            head::write_text(out, "logical")?;
            head::write_text(out, value)?;
        }
        if let Some(value) = &self.gts_term_id {
            head::write_text(out, "gts_term_id")?;
            head::write_head(out, head::UNSIGNED, *value)?;
        }
        if let Some(value) = &self.gts_quad_index {
            head::write_text(out, "gts_quad_index")?;
            head::write_head(out, head::UNSIGNED, *value)?;
        }
        if let Some(value) = &self.gts_reifier_id {
            head::write_text(out, "gts_reifier_id")?;
            head::write_head(out, head::UNSIGNED, *value)?;
        }
        if let Some(value) = &self.gts_frame_index {
            head::write_text(out, "gts_frame_index")?;
            head::write_head(out, head::UNSIGNED, *value)?;
        }
        if let Some(value) = &self.gts_segment_index {
            head::write_text(out, "gts_segment_index")?;
            head::write_head(out, head::UNSIGNED, *value)?;
        }
        Ok(())
    }
}

/// Admit the complete Location record without normalization or ignored fields.
fn read_location(value: &Value) -> Result<Location, DiagnosticCborError> {
    let mut path = None;
    let mut line = None;
    let mut column = None;
    let mut logical = None;
    let mut gts_term_id = None;
    let mut gts_quad_index = None;
    let mut gts_reifier_id = None;
    let mut gts_frame_index = None;
    let mut gts_segment_index = None;
    for (key, value) in read_map(value)? {
        match read_text(key)? {
            "path" => {
                if path.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Location.path"));
                }
                path = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_text(value).map(str::to_owned)).map(Some)
                    })
                    .map_err(|error| error.at("Location.path"))?,
                );
            }
            "line" => {
                if line.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Location.line"));
                }
                line = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_u64(value)).map(Some)
                    })
                    .map_err(|error| error.at("Location.line"))?,
                );
            }
            "column" => {
                if column.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Location.column"));
                }
                column = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_u64(value).and_then(|value| {
                            u32::try_from(value).map_err(|error| {
                                DiagnosticCborError::caused_by(
                                    "diagnostic coordinate exceeds u32",
                                    error,
                                )
                            })
                        }))
                        .map(Some)
                    })
                    .map_err(|error| error.at("Location.column"))?,
                );
            }
            "logical" => {
                if logical.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Location.logical"));
                }
                logical = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_text(value).map(str::to_owned)).map(Some)
                    })
                    .map_err(|error| error.at("Location.logical"))?,
                );
            }
            "gts_term_id" => {
                if gts_term_id.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate Location.gts_term_id",
                    ));
                }
                gts_term_id = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_u64(value)).map(Some)
                    })
                    .map_err(|error| error.at("Location.gts_term_id"))?,
                );
            }
            "gts_quad_index" => {
                if gts_quad_index.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate Location.gts_quad_index",
                    ));
                }
                gts_quad_index = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_u64(value)).map(Some)
                    })
                    .map_err(|error| error.at("Location.gts_quad_index"))?,
                );
            }
            "gts_reifier_id" => {
                if gts_reifier_id.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate Location.gts_reifier_id",
                    ));
                }
                gts_reifier_id = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_u64(value)).map(Some)
                    })
                    .map_err(|error| error.at("Location.gts_reifier_id"))?,
                );
            }
            "gts_frame_index" => {
                if gts_frame_index.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate Location.gts_frame_index",
                    ));
                }
                gts_frame_index = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_u64(value)).map(Some)
                    })
                    .map_err(|error| error.at("Location.gts_frame_index"))?,
                );
            }
            "gts_segment_index" => {
                if gts_segment_index.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate Location.gts_segment_index",
                    ));
                }
                gts_segment_index = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_u64(value)).map(Some)
                    })
                    .map_err(|error| error.at("Location.gts_segment_index"))?,
                );
            }
            name => {
                return Err(DiagnosticCborError::invalid(format!(
                    "unknown Location field {name}"
                )));
            }
        }
    }
    Ok(Location {
        path: path.unwrap_or_default(),
        line: line.unwrap_or_default(),
        column: column.unwrap_or_default(),
        logical: logical.unwrap_or_default(),
        gts_term_id: gts_term_id.unwrap_or_default(),
        gts_quad_index: gts_quad_index.unwrap_or_default(),
        gts_reifier_id: gts_reifier_id.unwrap_or_default(),
        gts_frame_index: gts_frame_index.unwrap_or_default(),
        gts_segment_index: gts_segment_index.unwrap_or_default(),
    })
}

impl Rule {
    /// Stream the complete Rule in its original sparse declaration order.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        let length = 2_usize
            + usize::from(self.title.is_some())
            + usize::from(self.description.is_some())
            + usize::from(self.help_uri.is_some())
            + usize::from(self.remediation.is_some())
            + usize::from(self.how_to_use.is_some());
        head::write_head(out, head::MAP, length as u64)?;
        head::write_text(out, "id")?;
        head::write_text(out, &self.id)?;
        if let Some(value) = &self.title {
            head::write_text(out, "title")?;
            head::write_text(out, value)?;
        }
        if let Some(value) = &self.description {
            head::write_text(out, "description")?;
            head::write_text(out, value)?;
        }
        if let Some(value) = &self.help_uri {
            head::write_text(out, "help_uri")?;
            head::write_text(out, value)?;
        }
        if let Some(value) = &self.remediation {
            head::write_text(out, "remediation")?;
            head::write_text(out, value)?;
        }
        if let Some(value) = &self.how_to_use {
            head::write_text(out, "how_to_use")?;
            head::write_text(out, value)?;
        }
        head::write_text(out, "default_severity")?;
        self.default_severity.write_cbor(out)?;
        Ok(())
    }
}

/// Admit the complete Rule record without normalization or ignored fields.
fn read_rule(value: &Value) -> Result<Rule, DiagnosticCborError> {
    let mut id = None;
    let mut title = None;
    let mut description = None;
    let mut help_uri = None;
    let mut remediation = None;
    let mut how_to_use = None;
    let mut default_severity = None;
    for (key, value) in read_map(value)? {
        match read_text(key)? {
            "id" => {
                if id.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Rule.id"));
                }
                id = Some(
                    (read_text(value).map(str::to_owned)).map_err(|error| error.at("Rule.id"))?,
                );
            }
            "title" => {
                if title.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Rule.title"));
                }
                title = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_text(value).map(str::to_owned)).map(Some)
                    })
                    .map_err(|error| error.at("Rule.title"))?,
                );
            }
            "description" => {
                if description.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Rule.description"));
                }
                description = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_text(value).map(str::to_owned)).map(Some)
                    })
                    .map_err(|error| error.at("Rule.description"))?,
                );
            }
            "help_uri" => {
                if help_uri.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Rule.help_uri"));
                }
                help_uri = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_text(value).map(str::to_owned)).map(Some)
                    })
                    .map_err(|error| error.at("Rule.help_uri"))?,
                );
            }
            "remediation" => {
                if remediation.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Rule.remediation"));
                }
                remediation = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_text(value).map(str::to_owned)).map(Some)
                    })
                    .map_err(|error| error.at("Rule.remediation"))?,
                );
            }
            "how_to_use" => {
                if how_to_use.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Rule.how_to_use"));
                }
                how_to_use = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_text(value).map(str::to_owned)).map(Some)
                    })
                    .map_err(|error| error.at("Rule.how_to_use"))?,
                );
            }
            "default_severity" => {
                if default_severity.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate Rule.default_severity",
                    ));
                }
                default_severity = Some(
                    (read_severity(value)).map_err(|error| error.at("Rule.default_severity"))?,
                );
            }
            name => {
                return Err(DiagnosticCborError::invalid(format!(
                    "unknown Rule field {name}"
                )));
            }
        }
    }
    Ok(Rule {
        id: id.ok_or_else(|| DiagnosticCborError::invalid("missing Rule.id"))?,
        title: title.unwrap_or_default(),
        description: description.unwrap_or_default(),
        help_uri: help_uri.unwrap_or_default(),
        remediation: remediation.unwrap_or_default(),
        how_to_use: how_to_use.unwrap_or_default(),
        default_severity: default_severity
            .ok_or_else(|| DiagnosticCborError::invalid("missing Rule.default_severity"))?,
    })
}

impl RelatedLabel {
    /// Stream the complete RelatedLabel in its original sparse declaration order.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        let length = 2_usize;
        head::write_head(out, head::MAP, length as u64)?;
        head::write_text(out, "location")?;
        self.location.write_cbor(out)?;
        head::write_text(out, "message")?;
        head::write_text(out, &self.message)?;
        Ok(())
    }
}

/// Admit the complete RelatedLabel record without normalization or ignored fields.
fn read_related_label(value: &Value) -> Result<RelatedLabel, DiagnosticCborError> {
    let mut location = None;
    let mut message = None;
    for (key, value) in read_map(value)? {
        match read_text(key)? {
            "location" => {
                if location.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate RelatedLabel.location",
                    ));
                }
                location = Some(
                    (read_location(value)).map_err(|error| error.at("RelatedLabel.location"))?,
                );
            }
            "message" => {
                if message.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate RelatedLabel.message",
                    ));
                }
                message = Some(
                    (read_text(value).map(str::to_owned))
                        .map_err(|error| error.at("RelatedLabel.message"))?,
                );
            }
            name => {
                return Err(DiagnosticCborError::invalid(format!(
                    "unknown RelatedLabel field {name}"
                )));
            }
        }
    }
    Ok(RelatedLabel {
        location: location
            .ok_or_else(|| DiagnosticCborError::invalid("missing RelatedLabel.location"))?,
        message: message
            .ok_or_else(|| DiagnosticCborError::invalid("missing RelatedLabel.message"))?,
    })
}

impl DiagnosticAttribution {
    /// Stream the complete DiagnosticAttribution in its original sparse declaration order.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        let length = 2_usize + usize::from(self.evidence.is_some());
        head::write_head(out, head::MAP, length as u64)?;
        head::write_text(out, "slice_iri")?;
        head::write_text(out, &self.slice_iri)?;
        head::write_text(out, "role")?;
        head::write_text(out, &self.role)?;
        if let Some(value) = &self.evidence {
            head::write_text(out, "evidence")?;
            head::write_text(out, value)?;
        }
        Ok(())
    }
}

/// Admit the complete DiagnosticAttribution record without normalization or ignored fields.
fn read_diagnostic_attribution(
    value: &Value,
) -> Result<DiagnosticAttribution, DiagnosticCborError> {
    let mut slice_iri = None;
    let mut role = None;
    let mut evidence = None;
    for (key, value) in read_map(value)? {
        match read_text(key)? {
            "slice_iri" => {
                if slice_iri.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate DiagnosticAttribution.slice_iri",
                    ));
                }
                slice_iri = Some(
                    (read_text(value).map(str::to_owned))
                        .map_err(|error| error.at("DiagnosticAttribution.slice_iri"))?,
                );
            }
            "role" => {
                if role.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate DiagnosticAttribution.role",
                    ));
                }
                role = Some(
                    (read_text(value).map(str::to_owned))
                        .map_err(|error| error.at("DiagnosticAttribution.role"))?,
                );
            }
            "evidence" => {
                if evidence.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate DiagnosticAttribution.evidence",
                    ));
                }
                evidence = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_text(value).map(str::to_owned)).map(Some)
                    })
                    .map_err(|error| error.at("DiagnosticAttribution.evidence"))?,
                );
            }
            name => {
                return Err(DiagnosticCborError::invalid(format!(
                    "unknown DiagnosticAttribution field {name}"
                )));
            }
        }
    }
    Ok(DiagnosticAttribution {
        slice_iri: slice_iri.ok_or_else(|| {
            DiagnosticCborError::invalid("missing DiagnosticAttribution.slice_iri")
        })?,
        role: role
            .ok_or_else(|| DiagnosticCborError::invalid("missing DiagnosticAttribution.role"))?,
        evidence: evidence.unwrap_or_default(),
    })
}

impl Advice {
    /// Stream the complete Advice in its original sparse declaration order.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        let length = 3_usize;
        head::write_head(out, head::MAP, length as u64)?;
        head::write_text(out, "standpoint")?;
        self.standpoint.write_cbor(out)?;
        head::write_text(out, "text")?;
        head::write_text(out, &self.text)?;
        head::write_text(out, "help_uri")?;
        match &self.help_uri {
            Some(value) => {
                head::write_text(out, value)?;
            }
            None => cbor::write(&Value::Null, out)?,
        }
        Ok(())
    }
}

/// Admit the complete Advice record without normalization or ignored fields.
fn read_advice(value: &Value) -> Result<Advice, DiagnosticCborError> {
    let mut standpoint = None;
    let mut text = None;
    let mut help_uri = None;
    for (key, value) in read_map(value)? {
        match read_text(key)? {
            "standpoint" => {
                if standpoint.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Advice.standpoint"));
                }
                standpoint =
                    Some((read_standpoint(value)).map_err(|error| error.at("Advice.standpoint"))?);
            }
            "text" => {
                if text.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Advice.text"));
                }
                text = Some(
                    (read_text(value).map(str::to_owned))
                        .map_err(|error| error.at("Advice.text"))?,
                );
            }
            "help_uri" => {
                if help_uri.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Advice.help_uri"));
                }
                help_uri = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_text(value).map(str::to_owned)).map(Some)
                    })
                    .map_err(|error| error.at("Advice.help_uri"))?,
                );
            }
            name => {
                return Err(DiagnosticCborError::invalid(format!(
                    "unknown Advice field {name}"
                )));
            }
        }
    }
    Ok(Advice {
        standpoint: standpoint
            .ok_or_else(|| DiagnosticCborError::invalid("missing Advice.standpoint"))?,
        text: text.ok_or_else(|| DiagnosticCborError::invalid("missing Advice.text"))?,
        help_uri: help_uri.unwrap_or_default(),
    })
}

impl Remediation {
    /// Stream the complete Remediation in its original sparse declaration order.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        let length = 2_usize
            + usize::from(self.help_uri.is_some())
            + usize::from(self.artifact_change.is_some());
        head::write_head(out, head::MAP, length as u64)?;
        head::write_text(out, "text")?;
        head::write_text(out, &self.text)?;
        head::write_text(out, "standpoint")?;
        self.standpoint.write_cbor(out)?;
        if let Some(value) = &self.help_uri {
            head::write_text(out, "help_uri")?;
            head::write_text(out, value)?;
        }
        if let Some(value) = &self.artifact_change {
            head::write_text(out, "artifact_change")?;
            value.write_cbor(out)?;
        }
        Ok(())
    }
}

/// Admit the complete Remediation record without normalization or ignored fields.
fn read_remediation(value: &Value) -> Result<Remediation, DiagnosticCborError> {
    let mut text = None;
    let mut standpoint = None;
    let mut help_uri = None;
    let mut artifact_change = None;
    for (key, value) in read_map(value)? {
        match read_text(key)? {
            "text" => {
                if text.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Remediation.text"));
                }
                text = Some(
                    (read_text(value).map(str::to_owned))
                        .map_err(|error| error.at("Remediation.text"))?,
                );
            }
            "standpoint" => {
                if standpoint.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate Remediation.standpoint",
                    ));
                }
                standpoint = Some(
                    (read_standpoint(value)).map_err(|error| error.at("Remediation.standpoint"))?,
                );
            }
            "help_uri" => {
                if help_uri.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate Remediation.help_uri",
                    ));
                }
                help_uri = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_text(value).map(str::to_owned)).map(Some)
                    })
                    .map_err(|error| error.at("Remediation.help_uri"))?,
                );
            }
            "artifact_change" => {
                if artifact_change.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate Remediation.artifact_change",
                    ));
                }
                artifact_change = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_artifact_change(value)).map(Some)
                    })
                    .map_err(|error| error.at("Remediation.artifact_change"))?,
                );
            }
            name => {
                return Err(DiagnosticCborError::invalid(format!(
                    "unknown Remediation field {name}"
                )));
            }
        }
    }
    Ok(Remediation {
        text: text.ok_or_else(|| DiagnosticCborError::invalid("missing Remediation.text"))?,
        standpoint: standpoint
            .ok_or_else(|| DiagnosticCborError::invalid("missing Remediation.standpoint"))?,
        help_uri: help_uri.unwrap_or_default(),
        artifact_change: artifact_change.unwrap_or_default(),
    })
}

impl ArtifactChange {
    /// Stream the complete ArtifactChange in its original sparse declaration order.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        let length = 3_usize;
        head::write_head(out, head::MAP, length as u64)?;
        head::write_text(out, "artifact_uri")?;
        head::write_text(out, &self.artifact_uri)?;
        head::write_text(out, "region")?;
        self.region.write_cbor(out)?;
        head::write_text(out, "replacement")?;
        head::write_text(out, &self.replacement)?;
        Ok(())
    }
}

/// Admit the complete ArtifactChange record without normalization or ignored fields.
fn read_artifact_change(value: &Value) -> Result<ArtifactChange, DiagnosticCborError> {
    let mut artifact_uri = None;
    let mut region = None;
    let mut replacement = None;
    for (key, value) in read_map(value)? {
        match read_text(key)? {
            "artifact_uri" => {
                if artifact_uri.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate ArtifactChange.artifact_uri",
                    ));
                }
                artifact_uri = Some(
                    (read_text(value).map(str::to_owned))
                        .map_err(|error| error.at("ArtifactChange.artifact_uri"))?,
                );
            }
            "region" => {
                if region.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate ArtifactChange.region",
                    ));
                }
                region =
                    Some((read_region(value)).map_err(|error| error.at("ArtifactChange.region"))?);
            }
            "replacement" => {
                if replacement.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate ArtifactChange.replacement",
                    ));
                }
                replacement = Some(
                    (read_text(value).map(str::to_owned))
                        .map_err(|error| error.at("ArtifactChange.replacement"))?,
                );
            }
            name => {
                return Err(DiagnosticCborError::invalid(format!(
                    "unknown ArtifactChange field {name}"
                )));
            }
        }
    }
    Ok(ArtifactChange {
        artifact_uri: artifact_uri
            .ok_or_else(|| DiagnosticCborError::invalid("missing ArtifactChange.artifact_uri"))?,
        region: region
            .ok_or_else(|| DiagnosticCborError::invalid("missing ArtifactChange.region"))?,
        replacement: replacement
            .ok_or_else(|| DiagnosticCborError::invalid("missing ArtifactChange.replacement"))?,
    })
}

impl Region {
    /// Stream the complete Region in its original sparse declaration order.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        let length = usize::from(self.start_line.is_some())
            + usize::from(self.start_column.is_some())
            + usize::from(self.end_line.is_some())
            + usize::from(self.end_column.is_some());
        head::write_head(out, head::MAP, length as u64)?;
        if let Some(value) = &self.start_line {
            head::write_text(out, "start_line")?;
            head::write_head(out, head::UNSIGNED, u64::from(*value))?;
        }
        if let Some(value) = &self.start_column {
            head::write_text(out, "start_column")?;
            head::write_head(out, head::UNSIGNED, u64::from(*value))?;
        }
        if let Some(value) = &self.end_line {
            head::write_text(out, "end_line")?;
            head::write_head(out, head::UNSIGNED, u64::from(*value))?;
        }
        if let Some(value) = &self.end_column {
            head::write_text(out, "end_column")?;
            head::write_head(out, head::UNSIGNED, u64::from(*value))?;
        }
        Ok(())
    }
}

/// Admit the complete Region record without normalization or ignored fields.
fn read_region(value: &Value) -> Result<Region, DiagnosticCborError> {
    let mut start_line = None;
    let mut start_column = None;
    let mut end_line = None;
    let mut end_column = None;
    for (key, value) in read_map(value)? {
        match read_text(key)? {
            "start_line" => {
                if start_line.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Region.start_line"));
                }
                start_line = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_u64(value).and_then(|value| {
                            u32::try_from(value).map_err(|error| {
                                DiagnosticCborError::caused_by(
                                    "diagnostic coordinate exceeds u32",
                                    error,
                                )
                            })
                        }))
                        .map(Some)
                    })
                    .map_err(|error| error.at("Region.start_line"))?,
                );
            }
            "start_column" => {
                if start_column.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate Region.start_column",
                    ));
                }
                start_column = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_u64(value).and_then(|value| {
                            u32::try_from(value).map_err(|error| {
                                DiagnosticCborError::caused_by(
                                    "diagnostic coordinate exceeds u32",
                                    error,
                                )
                            })
                        }))
                        .map(Some)
                    })
                    .map_err(|error| error.at("Region.start_column"))?,
                );
            }
            "end_line" => {
                if end_line.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Region.end_line"));
                }
                end_line = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_u64(value).and_then(|value| {
                            u32::try_from(value).map_err(|error| {
                                DiagnosticCborError::caused_by(
                                    "diagnostic coordinate exceeds u32",
                                    error,
                                )
                            })
                        }))
                        .map(Some)
                    })
                    .map_err(|error| error.at("Region.end_line"))?,
                );
            }
            "end_column" => {
                if end_column.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Region.end_column"));
                }
                end_column = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_u64(value).and_then(|value| {
                            u32::try_from(value).map_err(|error| {
                                DiagnosticCborError::caused_by(
                                    "diagnostic coordinate exceeds u32",
                                    error,
                                )
                            })
                        }))
                        .map(Some)
                    })
                    .map_err(|error| error.at("Region.end_column"))?,
                );
            }
            name => {
                return Err(DiagnosticCborError::invalid(format!(
                    "unknown Region field {name}"
                )));
            }
        }
    }
    Ok(Region {
        start_line: start_line.unwrap_or_default(),
        start_column: start_column.unwrap_or_default(),
        end_line: end_line.unwrap_or_default(),
        end_column: end_column.unwrap_or_default(),
    })
}

impl Guidance {
    /// Stream the complete Guidance in its original sparse declaration order.
    ///
    /// # Errors
    /// Returns the sink's original failure without reporting partial success.
    fn write_cbor<W: Write + ?Sized>(&self, out: &mut W) -> io::Result<()> {
        let length = 5_usize + usize::from(self.help_uri.is_some());
        head::write_head(out, head::MAP, length as u64)?;
        head::write_text(out, "modality")?;
        self.modality.write_cbor(out)?;
        head::write_text(out, "source")?;
        self.source.write_cbor(out)?;
        head::write_text(out, "term_iri")?;
        head::write_text(out, &self.term_iri)?;
        head::write_text(out, "text")?;
        head::write_text(out, &self.text)?;
        head::write_text(out, "standpoint")?;
        self.standpoint.write_cbor(out)?;
        if let Some(value) = &self.help_uri {
            head::write_text(out, "help_uri")?;
            head::write_text(out, value)?;
        }
        Ok(())
    }
}

/// Admit the complete Guidance record without normalization or ignored fields.
fn read_guidance(value: &Value) -> Result<Guidance, DiagnosticCborError> {
    let mut modality = None;
    let mut source = None;
    let mut term_iri = None;
    let mut text = None;
    let mut standpoint = None;
    let mut help_uri = None;
    for (key, value) in read_map(value)? {
        match read_text(key)? {
            "modality" => {
                if modality.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Guidance.modality"));
                }
                modality = Some(
                    (read_guidance_modality(value))
                        .map_err(|error| error.at("Guidance.modality"))?,
                );
            }
            "source" => {
                if source.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Guidance.source"));
                }
                source = Some(
                    (read_guidance_source(value)).map_err(|error| error.at("Guidance.source"))?,
                );
            }
            "term_iri" => {
                if term_iri.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Guidance.term_iri"));
                }
                term_iri = Some(
                    (read_text(value).map(str::to_owned))
                        .map_err(|error| error.at("Guidance.term_iri"))?,
                );
            }
            "text" => {
                if text.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Guidance.text"));
                }
                text = Some(
                    (read_text(value).map(str::to_owned))
                        .map_err(|error| error.at("Guidance.text"))?,
                );
            }
            "standpoint" => {
                if standpoint.is_some() {
                    return Err(DiagnosticCborError::invalid(
                        "duplicate Guidance.standpoint",
                    ));
                }
                standpoint = Some(
                    (read_standpoint(value)).map_err(|error| error.at("Guidance.standpoint"))?,
                );
            }
            "help_uri" => {
                if help_uri.is_some() {
                    return Err(DiagnosticCborError::invalid("duplicate Guidance.help_uri"));
                }
                help_uri = Some(
                    (if matches!(value, Value::Null) {
                        Ok(None)
                    } else {
                        (read_text(value).map(str::to_owned)).map(Some)
                    })
                    .map_err(|error| error.at("Guidance.help_uri"))?,
                );
            }
            name => {
                return Err(DiagnosticCborError::invalid(format!(
                    "unknown Guidance field {name}"
                )));
            }
        }
    }
    Ok(Guidance {
        modality: modality
            .ok_or_else(|| DiagnosticCborError::invalid("missing Guidance.modality"))?,
        source: source.ok_or_else(|| DiagnosticCborError::invalid("missing Guidance.source"))?,
        term_iri: term_iri
            .ok_or_else(|| DiagnosticCborError::invalid("missing Guidance.term_iri"))?,
        text: text.ok_or_else(|| DiagnosticCborError::invalid("missing Guidance.text"))?,
        standpoint: standpoint
            .ok_or_else(|| DiagnosticCborError::invalid("missing Guidance.standpoint"))?,
        help_uri: help_uri.unwrap_or_default(),
    })
}

/// Stream the report's retained metadata map in its existing key iteration order.
fn write_metadata_map<W: Write + ?Sized>(
    metadata: &BTreeMap<String, serde_json::Value>,
    out: &mut W,
) -> io::Result<()> {
    head::write_head(out, head::MAP, metadata.len() as u64)?;
    for (key, value) in metadata {
        head::write_text(out, key)?;
        write_metadata(value, out)?;
    }
    Ok(())
}

/// Traverse existing metadata values directly; no JSON or CBOR value tree is built.
fn write_metadata<W: Write + ?Sized>(value: &serde_json::Value, out: &mut W) -> io::Result<()> {
    enum Item<'a> {
        Value(&'a serde_json::Value),
        Key(&'a str),
    }
    let mut pending = vec![Item::Value(value)];
    while let Some(item) = pending.pop() {
        let value = match item {
            Item::Key(key) => {
                head::write_text(out, key)?;
                continue;
            }
            Item::Value(value) => value,
        };
        match value {
            serde_json::Value::Null => cbor::write(&Value::Null, out)?,
            serde_json::Value::Bool(value) => cbor::write(&Value::Bool(*value), out)?,
            serde_json::Value::String(value) => head::write_text(out, value)?,
            serde_json::Value::Number(number) => {
                if let Some(value) = number.as_u64() {
                    cbor::write(&Value::from(value), out)?;
                } else if let Some(value) = number.as_i64() {
                    cbor::write(&Value::from(value), out)?;
                } else {
                    let value = number
                        .as_f64()
                        .filter(|value| value.is_finite())
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                "unsupported diagnostic metadata number",
                            )
                        })?;
                    cbor::write(&Value::Float(value), out)?;
                }
            }
            serde_json::Value::Array(values) => {
                head::write_head(out, head::ARRAY, values.len() as u64)?;
                pending.extend(values.iter().rev().map(Item::Value));
            }
            serde_json::Value::Object(entries) => {
                head::write_head(out, head::MAP, entries.len() as u64)?;
                for (key, value) in entries.iter().rev() {
                    pending.push(Item::Value(value));
                    pending.push(Item::Key(key));
                }
            }
        }
    }
    Ok(())
}

/// Admit distinct textual metadata keys and only the existing retained value kinds.
fn read_metadata_map(
    value: &Value,
) -> Result<BTreeMap<String, serde_json::Value>, DiagnosticCborError> {
    let mut metadata = BTreeMap::new();
    for (key, value) in read_map(value)? {
        let key = read_text(key)?.to_owned();
        if metadata.contains_key(&key) {
            return Err(DiagnosticCborError::invalid(format!(
                "duplicate diagnostic metadata key {key}"
            )));
        }
        metadata.insert(key, read_metadata(value, 1)?);
    }
    Ok(metadata)
}

/// Reconstruct bounded metadata without text conversion, key replacement or nonfinite loss.
fn read_metadata(value: &Value, depth: usize) -> Result<serde_json::Value, DiagnosticCborError> {
    use serde_json::{Map, Number, Value as MetadataValue};
    match value {
        Value::Null => Ok(MetadataValue::Null),
        Value::Bool(value) => Ok(MetadataValue::Bool(*value)),
        Value::Text(value) => Ok(MetadataValue::String(value.clone())),
        Value::Integer(integer) => {
            let value = integer.get();
            if value >= 0 {
                let value = u64::try_from(value).map_err(|error| {
                    DiagnosticCborError::caused_by("metadata unsigned integer exceeds u64", error)
                })?;
                Ok(MetadataValue::Number(Number::from(value)))
            } else {
                let value = i64::try_from(value).map_err(|error| {
                    DiagnosticCborError::caused_by("metadata signed integer exceeds i64", error)
                })?;
                Ok(MetadataValue::Number(Number::from(value)))
            }
        }
        Value::Float(value) => Number::from_f64(*value)
            .map(MetadataValue::Number)
            .ok_or_else(|| DiagnosticCborError::invalid("nonfinite diagnostic metadata float")),
        Value::Array(values) => {
            if depth >= MAX_DIAGNOSTIC_CBOR_DEPTH {
                return Err(DiagnosticCborError::invalid(
                    "diagnostic metadata exceeds its structural bound",
                ));
            }
            values
                .iter()
                .map(|value| read_metadata(value, depth + 1))
                .collect::<Result<Vec<_>, _>>()
                .map(MetadataValue::Array)
        }
        Value::Map(entries) => {
            if depth >= MAX_DIAGNOSTIC_CBOR_DEPTH {
                return Err(DiagnosticCborError::invalid(
                    "diagnostic metadata exceeds its structural bound",
                ));
            }
            let mut object = Map::new();
            for (key, value) in entries {
                let key = read_text(key)?.to_owned();
                if object.contains_key(&key) {
                    return Err(DiagnosticCborError::invalid(format!(
                        "duplicate diagnostic metadata key {key}"
                    )));
                }
                object.insert(key, read_metadata(value, depth + 1)?);
            }
            Ok(MetadataValue::Object(object))
        }
        _ => Err(DiagnosticCborError::invalid(
            "unsupported diagnostic metadata CBOR kind",
        )),
    }
}
