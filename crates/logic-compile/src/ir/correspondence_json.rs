// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete correspondence evidence transport; hydration does not reconstruct claims.

use super::{
    AxisEvidence, Correspondence, CorrespondenceCaveat, CorrespondenceLaw, CorrespondenceRelation,
    Determinacy, DischargeCondition, DischargeVerdict, LawClaimIr, LegPath, MorphismClass,
    MorphismKind, RecoveryCaseIr, literal_serde,
};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson, Within},
};

impl ToJson for CorrespondenceRelation {
    fn to_json(&self) -> Value {
        match self {
            Self::Equiv => "Equiv",
            Self::Subsumes => "Subsumes",
            Self::SubsumedBy => "SubsumedBy",
            Self::Overlaps => "Overlaps",
            Self::RelatedMatch => "RelatedMatch",
            Self::Disjoint => "Disjoint",
        }
        .to_json()
    }
}
impl FromJson for CorrespondenceRelation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "CorrespondenceRelation")? {
            "Equiv" => Ok(Self::Equiv),
            "Subsumes" => Ok(Self::Subsumes),
            "SubsumedBy" => Ok(Self::SubsumedBy),
            "Overlaps" => Ok(Self::Overlaps),
            "RelatedMatch" => Ok(Self::RelatedMatch),
            "Disjoint" => Ok(Self::Disjoint),
            name => Err(DecodeError::unknown_variant(
                name,
                &[
                    "Equiv",
                    "Subsumes",
                    "SubsumedBy",
                    "Overlaps",
                    "RelatedMatch",
                    "Disjoint",
                ],
            )),
        }
    }
}

impl ToJson for MorphismClass {
    fn to_json(&self) -> Value {
        match self {
            Self::Isomorphism => "Isomorphism",
            Self::SectionRetraction => "SectionRetraction",
            Self::WellBehavedLens => "WellBehavedLens",
            Self::LossyLens => "LossyLens",
            Self::Prism => "Prism",
            Self::AffineCorrespondence => "AffineCorrespondence",
            Self::BridgeView => "BridgeView",
        }
        .to_json()
    }
}
impl FromJson for MorphismClass {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "MorphismClass")? {
            "Isomorphism" => Ok(Self::Isomorphism),
            "SectionRetraction" => Ok(Self::SectionRetraction),
            "WellBehavedLens" => Ok(Self::WellBehavedLens),
            "LossyLens" => Ok(Self::LossyLens),
            "Prism" => Ok(Self::Prism),
            "AffineCorrespondence" => Ok(Self::AffineCorrespondence),
            "BridgeView" => Ok(Self::BridgeView),
            name => Err(DecodeError::unknown_variant(
                name,
                &[
                    "Isomorphism",
                    "SectionRetraction",
                    "WellBehavedLens",
                    "LossyLens",
                    "Prism",
                    "AffineCorrespondence",
                    "BridgeView",
                ],
            )),
        }
    }
}

impl ToJson for MorphismKind {
    fn to_json(&self) -> Value {
        match self {
            Self::InstitutionMorphism => "InstitutionMorphism",
            Self::CommitmentShiftingBridge => "CommitmentShiftingBridge",
        }
        .to_json()
    }
}
impl FromJson for MorphismKind {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "MorphismKind")? {
            "InstitutionMorphism" => Ok(Self::InstitutionMorphism),
            "CommitmentShiftingBridge" => Ok(Self::CommitmentShiftingBridge),
            name => Err(DecodeError::unknown_variant(
                name,
                &["InstitutionMorphism", "CommitmentShiftingBridge"],
            )),
        }
    }
}

impl ToJson for Determinacy {
    fn to_json(&self) -> Value {
        match self {
            Self::Crisp => "Crisp",
            Self::Vague => "Vague",
        }
        .to_json()
    }
}
impl FromJson for Determinacy {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "Determinacy")? {
            "Crisp" => Ok(Self::Crisp),
            "Vague" => Ok(Self::Vague),
            name => Err(DecodeError::unknown_variant(name, &["Crisp", "Vague"])),
        }
    }
}

impl ToJson for CorrespondenceLaw {
    fn to_json(&self) -> Value {
        match self {
            Self::GetPut => "GetPut",
            Self::PutGet => "PutGet",
            Self::PutPut => "PutPut",
            Self::SectionLaw => "SectionLaw",
        }
        .to_json()
    }
}
impl FromJson for CorrespondenceLaw {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "CorrespondenceLaw")? {
            "GetPut" => Ok(Self::GetPut),
            "PutGet" => Ok(Self::PutGet),
            "PutPut" => Ok(Self::PutPut),
            "SectionLaw" => Ok(Self::SectionLaw),
            name => Err(DecodeError::unknown_variant(
                name,
                &["GetPut", "PutGet", "PutPut", "SectionLaw"],
            )),
        }
    }
}

impl ToJson for DischargeVerdict {
    fn to_json(&self) -> Value {
        match self {
            Self::ObligationDischarged => "ObligationDischarged",
            Self::ObligationUnknown => "ObligationUnknown",
            Self::ObligationViolated => "ObligationViolated",
        }
        .to_json()
    }
}
impl FromJson for DischargeVerdict {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "DischargeVerdict")? {
            "ObligationDischarged" => Ok(Self::ObligationDischarged),
            "ObligationUnknown" => Ok(Self::ObligationUnknown),
            "ObligationViolated" => Ok(Self::ObligationViolated),
            name => Err(DecodeError::unknown_variant(
                name,
                &[
                    "ObligationDischarged",
                    "ObligationUnknown",
                    "ObligationViolated",
                ],
            )),
        }
    }
}

impl ToJson for DischargeCondition {
    fn to_json(&self) -> Value {
        match self {
            Self::DischargeCertifiedFragment => "DischargeCertifiedFragment",
            Self::DischargeFiniteClosure => "DischargeFiniteClosure",
            Self::DischargeSyntacticReachability => "DischargeSyntacticReachability",
            Self::DischargeConservativeExtension => "DischargeConservativeExtension",
            Self::DischargeBoundedCorpus => "DischargeBoundedCorpus",
        }
        .to_json()
    }
}
impl FromJson for DischargeCondition {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "DischargeCondition")? {
            "DischargeCertifiedFragment" => Ok(Self::DischargeCertifiedFragment),
            "DischargeFiniteClosure" => Ok(Self::DischargeFiniteClosure),
            "DischargeSyntacticReachability" => Ok(Self::DischargeSyntacticReachability),
            "DischargeConservativeExtension" => Ok(Self::DischargeConservativeExtension),
            "DischargeBoundedCorpus" => Ok(Self::DischargeBoundedCorpus),
            name => Err(DecodeError::unknown_variant(
                name,
                &[
                    "DischargeCertifiedFragment",
                    "DischargeFiniteClosure",
                    "DischargeSyntacticReachability",
                    "DischargeConservativeExtension",
                    "DischargeBoundedCorpus",
                ],
            )),
        }
    }
}

::purrdf_lex::json_record!(impl ToJson for LawClaimIr {
    "law" => law,
    "verdict" => verdict,
    "condition" => condition,
});
impl FromJson for LawClaimIr {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct LawClaimIr")?;
        Ok(Self {
            law: fields.required("law")?,
            verdict: fields.required("verdict")?,
            condition: fields.optional("condition")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for RecoveryCaseIr {
    "iri" => iri,
    "transform" => transform,
});
impl FromJson for RecoveryCaseIr {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct RecoveryCaseIr")?;
        Ok(Self {
            iri: fields.required("iri")?,
            transform: fields.required("transform")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for AxisEvidence {
    "sources" => sources,
    "scale" => scale,
    "probability_model" => probability_model,
});
impl FromJson for AxisEvidence {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct AxisEvidence")?;
        Ok(Self {
            sources: fields.required("sources")?,
            scale: fields.optional("scale")?,
            probability_model: fields.optional("probability_model")?,
        })
    }
}

impl ToJson for LegPath {
    fn to_json(&self) -> Value {
        match self {
            Self::Step(value) => Object::new().with("Step", value.to_json()).into(),
            Self::Inverse(value) => Object::new().with("Inverse", value.to_json()).into(),
            Self::Seq(value) => Object::new().with("Seq", value.to_json()).into(),
            Self::Alt(value) => Object::new().with("Alt", value.to_json()).into(),
        }
    }
}
impl FromJson for LegPath {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let (tag, payload) =
            gmeow_errors::json::read_variant(value, "LegPath", "one LegPath variant")?;
        let decoded = match tag {
            "Step" => String::from_json(payload).map(Self::Step),
            "Inverse" => Self::from_json(payload).map(|value| Self::Inverse(Box::new(value))),
            "Seq" => Vec::<Self>::from_json(payload).map(Self::Seq),
            "Alt" => Vec::<Self>::from_json(payload).map(Self::Alt),
            name => Err(DecodeError::unknown_variant(
                name,
                &["Step", "Inverse", "Seq", "Alt"],
            )),
        };
        decoded.map_err(|error| error.within(tag))
    }
}
impl ToJson for CorrespondenceCaveat {
    fn to_json(&self) -> Value {
        Object::new()
            .with("iri", self.iri.to_json())
            .with("comments", literal_serde::to_json(&self.comments))
            .into()
    }
}
impl FromJson for CorrespondenceCaveat {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct CorrespondenceCaveat")?;
        Ok(Self {
            iri: fields.required("iri")?,
            comments: fields.required_with("comments", |value| {
                let comments = literal_serde::from_json_set(value)?;
                if comments.is_empty() {
                    return Err(DecodeError::custom(
                        "a correspondence caveat requires at least one comment",
                    ));
                }
                Ok(comments)
            })?,
        })
    }
}
impl ToJson for Correspondence {
    fn to_json(&self) -> Value {
        Object::new()
            .with("iri", self.iri.to_json())
            .with("relation", self.relation.to_json())
            .with("morphism_class", self.morphism_class.to_json())
            .with("morphism_kind", self.morphism_kind.to_json())
            .with("mnemomorphic", self.mnemomorphic.to_json())
            .with("determinacy", self.determinacy.to_json())
            .with("get_leg", self.get_leg.to_json())
            .with("put_leg", self.put_leg.to_json())
            .with("law_claims", self.law_claims.to_json())
            .with("caveats", self.caveats.to_json())
            .with("loss_evidence", literal_serde::to_json(&self.loss_evidence))
            .with("confidence", self.confidence.to_json())
            .with("evidence_strength", self.evidence_strength.to_json())
            .with("axis_evidence", self.axis_evidence.to_json())
            .with("weight", self.weight.to_json())
            .with("probability", self.probability.to_json())
            .with("according_to", self.according_to.to_json())
            .with("preservation", self.preservation.to_json())
            .with("source_endpoint", self.source_endpoint.to_json())
            .with("target_endpoint", self.target_endpoint.to_json())
            .with("grounding", self.grounding.to_json())
            .with("recovery_cases", self.recovery_cases.to_json())
            .into()
    }
}
impl FromJson for Correspondence {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Correspondence")?;
        Ok(Self {
            iri: fields.required("iri")?,
            relation: fields.required("relation")?,
            morphism_class: fields.required("morphism_class")?,
            morphism_kind: fields.required("morphism_kind")?,
            mnemomorphic: fields.required("mnemomorphic")?,
            determinacy: fields.optional("determinacy")?,
            get_leg: fields.optional("get_leg")?,
            put_leg: fields.optional("put_leg")?,
            law_claims: fields.required("law_claims")?,
            caveats: fields.required("caveats")?,
            loss_evidence: fields.required_with("loss_evidence", literal_serde::from_json_set)?,
            confidence: fields.optional("confidence")?,
            evidence_strength: fields.optional("evidence_strength")?,
            axis_evidence: fields.required("axis_evidence")?,
            weight: fields.optional("weight")?,
            probability: fields.optional("probability")?,
            according_to: fields.optional("according_to")?,
            preservation: fields.optional("preservation")?,
            source_endpoint: fields.optional("source_endpoint")?,
            target_endpoint: fields.optional("target_endpoint")?,
            grounding: fields.required("grounding")?,
            recovery_cases: fields.required("recovery_cases")?,
        })
    }
}

#[cfg(test)]
mod tests;
