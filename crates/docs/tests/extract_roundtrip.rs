// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Extract-tree coverage (R4).
//!
//! What IS pure-Rust and load-bearing are the render-tree properties
//! that writer relies on: a language-independent path graph, the English-carrier
//! identity, and the archive-prefix selection. Those are tested here. The disk
//! write itself is trivial sorted-BTreeMap I/O over these guaranteed trees.

// Rich colored line-diffs on assert_eq! failure; shadows the std macro
// for this file. Identical behaviour on pass; insta snapshots are unaffected.
use pretty_assertions::assert_eq;
use std::collections::BTreeSet;
use std::path::PathBuf;

use gmeow_docs::Translations;

mod common;

/// The set of site-relative paths a language's render emits, read from the shared
/// once-per-run cached render (every available language is cached by `prime`, the
/// English carrier and each translation symmetrically) rather than re-rendering.
fn cached_path_graph(lang: &str) -> BTreeSet<String> {
    common::cached_site_lang(lang).files.into_keys().collect()
}

// The file/path set is language-independent (only prose changes), so a
// per-language extract writes the SAME tree shape — the invariant that lets
// `write_artifacts` / `create_docs` select a language without re-planning the
// tree, and that keeps every language's links resolving. The English carrier is
// the canonical graph (`render_site` == `render_site_lang(_, "english")`). Each
// translation's path graph is compared against it below, both read from the shared
// per-language cache. This is NOT a tautology: the fr/zh and English caches are
// produced by INDEPENDENT `render_site_lang(model, lang)` calls in `prime`, so a
// language-dependent path divergence still surfaces as a different key set.

#[test]
fn french_tree_shares_the_english_path_graph() {
    assert_eq!(
        cached_path_graph("fr"),
        cached_path_graph("english"),
        "fr tree has a different path graph than english"
    );
}

#[test]
fn chinese_tree_shares_the_english_path_graph() {
    assert_eq!(
        cached_path_graph("zh"),
        cached_path_graph("english"),
        "zh tree has a different path graph than english"
    );
}

#[test]
fn english_carrier_tree_uses_the_authenticated_site_product() {
    let expected = common::cached_site().files.into_keys().collect();
    assert_eq!(cached_path_graph("english"), expected);
}

#[test]
fn archive_prefix_fallback_uses_default_tag_when_undeclared() {
    // When a catalog declares no internal-tag mapping (the `from_entries` builder
    // leaves it empty), `create_docs`'s archive-prefix selection defaults to
    // `x-gmeow-<code>`; English is special-cased.
    let tr = Translations::from_entries(
        Vec::<((String, String, String), String)>::new(),
        ["fr".to_string()],
    );
    assert_eq!(tr.internal_tag("english"), "x-gmeow-english");
    assert_eq!(tr.internal_tag("fr"), "x-gmeow-fr");
}

#[test]
fn live_model_archive_prefix_uses_declared_bcp47_internal_tags() {
    // The REAL mapping `create_docs` selects on, read from the language slices'
    // declared BCP-47 → internal `x-gmeow-*` pairs — NOT the empty-map fallback.
    // This pins the actual production selection (`fr` → French archive, `zh` →
    // Mandarin archive), which the empty-fixture test above cannot exercise.
    let model = common::cached_model();
    let tr = &model.translations;
    assert_eq!(tr.internal_tag("english"), "x-gmeow-english");
    assert_eq!(
        tr.internal_tag("fr"),
        "x-gmeow-french",
        "fr must resolve to the declared French internal tag, not the fallback"
    );
    assert_eq!(
        tr.internal_tag("zh"),
        "x-gmeow-mandarin",
        "zh must resolve to the declared Mandarin internal tag, not the fallback"
    );
}

#[test]
fn rendered_tree_is_disk_faithful() {
    // Drive the REAL writer `render::write_site` (the pure-Rust core that the
    // PyO3 `DocSet::write_artifacts` wraps) and read every file back: the on-disk
    // bytes are identical to the in-memory Site, and the returned path list
    // matches the tree. Catches a path-join / create_dir / encoding regression in
    // the actual write contract — not a mirrored copy of it.
    let site = common::cached_site();
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
