// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Hermetic tests for the rendered-artifact half of the cache — no model build
//! and no live render (those are the integration suite's job). They pin the site
//! envelope round-trip, the per-artifact cache paths, and the integrity-violation
//! panics. The model envelope, the cache key and the derived implementation
//! closure are pinned beside them in `gmeow_docs_model::fixture`.
use super::*;
use std::fs;

fn model_identity() -> FixtureIdentity {
    FixtureIdentity {
        receipt_digest: "model-receipt".to_string(),
        product_digest: "model-product".to_string(),
        producer: gmeow_action_cache::ProducerIdentity::new("fixture-producer"),
    }
}

/// A fresh, empty temp root (cache_key over absent discovery roots = salt
/// only, so these stay cheap). The root is owned by the returned
/// [`tempfile::TempDir`], which removes the whole tree when it drops — on
/// success, on panic, and on early return. Uniqueness comes from the guard,
/// so the tag is purely a readable name for the root inside it. Callers must
/// bind the guard (`let (_tmp, root) = temp_root("key");`); binding it to a
/// bare `_` drops it immediately and deletes the root out from under the test.
fn temp_root(tag: &str) -> (tempfile::TempDir, PathBuf) {
    let guard = tempfile::tempdir().expect("create temp dir");
    let root = guard.path().join(tag);
    fs::create_dir_all(&root).expect("create temp root");
    (guard, root)
}

#[test]
fn cached_site_round_trips_to_identical_bytes() {
    let mut files = BTreeMap::new();
    files.insert("index.html".to_string(), b"<h1>hi</h1>".to_vec());
    // Multibyte UTF-8 (éèê + a snowman) to prove the string envelope is faithful.
    files.insert(
        "a/b.md".to_string(),
        "# \u{e9}\u{e8}\u{ea} \u{2603}\n".as_bytes().to_vec(),
    );
    let site = Site { files };
    assert_eq!(
        site,
        CachedSite::from_site(&site).into_site(Path::new("<in-memory>"))
    );
}

/// An envelope whose PAYLOAD was edited after it was written is refused, even though
/// it deserializes cleanly and its cache key is untouched.
///
/// This is the whole point of the payload digest: the key content-addresses the
/// INPUTS, so editing the cached OUTPUT leaves it satisfied. `.cache/` is gitignored
/// and persists, so an entry edited once would keep being served.
#[test]
#[should_panic(expected = "tampered docs-fixture site cache")]
fn an_edited_site_envelope_is_refused() {
    let mut files = BTreeMap::new();
    files.insert("index.html".to_string(), b"<h1>hi</h1>".to_vec());
    let mut cached = CachedSite::from_site(&Site { files });
    // The hand-edit: rewrite a cached page, leave the declared digest alone.
    cached
        .files
        .insert("index.html".to_string(), "<h1>edited</h1>".to_string());
    let _ = cached.into_site(Path::new("<in-memory>"));
}

/// **The write→read proof, across the real serde boundary.** An envelope written by
/// [`ActionStore`] and read back by the loader that serves warm hits must VERIFY.
///
/// The in-memory `from_site(..).into_site(..)` round trip above cannot see this
/// class: the digest is folded over a re-serialization of the payload, so a payload
/// field that does not survive JSON — one whose `skip_serializing_if` has no matching
/// `default`, or a map whose iteration order is not the wire order — folds to one
/// value before the write and another after the read, and the guard fires on every
/// warm hit even though nothing was edited. That is exactly the failure the SHACL
/// verdict's own self-digest shipped with (its digest was folded over the pre-render
/// report while the file carried the normalized one), so the docs fixture's analogous
/// guard is proven here rather than assumed. The model envelope's twin of this proof
/// runs beside it in `gmeow_docs_model::fixture`.
#[test]
fn a_site_envelope_written_to_disk_verifies_when_read_back() {
    let (_tmp, root) = temp_root("disk-round-trip");

    let identity = model_identity();
    let path = site_cache_path(&root, ENGLISH, &identity);
    let mut files = BTreeMap::new();
    files.insert("index.html".to_string(), b"<h1>hi</h1>".to_vec());
    files.insert(
        "a/b.md".to_string(),
        "# \u{e9}\u{e8}\u{ea} \u{2603}\n".as_bytes().to_vec(),
    );
    let built = Site { files };
    let cold = load_or_build_cached_site(&root, "site", Some(ENGLISH), &identity, || built.clone());
    assert_eq!(cold, built, "the cold miss returns the built site");
    assert!(path.is_file(), "the miss wrote the envelope");
    let warm = load_or_build_cached_site(&root, "site", Some(ENGLISH), &identity, || {
        panic!("the warm hit must be served from disk, not rebuilt")
    });
    assert_eq!(
        warm, built,
        "the warm hit verifies its payload digest and reconstructs the site"
    );
}

#[test]
fn selected_render_uses_producer_identity_and_refuses_other_languages_or_receipts() {
    let (_tmp, root) = temp_root("selected-render");
    let identity = model_identity();
    let store = action_store(&root);
    let site = Site {
        files: BTreeMap::from([("index.html".to_string(), b"selected".to_vec())]),
    };
    let (_, _, selected) =
        load_or_build_cached_site_in_store(&store, &root, "site", Some(ENGLISH), &identity, || {
            site.clone()
        });
    fs::create_dir_all(root.join("crates/docs/src")).unwrap();
    fs::write(
        root.join("crates/docs/src/lib.rs"),
        b"consumer-only source edit",
    )
    .unwrap();
    assert_eq!(
        load_selected_site(&root, "site", Some(ENGLISH), &identity, &selected),
        site
    );
    assert!(
        std::panic::catch_unwind(|| {
            load_selected_site(&root, "site", Some("fr"), &identity, &selected)
        })
        .is_err()
    );
    let mut wrong = selected;
    wrong.receipt_digest = "0".repeat(64);
    assert!(
        std::panic::catch_unwind(|| {
            load_selected_site(&root, "site", Some(ENGLISH), &identity, &wrong)
        })
        .is_err()
    );
}

#[test]
#[should_panic(expected = "is not UTF-8")]
fn cached_site_rejects_non_utf8_files() {
    let mut files = BTreeMap::new();
    files.insert("bad.bin".to_string(), vec![0xff, 0xfe, 0x00]);
    let _ = CachedSite::from_site(&Site { files });
}

#[test]
fn model_site_and_book_share_the_store_without_key_aliases() {
    let (_tmp, root) = temp_root("paths");
    let identity = model_identity();
    let model = gmeow_docs_model::fixture::cache_path(&root);
    let site = site_cache_path(&root, ENGLISH, &identity);
    let book = book_cache_path(&root, &identity);
    assert_eq!(model.parent(), site.parent());
    assert_eq!(site.parent(), book.parent());
    assert_ne!(model, site);
    assert_ne!(site, book);
    assert_ne!(book, site_cache_path(&root, "book", &identity));
}

#[test]
fn language_is_a_first_class_render_action_dimension() {
    let (_tmp, root) = temp_root("lang-paths");
    let identity = model_identity();
    assert_ne!(
        site_cache_path(&root, "fr", &identity),
        site_cache_path(&root, "zh", &identity),
        "distinct languages must not share a site cache path"
    );
    assert_ne!(
        site_cache_path(&root, ENGLISH, &identity),
        site_cache_path(&root, "fr", &identity)
    );
}

#[test]
#[should_panic(expected = "corrupt docs-fixture site action cache")]
fn present_but_corrupt_site_cache_panics() {
    let (_tmp, root) = temp_root("corrupt-site");
    let identity = model_identity();
    let sp = site_cache_path(&root, ENGLISH, &identity);
    fs::create_dir_all(sp.parent().unwrap()).unwrap();
    fs::write(&sp, b"{ not valid json").unwrap();
    let _ = load_or_build_cached_site(&root, "site", Some(ENGLISH), &identity, || {
        panic!("a present corrupt action must not rebuild")
    });
}

#[test]
#[should_panic(expected = "corrupt docs-fixture book action cache")]
fn present_but_corrupt_book_cache_panics() {
    let (_tmp, root) = temp_root("corrupt-book");
    let identity = model_identity();
    let bp = book_cache_path(&root, &identity);
    fs::create_dir_all(bp.parent().unwrap()).unwrap();
    fs::write(&bp, b"{ not valid json").unwrap();
    let _ = load_or_build_cached_site(&root, "book", None, &identity, || {
        panic!("a present corrupt action must not rebuild")
    });
}
