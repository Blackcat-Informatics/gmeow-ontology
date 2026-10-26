// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete source-observation transport. Hydration retains evidence and never
//! executes a program, repairs graph identities or recomputes annotations.
//! Annotation records use the CL observation profile's signed i64 element domain.

use crate::annotation::{
    AnnotatedFactKey, AnnotatedQuad, AnnotatedTupleKey, AnnotationCertification,
    AnnotationDerivation, AnnotationLineageContract, AnnotationQueryClass, SemiringLaw,
};
use crate::external_relation::ProviderTupleSource;
use crate::result::PreservationClaim;
use crate::seam::{BudgetStatus, DerivationId, DerivedQuad};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson, Within},
};

::gmeow_errors::json::json_string_newtype!(DerivationId);

impl ToJson for BudgetStatus {
    fn to_json(&self) -> Value {
        match self {
            Self::Ok => "Ok",
            Self::Partial => "Partial",
            Self::Exhausted => "Exhausted",
        }
        .to_json()
    }
}
impl FromJson for BudgetStatus {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "BudgetStatus")? {
            "Ok" => Ok(Self::Ok),
            "Partial" => Ok(Self::Partial),
            "Exhausted" => Ok(Self::Exhausted),
            name => Err(DecodeError::unknown_variant(
                name,
                &["Ok", "Partial", "Exhausted"],
            )),
        }
    }
}

impl ToJson for SemiringLaw {
    fn to_json(&self) -> Value {
        match self {
            Self::AddCommutative => "AddCommutative",
            Self::AddAssociative => "AddAssociative",
            Self::AdditiveIdentity => "AdditiveIdentity",
            Self::MultiplyAssociative => "MultiplyAssociative",
            Self::MultiplicativeIdentity => "MultiplicativeIdentity",
            Self::Distributive => "Distributive",
            Self::ZeroAnnihilates => "ZeroAnnihilates",
        }
        .to_json()
    }
}
impl FromJson for SemiringLaw {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "SemiringLaw")? {
            "AddCommutative" => Ok(Self::AddCommutative),
            "AddAssociative" => Ok(Self::AddAssociative),
            "AdditiveIdentity" => Ok(Self::AdditiveIdentity),
            "MultiplyAssociative" => Ok(Self::MultiplyAssociative),
            "MultiplicativeIdentity" => Ok(Self::MultiplicativeIdentity),
            "Distributive" => Ok(Self::Distributive),
            "ZeroAnnihilates" => Ok(Self::ZeroAnnihilates),
            name => Err(DecodeError::unknown_variant(
                name,
                &[
                    "AddCommutative",
                    "AddAssociative",
                    "AdditiveIdentity",
                    "MultiplyAssociative",
                    "MultiplicativeIdentity",
                    "Distributive",
                    "ZeroAnnihilates",
                ],
            )),
        }
    }
}

impl ToJson for AnnotationQueryClass {
    fn to_json(&self) -> Value {
        match self {
            Self::PositiveAcyclic => "PositiveAcyclic",
            Self::PositiveRecursive => "PositiveRecursive",
            Self::PositiveNaryAcyclic => "PositiveNaryAcyclic",
            Self::PositiveNaryRecursive => "PositiveNaryRecursive",
            Self::StratifiedNaf => "StratifiedNaf",
            Self::StratifiedAggregate => "StratifiedAggregate",
            Self::StratifiedAggregateChase => "StratifiedAggregateChase",
            Self::WellFounded => "WellFounded",
            Self::StableModel => "StableModel",
            Self::ExistentialChase => "ExistentialChase",
        }
        .to_json()
    }
}
impl FromJson for AnnotationQueryClass {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "AnnotationQueryClass")? {
            "PositiveAcyclic" => Ok(Self::PositiveAcyclic),
            "PositiveRecursive" => Ok(Self::PositiveRecursive),
            "PositiveNaryAcyclic" => Ok(Self::PositiveNaryAcyclic),
            "PositiveNaryRecursive" => Ok(Self::PositiveNaryRecursive),
            "StratifiedNaf" => Ok(Self::StratifiedNaf),
            "StratifiedAggregate" => Ok(Self::StratifiedAggregate),
            "StratifiedAggregateChase" => Ok(Self::StratifiedAggregateChase),
            "WellFounded" => Ok(Self::WellFounded),
            "StableModel" => Ok(Self::StableModel),
            "ExistentialChase" => Ok(Self::ExistentialChase),
            name => Err(DecodeError::unknown_variant(
                name,
                &[
                    "PositiveAcyclic",
                    "PositiveRecursive",
                    "PositiveNaryAcyclic",
                    "PositiveNaryRecursive",
                    "StratifiedNaf",
                    "StratifiedAggregate",
                    "StratifiedAggregateChase",
                    "WellFounded",
                    "StableModel",
                    "ExistentialChase",
                ],
            )),
        }
    }
}

impl ToJson for AnnotationLineageContract {
    fn to_json(&self) -> Value {
        match self {
            Self::AllPhysicalDerivations => "AllPhysicalDerivations",
            Self::CompleteGroupSupport => "CompleteGroupSupport",
            Self::SelectedPhysicalDerivation => "SelectedPhysicalDerivation",
        }
        .to_json()
    }
}
impl FromJson for AnnotationLineageContract {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "AnnotationLineageContract")? {
            "AllPhysicalDerivations" => Ok(Self::AllPhysicalDerivations),
            "CompleteGroupSupport" => Ok(Self::CompleteGroupSupport),
            "SelectedPhysicalDerivation" => Ok(Self::SelectedPhysicalDerivation),
            name => Err(DecodeError::unknown_variant(
                name,
                &[
                    "AllPhysicalDerivations",
                    "CompleteGroupSupport",
                    "SelectedPhysicalDerivation",
                ],
            )),
        }
    }
}

::purrdf_lex::json_record!(impl ToJson for AnnotationCertification {
    "query_class" => query_class,
    "preservation" => preservation,
    "declared_deviations" => declared_deviations,
    "lineage_contract" => lineage_contract,
});
impl FromJson for AnnotationCertification {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct AnnotationCertification")?;
        Ok(Self {
            query_class: fields.required("query_class")?,
            preservation: fields.required("preservation")?,
            declared_deviations: fields.required("declared_deviations")?,
            lineage_contract: fields.required("lineage_contract")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for AnnotatedFactKey {
    "graph" => graph,
    "subject" => subject,
    "predicate" => predicate,
    "object" => object,
});
impl FromJson for AnnotatedFactKey {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct AnnotatedFactKey")?;
        Ok(Self {
            graph: fields.required("graph")?,
            subject: fields.required("subject")?,
            predicate: fields.required("predicate")?,
            object: fields.required("object")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for AnnotatedTupleKey {
    "graph" => graph,
    "relation" => relation,
    "arguments" => arguments,
});
impl FromJson for AnnotatedTupleKey {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct AnnotatedTupleKey")?;
        Ok(Self {
            graph: fields.required("graph")?,
            relation: fields.required("relation")?,
            arguments: fields.required("arguments")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for ProviderTupleSource {
    "provider_iri" => provider_iri,
    "artifact_generation" => artifact_generation,
    "model_iri" => model_iri,
    "request_iri" => request_iri,
    "relation_iri" => relation_iri,
    "arguments" => arguments,
    "annotation_dimension_iri" => annotation_dimension_iri,
});
impl FromJson for ProviderTupleSource {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ProviderTupleSource")?;
        Ok(Self {
            provider_iri: fields.required("provider_iri")?,
            artifact_generation: fields.required("artifact_generation")?,
            model_iri: fields.required("model_iri")?,
            request_iri: fields.required("request_iri")?,
            relation_iri: fields.required("relation_iri")?,
            arguments: fields.required("arguments")?,
            annotation_dimension_iri: fields.required("annotation_dimension_iri")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for PreservationClaim {
    "polarities" => polarities,
    "unsupported_constructs" => unsupported_constructs,
});
impl FromJson for PreservationClaim {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct PreservationClaim")?;
        let claim = Self {
            polarities: fields.required("polarities")?,
            unsupported_constructs: fields.required("unsupported_constructs")?,
        };
        claim
            .validate()
            .map_err(|error| DecodeError::custom(error).within("polarities"))?;
        Ok(claim)
    }
}
impl ToJson for DerivedQuad {
    fn to_json(&self) -> Value {
        Object::new()
            .with("graph", self.graph.to_json())
            .with("subject", crate::term_serde::to_json(&self.subject))
            .with("predicate", self.predicate.to_json())
            .with("object", crate::term_serde::to_json(&self.object))
            .with("graph_component", self.graph_component.to_json())
            .with("derivation_id", self.derivation_id.to_json())
            .with("rule_iri", self.rule_iri.to_json())
            .with("source_quad_ids", self.source_quad_ids.to_json())
            .with("profile", self.profile.to_json())
            .with("budget_status", self.budget_status.to_json())
            .into()
    }
}
impl FromJson for DerivedQuad {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct DerivedQuad")?;
        let quad = Self {
            graph: fields.required("graph")?,
            subject: fields.required_with("subject", crate::term_serde::from_json)?,
            predicate: fields.required("predicate")?,
            object: fields.required_with("object", crate::term_serde::from_json)?,
            graph_component: fields.required("graph_component")?,
            derivation_id: fields.required("derivation_id")?,
            rule_iri: fields.required("rule_iri")?,
            source_quad_ids: fields.required("source_quad_ids")?,
            profile: fields.required("profile")?,
            budget_status: fields.required("budget_status")?,
        };
        if quad.graph != quad.graph_component {
            return Err(
                DecodeError::custom("DerivedQuad.graph_component must equal graph")
                    .within("graph_component"),
            );
        }
        Ok(quad)
    }
}
fn annotation_weight(value: &Value) -> Result<i64, DecodeError> {
    gmeow_errors::json::read_signed(value, "i64 annotation integer excluding -0")
}

::purrdf_lex::json_record!(impl ToJson for AnnotationDerivation<i64> {
    "rule_iri" => rule_iri,
    "sources" => sources,
    "tuple_sources" => tuple_sources,
    "provider_sources" => provider_sources,
    "annotation" => annotation,
});
impl FromJson for AnnotationDerivation<i64> {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct AnnotationDerivation")?;
        Ok(Self {
            rule_iri: fields.required("rule_iri")?,
            sources: fields.required("sources")?,
            tuple_sources: fields.required("tuple_sources")?,
            provider_sources: fields.required("provider_sources")?,
            annotation: fields.required_with("annotation", annotation_weight)?,
        })
    }
}
::purrdf_lex::json_record!(impl ToJson for AnnotatedQuad<i64> {
    "quad" => quad,
    "annotation" => annotation,
    "derivations" => derivations,
});
impl FromJson for AnnotatedQuad<i64> {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct AnnotatedQuad")?;
        Ok(Self {
            quad: fields.required("quad")?,
            annotation: fields.required_with("annotation", annotation_weight)?,
            derivations: fields.required("derivations")?,
        })
    }
}

#[cfg(test)]
mod tests;
