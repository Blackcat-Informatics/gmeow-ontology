// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Producer-owned reasoning contract for the shipped advisory claim graph.
//!
//! Validation closes its exact claim graph with the original standalone norms
//! document while both native datasets are resident. The terminal fixture producer
//! independently observes the shipped graph. Consumers require equal graph
//! commitments and grade the recorded closure; they never construct a corpus.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_errors::{Diag, DiagLedger, RecordedDiag, StageId};
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfTerm};
use serde::{Deserialize, Serialize};

use crate::stages::parse_sources::SourceCatalog;

/// Compact source-stage observation exported with the exact validation receipt.
pub const CHANNEL: &str = "pipeline/norm-claims-reasoning.json";
/// Independent census and commitment of the actual selected terminal graph.
pub const ARTIFACT: &str = "shipped-norm-claims-observation-v1.json";
/// Standalone source scope; aggregate modules must not supply missing norms.
pub const NORMS_SOURCE: &str = "slices/core/norms/module.ttl";
/// The only claim graph selected by this contract.
pub const GRAPH: &str = "https://blackcatinformatics.ca/gmeow/graph/norm-claims";

/// Native graph commitment and advisory census, without a serialized graph copy.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShippedObservation {
    /// Exact named graph selected from the producer's dataset.
    pub graph: String,
    /// Canonical RDF 1.2 graph digest, including statement metadata.
    pub graph_digest: String,
    /// Number of ordinary asserted quads in the selected graph.
    pub asserted_quads: usize,
    /// Actual ComplianceAssessment subjects carrying an `advice.` family code.
    pub advisory_assessments: BTreeSet<String>,
}

/// Successful closure evidence tied to the original standalone source identity.
#[derive(Debug, Serialize, Deserialize)]
pub struct ClosureObservation {
    /// Original source path within the admitted parse catalog.
    pub source_path: String,
    /// Catalog commitment of that original document, before aggregate lowering.
    pub source_digest: String,
    /// Nonempty source scope, independently visible beside the claim census.
    pub source_quads: usize,
    /// Total asserted and derived rows returned by the native RL closure surface.
    pub closure_triples: usize,
}

/// The producer records failures as failures; the read-only consumer grades them.
#[derive(Debug, Serialize, Deserialize)]
pub struct Observation {
    /// Exact graph passed to the closure, compared with the terminal observation.
    pub claims: ShippedObservation,
    /// A failed source lookup or closure cannot become a successful empty result.
    pub closure: Result<ClosureObservation, RecordedDiag>,
}

/// Inspect the selected claim graph without evaluating any ontology or query.
///
/// # Errors
/// Returns the canonicalization refusal if the exact graph cannot be committed.
pub fn observe_shipped(dataset: &RdfDataset) -> gmeow_errors::Result<ShippedObservation> {
    let graph = dataset.project_named_graph(GRAPH);
    let advisory_assessments = graph
        .owned_quads()
        .filter_map(
            |quad| match (quad.subject, quad.predicate.as_str(), quad.object) {
                (
                    RdfTerm::Iri(subject),
                    "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                    RdfTerm::Iri(class),
                ) if class == "https://blackcatinformatics.ca/gmeow/ComplianceAssessment"
                    && subject.contains("advice.") =>
                {
                    Some(subject)
                }
                _ => None,
            },
        )
        .collect();
    Ok(ShippedObservation {
        graph: GRAPH.to_owned(),
        graph_digest: purrdf::try_graph_digest_view(dataset, GRAPH)
            .map_err(Diag::from)?
            .to_hex(),
        asserted_quads: graph.quad_count(),
        advisory_assessments,
    })
}

pub(super) fn record(
    catalog: &SourceCatalog,
    claims: &RdfDataset,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let observed = Observation {
        claims: observe_shipped(claims)?,
        closure: (|| {
            let norms = catalog.document(NORMS_SOURCE)?;
            let source_digest = catalog.document_digest(NORMS_SOURCE)?.to_owned();
            close(norms, &source_digest, claims)
        })()
        .map_err(|error| DiagLedger::new().record(error, StageId::new("validate.norm-claims"))),
    };
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&observed).map_err(Diag::from)?,
    );
    Ok(())
}

fn close(
    norms: &RdfDataset,
    source_digest: &str,
    claims: &RdfDataset,
) -> gmeow_errors::Result<ClosureObservation> {
    // This contract explicitly joins the one selected claim graph to the one
    // standalone TBox in the default world. Preserve native statement metadata;
    // do not flatten through an RDF text projection or pull in unrelated graphs.
    let claims = claims.project_named_graph(GRAPH);
    let mut builder = RdfDatasetBuilder::new();
    builder.push_dataset(norms);
    for mut quad in claims.owned_quads() {
        quad.graph_name = None;
        builder.push_owned_quad(&quad);
    }
    for mut reifier in claims.owned_reifiers() {
        reifier.graph = None;
        builder.push_owned_reifier(&reifier);
    }
    for mut annotation in claims.owned_annotations() {
        annotation.graph = None;
        builder.push_owned_annotation(&annotation);
    }
    let input = builder.freeze().map_err(Diag::from)?;
    let closure = gmeow_logic::reason::rl_closure(&input)?;
    Ok(ClosureObservation {
        source_path: NORMS_SOURCE.to_owned(),
        source_digest: source_digest.to_owned(),
        source_quads: norms.quad_count(),
        closure_triples: closure.triples.len(),
    })
}

#[path = "norm_claims.tests.rs"]
#[cfg(test)]
mod tests;
