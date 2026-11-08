// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

mod json;

use crate::{Result, checked_path, digest, fail};
use std::collections::BTreeMap;
use std::path::Path;

const NATIVE_ROOTS: &[&str] = &[
    "crates/logic/src/lib.rs",
    "crates/logic-compile/src/lib.rs",
    "crates/term-arena/src/lib.rs",
    "crates/ns/src/lib.rs",
];

/// The exact directory observation emitted by the native build script. A missing
/// configuration directory is observed through its parent so its creation is seen.
pub(crate) fn configuration_watch_owner(workspace: &Path) -> Result<String> {
    let directory = workspace.join(".cargo");
    match std::fs::symlink_metadata(&directory) {
        Ok(_) => {
            let directory = checked_path(workspace, Path::new(".cargo"))?;
            if !directory.is_dir() {
                return Err(fail("native Cargo configuration owner is not a directory"));
            }
            Ok(".cargo".into())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(fail(format!("read {}: {error}", directory.display()))),
    }
}

/// The immutable upstream source behind native datatype and entailment semantics.
/// Registry releases bind every locked substrate leaf, not just the facade version.
/// Reading this contract performs no compiler or host discovery.
pub fn native_substrate_source(workspace: &Path) -> Result<String> {
    match native_selection(workspace)? {
        NativeSelection::Git {
            repository,
            revision,
        } => Ok(format!("{repository}#{revision}")),
        NativeSelection::Registry(packages) => {
            let facade = packages
                .get("purrdf")
                .ok_or_else(|| fail("native registry substrate has no facade"))?;
            let mut inventory = String::from("gmeow-purrdf-registry-inventory-v1\n");
            for (name, owner) in &packages {
                let checksum = owner.checksum.as_deref().ok_or_else(|| {
                    fail(format!("native registry package {name} has no checksum"))
                })?;
                inventory.push_str(&format!(
                    "{name}\t{}\t{}\t{checksum}\n",
                    owner.version, owner.source
                ));
            }
            let checksum = facade
                .checksum
                .as_deref()
                .ok_or_else(|| fail("native registry facade has no checksum"))?;
            Ok(format!(
                "{}#purrdf@{};checksum={checksum};purrdf-lock-sha256={}",
                facade.source,
                facade.version,
                digest(inventory.as_bytes())
            ))
        }
    }
}

enum NativeSelection {
    Git {
        repository: String,
        revision: String,
    },
    Registry(BTreeMap<String, crate::json_macros::Owner>),
}

/// Compare actual macro provenance with the selected substrate. A Git revision
/// owns the monorepo; a registry facade delegates to the exact locked lex leaf.
pub(crate) fn matches_native_json_owner(
    workspace: &Path,
    owner: &crate::json_macros::Owner,
) -> Result<bool> {
    Ok(match native_selection(workspace)? {
        NativeSelection::Git {
            repository,
            revision,
        } => owner.source == format!("git+{repository}?rev={revision}") && owner.checksum.is_none(),
        NativeSelection::Registry(packages) => packages.get("purrdf-lex") == Some(owner),
    })
}

fn native_selection(workspace: &Path) -> Result<NativeSelection> {
    let manifest =
        std::fs::read_to_string(checked_path(workspace, Path::new("Cargo.toml"))?).map_err(fail)?;
    let manifest: toml::Value = toml::from_str(&manifest).map_err(fail)?;
    let dependency = manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(|dependencies| dependencies.get("purrdf"))
        .ok_or_else(|| fail("native substrate requires workspace.dependencies.purrdf"))?;
    if let Some(fields) = dependency.as_table()
        && fields.keys().any(|key| {
            !matches!(
                key.as_str(),
                "git" | "rev" | "version" | "features" | "default-features" | "optional"
            )
        })
    {
        return Err(fail(
            "native substrate has an unsupported source selector or package override",
        ));
    }
    let git = dependency.get("git").and_then(toml::Value::as_str);
    let revision = dependency.get("rev").and_then(toml::Value::as_str);
    if dependency.get("git").is_some() || dependency.get("rev").is_some() {
        return match (git, revision) {
            (Some(git), Some(revision))
                if !git.is_empty()
                    && !git.chars().any(char::is_whitespace)
                    && revision.len() == 40
                    && revision.bytes().all(|byte| byte.is_ascii_hexdigit()) =>
            {
                Ok(NativeSelection::Git {
                    repository: git.into(),
                    revision: revision.to_ascii_lowercase(),
                })
            }
            _ => Err(fail(
                "native substrate requires a Git URL and full immutable revision",
            )),
        };
    }
    let requirement = dependency
        .as_str()
        .or_else(|| dependency.get("version").and_then(toml::Value::as_str))
        .ok_or_else(|| fail("native registry substrate requires a version requirement"))?;
    let requirement = semver::VersionReq::parse(requirement).map_err(fail)?;
    registry_policy(workspace, &manifest)?;
    let lock: toml::Value =
        std::fs::read_to_string(checked_path(workspace, Path::new("Cargo.lock"))?)
            .map_err(fail)?
            .parse()
            .map_err(fail)?;
    let records = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| fail("native substrate Cargo.lock has no package records"))?;
    let mut packages = BTreeMap::new();
    for record in records {
        let name = record
            .get("name")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| fail("native substrate lock package has no valid name"))?;
        if name != "purrdf" && !name.starts_with("purrdf-") {
            continue;
        }
        if !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(fail(format!(
                "native substrate package name is invalid: {name:?}"
            )));
        }
        let version = record
            .get("version")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| fail(format!("native registry package {name} has no version")))?;
        semver::Version::parse(version).map_err(fail)?;
        let source = record.get("source").and_then(toml::Value::as_str);
        if source != Some(crate::json_macros::REGISTRY) {
            return Err(fail(format!(
                "native registry package {name} has an invalid or mixed source"
            )));
        }
        let checksum = record.get("checksum").and_then(toml::Value::as_str)
            .filter(|checksum| crate::json_macros::valid_checksum(checksum))
            .ok_or_else(|| fail(format!("native registry package {name} requires a 64-character lowercase hexadecimal checksum")))?;
        let owner = crate::json_macros::Owner {
            source: crate::json_macros::REGISTRY.into(),
            version: version.into(),
            checksum: Some(checksum.into()),
        };
        if packages.insert(name.to_owned(), owner).is_some() {
            return Err(fail(format!(
                "native registry package {name} has ambiguous lock records"
            )));
        }
    }
    let facade = packages
        .get("purrdf")
        .ok_or_else(|| fail("native registry substrate has no locked purrdf facade"))?;
    if !requirement.matches(&semver::Version::parse(&facade.version).map_err(fail)?) {
        return Err(fail(
            "native registry substrate version does not satisfy its declaration",
        ));
    }
    if !packages.contains_key("purrdf-lex") {
        return Err(fail(
            "native registry substrate has no delegated purrdf-lex leaf",
        ));
    }
    if packages
        .values()
        .any(|owner| owner.version != facade.version)
    {
        return Err(fail("native registry substrate has mixed package versions"));
    }
    Ok(NativeSelection::Registry(packages))
}

fn registry_policy(workspace: &Path, manifest: &toml::Value) -> Result<()> {
    for document in std::iter::once(manifest.clone()).chain(
        [".cargo/config.toml", ".cargo/config"]
            .into_iter()
            .filter(|path| workspace.join(path).is_file())
            .map(|path| {
                std::fs::read_to_string(checked_path(workspace, Path::new(path))?)
                    .map_err(fail)?
                    .parse::<toml::Value>()
                    .map_err(fail)
            })
            .collect::<Result<Vec<_>>>()?,
    ) {
        if ["patch", "replace", "source"].iter().any(|key| {
            document
                .get(key)
                .is_some_and(|value| value.as_table().is_none_or(|table| !table.is_empty()))
        }) || document
            .get("paths")
            .is_some_and(|paths| paths.as_array().is_none_or(|paths| !paths.is_empty()))
        {
            return Err(fail(
                "native registry substrate rejects Cargo source, path, patch and replace overrides",
            ));
        }
    }
    Ok(())
}

/// Resolve the native kernel's production module paths for an offline extraction
/// plan. This does not admit any source identity or produce a semantic digest.
pub fn native_extraction_paths(workspace: &Path) -> Result<Vec<String>> {
    crate::modules::extraction_paths(workspace, NATIVE_ROOTS)
}

/// Portable semantic source contract, deliberately independent of a Cargo profile,
/// target or selected feature set. Every non-test branch of the canonical kernel's
/// module declarations belongs to this contract, on native and WebAssembly alike.
#[derive(Clone, Debug, PartialEq, Eq)]
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
        Ok(digest(&purrdf_lex::json::record::to_vec(&(
            "gmeow-native-semantic-sources-v1",
            &self.files,
        ))))
    }

    /// Watch selected sources and the manifests/configuration proving macro ownership.
    /// Test subtrees remain outside this source contract.
    pub fn emit_cargo_rerun_directives(&self, workspace: &Path) -> Result<()> {
        let config_owner = configuration_watch_owner(workspace)?;
        for path in self.files.keys() {
            println!("cargo:rerun-if-changed={}", workspace.join(path).display());
        }
        for path in [
            "Cargo.toml",
            "Cargo.lock",
            "crates/logic/Cargo.toml",
            "crates/logic-compile/Cargo.toml",
            "crates/term-arena/Cargo.toml",
            "crates/ns/Cargo.toml",
        ] {
            println!("cargo:rerun-if-changed={}", workspace.join(path).display());
        }
        // Cargo recursively watches a directory, including new config files. An
        // absent file watch would instead make every invocation rerun this build.
        println!(
            "cargo:rerun-if-changed={}",
            workspace.join(config_owner).display()
        );
        Ok(())
    }
}
