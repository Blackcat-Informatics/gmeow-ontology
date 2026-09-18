// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Resolve, guard, and expose every producer-authored asset embedded in `gmeow`.
//!
//! The bundle is a git-ignored local/release product materialized by
//! `make check` (or `make install`), never a committed input — so this build
//! script resolves an ABSOLUTE path (independent of the build's CWD) and
//! fails the build closed, with an actionable message naming the bootstrap
//! command, when the file is absent or zero-length (empty/truncated) rather
//! than letting a bare `include_bytes!` "file not found" or a silently
//! truncated embed reach a consumer.
//!
//! Release/package flows may override either producer path. The same
//! absent/empty guard applies to an override, so no build can silently ship a
//! partial consumer.
//!
//! Dependency-free (std only) so this never perturbs the build graph.

use std::path::{Path, PathBuf};

/// Bind every mandatory producer artifact into the consumer compile.
fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set by cargo");
    let generated = Path::new(&manifest).join("..").join("..").join("generated");
    expose_asset(
        "GMEOW_BUNDLE_PATH",
        generated.join("dist").join("gmeow.gts"),
        "gmeow.gts bundle",
    );
    expose_asset(
        "GMEOW_CODEBOOK_PATH",
        generated
            .join("projections")
            .join("lang")
            .join("gmn-codebook.cbor"),
        "GMN codebook",
    );
}

/// Resolve one default/override path, reject an absent payload, and expose its
/// canonical absolute path to `include_bytes!`.
fn expose_asset(env_name: &str, default: PathBuf, label: &str) {
    // A flip of the override (set/unset/changed) must re-resolve and re-embed.
    println!("cargo:rerun-if-env-changed={env_name}");

    let raw = std::env::var_os(env_name).map_or(default, PathBuf::from);

    // Absolutize before existence is even checked, so the guard message (and
    // any emitted env var) never depends on the build's current working
    // directory.
    let absolute = if raw.is_absolute() {
        raw.clone()
    } else {
        std::env::current_dir()
            .expect("current working directory")
            .join(&raw)
    };

    let len = std::fs::metadata(&absolute).map(|m| m.len()).unwrap_or(0);
    if len == 0 {
        panic!(
            "gmeow: staged {label} {} is missing or empty — run `make cli-build` (or \
             `make install`) to materialize every embedded producer asset before building this \
             consumer. It is a git-ignored local/release product, not a committed input.",
            absolute.display()
        );
    }

    // Presence is confirmed — canonicalize away any `..`/symlinks so
    // `include_bytes!` resolves the identical bytes regardless of CWD.
    let resolved = absolute.canonicalize().unwrap_or_else(|e| {
        panic!(
            "gmeow: cannot canonicalize staged {label} path {}: {e}",
            absolute.display()
        )
    });

    println!("cargo:rerun-if-changed={}", resolved.display());
    println!("cargo:rustc-env={env_name}={}", resolved.display());
}
