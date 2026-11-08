// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Package proof for the two syntax-only JSON macros in the pinned substrate.
//! A dependency alias alone, local package, alternate repository or floating Git
//! selection never grants a source-input capability. Registry capability is an
//! explicitly reviewed protocol for purrdf-lex 4.0.0, not a grant to every release.

use crate::{Result, UnitSelection, checked_path, fail};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const REPOSITORY: &str = "https://github.com/Blackcat-Informatics/purrdf";
const PACKAGE: &str = "purrdf-lex";

/// Read a checked workspace document without admitting an external path.
fn document(workspace: &Path, relative: &str) -> Result<toml::Value> {
    std::fs::read_to_string(checked_path(workspace, Path::new(relative))?)
        .map_err(fail)?
        .parse()
        .map_err(fail)
}

pub(crate) const REGISTRY: &str = "registry+https://github.com/rust-lang/crates.io-index";
const REVIEWED_REGISTRY_VERSION: &str = "4.0.0";

/// Exact resolved package owner; the lock checksum identifies registry bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Owner {
    pub source: String,
    pub version: String,
    pub checksum: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Declaration {
    Git {
        revision: String,
        requirement: Option<String>,
    },
    Registry {
        requirement: String,
    },
}

/// Validate Cargo checksum syntax through the shared canonical hex reader.
pub(crate) fn valid_checksum(value: &str) -> bool {
    value.len() == 64 && purrdf_hash::hex::decode_canonical(value).is_ok()
}

/// Admit only the official lexical package's supported source declarations.
fn source(name: &str, value: &toml::Value) -> Option<Declaration> {
    if let Some(requirement) = value.as_str() {
        semver::VersionReq::parse(requirement).ok()?;
        return (name == PACKAGE).then(|| Declaration::Registry {
            requirement: requirement.into(),
        });
    }
    let fields = value.as_table()?;
    if fields.keys().any(|key| {
        !matches!(
            key.as_str(),
            "package" | "git" | "rev" | "version" | "optional" | "features" | "default-features"
        )
    }) {
        return None;
    }
    let package = match value.get("package") {
        None => name,
        Some(package) => package.as_str()?,
    };
    if package != PACKAGE {
        return None;
    }
    let requirement = match value.get("version") {
        None => None,
        Some(version) => {
            let requirement = version.as_str()?;
            semver::VersionReq::parse(requirement).ok()?;
            Some(requirement.to_owned())
        }
    };
    if value.get("git").is_some() || value.get("rev").is_some() {
        if value.get("git").and_then(toml::Value::as_str) != Some(REPOSITORY) {
            return None;
        }
        let revision = value.get("rev")?.as_str()?;
        if revision.len() != 40
            || !revision
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return None;
        }
        Some(Declaration::Git {
            revision: revision.into(),
            requirement,
        })
    } else {
        Some(Declaration::Registry {
            requirement: requirement?,
        })
    }
}

/// Bind a declaration to one exact lock owner and its reviewed macro protocol.
fn resolved_owner(declaration: &Declaration, records: &[toml::Value]) -> Option<Owner> {
    let mut matches = records
        .iter()
        .filter(|record| record.get("name").and_then(toml::Value::as_str) == Some(PACKAGE));
    let record = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    let version = record.get("version")?.as_str()?;
    let parsed_version = semver::Version::parse(version).ok()?;
    let locked_source = record.get("source")?.as_str()?;
    match declaration {
        Declaration::Git {
            revision,
            requirement,
        } => {
            let source = format!("git+{REPOSITORY}?rev={revision}");
            if record.get("checksum").is_some()
                || locked_source != format!("{source}#{revision}")
                || requirement.as_ref().is_some_and(|requirement| {
                    !semver::VersionReq::parse(requirement)
                        .is_ok_and(|requirement| requirement.matches(&parsed_version))
                })
            {
                return None;
            }
            Some(Owner {
                source,
                version: version.into(),
                checksum: None,
            })
        }
        Declaration::Registry { requirement } => {
            if locked_source != REGISTRY
                || version != REVIEWED_REGISTRY_VERSION
                || !semver::VersionReq::parse(requirement)
                    .ok()?
                    .matches(&parsed_version)
            {
                return None;
            }
            let checksum = record.get("checksum")?.as_str()?;
            if !valid_checksum(checksum) {
                return None;
            }
            Some(Owner {
                source: REGISTRY.into(),
                version: version.into(),
                checksum: Some(checksum.into()),
            })
        }
    }
}

/// Detect overrides that could replace or obscure the lexical macro provider.
fn overrides(value: &toml::Value) -> bool {
    value
        .get("replace")
        .is_some_and(|rows| rows.as_table().is_none_or(|rows| !rows.is_empty()))
        || value.get("patch").is_some_and(|patch| {
            patch
                .as_table()
                .is_none_or(|sources| sources.values().any(|source| source.as_table().is_none()))
        })
        || value
            .get("patch")
            .and_then(toml::Value::as_table)
            .into_iter()
            .flat_map(|sources| sources.values())
            .filter_map(toml::Value::as_table)
            .flat_map(|packages| packages.iter())
            .any(|(name, dependency)| {
                if !dependency.is_str() && !dependency.is_table() {
                    return true;
                }
                match dependency.get("package") {
                    None => name == PACKAGE,
                    Some(package) => package.as_str().is_none_or(|package| package == PACKAGE),
                }
            })
}

/// Resolve workspace inheritance without accepting local source overrides.
fn inherited<'a>(
    root: &'a toml::Value,
    name: &str,
    declaration: &'a toml::Value,
) -> Result<Option<&'a toml::Value>> {
    if declaration.get("workspace").and_then(toml::Value::as_bool) != Some(true) {
        return Ok(Some(declaration));
    }
    if declaration.as_table().is_none_or(|fields| {
        fields.keys().any(|key| {
            !matches!(
                key.as_str(),
                "workspace" | "optional" | "features" | "default-features"
            )
        })
    }) {
        return Ok(None);
    }
    root.get("workspace")
        .and_then(|value| value.get("dependencies"))
        .and_then(|dependencies| dependencies.get(name))
        .map(Some)
        .ok_or_else(|| fail(format!("missing workspace dependency {name}")))
}

/// Retain aliases whose declarations all name one unambiguous supported source.
fn declared_owners(
    root: &toml::Value,
    declared: &BTreeMap<String, Vec<&toml::Value>>,
) -> Result<BTreeMap<String, Declaration>> {
    let mut candidates = BTreeMap::new();
    for (name, declarations) in declared {
        let mut sources = BTreeSet::new();
        let mut complete = true;
        for declaration in declarations {
            let Some(value) = inherited(root, name, declaration)? else {
                complete = false;
                break;
            };
            match source(name, value) {
                Some(source) => {
                    sources.insert(source);
                }
                None => {
                    complete = false;
                    break;
                }
            }
        }
        let admitted = if complete && sources.len() == 1 {
            sources.into_iter().next()
        } else {
            None
        };
        candidates
            .entry(name.replace('-', "_"))
            .and_modify(|previous| {
                if *previous != admitted {
                    *previous = None;
                }
            })
            .or_insert(admitted);
    }
    Ok(candidates
        .into_iter()
        .filter_map(|(name, source)| source.map(|source| (name, source)))
        .collect())
}

/// Prove macro ownership from declarations, configuration and an exact lock entry.
pub(crate) fn portable(
    workspace: &Path,
    owner: &toml::Value,
    declared: &BTreeMap<String, Vec<&toml::Value>>,
) -> Result<BTreeMap<String, Owner>> {
    let root = document(workspace, "Cargo.toml")?;
    if overrides(&root) || overrides(owner) {
        return Ok(BTreeMap::new());
    }
    let allowed = declared_owners(&root, declared)?;
    if allowed.is_empty() {
        return Ok(BTreeMap::new());
    }
    for path in [".cargo/config.toml", ".cargo/config"] {
        if !workspace.join(path).is_file() {
            continue;
        }
        let config = document(workspace, path)?;
        if overrides(&config)
            || config.get("source").is_some()
            || config
                .get("paths")
                .is_some_and(|paths| paths.as_array().is_none_or(|paths| !paths.is_empty()))
        {
            return Ok(BTreeMap::new());
        }
    }
    // A declaration grants capability only through one exact locked owner.
    let lock = document(workspace, "Cargo.lock")?;
    let records = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| fail("Cargo.lock has no package records"))?;
    Ok(allowed
        .into_iter()
        .filter_map(|(alias, declaration)| {
            resolved_owner(&declaration, records).map(|owner| (alias, owner))
        })
        .collect())
}

/// Admit aliases only when the selected external source, name and version match.
pub(crate) fn selected(
    workspace: &Path,
    unit: &UnitSelection,
    units: &[UnitSelection],
) -> Result<BTreeSet<String>> {
    if !matches!(unit.edition.as_str(), "2018" | "2021" | "2024") {
        return Ok(BTreeSet::new());
    }
    let Some(manifest) = &unit.manifest else {
        return Ok(BTreeSet::new());
    };
    let owner = document(workspace, manifest)?;
    let declared = crate::cargo::declared_dependencies(&owner);
    let proven = portable(workspace, &owner, &declared)?;
    Ok(unit
        .dependency_names
        .iter()
        .zip(&unit.dependencies)
        .filter_map(|(alias, index)| {
            let dependency = &units[*index];
            let (source, name, version) = crate::cargo::package_parts(&dependency.package).ok()?;
            (dependency.manifest.is_none()
                && name == PACKAGE
                && proven.get(alias).is_some_and(|expected| {
                    source == expected.source && version == expected.version
                }))
            .then(|| alias.clone())
        })
        .collect())
}
