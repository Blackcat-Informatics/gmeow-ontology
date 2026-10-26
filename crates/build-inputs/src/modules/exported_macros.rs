// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Restricted local exports: one selected macro_export definition, either at its
//! root export or a direct public `use crate::name` in public modules. Resolution
//! waits until the ordinary traversal has scanned every definition and invocation.

use crate::{InputInventory, InputRole, Result, UnitSelection, checked_path, fail, relative};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Valid only within one collection. Equality includes every selected unit field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct OwnerId(usize);

#[derive(Clone)]
struct Invocation {
    owner: OwnerId,
    path: Vec<String>,
    external: bool,
}

#[derive(Default)]
pub(super) struct Exports {
    units: Vec<UnitSelection>,
    definitions: BTreeMap<(OwnerId, String), usize>,
    reexports: BTreeMap<(OwnerId, Vec<String>), usize>,
    shadows: BTreeSet<(OwnerId, String)>,
    scanned: BTreeSet<OwnerId>,
    pending: Vec<Invocation>,
    resolved: BTreeMap<(OwnerId, String), OwnerId>,
    cursor: usize,
}

impl Exports {
    fn intern(&mut self, unit: &UnitSelection) -> OwnerId {
        if let Some(index) = self.units.iter().position(|known| known == unit) {
            return OwnerId(index);
        }
        let id = OwnerId(self.units.len());
        self.units.push(unit.clone());
        id
    }

    pub fn begin_scan(&mut self, unit: &UnitSelection) -> Option<OwnerId> {
        let id = self.intern(unit);
        self.scanned.insert(id).then_some(id)
    }

    pub fn is_scanned(&self, id: OwnerId) -> bool {
        self.scanned.contains(&id)
    }

    pub fn unit(&self, id: OwnerId) -> &UnitSelection {
        &self.units[id.0]
    }

    pub fn definition(&mut self, owner: OwnerId, name: String) {
        *self.definitions.entry((owner, name)).or_default() += 1;
    }

    pub fn reexport(&mut self, owner: OwnerId, mut module: Vec<String>, name: String) {
        module.push(name);
        *self.reexports.entry((owner, module)).or_default() += 1;
    }

    pub fn shadow(&mut self, owner: OwnerId, binding: String) {
        self.shadows.insert((owner, binding));
    }

    pub fn defer(&mut self, owner: OwnerId, path: &syn::Path) -> bool {
        if path.segments.len() < 2 || path.segments.iter().any(|part| !part.arguments.is_empty()) {
            return false;
        }
        let parts: Vec<_> = path
            .segments
            .iter()
            .map(|part| part.ident.to_string())
            .collect();
        let external = path.leading_colon.is_some();
        if (!external && parts[0] != "crate") || (external && parts[0] == "crate") {
            return false;
        }
        self.pending.push(Invocation {
            owner,
            path: parts,
            external,
        });
        true
    }

    /// Return the next owner to traverse, not an accepted capability. `finish`
    /// checks every invocation only after all recursively needed sources pass.
    pub fn next_owner(
        &mut self,
        workspace: &Path,
        selected: Option<&[UnitSelection]>,
        inventory: &mut InputInventory,
    ) -> Result<Option<OwnerId>> {
        let Some(invocation) = self.pending.get(self.cursor).cloned() else {
            return Ok(None);
        };
        self.cursor += 1;
        let binding = (invocation.owner, invocation.path[0].clone());
        let id = match self.resolved.get(&binding) {
            Some(id) => *id,
            None => {
                let unit = resolve_owner(
                    workspace,
                    self.unit(invocation.owner),
                    &invocation,
                    selected,
                    inventory,
                )?;
                let id = self.intern(&unit);
                self.resolved.insert(binding, id);
                id
            }
        };
        if invocation.external
            && self
                .shadows
                .contains(&(invocation.owner, invocation.path[0].clone()))
        {
            return Err(fail(
                "extern crate alias shadows a selected local macro owner",
            ));
        }
        Ok(Some(id))
    }

    pub fn finish(&self) -> Result<()> {
        for invocation in &self.pending {
            let binding = (invocation.owner, invocation.path[0].clone());
            let key = *self
                .resolved
                .get(&binding)
                .ok_or_else(|| fail("exported macro owner was not scanned"))?;
            let name = invocation.path.last().unwrap();
            if !self.scanned.contains(&key)
                || self.definitions.get(&(key, name.clone())) != Some(&1)
                || (invocation.path.len() > 2
                    && self.reexports.get(&(key, invocation.path[1..].to_vec())) != Some(&1))
            {
                return Err(fail(format!(
                    "macro {}! has no unique selected local export or matching direct public reexport",
                    invocation.path.join("::")
                )));
            }
            if invocation.external
                && self
                    .shadows
                    .contains(&(invocation.owner, invocation.path[0].clone()))
            {
                return Err(fail(
                    "extern crate alias shadows a selected local macro owner",
                ));
            }
        }
        Ok(())
    }
}

fn document(workspace: &Path, path: &str, inventory: &mut InputInventory) -> Result<toml::Value> {
    inventory.add_file(workspace, path, "local-macro-selection", InputRole::Policy)?;
    std::fs::read_to_string(checked_path(workspace, Path::new(path))?)
        .map_err(fail)?
        .parse()
        .map_err(fail)
}

fn library_source(workspace: &Path, manifest: &str, value: &toml::Value) -> Result<String> {
    if value.get("lib").is_some_and(|lib| !lib.is_table()) {
        return Err(fail("local macro library declaration is not a table"));
    }
    if value.get("lib").and_then(|lib| lib.get("proc-macro")) == Some(&toml::Value::Boolean(true))
        || (value.get("lib").is_none()
            && value
                .get("package")
                .and_then(|package| package.get("autolib"))
                == Some(&toml::Value::Boolean(false)))
    {
        return Err(fail(
            "local exported macro requires a declared ordinary library",
        ));
    }
    let source = value.get("lib").and_then(|lib| lib.get("path"));
    let source = match source {
        Some(source) => source
            .as_str()
            .ok_or_else(|| fail("local macro library path is not a string"))?,
        None => "src/lib.rs",
    };
    let path = Path::new(manifest).parent().unwrap().join(source);
    relative(workspace, &checked_path(workspace, &path)?)
}

fn declared_owner(
    workspace: &Path,
    unit: &UnitSelection,
    alias: &str,
    inventory: &mut InputInventory,
) -> Result<(String, toml::Value)> {
    let manifest = unit
        .manifest
        .as_deref()
        .ok_or_else(|| fail("local macro caller has no manifest"))?;
    let own = document(workspace, manifest, inventory)?;
    let root = document(workspace, "Cargo.toml", inventory)?;
    let declared = crate::cargo::declared_dependencies(&own);
    let declarations: Vec<_> = declared
        .iter()
        .filter(|(name, _)| name.replace('-', "_") == alias)
        .flat_map(|(_, values)| values.iter().copied())
        .collect();
    if declarations.is_empty() {
        return Err(fail("local macro owner is not a declared dependency"));
    }
    let mut owners = BTreeSet::new();
    let mut package = None;
    for declaration in declarations {
        let inherited = declaration.get("workspace").and_then(toml::Value::as_bool) == Some(true);
        if inherited
            && declaration.as_table().is_none_or(|fields| {
                fields.keys().any(|key| {
                    !matches!(
                        key.as_str(),
                        "workspace" | "optional" | "features" | "default-features"
                    )
                })
            })
        {
            return Err(fail("unsupported inherited local macro declaration"));
        }
        let dependency = if inherited {
            let name = declared
                .keys()
                .find(|name| name.replace('-', "_") == alias)
                .unwrap();
            root.get("workspace")
                .and_then(|value| value.get("dependencies"))
                .and_then(|value| value.get(name))
                .ok_or_else(|| fail("missing inherited local macro owner"))?
        } else {
            declaration
        };
        if dependency.as_table().is_none_or(|fields| {
            fields.keys().any(|key| {
                !matches!(
                    key.as_str(),
                    "path" | "package" | "version" | "optional" | "features" | "default-features"
                )
            })
        }) {
            return Err(fail(
                "exported macro owner must be an unambiguous local path declaration",
            ));
        }
        let path = dependency
            .get("path")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| fail("exported macro owner has no declared local path"))?;
        let base = if inherited {
            Path::new("")
        } else {
            Path::new(manifest).parent().unwrap()
        };
        let target = checked_path(workspace, &base.join(path).join("Cargo.toml"))?;
        owners.insert(relative(workspace, &target)?);
        let name = match dependency.get("package") {
            Some(name) => name
                .as_str()
                .ok_or_else(|| fail("local macro package is not a string"))?
                .to_owned(),
            None => declared
                .keys()
                .find(|name| name.replace('-', "_") == alias)
                .unwrap()
                .clone(),
        };
        if package.as_ref().is_some_and(|previous| previous != &name) {
            return Err(fail("ambiguous local macro dependency package"));
        }
        package = Some(name);
    }
    if owners.len() != 1 {
        return Err(fail("ambiguous local macro dependency path"));
    }
    let target = owners.into_iter().next().unwrap();
    let value = document(workspace, &target, inventory)?;
    if value
        .get("package")
        .and_then(|value| value.get("name"))
        .and_then(toml::Value::as_str)
        != package.as_deref()
    {
        return Err(fail(
            "local macro dependency package differs from its manifest",
        ));
    }
    Ok((target, value))
}

fn selected_library(
    workspace: &Path,
    unit: &UnitSelection,
    manifest: &str,
    value: &toml::Value,
) -> Result<()> {
    let source = library_source(workspace, manifest, value)?;
    let (package_source, package, _) = crate::cargo::package_parts(&unit.package)?;
    if unit.manifest.as_deref() != Some(manifest)
        || unit.source.as_deref() != Some(source.as_str())
        || !unit
            .kinds
            .iter()
            .any(|kind| matches!(kind.as_str(), "lib" | "rlib"))
        || !package_source.starts_with("path+file://")
        || unit.edition != crate::cargo::source_edition(workspace, &source)?
        || value
            .get("package")
            .and_then(|value| value.get("name"))
            .and_then(toml::Value::as_str)
            != Some(package)
    {
        return Err(fail(format!(
            "selected local macro owner {} target {} ({:?}, source {:?}) differs from declared library {manifest}:{source}",
            unit.package, unit.target, unit.kinds, unit.source
        )));
    }
    Ok(())
}

fn resolve_owner(
    workspace: &Path,
    caller: &UnitSelection,
    invocation: &Invocation,
    selected: Option<&[UnitSelection]>,
    inventory: &mut InputInventory,
) -> Result<UnitSelection> {
    if !invocation.external {
        let manifest = caller
            .manifest
            .as_deref()
            .ok_or_else(|| fail("local macro has no crate owner"))?;
        let value = document(workspace, manifest, inventory)?;
        if selected.is_some() {
            selected_library(workspace, caller, manifest, &value)?;
        } else if caller.source.as_deref()
            != Some(library_source(workspace, manifest, &value)?.as_str())
        {
            return Err(fail(
                "portable local macro source differs from its declared library",
            ));
        }
        return Ok(caller.clone());
    }
    if !matches!(caller.edition.as_str(), "2018" | "2021" | "2024") {
        return Err(fail(
            "external local macro path requires the external prelude",
        ));
    }
    let alias = &invocation.path[0];
    let (manifest, declarations) = declared_owner(workspace, caller, alias, inventory)?;
    let source = library_source(workspace, &manifest, &declarations)?;
    if let Some(units) = selected {
        if caller.dependencies.len() != caller.dependency_names.len() {
            return Err(fail("selected macro dependency names and indices differ"));
        }
        let owners: Vec<_> = caller
            .dependency_names
            .iter()
            .zip(&caller.dependencies)
            .filter(|(name, _)| *name == alias)
            .collect();
        if owners.len() != 1 {
            return Err(fail("local macro has no unique selected dependency"));
        }
        let target = units
            .get(*owners[0].1)
            .ok_or_else(|| fail("dangling selected local macro owner"))?;
        selected_library(workspace, target, &manifest, &declarations)?;
        return Ok(target.clone());
    }
    // Portable source ownership has no Cargo unit graph. Only an exact declared
    // path library may supply it; replacements/config overrides are not resolved.
    for path in ["Cargo.toml", ".cargo/config", ".cargo/config.toml"] {
        if !workspace.join(path).is_file() {
            continue;
        }
        let policy = document(workspace, path, inventory)?;
        if ["patch", "replace", "source", "paths"]
            .iter()
            .any(|key| policy.get(key).is_some())
        {
            return Err(fail(
                "portable local macro ownership cannot resolve Cargo overrides",
            ));
        }
    }
    let mut target = caller.clone();
    target.package = format!("portable-local:{manifest}");
    target.manifest = Some(manifest);
    target.source = Some(source.clone());
    target.edition = crate::cargo::source_edition(workspace, &source)?;
    target.kinds = vec!["lib".into()];
    target.controller = false;
    target.dependencies.clear();
    target.dependency_names = crate::cargo::declared_dependencies(&declarations)
        .keys()
        .map(|name| name.replace('-', "_"))
        .collect();
    Ok(target)
}
