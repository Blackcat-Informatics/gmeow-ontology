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
//! P1 ships the deterministic key primitive; the on-disk store + self-verifying
//! load land in P2.

use sha2::{Digest, Sha256};

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
