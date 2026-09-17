// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use crate::{InputInventory, InputRole, Result, UnitSelection, checked_path, fail, relative};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Complete authored verify-query owner shared by the source inventory and the
/// source generator. Directory membership and contents are separate observations.
#[derive(Clone, Debug)]
pub struct VerifyQueries {
    pub files: BTreeMap<String, PathBuf>,
    pub memberships: BTreeMap<String, Vec<String>>,
    watchers: BTreeSet<String>,
}
impl VerifyQueries {
    pub fn collect(workspace: &Path) -> Result<Self> {
        let mut result = Self {
            files: BTreeMap::new(),
            memberships: BTreeMap::new(),
            watchers: BTreeSet::new(),
        };
        result.queries(workspace, &workspace.join("queries/verify"))?;
        result.slices(workspace, &workspace.join("slices"))?;
        let directories = result
            .memberships
            .keys()
            .filter(|name| name.starts_with("slices/"))
            .cloned()
            .collect();
        result.memberships.insert("slices".into(), directories);
        if result.files.is_empty() {
            return Err(fail("embedded verify query selection is empty"));
        }
        Ok(result)
    }
    fn children(&mut self, workspace: &Path, directory: &Path) -> Result<Vec<PathBuf>> {
        let directory = checked_path(workspace, Path::new(&relative(workspace, directory)?))?;
        self.watchers.insert(relative(workspace, &directory)?);
        let mut paths = std::fs::read_dir(&directory)
            .map_err(fail)?
            .map(|entry| entry.map(|entry| entry.path()).map_err(fail))
            .collect::<Result<Vec<_>>>()?;
        paths.sort();
        for path in &paths {
            checked_path(workspace, Path::new(&relative(workspace, path)?))?;
        }
        Ok(paths)
    }
    fn queries(&mut self, workspace: &Path, directory: &Path) -> Result<()> {
        let children = self.children(workspace, directory)?;
        let mut members = Vec::new();
        for path in children {
            if path.extension().is_some_and(|extension| extension == "rq") {
                if !path.is_file() {
                    return Err(fail("query owner selected a non-file"));
                }
                let stem = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .ok_or_else(|| fail("query stem is not UTF-8"))?
                    .to_owned();
                members.push(relative(workspace, &path)?);
                if self.files.insert(stem.clone(), path).is_some() {
                    return Err(fail(format!("duplicate verify-query stem {stem}")));
                }
            }
        }
        self.memberships
            .insert(relative(workspace, directory)?, members);
        Ok(())
    }
    fn slices(&mut self, workspace: &Path, directory: &Path) -> Result<()> {
        let children = self.children(workspace, directory)?;
        for path in children {
            if path.is_dir() {
                let verify = path.join("queries/verify");
                if verify.is_dir() {
                    self.queries(workspace, &verify)?;
                }
                self.slices(workspace, &path)?;
            }
        }
        Ok(())
    }
    pub fn emit_cargo_rerun_directives(&self, workspace: &Path) {
        for path in self.files.values() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
        for path in &self.watchers {
            println!("cargo:rerun-if-changed={}", workspace.join(path).display());
        }
    }
}
pub(crate) fn generated(
    workspace: &Path,
    inventory: &mut InputInventory,
    unit: &UnitSelection,
    output: &str,
) -> Result<()> {
    match (unit.manifest.as_deref(), output) {
        (Some("crates/logic/Cargo.toml"), "verify_queries.rs") => {
            let queries = VerifyQueries::collect(workspace)?;
            for path in queries.files.values() {
                inventory.add_file(
                    workspace,
                    &relative(workspace, path)?,
                    "logic-verify-queries",
                    InputRole::Dynamic,
                )?;
            }
            inventory.memberships.extend(queries.memberships);
            inventory
                .generated
                .insert("verify_queries.rs".into(), "crates/logic/build.rs".into());
            inventory.add_file(
                workspace,
                "crates/logic/build.rs",
                "logic-verify-queries",
                InputRole::Rust,
            )
        }
        (Some("crates/logic/Cargo.toml"), "native_semantic_sources.rs") => {
            let native = crate::NativeSources::collect(workspace)?;
            for path in native.files.keys() {
                inventory.add_file(
                    workspace,
                    path,
                    "native-semantic-kernel",
                    InputRole::Dynamic,
                )?;
            }
            inventory.generated.insert(
                "native_semantic_sources.rs".into(),
                "crates/logic/build.rs".into(),
            );
            inventory.add_file(
                workspace,
                "crates/logic/build.rs",
                "native-semantic-kernel",
                InputRole::Rust,
            )
        }
        _ => Err(fail(format!(
            "generated source {output} has no declared owner for {}",
            unit.package
        ))),
    }
}

/// Compile-time environment is admitted through its deterministic Cargo or
/// generator owner. Arbitrary ambient variables are never silently omitted.
pub(crate) fn environment_owner(unit: &UnitSelection, name: &str) -> Result<String> {
    let manifest = unit
        .manifest
        .as_deref()
        .ok_or_else(|| fail("workspace environment owner has no manifest"))?;
    if matches!(
        name,
        "CARGO_MANIFEST_DIR"
            | "CARGO_PKG_NAME"
            | "CARGO_PKG_VERSION"
            | "CARGO_PKG_VERSION_MAJOR"
            | "CARGO_PKG_VERSION_MINOR"
            | "CARGO_PKG_VERSION_PATCH"
            | "CARGO_PKG_VERSION_PRE"
            | "CARGO_PKG_AUTHORS"
            | "CARGO_PKG_DESCRIPTION"
            | "CARGO_PKG_HOMEPAGE"
            | "CARGO_PKG_LICENSE"
            | "CARGO_PKG_LICENSE_FILE"
            | "CARGO_PKG_REPOSITORY"
            | "CARGO_PKG_RUST_VERSION"
            | "CARGO_PKG_README"
    ) {
        return Ok(format!("cargo-manifest:{manifest}"));
    }
    let owns = match manifest {
        "crates/pipeline/Cargo.toml" => matches!(
            name,
            "GMEOW_BUILD_FEATURES"
                | "GMEOW_BUILD_FINGERPRINT"
                | "GMEOW_BUILD_PROFILE"
                | "GMEOW_BUILD_TARGET"
                | "GMEOW_PRODUCER_BUILD_CONTRACT"
                | "GMEOW_PRODUCER_COMPILATION_CONTRACT"
                | "GMEOW_TOOLCHAIN_FINGERPRINT"
        ),
        "crates/bundle-import/Cargo.toml" => name == "GMEOW_BUNDLE_IMPORT_BUILD_FINGERPRINT",
        "crates/slicetest/Cargo.toml" => name == "GMEOW_SLICETEST_BUILD_FINGERPRINT",
        "crates/logic/Cargo.toml" => name == "OUT_DIR",
        _ => false,
    };
    if owns {
        Ok(format!("selected-build-script:{manifest}"))
    } else {
        Err(fail(format!(
            "compile-time environment {name} has no input owner for {manifest}"
        )))
    }
}
