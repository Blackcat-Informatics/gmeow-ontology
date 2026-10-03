// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Resolve the declared PurRDF requirement against authenticated build inputs.
//! Manifests state compatibility; committed lockfiles state the selected release.

use std::collections::BTreeMap;
use std::path::Path;

use gmeow_errors::{Diag, Result};

/// The lowest PurRDF major release any declared requirement or locked package may name.
///
/// Every `purrdf` and `purrdf-*` crate moved to the 3.x substrate together; a 2.x
/// component anywhere in either committed lockfile, or a requirement that could admit
/// one, would reintroduce the retired serde/ciborium/ed25519-dalek boundary types.
pub const PURRDF_MAJOR_FLOOR: u64 = 3;

fn is_purrdf_package(name: &str) -> bool {
    name == "purrdf" || name.starts_with("purrdf-")
}

/// Whether some comparator of `requirement` excludes every release below the floor.
///
/// Cargo intersects comparators, so one lower bound at or above `FLOOR.0.0` suffices;
/// `<`/`<=` comparators carry no lower bound and never satisfy it.
fn excludes_below_floor(requirement: &semver::VersionReq) -> bool {
    requirement.comparators.iter().any(|comparator| {
        use semver::Op;
        match comparator.op {
            Op::Exact | Op::GreaterEq | Op::Tilde | Op::Caret | Op::Wildcard => {
                comparator.major >= PURRDF_MAJOR_FLOOR
            }
            // `>N` with no minor excludes all of N.x.y; `>N.m[.p]` only part of it.
            Op::Greater => {
                comparator.major >= PURRDF_MAJOR_FLOOR
                    || (comparator.major + 1 == PURRDF_MAJOR_FLOOR && comparator.minor.is_none())
            }
            _ => false,
        }
    })
}

/// A registry requirement and the concrete release selected by its lockfile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurrdfResolution {
    /// The manifest's normalized semantic-version requirement.
    pub requirement: String,
    /// The exact resolved release, never a compatibility range.
    pub version: String,
}

/// The complete Cargo source identity of one locked PurRDF package.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PackageIdentity {
    version: String,
    source: String,
    checksum: String,
}

fn invalid(detail: impl Into<String>) -> Diag {
    Diag::of_kind(crate::error::Parse {
        detail: detail.into(),
    })
}

fn read_toml(path: &Path) -> Result<toml::Value> {
    let text = std::fs::read_to_string(path).map_err(|error| {
        Diag::of_kind(crate::error::Io {
            detail: format!("{}: {error}", path.display()),
        })
    })?;
    text.parse()
        .map_err(|error| invalid(format!("{}: {error}", path.display())))
}

/// Read every PurRDF package, rejecting missing or ambiguous source identities.
fn locked_purrdf_packages(path: &Path) -> Result<BTreeMap<String, PackageIdentity>> {
    let lock = read_toml(path)?;
    let packages = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| invalid(format!("{}: missing package table", path.display())))?;
    let mut identities = BTreeMap::new();
    for package in packages {
        let name = package
            .get("name")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| invalid(format!("{}: package has no name", path.display())))?;
        if !is_purrdf_package(name) {
            continue;
        }
        let field = |key| {
            package
                .get(key)
                .and_then(toml::Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| invalid(format!("{}: {name} has no {key}", path.display())))
        };
        let identity = PackageIdentity {
            version: field("version")?,
            source: field("source")?,
            checksum: field("checksum")?,
        };
        if identity.source != "registry+https://github.com/rust-lang/crates.io-index" {
            return Err(invalid(format!(
                "{}: {name} is not a crates.io release",
                path.display()
            )));
        }
        let parsed = semver::Version::parse(&identity.version).map_err(|error| {
            invalid(format!(
                "{}: invalid {name} version: {error}",
                path.display()
            ))
        })?;
        if parsed.major < PURRDF_MAJOR_FLOOR {
            return Err(invalid(format!(
                "{}: {name} {} is below the PurRDF {PURRDF_MAJOR_FLOOR}.x floor",
                path.display(),
                identity.version
            )));
        }
        if identity.checksum.len() != 64
            || !identity
                .checksum
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(invalid(format!(
                "{}: {name} requires its complete release checksum",
                path.display()
            )));
        }
        if identities.insert(name.to_owned(), identity).is_some() {
            return Err(invalid(format!(
                "{}: multiple resolutions of {name}",
                path.display()
            )));
        }
    }
    let selected = identities
        .get("purrdf")
        .ok_or_else(|| invalid(format!("{}: resolves no purrdf package", path.display())))?;
    if !identities.contains_key("purrdf-core") {
        return Err(invalid(format!(
            "{}: resolves no purrdf-core package",
            path.display()
        )));
    }
    for (name, identity) in &identities {
        if identity.version != selected.version || identity.source != selected.source {
            return Err(invalid(format!(
                "{}: {name} differs from the selected purrdf release or source",
                path.display()
            )));
        }
    }
    Ok(identities)
}

/// Read one named dependency requirement without admitting source replacement.
fn registry_requirement(
    dependencies: &toml::Value,
    name: &str,
    manifest_path: &Path,
) -> Result<semver::VersionReq> {
    let dependency = dependencies.get(name).ok_or_else(|| {
        invalid(format!(
            "{}: declares no {name} dependency",
            manifest_path.display()
        ))
    })?;
    let requirement = match dependency {
        toml::Value::String(value) => Some(value.as_str()),
        toml::Value::Table(table)
            if ![
                "git",
                "path",
                "registry",
                "registry-index",
                "rev",
                "branch",
                "tag",
                "workspace",
            ]
            .iter()
            .any(|key| table.contains_key(*key))
                && table
                    .get("package")
                    .is_none_or(|package| package.as_str() == Some(name)) =>
        {
            table.get("version").and_then(toml::Value::as_str)
        }
        _ => None,
    }
    .ok_or_else(|| {
        invalid(format!(
            "{}: {name} must declare a crates.io version requirement",
            manifest_path.display()
        ))
    })?;
    semver::VersionReq::parse(requirement).map_err(|error| {
        invalid(format!(
            "{}: invalid {name} requirement: {error}",
            manifest_path.display()
        ))
    })
}

/// Resolve a root or standalone manifest's PurRDF requirement against its lockfile.
///
/// # Errors
/// Rejects missing inputs, non-registry declarations, ambiguous resolutions, and a
/// locked release outside the declared requirement. The workspace must declare the
/// direct core dependency, and every declared `purrdf-*` component must state the same
/// compatibility requirement as `purrdf`, which must admit nothing below
/// [`PURRDF_MAJOR_FLOOR`]. Every locked PurRDF component must select that release, sit
/// at or above the floor, and retain its own checksum. This never updates a lockfile.
pub fn resolve_purrdf(manifest_path: &Path, lock_path: &Path) -> Result<PurrdfResolution> {
    let manifest = read_toml(manifest_path)?;
    let workspace_dependencies = manifest
        .get("workspace")
        .and_then(|table| table.get("dependencies"));
    let dependencies = workspace_dependencies
        .or_else(|| manifest.get("dependencies"))
        .ok_or_else(|| {
            invalid(format!(
                "{}: declares no dependencies",
                manifest_path.display()
            ))
        })?;
    let requirement = registry_requirement(dependencies, "purrdf", manifest_path)?;
    if !excludes_below_floor(&requirement) {
        return Err(invalid(format!(
            "{}: purrdf requirement {requirement} admits releases below the PurRDF \
             {PURRDF_MAJOR_FLOOR}.x floor",
            manifest_path.display()
        )));
    }
    if workspace_dependencies.is_some() {
        registry_requirement(dependencies, "purrdf-core", manifest_path)?;
    }
    let components = dependencies
        .as_table()
        .into_iter()
        .flat_map(|table| table.keys())
        .filter(|name| name.as_str() != "purrdf" && is_purrdf_package(name));
    for name in components {
        if registry_requirement(dependencies, name, manifest_path)? != requirement {
            return Err(invalid(format!(
                "{}: purrdf and {name} requirements disagree",
                manifest_path.display()
            )));
        }
    }
    let packages = locked_purrdf_packages(lock_path)?;
    let version = &packages["purrdf"].version;
    let parsed = semver::Version::parse(version).map_err(|error| {
        invalid(format!(
            "{}: invalid purrdf version: {error}",
            lock_path.display()
        ))
    })?;
    if !requirement.matches(&parsed) {
        return Err(invalid(format!(
            "{}: purrdf requirement {requirement} does not admit {version} from {}",
            manifest_path.display(),
            lock_path.display()
        )));
    }
    Ok(PurrdfResolution {
        requirement: requirement.to_string(),
        version: version.clone(),
    })
}

/// Require the standalone fuzz workspace to select exactly the production substrate.
///
/// # Errors
/// Rejects missing lockfiles and any differing PurRDF package, release, source, or
/// checksum. The two workspaces may select different non-PurRDF dependencies.
pub fn verify_fuzz_substrate(root: &Path) -> Result<()> {
    let root_resolution = resolve_purrdf(&root.join("Cargo.toml"), &root.join("Cargo.lock"))?;
    let fuzz_resolution =
        resolve_purrdf(&root.join("fuzz/Cargo.toml"), &root.join("fuzz/Cargo.lock"))?;
    if root_resolution.requirement != fuzz_resolution.requirement {
        return Err(invalid("root and fuzz purrdf requirements disagree"));
    }
    let production = locked_purrdf_packages(&root.join("Cargo.lock"))?;
    let fuzz = locked_purrdf_packages(&root.join("fuzz/Cargo.lock"))?;
    if production != fuzz {
        let names: std::collections::BTreeSet<_> = production.keys().chain(fuzz.keys()).collect();
        let changed: Vec<_> = names
            .into_iter()
            .filter(|name| production.get(*name) != fuzz.get(*name))
            .cloned()
            .collect();
        return Err(invalid(format!(
            "fuzz/Cargo.lock differs from Cargo.lock for PurRDF packages: {}",
            changed.join(", ")
        )));
    }
    Ok(())
}

#[path = "substrate.tests.rs"]
#[cfg(test)]
mod tests;
