// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authenticated executable recipes. The builder owns policy admission; consumers
//! bind the admitted recipe to exact executable bytes without invoking a compiler.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{ActionCacheError, content_digest};

/// A resolved Cargo compilation unit, with physical checkout paths removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompilationUnit {
    /// Cargo package identifier with the selected checkout root replaced by `<workspace>`.
    pub package: String,
    /// Cargo target name for this compilation unit.
    pub target: String,
    /// Cargo target kinds, including library, binary, build-script, or procedural-macro roles.
    pub target_kinds: Vec<String>,
    /// Compilation mode reported by Cargo's resolved unit graph.
    pub mode: String,
    /// Optional target platform identifier reported by Cargo for this unit.
    pub platform: Option<String>,
    /// Features resolved by Cargo for this unit.
    pub features: Vec<String>,
    /// Complete resolved Cargo unit profile, retained in its original JSON structure.
    pub profile: serde_json::Value,
    /// Zero-based dependency indices into the containing [`ExecutableRecipe::units`].
    pub dependencies: Vec<usize>,
}

/// Complete portable identity of an admitted build. Actual runtime policy is
/// checked by the builder, before the recipe digest is supplied to rustc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutableRecipe {
    /// Recipe format version; the current receipt reader accepts version 2.
    pub schema: u32,
    /// Selected Cargo profile name, `pipeline` for an admitted producer.
    pub profile: String,
    /// Domain-separated SHA-256 of the exact typed production input inventory.
    pub source_digest: String,
    /// Typed pre-build source selection and its exact owned input inventory.
    pub source_inventory: gmeow_build_inputs::InputInventory,
    /// Trimmed `rustc -Vv` output identifying the selected Rust compiler.
    pub rustc: String,
    /// Trimmed `cargo --version` output identifying the selected Cargo executable.
    pub cargo: String,
    /// Builder-selected compiler settings and compiler, target, and CPU identity records.
    pub compiler_environment: BTreeMap<String, String>,
    /// Resolved compilation graph in Cargo's unit-index order.
    pub units: Vec<CompilationUnit>,
    /// Zero-based indices into [`Self::units`] for the selected executable roots.
    pub roots: Vec<usize>,
}

impl ExecutableRecipe {
    /// Domain-separated identity embedded by all producer artifact owners.
    pub fn digest(&self) -> Result<String, ActionCacheError> {
        Ok(content_digest(&[
            b"executable-recipe-v2",
            &serde_json::to_vec(self)?,
        ]))
    }

    /// Compilation policy for action owners that independently hash their exact
    /// implementation inputs. Executable authentication still uses [`Self::digest`].
    /// Excluding the executable-wide source digest here prevents CLI-only edits
    /// from invalidating products whose implementation and compiler are unchanged.
    pub fn compilation_digest(&self) -> Result<String, ActionCacheError> {
        Ok(content_digest(&[
            b"producer-compilation-policy-v2",
            &serde_json::to_vec(&(
                &self.profile,
                &self.rustc,
                &self.cargo,
                &self.compiler_environment,
                &self.units,
                &self.roots,
            ))?,
        ]))
    }
}

/// A builder-issued binding between an admitted recipe and the linked executable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutableReceipt {
    /// Receipt format version; the current reader accepts version 2.
    pub schema: u32,
    /// Complete admitted recipe whose digest is embedded in the producer executable.
    pub recipe: ExecutableRecipe,
    /// Lowercase SHA-256 of the exact linked executable bytes.
    pub executable_sha256: String,
    /// Freshness of the controller's actual Cargo resolution. Development-only
    /// changes may refresh this witness without changing the recipe or binary.
    pub resolution: gmeow_build_inputs::CargoResolutionEvidence,
}

/// Version classification available only to the explicit build controller. An
/// unsupported receipt is never accepted as executable or corpus evidence.
#[derive(Debug)]
pub enum ReceiptDocument {
    Current(ExecutableReceipt),
    Unsupported {
        receipt_schema: u32,
        recipe_schema: u32,
    },
}

impl ExecutableReceipt {
    /// Admit only the current receipt format. Read-only consumers never repair it.
    pub fn read(path: &Path) -> Result<Self, ActionCacheError> {
        match Self::read_versioned(path)? {
            ReceiptDocument::Current(receipt) => Ok(receipt),
            ReceiptDocument::Unsupported { .. } => Err(ActionCacheError::message(
                "unsupported executable receipt schema",
            )),
        }
    }
    /// Classify a bounded, structurally valid version header without accepting
    /// obsolete executable evidence. Only the builder may turn Unsupported into
    /// a fresh optimized build; malformed current documents remain errors.
    pub fn read_versioned(path: &Path) -> Result<ReceiptDocument, ActionCacheError> {
        const MAX_RECEIPT_BYTES: u64 = 8 * 1024 * 1024;
        let file = File::open(path)?;
        let mut bytes = Vec::new();
        file.take(MAX_RECEIPT_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_RECEIPT_BYTES {
            return Err(ActionCacheError::message(
                "executable receipt exceeds its bound",
            ));
        }
        #[derive(Deserialize)]
        struct Header {
            schema: u32,
            recipe: RecipeHeader,
        }
        #[derive(Deserialize)]
        struct RecipeHeader {
            schema: u32,
        }
        let header: Header = serde_json::from_slice(&bytes)?;
        if header.schema != 2 || header.recipe.schema != 2 {
            return Ok(ReceiptDocument::Unsupported {
                receipt_schema: header.schema,
                recipe_schema: header.recipe.schema,
            });
        }
        Ok(ReceiptDocument::Current(serde_json::from_slice(&bytes)?))
    }

    /// Authenticate bytes and the recipe identity embedded in the executable.
    /// This operation never builds, repairs, or produces a corpus.
    pub fn verify(
        &self,
        executable: &Path,
        embedded_recipe_digest: &str,
    ) -> Result<(), ActionCacheError> {
        if self.schema != 2 || self.recipe.schema != 2 {
            return Err(ActionCacheError::message(
                "unsupported executable receipt schema",
            ));
        }
        if self.recipe.source_inventory.schema != gmeow_build_inputs::SCHEMA {
            return Err(ActionCacheError::message(
                "unsupported source inventory schema",
            ));
        }
        self.recipe
            .source_inventory
            .selection
            .validate()
            .map_err(|error| ActionCacheError::message(error.to_string()))?;
        self.resolution
            .verify_selection(&self.recipe.source_inventory.selection)
            .map_err(|error| ActionCacheError::message(error.to_string()))?;
        if self
            .recipe
            .source_inventory
            .digest()
            .map_err(|error| ActionCacheError::message(error.to_string()))?
            != self.recipe.source_digest
        {
            return Err(ActionCacheError::message(
                "source digest does not bind the selected input inventory",
            ));
        }
        if self.recipe.digest()? != embedded_recipe_digest {
            return Err(ActionCacheError::message(
                "executable recipe differs from the compiled producer identity",
            ));
        }
        if sha256_file(executable)? != self.executable_sha256 {
            return Err(ActionCacheError::message(
                "producer executable digest does not match its receipt",
            ));
        }
        Ok(())
    }

    /// Require current Cargo-resolution evidence before source admission. This
    /// is read-only and never invokes Cargo or repairs stale evidence.
    pub fn verify_current_inputs(&self, root: &Path) -> Result<(), ActionCacheError> {
        self.resolution
            .verify_current(root, &self.recipe.source_inventory.selection)
            .and_then(|()| self.recipe.source_inventory.verify_current(root))
            .map_err(|error| ActionCacheError::message(error.to_string()))
    }

    /// Publish a complete receipt atomically beside its executable.
    pub fn write(&self, path: &Path) -> Result<(), ActionCacheError> {
        let parent = path.parent().ok_or_else(|| {
            ActionCacheError::message("executable receipt requires a parent directory")
        })?;
        std::fs::create_dir_all(parent)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&serde_json::to_vec_pretty(self)?)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(path)
            .map_err(|error| ActionCacheError::message(error.to_string()))?;
        Ok(())
    }
}

/// Admit the already-built producer and return one exact action owner's current
/// source inventory. Read-only consumers neither resolve Cargo nor construct data.
pub fn current_source_inventory(
    root: &Path,
    manifest: &str,
) -> Result<gmeow_build_inputs::InputInventory, ActionCacheError> {
    let executable = root.join("dist/bin/gmeow-dev");
    let receipt = ExecutableReceipt::read(&executable.with_extension("receipt.json"))?;
    if receipt.recipe.profile != "pipeline" {
        return Err(ActionCacheError::message(
            "source inventory requires the optimized producer profile",
        ));
    }
    let expected = receipt.recipe.digest()?;
    receipt.verify(&executable, &expected)?;
    let probe = std::process::Command::new(&executable)
        .arg("build-identity")
        .current_dir(root)
        .output()?;
    if !probe.status.success()
        || probe.stdout.len() > 128
        || String::from_utf8_lossy(&probe.stdout).trim() != expected
    {
        return Err(ActionCacheError::message(
            "producer executable does not embed the selected source recipe",
        ));
    }
    receipt.verify_current_inputs(root)?;
    let selected = receipt
        .recipe
        .source_inventory
        .selection
        .scoped_to_manifest(manifest)
        .map_err(|error| ActionCacheError::message(error.to_string()))?;
    gmeow_build_inputs::InputInventory::collect(root, &selected)
        .map_err(|error| ActionCacheError::message(error.to_string()))
}

/// Stream the executable digest without holding its bytes in memory.
pub fn sha256_file(path: &Path) -> Result<String, ActionCacheError> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

#[path = "executable.tests.rs"]
#[cfg(test)]
mod tests;
