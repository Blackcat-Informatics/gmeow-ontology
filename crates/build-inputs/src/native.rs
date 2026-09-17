// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use crate::{Result, digest, fail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

const NATIVE_ROOTS: &[&str] = &[
    "crates/logic/src/lib.rs",
    "crates/logic-compile/src/lib.rs",
    "crates/term-arena/src/lib.rs",
    "crates/ns/src/lib.rs",
];

/// Resolve the native kernel's production module paths for an offline extraction
/// plan. This does not admit any source identity or produce a semantic digest.
pub fn native_extraction_paths(workspace: &Path) -> Result<Vec<String>> {
    crate::modules::extraction_paths(workspace, NATIVE_ROOTS)
}

/// Portable semantic source contract, deliberately independent of a Cargo profile,
/// target or selected feature set. Every non-test branch of the canonical kernel's
/// module declarations belongs to this contract, on native and WebAssembly alike.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeSources {
    /// Complete selected file bytes, represented by their SHA-256 digests.
    pub files: BTreeMap<String, String>,
    /// External modules excluded by their actual test/documentation cfg ownership.
    /// Their contents never enter the semantic digest.
    pub excluded: BTreeMap<String, String>,
}
impl NativeSources {
    /// Follow real module/include declarations; no recursive filename census or
    /// manually mirrored per-file source list participates in the contract.
    pub fn collect(workspace: &Path) -> Result<Self> {
        crate::modules::semantic_sources(workspace, NATIVE_ROOTS)
    }

    /// The same kernel bytes produce this identity under every host/build profile.
    pub fn digest(&self) -> Result<String> {
        Ok(digest(
            &serde_json::to_vec(&("gmeow-native-semantic-sources-v1", &self.files))
                .map_err(fail)?,
        ))
    }

    /// Emit only selected files and module-owning files, not test subtree watchers.
    pub fn emit_cargo_rerun_directives(&self, workspace: &Path) {
        for path in self.files.keys() {
            println!("cargo:rerun-if-changed={}", workspace.join(path).display());
        }
    }
}
