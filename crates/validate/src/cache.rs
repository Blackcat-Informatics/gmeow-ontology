// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete validation verdicts in the shared authenticated action store.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_action_cache::{
    ActionContext, ActionInput, ActionStore, FileKind, ProducerIdentity, STORE_FORMAT_VERSION,
    StoreLimits, bytes_digest, content_digest,
};
use gmeow_errors::{Diag, Finding};
use serde::{Deserialize, Serialize};

/// A cached validation result: the structured findings produced by one cached
/// phase. Serialized as `{"findings": [...]}`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CachedResult {
    /// The structured findings for this phase.
    pub findings: Vec<Finding>,
}

impl CachedResult {
    /// Build a cached result from a slice of findings.
    pub fn from_findings(findings: Vec<Finding>) -> Self {
        Self { findings }
    }

    /// Merge another cached result into this one.
    pub fn extend(&mut self, other: CachedResult) {
        self.findings.extend(other.findings);
    }
}

const CODEC: &str = "gmeow-validation-findings-v2";

/// Repository-local validation cache with explicit implementation authority.
#[derive(Clone)]
pub struct ValidationCache {
    project_root: PathBuf,
    implementation: ProducerIdentity,
    store: Arc<ActionStore>,
}

impl std::fmt::Debug for ValidationCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidationCache")
            .field("project_root", &self.project_root)
            .field("implementation", &self.implementation)
            .finish_non_exhaustive()
    }
}

fn cache_error(error: impl std::fmt::Display) -> Diag {
    Diag::of_kind(crate::error::Io {
        detail: format!("validation action cache: {error}"),
    })
}

impl ValidationCache {
    /// Select repository caching under an exact, caller-admitted implementation.
    /// The caller owns producer admission; package versions alone are insufficient.
    pub fn new(
        project_root: impl AsRef<Path>,
        implementation: ProducerIdentity,
    ) -> gmeow_errors::Result<Self> {
        if implementation.digest.len() != 64
            || !implementation.digest.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(cache_error(
                "implementation requires a complete SHA-256 identity",
            ));
        }
        let project_root = project_root.as_ref().canonicalize().map_err(cache_error)?;
        if project_root.to_str().is_none() {
            return Err(cache_error("repository path is not UTF-8"));
        }
        let store = ActionStore::open(
            ActionStore::default_root(&project_root),
            STORE_FORMAT_VERSION,
            StoreLimits::default(),
        )
        .map_err(cache_error)?;
        Ok(Self {
            project_root,
            implementation,
            store: Arc::new(store),
        })
    }

    /// Shared store root. Validation does not own a separate quota or JSON tree.
    pub fn cache_dir(&self) -> PathBuf {
        self.store.root().to_path_buf()
    }

    /// Complete, length-framed content identity; embedded separators cannot alias.
    pub fn cache_key(parts: &[&[u8]]) -> String {
        content_digest(parts)
    }

    /// Hash the ordered source selection, including diagnostic paths. Missing inputs fail.
    pub fn files_cache_key(&self, paths: &[PathBuf]) -> gmeow_errors::Result<String> {
        Self::files_cache_key_with_root(paths, &self.project_root)
    }

    /// The same input identity for callers that have not selected a cache store.
    pub fn files_cache_key_with_root(
        paths: &[PathBuf],
        root: &Path,
    ) -> gmeow_errors::Result<String> {
        let mut inputs = Vec::with_capacity(paths.len());
        for path in paths {
            // Preserve the caller's spelling: DSL findings contain this exact path.
            let selected = path
                .to_str()
                .ok_or_else(|| cache_error("selected path is not UTF-8"))?;
            let absolute = if path.is_absolute() {
                path.clone()
            } else {
                std::env::current_dir().map_err(cache_error)?.join(path)
            };
            let logical = absolute.strip_prefix(root).unwrap_or(&absolute);
            let logical = logical
                .to_str()
                .ok_or_else(|| cache_error("input path is not UTF-8"))?;
            let metadata = fs::symlink_metadata(&absolute).map_err(cache_error)?;
            let (file_kind, link_target) = if metadata.is_symlink() {
                let target = fs::read_link(&absolute).map_err(cache_error)?;
                let target = target
                    .to_str()
                    .ok_or_else(|| cache_error("symlink target is not UTF-8"))?
                    .to_owned();
                (FileKind::Symlink, target)
            } else {
                (FileKind::File, String::new())
            };
            let bytes = fs::read(&absolute).map_err(cache_error)?;
            inputs.push(ActionInput::Raw {
                logical_path: logical.to_owned(),
                file_kind,
                executable: false,
                digest: content_digest(&[selected.as_bytes(), link_target.as_bytes(), &bytes]),
            });
        }
        // Order and repetitions affect first-source attribution and blank scoping.
        // This sequence is hashed before ActionContext normalizes its input set.
        Ok(content_digest(&[
            b"validation-file-inputs-v2",
            &serde_json::to_vec(&inputs).map_err(cache_error)?,
        ]))
    }

    /// Additional vocabulary/codec dimensions. The required implementation identity
    /// is bound separately into every action; these versions never establish freshness.
    pub fn toolchain_salt() -> String {
        Self::cache_key(&[
            CODEC.as_bytes(),
            env!("CARGO_PKG_VERSION").as_bytes(),
            purrdf::shapes::VERSION.as_bytes(),
            &purrdf::gts::wire::VERSION.to_le_bytes(),
        ])
    }

    fn context(&self, kind: &str, key: &str) -> ActionContext {
        ActionContext::new(
            "validation",
            kind,
            self.implementation.clone(),
            CODEC,
            Vec::new(),
        )
        .with_dimension("input-context", key)
        .with_dimension(
            "repository-location",
            self.project_root
                .to_str()
                .expect("validated repository path"),
        )
    }

    /// Read complete findings only after receipt/context/blob verification. A missing
    /// action is a miss; a corrupt entry is an explicit failure, never an empty verdict.
    pub fn read_cached_result(
        &self,
        kind: &str,
        key: &str,
    ) -> gmeow_errors::Result<Option<CachedResult>> {
        let Some(entry) = self
            .store
            .get::<()>(&self.context(kind, key))
            .map_err(cache_error)?
        else {
            return Ok(None);
        };
        if entry.receipt.product_digest != bytes_digest(&entry.bytes) {
            return Err(cache_error(
                "findings product digest disagrees with authenticated bytes",
            ));
        }
        serde_json::from_slice(&entry.bytes)
            .map(Some)
            .map_err(cache_error)
    }

    /// Publish a complete deterministic verdict under the exact selected context.
    pub fn write_cached_result(
        &self,
        kind: &str,
        key: &str,
        result: &CachedResult,
    ) -> gmeow_errors::Result<()> {
        let bytes = serde_json::to_vec(result).map_err(cache_error)?;
        self.store
            .publish(&self.context(kind, key), bytes_digest(&bytes), (), &bytes)
            .map_err(cache_error)?;
        Ok(())
    }
}
