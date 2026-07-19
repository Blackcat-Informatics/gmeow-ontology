// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Emit `GMEOW_BUILD_FINGERPRINT`: a content hash of the entire workspace Rust
//! source (every `crates/**/*.rs`), `Cargo.lock`, and the `rustc` version.
//!
//! The full-run clean manifest and focused per-stage cache keys incorporate this
//! fingerprint, so ANY change to ANY workspace crate — or a toolchain or dependency
//! bump — invalidates the cached proof. That makes both cache boundaries fail-closed
//! against the hazard where a Rust implementation change could serve a stale
//! pre-change product because no manual `impl_version` was bumped. Full sync runs do
//! not serialize cumulative carriers per stage; their warm path is the whole-run
//! clean manifest.
//!
//! A `rerun-if-changed` is emitted for every hashed file, so Cargo recomputes the
//! fingerprint exactly when a hashed input changes (and not otherwise).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    // crates/pipeline → workspace root is two levels up.
    let workspace = Path::new(&manifest)
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonicalize workspace root");

    // Collect, sorted by relative path for determinism, every workspace .rs source
    // plus Cargo.lock. BTreeMap keeps the fold order stable regardless of walk order.
    let mut inputs: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    collect_rs(&workspace.join("crates"), &workspace, &mut inputs);
    let lock = workspace.join("Cargo.lock");
    if let Ok(bytes) = std::fs::read(&lock) {
        println!("cargo:rerun-if-changed={}", lock.display());
        inputs.insert("Cargo.lock".to_string(), bytes);
    }

    let mut hasher = Sha256::new();
    for (rel, content) in &inputs {
        hasher.update(rel.as_bytes());
        hasher.update([0x1f]);
        hasher.update(content);
        hasher.update([0x1e]);
    }
    // Fold the compiler version: a toolchain change can alter deterministic output
    // without any source change, so it must invalidate the cache too.
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    if let Ok(out) = std::process::Command::new(rustc).arg("--version").output() {
        hasher.update(b"rustc\x1f");
        hasher.update(&out.stdout);
    }

    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for b in digest {
        hex.push_str(&format!("{b:02x}"));
    }
    println!("cargo:rustc-env=GMEOW_BUILD_FINGERPRINT={hex}");
}

/// Recursively collect every `*.rs` file under `dir`, keyed by its path relative to
/// `workspace`, emitting a `rerun-if-changed` for each so Cargo refingerprints on any
/// edit. Skips `target/` (build output) — nothing under it feeds the source identity.
fn collect_rs(dir: &Path, workspace: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            collect_rs(&path, workspace, out);
        } else if path.extension().is_some_and(|e| e == "rs")
            && let Ok(bytes) = std::fs::read(&path)
        {
            let rel = path
                .strip_prefix(workspace)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            println!("cargo:rerun-if-changed={}", path.display());
            out.insert(rel, bytes);
        }
    }
}
