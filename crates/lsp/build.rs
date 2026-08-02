// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Resolve, guard, and expose the staged `generated/dist/gmeow.gts` bundle path
//! that `src/lib.rs` embeds with `include_bytes!(env!("GMEOW_BUNDLE_PATH"))`.
//!
//! The bundle is a git-ignored local/release product materialized by
//! `make check` (or `make install`), never a committed input — so this build
//! script resolves an ABSOLUTE path (independent of the build's CWD) and
//! fails the build closed, with an actionable message naming the bootstrap
//! command, when the file is absent or zero-length (empty/truncated) rather
//! than letting a bare `include_bytes!` "file not found" or a silently
//! truncated embed reach a consumer.
//!
//! `GMEOW_BUNDLE_PATH` may be set in the environment to override the default
//! `<CARGO_MANIFEST_DIR>/../../generated/dist/gmeow.gts` location — an escape
//! hatch for release/package flows that stage the bundle elsewhere — and the
//! same absent/empty guard still applies to the override.
//!
//! Dependency-free (std only) so this never perturbs the build graph.

use std::path::{Path, PathBuf};

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set by cargo");

    // A flip of the override (set/unset/changed) must re-resolve and re-embed.
    println!("cargo:rerun-if-env-changed=GMEOW_BUNDLE_PATH");

    let raw: PathBuf = match std::env::var_os("GMEOW_BUNDLE_PATH") {
        Some(over) => PathBuf::from(over),
        None => Path::new(&manifest)
            .join("..")
            .join("..")
            .join("generated")
            .join("dist")
            .join("gmeow.gts"),
    };

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
            "gmeow: staged bundle {} is missing or empty — run `make check` (or `make install`) \
             to materialize generated/dist/gmeow.gts before building this consumer. It is a \
             git-ignored local/release product, not a committed input.",
            absolute.display()
        );
    }

    // Presence is confirmed — canonicalize away any `..`/symlinks so
    // `include_bytes!` resolves the identical bytes regardless of CWD.
    let resolved = absolute.canonicalize().unwrap_or_else(|e| {
        panic!(
            "gmeow: cannot canonicalize staged bundle path {}: {e}",
            absolute.display()
        )
    });

    println!("cargo:rerun-if-changed={}", resolved.display());
    println!("cargo:rustc-env=GMEOW_BUNDLE_PATH={}", resolved.display());
}
