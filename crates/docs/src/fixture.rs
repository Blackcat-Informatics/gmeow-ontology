// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Once-per-run, content-addressed disk cache for the RENDERED documentation
//! artifacts — the per-language static site and the default mdBook source tree.
//!
//! The MODEL half of this fixture lives in [`gmeow_docs_model::fixture`]: the cache
//! key, the payload digest, the atomic writer, and [`gmeow_docs_model::fixture::load`]
//! itself. It sits there because it must be reachable from every model consumer
//! without linking the renderer — `gmeow-slice-quality`'s `DocMaturity` axis reads it,
//! and an edge from that crate to this one would close a first-party dependency cycle
//! (`gmeow-docs` dev-depends on `gmeow-mcp`, which depends on `gmeow-slice-quality`).
//! What stays here is exactly what needs [`Site`], [`render_site_lang`] and
//! [`render_book`]. Both halves share ONE key, ONE digest function and ONE writer, all
//! exported by the model crate — a second copy of any of them would be the
//! two-sources-of-truth defect the digest exists to catch.
//!
//! A [`DocsModel`] build is a ~12 s repo-wide walk, and rendering the site on top of it
//! is more; the gmeow-docs integration suite has ~40 tests that each need one or both,
//! and the test runner executes every test in its own process — so a fresh build and
//! render per test is paid dozens of times, and when many start at once the concurrent
//! builds contend and each takes far longer than a single build would.
//!
//! This module renders the site for EVERY available language and the default mdBook
//! render ONCE and stores each in a content-addressed disk cache; later callers load
//! them cheaply. The English carrier and each translation (`fr`, `zh`, …) are cached
//! symmetrically, and the mdBook source tree ([`render_book`] with default executable
//! data) is cached alongside them, so a per-language render and the book render are
//! each paid once in [`prime`] rather than live in each test process. [`prime`] is run
//! once before the test processes spawn — by the `prime-docs-fixture` example, which
//! the Makefile test lanes and the CI test job invoke immediately before
//! `cargo nextest` — so no test pays the build or any render.
//! [`load_site`] / [`load_site_lang`] / [`load_book`] are the per-process loaders,
//! which also render-and-cache on a genuine miss so a plain `cargo test` (no prime
//! step) still works.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::mdbook::render_book;
use crate::render::{Site, render_site_lang};
use gmeow_docs_model::exec::ExecutableDocsData;
use gmeow_docs_model::fixture::{
    cache_key, cache_path, load, payload_digest, verify_payload, write_cache,
};
use gmeow_docs_model::i18n::ENGLISH;
use gmeow_docs_model::model::DocsModel;

/// Load the rendered English static site rooted at `root` — a thin wrapper over
/// [`load_site_lang`] for the English carrier (`render_site` ≡
/// `render_site_lang(_, "english")`). Byte-identical to a fresh `render_site(&load(root))`.
#[must_use]
pub fn load_site(root: &Path) -> Site {
    load_site_lang(root, ENGLISH)
}

/// Load the rendered static site for `lang` rooted at `root`, from the once-per-run
/// cache when present, otherwise rendered via [`render_site_lang`] and cached for
/// the rest of the run. Byte-identical to a fresh `render_site_lang(&load(root), lang)`.
///
/// Every gated test that needs a rendered site — determinism checks, the
/// carrier-vs-`render_site` identity, per-language path-graph comparisons, lint
/// passes — loads it from here instead of paying a fresh render. That removes the
/// dominant per-test cost (a full site render) and the cross-process render
/// contention that pushed those tests over the gate. The English carrier and each
/// translation are cached symmetrically, so the `fr` / `zh` round-trip tests pay no
/// live render either.
///
/// Corrupt-but-present is an integrity violation and panics; only a genuine
/// absence falls through to a fresh render (so a plain `cargo test` still works).
#[must_use]
pub fn load_site_lang(root: &Path, lang: &str) -> Site {
    // Reuse the warm model cache (built first by `prime`, or built-and-cached on a
    // plain `cargo test` miss) rather than re-walking the slices.
    load_cached_site(&site_cache_path(root, lang), "site", || {
        render_site_lang(&load(root), lang)
    })
}

/// Load the default mdBook render (the mdBook `src/` source tree —
/// `book.toml`, `SUMMARY.md`, and one `src/<page>/index.md` per page) rooted at
/// `root`, from the once-per-run cache when present, otherwise rendered via
/// [`render_book`] with default executable data and cached for the rest of the run.
/// Byte-identical to a fresh `render_book(&load(root), &ExecutableDocsData::default())`.
///
/// This is a distinct artifact from [`load_site`] — the static HTML site and the
/// mdBook source tree share the `Site` type but not their contents — so it lives at
/// its own cache path. The default book render is language-agnostic, so unlike the
/// per-language site there is no `lang` component. Every gated `mdbook_render` test
/// that needs the default book loads it from here instead of paying a fresh render.
///
/// Corrupt-but-present is an integrity violation and panics; only a genuine absence
/// falls through to a fresh render (so a plain `cargo test` still works).
#[must_use]
pub fn load_book(root: &Path) -> Site {
    load_cached_site(&book_cache_path(root), "book", || {
        render_book(&load(root), &ExecutableDocsData::default())
    })
}

/// Shared loader for a [`CachedSite`]-envelope artifact: load from `cache_path`
/// when present, else `build` it and cache for the rest of the run. `label` names
/// the artifact in diagnostics (`"site"` / `"book"`). A cache file that is PRESENT
/// but undeserializable is an integrity violation and panics loudly rather than
/// silently rebuilding and masking it; only a genuine absence (`NotFound`) is a
/// legitimate miss that falls through to `build`. This is the single authority for
/// the site/book integrity contract — do not reintroduce a per-artifact copy.
fn load_cached_site(cache_path: &Path, label: &str, build: impl FnOnce() -> Site) -> Site {
    match fs::read(cache_path) {
        Ok(bytes) => {
            let cached: CachedSite = serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                panic!(
                    "corrupt docs-fixture {label} cache at {}: {e}\n\
                     delete the file (or run `rm -rf .cache/docs-fixture`) to rebuild",
                    cache_path.display()
                )
            });
            cached.into_site(cache_path)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let site = build();
            write_cache(cache_path, &CachedSite::from_site(&site));
            site
        }
        Err(e) => panic!(
            "cannot read docs-fixture {label} cache at {}: {e}",
            cache_path.display()
        ),
    }
}

/// Build the model, the rendered site for every available language, and the
/// default mdBook render, writing each cache if it is not already present. Run
/// once before a batch of tests so none of them pays the (contended) model build
/// or any render.
///
/// The fully-warm path is a pure stat-check (a few `exists()` calls, no model
/// deserialize and no render): the English site is written LAST, so its presence
/// is the sentinel that the whole per-language set AND the book for this cache key
/// are on disk. Only a cold or interrupted-partial cache loads the model — once —
/// to enumerate `available_languages` and render the missing artifacts.
pub fn prime(root: &Path) {
    // English is rendered last below, so an existing English site means every
    // translation AND the book for this key are already warm — return without
    // loading the model. The book check makes a pre-book-cache warm directory
    // (English present, book absent) correctly re-prime the book.
    if cache_path(root).exists()
        && site_cache_path(root, ENGLISH).exists()
        && book_cache_path(root).exists()
    {
        return;
    }
    // Cold, or interrupted-partial: `load` builds-and-caches the model on a genuine
    // miss and deserializes the warm entry otherwise — one authority for both.
    let model: DocsModel = load(root);

    // Render every translation first, then the book, then the English carrier last
    // so the sentinel above only becomes true once the complete set is on disk.
    for lang in &model.available_languages {
        if lang == ENGLISH {
            continue;
        }
        let path = site_cache_path(root, lang);
        if !path.exists() {
            let site = render_site_lang(&model, lang);
            write_cache(&path, &CachedSite::from_site(&site));
        }
    }
    // The book cache is written BEFORE the English-site sentinel below, so
    // English-site-present ⇒ book-present. Do not reorder these two writes.
    let book_path = book_cache_path(root);
    if !book_path.exists() {
        let book = render_book(&model, &ExecutableDocsData::default());
        write_cache(&book_path, &CachedSite::from_site(&book));
    }
    let english_path = site_cache_path(root, ENGLISH);
    let english = render_site_lang(&model, ENGLISH);
    write_cache(&english_path, &CachedSite::from_site(&english));
}

/// The on-disk cache path for a language's rendered site. Shares the model cache
/// key (the site is a pure function of the model, and a render-logic change is
/// covered by the crate-version salt and by the `crates/docs` implementation closure
/// [`cache_key`] folds), with a per-language suffix. The English carrier keeps the
/// bare `.site.json` suffix; every translation is tagged (`.site.fr.json`,
/// `.site.zh.json`, …) so the languages never collide.
fn site_cache_path(root: &Path, lang: &str) -> PathBuf {
    let key = cache_key(root);
    let name = if lang == ENGLISH {
        format!("{key}.site.json")
    } else {
        format!("{key}.site.{lang}.json")
    };
    root.join(".cache").join("docs-fixture").join(name)
}

/// The on-disk cache path for the default mdBook render. Shares the model cache
/// key (the book is a pure function of the model, and a render-logic change is
/// covered by the crate-version salt and the hashed `crates/docs/src` tree). The
/// `.book.json` suffix keeps it distinct from the model (`.json`) and the site
/// (`.site.json` / `.site.<lang>.json`) caches. The default book render is
/// language-agnostic, so there is no per-language component.
fn book_cache_path(root: &Path) -> PathBuf {
    let key = cache_key(root);
    root.join(".cache")
        .join("docs-fixture")
        .join(format!("{key}.book.json"))
}

/// The serialized rendered-site envelope. Every emitted file is UTF-8 text (each
/// is `String::into_bytes()` at render time), so the file bytes are carried as
/// JSON strings — far more compact and faster to parse than a `Vec<u8>` number
/// array, with no extra dependency. A non-UTF-8 file would be a render-layer
/// regression and hard-fails loudly on cache write.
///
/// The digest is folded by [`payload_digest`] and checked by [`verify_payload`], the
/// same two functions the model envelope uses.
#[derive(Serialize, Deserialize)]
struct CachedSite {
    digest: String,
    files: BTreeMap<String, String>,
}

impl CachedSite {
    fn from_site(site: &Site) -> Self {
        let files: BTreeMap<String, String> = site
            .files
            .iter()
            .map(|(path, bytes)| {
                let text = std::str::from_utf8(bytes)
                    .unwrap_or_else(|e| panic!("rendered site file {path} is not UTF-8: {e}"));
                (path.clone(), text.to_string())
            })
            .collect();
        Self {
            digest: payload_digest("site", &files),
            files,
        }
    }

    /// Reconstruct the site, first proving the envelope carries the files it claims.
    fn into_site(self, cache_path: &Path) -> Site {
        verify_payload(cache_path, "site", &self.digest, &self.files);
        Site {
            files: self
                .files
                .into_iter()
                .map(|(path, text)| (path, text.into_bytes()))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    //! Hermetic tests for the rendered-artifact half of the cache — no model build
    //! and no live render (those are the integration suite's job). They pin the site
    //! envelope round-trip, the per-artifact cache paths, and the integrity-violation
    //! panics. The model envelope, the cache key and the derived implementation
    //! closure are pinned beside them in `gmeow_docs_model::fixture`.
    use super::*;

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
    /// [`write_cache`] and read back by the loader that serves warm hits must VERIFY.
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

        // The loader writes on the miss, then serves — and verifies — the warm hit.
        let path = root.join(".cache/docs-fixture/site.json");
        let mut files = BTreeMap::new();
        files.insert("index.html".to_string(), b"<h1>hi</h1>".to_vec());
        files.insert(
            "a/b.md".to_string(),
            "# \u{e9}\u{e8}\u{ea} \u{2603}\n".as_bytes().to_vec(),
        );
        let built = Site { files };
        let cold = load_cached_site(&path, "site", || built.clone());
        assert_eq!(cold, built, "the cold miss returns the built site");
        assert!(path.is_file(), "the miss wrote the envelope");
        let warm = load_cached_site(&path, "site", || {
            panic!("the warm hit must be served from disk, not rebuilt")
        });
        assert_eq!(
            warm, built,
            "the warm hit verifies its payload digest and reconstructs the site"
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
    fn model_and_site_cache_paths_share_the_key_with_distinct_suffix() {
        let (_tmp, root) = temp_root("paths");
        let key = cache_key(&root);
        assert_eq!(
            cache_path(&root).file_name().unwrap().to_string_lossy(),
            format!("{key}.json")
        );
        assert_eq!(
            site_cache_path(&root, ENGLISH)
                .file_name()
                .unwrap()
                .to_string_lossy(),
            format!("{key}.site.json")
        );
        // The book cache shares the key with its own `.book.json` suffix.
        assert_eq!(
            book_cache_path(&root)
                .file_name()
                .unwrap()
                .to_string_lossy(),
            format!("{key}.book.json")
        );
        // A hypothetical language literally named "book" renders `.site.book.json`,
        // which must NOT alias the book cache's `.book.json`.
        assert_ne!(
            book_cache_path(&root),
            site_cache_path(&root, "book"),
            "the book cache must not collide with a site named \"book\""
        );
    }

    #[test]
    fn per_language_site_paths_are_tagged_and_distinct() {
        let (_tmp, root) = temp_root("lang-paths");
        let key = cache_key(&root);
        // English keeps the bare suffix; translations are tagged by language.
        assert_eq!(
            site_cache_path(&root, ENGLISH)
                .file_name()
                .unwrap()
                .to_string_lossy(),
            format!("{key}.site.json")
        );
        assert_eq!(
            site_cache_path(&root, "fr")
                .file_name()
                .unwrap()
                .to_string_lossy(),
            format!("{key}.site.fr.json")
        );
        assert_ne!(
            site_cache_path(&root, "fr"),
            site_cache_path(&root, "zh"),
            "distinct languages must not share a site cache path"
        );
    }

    #[test]
    #[should_panic(expected = "corrupt docs-fixture site cache")]
    fn present_but_corrupt_site_cache_panics() {
        let (_tmp, root) = temp_root("corrupt-site");
        let sp = site_cache_path(&root, ENGLISH);
        fs::create_dir_all(sp.parent().unwrap()).unwrap();
        fs::write(&sp, b"{ not valid json").unwrap();
        let _ = load_site(&root);
    }

    #[test]
    #[should_panic(expected = "corrupt docs-fixture book cache")]
    fn present_but_corrupt_book_cache_panics() {
        let (_tmp, root) = temp_root("corrupt-book");
        let bp = book_cache_path(&root);
        fs::create_dir_all(bp.parent().unwrap()).unwrap();
        fs::write(&bp, b"{ not valid json").unwrap();
        let _ = load_book(&root);
    }
}
