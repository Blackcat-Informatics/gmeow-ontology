// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Extract-tree coverage (R4 of #859).
//!
//! The on-disk writer `DocSet::write_artifacts` is a private GIL-bound PyO3
//! method (and pyo3's `extension-module` feature precludes a standalone GIL test
//! under `cargo test`), and the bundle→tree unpack proper lives in the *Python*
//! `create_docs` (out of the Rust-harness scope, by the same-code-under-test
//! doctrine). What IS pure-Rust and load-bearing are the render-tree properties
//! that writer relies on: a language-independent path graph, the English-carrier
//! identity, and the archive-prefix selection. Those are tested here. The disk
//! write itself is trivial sorted-BTreeMap I/O over these guaranteed trees.

use std::collections::BTreeSet;
use std::path::PathBuf;

use gmeow_docs::render::{render_site, render_site_lang};
use gmeow_docs::{DocsModel, Translations};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("crate is at <repo>/crates/docs")
        .to_path_buf()
}

fn live_model() -> DocsModel {
    DocsModel::discover(&repo_root()).expect("build docs model from live slices")
}

#[test]
fn language_trees_share_one_path_graph() {
    // The file/path set is language-independent (only prose changes), so a
    // per-language extract writes the SAME tree shape — the invariant that lets
    // `write_artifacts` / `create_docs` select a language without re-planning the
    // tree, and that keeps every language's links resolving.
    let model = live_model();
    let english: BTreeSet<String> = render_site(&model).files.into_keys().collect();
    for lang in &model.available_languages {
        let keys: BTreeSet<String> = render_site_lang(&model, lang).files.into_keys().collect();
        assert_eq!(
            keys, english,
            "language `{lang}` tree has a different path graph than english"
        );
    }
}

#[test]
fn english_carrier_tree_matches_render_site() {
    // `render_site_lang(model, "english")` is exactly `render_site(model)` — the
    // carrier needs no rewrite, so the extracted English tree is the canonical one.
    let model = live_model();
    assert_eq!(render_site_lang(&model, "english"), render_site(&model));
}

#[test]
fn archive_prefix_maps_language_to_internal_tag() {
    // `create_docs` selects the stored archive by the internal `x-gmeow-*` tag.
    // English is special-cased; an undeclared language defaults to `x-gmeow-<code>`.
    let tr = Translations::from_entries(
        Vec::<((String, String, String), String)>::new(),
        ["fr".to_string()],
    );
    assert_eq!(tr.internal_tag("english"), "x-gmeow-english");
    assert_eq!(tr.internal_tag("fr"), "x-gmeow-fr");
}

#[test]
fn rendered_tree_is_disk_faithful() {
    // Drive the REAL writer `render::write_site` (the pure-Rust core that the
    // PyO3 `DocSet::write_artifacts` wraps) and read every file back: the on-disk
    // bytes are identical to the in-memory Site, and the returned path list
    // matches the tree. Catches a path-join / create_dir / encoding regression in
    // the actual write contract — not a mirrored copy of it.
    let model = live_model();
    let site = render_site_lang(&model, "english");
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("docs-extract-roundtrip");
    let _ = std::fs::remove_dir_all(&root);

    let written = gmeow_docs::render::write_site(&site, &root).expect("write_site");
    assert_eq!(
        written.len(),
        site.files.len(),
        "write_site must return one path per file"
    );
    for (rel, data) in &site.files {
        let got = std::fs::read(root.join(rel)).expect("read back");
        assert_eq!(&got, data, "disk round-trip mismatch for `{rel}`");
    }
    let _ = std::fs::remove_dir_all(&root);
}
