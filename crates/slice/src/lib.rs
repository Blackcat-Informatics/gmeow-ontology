// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-slice` — native slice catalog: manifest-based discovery, typed
//! artifact inventory, and content-addressed IDs for the GMEOW ontology slices.

pub mod analysis;
pub mod artifact;
pub mod cache;
pub mod catalog;
pub mod dsl_stats_emit;
pub mod edoal_emit;
pub mod error;
pub mod fix_deps;
pub mod fno_emit;
pub mod mapping_emit;
pub mod ownership;
pub mod sparql_emit;
pub mod standpoint_emit;

#[cfg(feature = "python")]
pub mod py;

#[cfg(feature = "python")]
pub use py::register;

pub use analysis::{
    bundle_content_id, emit_analysis_graph, is_forbidden_edge, AnalysisError, AnalysisGraph,
    ANALYSIS_GRAPH_IRI, COMPUTED_PROFILE_MEMBERSHIP, COMPUTED_SLICE_DEPENDENCY,
    DEPENDENCY_EVIDENCE, DEPENDENCY_STATUS, TERM_COVERAGE,
};
pub use artifact::{ArtifactRecord, ArtifactRole};
pub use cache::{
    dependency_closure, link_unit_key, link_units, product_unit, product_unit_key, source_unit_key,
    CacheKey, LinkUnit, Phase, ProductUnit, ToolchainContext,
};
pub use catalog::{ManifestView, SliceCatalog, SliceRecord, SliceTier};
pub use dsl_stats_emit::emit_dsl_stats;
pub use edoal_emit::emit_edoal_sets;
pub use error::SliceError;
pub use fix_deps::{compute_fix_deps, ManifestPatch};
pub use mapping_emit::emit_sssom_sets;
pub use ownership::{
    ArtifactEvidence, DependencyEdge, EdgeEvidence, EdgeKind, OwnershipAnalyzer,
    OwnershipDiagnostic, OwnershipReport, OwnershipStatus, ReconciliationStatus, SliceIri,
    TermOwnership,
};
pub use sparql_emit::emit_sparql_sets;
pub use standpoint_emit::emit_standpoint_sets;
