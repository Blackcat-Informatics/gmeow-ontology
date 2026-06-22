// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `schemas` export leaf (#861 P4): LinkML/JSON-Schema/Pydantic/TS/GraphQL/OpenAPI.
//!
//! STUB — body filled in by the #861 P4 leaf port. The struct name, `id()`,
//! and registry key are stable wiring contracts; only `run()` (and helpers)
//! are filled in.

use std::collections::BTreeMap;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};

/// The `stage-export-schemas` export-leaf stage.
pub struct SchemasStage;

impl Stage for SchemasStage {
    fn id(&self) -> &str {
        "stage-export-schemas"
    }
    fn kind(&self) -> StageKind {
        StageKind::ExportLeaf
    }
    fn consumes(&self) -> &[String] {
        &[]
    }
    fn impl_version(&self) -> &str {
        "schemas.v1"
    }
    fn run(&self, _input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        // STUB: filled by the leaf port.
        let artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        Ok(StageOutput {
            product: StageProduct::from_artifacts(self.id(), artifacts),
        })
    }
}
