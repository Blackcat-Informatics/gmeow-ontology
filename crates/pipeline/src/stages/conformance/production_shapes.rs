// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! One archived production shape profile shared by the fixture producer's tiny
//! flagship and advisory controls. Its lifetime ends with the selected bundle's
//! artifact admission; no global shape cache or second bundle import is needed.

use std::sync::Arc;

use gmeow_validate::findings::FailureClassIndex;
use purrdf::RdfDataset;
use purrdf::shapes::engine::PreparedShapes;
use purrdf::shapes::shapes::from_dataset_with_prefixes;
use purrdf::shapes::text_ingest::extract_prefixes;

/// Both original controls select these exact archived bytes and document prefixes.
pub const PROFILE: &str = "archived-production-shapes:declared-prefixes-v1";

/// Immutable parsed shape source, shared analysis and exact property-shape owner
/// index. Data-dependent target binding remains independent for each control.
pub struct PreparedProductionShapes {
    pub(super) dataset: Arc<RdfDataset>,
    pub(super) shapes: PreparedShapes,
    pub(super) classes: FailureClassIndex,
    pub(super) digest: String,
}

impl PreparedProductionShapes {
    /// Parse and prepare the source profile once at the explicit producer boundary.
    pub fn new(production_shapes: &str) -> gmeow_errors::Result<Self> {
        let dataset = purrdf::parse_dataset(production_shapes.as_bytes(), "text/turtle", None)
            .map_err(fail)?;
        let shapes = from_dataset_with_prefixes(&dataset, &extract_prefixes(production_shapes))
            .map_err(fail)?;
        let classes = FailureClassIndex::from_shapes_dataset(&dataset);
        Ok(Self {
            dataset,
            shapes: PreparedShapes::new(Arc::new(shapes)),
            classes,
            digest: gmeow_action_cache::bytes_digest(production_shapes.as_bytes()),
        })
    }
}

/// Preserve shape admission failures on the conformance diagnostic surface.
fn fail(error: impl std::fmt::Display) -> gmeow_errors::Diag {
    super::stage_err(&error.to_string())
}
