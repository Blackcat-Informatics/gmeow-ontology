// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native DL diagnostic projections; hydration preserves stored evidence.

use super::{DlCoverage, DlGap, DlVerdict, InconsistencyWitness, UnsatClass};
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record},
};

/// Decode exact premise triples through the shared string-row owner.
fn premise_triples(value: &Value) -> Result<Vec<(String, String, String)>, DecodeError> {
    purrdf_lex::json::record::items_with(value, |value| {
        let [subject, predicate, object] = gmeow_errors::json::read_string_row::<3>(value)?;
        Ok((subject, predicate, object))
    })
}

::purrdf_lex::json_record! { impl ToJson for UnsatClass {
    "class" => class,
    "world" => world,
    "premises" => premises,
}}

impl FromJson for UnsatClass {
    /// Decode the complete record while retaining its existing open field profile.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct UnsatClass")?;
        Ok(Self {
            class: fields.required("class")?,
            world: fields.required("world")?,
            premises: fields.required_with("premises", premise_triples)?,
        })
    }
}

::purrdf_lex::json_record! { impl ToJson for InconsistencyWitness {
    "individual" => individual,
    "world" => world,
    "premises" => premises,
}}

impl FromJson for InconsistencyWitness {
    /// Decode the complete record while retaining its existing open field profile.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct InconsistencyWitness")?;
        Ok(Self {
            individual: fields.required("individual")?,
            world: fields.required("world")?,
            premises: fields.required_with("premises", premise_triples)?,
        })
    }
}

::purrdf_lex::json_record! { impl ToJson for DlCoverage {
    "present" => present,
    "decided" => decided,
    "unsupported" => unsupported,
}}

impl FromJson for DlCoverage {
    /// Decode the complete record while retaining its existing open field profile.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct DlCoverage")?;
        Ok(Self {
            present: fields.required("present")?,
            decided: fields.required("decided")?,
            unsupported: fields.required("unsupported")?,
        })
    }
}

::purrdf_lex::json_record! { impl ToJson for DlGap {
    "code" => code,
    "message" => message,
}}

impl FromJson for DlGap {
    /// Decode the complete record while retaining its existing open field profile.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct DlGap")?;
        Ok(Self {
            code: fields.required("code")?,
            message: fields.required("message")?,
        })
    }
}

::purrdf_lex::json_record! { impl ToJson for DlVerdict {
    "consistent" => consistent,
    "unsatisfiable_classes" => unsatisfiable_classes,
    "inconsistencies" => inconsistencies,
    "coverage" => coverage,
    "gaps" => gaps,
    "boundary_findings" => boundary_findings,
}}

impl FromJson for DlVerdict {
    /// Decode the complete record while retaining its existing open field profile.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct DlVerdict")?;
        Ok(Self {
            consistent: fields.required("consistent")?,
            unsatisfiable_classes: fields.required("unsatisfiable_classes")?,
            inconsistencies: fields.required("inconsistencies")?,
            coverage: fields.required("coverage")?,
            gaps: fields.required("gaps")?,
            boundary_findings: fields.required("boundary_findings")?,
        })
    }
}
