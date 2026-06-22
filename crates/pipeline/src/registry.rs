// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `STAGE_REGISTRY`: the binding from a `gmeow:stageImpl` key to its Rust
//! [`Stage`] implementation (#861).
//!
//! The codebase has no static trait-object registry, so this follows the manual
//! registration pattern used by `crates/native` for engine submodules: a
//! `StageRegistry` is constructed and stages are registered into it explicitly
//! by [`default_registry`]. The loader resolves each `gmeow:PipelineStage`
//! individual's `gmeow:stageImpl` against this map and HARD-fails on a key with
//! no implementation (no-optionality).
//!
//! P1 ships the registry mechanism with no production stages registered; the
//! source / transform / reason stages (P3), export leaves (P4), and docs render
//! (P5) register here as those parcels land.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::node::Stage;

/// A name → implementation map for pipeline stages. Insertion is by
/// `gmeow:stageImpl` key; lookups are deterministic (BTreeMap).
#[derive(Default)]
pub struct StageRegistry {
    stages: BTreeMap<String, Arc<dyn Stage>>,
}

impl StageRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self {
            stages: BTreeMap::new(),
        }
    }

    /// Register `stage` under `impl_key` (the `gmeow:stageImpl` value). A later
    /// registration under the same key replaces the earlier one.
    pub fn register(&mut self, impl_key: impl Into<String>, stage: Arc<dyn Stage>) {
        self.stages.insert(impl_key.into(), stage);
    }

    /// Resolve an `impl_key` to its stage, if registered.
    pub fn get(&self, impl_key: &str) -> Option<Arc<dyn Stage>> {
        self.stages.get(impl_key).cloned()
    }

    /// Whether `impl_key` is registered.
    pub fn contains(&self, impl_key: &str) -> bool {
        self.stages.contains_key(impl_key)
    }

    /// The registered `gmeow:stageImpl` keys, sorted.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.stages.keys().map(String::as_str)
    }

    /// Number of registered stages.
    pub fn len(&self) -> usize {
        self.stages.len()
    }

    /// Whether no stages are registered.
    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }
}

/// Build the default registry of production stages.
///
/// P1: empty — the executable stages register here in P3–P5. Kept as the single
/// construction entrypoint so the loader and the PyO3 `run_pipeline` (P6) share
/// one stage inventory.
pub fn default_registry() -> StageRegistry {
    StageRegistry::new()
}
