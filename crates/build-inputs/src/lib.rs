// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! One production source inventory, shared by pre-build and read-only admission.
//! Complete selected files are hashed. Test implementation must live in external
//! inactive modules; no token removal or compiler-discovery fallback is permitted.

mod build_script;
mod cargo;
mod cfg;
mod dep_info;
mod embedded;
mod modules;
mod native;
mod resolution;

pub use build_script::emit_action_identity;
pub use cargo::{
    ProductionSelection, UnitSelection, cargo_policy_files, declared_path_dependency_manifests,
};
pub use cfg::CfgContext;
pub use dep_info::{CompilerArtifact, CompilerInputs, GeneratedInputs};
pub use embedded::VerifyQueries;
pub use native::{NativeSources, native_extraction_paths};
pub use resolution::{CargoResolutionEvidence, CargoResolutionInputs};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

/// Serialized selection supplied by the authenticated producer controller.
pub const SELECTION_ENV: &str = "GMEOW_PRODUCER_SOURCE_SELECTION";
/// Commitment to the controller-written selection document named by SELECTION_ENV.
pub const SELECTION_DIGEST_ENV: &str = "GMEOW_PRODUCER_SOURCE_SELECTION_SHA256";
/// Executable inventory format. No older inventory has equivalent admission.
pub const SCHEMA: u32 = 1;
/// Explicit identity for code that cannot produce corpus actions.
pub const NON_PRODUCER: &str = "non-producer:test-debug";

/// An input contract failure. An incomplete inventory is never a cache miss.
#[derive(Debug)]
pub struct InputError(pub String);
impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for InputError {}
pub type Result<T> = std::result::Result<T, InputError>;
pub(crate) fn fail(error: impl std::fmt::Display) -> InputError {
    InputError(error.to_string())
}
/// SHA-256 commitment used by input records and source-migration evidence.
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Why a complete physical file belongs to this selected compilation.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum InputRole {
    Rust,
    Embedded,
    BuildController,
    Policy,
    Dynamic,
}

/// An exact selected file and all its semantic owners.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileInput {
    pub sha256: String,
    pub owners: BTreeSet<String>,
    pub roles: BTreeSet<InputRole>,
}

/// Deterministic inputs to one selected operation. Cargo documents are parsed
/// structurally; Rust and embedded content retain their complete original bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputInventory {
    pub schema: u32,
    pub selection: ProductionSelection,
    pub files: BTreeMap<String, FileInput>,
    pub manifests: BTreeMap<String, String>,
    pub packages: BTreeMap<String, String>,
    pub memberships: BTreeMap<String, Vec<String>>,
    pub generated: BTreeMap<String, String>,
    pub environment_owners: BTreeMap<String, BTreeSet<String>>,
}
/// Resolve an actual producer selection to migration source paths only. Inline
/// tests remain extraction work; this result cannot authenticate any producer.
pub fn production_extraction_paths(
    workspace: &Path,
    selection: &ProductionSelection,
) -> Result<Vec<String>> {
    modules::production_extraction_paths(workspace, selection)
}

impl InputInventory {
    pub(crate) fn empty(selection: ProductionSelection) -> Self {
        Self {
            schema: SCHEMA,
            selection,
            files: BTreeMap::new(),
            manifests: BTreeMap::new(),
            packages: BTreeMap::new(),
            memberships: BTreeMap::new(),
            generated: BTreeMap::new(),
            environment_owners: BTreeMap::new(),
        }
    }
    /// Discover selected modules and explicit embedded owners before compilation.
    pub fn collect(workspace: &Path, selection: &ProductionSelection) -> Result<Self> {
        selection.validate()?;
        if cargo_policy_files(workspace)? != selection.policy_files {
            return Err(fail("selected Cargo policy-file membership is stale"));
        }
        let mut result = Self::empty(selection.clone());
        cargo::collect(workspace, &mut result)?;
        modules::collect(workspace, &mut result)?;
        for relative in &selection.policy_files {
            if matches!(relative.as_str(), ".cargo/config" | ".cargo/config.toml") {
                cargo::configuration(workspace, relative, &mut result)?;
            } else {
                result.add_file(workspace, relative, "cargo-policy", InputRole::Policy)?;
            }
        }
        Ok(result)
    }
    /// Domain-separated portable identity; absolute checkout paths never enter it.
    pub fn digest(&self) -> Result<String> {
        let mut hash = Sha256::new();
        hash.update(b"gmeow-production-input-inventory-v1\0");
        hash.update(serde_json::to_vec(self).map_err(fail)?);
        Ok(format!("{:x}", hash.finalize()))
    }
    /// Re-read the exact selected source contracts without invoking Cargo or building.
    pub fn verify_current(&self, workspace: &Path) -> Result<()> {
        if self.schema != SCHEMA || Self::collect(workspace, &self.selection)? != *self {
            return Err(fail(
                "production input inventory is stale or has an unsupported schema",
            ));
        }
        Ok(())
    }
    /// Watch selected files and declared directory memberships, never full src trees.
    pub fn emit_cargo_rerun_directives(&self, workspace: &Path) {
        for path in self.files.keys().chain(self.manifests.keys()) {
            println!("cargo:rerun-if-changed={}", workspace.join(path).display());
        }
        println!(
            "cargo:rerun-if-changed={}",
            workspace.join("Cargo.lock").display()
        );
        for path in self.memberships.keys() {
            println!("cargo:rerun-if-changed={}", workspace.join(path).display());
        }
    }
    /// Admit a directory-only compiler observation when it is an exact
    /// membership owner, or lies below a recursively selected membership root.
    /// This never admits file contents: every compiler-read file still needs its
    /// own inventory, resolution, generated-source, or external-package owner.
    pub(crate) fn declares_directory_observation(&self, path: &str) -> bool {
        if self.memberships.contains_key(path) {
            return true;
        }
        let candidate = Path::new(path);
        self.memberships.iter().any(|(root, members)| {
            !members.is_empty()
                && members
                    .iter()
                    .all(|member| self.memberships.contains_key(member))
                && candidate.starts_with(root)
        })
    }

    pub(crate) fn add_file(
        &mut self,
        workspace: &Path,
        relative: &str,
        owner: &str,
        role: InputRole,
    ) -> Result<()> {
        let path = checked_path(workspace, Path::new(relative))?;
        let bytes = std::fs::read(&path)
            .map_err(|e| fail(format!("{owner}: read {}: {e}", path.display())))?;
        let entry = self
            .files
            .entry(relative.to_owned())
            .or_insert_with(|| FileInput {
                sha256: digest(&bytes),
                owners: BTreeSet::new(),
                roles: BTreeSet::new(),
            });
        if entry.sha256 != digest(&bytes) {
            return Err(fail(format!("input changed during collection: {relative}")));
        }
        entry.owners.insert(owner.to_owned());
        entry.roles.insert(role);
        Ok(())
    }
}

/// Normalize a selected path and refuse symlinks, absolute paths and workspace escapes.
pub fn checked_path(workspace: &Path, relative: &Path) -> Result<PathBuf> {
    if relative.is_absolute() {
        return Err(fail(format!(
            "input is not workspace-relative: {}",
            relative.display()
        )));
    }
    let mut normalized = PathBuf::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => {
                normalized.push(part);
                let current = workspace.join(&normalized);
                let metadata = std::fs::symlink_metadata(&current)
                    .map_err(|e| fail(format!("read {}: {e}", current.display())))?;
                if metadata.file_type().is_symlink() {
                    return Err(fail(format!("input is a symlink: {}", current.display())));
                }
            }
            Component::CurDir => {}
            Component::ParentDir if normalized.pop() => {}
            _ => {
                return Err(fail(format!(
                    "input escapes workspace: {}",
                    relative.display()
                )));
            }
        }
    }
    let current = workspace.join(normalized);
    Ok(current)
}

pub(crate) fn relative(workspace: &Path, path: &Path) -> Result<String> {
    path.strip_prefix(workspace)
        .map_err(fail)?
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| fail("input path is not UTF-8"))
}
