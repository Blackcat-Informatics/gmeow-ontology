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
//! This module builds the model ONCE and stores it in a content-addressed disk
//! cache; later callers load it cheaply. [`prime`] is run once before the test
//! processes spawn (a runner setup step) so no test pays the build; [`load`] is
//! the per-process loader, which also builds-and-caches on a genuine miss so a
//! plain `cargo test` (no setup step) still works.
//!
//! The cache key is salted with the crate version and the model schema version
//! and folds the bytes of every input `discover()` reads, so a slice edit (or a
//! code change that bumps the crate version) invalidates it — a stale model is
//! never served. This is the same content-addressed, atomic-temp-then-rename
//! pattern the validate and slice caches use.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::i18n::{Translations, UiCatalog};
use crate::model::DocsModel;
use crate::render::{render_site, Site};

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

/// Load the rendered English static site rooted at `root`, from the once-per-run
/// cache when present, otherwise rendered via [`render_site`] and cached for the
/// rest of the run. Byte-identical to a fresh `render_site(&load(root))`.
///
/// The English render is the canonical carrier (`render_site` ≡
/// `render_site_lang(_, "english")`), and every gated test that needs the live
/// site — determinism checks, the carrier-vs-`render_site` identity, English
/// path-graph comparisons, lint passes — loads it from here instead of paying a
/// fresh render. That removes the dominant per-test cost (a full site render) and
/// the cross-process render contention that pushed those tests over the gate.
///
/// Corrupt-but-present is an integrity violation and panics; only a genuine
/// absence falls through to a fresh render (so a plain `cargo test` still works).
pub fn load_site(root: &Path) -> Site {
    let cache_path = site_cache_path(root);
    match fs::read(&cache_path) {
        Ok(bytes) => {
            let cached: CachedSite = serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                panic!(
                    "corrupt docs-fixture site cache at {}: {e}\n\
                     delete the file (or run `rm -rf .cache/docs-fixture`) to rebuild",
                    cache_path.display()
                )
            });
            cached.into_site()
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            build_site_and_cache(root, &cache_path)
        }
        Err(e) => panic!(
            "cannot read docs-fixture site cache at {}: {e}",
            cache_path.display()
        ),
    }
}

/// Build the model and the rendered site and write each cache if it is not
/// already present. Run once before a batch of tests so none of them pays the
/// (contended) model build or site render. A no-op when both caches are warm.
pub fn prime(root: &Path) {
    let cache_path = cache_path(root);
    if !cache_path.exists() {
        build_and_cache(root, &cache_path);
    }
    let site_path = site_cache_path(root);
    if !site_path.exists() {
        build_site_and_cache(root, &site_path);
    }
}

fn build_and_cache(root: &Path, cache_path: &Path) -> DocsModel {
    let model = DocsModel::discover(root).expect("build docs model from live slices");
    write_cache(cache_path, &CachedModel::from_model(&model));
    model
}

fn build_site_and_cache(root: &Path, site_path: &Path) -> Site {
    // Reuse the warm model cache (built first by `prime`, or built-and-cached
    // here on a plain `cargo test` miss) rather than re-walking the slices.
    let model = load(root);
    let site = render_site(&model);
    write_cache(site_path, &CachedSite::from_site(&site));
    site
}

/// The on-disk cache path for the model built from the inputs under `root`.
fn cache_path(root: &Path) -> PathBuf {
    let key = cache_key(root);
    root.join(".cache")
        .join("docs-fixture")
        .join(format!("{key}.json"))
}

/// The on-disk cache path for the rendered site. Shares the model cache key (the
/// site is a pure function of the model, and a render-logic change is covered by
/// the crate-version salt), with a distinct suffix.
fn site_cache_path(root: &Path) -> PathBuf {
    let key = cache_key(root);
    root.join(".cache")
        .join("docs-fixture")
        .join(format!("{key}.site.json"))
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
/// `(relative-path, bytes)` of every file under the discovery roots. Any slice /
/// shape / i18n / metadata edit changes the key; a crate-version bump (a
/// `discover`/`render` logic change) changes the salt.
fn cache_key(root: &Path) -> String {
    let mut hasher = Sha1::new();
    hasher.update(b"gmeow-docs-fixture\x1f");
    hasher.update(env!("CARGO_PKG_VERSION").as_bytes());
    hasher.update(b"\x1f");
    hasher.update(DocsModel::VERSION.as_bytes());
    hasher.update(b"\x1e");

    let mut files: Vec<PathBuf> = Vec::new();
    // Directory roots discover() walks recursively.
    for dir in ["slices", "shapes", "i18n"] {
        collect_files(&root.join(dir), &mut files);
    }
    // Individual files discover() reads directly.
    for file in ["docs/four-boxes.md", "metadata/gmeow-self.ttl"] {
        let p = root.join(file);
        if p.is_file() {
            files.push(p);
        }
    }
    files.sort();

    for path in &files {
        let rel = path.strip_prefix(root).unwrap_or(path);
        hasher.update(rel.to_string_lossy().as_bytes());
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
/// Write failures are non-fatal (a parallel writer may have won the rename); the
/// cache is an optimization and the value is already in hand.
fn write_cache<T: Serialize>(path: &Path, cached: &T) {
    let dir = path.parent().expect("cache path has a parent");
    if let Err(e) = fs::create_dir_all(dir) {
        if e.kind() != std::io::ErrorKind::AlreadyExists {
            panic!("creating cache dir {}: {e}", dir.display());
        }
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
    if write_result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
}

/// Lowercase hex of a digest.
fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
