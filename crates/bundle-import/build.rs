// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Emit the exact producer identity for the graph-preserving bundle-import product.
//!
//! The content key already binds the exact GTS bytes. The producer identity binds this
//! leaf's implementation, dependency resolution, compiler unit, target, profile, features,
//! and code-generation flags. Unrelated workspace test/source edits are deliberately absent:
//! they cannot change this codec and must not invalidate a 140 MiB derived product.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

#[path = "src/build_inputs.rs"]
mod build_inputs;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let workspace = manifest
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let mut inputs = BTreeMap::<String, Vec<u8>>::new();
    for crate_dir in build_inputs::transitive_path_dependency_dirs(&manifest) {
        for path in build_inputs::crate_input_paths(&crate_dir) {
            collect_file(&path, &workspace, &mut inputs);
        }
    }
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

    let mut hash = Sha256::new();
    hash.update(b"gmeow:bundle-import-build:v2\x1f");
    for (path, bytes) in inputs {
        hash.update(path.as_bytes());
        hash.update([0x1f]);
        hash.update(bytes);
        hash.update([0x1e]);
    }
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    hash.update(b"rustc-vv\x1f");
    hash.update(command_identity(&rustc, &["-Vv"]).as_bytes());

    let mut unit = BTreeMap::new();
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
            unit.insert(name.to_string(), value);
        }
    }
    for (name, value) in std::env::vars().filter(|(name, _)| name.starts_with("CARGO_CFG_")) {
        unit.insert(name, value);
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
    unit.insert("FEATURES".to_string(), features.join(","));
    for (name, value) in unit {
        hash.update(name.as_bytes());
        hash.update([0x1f]);
        hash.update(value.as_bytes());
        hash.update([0x1e]);
    }
    println!(
        "cargo:rustc-env=GMEOW_BUNDLE_IMPORT_BUILD_FINGERPRINT={}",
        hex(&hash.finalize())
    );
}

fn collect_file(path: &Path, workspace: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
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
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
