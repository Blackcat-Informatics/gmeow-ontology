// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! List admission over the complete projection already built by the producer.

use std::collections::BTreeMap;

pub(crate) use gmeow_logic::reason::refute::LIST_CORPUS_CHANNEL as CHANNEL;
use gmeow_logic::reason::refute::{
    FragmentBoundary, ListCorpusObservation, PreparedClassAnalysis, RefutationSourceIssue,
    encode_list_corpus_observation,
};
use gmeow_logic_compile::frontend::CompiledTheory;
use purrdf::{DatasetView, GraphMatch, RdfDataset, RdfTerm};

use crate::stages::parse_sources::SourceCatalog;

pub(super) fn record(
    catalog: &SourceCatalog,
    theory: &CompiledTheory,
    canonical: &RdfDataset,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let observation = super::observed(observe(catalog, theory, canonical));
    artifacts.insert(
        CHANNEL.to_owned(),
        encode_list_corpus_observation(&observation),
    );
    Ok(())
}

fn observe(
    catalog: &SourceCatalog,
    theory: &CompiledTheory,
    canonical: &RdfDataset,
) -> gmeow_errors::Result<ListCorpusObservation> {
    // This is the exact default-graph projection subsequently folded into
    // graph/logic. Never parse source documents or compile another program here.
    let analysis = PreparedClassAnalysis::new(canonical)?;
    let admission = analysis.admission();
    let mut admission_defects = Vec::new();
    for world in admission.selected_worlds.values() {
        if let Some(boundary) = &world.refusal {
            collect_list_defects(boundary, &mut admission_defects)?;
        }
    }
    let mut observation = ListCorpusObservation {
        schema_version: 1,
        source_catalog_id: catalog.identity().to_owned(),
        source_modules: catalog
            .sources()
            .sources()
            .iter()
            .filter(|source| {
                source.relative_path.starts_with("slices/")
                    && source.relative_path.ends_with("/module.ttl")
            })
            .map(|source| (source.relative_path.clone(), source.content_digest.clone()))
            .collect(),
        destination_graph: super::GRAPH_LOGIC.to_owned(),
        admission_contract: admission.contract.clone(),
        selected_world_count: admission.selected_worlds.len(),
        list_diagnostics: theory
            .diagnostics()
            .iter()
            .filter(|diagnostic| {
                diagnostic.code.contains("LIST")
                    || diagnostic.code.contains("ENUMERATION")
                    || diagnostic.code == "UNSUPPORTED_NESTED_CLASS_EXPRESSION"
            })
            .cloned()
            .collect(),
        blank_list_fields: Vec::new(),
        invalid_constructor_objects: Vec::new(),
        list_sizes: BTreeMap::new(),
        list_errors: Vec::new(),
        admission_defects,
    };
    for quad in canonical.owned_quads() {
        let constructor = gmeow_logic_compile::lists::constructor(&quad.predicate).is_some();
        let field = matches!(
            quad.predicate.as_str(),
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#first"
                | "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest"
        );
        if (constructor || field)
            && (matches!(quad.subject, RdfTerm::BlankNode(_))
                || matches!(quad.object, RdfTerm::BlankNode(_)))
        {
            observation.blank_list_fields.push(format!("{quad:?}"));
        }
        if !constructor {
            continue;
        }
        let RdfTerm::Iri(head) = &quad.object else {
            observation
                .invalid_constructor_objects
                .push(format!("{quad:?}"));
            continue;
        };
        if observation.list_sizes.contains_key(head) {
            continue;
        }
        let head_id = canonical
            .term_id_by_iri(head)
            .ok_or_else(|| super::stage_err(format!("missing constructor head {head}")))?;
        match canonical.rdf_list_strict(head_id, GraphMatch::Default) {
            Ok(members) => {
                observation.list_sizes.insert(head.clone(), members.len());
            }
            Err(error) => observation.list_errors.push(format!("{head}: {error:?}")),
        }
    }
    Ok(observation)
}

fn collect_list_defects(
    boundary: &FragmentBoundary,
    defects: &mut Vec<RefutationSourceIssue>,
) -> gmeow_errors::Result<()> {
    match boundary {
        FragmentBoundary::SourceAdmission { issue, .. } => {
            if matches!(
                issue,
                RefutationSourceIssue::IncompleteList { .. }
                    | RefutationSourceIssue::CyclicList { .. }
                    | RefutationSourceIssue::MalformedNil
                    | RefutationSourceIssue::ConflictingListField { .. }
                    | RefutationSourceIssue::ExpressionMultiplicity { .. }
            ) {
                defects.push(issue.clone());
            }
        }
        FragmentBoundary::Combined(boundaries) => {
            for boundary in boundaries {
                collect_list_defects(boundary, defects)?;
            }
        }
        other => {
            return Err(super::stage_err(format!(
                "list admission reported a non-source boundary: {other:?}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "list_corpus.tests.rs"]
mod tests;
