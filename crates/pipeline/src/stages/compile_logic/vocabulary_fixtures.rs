// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native vocabulary observations from the retained grounding document. The
//! producer records declarations; authenticated consumers compare them to Rust.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_logic_compile::graphutil::default_graph_pattern;
use gmeow_logic_compile::ir::LOGIC_NAMESPACE;
use purrdf::{RdfDataset, TermId, TermRef};
use serde::{Deserialize, Serialize};

const CHANNEL: &str = "pipeline/logic-vocabulary-fixtures.json";
const CLASSES: [&str; 12] = [
    "ReasoningPreset",
    "CompatibilityRule",
    "PreservationKind",
    "NodeKind",
    "FormulaShape",
    "CorrespondenceRelation",
    "MorphismClass",
    "MorphismKind",
    "Determinacy",
    "CorrespondenceLaw",
    "DischargeVerdict",
    "DischargeCondition",
];

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Observation {
    source: String,
    members: BTreeMap<String, Result<BTreeSet<String>, gmeow_errors::RecordedDiag>>,
    preset_facets: Result<BTreeMap<String, BTreeSet<String>>, gmeow_errors::RecordedDiag>,
}

pub(super) fn record(
    dataset: &RdfDataset,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&observe(dataset))
            .map_err(|error| super::stage_err(format!("encode vocabulary observation: {error}")))?,
    );
    Ok(())
}

fn observe(dataset: &RdfDataset) -> Observation {
    let members: BTreeMap<_, _> = CLASSES
        .into_iter()
        .map(|class| {
            (
                class.to_owned(),
                super::observed(members_of(dataset, class)),
            )
        })
        .collect();
    let preset_facets = members["ReasoningPreset"]
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|presets| {
            super::observed(
                presets
                    .iter()
                    .map(|preset| {
                        let subject = dataset
                            .term_id_by_iri(preset)
                            .expect("observed native subject");
                        let mut facets = BTreeSet::new();
                        for field in ["expandsToFacet", "resourcePolicy"] {
                            if let Some(predicate) =
                                dataset.term_id_by_iri(&format!("{LOGIC_NAMESPACE}{field}"))
                            {
                                for quad in default_graph_pattern(
                                    dataset,
                                    Some(subject),
                                    Some(predicate),
                                    None,
                                ) {
                                    facets.insert(named_iri(dataset, quad.o)?.to_owned());
                                }
                            }
                        }
                        Ok((preset.clone(), facets))
                    })
                    .collect(),
            )
        });
    Observation {
        source: super::SOURCE_PATH.to_owned(),
        members,
        preset_facets,
    }
}

fn members_of(dataset: &RdfDataset, class: &str) -> gmeow_errors::Result<BTreeSet<String>> {
    let mut members = BTreeSet::new();
    let Some(class) = dataset.term_id_by_iri(&format!("{LOGIC_NAMESPACE}{class}")) else {
        return Ok(members);
    };
    for predicate in [
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
        "https://blackcatinformatics.ca/logic/instanceOf",
    ] {
        if let Some(predicate) = dataset.term_id_by_iri(predicate) {
            for quad in default_graph_pattern(dataset, None, Some(predicate), Some(class)) {
                members.insert(named_iri(dataset, quad.s)?.to_owned());
            }
        }
    }
    Ok(members)
}

fn named_iri(dataset: &RdfDataset, term: TermId) -> gmeow_errors::Result<&str> {
    match dataset.resolve(term) {
        TermRef::Iri(iri) => Ok(iri),
        other => Err(super::stage_err(format!(
            "vocabulary member or facet requires a named IRI, found {other:?}"
        ))),
    }
}

#[cfg(test)]
mod corpus_tests;

#[path = "vocabulary_fixtures.tests.rs"]
#[cfg(test)]
mod tests;
