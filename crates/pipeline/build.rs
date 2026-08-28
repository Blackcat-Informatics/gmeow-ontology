// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Emit the fail-closed build identity used by pipeline action keys.
//!
//! The identity covers the pipeline library's exact transitive non-dev path-dependency
//! closure (Rust plus compile-time assets/templates), the workspace manifest and lockfile,
//! Cargo configuration, the pinned toolchain declaration, the complete compiler identity,
//! target/profile/features, and code-generation flags. Unrelated workspace packages and
//! each crate's separate binary target tree cannot enter this producer library and are
//! deliberately excluded. The pipeline's fixture selector and unit-test module are
//! consumers/orchestrators of stage products, not stage semantics, so changing them must
//! not invalidate every production-stage action either.
//!
//! The full-run clean manifest and focused per-stage cache keys incorporate this
//! fingerprint, so any change capable of affecting the producer — or a toolchain or
//! dependency bump — invalidates the cached proof. Test/report-only code changes do not.
//! That makes both cache boundaries fail-closed against the hazard where a Rust
//! implementation change could serve a stale pre-change product because no manual
//! `impl_version` was bumped. Full sync runs use this identity both for the whole-run
//! clean manifest and for the DAG-admitted cache of independently bounded stage
//! contributions.
//!
//! A `rerun-if-changed` is emitted for every hashed file, so Cargo recomputes the
//! fingerprint exactly when a hashed input changes (and not otherwise).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

#[path = "../../build-support/path_dependency_inputs.rs"]
mod build_inputs;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    // crates/pipeline → workspace root is two levels up.
    let workspace = manifest
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonicalize workspace root");

    // Derive the local implementation closure from Cargo manifests. Cargo.lock binds
    // registry/Git dependencies; path dependencies have no content checksum there and
    // therefore need this live content fold. Separate binary trees are not linked into
    // the producer library. BTreeMap keeps the fold stable.
    let mut inputs: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for crate_dir in build_inputs::transitive_path_dependency_dirs(&manifest) {
        for path in build_inputs::crate_input_paths(&crate_dir) {
            if is_library_implementation_input(&path, &crate_dir) {
                collect_file(&path, &workspace, &mut inputs);
            }
        }
    }
    collect_file(
        &workspace.join("build-support/path_dependency_inputs.rs"),
        &workspace,
        &mut inputs,
    );
    for relative in [
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain",
        "rust-toolchain.toml",
        ".cargo/config",
        ".cargo/config.toml",
    ] {
        collect_file(&workspace.join(relative), &workspace, &mut inputs);
    }

    let mut hasher = Sha256::new();
    hasher.update(b"gmeow:pipeline-build:v3\x1f");
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

fn collect_file(path: &Path, workspace: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
    // Watching a nonexistent optional spelling marks the package dirty forever. Watch
    // and hash only files that exist; a newly selected manifest/toolchain spelling is
    // observed when Cargo next resolves the package.
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    println!("cargo:rerun-if-changed={}", path.display());
    let relative = path
        .strip_prefix(workspace)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned();
    out.insert(relative, bytes);
}

fn is_library_implementation_input(path: &Path, crate_dir: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(crate_dir) else {
        return true;
    };
    if relative == Path::new("src/main.rs") || relative.starts_with("src/bin") {
        return false;
    }
    if crate_dir.file_name().is_some_and(|name| name == "pipeline")
        && matches!(relative.to_str(), Some("src/fixture.rs" | "src/tests.rs"))
    {
        return false;
    }
    let docs_non_library_assets = [
        "assets/console/pkg",
        "assets/console/smoke",
        "assets/console/tests",
        "assets/tests",
    ];
    crate_dir.file_name().is_none_or(|name| name != "docs")
        || docs_non_library_assets
            .iter()
            .all(|excluded| !relative.starts_with(excluded))
}
