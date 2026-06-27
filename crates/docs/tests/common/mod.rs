// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared, once-per-run fixture for the gmeow-docs integration tests.
//!
//! The cache machinery lives in [`gmeow_docs::fixture`]; this module only pins
//! the repo root (via the crate manifest dir) and exposes the loaders under the
//! `common::cached_model()` / `common::cached_site()` names the binaries call.
//! The cache is primed once before the test processes spawn by the
//! `prime-docs-fixture` example, which the Makefile test lanes and the CI test
//! job run immediately before `cargo nextest`, so no test pays the ~12 s model
//! build or the site render; on a plain `cargo test` (no prime step) the first
//! caller builds and caches it.

#![allow(dead_code)] // not every binary uses every helper

use std::path::PathBuf;

use gmeow_docs::render::Site;
use gmeow_docs::DocsModel;

/// The repository root, derived from this crate's manifest dir (`<repo>/crates/docs`).
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("crate is at <repo>/crates/docs")
        .to_path_buf()
}

/// The live documentation model, loaded from the shared once-per-run cache.
pub fn cached_model() -> DocsModel {
    gmeow_docs::fixture::load(&repo_root())
}

/// The rendered English static site, loaded from the shared once-per-run cache.
/// The canonical carrier render (`render_site` ≡ `render_site_lang(_, "english")`);
/// tests that only need the site (not the live render path) load it from here so
/// the suite pays the full render once, not once per process.
pub fn cached_site() -> Site {
    gmeow_docs::fixture::load_site(&repo_root())
}
