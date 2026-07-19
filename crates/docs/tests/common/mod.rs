// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared, once-per-run fixture for the gmeow-docs integration tests.
//!
//! The cache machinery lives in [`gmeow_docs::fixture`]; this module only pins
//! the repo root (via the crate manifest dir) and exposes the loaders under the
//! `common::cached_model()` / `common::cached_site()` / `common::cached_book()`
//! names the binaries call.
//! The cache is primed once before the test processes spawn by the
//! `prime-docs-fixture` example, which the Makefile test lanes and the CI test
//! job run immediately before `cargo nextest`, so no test pays the ~12 s model
//! build or the site render; on a plain `cargo test` (no prime step) the first
//! caller builds and caches it.

#![allow(dead_code)] // not every binary uses every helper

use std::path::PathBuf;

use gmeow_docs::DocsModel;
use gmeow_docs::render::Site;

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

/// The rendered static site for `lang`, loaded from the shared once-per-run cache.
/// The English carrier and every translation (`fr`, `zh`, …) are cached
/// symmetrically by `prime`, so a per-language round-trip test reads its tree from
/// here instead of paying a live `render_site_lang` walk.
pub fn cached_site_lang(lang: &str) -> Site {
    gmeow_docs::fixture::load_site_lang(&repo_root(), lang)
}

/// The default mdBook render (`render_book(&model, &ExecutableDocsData::default())`),
/// loaded from the shared once-per-run cache. `mdbook_render` tests that render the
/// default book read it from here so the suite pays the full book render once, not
/// once per process. Tests that mutate the model or pass custom executable data
/// still call `render_book` directly.
pub fn cached_book() -> Site {
    gmeow_docs::fixture::load_book(&repo_root())
}
