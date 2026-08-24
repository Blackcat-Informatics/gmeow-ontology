// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Emit the fail-closed build identity used by pipeline action keys.
//!
//! The identity covers every file under every workspace crate (Rust plus compile-time
//! assets/fixtures), all Cargo manifests, the lockfile, Cargo configuration, the pinned
//! toolchain declaration, the complete compiler identity, target/profile/features, and
//! code-generation flags. A cache hit is an executable claim about all of those inputs,
//! so omitting any one would make a syntactically valid key semantically incomplete.
//!
//! The full-run clean manifest and focused per-stage cache keys incorporate this
//! fingerprint, so ANY change to ANY workspace crate — or a toolchain or dependency
//! bump — invalidates the cached proof. That makes both cache boundaries fail-closed
//! against the hazard where a Rust implementation change could serve a stale
//! pre-change product because no manual `impl_version` was bumped. Full sync runs use
//! this identity both for the whole-run clean manifest and for the DAG-admitted cache
//! of independently bounded stage contributions.
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

    // Collect, sorted by relative path for determinism, every workspace crate file.
    // This deliberately includes non-Rust compile-time inputs (`include_str!`,
    // `include_bytes!`, templates, wasm, fixtures) rather than trying to maintain an
    // incomplete extension allowlist. BTreeMap keeps the fold stable.
    let mut inputs: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    collect_workspace_inputs(&workspace.join("crates"), &workspace, &mut inputs);
    for relative in [
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain",
        "rust-toolchain.toml",
        ".cargo/config",
        ".cargo/config.toml",
    ] {
        let path = workspace.join(relative);
        // Watching a nonexistent optional spelling marks the package dirty forever
        // (`the file rust-toolchain is missing`). Watch and hash only the spelling
        // that actually exists; creating the alternative necessarily changes the
        // workspace directory/package inputs and is picked up on the next build.
        if let Ok(bytes) = std::fs::read(&path) {
            println!("cargo:rerun-if-changed={}", path.display());
            inputs.insert(relative.to_string(), bytes);
        }
    }

    let mut hasher = Sha256::new();
    for (rel, content) in &inputs {
        hasher.update(rel.as_bytes());
        hasher.update([0x1f]);
        hasher.update(content);
        hasher.update([0x1e]);
    }
    // Fold the COMPLETE compiler identity. `-Vv` includes commit, host, release, and
    // LLVM identity; the short version formerly used here did not distinguish every
    // code-generation authority.
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let rustc_identity = command_identity(&rustc, &["-Vv"]);
    hasher.update(b"rustc-vv\x1f");
    hasher.update(rustc_identity.as_bytes());

    // These values select the executable artifact Cargo builds even when sources and
    // the lockfile are unchanged. Keep the ordered, human-readable projection as build
    // metadata as well as folding it into the opaque fingerprint. They are Cargo unit
    // inputs already: do not emit `rerun-if-env-changed` for Cargo-provided HOST/TARGET/
    // PROFILE/FEATURE/CFG values. Doing so compares the outer Cargo environment with the
    // build-script environment and can make an otherwise warm crate rebuild forever.
    let mut context: BTreeMap<String, String> = BTreeMap::new();
    for name in [
        "HOST",
        "TARGET",
        "PROFILE",
        "OPT_LEVEL",
        "DEBUG",
        "CARGO_BUILD_TARGET",
        "CARGO_ENCODED_RUSTFLAGS",
        "RUSTFLAGS",
        "RUSTDOCFLAGS",
    ] {
        if let Ok(value) = std::env::var(name) {
            context.insert(name.to_string(), value);
        }
    }
    // Cargo's target configuration affects conditional compilation and ABI without
    // necessarily changing TARGET itself. Fold every exposed `CARGO_CFG_*` dimension
    // rather than maintaining an allowlist that can silently omit a new cfg axis.
    let mut cargo_cfg: Vec<(String, String)> = std::env::vars()
        .filter(|(name, _)| name.starts_with("CARGO_CFG_"))
        .collect();
    cargo_cfg.sort();
    for (name, value) in cargo_cfg {
        context.insert(name, value);
    }
    let mut features: Vec<String> = std::env::vars()
        .filter_map(|(name, value)| {
            name.strip_prefix("CARGO_FEATURE_")
                .filter(|_| value == "1")
                .map(str::to_owned)
        })
        .collect();
    features.sort();
    features.dedup();
    context.insert("FEATURES".to_string(), features.join(","));
    for (name, value) in &context {
        hasher.update(name.as_bytes());
        hasher.update([0x1f]);
        hasher.update(value.as_bytes());
        hasher.update([0x1e]);
    }

    let toolchain_digest = {
        let digest = Sha256::digest(rustc_identity.as_bytes());
        hex(&digest)
    };

    let digest = hasher.finalize();
    println!("cargo:rustc-env=GMEOW_BUILD_FINGERPRINT={}", hex(&digest));
    println!("cargo:rustc-env=GMEOW_TOOLCHAIN_FINGERPRINT={toolchain_digest}");
    println!(
        "cargo:rustc-env=GMEOW_BUILD_TARGET={}",
        context
            .get("TARGET")
            .map(String::as_str)
            .unwrap_or("unknown")
    );
    println!(
        "cargo:rustc-env=GMEOW_BUILD_PROFILE={}",
        context
            .get("PROFILE")
            .map(String::as_str)
            .unwrap_or("unknown")
    );
    println!(
        "cargo:rustc-env=GMEOW_BUILD_FEATURES={}",
        features.join(",")
    );
}

fn command_identity(program: &str, args: &[&str]) -> String {
    match std::process::Command::new(program).args(args).output() {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).into_owned()
        }
        Ok(output) => format!(
            "status={};stdout={};stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
        Err(error) => format!("unavailable:{error}"),
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Recursively collect every regular file under the workspace `crates/` tree, keyed by
/// its path relative to `workspace`, and emit a `rerun-if-changed` for each. This tree
/// contains compile-time assets and fixtures as well as Rust. Build output is excluded
/// because it is an effect of this identity, never an input to it.
fn collect_workspace_inputs(dir: &Path, workspace: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
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
            collect_workspace_inputs(&path, workspace, out);
        } else if path.is_file()
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
