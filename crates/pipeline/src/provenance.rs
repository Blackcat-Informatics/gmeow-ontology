// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-stage provenance stamping (#861 P2).
//!
//! Each stage stamps the quads it produces into the kernel `DatasetProvenance`
//! sidecar with the correct `OriginKind` (`Source` / `Generated` / `Import` /
//! `RootOntology`) and a `UnitId`, so the composed bundle records which stage
//! authored every quad. Derives the canonical `OriginKind` from a stage's declared
//! capabilities ([`crate::node::SOURCE_ORIGIN`] → `Source`, else `Generated`).
//!
//! P2 registers one provenance unit per stage with the capability-derived origin; the
//! per-quad occurrence recording is wired in P3, when stages emit real quads.

use gmeow_rdf::provenance::{DatasetProvenance, OriginKind, UnitId};

use crate::node::SOURCE_ORIGIN;

/// The canonical `OriginKind` a stage holding these `capabilities` stamps onto its
/// quads: a stage holding [`crate::node::SOURCE_ORIGIN`] (the authored-source loader,
/// whose root ontology and imports are distinguished at parse time) emits `Source`;
/// every other stage emits derived `Generated` quads.
pub fn origin_kind(capabilities: &[String]) -> OriginKind {
    if capabilities.iter().any(|c| c == SOURCE_ORIGIN) {
        OriginKind::Source
    } else {
        OriginKind::Generated
    }
}

/// Register one provenance unit for a stage, named by its id and carrying the
/// capability-derived [`OriginKind`]. Returns the interned [`UnitId`] the stage stamps
/// onto every quad it emits (idempotent: re-registering the same id is a no-op).
pub fn register_stage_unit(
    prov: &mut DatasetProvenance,
    stage_id: &str,
    capabilities: &[String],
) -> UnitId {
    prov.register_unit(stage_id, origin_kind(capabilities))
}
