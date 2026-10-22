// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native lowered-node wire records. Semantic identity and ledger replay remain
//! owned by the original types; decoding never recomputes a supplied fingerprint.
//! Structs admit named records only. Unit enums also retain their exact
//! single-key null-object form. Unknown record fields remain ignored.

use super::{DiagFingerprint, DiagNode, Observation, RecordedDiag, SerFrame, SerLocation};
use crate::diag::{
    Advice, ArtifactChange, Focus, Guidance, GuidanceModality, GuidanceSource, Label, Region,
    Remediation, Slot, SourceContext, StageId, TermRole,
};
use crate::grade::{Belnap, Grade, Standpoint};
use crate::model::{DiagnosticAttribution, FindingCategory, Location, Severity};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson, items_with},
};

fn unsigned<T: TryFrom<u64>>(value: &Value, expected: &str) -> Result<T, DecodeError> {
    value
        .as_u64()
        .and_then(|number| T::try_from(number).ok())
        .ok_or_else(|| DecodeError::invalid_value(value, expected))
}

impl ToJson for RecordedDiag {
    fn to_json(&self) -> Value {
        Object::new()
            .with("root", self.root.to_json())
            .with("causes", self.causes.to_json())
            .into()
    }
}

impl FromJson for RecordedDiag {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct RecordedDiag")?;
        Ok(Self {
            root: record.required("root")?,
            causes: record.required("causes")?,
        })
    }
}

fn unit_name<'a>(value: &'a Value, expected: &str) -> Result<&'a str, DecodeError> {
    if let Some(name) = value.as_str() {
        return Ok(name);
    }
    let object = value
        .as_object()
        .ok_or_else(|| DecodeError::invalid_type(value, expected))?;
    let mut entries = object.iter();
    let Some((name, payload)) = entries.next() else {
        return Err(DecodeError::invalid_value(value, expected));
    };
    if entries.next().is_some() || !matches!(payload, Value::Null) {
        return Err(DecodeError::invalid_value(value, expected));
    }
    Ok(name)
}

impl ToJson for DiagFingerprint {
    fn to_json(&self) -> Value {
        self.0.to_json()
    }
}

impl FromJson for DiagFingerprint {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let bytes = items_with(value, |item| unsigned::<u8>(item, "u8"))?;
        let bytes = bytes
            .try_into()
            .map_err(|_| DecodeError::invalid_value(value, "16 fingerprint bytes"))?;
        Ok(Self(bytes))
    }
}

impl ToJson for StageId {
    fn to_json(&self) -> Value {
        self.0.to_json()
    }
}
impl FromJson for StageId {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        String::from_json(value).map(Self::new)
    }
}

impl ToJson for Focus {
    fn to_json(&self) -> Value {
        self.0.to_json()
    }
}
impl FromJson for Focus {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        String::from_json(value).map(Self)
    }
}

impl ToJson for Severity {
    fn to_json(&self) -> Value {
        Value::from(match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note => "note",
            Self::Info => "info",
        })
    }
}
impl FromJson for Severity {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match unit_name(value, "enum Severity")? {
            "error" => Ok(Self::Error),
            "warning" => Ok(Self::Warning),
            "note" => Ok(Self::Note),
            "info" => Ok(Self::Info),
            other => Err(DecodeError::unknown_variant(
                other,
                &["error", "warning", "note", "info"],
            )),
        }
    }
}

impl ToJson for FindingCategory {
    fn to_json(&self) -> Value {
        Value::from(self.as_str())
    }
}
impl FromJson for FindingCategory {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let name = unit_name(value, "enum FindingCategory")?;
        Self::ALL
            .into_iter()
            .find(|category| category.as_str() == name)
            .ok_or_else(|| DecodeError::unknown_variant(name, &Self::ALL.map(Self::as_str)))
    }
}

impl ToJson for Standpoint {
    fn to_json(&self) -> Value {
        Value::from(match self {
            Self::Advisory => "advisory",
            Self::Perspectival => "perspectival",
            Self::Binding => "binding",
        })
    }
}
impl FromJson for Standpoint {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match unit_name(value, "enum Standpoint")? {
            "advisory" => Ok(Self::Advisory),
            "perspectival" => Ok(Self::Perspectival),
            "binding" => Ok(Self::Binding),
            other => Err(DecodeError::unknown_variant(
                other,
                &["advisory", "perspectival", "binding"],
            )),
        }
    }
}

impl ToJson for Belnap {
    fn to_json(&self) -> Value {
        Value::from(match self {
            Self::Neither => "neither",
            Self::Supported => "supported",
            Self::Opposed => "opposed",
            Self::Both => "both",
        })
    }
}
impl FromJson for Belnap {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match unit_name(value, "enum Belnap")? {
            "neither" => Ok(Self::Neither),
            "supported" => Ok(Self::Supported),
            "opposed" => Ok(Self::Opposed),
            "both" => Ok(Self::Both),
            other => Err(DecodeError::unknown_variant(
                other,
                &["neither", "supported", "opposed", "both"],
            )),
        }
    }
}

impl ToJson for TermRole {
    fn to_json(&self) -> Value {
        Value::from(match self {
            Self::Subject => "subject",
            Self::Predicate => "predicate",
            Self::Object => "object",
            Self::Graph => "graph",
        })
    }
}
impl FromJson for TermRole {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match unit_name(value, "enum TermRole")? {
            "subject" => Ok(Self::Subject),
            "predicate" => Ok(Self::Predicate),
            "object" => Ok(Self::Object),
            "graph" => Ok(Self::Graph),
            other => Err(DecodeError::unknown_variant(
                other,
                &["subject", "predicate", "object", "graph"],
            )),
        }
    }
}

impl ToJson for GuidanceModality {
    fn to_json(&self) -> Value {
        Value::from(match self {
            Self::HowToUse => "HowToUse",
            Self::UseWhen => "UseWhen",
            Self::AvoidWhen => "AvoidWhen",
        })
    }
}
impl FromJson for GuidanceModality {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match unit_name(value, "enum GuidanceModality")? {
            "HowToUse" => Ok(Self::HowToUse),
            "UseWhen" => Ok(Self::UseWhen),
            "AvoidWhen" => Ok(Self::AvoidWhen),
            other => Err(DecodeError::unknown_variant(
                other,
                &["HowToUse", "UseWhen", "AvoidWhen"],
            )),
        }
    }
}

impl ToJson for GuidanceSource {
    fn to_json(&self) -> Value {
        Value::from(match self {
            Self::RuleGoverningTerm => "RuleGoverningTerm",
            Self::DocumentedTerm => "DocumentedTerm",
        })
    }
}
impl FromJson for GuidanceSource {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match unit_name(value, "enum GuidanceSource")? {
            "RuleGoverningTerm" => Ok(Self::RuleGoverningTerm),
            "DocumentedTerm" => Ok(Self::DocumentedTerm),
            other => Err(DecodeError::unknown_variant(
                other,
                &["RuleGoverningTerm", "DocumentedTerm"],
            )),
        }
    }
}

impl ToJson for SerLocation {
    fn to_json(&self) -> Value {
        let mut object = Object::new();
        object.insert("file", self.file.to_json());
        object.insert("line", self.line.to_json());
        object.insert("column", self.column.to_json());
        Value::Object(object)
    }
}
impl FromJson for SerLocation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct SerLocation")?;
        Ok(Self {
            file: record.required("file")?,
            line: record.required_with("line", |item| unsigned::<u32>(item, "u32"))?,
            column: record.required_with("column", |item| unsigned::<u32>(item, "u32"))?,
        })
    }
}

impl ToJson for SerFrame {
    fn to_json(&self) -> Value {
        let mut object = Object::new();
        object.insert("message", self.message.to_json());
        if self.at.is_some() {
            object.insert("at", self.at.to_json());
        }
        Value::Object(object)
    }
}
impl FromJson for SerFrame {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct SerFrame")?;
        Ok(Self {
            message: record.required("message")?,
            at: record.optional("at")?,
        })
    }
}

impl ToJson for Observation {
    fn to_json(&self) -> Value {
        let mut object = Object::new();
        object.insert("message", self.message.to_json());
        if self.observed.is_some() {
            object.insert("observed", self.observed.to_json());
        }
        if self.expected.is_some() {
            object.insert("expected", self.expected.to_json());
        }
        Value::Object(object)
    }
}
impl FromJson for Observation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct Observation")?;
        Ok(Self {
            message: record.required("message")?,
            observed: record.optional("observed")?,
            expected: record.optional("expected")?,
        })
    }
}

impl ToJson for Grade {
    fn to_json(&self) -> Value {
        let mut object = Object::new();
        object.insert("severity", self.severity.to_json());
        object.insert("category", self.category.to_json());
        object.insert("standpoint", self.standpoint.to_json());
        Value::Object(object)
    }
}
impl FromJson for Grade {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct Grade")?;
        Ok(Self::new(
            record.required("severity")?,
            record.required("category")?,
            record.required("standpoint")?,
        ))
    }
}

impl ToJson for Slot {
    fn to_json(&self) -> Value {
        let mut object = Object::new();
        object.insert("lexical", self.lexical.to_json());
        object.insert("datatype", self.datatype.to_json());
        Value::Object(object)
    }
}
impl FromJson for Slot {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct Slot")?;
        Ok(Self {
            lexical: record.required("lexical")?,
            datatype: record.optional("datatype")?,
        })
    }
}

impl ToJson for Location {
    fn to_json(&self) -> Value {
        let mut object = Object::new();
        if self.path.is_some() {
            object.insert("path", self.path.to_json());
        }
        if self.line.is_some() {
            object.insert("line", self.line.to_json());
        }
        if self.column.is_some() {
            object.insert("column", self.column.to_json());
        }
        if self.logical.is_some() {
            object.insert("logical", self.logical.to_json());
        }
        if self.gts_term_id.is_some() {
            object.insert("gts_term_id", self.gts_term_id.to_json());
        }
        if self.gts_quad_index.is_some() {
            object.insert("gts_quad_index", self.gts_quad_index.to_json());
        }
        if self.gts_reifier_id.is_some() {
            object.insert("gts_reifier_id", self.gts_reifier_id.to_json());
        }
        if self.gts_frame_index.is_some() {
            object.insert("gts_frame_index", self.gts_frame_index.to_json());
        }
        if self.gts_segment_index.is_some() {
            object.insert("gts_segment_index", self.gts_segment_index.to_json());
        }
        Value::Object(object)
    }
}
impl FromJson for Location {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct Location")?;
        Ok(Self {
            path: record.optional("path")?,
            line: record.optional_with("line", |item| unsigned::<u64>(item, "u64"))?,
            column: record.optional_with("column", |item| unsigned::<u32>(item, "u32"))?,
            logical: record.optional("logical")?,
            gts_term_id: record
                .optional_with("gts_term_id", |item| unsigned::<u64>(item, "u64"))?,
            gts_quad_index: record
                .optional_with("gts_quad_index", |item| unsigned::<u64>(item, "u64"))?,
            gts_reifier_id: record
                .optional_with("gts_reifier_id", |item| unsigned::<u64>(item, "u64"))?,
            gts_frame_index: record
                .optional_with("gts_frame_index", |item| unsigned::<u64>(item, "u64"))?,
            gts_segment_index: record
                .optional_with("gts_segment_index", |item| unsigned::<u64>(item, "u64"))?,
        })
    }
}

impl ToJson for SourceContext {
    fn to_json(&self) -> Value {
        let mut object = Object::new();
        object.insert("location", self.location.to_json());
        if self.graph.is_some() {
            object.insert(
                "graph",
                self.graph.as_ref().map(|iri| iri.as_str()).to_json(),
            );
        }
        object.insert("focus", self.focus.to_json());
        object.insert("term_role", self.term_role.to_json());
        Value::Object(object)
    }
}
impl FromJson for SourceContext {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct SourceContext")?;
        Ok(Self {
            location: record.required("location")?,
            graph: record.optional_with("graph", |item| {
                let iri = String::from_json(item)?;
                purrdf_core::BaseIri::parse(&iri).map_err(DecodeError::custom)
            })?,
            focus: record.optional("focus")?,
            term_role: record.optional("term_role")?,
        })
    }
}

impl ToJson for Advice {
    fn to_json(&self) -> Value {
        let mut object = Object::new();
        object.insert("standpoint", self.standpoint.to_json());
        object.insert("text", self.text.to_json());
        object.insert("help_uri", self.help_uri.to_json());
        Value::Object(object)
    }
}
impl FromJson for Advice {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct Advice")?;
        Ok(Self {
            standpoint: record.required("standpoint")?,
            text: record.required("text")?,
            help_uri: record.optional("help_uri")?,
        })
    }
}

impl ToJson for Label {
    fn to_json(&self) -> Value {
        let mut object = Object::new();
        object.insert("location", self.location.to_json());
        object.insert("text", self.text.to_json());
        Value::Object(object)
    }
}
impl FromJson for Label {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct Label")?;
        Ok(Self {
            location: record.required("location")?,
            text: record.required("text")?,
        })
    }
}

impl ToJson for Region {
    fn to_json(&self) -> Value {
        let mut object = Object::new();
        if self.start_line.is_some() {
            object.insert("start_line", self.start_line.to_json());
        }
        if self.start_column.is_some() {
            object.insert("start_column", self.start_column.to_json());
        }
        if self.end_line.is_some() {
            object.insert("end_line", self.end_line.to_json());
        }
        if self.end_column.is_some() {
            object.insert("end_column", self.end_column.to_json());
        }
        Value::Object(object)
    }
}
impl FromJson for Region {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct Region")?;
        Ok(Self {
            start_line: record.optional_with("start_line", |item| unsigned::<u32>(item, "u32"))?,
            start_column: record
                .optional_with("start_column", |item| unsigned::<u32>(item, "u32"))?,
            end_line: record.optional_with("end_line", |item| unsigned::<u32>(item, "u32"))?,
            end_column: record.optional_with("end_column", |item| unsigned::<u32>(item, "u32"))?,
        })
    }
}

impl ToJson for ArtifactChange {
    fn to_json(&self) -> Value {
        let mut object = Object::new();
        object.insert("artifact_uri", self.artifact_uri.to_json());
        object.insert("region", self.region.to_json());
        object.insert("replacement", self.replacement.to_json());
        Value::Object(object)
    }
}
impl FromJson for ArtifactChange {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct ArtifactChange")?;
        Ok(Self {
            artifact_uri: record.required("artifact_uri")?,
            region: record.required("region")?,
            replacement: record.required("replacement")?,
        })
    }
}

impl ToJson for Remediation {
    fn to_json(&self) -> Value {
        let mut object = Object::new();
        object.insert("text", self.text.to_json());
        object.insert("standpoint", self.standpoint.to_json());
        if self.help_uri.is_some() {
            object.insert("help_uri", self.help_uri.to_json());
        }
        if self.artifact_change.is_some() {
            object.insert("artifact_change", self.artifact_change.to_json());
        }
        Value::Object(object)
    }
}
impl FromJson for Remediation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct Remediation")?;
        Ok(Self {
            text: record.required("text")?,
            standpoint: record.required("standpoint")?,
            help_uri: record.optional("help_uri")?,
            artifact_change: record.optional("artifact_change")?,
        })
    }
}

impl ToJson for Guidance {
    fn to_json(&self) -> Value {
        let mut object = Object::new();
        object.insert("modality", self.modality.to_json());
        object.insert("source", self.source.to_json());
        object.insert("term_iri", self.term_iri.to_json());
        object.insert("text", self.text.to_json());
        object.insert("standpoint", self.standpoint.to_json());
        if self.help_uri.is_some() {
            object.insert("help_uri", self.help_uri.to_json());
        }
        Value::Object(object)
    }
}
impl FromJson for Guidance {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct Guidance")?;
        Ok(Self {
            modality: record.required("modality")?,
            source: record.required("source")?,
            term_iri: record.required("term_iri")?,
            text: record.required("text")?,
            standpoint: record.required("standpoint")?,
            help_uri: record.optional("help_uri")?,
        })
    }
}

impl ToJson for DiagnosticAttribution {
    fn to_json(&self) -> Value {
        let mut object = Object::new();
        object.insert("slice_iri", self.slice_iri.to_json());
        object.insert("role", self.role.to_json());
        if self.evidence.is_some() {
            object.insert("evidence", self.evidence.to_json());
        }
        Value::Object(object)
    }
}
impl FromJson for DiagnosticAttribution {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct DiagnosticAttribution")?;
        Ok(Self {
            slice_iri: record.required("slice_iri")?,
            role: record.required("role")?,
            evidence: record.optional("evidence")?,
        })
    }
}

impl ToJson for DiagNode {
    fn to_json(&self) -> Value {
        let mut object = Object::new();
        object.insert("fingerprint", self.fingerprint.to_json());
        object.insert("stage", self.stage.to_json());
        object.insert("grade", self.grade.to_json());
        object.insert("code", self.code.to_json());
        object.insert("observations", self.observations.to_json());
        object.insert("frames", self.frames.to_json());
        object.insert("antecedents", self.antecedents.to_json());
        object.insert("source_ctx", self.source_ctx.to_json());
        object.insert("attributions", self.attributions.to_json());
        object.insert("advice", self.advice.to_json());
        if !self.remediation.is_empty() {
            object.insert("remediation", self.remediation.to_json());
        }
        if !self.guidance.is_empty() {
            object.insert("guidance", self.guidance.to_json());
        }
        if !self.derived_from_quads.is_empty() {
            object.insert("derived_from_quads", self.derived_from_quads.to_json());
        }
        if !self.cited_iris.is_empty() {
            object.insert("cited_iris", self.cited_iris.to_json());
        }
        object.insert("labels", self.labels.to_json());
        object.insert("tags", self.tags.to_json());
        if !self.documented_terms.is_empty() {
            object.insert("documented_terms", self.documented_terms.to_json());
        }
        if self.failure_class.is_some() {
            object.insert("failure_class", self.failure_class.to_json());
        }
        object.insert("knowledge", self.knowledge.to_json());
        object.insert("emitted_at", self.emitted_at.to_json());
        if self.locus_stage.is_some() {
            object.insert("locus_stage", self.locus_stage.to_json());
        }
        Value::Object(object)
    }
}
impl FromJson for DiagNode {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct DiagNode")?;
        Ok(Self {
            fingerprint: record.required("fingerprint")?,
            stage: record.required("stage")?,
            grade: record.required("grade")?,
            code: record.required("code")?,
            observations: record.required("observations")?,
            frames: record.required("frames")?,
            antecedents: record
                .required::<Vec<DiagFingerprint>>("antecedents")?
                .into_boxed_slice(),
            source_ctx: record.required("source_ctx")?,
            attributions: record.required("attributions")?,
            advice: record.required("advice")?,
            remediation: record.defaulted("remediation")?,
            guidance: record.defaulted("guidance")?,
            derived_from_quads: record.defaulted("derived_from_quads")?,
            cited_iris: record.defaulted("cited_iris")?,
            labels: record.required("labels")?,
            tags: record.required("tags")?,
            documented_terms: record.defaulted("documented_terms")?,
            failure_class: record.optional("failure_class")?,
            knowledge: record.required("knowledge")?,
            emitted_at: record.required("emitted_at")?,
            locus_stage: record.optional("locus_stage")?,
        })
    }
}

#[cfg(test)]
mod tests;
