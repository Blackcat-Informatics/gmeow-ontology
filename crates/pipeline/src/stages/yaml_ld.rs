// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `yaml_ld` export leaf (#699): RDF → YAML-LD-star / JSON-LD-star.
//!
//! This is a scaffold stage for Task 1 of issue #699. It consumes THIS run's
//! snapshot fold and currently emits no artifacts; the full YAML-LD-star /
//! JSON-LD-star serializer will land in a follow-up task.

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};

/// The `yaml_ld` export-leaf stage.
pub struct YamlLdStage {
    consumes: Vec<String>,
}

impl YamlLdStage {
    /// Construct the stage; it consumes THIS run's snapshot fold.
    pub fn new() -> Self {
        Self {
            consumes: vec!["stage-snapshot".to_string()],
        }
    }
}

impl Default for YamlLdStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for YamlLdStage {
    fn id(&self) -> &str {
        "stage-export-yaml-ld"
    }
    fn kind(&self) -> StageKind {
        StageKind::ExportLeaf
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        "yaml_ld.v0"
    }
    fn run(&self, _input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        // Task 1 scaffold: emit an empty product. The full YAML-LD-star /
        // JSON-LD-star serialization will be added in a later task.
        Ok(StageOutput {
            product: StageProduct::new(self.id(), "yaml_ld.scaffold.v0"),
        })
    }
}
