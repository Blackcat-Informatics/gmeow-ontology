// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Package proof for the two syntax-only JSON macros in the pinned substrate.
//! A dependency alias alone, local package, alternate repository or floating Git
//! selection never grants a source-input capability.

use crate::{Result, UnitSelection, checked_path, fail};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const REPOSITORY: &str = "https://github.com/Blackcat-Informatics/purrdf";
const PACKAGE: &str = "purrdf-lex";

fn document(workspace: &Path, relative: &str) -> Result<toml::Value> {
    std::fs::read_to_string(checked_path(workspace, Path::new(relative))?)
        .map_err(fail)?
        .parse()
        .map_err(fail)
}

fn source(name: &str, value: &toml::Value) -> Option<String> {
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
    if value
        .get("version")
        .is_some_and(|version| version.as_str().is_none())
    {
        return None;
    }
    if package != PACKAGE || value.get("git").and_then(toml::Value::as_str) != Some(REPOSITORY) {
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
    Some(format!("git+{REPOSITORY}?rev={revision}"))
}

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

fn declared_owners(
    root: &toml::Value,
    declared: &BTreeMap<String, Vec<&toml::Value>>,
) -> Result<BTreeMap<String, String>> {
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

pub(crate) fn portable(
    workspace: &Path,
    owner: &toml::Value,
    declared: &BTreeMap<String, Vec<&toml::Value>>,
) -> Result<BTreeMap<String, String>> {
    let root = document(workspace, "Cargo.toml")?;
    if overrides(&root) || overrides(owner) {
        return Ok(BTreeMap::new());
    }
    let mut allowed = declared_owners(&root, declared)?;
    if allowed.is_empty() {
        return Ok(allowed);
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
    // A full revision must resolve to that exact object, never a different lock
    // object under a claimed immutable rev. Portable policy has no Cargo units.
    let lock = document(workspace, "Cargo.lock")?;
    let records = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| fail("Cargo.lock has no package records"))?;
    allowed.retain(|_, source| {
        let revision = source.rsplit_once('=').map_or("", |(_, revision)| revision);
        let expected = format!("{source}#{revision}");
        records
            .iter()
            .filter(|record| {
                record.get("name").and_then(toml::Value::as_str) == Some(PACKAGE)
                    && record
                        .get("version")
                        .and_then(toml::Value::as_str)
                        .is_some_and(|version| !version.is_empty())
                    && record.get("source").and_then(toml::Value::as_str) == Some(expected.as_str())
            })
            .count()
            == 1
    });
    Ok(allowed)
}

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
            let (source, name, _) = crate::cargo::package_parts(&dependency.package).ok()?;
            (dependency.manifest.is_none()
                && name == PACKAGE
                && proven.get(alias).is_some_and(|expected| source == expected))
            .then(|| alias.clone())
        })
        .collect())
}
