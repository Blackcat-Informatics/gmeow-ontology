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

use crate::{ActionCacheError, bytes_digest, content_digest};

/// A resolved Cargo compilation unit, with physical checkout paths removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompilationUnit {
    pub package: String,
    pub target: String,
    pub target_kinds: Vec<String>,
    pub mode: String,
    pub platform: Option<String>,
    pub features: Vec<String>,
    pub profile: serde_json::Value,
    pub dependencies: Vec<usize>,
}

/// Complete portable identity of an admitted build. Actual runtime policy is
/// checked by the builder, before the recipe digest is supplied to rustc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutableRecipe {
    pub schema: u32,
    pub profile: String,
    pub source_digest: String,
    pub rustc: String,
    pub cargo: String,
    pub compiler_environment: BTreeMap<String, String>,
    pub units: Vec<CompilationUnit>,
    pub roots: Vec<usize>,
}

impl ExecutableRecipe {
    /// Domain-separated identity embedded by all producer artifact owners.
    pub fn digest(&self) -> Result<String, ActionCacheError> {
        Ok(content_digest(&[
            b"executable-recipe-v1",
            &serde_json::to_vec(self)?,
        ]))
    }

    /// Compilation policy for action owners that independently hash their exact
    /// implementation inputs. Executable authentication still uses [`Self::digest`].
    /// Excluding the executable-wide source digest here prevents CLI-only edits
    /// from invalidating products whose implementation and compiler are unchanged.
    pub fn compilation_digest(&self) -> Result<String, ActionCacheError> {
        let mut compilation = self.clone();
        compilation.source_digest.clear();
        Ok(content_digest(&[
            b"producer-compilation-policy-v1",
            &serde_json::to_vec(&compilation)?,
        ]))
    }
}

/// A builder-issued binding between an admitted recipe and the linked executable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutableReceipt {
    pub schema: u32,
    pub recipe: ExecutableRecipe,
    pub executable_sha256: String,
}

impl ExecutableReceipt {
    /// Read an existing receipt only. A missing or oversized receipt is an error.
    pub fn read(path: &Path) -> Result<Self, ActionCacheError> {
        const MAX_RECEIPT_BYTES: u64 = 8 * 1024 * 1024;
        let file = File::open(path)?;
        let mut bytes = Vec::new();
        file.take(MAX_RECEIPT_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_RECEIPT_BYTES {
            return Err(ActionCacheError::message(
                "executable receipt exceeds its bound",
            ));
        }
        let receipt: Self = serde_json::from_slice(&bytes)?;
        if receipt.schema != 1 || receipt.recipe.schema != 1 {
            return Err(ActionCacheError::message(
                "unsupported executable receipt schema",
            ));
        }
        Ok(receipt)
    }

    /// Authenticate bytes and the recipe identity embedded in the executable.
    /// This operation never builds, repairs, or produces a corpus.
    pub fn verify(
        &self,
        executable: &Path,
        embedded_recipe_digest: &str,
    ) -> Result<(), ActionCacheError> {
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

/// Fold a sorted source inventory without relying on timestamps or absolute paths.
pub fn source_digest(
    root: &Path,
    inputs: impl IntoIterator<Item = std::path::PathBuf>,
) -> Result<String, ActionCacheError> {
    let mut records = BTreeMap::new();
    for path in inputs {
        let relative = path.strip_prefix(root).map_err(|_| {
            ActionCacheError::message("producer source inventory escaped its workspace")
        })?;
        let relative = relative
            .to_str()
            .ok_or_else(|| ActionCacheError::message("producer source path is not UTF-8"))?;
        records.insert(relative.to_owned(), sha256_file(&path)?);
    }
    Ok(bytes_digest(&serde_json::to_vec(&records)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipe() -> ExecutableRecipe {
        ExecutableRecipe {
            schema: 1,
            profile: "pipeline".into(),
            source_digest: "source".into(),
            rustc: "compiler".into(),
            cargo: "cargo".into(),
            compiler_environment: BTreeMap::new(),
            units: Vec::new(),
            roots: Vec::new(),
        }
    }

    #[test]
    fn executable_and_recipe_substitution_are_rejected() {
        let scratch = tempfile::tempdir().expect("scratch");
        let binary = scratch.path().join("producer");
        std::fs::write(&binary, b"linked executable").expect("write");
        let receipt = ExecutableReceipt {
            schema: 1,
            recipe: recipe(),
            executable_sha256: sha256_file(&binary).expect("digest"),
        };
        let digest = receipt.recipe.digest().expect("recipe");
        receipt.verify(&binary, &digest).expect("authentic");
        let mut changed = receipt.clone();
        changed.recipe.profile = "test".into();
        assert!(changed.verify(&binary, &digest).is_err());
        std::fs::write(&binary, b"substituted executable").expect("replace");
        assert!(receipt.verify(&binary, &digest).is_err());
    }

    #[test]
    fn compilation_policy_and_executable_source_have_separate_identities() {
        let original = recipe();
        let mut cli_edit = original.clone();
        cli_edit.source_digest = "changed-cli-source".into();
        assert_ne!(original.digest().unwrap(), cli_edit.digest().unwrap());
        assert_eq!(
            original.compilation_digest().unwrap(),
            cli_edit.compilation_digest().unwrap()
        );
        let changes: [fn(&mut ExecutableRecipe); 3] = [
            |recipe: &mut ExecutableRecipe| recipe.rustc.push_str("changed compiler"),
            |recipe: &mut ExecutableRecipe| recipe.profile = "test".into(),
            |recipe: &mut ExecutableRecipe| {
                recipe
                    .compiler_environment
                    .insert("CFLAGS".into(), "-O2".into());
            },
        ];
        for change in changes {
            let mut changed = original.clone();
            change(&mut changed);
            assert_ne!(
                original.compilation_digest().unwrap(),
                changed.compilation_digest().unwrap()
            );
        }
    }

    #[test]
    fn source_inventory_detects_new_and_changed_files_but_not_location() {
        let left = tempfile::tempdir().expect("left");
        let right = tempfile::tempdir().expect("right");
        for root in [left.path(), right.path()] {
            std::fs::write(root.join("input"), b"source").expect("write");
        }
        let baseline = source_digest(left.path(), [left.path().join("input")]).expect("digest");
        assert_eq!(
            baseline,
            source_digest(right.path(), [right.path().join("input")]).expect("relocation")
        );
        std::fs::write(left.path().join("new"), b"new source").expect("new");
        assert_ne!(
            baseline,
            source_digest(
                left.path(),
                [left.path().join("input"), left.path().join("new")]
            )
            .expect("new inventory")
        );
        std::fs::write(right.path().join("input"), b"changed").expect("change");
        assert_ne!(
            baseline,
            source_digest(right.path(), [right.path().join("input")]).expect("changed inventory")
        );
    }
}
