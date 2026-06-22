// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The per-stage content-addressed cache (#861 P2).
//!
//! The cache key folds `stage.id ++ impl_version ++ sorted(upstream output
//! digests) ++ source_file_digest[SourceLoad only]` into a [`content_digest`],
//! and `generated/.pipeline-cache/` (gitignored) maps key → `StageProduct`,
//! backed by the kernel `ContentStore`. It is self-verifying: a digest recheck
//! on load HARD-fails on mismatch and never silently repairs (no-optionality).
//!
//! P2 adds the on-disk [`PipelineCache`]: `generated/.pipeline-cache/` holds an
//! `index.json` mapping each stage key to a blob digest, and `blobs/<digest>`
//! holds the serialized [`StageProduct`]. On load the blob is re-hashed and
//! compared to the indexed digest — a mismatch is a HARD failure, never a
//! silent repair (no-optionality).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use gmeow_rdf::ContentDigest;
use sha2::{Digest, Sha256};

use crate::error::PipelineError;
use crate::node::StageProduct;

/// Compute a hex SHA-256 over a sequence of byte fields, each length-free but
/// unit-separated, so the digest is unambiguous and order-sensitive.
pub fn content_digest(fields: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    for field in fields {
        hasher.update(field);
        hasher.update(b"\x1f");
    }
    let bytes = hasher.finalize();
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// The per-stage cache key: stage id + impl version + the sorted upstream output
/// digests (Merkle composition). `source_file_digest` is folded only for
/// `SourceLoad` stages (their inputs are files, not upstream products).
pub fn stage_key(
    stage_id: &str,
    impl_version: &str,
    upstream_digests_sorted: &[String],
    source_file_digest: Option<&str>,
) -> String {
    let mut fields: Vec<&[u8]> = vec![stage_id.as_bytes(), impl_version.as_bytes()];
    for d in upstream_digests_sorted {
        fields.push(d.as_bytes());
    }
    if let Some(src) = source_file_digest {
        fields.push(b"source");
        fields.push(src.as_bytes());
    }
    content_digest(&fields)
}

// ── On-disk content-addressed cache ──────────────────────────────────────────

/// The persistent per-stage cache under `generated/.pipeline-cache/` (gitignored).
///
/// `index.json` maps `stage_key → blob ContentDigest (hex)`; `blobs/<hex>` holds
/// the serialized [`StageProduct`]. Reads re-hash the blob and HARD-fail on a
/// digest mismatch (self-verifying, no silent repair).
pub struct PipelineCache {
    dir: PathBuf,
    index: BTreeMap<String, String>,
}

impl PipelineCache {
    /// The conventional cache directory under a repo root.
    pub fn default_dir(root: &Path) -> PathBuf {
        root.join("generated").join(".pipeline-cache")
    }

    /// Open (or create) the cache rooted at `dir`, loading its index.
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self, PipelineError> {
        let dir = dir.into();
        fs::create_dir_all(dir.join("blobs"))?;
        let index_path = dir.join("index.json");
        let index: BTreeMap<String, String> = if index_path.exists() {
            let bytes = fs::read(&index_path)?;
            serde_json::from_slice(&bytes)
                .map_err(|e| PipelineError::Parse(format!("corrupt pipeline cache index: {e}")))?
        } else {
            BTreeMap::new()
        };
        Ok(Self { dir, index })
    }

    /// Look up a stage product by cache key. Returns `None` on a miss. HARD-fails
    /// (`CacheMismatch`) if the blob exists but its re-hashed digest disagrees
    /// with the index — the cache is never silently repaired.
    pub fn get(&self, stage_key: &str) -> Result<Option<StageProduct>, PipelineError> {
        let Some(digest_hex) = self.index.get(stage_key) else {
            return Ok(None);
        };
        let blob_path = self.dir.join("blobs").join(digest_hex);
        if !blob_path.exists() {
            // Index references a missing blob: a corrupt cache, not a clean miss.
            return Err(PipelineError::CacheMismatch {
                expected: digest_hex.clone(),
                actual: "<missing blob>".to_string(),
            });
        }
        let bytes = fs::read(&blob_path)?;
        let actual = ContentDigest::of(&bytes).to_hex();
        if &actual != digest_hex {
            return Err(PipelineError::CacheMismatch {
                expected: digest_hex.clone(),
                actual,
            });
        }
        let product: StageProduct = serde_json::from_slice(&bytes)
            .map_err(|e| PipelineError::Parse(format!("corrupt cached product: {e}")))?;
        Ok(Some(product))
    }

    /// Store a stage product under `stage_key`, persisting the blob and index.
    pub fn put(&mut self, stage_key: &str, product: &StageProduct) -> Result<(), PipelineError> {
        let bytes = serde_json::to_vec(product)
            .map_err(|e| PipelineError::Parse(format!("cannot serialize product: {e}")))?;
        let digest_hex = ContentDigest::of(&bytes).to_hex();
        fs::write(self.dir.join("blobs").join(&digest_hex), &bytes)?;
        self.index.insert(stage_key.to_string(), digest_hex);
        self.persist_index()?;
        Ok(())
    }

    /// Number of cached entries.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    fn persist_index(&self) -> Result<(), PipelineError> {
        // Deterministic: BTreeMap serializes in sorted key order.
        let bytes = serde_json::to_vec_pretty(&self.index)
            .map_err(|e| PipelineError::Parse(format!("cannot serialize cache index: {e}")))?;
        fs::write(self.dir.join("index.json"), bytes)?;
        Ok(())
    }
}
