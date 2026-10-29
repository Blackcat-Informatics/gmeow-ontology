// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

mod json;

use crate::{CfgContext, InputInventory, Result, SCHEMA, checked_path, digest, fail};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Cargo's actual resolved target, not an inferred lib.rs/main.rs convention.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitSelection {
    pub package: String,
    pub manifest: Option<String>,
    pub source: Option<String>,
    pub target: String,
    /// Edition selected by Cargo for this exact target.
    pub edition: String,
    pub kinds: Vec<String>,
    pub cfg: CfgContext,
    pub dependencies: Vec<usize>,
    pub dependency_names: Vec<String>,
    pub controller: bool,
}
/// Immutable build inputs selected before code generation. External package IDs
/// are bound to their exact lock records; local roots are traversed semantically.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductionSelection {
    pub schema: u32,
    pub units: Vec<UnitSelection>,
    pub roots: Vec<usize>,
    pub policy_files: Vec<String>,
}
impl ProductionSelection {
    pub fn validate(&self) -> Result<()> {
        if self.schema != SCHEMA || self.roots.is_empty() {
            return Err(fail("missing or unsupported production selection"));
        }
        for index in self.roots.iter().copied().chain(
            self.units
                .iter()
                .flat_map(|unit| unit.dependencies.iter().copied()),
        ) {
            if index >= self.units.len() {
                return Err(fail("dangling selected Cargo unit"));
            }
        }
        for unit in &self.units {
            if !matches!(unit.edition.as_str(), "2015" | "2018" | "2021" | "2024") {
                return Err(fail(format!(
                    "unsupported selected Rust edition {}",
                    unit.edition
                )));
            }
            if unit.manifest.is_some() != unit.source.is_some() {
                return Err(fail(
                    "local source root and Cargo manifest must have the same owner",
                ));
            }
            if unit.cfg.flags.contains("test") || unit.cfg.flags.contains("doc") {
                return Err(fail("test/doc units cannot select production sources"));
            }
        }
        Ok(())
    }
    /// Keep exactly this action owner's already-resolved units and dependencies.
    /// Nothing resolves features or invokes Cargo at runtime/build-script admission.
    pub fn scoped_to_manifest(&self, manifest: &str) -> Result<Self> {
        let roots: Vec<_> = self
            .units
            .iter()
            .enumerate()
            .filter(|(_, unit)| {
                unit.manifest.as_deref() == Some(manifest)
                    && !unit.kinds.iter().any(|kind| kind == "custom-build")
                    && !unit.controller
            })
            .map(|(index, _)| index)
            .collect();
        if roots.is_empty() {
            return Err(fail(format!("no selected action owner {manifest}")));
        }
        let mut retained = BTreeSet::new();
        let mut pending = roots.clone();
        while let Some(index) = pending.pop() {
            if retained.insert(index) {
                pending.extend(self.units[index].dependencies.iter().copied());
            }
        }
        let remap: BTreeMap<_, _> = retained
            .iter()
            .enumerate()
            .map(|(new, old)| (*old, new))
            .collect();
        let units = retained
            .iter()
            .map(|index| {
                let mut unit = self.units[*index].clone();
                unit.dependencies = unit.dependencies.iter().map(|index| remap[index]).collect();
                unit
            })
            .collect();
        Ok(Self {
            schema: SCHEMA,
            units,
            roots: roots.iter().map(|index| remap[index]).collect(),
            policy_files: self.policy_files.clone(),
        })
    }
}

pub(crate) fn collect(workspace: &Path, inventory: &mut InputInventory) -> Result<()> {
    let profiles = production_profiles(workspace)?;
    let manifests: BTreeSet<_> = inventory
        .selection
        .units
        .iter()
        .filter_map(|unit| unit.manifest.clone())
        .chain(std::iter::once("Cargo.toml".to_owned()))
        .collect();
    let mut inherited = BTreeSet::new();
    for manifest in manifests
        .iter()
        .filter(|manifest| manifest.as_str() != "Cargo.toml")
    {
        let value: toml::Value =
            std::fs::read_to_string(checked_path(workspace, Path::new(manifest))?)
                .map_err(fail)?
                .parse()
                .map_err(fail)?;
        let names: BTreeSet<_> = inventory
            .selection
            .units
            .iter()
            .filter(|unit| unit.manifest.as_deref() == Some(manifest.as_str()))
            .flat_map(|unit| unit.dependency_names.iter().cloned())
            .collect();
        selected_inherited(&value, &names, &mut inherited);
    }
    for manifest in manifests {
        let path = checked_path(workspace, Path::new(&manifest))?;
        let mut value: toml::Value = std::fs::read_to_string(path)
            .map_err(fail)?
            .parse()
            .map_err(fail)?;
        let features: BTreeSet<String> = inventory
            .selection
            .units
            .iter()
            .filter(|unit| unit.manifest.as_deref() == Some(manifest.as_str()))
            .flat_map(|unit| {
                unit.cfg
                    .values
                    .get("feature")
                    .into_iter()
                    .flatten()
                    .cloned()
            })
            .collect();
        production_manifest(
            &mut value,
            &inherited,
            &features,
            &profiles,
            inventory.selection.units.iter().any(|unit| unit.controller),
        )?;
        inventory
            .manifests
            .insert(manifest, digest(&serde_json::to_vec(&value).map_err(fail)?));
    }
    let lock: toml::Value =
        std::fs::read_to_string(checked_path(workspace, Path::new("Cargo.lock"))?)
            .map_err(fail)?
            .parse()
            .map_err(fail)?;
    let records = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| fail("Cargo.lock has no package records"))?;
    for unit in &inventory.selection.units {
        if unit.manifest.is_some() {
            continue;
        }
        let (source, name, version) = package_parts(&unit.package)?;
        let matching: Vec<_> = records
            .iter()
            .filter(|record| {
                record.get("name").and_then(toml::Value::as_str) == Some(name)
                    && record.get("version").and_then(toml::Value::as_str) == Some(version)
                    && record
                        .get("source")
                        .and_then(toml::Value::as_str)
                        .is_some_and(|record_source| {
                            record_source == source
                                || (source.starts_with("git+")
                                    && record_source
                                        .rsplit_once('#')
                                        .is_some_and(|(base, _)| base == source))
                        })
            })
            .collect();
        if matching.len() != 1 {
            return Err(fail(format!(
                "selected package has no unique lock identity: {}",
                unit.package
            )));
        }
        let record = matching[0];
        let mut selected = BTreeMap::new();
        for key in ["name", "version", "source", "checksum"] {
            if let Some(value) = record.get(key) {
                selected.insert(key, value);
            }
        }
        if source.starts_with("registry+") && !selected.contains_key("checksum") {
            return Err(fail("selected registry package has no checksum"));
        }
        inventory.packages.insert(
            unit.package.clone(),
            digest(&serde_json::to_vec(&selected).map_err(fail)?),
        );
    }
    Ok(())
}

/// Cargo manifests are data, not Rust source: project away only development
/// declarations while retaining production dependency/configuration fields.
fn production_manifest(
    value: &mut toml::Value,
    inherited: &BTreeSet<String>,
    features: &BTreeSet<String>,
    profile_names: &BTreeSet<String>,
    controller: bool,
) -> Result<()> {
    let table = value
        .as_table_mut()
        .ok_or_else(|| fail("Cargo manifest is not a table"))?;
    for key in ["dev-dependencies", "test", "bench", "example"] {
        table.remove(key);
    }
    if let Some(targets) = table.get_mut("target").and_then(toml::Value::as_table_mut) {
        for (_, target) in targets.iter_mut() {
            if let Some(target) = target.as_table_mut() {
                target.remove("dev-dependencies");
            }
        }
    }
    if let Some(profiles) = table.get_mut("profile").and_then(toml::Value::as_table_mut) {
        select_profiles(profiles, profile_names, controller);
    }
    // The resolved selected graph is the authority for workspace membership;
    // unrelated members cannot become compilation inputs through this projection.
    if let Some(workspace) = table
        .get_mut("workspace")
        .and_then(toml::Value::as_table_mut)
    {
        for key in ["members", "default-members", "exclude"] {
            workspace.remove(key);
        }
        if let Some(dependencies) = workspace
            .get_mut("dependencies")
            .and_then(toml::Value::as_table_mut)
        {
            dependencies.retain(|name, _| inherited.contains(name));
        }
    }
    if let Some(selected) = table
        .get_mut("features")
        .and_then(toml::Value::as_table_mut)
    {
        selected.retain(|name, _| name == "default" || features.contains(name));
    }
    prune_empty_tables(value);
    Ok(())
}
// Controller execution uses Cargo's dev profile. Only settings that can select
// different Rust cfg branches belong to its source contract; optimization/debug
// information choices for that separate tool never rebuild the corpus producer.
fn select_profiles(
    profiles: &mut toml::map::Map<String, toml::Value>,
    selected: &BTreeSet<String>,
    controller: bool,
) {
    profiles.retain(|name, _| selected.contains(name) || (controller && name == "dev"));
    if controller
        && !selected.contains("dev")
        && let Some(dev) = profiles.get_mut("dev")
    {
        retain_controller_cfg(dev);
    }
}
fn retain_controller_cfg(value: &mut toml::Value) {
    if let Some(table) = value.as_table_mut() {
        for (_, child) in table.iter_mut() {
            if child.is_table() {
                retain_controller_cfg(child);
            }
        }
        table.retain(|name, child| {
            matches!(name, "debug-assertions" | "overflow-checks" | "panic")
                || child.as_table().is_some_and(|table| !table.is_empty())
        });
    }
}

fn prune_empty_tables(value: &mut toml::Value) {
    if let Some(table) = value.as_table_mut() {
        for (_, value) in table.iter_mut() {
            prune_empty_tables(value);
        }
        table.retain(|_, value| !value.as_table().is_some_and(|table| table.is_empty()));
    }
}
pub(crate) fn package_parts(id: &str) -> Result<(&str, &str, &str)> {
    let (source, package) = id
        .rsplit_once('#')
        .ok_or_else(|| fail(format!("invalid Cargo package identity {id}")))?;
    let (name, version) = package
        .rsplit_once('@')
        .ok_or_else(|| fail(format!("Cargo external package has no name/version {id}")))?;
    Ok((source, name, version))
}

fn selected_inherited(
    value: &toml::Value,
    names: &BTreeSet<String>,
    inherited: &mut BTreeSet<String>,
) {
    for key in ["dependencies", "build-dependencies"] {
        if let Some(dependencies) = value.get(key).and_then(toml::Value::as_table) {
            for (name, value) in dependencies {
                if names.contains(&name.replace('-', "_"))
                    && value.get("workspace").and_then(toml::Value::as_bool) == Some(true)
                {
                    inherited.insert(name.clone());
                }
            }
        }
    }
    if let Some(targets) = value.get("target").and_then(toml::Value::as_table) {
        for value in targets.values() {
            selected_inherited(value, names, inherited);
        }
    }
}

/// Retain every declaration for portable policy, including conflicting target
/// branches. Manifest keys alone do not establish a macro's package or source.
pub(crate) fn declared_dependencies(value: &toml::Value) -> BTreeMap<String, Vec<&toml::Value>> {
    let mut declared: BTreeMap<String, Vec<&toml::Value>> = BTreeMap::new();
    for key in ["dependencies", "build-dependencies"] {
        if let Some(dependencies) = value.get(key).and_then(toml::Value::as_table) {
            for (name, dependency) in dependencies {
                declared.entry(name.clone()).or_default().push(dependency);
            }
        }
    }
    if let Some(targets) = value.get("target").and_then(toml::Value::as_table) {
        for target in targets.values() {
            for (name, declarations) in declared_dependencies(target) {
                declared.entry(name).or_default().extend(declarations);
            }
        }
    }
    declared
}

fn dependency_package<'a>(name: &'a str, dependency: &'a toml::Value) -> Option<&'a str> {
    match dependency.get("package") {
        Some(package) => package.as_str(),
        None => Some(name),
    }
}

fn registry_tracing(name: &str, dependency: &toml::Value) -> bool {
    dependency_package(name, dependency) == Some("tracing")
        && dependency
            .as_str()
            .or_else(|| dependency.get("version").and_then(toml::Value::as_str))
            .is_some_and(|version| !version.trim().is_empty())
        && [
            "path",
            "git",
            "rev",
            "branch",
            "tag",
            "registry-index",
            "workspace",
        ]
        .iter()
        .all(|key| dependency.get(key).is_none())
        && match dependency.get("registry") {
            None => true,
            Some(registry) => registry.as_str() == Some("crates-io"),
        }
}

fn overrides_tracing(value: &toml::Value) -> bool {
    // Portable policy has no selected package IDs for deprecated replacements.
    value
        .get("replace")
        .and_then(toml::Value::as_table)
        .is_some_and(|replacements| !replacements.is_empty())
        || value
            .get("patch")
            .and_then(toml::Value::as_table)
            .into_iter()
            .flat_map(|sources| sources.values())
            .filter_map(toml::Value::as_table)
            .flat_map(|packages| packages.iter())
            .any(|(name, dependency)| dependency_package(name, dependency) == Some("tracing"))
}

pub(crate) fn portable_tracing_names(
    workspace: &Path,
    owner: &toml::Value,
    declared: &BTreeMap<String, Vec<&toml::Value>>,
) -> Result<BTreeSet<String>> {
    if declared.is_empty() {
        return Ok(BTreeSet::new());
    }
    let root_path = workspace.join("Cargo.toml");
    let root: Option<toml::Value> = if root_path.is_file() {
        Some(
            std::fs::read_to_string(checked_path(workspace, Path::new("Cargo.toml"))?)
                .map_err(fail)?
                .parse()
                .map_err(fail)?,
        )
    } else {
        None
    };
    let mut allowed: BTreeMap<String, bool> = BTreeMap::new();
    for (name, declarations) in declared {
        let mut admitted = true;
        for declaration in declarations {
            let dependency =
                if declaration.get("workspace").and_then(toml::Value::as_bool) == Some(true) {
                    if declaration.as_table().is_none_or(|fields| {
                        fields.keys().any(|key| {
                            !matches!(
                                key.as_str(),
                                "workspace" | "optional" | "features" | "default-features"
                            )
                        })
                    }) {
                        admitted = false;
                        break;
                    }
                    root.as_ref()
                        .and_then(|value| value.get("workspace"))
                        .and_then(|value| value.get("dependencies"))
                        .and_then(|value| value.get(name))
                        .ok_or_else(|| fail(format!("missing workspace dependency {name}")))?
                } else {
                    declaration
                };
            admitted &= registry_tracing(name, dependency);
        }
        allowed
            .entry(name.replace('-', "_"))
            .and_modify(|previous| *previous &= admitted)
            .or_insert(admitted);
    }
    let names: BTreeSet<_> = allowed
        .into_iter()
        .filter_map(|(name, admitted)| admitted.then_some(name))
        .collect();
    if names.is_empty() || overrides_tracing(root.as_ref().unwrap_or(owner)) {
        return Ok(BTreeSet::new());
    }
    for config in [".cargo/config.toml", ".cargo/config"] {
        if !workspace.join(config).is_file() {
            continue;
        }
        let value: toml::Value =
            std::fs::read_to_string(checked_path(workspace, Path::new(config))?)
                .map_err(fail)?
                .parse()
                .map_err(fail)?;
        if overrides_tracing(&value)
            || value
                .get("paths")
                .is_some_and(|paths| paths.as_array().is_none_or(|paths| !paths.is_empty()))
            || value
                .get("source")
                .and_then(|value| value.get("crates-io"))
                .is_some()
            || value
                .get("registries")
                .and_then(|value| value.get("crates-io"))
                .is_some()
        {
            return Ok(BTreeSet::new());
        }
    }
    Ok(names)
}

/// Inspect declared local dependency ownership for structural controls. This is
/// deliberately not a build selection: only Cargo's resolved units may populate
/// a production inventory. Development declarations never enter this census.
pub fn declared_path_dependency_manifests(
    workspace: &Path,
    root: &str,
) -> Result<BTreeSet<String>> {
    let workspace_manifest: toml::Value =
        std::fs::read_to_string(checked_path(workspace, Path::new("Cargo.toml"))?)
            .map_err(fail)?
            .parse()
            .map_err(fail)?;
    let mut seen = BTreeSet::new();
    let mut pending = vec![root.to_owned()];
    while let Some(manifest) = pending.pop() {
        if !seen.insert(manifest.clone()) {
            continue;
        }
        let path = checked_path(workspace, Path::new(&manifest))?;
        let value: toml::Value = std::fs::read_to_string(&path)
            .map_err(fail)?
            .parse()
            .map_err(fail)?;
        let mut tables = vec![&value];
        if let Some(targets) = value.get("target").and_then(toml::Value::as_table) {
            tables.extend(targets.values());
        }
        for table in tables {
            for key in ["dependencies", "build-dependencies"] {
                if let Some(dependencies) = table.get(key).and_then(toml::Value::as_table) {
                    for (name, dependency) in dependencies {
                        let (dependency, base) =
                            if dependency.get("workspace").and_then(toml::Value::as_bool)
                                == Some(true)
                            {
                                (
                                    workspace_manifest
                                        .get("workspace")
                                        .and_then(|value| value.get("dependencies"))
                                        .and_then(|value| value.get(name))
                                        .ok_or_else(|| {
                                            fail(format!("missing workspace dependency {name}"))
                                        })?,
                                    workspace,
                                )
                            } else {
                                (dependency, path.parent().unwrap())
                            };
                        if let Some(relative) = dependency.get("path").and_then(toml::Value::as_str)
                        {
                            let target = base.join(relative).join("Cargo.toml");
                            let target = checked_path(
                                workspace,
                                target.strip_prefix(workspace).map_err(fail)?,
                            )?;
                            pending.push(crate::relative(workspace, &target)?);
                        }
                    }
                }
            }
        }
    }
    Ok(seen)
}

/// Cargo configuration is typed data as well: developer aliases and development
/// profiles do not select production code. Remaining policy changes are bound.
pub(crate) fn configuration(
    workspace: &Path,
    path: &str,
    inventory: &mut InputInventory,
) -> Result<()> {
    let mut value: toml::Value = std::fs::read_to_string(checked_path(workspace, Path::new(path))?)
        .map_err(fail)?
        .parse()
        .map_err(fail)?;
    let table = value
        .as_table_mut()
        .ok_or_else(|| fail("Cargo config is not a table"))?;
    table.remove("alias");
    let selected_profiles = production_profiles(workspace)?;
    if let Some(profiles) = table.get_mut("profile").and_then(toml::Value::as_table_mut) {
        select_profiles(
            profiles,
            &selected_profiles,
            inventory.selection.units.iter().any(|unit| unit.controller),
        );
    }
    prune_empty_tables(&mut value);
    inventory.manifests.insert(
        path.to_owned(),
        digest(&serde_json::to_vec(&value).map_err(fail)?),
    );
    Ok(())
}

/// Production profile inheritance is part of typed Cargo ownership. A `dev`
/// profile ceases to be development-only if the admitted pipeline inherits it.
fn production_profiles(workspace: &Path) -> Result<BTreeSet<String>> {
    let mut inherits = BTreeMap::new();
    for path in ["Cargo.toml", ".cargo/config", ".cargo/config.toml"] {
        if !workspace.join(path).try_exists().map_err(fail)? {
            continue;
        }
        let value: toml::Value = std::fs::read_to_string(checked_path(workspace, Path::new(path))?)
            .map_err(fail)?
            .parse()
            .map_err(fail)?;
        if let Some(profiles) = value.get("profile").and_then(toml::Value::as_table) {
            for (name, value) in profiles {
                if let Some(parent) = value.get("inherits").and_then(toml::Value::as_str) {
                    inherits.insert(name.clone(), parent.to_owned());
                }
            }
        }
    }
    let mut selected: BTreeSet<_> = ["pipeline".to_owned(), "release".to_owned()]
        .into_iter()
        .collect();
    let mut next = inherits.get("pipeline").cloned();
    while let Some(name) = next {
        if !selected.insert(name.clone()) {
            break;
        }
        next = inherits.get(&name).cloned();
    }
    Ok(selected)
}

/// The fixed, explicit Cargo/toolchain policy inputs owned by this workspace.
/// Their presence is part of admission, so adding a previously absent config
/// cannot be hidden by an older receipt's selected path list.
pub fn cargo_policy_files(workspace: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();
    for path in [
        "rust-toolchain",
        "rust-toolchain.toml",
        ".cargo/config",
        ".cargo/config.toml",
    ] {
        if workspace.join(path).try_exists().map_err(fail)? {
            checked_path(workspace, Path::new(path))?;
            files.push(path.to_owned());
        }
    }
    Ok(files)
}

/// Resolve the manifest-owned edition for a portable source-policy or extraction traversal.
pub fn source_edition(workspace: &Path, source: &str) -> Result<String> {
    let mut directory = workspace
        .join(source)
        .parent()
        .ok_or_else(|| fail("source has no parent"))?
        .to_path_buf();
    loop {
        let manifest = directory.join("Cargo.toml");
        if manifest.try_exists().map_err(fail)? {
            let package: toml::Value = std::fs::read_to_string(&manifest)
                .map_err(fail)?
                .parse()
                .map_err(fail)?;
            let edition = package
                .get("package")
                .and_then(|package| package.get("edition"));
            if let Some(edition) = edition.and_then(toml::Value::as_str) {
                return Ok(edition.to_owned());
            }
            if edition
                .and_then(|edition| edition.get("workspace"))
                .and_then(toml::Value::as_bool)
                == Some(true)
            {
                let root: toml::Value = std::fs::read_to_string(workspace.join("Cargo.toml"))
                    .map_err(fail)?
                    .parse()
                    .map_err(fail)?;
                return root
                    .get("workspace")
                    .and_then(|workspace| workspace.get("package"))
                    .and_then(|package| package.get("edition"))
                    .and_then(toml::Value::as_str)
                    .map(str::to_owned)
                    .ok_or_else(|| fail("missing workspace Rust edition"));
            }
            if package.get("package").is_some() {
                return Ok("2015".into());
            }
        }
        if directory == workspace || !directory.pop() {
            return Err(fail("source has no Cargo edition owner"));
        }
    }
}
