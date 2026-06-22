// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `evals` export leaf (#861 P4): scorecard + leaderboard + scores.ttl.
//!
//! STUB — body filled in by the #861 P4 leaf port. The struct name, `id()`,
//! and registry key are stable wiring contracts; only `run()` (and helpers)
//! are filled in.

use std::collections::BTreeMap;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};

/// The `stage-export-evals` export-leaf stage.
pub struct EvalsStage;

impl Stage for EvalsStage {
    fn id(&self) -> &str {
        "stage-export-evals"
    }
    fn kind(&self) -> StageKind {
        StageKind::ExportLeaf
    }
    fn consumes(&self) -> &[String] {
        &[]
    }
    fn impl_version(&self) -> &str {
        "evals.v1"
    }
    fn run(&self, _input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        // STUB: filled by the leaf port.
        let artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        Ok(StageOutput {
            product: StageProduct::from_artifacts(self.id(), artifacts),
        })
    }
}
