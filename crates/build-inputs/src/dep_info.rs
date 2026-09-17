// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use crate::{CargoResolutionEvidence, InputInventory, InputRole, Result, fail, relative};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

/// A compiler artifact reported by Cargo for one selected workspace unit.
#[derive(Clone, Debug)]
pub struct CompilerArtifact {
    pub files: Vec<PathBuf>,
    pub source: PathBuf,
    pub build_script: bool,
    pub package: String,
}
/// A Cargo-reported generated directory and its selected package owner.
#[derive(Clone, Debug)]
pub struct GeneratedInputs {
    pub directory: PathBuf,
    pub package: String,
}

/// Actual compiler-read files collected after the admitted build. They check the
/// inventory's completeness; they never discover or redefine its source digest.
#[derive(Default, Debug)]
pub struct CompilerInputs {
    pub files: BTreeSet<PathBuf>,
    pub environments: BTreeMap<String, BTreeSet<String>>,
}
impl CompilerInputs {
    /// Bind dep-info to the artifacts Cargo actually reported, independent of
    /// Cargo's legacy/new build-directory layouts or guessed filename stems.
    pub fn from_artifacts(workspace: &Path, artifacts: &[CompilerArtifact]) -> Result<Self> {
        let directories: BTreeSet<_> = artifacts
            .iter()
            .flat_map(|artifact| artifact.files.iter())
            .map(|file| {
                file.parent()
                    .map(Path::to_path_buf)
                    .ok_or_else(|| fail("compiler artifact has no parent"))
            })
            .collect::<Result<_>>()?;
        let mut records = BTreeMap::new();
        for directory in directories {
            for entry in std::fs::read_dir(directory).map_err(fail)? {
                let path = entry.map_err(fail)?.path();
                if path.extension().is_some_and(|extension| extension == "d") {
                    let text = std::fs::read_to_string(&path).map_err(fail)?;
                    let (targets, inputs, environment) = parse(&text, workspace)?;
                    records.insert(path, (targets, inputs, environment));
                }
            }
        }
        let mut result = Self::default();
        for artifact in artifacts {
            let files: BTreeSet<_> = artifact
                .files
                .iter()
                .map(|path| normalize(path))
                .collect::<Result<_>>()?;
            let source = normalize(&artifact.source)?;
            let mut matched = Vec::new();
            for (dep_info, (targets, inputs, environment)) in &records {
                // Cargo's build-script executable can be an alias of rustc's
                // emitted binary. Its exact artifact directory and source root
                // identify that unit without relying on either filename spelling.
                let script_alias = artifact.build_script
                    && artifact
                        .files
                        .iter()
                        .any(|file| file.parent() == dep_info.parent())
                    && inputs.contains(&source);
                if !files.is_disjoint(targets) || script_alias {
                    matched.push((inputs, environment));
                }
            }
            if matched.is_empty() {
                return Err(fail(format!(
                    "Cargo artifact has no matching compiler dep-info: {:?}",
                    artifact.files
                )));
            }
            for (inputs, environment) in matched {
                result.files.extend(inputs.iter().cloned());
                result
                    .environments
                    .entry(artifact.package.clone())
                    .or_default()
                    .extend(environment.iter().cloned());
            }
        }
        Ok(result)
    }

    /// Read rustc's Make dep-info, including escaped spaces and line continuations.
    pub fn extend_dep_info(&mut self, path: &Path, workspace: &Path) -> Result<()> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| fail(format!("read compiler dep-info {}: {e}", path.display())))?;
        let (_, files, environment) = parse(&text, workspace)?;
        if !environment.is_empty() {
            return Err(fail(
                "environment-bearing compiler evidence requires an exact artifact package owner",
            ));
        }
        self.files.extend(files);
        Ok(())
    }

    /// Every workspace compiler read must be declared; generated outputs need
    /// their exact generator and Cargo package owner. Registry-owned generated
    /// sources are covered by the selected immutable package identity.
    pub fn verify(
        &self,
        workspace: &Path,
        inventory: &InputInventory,
        resolution: &CargoResolutionEvidence,
        external_roots: &[PathBuf],
        generated_roots: &[GeneratedInputs],
    ) -> Result<()> {
        if self.files.is_empty() {
            return Err(fail("optimized compilation produced no dep-info evidence"));
        }
        resolution.verify_selection(&inventory.selection)?;
        for (package, names) in &self.environments {
            let unit = inventory
                .selection
                .units
                .iter()
                .find(|unit| &unit.package == package && unit.manifest.is_some())
                .ok_or_else(|| fail("compiler environment has no selected workspace unit owner"))?;
            for name in names {
                crate::embedded::environment_owner(unit, name)?;
            }
        }
        let mut observed = BTreeSet::new();
        let mut undeclared = BTreeSet::new();
        for path in &self.files {
            let path = normalize(path)?;
            if let Ok(name) = relative(workspace, &path) {
                if inventory.files.contains_key(&name) || inventory.manifests.contains_key(&name) {
                    observed.insert(name);
                    continue;
                }
                if resolution.declares_compiler_input(&name) {
                    continue;
                }
                if path.is_dir() && inventory.declares_directory_observation(&name) {
                    continue;
                }
            }
            if let Some(generated) = generated_roots
                .iter()
                .find(|generated| path.starts_with(&generated.directory))
            {
                let owners: Vec<_> = inventory
                    .selection
                    .units
                    .iter()
                    .filter(|unit| unit.package == generated.package)
                    .collect();
                if owners.is_empty() {
                    return Err(fail(
                        "compiler generated input has no selected package owner",
                    ));
                }
                if owners.iter().all(|unit| unit.manifest.is_none()) {
                    continue;
                }
                let name = path
                    .strip_prefix(&generated.directory)
                    .map_err(fail)?
                    .to_str()
                    .ok_or_else(|| fail("generated source name is not UTF-8"))?;
                let generator = inventory.generated.get(name).ok_or_else(|| {
                    fail(format!("compiler read undeclared generated input {name}"))
                })?;
                if !owners.iter().any(|unit| {
                    unit.manifest.as_ref().is_some_and(|manifest| {
                        Path::new(manifest).parent().is_some_and(|directory| {
                            directory.join("build.rs") == Path::new(generator)
                        })
                    })
                }) {
                    return Err(fail(format!(
                        "generated source {name} belongs to a different build-script owner"
                    )));
                }
                continue;
            }
            if !external_roots.iter().any(|root| path.starts_with(root)) {
                undeclared.insert(path);
            }
        }
        if !undeclared.is_empty() {
            return Err(fail(format!(
                "compiler read undeclared inputs:\n{}",
                undeclared
                    .iter()
                    .map(|path| format!("- {}", path.display()))
                    .collect::<Vec<_>>()
                    .join("\n")
            )));
        }
        for (path, input) in &inventory.files {
            if input.roles.contains(&InputRole::Rust) && !observed.contains(path) {
                return Err(fail(format!(
                    "selected Rust implementation absent from compiler reads: {path}"
                )));
            }
        }
        Ok(())
    }
}

fn parse(
    text: &str,
    workspace: &Path,
) -> Result<(BTreeSet<PathBuf>, BTreeSet<PathBuf>, BTreeSet<String>)> {
    let mut targets = BTreeSet::new();
    let mut inputs = BTreeSet::new();
    let mut environment = BTreeSet::new();
    for line in text.replace("\\\n", "").lines() {
        if let Some(dependency) = line.strip_prefix("# env-dep:") {
            environment.insert(
                dependency
                    .split_once('=')
                    .map_or(dependency, |(name, _)| name)
                    .to_owned(),
            );
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        let mut escaped = false;
        let split = line.char_indices().find_map(|(offset, ch)| {
            if escaped {
                escaped = false;
                None
            } else if ch == '\\' {
                escaped = true;
                None
            } else if ch == ':' {
                Some(offset)
            } else {
                None
            }
        });
        let Some(split) = split else {
            if line.trim().is_empty() {
                continue;
            }
            return Err(fail("malformed compiler dep-info rule"));
        };
        targets.extend(words(&line[..split], workspace)?);
        inputs.extend(words(&line[split + 1..], workspace)?);
    }
    Ok((targets, inputs, environment))
}
fn words(text: &str, workspace: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    let mut word = String::new();
    let mut escaped = false;
    for ch in text.chars() {
        if escaped {
            word.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch.is_whitespace() {
            if !word.is_empty() {
                paths.push(normalize(&workspace.join(&word))?);
                word.clear();
            }
        } else {
            word.push(ch);
        }
    }
    if escaped {
        return Err(fail("unterminated dep-info path escape"));
    }
    if !word.is_empty() {
        paths.push(normalize(&workspace.join(word))?);
    }
    Ok(paths)
}
fn normalize(path: &Path) -> Result<PathBuf> {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    return Err(fail("compiler path escapes its filesystem root"));
                }
            }
            component => result.push(component.as_os_str()),
        }
    }
    Ok(result)
}
