// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-stage provenance stamping (#861 P2).
//!
//! Each stage stamps the quads it produces into the kernel `DatasetProvenance`
//! sidecar with the correct `OriginKind` (`Source` / `Generated` / `Import` /
//! `RootOntology`) and a `UnitId`, so the composed bundle records which stage
//! authored every quad. Maps a [`StageKind`] to its canonical `OriginKind`.
//!
//! P1 ships the kind → origin mapping; the occurrence recording lands in P2
//! when stages produce real quads.

use gmeow_rdf::provenance::OriginKind;

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
