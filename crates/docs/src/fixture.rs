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

use crate::exec::ExecutableDocsData;
use crate::i18n::{ENGLISH, Translations, UiCatalog};
use crate::mdbook::render_book;
use crate::model::{DocsError, DocsModel};
use crate::render::{Site, render_site_lang};

/// Load the live documentation model rooted at `root`, from the once-per-run
/// cache when present, otherwise built via [`DocsModel::discover`] and cached for
/// the rest of the run. Byte-identical to a fresh `discover()` — the three
/// `#[serde(skip)]` i18n fields are carried explicitly in the cache envelope, so
/// localized (`fr` / `zh`) rendering is preserved (not an English fallback).
///
/// A cache file that is PRESENT but unreadable / undeserializable is an integrity
/// violation (a corrupt or partial envelope, or a serde regression) — it panics
/// loudly rather than silently rebuilding and masking it. So is a file that
/// deserializes but does not fold to the payload digest it carries: an entry EDITED
/// after it was written (see [`verify_payload`]). Only a genuine absence is a
/// legitimate miss that falls through to `discover()`.
pub fn load(root: &Path) -> DocsModel {
    try_load(root).unwrap_or_else(|e| panic!("build docs model from live slices: {e}"))
}

/// [`load`], but surfacing a model-BUILD failure as `Err` instead of panicking.
///
/// Same cache, same key, same integrity contract: a cache file that is present but
/// undeserializable still panics (that is corruption, not an honest absence), and a
/// genuine miss still builds and caches. The only difference is the disposition of a
/// [`DocsModel::discover`] error, which some callers must report as a first-class
/// finding rather than a crash — `gmeow-slice-quality`'s DocMaturity axis records it
/// as `slice-quality.doc-maturity.model-unavailable`, and swapping that for a panic
/// would turn a recorded, gradeable condition into a dead process.
pub fn try_load(root: &Path) -> Result<DocsModel, DocsError> {
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
            Ok(cached.into_model(&cache_path))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let model = DocsModel::discover(root)?;
            write_cache(&cache_path, &CachedModel::from_model(&model));
            Ok(model)
        }
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
    let model = load(root);

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

/// The digest an envelope carries over its OWN payload, and the guard that refuses a
/// payload which does not fold to it.
///
/// The cache KEY content-addresses the INPUTS: it proves the entry was built from these
/// slices, these shapes, this renderer. It says nothing about the entry, so an envelope
/// edited on disk — `.cache/` is gitignored and persists across every branch — is served
/// verbatim as if the model builder had produced it. The `DocMaturity` quality axis reads
/// its whole coverage computation out of this cache, so an edited model is an edited
/// grade. The payload digest closes that: the key says WHICH INPUTS, the digest says WHAT
/// WAS CACHED, and a warm read requires both.
///
/// It is a fold over the payload's re-serialization rather than the raw file bytes, so it
/// is invariant to JSON formatting while sensitive to every value a reader consumes.
fn payload_digest<T: Serialize>(label: &str, payload: &T) -> String {
    let bytes = serde_json::to_vec(payload)
        .unwrap_or_else(|e| panic!("serializing the {label} cache payload for its digest: {e}"));
    let mut hasher = Sha1::new();
    hasher.update(b"gmeow-docs-fixture-payload\x1f");
    hasher.update(label.as_bytes());
    hasher.update(b"\x1e");
    hasher.update(&bytes);
    hex(&hasher.finalize())
}

/// Hard-fail unless `payload` folds to the `declared` digest its envelope carries.
///
/// An edited cache entry is corruption of the same class as an undeserializable one, and
/// is treated identically: panic naming the file, never a silent rebuild that would mask
/// it and never a quiet acceptance of the edited values.
fn verify_payload<T: Serialize>(cache_path: &Path, label: &str, declared: &str, payload: &T) {
    let live = payload_digest(label, payload);
    assert!(
        live == declared,
        "tampered docs-fixture {label} cache at {}: it declares payload digest {declared} but \
         its content folds to {live}. The entry was edited after it was written — delete the \
         file (or run `rm -rf .cache/docs-fixture`) to rebuild; an edited cache entry is never \
         served",
        cache_path.display(),
    );
}

/// The serialized cache envelope: the payload plus a digest OVER that payload.
///
/// The model serializes with its i18n fields `#[serde(skip)]`ped (empty in JSON), so the
/// three derived-from-catalog fields are carried alongside it explicitly and reattached on
/// load. `digest` is `#[serde(skip)]`ped OUT of the digested body by construction — it
/// lives on the envelope, the body is what gets folded — so the fold has nothing circular
/// in it.
#[derive(Serialize, Deserialize)]
struct CachedModel {
    digest: String,
    body: CachedModelBody,
}

/// The digested half of [`CachedModel`] — everything a loader reconstructs the model from.
#[derive(Serialize, Deserialize)]
struct CachedModelBody {
    model: DocsModel,
    available_languages: Vec<String>,
    translations: Translations,
    ui_catalog: UiCatalog,
}

impl CachedModel {
    fn from_model(model: &DocsModel) -> Self {
        let body = CachedModelBody {
            model: model.clone(),
            available_languages: model.available_languages.clone(),
            translations: model.translations.clone(),
            ui_catalog: model.ui_catalog.clone(),
        };
        Self {
            digest: payload_digest("model", &body),
            body,
        }
    }

    /// Reconstruct the model, first proving the envelope carries the payload it claims.
    fn into_model(self, cache_path: &Path) -> DocsModel {
        verify_payload(cache_path, "model", &self.digest, &self.body);
        let CachedModelBody {
            mut model,
            available_languages,
            translations,
            ui_catalog,
        } = self.body;
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

/// Every workspace crate whose Rust sources can execute inside the model build or
/// a render — the TRANSITIVE `path = "../…"` dependency closure of `gmeow-docs`,
/// `gmeow-docs` itself included.
///
/// A path dependency carries NO content hash in `Cargo.lock` (unlike a registry or
/// git dependency, which is pinned by checksum/rev), so editing one changes what
/// `discover()` computes while leaving every other hashed input byte-identical. A
/// cache keyed on less than this closure therefore serves a stale model across such
/// an edit — and `.cache/` is gitignored and persists, so the stale entry survives
/// indefinitely. The closure — not a hand-picked subset — is the only set that is
/// sound by construction: it needs no per-call-site argument about which imported
/// symbol "really" affects the model, and `crate_dep_closure_is_fully_hashed`
/// re-derives it from the workspace manifests so a NEW path dependency reds a test
/// instead of silently opening the hole again.
///
/// Sorted; each entry's `src/` tree and `Cargo.toml` are folded into the key.
const HASHED_CRATE_ROOTS: &[&str] = &[
    "affect-ingest",
    "cost-measure",
    "docs",
    "errors",
    "gts-profile",
    "lang-bridge",
    "lang-form",
    "license",
    "logic",
    "logic-compile",
    "math",
    "math-lift",
    "ns",
    "query-wasm",
    "term-arena",
    "validate",
];

/// The repo-root-relative directories a `gmeow:cqQueryFile` may resolve into.
///
/// This is BOTH the authoring contract `crates/slicetest/src/paths.rs::query_file`
/// documents (a shared `queries/…` tree or a slice's own
/// `slices/<group>/<name>/queries/…`) AND the soundness boundary of
/// [`cache_key`]: every path under these roots is folded into the key, so a
/// competency query's TEXT can never change behind the cache's back.
/// `apply_competency_query_text` hard-fails on a `cqQueryFile` outside them —
/// see [`crate::model::COMPETENCY_QUERY_ROOTS`], which this mirrors and the
/// `competency_query_roots_are_hashed` test pins.
const COMPETENCY_QUERY_ROOTS: &[&str] = crate::model::COMPETENCY_QUERY_ROOTS;

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
    // covered by the `slices` walk — and `COMPETENCY_QUERY_ROOTS` is the enforced
    // boundary that keeps that resolution inside this hashed set.
    for dir in COMPETENCY_QUERY_ROOTS
        .iter()
        .map(|r| r.trim_end_matches('/'))
        .chain(["shapes", "i18n"])
    {
        collect_files(&root.join(dir), &mut files);
    }
    // gmeow-docs' own non-source render inputs (templates + assets).
    for dir in ["crates/docs/templates", "crates/docs/assets"] {
        collect_files(&root.join(dir), &mut files);
    }
    // The implementation roots close the stale-cache hole left by the old
    // crate-version-only salt (normal source edits do not bump Cargo's package
    // version on every commit) AND the path-dependency hole: a `path = "../…"`
    // dependency carries no content hash in `Cargo.lock`, so nothing else in the
    // key moves when one is edited. See `HASHED_CRATE_ROOTS`.
    for krate in HASHED_CRATE_ROOTS {
        collect_files(&root.join("crates").join(krate).join("src"), &mut files);
        let manifest = root.join("crates").join(krate).join("Cargo.toml");
        if manifest.is_file() {
            files.push(manifest);
        }
    }
    // Individual files discover() reads directly, plus `Cargo.lock` — which pins
    // every REGISTRY and GIT dependency by checksum/rev (purrdf included), the
    // half of the dependency graph `HASHED_CRATE_ROOTS` does not cover.
    for file in [
        "docs/four-boxes.md",
        "metadata/gmeow-self.ttl",
        "dsl/mappings/mapping-sets.ttl",
        "generated/catalog/constraint-catalog.nq",
        "generated/catalog/term-content-manifest.nq",
        "Cargo.lock",
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
    /// and persists, so an entry edited once would keep being served — and the
    /// `DocMaturity` quality axis reads its coverage computation straight out of it.
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
    /// guard is proven here rather than assumed. Both envelope kinds are driven through
    /// the real writer and the real loader, in one process.
    #[test]
    fn an_envelope_written_to_disk_verifies_when_read_back() {
        let (_tmp, root) = temp_root("disk-round-trip");

        // Site: the loader writes on the miss, then serves — and verifies — the warm hit.
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

        // Model: written by the same writer, read by the same deserialize + verify path
        // `try_load` takes on a warm hit.
        let model_path = root.join(".cache/docs-fixture/model.json");
        let model = DocsModel::default();
        write_cache(&model_path, &CachedModel::from_model(&model));
        let bytes = fs::read(&model_path).expect("read back the model envelope");
        let cached: CachedModel = serde_json::from_slice(&bytes).expect("the envelope parses");
        let recovered = cached.into_model(&model_path);
        assert_eq!(
            recovered.available_languages, model.available_languages,
            "the reattached i18n fields survive the disk round trip"
        );
    }

    /// The model envelope carries the same guard, over the whole reconstructed payload.
    #[test]
    #[should_panic(expected = "tampered docs-fixture model cache")]
    fn an_edited_model_envelope_is_refused() {
        let mut cached = CachedModel::from_model(&DocsModel::default());
        // The hand-edit: claim a language the builder never found.
        cached.body.available_languages.push("klingon".to_string());
        let _ = cached.into_model(Path::new("<in-memory>"));
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
        let (_tmp, root) = temp_root("key");
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

    /// Every crate in `HASHED_CRATE_ROOTS` genuinely moves the key. A path
    /// dependency has no `Cargo.lock` checksum, so this is the ONLY thing standing
    /// between an edit to one of them and a stale cached model.
    #[test]
    fn every_hashed_crate_root_moves_the_key() {
        let (_tmp, root) = temp_root("crate-roots");
        for krate in HASHED_CRATE_ROOTS {
            let src = root.join("crates").join(krate).join("src");
            fs::create_dir_all(&src).unwrap();
            let before = cache_key(&root);
            fs::write(src.join("lib.rs"), format!("// {krate} v1")).unwrap();
            assert_ne!(
                before,
                cache_key(&root),
                "editing crates/{krate}/src must invalidate the docs-fixture cache"
            );
            let before = cache_key(&root);
            fs::write(
                root.join("crates").join(krate).join("Cargo.toml"),
                b"[package]\n",
            )
            .unwrap();
            assert_ne!(
                before,
                cache_key(&root),
                "editing crates/{krate}/Cargo.toml must invalidate the docs-fixture cache"
            );
        }
    }

    /// `HASHED_CRATE_ROOTS` IS the transitive `path = "../…"` dependency closure of
    /// `gmeow-docs`, re-derived here from the live workspace manifests.
    ///
    /// This is the construction that keeps the cache sound as the crate graph moves:
    /// adding a path dependency anywhere in the closure opens a fresh unhashed hole
    /// (path deps carry no `Cargo.lock` checksum), and this test reds instead of the
    /// hole opening silently. Removing one leaves a dead entry, which it also reds.
    #[test]
    fn crate_dep_closure_is_fully_hashed() {
        /// The `path = "../<name>"` dependencies declared by one crate manifest.
        fn path_deps(crates_dir: &Path, krate: &str) -> Vec<String> {
            let manifest = crates_dir.join(krate).join("Cargo.toml");
            let text = fs::read_to_string(&manifest)
                .unwrap_or_else(|e| panic!("read {}: {e}", manifest.display()));
            let mut out = Vec::new();
            for (idx, _) in text.match_indices("path = \"../") {
                let rest = &text[idx + "path = \"../".len()..];
                let Some(end) = rest.find('"') else { continue };
                let name = &rest[..end];
                // Only sibling crates (`../<name>`), never a deeper relative path.
                if !name.is_empty() && !name.contains('/') {
                    out.push(name.to_string());
                }
            }
            out
        }

        let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("crates/docs has a parent")
            .to_path_buf();

        let mut closure: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut queue = vec!["docs".to_string()];
        while let Some(krate) = queue.pop() {
            if !closure.insert(krate.clone()) {
                continue;
            }
            queue.extend(path_deps(&crates_dir, &krate));
        }

        let hashed: std::collections::BTreeSet<String> = HASHED_CRATE_ROOTS
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        assert_eq!(
            hashed, closure,
            "HASHED_CRATE_ROOTS must be exactly gmeow-docs' transitive path-dependency \
             closure — a path dependency carries no Cargo.lock checksum, so an unhashed \
             one is a permanent stale-cache hole in .cache/docs-fixture"
        );
    }

    /// The competency-query resolution boundary the model enforces is exactly a set
    /// of directories this key walks in full — otherwise a `gmeow:cqQueryFile` could
    /// name a file whose text changes without moving the key.
    #[test]
    fn competency_query_roots_are_hashed() {
        for boundary in COMPETENCY_QUERY_ROOTS {
            let dir = boundary.trim_end_matches('/');
            let (_tmp, root) = temp_root(&format!("cq-{dir}"));
            fs::create_dir_all(root.join(dir)).unwrap();
            let before = cache_key(&root);
            fs::write(root.join(dir).join("q.rq"), b"SELECT * {}").unwrap();
            assert_ne!(
                before,
                cache_key(&root),
                "a competency query under {boundary} must be folded into the cache key"
            );
        }
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
    #[should_panic(expected = "corrupt docs-fixture cache")]
    fn present_but_corrupt_model_cache_panics() {
        let (_tmp, root) = temp_root("corrupt-model");
        let cp = cache_path(&root);
        fs::create_dir_all(cp.parent().unwrap()).unwrap();
        fs::write(&cp, b"{ not valid json").unwrap();
        let _ = load(&root);
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
