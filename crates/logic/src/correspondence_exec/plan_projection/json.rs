// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete planned-skeleton payload transport; admission never verifies receipts.

use super::{
    FreshnessRequirement, ObservedPlanOccurrence, PlanLiteral, PlanProjection,
    PlanProjectionCertificate, PlanProjectionComplement, PlanRecovery, PlanRecoveryVerdict,
    PlannedEdge, PlannedEdgeKind, PlannedStep, PlannedStepKind, ProjectedAction,
    ProjectedCarrierFact, ProjectedConditionalStep, ProjectedExpression, ProjectedExpressionFact,
    ProjectedGuard, ProjectedOutcome,
};
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

::purrdf_lex::json_record!(impl ToJson for PlannedStep {
    "id" => id,
    "cycle" => cycle,
    "position" => position,
    "source_node" => source_node,
    "kind" => kind,
});
impl FromJson for PlannedStep {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct PlannedStep")?;
        Ok(Self {
            id: fields.required("id")?,
            cycle: fields.required_with("cycle", |value| {
                gmeow_errors::json::read_unsigned(value, "usize cycle")
            })?,
            position: fields.required_with("position", |value| {
                gmeow_errors::json::read_unsigned(value, "usize position")
            })?,
            source_node: fields.required("source_node")?,
            kind: fields.required("kind")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for PlannedEdge {
    "from" => from,
    "to" => to,
    "kind" => kind,
});
impl FromJson for PlannedEdge {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct PlannedEdge")?;
        Ok(Self {
            from: fields.required("from")?,
            to: fields.required("to")?,
            kind: fields.required("kind")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for ProjectedOutcome {
    "source_node" => source_node,
    "case" => case,
    "continuation" => continuation,
    "compensation" => compensation,
});
impl FromJson for ProjectedOutcome {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ProjectedOutcome")?;
        Ok(Self {
            source_node: fields.required("source_node")?,
            case: fields.required("case")?,
            continuation: fields.optional("continuation")?,
            compensation: fields.optional("compensation")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for ProjectedExpressionFact {
    "subject" => subject,
    "predicate" => predicate,
    "object" => object,
});
impl FromJson for ProjectedExpressionFact {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        gmeow_errors::json::read_statement_fields(value, "struct ProjectedExpressionFact").map(
            |(subject, predicate, object)| ProjectedExpressionFact {
                subject,
                predicate,
                object,
            },
        )
    }
}

::purrdf_lex::json_record!(impl ToJson for ProjectedExpression {
    "root" => root,
    "digest" => digest,
    "facts" => facts,
    "tracked_states" => tracked_states,
});
impl FromJson for ProjectedExpression {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ProjectedExpression")?;
        Ok(Self {
            root: fields.required("root")?,
            digest: fields.required("digest")?,
            facts: fields.required("facts")?,
            tracked_states: fields.required("tracked_states")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for ProjectedConditionalStep {
    "source_node" => source_node,
    "guard" => guard,
    "then_steps" => then_steps,
});
impl FromJson for ProjectedConditionalStep {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ProjectedConditionalStep")?;
        Ok(Self {
            source_node: fields.required("source_node")?,
            guard: fields.required("guard")?,
            then_steps: fields.required("then_steps")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for ProjectedAction {
    "schema" => schema,
    "preconditions" => preconditions,
    "effects" => effects,
    "resources" => resources,
    "capabilities" => capabilities,
    "observations" => observations,
    "outcomes" => outcomes,
    "conditional_steps" => conditional_steps,
});
impl FromJson for ProjectedAction {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ProjectedAction")?;
        Ok(Self {
            schema: fields.required("schema")?,
            preconditions: fields.required("preconditions")?,
            effects: fields.required("effects")?,
            resources: fields.required("resources")?,
            capabilities: fields.required("capabilities")?,
            observations: fields.required("observations")?,
            outcomes: fields.required("outcomes")?,
            conditional_steps: fields.required("conditional_steps")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for ProjectedGuard {
    "guard" => guard,
    "condition" => condition,
    "then_steps" => then_steps,
    "else_steps" => else_steps,
});
impl FromJson for ProjectedGuard {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ProjectedGuard")?;
        Ok(Self {
            guard: fields.required("guard")?,
            condition: fields.required("condition")?,
            then_steps: fields.required("then_steps")?,
            else_steps: fields.required("else_steps")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for PlanLiteral {
    "lexical_form" => lexical_form,
    "datatype" => datatype,
    "language" => language,
    "direction" => direction,
});
impl FromJson for PlanLiteral {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct PlanLiteral")?;
        Ok(Self {
            lexical_form: fields.required("lexical_form")?,
            datatype: fields.required("datatype")?,
            language: fields.optional("language")?,
            direction: fields.optional("direction")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for FreshnessRequirement {
    "tracked_state" => tracked_state,
    "currency" => currency,
});
impl FromJson for FreshnessRequirement {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct FreshnessRequirement")?;
        Ok(Self {
            tracked_state: fields.required("tracked_state")?,
            currency: fields.required("currency")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for ProjectedCarrierFact {
    "graph" => graph,
    "subject" => subject,
    "predicate" => predicate,
    "object" => object,
});
impl FromJson for ProjectedCarrierFact {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ProjectedCarrierFact")?;
        Ok(Self {
            graph: fields.optional("graph")?,
            subject: fields.required("subject")?,
            predicate: fields.required("predicate")?,
            object: fields.required("object")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for PlanProjectionComplement {
    "source_digest" => source_digest,
    "source_graph" => source_graph,
    "plan" => plan,
    "standpoint" => standpoint,
    "loop_body" => loop_body,
    "expression_digests" => expression_digests,
    "source_facts" => source_facts,
});
impl FromJson for PlanProjectionComplement {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct PlanProjectionComplement")?;
        Ok(Self {
            source_digest: fields.required("source_digest")?,
            source_graph: fields.optional("source_graph")?,
            plan: fields.required("plan")?,
            standpoint: fields.optional("standpoint")?,
            loop_body: fields.required("loop_body")?,
            expression_digests: fields.required("expression_digests")?,
            source_facts: fields.required("source_facts")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for PlanProjectionCertificate {
    "version" => version,
    "output_contract" => output_contract,
    "plan" => plan,
    "source_digest" => source_digest,
    "projection_digest" => projection_digest,
    "certificate_digest" => certificate_digest,
});
impl FromJson for PlanProjectionCertificate {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct PlanProjectionCertificate")?;
        Ok(Self {
            version: fields.required("version")?,
            output_contract: fields.required("output_contract")?,
            plan: fields.required("plan")?,
            source_digest: fields.required("source_digest")?,
            projection_digest: fields.required("projection_digest")?,
            certificate_digest: fields.required("certificate_digest")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for PlanProjection {
    "plan" => plan,
    "source_graph" => source_graph,
    "standpoint" => standpoint,
    "preconditions" => preconditions,
    "loop_count" => loop_count,
    "loop_variable" => loop_variable,
    "loop_period" => loop_period,
    "loop_body" => loop_body,
    "steps" => steps,
    "edges" => edges,
    "actions" => actions,
    "guards" => guards,
    "freshness" => freshness,
    "loss_evidence" => loss_evidence,
    "complement" => complement,
    "certificate" => certificate,
});
impl FromJson for PlanProjection {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct PlanProjection")?;
        Ok(Self {
            plan: fields.required("plan")?,
            source_graph: fields.optional("source_graph")?,
            standpoint: fields.optional("standpoint")?,
            preconditions: fields.required("preconditions")?,
            loop_count: fields.required_with("loop_count", |value| {
                gmeow_errors::json::read_unsigned(value, "usize loop_count")
            })?,
            loop_variable: fields.optional("loop_variable")?,
            loop_period: fields.optional("loop_period")?,
            loop_body: fields.required("loop_body")?,
            steps: fields.required("steps")?,
            edges: fields.required("edges")?,
            actions: fields.required("actions")?,
            guards: fields.required("guards")?,
            freshness: fields.required("freshness")?,
            loss_evidence: fields.required("loss_evidence")?,
            complement: fields.required("complement")?,
            certificate: fields.required("certificate")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for ObservedPlanOccurrence {
    "occurrence" => occurrence,
    "graph" => graph,
    "schema" => schema,
});
impl FromJson for ObservedPlanOccurrence {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ObservedPlanOccurrence")?;
        Ok(Self {
            occurrence: fields.required("occurrence")?,
            graph: fields.optional("graph")?,
            schema: fields.required("schema")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for PlanRecovery {
    "version" => version,
    "output_contract" => output_contract,
    "plan" => plan,
    "projection_certificate_digest" => projection_certificate_digest,
    "observed_input_digest" => observed_input_digest,
    "verdict" => verdict,
    "occurrences" => occurrences,
    "recovered_schemas" => recovered_schemas,
    "unobserved_planned_schemas" => unobserved_planned_schemas,
    "off_plan_occurrences" => off_plan_occurrences,
    "loss_evidence" => loss_evidence,
    "recovery_digest" => recovery_digest,
});
impl FromJson for PlanRecovery {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct PlanRecovery")?;
        Ok(Self {
            version: fields.required("version")?,
            output_contract: fields.required("output_contract")?,
            plan: fields.required("plan")?,
            projection_certificate_digest: fields.required("projection_certificate_digest")?,
            observed_input_digest: fields.required("observed_input_digest")?,
            verdict: fields.required("verdict")?,
            occurrences: fields.required("occurrences")?,
            recovered_schemas: fields.required("recovered_schemas")?,
            unobserved_planned_schemas: fields.required("unobserved_planned_schemas")?,
            off_plan_occurrences: fields.required("off_plan_occurrences")?,
            loss_evidence: fields.required("loss_evidence")?,
            recovery_digest: fields.required("recovery_digest")?,
        })
    }
}

impl ToJson for PlannedStepKind {
    fn to_json(&self) -> Value {
        match self {
            Self::Action => "action",
            Self::Guard => "guard",
        }
        .to_json()
    }
}
impl FromJson for PlannedStepKind {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "PlannedStepKind")? {
            "action" => Ok(Self::Action),
            "guard" => Ok(Self::Guard),
            name => Err(DecodeError::unknown_variant(name, &["action", "guard"])),
        }
    }
}

impl ToJson for PlannedEdgeKind {
    fn to_json(&self) -> Value {
        match self {
            Self::Serial => "serial",
            Self::GuardThen => "guard-then",
            Self::GuardElse => "guard-else",
            Self::NextIteration => "next-iteration",
        }
        .to_json()
    }
}
impl FromJson for PlannedEdgeKind {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "PlannedEdgeKind")? {
            "serial" => Ok(Self::Serial),
            "guard-then" => Ok(Self::GuardThen),
            "guard-else" => Ok(Self::GuardElse),
            "next-iteration" => Ok(Self::NextIteration),
            name => Err(DecodeError::unknown_variant(
                name,
                &["serial", "guard-then", "guard-else", "next-iteration"],
            )),
        }
    }
}

impl ToJson for PlanRecoveryVerdict {
    fn to_json(&self) -> Value {
        match self {
            Self::PlannedSkeletonRecovered => "planned-skeleton-recovered",
        }
        .to_json()
    }
}
impl FromJson for PlanRecoveryVerdict {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "PlanRecoveryVerdict")? {
            "planned-skeleton-recovered" => Ok(Self::PlannedSkeletonRecovered),
            name => Err(DecodeError::unknown_variant(
                name,
                &["planned-skeleton-recovered"],
            )),
        }
    }
}

#[cfg(test)]
mod tests;
