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
use gmeow_docs::render::render_site_lang;

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
fn english_carrier_tree_matches_render_site() {
    // `render_site_lang(model, "english")` is exactly `render_site(model)` — the
    // carrier needs no rewrite, so the extracted English tree is the canonical one.
    // Compared against the shared cached render (which IS `render_site`).
    let model = common::cached_model();
    assert_eq!(render_site_lang(&model, "english"), common::cached_site());
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

/// A site of literal bytes — enough to exercise the WRITER without rendering a model.
fn literal_site(files: &[(&str, &str)]) -> gmeow_docs::Site {
    gmeow_docs::Site {
        files: files
            .iter()
            .map(|(rel, data)| ((*rel).to_string(), (*data).as_bytes().to_vec()))
            .collect(),
    }
}

#[test]
fn rewriting_a_tree_removes_what_the_producer_stopped_emitting() {
    // The writer RECONCILES: assembling over a previous tree must leave exactly the
    // current emission behind. A writer that only adds keeps serving a file the current
    // build does not produce — which is how a dev-only scaffold went on being deployed
    // after it had been dropped from the console producer's file set. That is a wrong
    // shipped artifact, so it is asserted on the writer rather than left to a `rm -rf`
    // at whichever call site remembered one.
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("docs-write-site-reconcile");
    let _ = std::fs::remove_dir_all(&root);

    let first = literal_site(&[
        ("console/index.html", "<!doctype html>"),
        ("console/smoke/package.json", "{}"),
        ("console/smoke/nested/package-lock.json", "{}"),
        ("assets/gmeow.gts", "bundle-v1"),
    ]);
    gmeow_docs::render::write_site(&first, &root).expect("first write");
    assert!(root.join("console/smoke/package.json").is_file());

    // A file the producer no longer emits, a whole subtree it no longer emits, and a
    // file it still emits with CHANGED bytes.
    let second = literal_site(&[
        ("console/index.html", "<!doctype html>"),
        ("assets/gmeow.gts", "bundle-v2"),
    ]);
    let written = gmeow_docs::render::write_site(&second, &root).expect("second write");

    assert_eq!(
        written.len(),
        2,
        "the writer reports exactly what it emitted"
    );
    assert!(
        !root.join("console/smoke/package.json").exists(),
        "a file the producer stopped emitting must not survive a re-write"
    );
    assert!(
        !root.join("console/smoke").exists(),
        "the directory emptied by that removal must go with it"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("assets/gmeow.gts")).expect("re-read"),
        "bundle-v2",
        "a still-emitted file carries the new bytes"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("console/index.html")).expect("re-read"),
        "<!doctype html>"
    );

    // Reconciliation is total: the on-disk tree IS the emitted key set, nothing more.
    let mut on_disk = BTreeSet::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("walk") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                stack.push(path);
            } else {
                on_disk.insert(
                    path.strip_prefix(&root)
                        .expect("under root")
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    assert_eq!(
        on_disk,
        second.files.keys().cloned().collect::<BTreeSet<_>>(),
        "the reconciled tree must be exactly the emitted set"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn reconciling_never_reaches_outside_the_output_tree() {
    // The pruning walk classifies with `symlink_metadata`, so a link is unlinked as the
    // link it is: the link goes, its TARGET does not, and a link to a directory is not a
    // way to descend out of the tree the writer was asked to write.
    let base = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("docs-write-site-symlink");
    let _ = std::fs::remove_dir_all(&base);
    let outside = base.join("outside");
    let root = base.join("tree");
    std::fs::create_dir_all(outside.join("dir")).expect("mkdir outside");
    std::fs::write(outside.join("dir/keep.txt"), b"untouched").expect("write outside");

    let site = literal_site(&[("console/index.html", "<!doctype html>")]);
    gmeow_docs::render::write_site(&site, &root).expect("first write");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.join("dir"), root.join("console/escape")).expect("link");
        std::os::unix::fs::symlink(outside.join("dir/keep.txt"), root.join("stale-link"))
            .expect("link");
    }

    gmeow_docs::render::write_site(&site, &root).expect("second write");

    #[cfg(unix)]
    {
        assert!(
            !root.join("console/escape").exists() && !root.join("stale-link").exists(),
            "a stale link inside the tree is removed"
        );
    }
    assert_eq!(
        std::fs::read_to_string(outside.join("dir/keep.txt")).expect("outside survives"),
        "untouched",
        "nothing outside the output tree may be removed"
    );
    assert!(outside.join("dir").is_dir());

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn an_empty_site_is_refused_rather_than_emptying_the_tree() {
    // Reconciling nothing would empty the destination. A producer that emitted zero files
    // has failed to produce, so the writer refuses rather than turning that failure into a
    // deletion.
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("docs-write-site-empty");
    let _ = std::fs::remove_dir_all(&root);
    gmeow_docs::render::write_site(&literal_site(&[("console/index.html", "x")]), &root)
        .expect("first write");

    let error = gmeow_docs::render::write_site(&literal_site(&[]), &root)
        .expect_err("an empty site must be refused");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert!(
        root.join("console/index.html").is_file(),
        "the refused write must leave the tree exactly as it found it"
    );

    let _ = std::fs::remove_dir_all(&root);
}
