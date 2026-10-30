// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native coherence observations; records preserve their existing open-field profile.

use super::{
    CharacteristicObservation, DisjointObservation, FoundationObservation, IriQuad,
    RelcompObservation,
};
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record},
};

/// Decode exact subject/predicate violation rows through shared string admission.
fn violation_pairs(value: &Value) -> Result<Vec<(String, String)>, DecodeError> {
    purrdf_lex::json::record::items_with(value, |value| {
        let [subject, predicate] = gmeow_errors::json::read_string_row::<2>(value)?;
        Ok((subject, predicate))
    })
}

::purrdf_lex::json_record! { impl ToJson for IriQuad {
    "subject" => subject,
    "predicate" => predicate,
    "object" => object,
    "graph" => graph,
}}

impl FromJson for IriQuad {
    /// Decode the complete record while retaining its existing open field profile.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct IriQuad")?;
        Ok(Self {
            subject: fields.required("subject")?,
            predicate: fields.required("predicate")?,
            object: fields.required("object")?,
            graph: fields.required("graph")?,
        })
    }
}

::purrdf_lex::json_record! { impl ToJson for DisjointObservation {
    "disjoint_world" => disjoint_world,
    "verdict" => verdict,
}}

impl FromJson for DisjointObservation {
    /// Decode the complete record while retaining its existing open field profile.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct DisjointObservation")?;
        Ok(Self {
            disjoint_world: fields.required("disjoint_world")?,
            verdict: fields.required("verdict")?,
        })
    }
}

::purrdf_lex::json_record! { impl ToJson for RelcompObservation {
    "clean_offenders" => clean_offenders,
    "poisoned_offenders" => poisoned_offenders,
}}

impl FromJson for RelcompObservation {
    /// Decode the complete record while retaining its existing open field profile.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct RelcompObservation")?;
        Ok(Self {
            clean_offenders: fields.required("clean_offenders")?,
            poisoned_offenders: fields.required("poisoned_offenders")?,
        })
    }
}

::purrdf_lex::json_record! { impl ToJson for FoundationObservation {
    "modal_evaluation" => modal_evaluation,
    "graph" => graph,
    "subject" => subject,
    "predicate" => predicate,
    "object" => object,
    "rule_iri" => rule_iri,
    "source_quad_ids" => source_quad_ids,
    "derivation_id" => derivation_id,
}}

impl FromJson for FoundationObservation {
    /// Decode the complete record while retaining its existing open field profile.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct FoundationObservation")?;
        Ok(Self {
            modal_evaluation: fields.optional("modal_evaluation")?,
            graph: fields.required("graph")?,
            subject: fields.required("subject")?,
            predicate: fields.required("predicate")?,
            object: fields.required("object")?,
            rule_iri: fields.required("rule_iri")?,
            source_quad_ids: fields.required("source_quad_ids")?,
            derivation_id: fields.required("derivation_id")?,
        })
    }
}

::purrdf_lex::json_record! { impl ToJson for CharacteristicObservation {
    "carrier_facts" => carrier_facts,
    "clean_violations" => clean_violations,
    "clean_disagreements" => clean_disagreements,
    "poisoned" => poisoned,
}}

impl FromJson for CharacteristicObservation {
    /// Decode the complete record while retaining its existing open field profile.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct CharacteristicObservation")?;
        Ok(Self {
            carrier_facts: fields.required("carrier_facts")?,
            clean_violations: fields.required_with("clean_violations", violation_pairs)?,
            clean_disagreements: fields.required("clean_disagreements")?,
            poisoned: fields.required("poisoned")?,
        })
    }
}
