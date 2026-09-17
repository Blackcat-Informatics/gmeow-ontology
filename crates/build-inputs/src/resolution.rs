// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Freshness of Cargo's actual resolution, separate from semantic build identity.
//!
//! Cargo.lock combines normal, build and development edges. A selected package
//! remaining in the lock therefore cannot prove that a production edge still
//! selects it. Only the explicit controller resolves Cargo. These receipts let
//! read-only consumers reject changes without running Cargo or guessing edges.

use crate::{
    ProductionSelection, Result, cargo_policy_files, checked_path, digest, fail, relative,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const RESOLUTION_SCHEMA: u32 = 1;

/// Raw resolution inputs observed before asking Cargo for the selected graph.
/// These bytes intentionally include development declarations, but NEVER enter
/// executable, action, or portable native semantic identities.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CargoResolutionInputs {
    schema: u32,
    /// Every local manifest returned by Cargo metadata, including unselected
    /// workspace members that can participate in lockfile resolution.
    manifests: BTreeSet<String>,
    policy_files: Vec<String>,
    files: BTreeMap<String, String>,
    /// Cargo's declared member/exclusion pattern matches, so a new member behind
    /// an unchanged glob cannot hide behind an older metadata manifest list.
    workspace_patterns: BTreeMap<String, Vec<String>>,
    /// Cargo's real ancestor/home configuration layering, bound by ordered owner
    /// roles and content rather than producer-host absolute checkout/home paths.
    configuration: BTreeMap<String, String>,
    /// Only commitments are persisted, including for registry credentials.
    environment: BTreeMap<String, String>,
}

impl CargoResolutionInputs {
    /// Snapshot explicitly discovered metadata manifests without resolving Cargo.
    /// The caller must capture this before its unit-graph resolution and verify
    /// it afterwards, then bind the exact graph returned by Cargo.
    pub fn capture(workspace: &Path, manifests: BTreeSet<String>) -> Result<Self> {
        if manifests.is_empty() {
            return Err(fail("Cargo resolution has no metadata manifest owners"));
        }
        let policy_files = cargo_policy_files(workspace)?;
        let mut files = BTreeMap::new();
        for path in manifests
            .iter()
            .cloned()
            .chain(["Cargo.toml".into(), "Cargo.lock".into()])
            .chain(policy_files.iter().cloned())
        {
            let checked = checked_path(workspace, Path::new(&path))?;
            files.insert(path, digest(&std::fs::read(checked).map_err(fail)?));
        }
        Ok(Self {
            schema: RESOLUTION_SCHEMA,
            manifests,
            policy_files,
            files,
            workspace_patterns: workspace_patterns(workspace)?,
            configuration: configuration_sources(workspace)?,
            environment: resolution_environment(workspace, std::env::vars_os())?,
        })
    }

    /// Fail closed on any resolution input change; this never invokes Cargo.
    pub fn verify_current(&self, workspace: &Path) -> Result<()> {
        if self.schema != RESOLUTION_SCHEMA {
            return Err(fail("unsupported Cargo resolution-input schema"));
        }
        if &Self::capture(workspace, self.manifests.clone())? != self {
            return Err(fail(
                "Cargo resolution evidence is stale; run make producer-build for explicit re-admission",
            ));
        }
        Ok(())
    }

    /// Bind freshly resolved selected units. This is evidence of the controller's
    /// resolution, not an alternate resolver or a semantic identity component.
    pub fn bind(self, selection: &ProductionSelection) -> Result<CargoResolutionEvidence> {
        selection.validate()?;
        if self.schema != RESOLUTION_SCHEMA
            || selection
                .units
                .iter()
                .filter_map(|unit| unit.manifest.as_ref())
                .any(|path| !self.manifests.contains(path))
            || self.policy_files != selection.policy_files
        {
            return Err(fail(
                "Cargo resolution evidence does not cover the selected manifest/policy owners",
            ));
        }
        Ok(CargoResolutionEvidence {
            schema: RESOLUTION_SCHEMA,
            selection_sha256: selection_digest(selection)?,
            inputs: self,
        })
    }
}

fn configuration_sources(workspace: &Path) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    let cargo_home = cargo_config2::cargo_home_with_cwd(workspace);
    let mut ancestor = 0usize;
    for path in cargo_config2::Walk::new(workspace) {
        let bytes = std::fs::read(&path).map_err(fail)?;
        let value: toml::Value = std::str::from_utf8(&bytes)
            .map_err(fail)?
            .parse()
            .map_err(fail)?;
        // These modes introduce mutable source/configuration owners outside the
        // selected metadata/module closure. Refuse them explicitly, rather than
        // treating their configuration file as proof of the unobserved contents.
        if value.get("include").is_some()
            || value.get("paths").is_some()
            || value
                .get("patch")
                .and_then(toml::Value::as_table)
                .is_some_and(|registries| {
                    registries.values().any(|registry| {
                        registry.as_table().is_some_and(|packages| {
                            packages
                                .values()
                                .any(|package| package.get("path").is_some())
                        })
                    })
                })
            || value
                .get("source")
                .and_then(toml::Value::as_table)
                .is_some_and(|sources| {
                    sources.values().any(|source| {
                        source.get("directory").is_some() || source.get("local-registry").is_some()
                    })
                })
        {
            return Err(fail(format!(
                "Cargo configuration {} selects an unsupported external resolution owner (include, paths, path patch, or mutable source replacement)",
                path.display()
            )));
        }
        let owner = if path.parent() == Some(workspace.join(".cargo").as_path()) {
            "workspace".to_owned()
        } else if path.parent() == cargo_home.as_deref() {
            "cargo-home".to_owned()
        } else {
            let owner = format!("ancestor:{ancestor}");
            ancestor += 1;
            owner
        };
        result.insert(owner, digest(&bytes));
    }
    Ok(result)
}

fn resolution_environment(
    workspace: &Path,
    values: impl IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    for (name, value) in values {
        let Some(name) = name.to_str() else { continue };
        // Output placement changes where Cargo writes bytes, never which packages,
        // features, target, or sources it resolves. The machine-level Cargo launcher
        // leases these directories per invocation, so admitting them would make a
        // freshly built producer stale merely because the next command received a
        // different output slot.
        if [
            "CARGO_TARGET_DIR",
            "CARGO_TARGET_TMPDIR",
            "CARGO_BUILD_TARGET_DIR",
            "CARGO_BUILD_BUILD_DIR",
        ]
        .contains(&name)
        {
            continue;
        }
        if [
            "RUSTUP_TOOLCHAIN",
            "RUSTC",
            "RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER",
        ]
        .contains(&name)
            || [
                "CARGO_BUILD_",
                "CARGO_TARGET_",
                "CARGO_PROFILE_",
                "CARGO_UNSTABLE_",
                "CARGO_SOURCE_",
                "CARGO_REGISTRIES_",
                "CARGO_REGISTRY_",
                "CARGO_NET_",
                "CARGO_RESOLVER_",
            ]
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            if name.starts_with("CARGO_SOURCE_")
                && (name.ends_with("_DIRECTORY") || name.ends_with("_LOCAL_REGISTRY"))
            {
                return Err(fail(
                    "Cargo environment selects an unsupported mutable source replacement",
                ));
            }
            let mut value = value
                .to_str()
                .ok_or_else(|| fail("Cargo resolution environment is not UTF-8"))?
                .replace(workspace.to_string_lossy().as_ref(), "<workspace>");
            if let Some(home) = cargo_config2::cargo_home_with_cwd(workspace) {
                value = value.replace(home.to_string_lossy().as_ref(), "<cargo-home>");
            }
            if let Some(home) = cargo_config2::rustup_home_with_cwd(workspace) {
                value = value.replace(home.to_string_lossy().as_ref(), "<rustup-home>");
            }
            result.insert(name.to_owned(), digest(value.as_bytes()));
        }
    }
    Ok(result)
}

/// Required freshness witness beside the executable recipe. It can be refreshed
/// after a development-only Cargo edit while every semantic identity stays equal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CargoResolutionEvidence {
    schema: u32,
    selection_sha256: String,
    inputs: CargoResolutionInputs,
}

impl CargoResolutionEvidence {
    /// Whether an exact workspace-relative file is authenticated as an input to
    /// Cargo resolution. These files may be observed by rustc or a selected
    /// build script, but remain deliberately separate from semantic source
    /// identity because Cargo.lock also carries development-only edges.
    pub(crate) fn declares_compiler_input(&self, path: &str) -> bool {
        self.inputs.files.contains_key(path)
    }

    /// Check the graph binding independently of checkout freshness. Only the
    /// explicit controller may refresh stale evidence after actual resolution.
    pub fn verify_selection(&self, selection: &ProductionSelection) -> Result<()> {
        if self.schema != RESOLUTION_SCHEMA || self.inputs.clone().bind(selection)? != *self {
            return Err(fail(
                "Cargo resolution evidence does not bind the admitted selection",
            ));
        }
        Ok(())
    }

    /// Authenticate current resolution inputs without Cargo or any production.
    pub fn verify_current(&self, workspace: &Path, selection: &ProductionSelection) -> Result<()> {
        self.verify_selection(selection)?;
        self.inputs.verify_current(workspace)
    }
}

fn selection_digest(selection: &ProductionSelection) -> Result<String> {
    Ok(digest(
        &serde_json::to_vec(&("cargo-selected-resolution-v1", selection)).map_err(fail)?,
    ))
}

fn workspace_patterns(workspace: &Path) -> Result<BTreeMap<String, Vec<String>>> {
    let manifest: toml::Value =
        std::fs::read_to_string(checked_path(workspace, Path::new("Cargo.toml"))?)
            .map_err(fail)?
            .parse()
            .map_err(fail)?;
    let mut result = BTreeMap::new();
    for key in ["members", "exclude"] {
        let Some(patterns) = manifest.get("workspace").and_then(|value| value.get(key)) else {
            continue;
        };
        for pattern in patterns
            .as_array()
            .ok_or_else(|| fail("Cargo workspace member patterns must be an array"))?
        {
            let pattern = pattern
                .as_str()
                .ok_or_else(|| fail("Cargo workspace member pattern is not a string"))?;
            if Path::new(pattern).is_absolute()
                || Path::new(pattern)
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir))
            {
                return Err(fail(
                    "Cargo workspace member pattern escapes the selected workspace",
                ));
            }
            let absolute = format!(
                "{}/{}",
                glob::Pattern::escape(&workspace.to_string_lossy()),
                pattern
            );
            let mut matches = Vec::new();
            for path in glob::glob(&absolute).map_err(fail)? {
                let path = path.map_err(fail)?;
                let path = relative(workspace, &path)?;
                checked_path(workspace, Path::new(&path))?;
                matches.push(path);
            }
            matches.sort();
            result.insert(format!("{key}:{pattern}"), matches);
        }
    }
    Ok(result)
}

#[cfg(test)]
#[path = "resolution_tests.rs"]
mod tests;
