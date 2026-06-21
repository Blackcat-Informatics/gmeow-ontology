// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-slice` — native slice catalog: manifest-based discovery, typed
//! artifact inventory, and content-addressed IDs for the GMEOW ontology slices.

pub mod artifact;
pub mod catalog;
pub mod error;
pub mod ownership;

pub use artifact::{ArtifactRecord, ArtifactRole};
pub use catalog::{ManifestView, SliceCatalog, SliceRecord, SliceTier};
pub use error::SliceError;
pub use ownership::{
    ArtifactEvidence, DependencyEdge, EdgeEvidence, EdgeKind, OwnershipAnalyzer,
    OwnershipDiagnostic, OwnershipReport, OwnershipStatus, ReconciliationStatus, SliceIri,
    TermOwnership,
};
