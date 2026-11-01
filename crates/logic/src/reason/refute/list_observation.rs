// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared data-only list projection observations and their recorded outcome.

use super::RefutationSourceIssue;
use gmeow_logic_compile::frontend::Diagnostic;
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record},
};
use std::collections::BTreeMap;

/// Exact artifact selected by the compile-logic producer and its consumers.
pub const LIST_CORPUS_CHANNEL: &str = "pipeline/logic-list-corpus-observation.json";

/// Retained source and list evidence. Hydration does not admit or rebuild a projection.
#[derive(Debug)]
pub struct ListCorpusObservation {
    /// Observation schema as supplied by the producer.
    pub schema_version: u32,
    /// Identity of the complete selected source catalog.
    pub source_catalog_id: String,
    /// Selected source modules and their original content digests.
    pub source_modules: BTreeMap<String, String>,
    /// Destination context of the retained logical projection.
    pub destination_graph: String,
    /// Original class-expression source-admission contract.
    pub admission_contract: String,
    /// Number of contexts selected by that admission.
    pub selected_world_count: usize,
    /// Frontend list and enumeration diagnostics, in source order.
    pub list_diagnostics: Vec<Diagnostic>,
    /// Original projected fields still containing blank nodes.
    pub blank_list_fields: Vec<String>,
    /// Original constructor rows whose objects are outside the required kind.
    pub invalid_constructor_objects: Vec<String>,
    /// Complete lengths of successfully traversed list heads.
    pub list_sizes: BTreeMap<String, usize>,
    /// Original traversal refusals, in observation order.
    pub list_errors: Vec<String>,
    /// Original selected source issues, without interpretation or normalization.
    pub admission_defects: Vec<RefutationSourceIssue>,
}

::purrdf_lex::json_record!(impl ToJson for ListCorpusObservation {
    "schema_version" => schema_version,
    "source_catalog_id" => source_catalog_id,
    "source_modules" => source_modules,
    "destination_graph" => destination_graph,
    "admission_contract" => admission_contract,
    "selected_world_count" => selected_world_count,
    "list_diagnostics" => list_diagnostics,
    "blank_list_fields" => blank_list_fields,
    "invalid_constructor_objects" => invalid_constructor_objects,
    "list_sizes" => list_sizes,
    "list_errors" => list_errors,
    "admission_defects" => admission_defects,
});

struct ListSize(usize);
impl FromJson for ListSize {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        gmeow_errors::json::read_unsigned(value, "usize list size").map(Self)
    }
}

impl FromJson for ListCorpusObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ListCorpusObservation")?;
        let schema_version = fields.required_with("schema_version", |value| {
            gmeow_errors::json::read_unsigned(value, "u32 list observation schema")
        })?;
        let source_catalog_id = fields.required("source_catalog_id")?;
        let source_modules = fields.required("source_modules")?;
        let destination_graph = fields.required("destination_graph")?;
        let admission_contract = fields.required("admission_contract")?;
        let selected_world_count = fields.required_with("selected_world_count", |value| {
            gmeow_errors::json::read_unsigned(value, "usize selected world count")
        })?;
        let list_diagnostics = fields.required("list_diagnostics")?;
        let blank_list_fields = fields.required("blank_list_fields")?;
        let invalid_constructor_objects = fields.required("invalid_constructor_objects")?;
        let list_sizes: BTreeMap<String, ListSize> = fields.required("list_sizes")?;
        Ok(Self {
            schema_version,
            source_catalog_id,
            source_modules,
            destination_graph,
            admission_contract,
            selected_world_count,
            list_diagnostics,
            blank_list_fields,
            invalid_constructor_objects,
            list_sizes: list_sizes
                .into_iter()
                .map(|(head, size)| (head, size.0))
                .collect(),
            list_errors: fields.required("list_errors")?,
            admission_defects: fields.required("admission_defects")?,
        })
    }
}

struct RecordedObservation(Result<ListCorpusObservation, gmeow_errors::RecordedDiag>);
impl FromJson for RecordedObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        gmeow_errors::json::read_recorded_result(
            value,
            "list corpus observation outcome",
            "one list corpus observation outcome variant",
        )
        .map(Self)
    }
}

/// Encode the complete recorded observation without cloning its payload.
#[must_use]
pub fn encode_list_corpus_observation(
    observation: &Result<ListCorpusObservation, gmeow_errors::RecordedDiag>,
) -> Vec<u8> {
    purrdf_lex::json::write_compact(&gmeow_errors::json::recorded_result_to_json(observation))
        .into_bytes()
}

/// Hydrate the producer's exact recorded outcome through native typed admission.
/// Corpus-success assertions belong to the authenticated consumer.
///
/// # Errors
/// Refuses malformed records, variants and typed fields under the native default depth policy.
pub fn decode_list_corpus_observation(
    bytes: &[u8],
) -> Result<Result<ListCorpusObservation, gmeow_errors::RecordedDiag>, DecodeError> {
    purrdf_lex::json::record::from_slice::<RecordedObservation>(bytes).map(|outcome| outcome.0)
}

#[cfg(test)]
#[path = "list_observation.tests.rs"]
mod tests;
