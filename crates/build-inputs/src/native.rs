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

/// The immutable upstream source behind native datatype and entailment semantics.
/// Reading the declared pin is portable and performs no compiler or host discovery.
pub fn native_substrate_source(workspace: &Path) -> Result<String> {
    let manifest = std::fs::read_to_string(workspace.join("Cargo.toml")).map_err(fail)?;
    let manifest: toml::Value = toml::from_str(&manifest).map_err(fail)?;
    let dependency = manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(|dependencies| dependencies.get("purrdf"))
        .ok_or_else(|| fail("native substrate requires workspace.dependencies.purrdf"))?;
    let git = dependency.get("git").and_then(toml::Value::as_str);
    let revision = dependency.get("rev").and_then(toml::Value::as_str);
    match (git, revision) {
        (Some(git), Some(revision))
            if !git.is_empty()
                && revision.len() == 40
                && revision.bytes().all(|byte| byte.is_ascii_hexdigit()) =>
        {
            Ok(format!("{git}#{}", revision.to_ascii_lowercase()))
        }
        _ => Err(fail(
            "native substrate requires a Git URL and full immutable revision",
        )),
    }
}

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
