// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The deterministic N-Triples / content-address emitter shared by every `lang:`
//! producer. Extracting it here keeps ONE digest algorithm and ONE line-canonicalization
//! across the translation corpus, the form corpus, and future projection corpora, so no
//! producer can drift its content addressing.
//!
//! The FULL content key is always the identity; [`digest16`] is a display-IRI shortener
//! only. [`assert_no_digest_collision`] enforces that the shortening never silently
//! aliases two distinct keys — a collision is a hard fail, not a merge.

use std::collections::HashMap;

use sha2::{Digest, Sha256};

/// A stable 16-hex-char content address over a domain-separated key: the SHA-256 of
/// `domain`, a unit-separator (`U+001F`), and `key`, truncated to the first 8 bytes.
///
/// Byte-identical to the translation producer's digest so a key addressed by either
/// producer resolves to the same short IRI segment.
pub fn digest16(domain: &str, key: &str) -> String {
    let digest = Sha256::digest(format!("{domain}\u{1f}{key}").as_bytes());
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// Canonicalize a set of N-Triples lines into a deterministic byte stream: sort, dedup,
/// join with `\n`, and terminate with a trailing newline. The output is a pure function
/// of the line set, so two runs that produce the same triples serialize byte-identically.
pub fn ntriples_sorted(mut lines: Vec<String>) -> Vec<u8> {
    lines.sort();
    lines.dedup();
    let mut out = lines.join("\n");
    out.push('\n');
    out.into_bytes()
}

/// Hard-fail collision guard: reject any case where two DISTINCT full keys map to the
/// same [`digest16`] output. The full content key stays the identity; the digest is a
/// display-IRI shortener, so a collision must surface as an error rather than silently
/// alias two keys to one short IRI.
///
/// `entries` is a list of `(full_key, digest)` pairs. Returns `Err` describing the first
/// colliding pair found; `Ok(())` when every digest maps to a single full key.
pub fn assert_no_digest_collision(entries: &[(String, String)]) -> Result<(), String> {
    let mut seen: HashMap<&str, &str> = HashMap::new();
    for (full_key, digest) in entries {
        match seen.get(digest.as_str()) {
            Some(prior) if *prior != full_key.as_str() => {
                return Err(format!(
                    "digest collision: distinct keys '{prior}' and '{full_key}' both map to \
                     digest '{digest}'"
                ));
            }
            _ => {
                seen.insert(digest.as_str(), full_key.as_str());
            }
        }
    }
    Ok(())
}
