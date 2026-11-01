// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete report ownership across producer release and snapshot consumption.

mod cbor;

use std::collections::BTreeMap;
use std::sync::Arc;

use gmeow_errors::{Diag, Report};

use super::PipelineHandle;
use crate::node::StageProduct;
use crate::stages::carrier::GRAPH_DIAGNOSTICS;

/// The closed report inventory consumed by documentation. There are at most two
/// owners; neither a tool label nor an arbitrary stage can create another owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagnosticReportOwner {
    /// Compiler findings, including the complete source-attributed loss report.
    CompileLogic,
    /// Validation findings, including advice, meta enrichment and the record seal.
    Validate,
}

impl DiagnosticReportOwner {
    /// Bind a closed producer identity to its established Report tool name.
    fn tool(self) -> &'static str {
        match self {
            Self::CompileLogic => "logic-compile",
            Self::Validate => "shacl",
        }
    }
}

/// Immutable, final rich reports selected from the diagnostic graph's producers.
/// Gate/meta RDF conclusions can extend the backing graph; this publication does
/// not claim that its reports reconstruct every row of the snapshot's union.
#[derive(Debug)]
pub struct DiagnosticsPublication {
    pub(crate) reports: BTreeMap<DiagnosticReportOwner, Arc<Report>>,
}

impl DiagnosticsPublication {
    /// Freeze the renderer's final report under its one actual producer owner.
    pub(crate) fn producer(
        owner: DiagnosticReportOwner,
        report: Arc<Report>,
    ) -> Result<Self, Diag> {
        let publication = Self {
            reports: BTreeMap::from([(owner, report)]),
        };
        publication.validate()?;
        Ok(publication)
    }

    /// Borrow a mandatory report without cloning findings or decoding artifacts.
    pub fn report(&self, owner: DiagnosticReportOwner) -> Result<&Arc<Report>, Diag> {
        self.reports
            .get(&owner)
            .ok_or_else(|| error(format!("missing native diagnostic report for {owner:?}")))
    }

    /// Check intrinsic report ownership at creation and native cache hydration.
    pub(crate) fn validate(&self) -> Result<(), Diag> {
        if self.reports.is_empty() {
            return Err(error(
                "native diagnostic publication has no report owner".into(),
            ));
        }
        for (owner, report) in &self.reports {
            if report.tool != owner.tool() {
                return Err(error(format!(
                    "native diagnostic owner {owner:?} requires tool {}, got {}",
                    owner.tool(),
                    report.tool
                )));
            }
        }
        Ok(())
    }

    /// A producer carries exactly its own report; the retained snapshot carries
    /// both. No unknown owner or missing half selects a weaker publication.
    pub(crate) fn validate_binding(&self, stage: &str, graph: &str) -> Result<(), Diag> {
        self.validate()?;
        let expected: &[DiagnosticReportOwner] = match stage {
            "stage-compile-logic" => &[DiagnosticReportOwner::CompileLogic],
            "stage-validate" => &[DiagnosticReportOwner::Validate],
            "stage-snapshot" => &[
                DiagnosticReportOwner::CompileLogic,
                DiagnosticReportOwner::Validate,
            ],
            _ => {
                return Err(error(format!(
                    "stage {stage} cannot own this native diagnostic publication"
                )));
            }
        };
        if graph != GRAPH_DIAGNOSTICS || !self.reports.keys().copied().eq(expected.iter().copied())
        {
            return Err(error(format!(
                "native diagnostic publication for {stage} requires graph/diagnostics and owners {expected:?}"
            )));
        }
        Ok(())
    }
}

/// Pin the exact report publication to the already produced diagnostic graph.
pub(crate) fn pin_diagnostics(
    bundle: &mut purrdf::PipelineBundle<PipelineHandle>,
    stage: &str,
    publication: Arc<DiagnosticsPublication>,
) -> Result<(), Diag> {
    publication.validate_binding(stage, GRAPH_DIAGNOSTICS)?;
    if !crate::handle_identity::contains_graph(bundle, GRAPH_DIAGNOSTICS) {
        return Err(error(format!(
            "{stage} has no declared diagnostic graph for its native report"
        )));
    }
    let pin = bundle.graph_digest(
        GRAPH_DIAGNOSTICS,
        &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None),
    )?;
    bundle
        .pin_handle(
            GRAPH_DIAGNOSTICS,
            PipelineHandle::Diagnostics(publication),
            pin,
            &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None),
        )
        .map_err(|cause| error(format!("pin native diagnostic publication: {cause}")))
}

/// Authenticate the explicitly selected native owner and borrow its rich report.
/// Only this handle is hashed; no unrelated program or whole RDF carrier is read.
pub(crate) fn diagnostics_from_product<'a>(
    product: &'a StageProduct,
    expected_stage: &str,
) -> Result<&'a DiagnosticsPublication, Diag> {
    if product.stage_id != expected_stage || product.carrier_released {
        return Err(error(format!(
            "native diagnostics require the retained {expected_stage} product"
        )));
    }
    let bundle = product.bundle();
    let entry = bundle
        .handle(GRAPH_DIAGNOSTICS)
        .ok_or_else(|| error("missing pinned native Diagnostics handle".into()))?;
    let PipelineHandle::Diagnostics(publication) = &entry.payload else {
        return Err(error(
            "graph/diagnostics requires the Diagnostics handle arm".into(),
        ));
    };
    publication.validate_binding(expected_stage, GRAPH_DIAGNOSTICS)?;
    if !crate::handle_identity::contains_graph(bundle, GRAPH_DIAGNOSTICS)
        || entry.content_digest
            != bundle.graph_digest(
                GRAPH_DIAGNOSTICS,
                &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None),
            )?
    {
        return Err(error(
            "native Diagnostics handle disagrees with its graph pin".into(),
        ));
    }
    let published = product
        .handle_commitments()
        .get(GRAPH_DIAGNOSTICS)
        .ok_or_else(|| error("missing published native Diagnostics commitment".into()))?;
    let current = crate::handle_identity::handle_commitment(
        GRAPH_DIAGNOSTICS,
        &entry.content_digest.to_hex(),
        &entry.payload,
    );
    if current.identity != published.identity || current.digest != published.digest {
        return Err(error(
            "native Diagnostics payload identity changed after publication".into(),
        ));
    }
    Ok(publication)
}

/// Retain only two shared reports after their original producers pass their last
/// consumer. Snapshot construction requires both exact native source owners.
pub(crate) fn snapshot_diagnostics(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<Arc<DiagnosticsPublication>, Diag> {
    let mut reports = BTreeMap::new();
    for (stage, owner) in [
        ("stage-compile-logic", DiagnosticReportOwner::CompileLogic),
        ("stage-validate", DiagnosticReportOwner::Validate),
    ] {
        let product = upstream
            .get(stage)
            .ok_or_else(|| error(format!("missing {stage} for snapshot diagnostics")))?;
        let publication = diagnostics_from_product(product, stage)?;
        reports.insert(owner, Arc::clone(publication.report(owner)?));
    }
    Ok(Arc::new(DiagnosticsPublication { reports }))
}

/// Attribute publication or admission refusal through the pipeline decode kind.
fn error(message: String) -> Diag {
    Diag::of_kind(crate::error::Decode::new(message))
}

#[cfg(test)]
pub(crate) mod tests;
