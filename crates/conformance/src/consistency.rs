// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native consistency evidence. Execution errors are not semantic gap verdicts.

use std::collections::BTreeMap;
use std::path::Path;

use gmeow_logic::reason::{
    DlVerdict, DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld,
};
use gmeow_logic::result::InformationState;
use purrdf::{NativeRdfFormat, RdfDataset, dataset_from_bytes};
use serde::{Deserialize, Serialize};

/// Complete native verdict, including world witnesses, coverage and identified
/// boundary findings. World counts retain the runner's exact per-world contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    /// Complete native evidence, including witnesses and any capability gaps.
    pub verdict: DlVerdict,
    /// Counts of asserted facts in each addressable named-IRI world.
    pub world_counts: BTreeMap<String, u64>,
}

impl Observation {
    /// A proved conflict remains `inconsistent` alongside any coverage gaps.
    /// `consistent` requires a complete supported claim; other admitted verdicts
    /// remain `incomplete`. Execution errors never construct an observation.
    #[must_use]
    pub fn token(&self) -> &'static str {
        match self.verdict.information_state() {
            InformationState::Both => "inconsistent",
            InformationState::Supported => "consistent",
            _ => "incomplete",
        }
    }
}

/// Observe the submitted conformance dataset as explicitly selected logical worlds.
/// One prepared native ingress retains the original assertions and context census;
/// the same execution supplies both the verdict and the original named-world counts.
/// The census includes only occupied named-IRI source graphs, never inferred rows,
/// declaration-only graphs, the default graph or blank-node graph names.
///
/// # Errors
/// Rejects source/domain admission, execution or native evidence validation failures.
pub fn observe(dataset: &RdfDataset) -> gmeow_errors::Result<Observation> {
    let input = gmeow_logic::reason::prepare_reasoning_input(dataset)?;
    // This operation explicitly interprets every submitted source context as a
    // logical theory. The caller-owned profile selects a nonempty object domain;
    // a physical graph name alone does not supply a domain law elsewhere.
    let domains = SelectedDomains::new(
        input
            .source_contexts()
            .values()
            .map(|graph| {
                SelectedLogicalWorld::new(
                    LogicalGraph::from_graph(graph.clone()),
                    DomainProfile::NonemptyObjectDomainV1,
                    "gmeow-conformance.consistency.v1".to_owned(),
                    *input.ingress_contract(),
                )
            })
            .collect::<gmeow_errors::Result<Vec<_>>>()?,
    )?;
    let result = gmeow_logic::reason::reason_all(input, &domains)?;
    let verdict = result.native_verdict()?;
    let world_counts = result
        .native_execution()?
        .class_admission
        .source_worlds
        .iter()
        .filter(|(_, source)| {
            source.assertions > 0 && matches!(&source.graph, Some(purrdf::TermValue::Iri(_)))
        })
        .map(|(world, source)| (world.clone(), source.assertions))
        .collect();
    Ok(Observation {
        verdict,
        world_counts,
    })
}

/// Explicit producer ingress. Tests must never call this on authored corpus.
pub fn read(input_nq: &Path) -> gmeow_errors::Result<Observation> {
    let bytes = std::fs::read(input_nq)
        .map_err(|error| fail(format!("read {}: {error}", input_nq.display())))?;
    let dataset = dataset_from_bytes(&bytes, NativeRdfFormat::NQuads)
        .map_err(|error| fail(format!("parse {}: {error}", input_nq.display())))?;
    observe(&dataset)
}

fn fail(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::RunFailed { detail })
}

#[path = "consistency.tests.rs"]
#[cfg(test)]
mod tests;
