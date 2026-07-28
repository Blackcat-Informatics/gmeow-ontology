// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Once-per-run, content-addressed disk cache for the documentation model.
//!
//! Building a [`DocsModel`] via [`DocsModel::discover`] walks the whole slice
//! catalog, parses every `module.ttl`, and folds the i18n catalogs (~12 s). The
//! gmeow-docs integration suite has ~40 tests that each need the live model, and
//! the test runner executes every test in its own process — so a fresh
//! `discover()` per test is paid dozens of times, and when many start at once the
//! concurrent builds contend and each takes far longer than a single build would.
//!
//! This module builds the model, the rendered site for EVERY available language,
//! and the default mdBook render ONCE and stores each in a content-addressed disk
//! cache; later callers load them cheaply. The English carrier and each translation
//! (`fr`, `zh`, …) are cached symmetrically, and the mdBook source tree
//! ([`render_book`] with default executable data) is cached alongside them, so a
//! per-language render and the book render are each paid once in [`prime`] rather
//! than live in each test process. [`prime`] is run once before the test processes
//! spawn — by the `prime-docs-fixture` example, which the Makefile test lanes and
//! the CI test job invoke immediately before `cargo nextest` — so no test pays the
//! build or any render. [`load`] / [`load_site`] / [`load_site_lang`] / [`load_book`]
//! are the per-process loaders, which also build-and-cache on a genuine miss so a
//! plain `cargo test` (no prime step) still works.
//!
//! The cache key is salted with the crate version and the model schema version,
//! then folds both every input `discover()` reads and the implementation sources
//! that build/serialize/render the fixture. Data, renderer, schema, and local
//! dependency changes therefore invalidate it without relying on a manual version
//! bump. This is the same content-addressed, atomic-temp-then-rename pattern the
//! validate and slice caches use.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::mdbook::render_book;
use crate::render::{Site, render_site_lang};
use gmeow_docs_model::exec::ExecutableDocsData;
use gmeow_docs_model::i18n::{ENGLISH, Translations, UiCatalog};
use gmeow_docs_model::model::DocsModel;

/// Load the live documentation model rooted at `root`, from the once-per-run
/// cache when present, otherwise built via [`DocsModel::discover`] and cached for
/// the rest of the run. Byte-identical to a fresh `discover()` — the three
/// `#[serde(skip)]` i18n fields are carried explicitly in the cache envelope, so
/// localized (`fr` / `zh`) rendering is preserved (not an English fallback).
///
/// A cache file that is PRESENT but unreadable / undeserializable is an integrity
/// violation (a corrupt or partial envelope, or a serde regression) — it panics
/// loudly rather than silently rebuilding and masking it. Only a genuine absence
/// is a legitimate miss that falls through to `discover()`.
pub fn load(root: &Path) -> DocsModel {
    let cache_path = cache_path(root);
    match fs::read(&cache_path) {
        Ok(bytes) => {
            let cached: CachedModel = serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                panic!(
                    "corrupt docs-fixture cache at {}: {e}\n\
                     delete the file (or run `rm -rf .cache/docs-fixture`) to rebuild",
                    cache_path.display()
                )
            });
            cached.into_model()
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => build_and_cache(root, &cache_path),
        Err(e) => panic!(
            "cannot read docs-fixture cache at {}: {e}",
            cache_path.display()
        ),
    }
}

/// Load the rendered English static site rooted at `root` — a thin wrapper over
/// [`load_site_lang`] for the English carrier (`render_site` ≡
/// `render_site_lang(_, "english")`). Byte-identical to a fresh `render_site(&load(root))`.
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
            cached.into_site()
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
    let cache_path = cache_path(root);
    let model = if cache_path.exists() {
        // English is rendered last below, so an existing English site means every
        // translation AND the book for this key are already warm — return without
        // loading the model. The book check makes a pre-book-cache warm directory
        // (English present, book absent) correctly re-prime the book.
        if site_cache_path(root, ENGLISH).exists() && book_cache_path(root).exists() {
            return;
        }
        load(root)
    } else {
        build_and_cache(root, &cache_path)
    };

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

fn build_and_cache(root: &Path, cache_path: &Path) -> DocsModel {
    let model = DocsModel::discover(root).expect("build docs model from live slices");
    write_cache(cache_path, &CachedModel::from_model(&model));
    model
}

/// The on-disk cache path for the model built from the inputs under `root`.
fn cache_path(root: &Path) -> PathBuf {
    let key = cache_key(root);
    root.join(".cache")
        .join("docs-fixture")
        .join(format!("{key}.json"))
}

/// The on-disk cache path for a language's rendered site. Shares the model cache
/// key (the site is a pure function of the model, and a render-logic change is
/// covered by the crate-version salt), with a per-language suffix. The English
/// carrier keeps the bare `.site.json` suffix; every translation is tagged
/// (`.site.fr.json`, `.site.zh.json`, …) so the languages never collide.
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

/// The serialized cache envelope. The model serializes with its i18n fields
/// `#[serde(skip)]`ped (empty in JSON), so the three derived-from-catalog fields
/// are carried alongside it explicitly and reattached on load.
#[derive(Serialize, Deserialize)]
struct CachedModel {
    model: DocsModel,
    available_languages: Vec<String>,
    translations: Translations,
    ui_catalog: UiCatalog,
}

impl CachedModel {
    fn from_model(model: &DocsModel) -> Self {
        Self {
            model: model.clone(),
            available_languages: model.available_languages.clone(),
            translations: model.translations.clone(),
            ui_catalog: model.ui_catalog.clone(),
        }
    }

    fn into_model(self) -> DocsModel {
        let CachedModel {
            mut model,
            available_languages,
            translations,
            ui_catalog,
        } = self;
        model.available_languages = available_languages;
        model.translations = translations;
        model.ui_catalog = ui_catalog;
        model
    }
}

/// The serialized rendered-site envelope. Every emitted file is UTF-8 text (each
/// is `String::into_bytes()` at render time), so the file bytes are carried as
/// JSON strings — far more compact and faster to parse than a `Vec<u8>` number
/// array, with no extra dependency. A non-UTF-8 file would be a render-layer
/// regression and hard-fails loudly on cache write.
#[derive(Serialize, Deserialize)]
struct CachedSite {
    files: BTreeMap<String, String>,
}

impl CachedSite {
    fn from_site(site: &Site) -> Self {
        Self {
            files: site
                .files
                .iter()
                .map(|(path, bytes)| {
                    let text = std::str::from_utf8(bytes)
                        .unwrap_or_else(|e| panic!("rendered site file {path} is not UTF-8: {e}"));
                    (path.clone(), text.to_string())
                })
                .collect(),
        }
    }

    fn into_site(self) -> Site {
        Site {
            files: self
                .files
                .into_iter()
                .map(|(path, text)| (path, text.into_bytes()))
                .collect(),
        }
    }
}

/// Content-address the inputs `discover()` reads. The key folds (in order): a
/// salt of the crate version + model schema version, then the sorted
/// `(relative-path, bytes)` of every file under the discovery and implementation
/// roots. Any slice / shape / i18n / metadata edit changes the key, as does a
/// renderer, model schema, or local model dependency edit.
fn cache_key(root: &Path) -> String {
    let mut hasher = Sha1::new();
    hasher.update(b"gmeow-docs-fixture\x1f");
    hasher.update(env!("CARGO_PKG_VERSION").as_bytes());
    hasher.update(b"\x1f");
    hasher.update(DocsModel::VERSION.as_bytes());
    hasher.update(b"\x1e");

    let mut files: Vec<PathBuf> = Vec::new();
    // Data roots discover() walks recursively. `queries` is the shared repo-root
    // SPARQL tree a `gmeow:cqQueryFile` may resolve into (T2:
    // `apply_competency_query_text`) alongside the per-slice `.rq` files already
    // covered by the `slices` walk. The implementation roots close the stale-cache
    // hole left by the old crate-version-only salt: normal source edits do not bump
    // Cargo's package version on every commit.
    for dir in [
        "slices",
        "shapes",
        "i18n",
        "queries",
        "crates/docs/src",
        "crates/docs/templates",
        "crates/docs/assets",
        "crates/errors/src",
        "crates/logic-compile/src",
        "crates/validate/src",
    ] {
        collect_files(&root.join(dir), &mut files);
    }
    // Individual files discover() reads directly.
    for file in [
        "docs/four-boxes.md",
        "metadata/gmeow-self.ttl",
        "dsl/mappings/mapping-sets.ttl",
        "generated/catalog/constraint-catalog.nq",
        "generated/catalog/term-content-manifest.nq",
        "Cargo.lock",
        "crates/docs/Cargo.toml",
        "crates/errors/Cargo.toml",
        "crates/logic-compile/Cargo.toml",
        "crates/validate/Cargo.toml",
    ] {
        let p = root.join(file);
        if p.is_file() {
            files.push(p);
        }
    }
    files.sort();

    for path in &files {
        let rel = path.strip_prefix(root).unwrap_or(path);
        // Normalize separators so the key is identical across platforms for the
        // same repository state (Windows `\` vs Unix `/` would otherwise diverge).
        let rel = rel.to_string_lossy().replace('\\', "/");
        hasher.update(rel.as_bytes());
        hasher.update(b"\x1f");
        let bytes = fs::read(path)
            .unwrap_or_else(|e| panic!("hashing fixture input {}: {e}", path.display()));
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
        hasher.update(b"\x1e");
    }

    hex(&hasher.finalize())
}

/// Recursively collect every regular file under `dir` (absent dir → no files).
fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(ft) if ft.is_dir() => collect_files(&path, out),
            Ok(ft) if ft.is_file() => out.push(path),
            _ => {}
        }
    }
}

/// Counter making concurrent temp-file names unique within a process.
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Atomically write an envelope: serialize to a uniquely-named temp file in the
/// destination directory, then rename it into place so a concurrent reader never
/// observes a partial JSON object. Mirrors the `.cache/validate` write pattern.
/// Atomic rename overwrites and every writer serializes byte-identical content,
/// so concurrent writers race harmlessly (last rename wins) — there is no benign
/// conflict to absorb. Any write error is therefore a genuine filesystem failure
/// and hard-fails rather than silently degrading to uncached per-process rebuilds.
fn write_cache<T: Serialize>(path: &Path, cached: &T) {
    let dir = path.parent().expect("cache path has a parent");
    if let Err(e) = fs::create_dir_all(dir)
        && e.kind() != std::io::ErrorKind::AlreadyExists
    {
        panic!("creating cache dir {}: {e}", dir.display());
    }
    let bytes = serde_json::to_vec(cached).expect("serialize docs-fixture cache");
    let tmp = dir.join(format!(
        ".{}.{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id(),
        TMP_COUNTER.fetch_add(1, Ordering::Relaxed),
    ));
    let write_result = (|| -> std::io::Result<()> {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if let Err(e) = write_result {
        let _ = fs::remove_file(&tmp);
        panic!("writing docs-fixture cache to {}: {e}", path.display());
    }
}

/// Lowercase hex of a digest.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        // Writing into a `String` is infallible — no per-byte allocation.
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    //! Hermetic tests for the cache machinery itself — no model build (those are
    //! the integration suite's job). They pin the envelope round-trips, the
    //! content-addressing contract, and the integrity-violation panic so a
    //! key/envelope regression fails here, not as a confusing downstream golden.
    use super::*;

    /// A fresh, empty temp root (cache_key over absent discovery roots = salt
    /// only, so these stay cheap). The process id keeps it unique under nextest's
    /// process-per-test model; the tag keeps tests within a process disjoint.
    fn temp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "gmeow-docs-fixture-test-{}-{tag}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create temp root");
        root
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
        assert_eq!(site, CachedSite::from_site(&site).into_site());
    }

    #[test]
    #[should_panic(expected = "is not UTF-8")]
    fn cached_site_rejects_non_utf8_files() {
        let mut files = BTreeMap::new();
        files.insert("bad.bin".to_string(), vec![0xff, 0xfe, 0x00]);
        let _ = CachedSite::from_site(&Site { files });
    }

    #[test]
    fn cache_key_is_deterministic_and_content_sensitive() {
        let root = temp_root("key");
        fs::create_dir_all(root.join("slices")).unwrap();
        fs::write(root.join("slices/a.ttl"), b"v1").unwrap();
        let k1 = cache_key(&root);
        assert_eq!(
            k1,
            cache_key(&root),
            "key must be stable for identical inputs"
        );
        fs::write(root.join("slices/a.ttl"), b"v2").unwrap();
        assert_ne!(
            k1,
            cache_key(&root),
            "key must change when an input byte changes"
        );

        let input_key = cache_key(&root);
        fs::create_dir_all(root.join("crates/docs/src")).unwrap();
        fs::write(root.join("crates/docs/src/render.rs"), b"implementation v1").unwrap();
        assert_ne!(
            input_key,
            cache_key(&root),
            "key must change when fixture implementation bytes change"
        );
    }

    #[test]
    fn model_and_site_cache_paths_share_the_key_with_distinct_suffix() {
        let root = temp_root("paths");
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
        let root = temp_root("lang-paths");
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
    #[should_panic(expected = "corrupt docs-fixture cache")]
    fn present_but_corrupt_model_cache_panics() {
        let root = temp_root("corrupt-model");
        let cp = cache_path(&root);
        fs::create_dir_all(cp.parent().unwrap()).unwrap();
        fs::write(&cp, b"{ not valid json").unwrap();
        let _ = load(&root);
    }

    #[test]
    #[should_panic(expected = "corrupt docs-fixture site cache")]
    fn present_but_corrupt_site_cache_panics() {
        let root = temp_root("corrupt-site");
        let sp = site_cache_path(&root, ENGLISH);
        fs::create_dir_all(sp.parent().unwrap()).unwrap();
        fs::write(&sp, b"{ not valid json").unwrap();
        let _ = load_site(&root);
    }

    #[test]
    #[should_panic(expected = "corrupt docs-fixture book cache")]
    fn present_but_corrupt_book_cache_panics() {
        let root = temp_root("corrupt-book");
        let bp = book_cache_path(&root);
        fs::create_dir_all(bp.parent().unwrap()).unwrap();
        fs::write(&bp, b"{ not valid json").unwrap();
        let _ = load_book(&root);
    }
}
