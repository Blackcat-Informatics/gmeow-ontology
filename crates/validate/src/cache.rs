// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Content-addressed validation cache under `.cache/validate`.
//!
//! The cache persists JSON objects `{"findings": [...]}` under
//! `<project-root>/.cache/validate/<kind>/<key>.json`, where each finding is a
//! serialized [`gmeow_errors::Finding`] — so the structured SHACL focus
//! nodes and GTS wire coordinates survive a cache hit, not just a fresh compute
//! . Keys are short SHA-256 hashes of NUL-delimited byte parts, matching
//! the Python `_cache_key` algorithm. Invalidation is purely content-based;
//! there is no TTL. Older `{"errors","warnings"}` entries simply fail to
//! deserialize and are treated as a miss, so the cache self-heals on upgrade.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use gmeow_errors::{Diag, Finding};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

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

/// Manages the `.cache/validate` content-addressed cache.
#[derive(Debug, Clone)]
pub struct ValidationCache {
    /// Project root: cache files live under `.cache/validate` here.
    project_root: PathBuf,
}

impl ValidationCache {
    /// Create a cache rooted at `<project_root>/.cache/validate`.
    ///
    /// `project_root` is resolved to an absolute path when possible so that
    /// relative file cache keys match the Python `generator.source_hash`
    /// behavior.
    pub fn new(project_root: impl AsRef<Path>) -> Self {
        let path = project_root.as_ref();
        let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        Self {
            project_root: resolved,
        }
    }

    /// Return the cache directory root (`<project_root>/.cache/validate`).
    pub fn cache_dir(&self) -> PathBuf {
        self.project_root.join(".cache").join("validate")
    }

    /// Return a short stable hash for cache-key parts.
    ///
    /// Mirrors Python `_cache_key`: SHA-256 over each part followed by a NUL
    /// byte, truncated to 16 hex characters.
    pub fn cache_key(parts: &[&[u8]]) -> String {
        let mut h = Sha256::new();
        for part in parts {
            h.update(part);
            h.update(b"\0");
        }
        hex_encode(&h.finalize())[..16].to_owned()
    }

    /// Return a content hash for a list of input files.
    ///
    /// Mirrors Python `generator.source_hash`: paths are resolved and sorted,
    /// then each contributes its path (relative to the project root when
    /// possible), file size, and raw content to a SHA-256 hash. Missing paths
    /// are skipped with a `tracing` warning rather than failing, matching the
    /// lenient behavior.
    pub fn files_cache_key(&self, paths: &[PathBuf]) -> gmeow_errors::Result<String> {
        Self::files_cache_key_with_root(paths, &self.project_root)
    }

    /// Core implementation of [`Self::files_cache_key`] with an explicit root.
    pub fn files_cache_key_with_root(
        paths: &[PathBuf],
        root: &Path,
    ) -> gmeow_errors::Result<String> {
        let mut h = Sha256::new();
        let mut sorted: Vec<PathBuf> = paths
            .iter()
            .map(|p| p.canonicalize().unwrap_or_else(|_| p.clone()))
            .collect();
        sorted.sort();
        for path in sorted {
            if !path.exists() {
                tracing::warn!(
                    target: "validation_cache",
                    path = %path.display(),
                    "validation cache input missing; skipping",
                );
                continue;
            }
            let rel = if let Ok(r) = path.strip_prefix(root) {
                r.to_string_lossy().into_owned()
            } else {
                path.to_string_lossy().into_owned()
            };
            let meta = fs::metadata(&path).map_err(|e| {
                Diag::of_kind(crate::error::Io {
                    detail: format!("metadata for {}: {e}", path.display()),
                })
            })?;
            let bytes = fs::read(&path).map_err(|e| {
                Diag::of_kind(crate::error::Io {
                    detail: format!("read {} for cache key: {e}", path.display()),
                })
            })?;
            h.update(rel.as_bytes());
            h.update(meta.len().to_string().as_bytes());
            h.update(&bytes);
        }
        Ok(hex_encode(&h.finalize())[..16].to_owned())
    }

    /// Return a cache salt for the SHACL validation toolchain versions.
    ///
    /// Mirrors Python `_validation_toolchain_salt`: hashes version strings for
    /// `gmeow-shacl`, `gmeow-validate`, and the `gmeow-gts` wire-format version.
    /// Because these are Rust crates, the package version from
    /// `CARGO_PKG_VERSION` is used instead of Python `importlib.metadata.version`.
    pub fn toolchain_salt() -> String {
        let validate_version = env!("CARGO_PKG_VERSION");
        let shacl_version = purrdf::shapes::VERSION;
        let gts_version = purrdf::gts::wire::VERSION;
        Self::cache_key(&[
            format!("gmeow-validate={validate_version}").as_bytes(),
            format!("gmeow-shacl={shacl_version}").as_bytes(),
            format!("gmeow-gts-wire={gts_version}").as_bytes(),
        ])
    }

    /// Read a cached result if present and valid.
    pub fn read_cached_result(&self, kind: &str, key: &str) -> Option<CachedResult> {
        let path = self.cache_path(kind, key);
        let bytes = fs::read(&path).ok()?;
        let result: CachedResult = serde_json::from_slice(&bytes).ok()?;
        Some(result)
    }

    /// Persist a cached result atomically.
    ///
    /// Writes to a temporary file in the same directory and renames it into
    /// place so concurrent readers never see a partial JSON object.
    pub fn write_cached_result(
        &self,
        kind: &str,
        key: &str,
        result: &CachedResult,
    ) -> gmeow_errors::Result<()> {
        let path = self.cache_path(kind, key);
        let parent = path.parent().ok_or_else(|| {
            Diag::of_kind(crate::error::Io {
                detail: format!("cache path has no parent: {}", path.display()),
            })
        })?;
        fs::create_dir_all(parent).map_err(|e| {
            Diag::of_kind(crate::error::Io {
                detail: format!("create cache dir {}: {e}", parent.display()),
            })
        })?;

        let payload = serde_json::to_vec(result).map_err(|e| {
            Diag::of_kind(crate::error::Serialize {
                detail: format!("serialize cached result: {e}"),
            })
        })?;

        let tmp_name = format!(
            ".{}.{}.{}",
            path.file_name().unwrap_or_default().to_string_lossy(),
            std::process::id(),
            TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let tmp_path = parent.join(tmp_name);
        let write_result: gmeow_errors::Result<()> = (|| {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp_path)
                .map_err(|e| {
                    Diag::of_kind(crate::error::Io {
                        detail: format!("create temp cache file {}: {e}", tmp_path.display()),
                    })
                })?;
            file.write_all(&payload).map_err(|e| {
                Diag::of_kind(crate::error::Io {
                    detail: format!("write temp cache file {}: {e}", tmp_path.display()),
                })
            })?;
            fs::rename(&tmp_path, &path).map_err(|e| {
                Diag::of_kind(crate::error::Io {
                    detail: format!(
                        "rename cache file {} -> {}: {e}",
                        tmp_path.display(),
                        path.display()
                    ),
                })
            })
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&tmp_path);
        }
        write_result
    }

    /// Return the filesystem path for a cached result.
    fn cache_path(&self, kind: &str, key: &str) -> PathBuf {
        let safe_kind = sanitize_kind(kind);
        self.cache_dir().join(safe_kind).join(format!("{key}.json"))
    }
}

/// Replace any run of characters that are not alphanumeric, dot, underscore, or
/// hyphen with a single hyphen, matching Python `_validation_cache_path`.
fn sanitize_kind(kind: &str) -> String {
    let mut out = String::with_capacity(kind.len());
    let mut prev_dash = false;
    for ch in kind.chars() {
        if ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-' {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out
}

/// Encode a byte slice as lowercase hexadecimal.
fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_result_write_replaces_cleanly() {
        // RAII: the directory is removed when `tmp` drops at end of scope,
        // including on panic or early return.
        let tmp = tempfile::tempdir().expect("create temp dir");
        let cache = ValidationCache::new(tmp.path());
        let key = "abc123";
        let kind = "test-phase";

        let old = CachedResult::from_findings(vec![Finding::new(
            gmeow_errors::Severity::Error,
            "old",
            "old error",
        )]);
        let new = CachedResult::from_findings(vec![Finding::new(
            gmeow_errors::Severity::Warning,
            "new",
            "new warning",
        )]);

        cache.write_cached_result(kind, key, &old).unwrap();
        cache.write_cached_result(kind, key, &new).unwrap();

        let read = cache
            .read_cached_result(kind, key)
            .expect("cached result must exist");
        assert_eq!(read.findings.len(), 1);
        assert_eq!(read.findings[0].code, "new");

        // No stray temp files left behind.
        let tmp_files: Vec<_> = cache
            .cache_dir()
            .read_dir()
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(&format!(".{key}.json."))
            })
            .collect();
        assert!(tmp_files.is_empty(), "temp cache files must be cleaned up");
    }

    #[test]
    fn cached_result_ignores_non_object_payload() {
        // RAII: the directory is removed when `tmp` drops at end of scope,
        // including on panic or early return.
        let tmp = tempfile::tempdir().expect("create temp dir");
        let cache = ValidationCache::new(tmp.path());
        let path = cache.cache_path("test-phase", "abc123");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"[]").unwrap();

        assert!(cache.read_cached_result("test-phase", "abc123").is_none());
    }
}
