// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Data-only native transport of producer observations. Object fields retain the
//! channel's recursive lexical order; vectors and rendered strings remain untouched.

use super::{
    GoalDirectedAnswer, GoalDirectedEvaluation, GoalDirectedObservation, GoalDirectedVerdict,
};
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record},
};

::purrdf_lex::json_record!(impl ToJson for GoalDirectedAnswer {
    "atom" => atom,
    "bindings" => bindings,
    "derivation_iri" => derivation_iri,
    "proof_checks" => proof_checks,
});

impl FromJson for GoalDirectedAnswer {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct GoalDirectedAnswer")?;
        Ok(Self {
            atom: fields.required("atom")?,
            bindings: fields.required("bindings")?,
            derivation_iri: fields.required("derivation_iri")?,
            proof_checks: fields.required("proof_checks")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for GoalDirectedVerdict {
    "atom" => atom,
    "verdict" => verdict,
});

impl FromJson for GoalDirectedVerdict {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct GoalDirectedVerdict")?;
        Ok(Self {
            atom: fields.required("atom")?,
            verdict: fields.required("verdict")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for GoalDirectedEvaluation {
    "answers" => answers,
    "clauses" => clauses,
    "description" => description,
    "goal" => goal,
    "iri" => iri,
    "name" => name,
    "status" => status,
    "verdict_probe_atoms" => verdict_probe_atoms,
    "verdicts" => verdicts,
});

impl FromJson for GoalDirectedEvaluation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct GoalDirectedEvaluation")?;
        Ok(Self {
            answers: fields.required("answers")?,
            clauses: fields.required("clauses")?,
            description: fields.required("description")?,
            goal: fields.required("goal")?,
            iri: fields.required("iri")?,
            name: fields.required("name")?,
            status: fields.required("status")?,
            verdict_probe_atoms: fields.required("verdict_probe_atoms")?,
            verdicts: fields.required("verdicts")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for GoalDirectedObservation {
    "evaluations" => evaluations,
    "program_projections" => program_projections,
    "projection" => projection,
    "repeated_program_projections" => repeated_program_projections,
    "repeated_projection" => repeated_projection,
});

impl FromJson for GoalDirectedObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct GoalDirectedObservation")?;
        Ok(Self {
            evaluations: fields.required("evaluations")?,
            program_projections: fields.required("program_projections")?,
            projection: fields.required("projection")?,
            repeated_program_projections: fields.required("repeated_program_projections")?,
            repeated_projection: fields.required("repeated_projection")?,
        })
    }
}

#[path = "json/tests.rs"]
#[cfg(test)]
mod tests;
