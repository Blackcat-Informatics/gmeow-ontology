// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Well-formedness gate for the emitted ShEx projections.
//!
//! gmeow projects its shape layer to ShExC, but nothing machine-checked that
//! the emitted `.shex` documents actually parse. purrdf ships a
//! conformance-tested ShEx 2.1 parser (`purrdf::shex::parse_shexc`); this test
//! feeds each emitted projection back through it and hard-fails on any parse
//! error, so a malformed projection can never be shipped silently.

#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};

use purrdf::shex::parse_shexc;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

/// Parse the projection at `rel` and hard-fail on a parse error. A malformed
/// projection is always a hard failure.
fn assert_shex_well_formed(src: &str, rel: &str) {
    if let Err(err) = parse_shexc(src, None) {
        panic!("emitted ShEx projection {rel} is not well-formed: {err}");
    }
}

#[test]
fn generated_validation_shapes_shex_is_well_formed() {
    // The committed, canonical shape projection: always present in a checkout, so
    // its absence is itself a hard failure (the projection must have run).
    let rel = "generated/shapes/validation-shapes.shex";
    let src = std::fs::read_to_string(repo_root().join(rel))
        .unwrap_or_else(|e| panic!("committed ShEx projection {rel} must be readable: {e}"));
    assert_shex_well_formed(&src, rel);
}

#[test]
fn dist_gmeow_shex_is_well_formed() {
    // `dist/gmeow.shex` is a `make build` output (git-ignored), not one of the
    // `make check` CHECK_TARGETS, so it need not exist when this lane runs in a
    // fresh checkout. When it HAS been built, it must be well-formed; a malformed
    // build output is still a hard failure. Absence is a build-ordering condition,
    // not a projection defect, so it is skipped (never silently degraded content).
    let rel = "dist/gmeow.shex";
    // Absent in a source-only checkout (`make build` not run): a build-ordering
    // condition, not a projection defect, so skipped silently.
    if let Ok(src) = std::fs::read_to_string(repo_root().join(rel)) {
        assert_shex_well_formed(&src, rel);
    }
}
