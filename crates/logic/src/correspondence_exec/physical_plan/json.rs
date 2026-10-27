// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete physical-plan report transport, independent of planning and verification.

use super::{
    CompositionLogicalStage, CompositionOptimizationScope, CompositionPhysicalPlan,
    CompositionPlanCertificate, CompositionPlanCost, CompositionPlanLimits, CompositionPlanReport,
    CompositionStageCheck, OriginalPlanReason,
};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson, Within},
};

::purrdf_lex::json_record!(impl ToJson for CompositionPlanLimits {
    "max_path_nodes" => max_path_nodes,
});
impl FromJson for CompositionPlanLimits {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct CompositionPlanLimits")?;
        Ok(Self {
            max_path_nodes: fields.required_with("max_path_nodes", |value| {
                gmeow_errors::json::read_unsigned(value, "usize max_path_nodes")
            })?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for CompositionOptimizationScope {
    "program_digest" => program_digest,
    "declaration_digest" => declaration_digest,
    "context_digest" => context_digest,
    "effect_digest" => effect_digest,
    "complement_digest" => complement_digest,
    "preservation_digest" => preservation_digest,
    "reasoning_contract_digest" => reasoning_contract_digest,
});
impl FromJson for CompositionOptimizationScope {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct CompositionOptimizationScope")?;
        Ok(Self {
            program_digest: fields.required("program_digest")?,
            declaration_digest: fields.required("declaration_digest")?,
            context_digest: fields.required("context_digest")?,
            effect_digest: fields.required("effect_digest")?,
            complement_digest: fields.required("complement_digest")?,
            preservation_digest: fields.required("preservation_digest")?,
            reasoning_contract_digest: fields.required("reasoning_contract_digest")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for CompositionLogicalStage {
    "correspondence" => correspondence,
    "get_leg" => get_leg,
    "path" => path,
    "source_endpoint" => source_endpoint,
    "target_endpoint" => target_endpoint,
    "standpoint" => standpoint,
    "checks" => checks,
});
impl FromJson for CompositionLogicalStage {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct CompositionLogicalStage")?;
        Ok(Self {
            correspondence: fields.required("correspondence")?,
            get_leg: fields.required("get_leg")?,
            path: fields.required("path")?,
            source_endpoint: fields.required("source_endpoint")?,
            target_endpoint: fields.required("target_endpoint")?,
            standpoint: fields.optional("standpoint")?,
            checks: fields.required("checks")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for CompositionPlanCost {
    "native_queries" => native_queries,
    "materialized_intermediate_relations" => materialized_intermediate_relations,
    "path_nodes" => path_nodes,
    "stable_tiebreak" => stable_tiebreak,
});
impl FromJson for CompositionPlanCost {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct CompositionPlanCost")?;
        Ok(Self {
            native_queries: fields.required_with("native_queries", |value| {
                gmeow_errors::json::read_unsigned(value, "usize native_queries")
            })?,
            materialized_intermediate_relations: fields.required_with(
                "materialized_intermediate_relations",
                |value| {
                    gmeow_errors::json::read_unsigned(
                        value,
                        "usize materialized_intermediate_relations",
                    )
                },
            )?,
            path_nodes: fields.required_with("path_nodes", |value| {
                gmeow_errors::json::read_unsigned(value, "usize path_nodes")
            })?,
            stable_tiebreak: fields.required("stable_tiebreak")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for CompositionPlanCertificate {
    "version" => version,
    "declaration" => declaration,
    "scope" => scope,
    "first" => first,
    "second" => second,
    "composite" => composite,
    "output_contract" => output_contract,
    "selected" => selected,
    "applied_rules" => applied_rules,
    "rule_catalog_digest" => rule_catalog_digest,
    "original_cost" => original_cost,
    "selected_cost" => selected_cost,
    "limits" => limits,
    "engine_descriptor_hash" => engine_descriptor_hash,
    "certificate_digest" => certificate_digest,
});
impl FromJson for CompositionPlanCertificate {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct CompositionPlanCertificate")?;
        Ok(Self {
            version: fields.required("version")?,
            declaration: fields.required("declaration")?,
            scope: fields.required("scope")?,
            first: fields.required("first")?,
            second: fields.required("second")?,
            composite: fields.required("composite")?,
            output_contract: fields.required("output_contract")?,
            selected: fields.required("selected")?,
            applied_rules: fields.required("applied_rules")?,
            rule_catalog_digest: fields.required("rule_catalog_digest")?,
            original_cost: fields.required("original_cost")?,
            selected_cost: fields.required("selected_cost")?,
            limits: fields.required("limits")?,
            engine_descriptor_hash: fields.required("engine_descriptor_hash")?,
            certificate_digest: fields.required("certificate_digest")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for CompositionPlanReport {
    "version" => version,
    "program_digest" => program_digest,
    "certificates" => certificates,
});
impl FromJson for CompositionPlanReport {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct CompositionPlanReport")?;
        Ok(Self {
            version: fields.required("version")?,
            program_digest: fields.required("program_digest")?,
            certificates: fields.required("certificates")?,
        })
    }
}

impl ToJson for CompositionStageCheck {
    fn to_json(&self) -> Value {
        match self {
            Self::EndpointType => "endpoint-type",
            Self::Standpoint => "standpoint",
            Self::PureReadEffect => "pure-read-effect",
            Self::PathAdmission => "path-admission",
            Self::ComplementRetention => "complement-retention",
            Self::Preservation => "preservation",
            Self::LogicalGate => "logical-gate",
        }
        .to_json()
    }
}
impl FromJson for CompositionStageCheck {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "CompositionStageCheck")? {
            "endpoint-type" => Ok(Self::EndpointType),
            "standpoint" => Ok(Self::Standpoint),
            "pure-read-effect" => Ok(Self::PureReadEffect),
            "path-admission" => Ok(Self::PathAdmission),
            "complement-retention" => Ok(Self::ComplementRetention),
            "preservation" => Ok(Self::Preservation),
            "logical-gate" => Ok(Self::LogicalGate),
            name => Err(DecodeError::unknown_variant(
                name,
                &[
                    "endpoint-type",
                    "standpoint",
                    "pure-read-effect",
                    "path-admission",
                    "complement-retention",
                    "preservation",
                    "logical-gate",
                ],
            )),
        }
    }
}

impl ToJson for OriginalPlanReason {
    fn to_json(&self) -> Value {
        match self {
            Self::SearchLimit => "search-limit",
            Self::NativeAdmission => "native-admission",
        }
        .to_json()
    }
}
impl FromJson for OriginalPlanReason {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "OriginalPlanReason")? {
            "search-limit" => Ok(Self::SearchLimit),
            "native-admission" => Ok(Self::NativeAdmission),
            name => Err(DecodeError::unknown_variant(
                name,
                &["search-limit", "native-admission"],
            )),
        }
    }
}

impl ToJson for CompositionPhysicalPlan {
    fn to_json(&self) -> Value {
        match self {
            Self::Original { reason } => Object::new()
                .with("kind", "original".to_json())
                .with("reason", reason.to_json())
                .into(),
            Self::FusedWitnessJoin => Object::new()
                .with("kind", "fused-witness-join".to_json())
                .into(),
        }
    }
}
impl FromJson for CompositionPhysicalPlan {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "internally tagged CompositionPhysicalPlan")?;
        let kind: String = fields.required("kind")?;
        match kind.as_str() {
            "original" => Ok(Self::Original {
                reason: fields.required("reason")?,
            }),
            // A unit variant has no payload fields. Unknown members, including
            // reason, retain the original internally tagged unit policy.
            "fused-witness-join" => Ok(Self::FusedWitnessJoin),
            name => Err(
                DecodeError::unknown_variant(name, &["original", "fused-witness-join"])
                    .within("kind"),
            ),
        }
    }
}

#[cfg(test)]
mod tests;
