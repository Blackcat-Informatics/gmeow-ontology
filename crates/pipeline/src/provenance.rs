// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-stage provenance stamping (#861 P2).
//!
//! Each stage stamps the quads it produces into the kernel `DatasetProvenance`
//! sidecar with the correct `OriginKind` (`Source` / `Generated` / `Import` /
//! `RootOntology`) and a `UnitId`, so the composed bundle records which stage
//! authored every quad. Maps a [`StageKind`] to its canonical `OriginKind`.
//!
//! P2 registers one provenance unit per stage with the kind-derived origin; the
//! per-quad occurrence recording is wired in P3, when stages emit real quads.

use gmeow_rdf::provenance::{DatasetProvenance, OriginKind, UnitId};

use crate::node::StageKind;

/// The canonical `OriginKind` a stage of this kind stamps onto its quads.
pub fn origin_kind(kind: StageKind) -> OriginKind {
    match kind {
        // Source load parses authored modules + transitive imports; the root
        // ontology and imports are distinguished at parse time, so the default
        // origin for the load stage's own emitted quads is `Source`.
        StageKind::SourceLoad => OriginKind::Source,
        // Everything downstream emits derived quads.
        StageKind::Transform
        | StageKind::Reason
        | StageKind::Validate
        | StageKind::DocsRender
        | StageKind::ExportLeaf
        | StageKind::Sink => OriginKind::Generated,
    }
}

/// Register one provenance unit for a stage, named by its id and carrying the
/// kind-derived [`OriginKind`]. Returns the interned [`UnitId`] the stage stamps
/// onto every quad it emits (idempotent: re-registering the same id is a no-op).
pub fn register_stage_unit(
    prov: &mut DatasetProvenance,
    stage_id: &str,
    kind: StageKind,
) -> UnitId {
    prov.register_unit(stage_id, origin_kind(kind))
}
