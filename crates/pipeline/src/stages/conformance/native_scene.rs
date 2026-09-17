// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native scene evaluations retained by the explicit conformance producer.
//! Selected grounding scenes reuse its existing source parse. The auxiliary
//! expression fixture has a separate exact-input action and never joins the
//! source catalog, an example world, or the production ontology implicitly.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gmeow_action_cache::{ActionStore, STORE_FORMAT_VERSION, StoreLimits, bytes_digest};
use gmeow_errors::Finding;
use gmeow_logic::result::ReasoningResult;
use gmeow_logic::result_rdf::GRAPH_REASONING;
use purrdf::{RdfDataset, RdfTerm, parse_dataset};
use serde::{Deserialize, Serialize};

use crate::bundle::PipelineHandle;

pub(crate) const CHANNEL: &str = "pipeline/native-scene-observations.json";
pub(super) const MODAL: &str = "slices/grounding/logic/examples/gmn-logic-roundtrip.ttl";
pub(super) const DIMENSION: &str = "slices/grounding/math/examples/gmn-dimension-roundtrip.ttl";
pub(super) const MATH_MODULE: &str = "slices/grounding/math/module.ttl";
pub(super) const ALPHA_DRIFT: &str =
    "slices/grounding/math/tests/conformance-fixtures/alpha-equivalence-drift-join.ttl";

#[derive(Serialize, Deserialize)]
pub(super) enum Scene {
    Modal(Box<ModalProduct>),
    Dimension(DimensionFindings),
}

/// The actual pinned stage product's native payload and identity, not a boolean
/// summary of whether the current implementation happened to satisfy a test.
/// Its inferred rows retain the frame, explicit evaluation world, antecedents,
/// rule identities and complete result provenance for independent assertions.
#[derive(Serialize, Deserialize)]
pub(super) struct ModalProduct {
    pub stage_id: String,
    pub product_digest: String,
    pub graph_iri: String,
    pub graph_digest: String,
    pub pinned_graph_digest: String,
    pub payload_digest: String,
    pub result: ReasoningResult,
}

#[derive(Serialize, Deserialize)]
pub(super) struct DimensionFindings {
    pub force_dimension_types: BTreeSet<String>,
    pub findings: Vec<Finding>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct AuxiliaryObservation {
    pub source_path: String,
    pub source_digest: String,
    pub findings: Vec<Finding>,
}

pub(super) fn input_files(root: &Path) -> Vec<PathBuf> {
    [MODAL, DIMENSION, MATH_MODULE, ALPHA_DRIFT]
        .into_iter()
        .map(|path| root.join(path))
        .collect()
}

/// Declared scene applicability, called inside the existing grounding-source
/// observation's cache miss. A selected scene always retains its success or
/// failure; consumers require this field and never invoke its producer.
pub(super) fn observe(
    path: &str,
    dataset: &RdfDataset,
) -> Option<Result<Scene, gmeow_errors::RecordedDiag>> {
    match path {
        MODAL => Some(
            modal_product(dataset)
                .map(|product| Scene::Modal(Box::new(product)))
                .map_err(super::record_failure),
        ),
        DIMENSION | MATH_MODULE => Some(Ok(Scene::Dimension(dimension_findings(dataset)))),
        _ => None,
    }
}

fn modal_product(dataset: &RdfDataset) -> Result<ModalProduct, gmeow_errors::Diag> {
    let rooted = crate::stages::carrier::rooted_in_graph(
        dataset,
        gmeow_logic::reasoning_graphs::GRAPH_EXAMPLES,
    )?;
    // This selected producer scene admits exactly the examples role. The source
    // action and native input contract retain its bytes separately from this stable
    // theory/profile authority; other carrier graph roles acquire no domain law.
    use gmeow_logic::reason::{DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld};
    const AUTHORITY: &str = "gmeow.pipeline.native-scene.modal.v1";
    let selection = serde_json::to_vec(&(
        AUTHORITY,
        DomainProfile::NonemptyObjectDomainV1,
        gmeow_logic::reasoning_graphs::GRAPH_EXAMPLES,
    ))
    .map_err(|error| super::stage_err(&format!("native scene domain identity: {error}")))?;
    let domains = SelectedDomains::new([SelectedLogicalWorld::new(
        LogicalGraph::Named(purrdf::TermValue::iri(
            gmeow_logic::reasoning_graphs::GRAPH_EXAMPLES,
        )),
        DomainProfile::NonemptyObjectDomainV1,
        AUTHORITY.to_owned(),
        *blake3::hash(&selection).as_bytes(),
    )?])?;
    let product = crate::stages::reason::reason_product_over_dataset(&rooted, &domains)?;
    let handle = product
        .bundle()
        .handle(GRAPH_REASONING)
        .ok_or_else(|| super::stage_err("stage-reason did not pin its required native result"))?;
    let PipelineHandle::Reasoning(result) = &handle.payload else {
        return Err(super::stage_err(
            "graph/reasoning must carry a Reasoning handle",
        ));
    };
    Ok(ModalProduct {
        stage_id: product.stage_id.clone(),
        product_digest: product.digest.clone(),
        graph_iri: GRAPH_REASONING.to_owned(),
        graph_digest: product.bundle().graph_digest(GRAPH_REASONING).to_hex(),
        pinned_graph_digest: handle.content_digest.to_hex(),
        payload_digest: crate::handle_identity::handle_payload_digest(&handle.payload),
        result: result.as_ref().clone(),
    })
}

fn dimension_findings(dataset: &RdfDataset) -> DimensionFindings {
    let force_dimension_types = dataset
        .owned_quads()
        .filter(|quad| {
            matches!(quad.subject, RdfTerm::Iri(ref iri)
                if iri == "https://blackcatinformatics.ca/math/forceDimension")
                && matches!(
                    quad.predicate.as_str(),
                    "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
                        | "https://blackcatinformatics.ca/logic/instanceOf"
                )
        })
        .filter_map(|quad| match quad.object {
            RdfTerm::Iri(class) => Some(class),
            _ => None,
        })
        .collect();
    DimensionFindings {
        force_dimension_types,
        findings: gmeow_logic::math_dimension::check_math_dimension_findings(dataset),
    }
}

/// The alpha-drift fixture is outside the authored module catalog and the
/// grounding examples. Its one native parse and one shared expression lowering
/// run only in this producer action; the complete findings retain source joins.
pub(super) fn record(
    root: &Path,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let store = ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(|error| super::stage_err(&error.to_string()))?;
    let observed = super::execution::cached_observation(
        root,
        &root.join(ALPHA_DRIFT),
        &store,
        "native-scene-alpha-drift-v1",
        |bytes| {
            let dataset =
                parse_dataset(bytes, "text/turtle", None).map_err(gmeow_errors::Diag::from)?;
            Ok(AuxiliaryObservation {
                source_path: ALPHA_DRIFT.to_owned(),
                source_digest: bytes_digest(bytes),
                // This fixture asserts no inferred surface edges: preserve the
                // original gate's exact asserted/inspection substrate selection.
                findings: gmeow_logic::math_expression::check_math_expression_findings(
                    &dataset, &dataset,
                ),
            })
        },
    )
    .map_err(super::record_failure);
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&observed).map_err(|error| super::stage_err(&error.to_string()))?,
    );
    Ok(())
}
