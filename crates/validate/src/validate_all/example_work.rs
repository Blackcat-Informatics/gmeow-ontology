// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Observations of example validation, never cached verdicts or semantic inputs.

use purrdf::ir::ViewStats;
use purrdf::shapes::data_view::ShaclViewStats;
use serde::{Deserialize, Serialize};

/// Work performed by this invocation of the parallel example-validation phase.
///
/// Per-example durations overlap and must not be summed as phase wall time.
/// Retention charges are native view accounting, not allocator or peak RSS data.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExampleValidationMeasurements {
    /// Elapsed microseconds projecting the shared base, once per cache-miss wave.
    pub base_projection_us: u128,
    /// Elapsed microseconds preparing immutable shapes, once per cache-miss wave.
    pub shape_preparation_us: u128,
    /// Samples in source-path order, independent of worker completion order.
    pub examples: Vec<ExampleValidationSample>,
}

/// One example's current work; a cache hit has no execution sample.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExampleValidationSample {
    /// Slice-relative example path, matching finding attribution.
    pub example: String,
    /// Present only when validation was attempted in this invocation.
    /// A syntax error records parsing work but has no successfully bound view.
    pub execution: Option<ExampleExecution>,
}

/// Observed example work, excluding cache I/O and the shared preparation above.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExampleExecution {
    /// Elapsed microseconds reading and parsing the authored example.
    pub parse_us: u128,
    /// Elapsed microseconds deriving GMEOW's selected canonical SHACL view.
    pub projection_us: u128,
    /// Elapsed microseconds constructing views and binding data-dependent analyses.
    pub binding_us: u128,
    /// Elapsed microseconds evaluating all required SHACL targets.
    pub validation_us: u128,
    /// Native measurements after validation; absent if parsing failed.
    pub views: Option<ExampleViews>,
}

/// Native accounting for one complete example binding, after evaluation.
///
/// Copy counters cover the composite view, not parsing or canonical projection.
/// Retained payload includes the shared base for each binding; summing samples
/// double-counts that shared storage. Core and query adapters may share storage
/// too, so their separate charges are not additive process memory measurements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExampleViews {
    /// Immutable input payload retained by the composite, excluding indexes.
    pub retained_payload_bytes: usize,
    /// Conservative composite bookkeeping charge, including construction work.
    pub composite_auxiliary_bytes: usize,
    /// Terms published in new dictionaries by the composite view.
    pub copied_terms: usize,
    /// RDF rows replayed by the composite at a materialization boundary.
    pub copied_rows: usize,
    /// UTF-8 dictionary payload copied by the composite, excluding importer buffers.
    pub copied_text_bytes: usize,
    /// ID payload written into composite mappings and suppression sets.
    pub copied_index_bytes: usize,
    /// Successful composite dataset freezes.
    pub freezes: usize,
    /// Successful complete composite materializations.
    pub composite_materializations: usize,
    /// Core adapter bookkeeping charge; excludes source-owned storage.
    pub core_auxiliary_bytes: usize,
    /// Query adapter bookkeeping charge; excludes source-owned storage.
    pub query_auxiliary_bytes: usize,
    /// Compatibility materializations through the Core adapter.
    pub core_materializations: usize,
    /// Compatibility materializations through the query adapter.
    pub query_materializations: usize,
}

impl ExampleViews {
    pub(super) fn observed(composite: ViewStats, [core, query]: [ShaclViewStats; 2]) -> Self {
        Self {
            retained_payload_bytes: composite.retained_payload_bytes,
            composite_auxiliary_bytes: composite.auxiliary_bytes,
            copied_terms: composite.work.copied_terms,
            copied_rows: composite.work.copied_rows,
            copied_text_bytes: composite.work.copied_text_bytes,
            copied_index_bytes: composite.work.copied_index_bytes,
            freezes: composite.work.freezes,
            composite_materializations: composite.work.materializations,
            core_auxiliary_bytes: core.auxiliary_bytes,
            query_auxiliary_bytes: query.auxiliary_bytes,
            core_materializations: core.materializations,
            query_materializations: query.materializations,
        }
    }
}
