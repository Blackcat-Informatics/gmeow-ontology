// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Once-per-run action cache for the rendered documentation
//! artifacts — the per-language static site and the default mdBook source tree.
//!
//! The model half of this fixture lives in [`gmeow_docs_model::fixture`]: its
//! authenticated receipt and [`gmeow_docs_model::fixture::load`]
//! itself. It sits there because it must be reachable from every model consumer
//! without linking the renderer — `gmeow-slice-quality`'s `DocMaturity` axis reads it,
//! and an edge from that crate to this one would close a first-party dependency cycle
//! (`gmeow-docs` dev-depends on `gmeow-mcp`, which depends on `gmeow-slice-quality`).
//! What stays here is exactly what needs [`Site`], [`render_site_lang`] and
//! [`render_book`]. Model, site, book, and pipeline stages all use the same bounded
//! immutable receipt/blob store and per-action process election.
//!
//! A [`gmeow_docs_model::model::DocsModel`] build is a ~12 s repo-wide walk, and
//! rendering the site on top of it is more; the gmeow-docs integration suite has
//! ~40 tests that each need one or both,
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
use std::path::{Path, PathBuf};

use gmeow_action_cache::{
    ActionCacheError, ActionContext, ActionInput, ActionReceipt, ActionStore, ProducerIdentity,
    STORE_FORMAT_VERSION, StoreLimits,
};
use serde::{Deserialize, Serialize};

use crate::mdbook::render_book;
use crate::render::{Site, render_site_lang};
use gmeow_docs_model::exec::ExecutableDocsData;
// `load` is RE-EXPORTED, not merely imported: the model cache lives in
// `gmeow-docs-model` so a consumer can share it without linking this crate's renderer
// (which `include_bytes!`s ~19 MB of wasm), but a caller that already depends on the
// renderer should not have to name a second crate to get the model.
pub use gmeow_docs_model::fixture::load;
use gmeow_docs_model::fixture::{
    FixtureIdentity, cache_key, load_with_identity, model_identity, payload_digest, verify_payload,
};
use gmeow_docs_model::i18n::ENGLISH;

const SITE_CODEC: &str = "docs-site-json-2";
const BOOK_CODEC: &str = "docs-book-json-2";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RenderActionPayload {
    schema_version: u32,
    artifact: String,
    language: Option<String>,
    model_receipt_digest: String,
}

fn action_store(root: &Path) -> ActionStore {
    ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .unwrap_or_else(|error| panic!("open bounded docs-render action cache: {error}"))
}

fn render_context(
    root: &Path,
    artifact: &str,
    language: Option<&str>,
    model: &FixtureIdentity,
) -> ActionContext {
    let mut context = ActionContext::new(
        "docs-fixture",
        format!("render-{artifact}"),
        ProducerIdentity::new(cache_key(root)),
        if artifact == "site" {
            SITE_CODEC
        } else {
            BOOK_CODEC
        },
        vec![ActionInput::Upstream {
            producer: "docs-model".to_string(),
            entity: None,
            receipt_digest: Some(model.receipt_digest.clone()),
            product_digest: model.product_digest.clone(),
        }],
    );
    if let Some(language) = language {
        context = context.with_dimension("language", language);
    }
    context
}

fn render_payload(
    artifact: &str,
    language: Option<&str>,
    model: &FixtureIdentity,
) -> RenderActionPayload {
    RenderActionPayload {
        schema_version: 1,
        artifact: artifact.to_string(),
        language: language.map(str::to_string),
        model_receipt_digest: model.receipt_digest.clone(),
    }
}

fn validate_render_receipt(
    artifact: &str,
    language: Option<&str>,
    model: &FixtureIdentity,
    receipt: &ActionReceipt<RenderActionPayload>,
) -> Result<(), ActionCacheError> {
    let expected = render_payload(artifact, language, model);
    if receipt.payload != expected {
        return Err(ActionCacheError::message(format!(
            "docs render receipt payload mismatch: expected {expected:?}, actual {:?}",
            receipt.payload
        )));
    }
    Ok(())
}

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
    let model = model_identity(root);
    load_cached_site(root, "site", Some(lang), &model, || {
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
    let model = model_identity(root);
    load_cached_site(root, "book", None, &model, || {
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
fn load_cached_site(
    root: &Path,
    artifact: &str,
    language: Option<&str>,
    model: &FixtureIdentity,
    build: impl FnOnce() -> Site,
) -> Site {
    let store = action_store(root);
    let context = render_context(root, artifact, language, model);
    let key = context.key();
    let cache_path = render_cache_path(root, &context);
    let outcome = store.coordinate::<_, ActionCacheError, _, _>(
        &key,
        || {
            let Some(entry) = store.get::<RenderActionPayload>(&context)? else {
                return Ok(None);
            };
            validate_render_receipt(artifact, language, model, &entry.receipt)?;
            let cached: CachedSite = serde_json::from_slice(&entry.bytes).map_err(|error| {
                ActionCacheError::message(format!(
                    "docs {artifact} payload JSON is corrupt: {error}"
                ))
            })?;
            Ok(Some(cached.into_site(&cache_path)))
        },
        || {
            let site = build();
            let cached = CachedSite::from_site(&site);
            let bytes = serde_json::to_vec(&cached)?;
            store.publish(
                &context,
                cached.digest.clone(),
                render_payload(artifact, language, model),
                &bytes,
            )?;
            Ok(site)
        },
    );
    outcome
        .unwrap_or_else(|error| {
            panic!(
                "corrupt docs-fixture {artifact} action cache at {}: {error}",
                cache_path.display()
            )
        })
        .value
}

/// Build the model, the rendered site for every available language, and the
/// default mdBook render, writing each cache if it is not already present. Run
/// once before a batch of tests so none of them pays the (contended) model build
/// or any render.
///
/// Every warm action is authenticated before the primer returns. A missing action
/// recomputes; a present corrupt action hard-fails rather than being hidden by a
/// sentinel file.
pub fn prime(root: &Path) {
    let (model, identity) = load_with_identity(root);
    let mut languages = model.available_languages.clone();
    languages.push(ENGLISH.to_string());
    languages.sort();
    languages.dedup();
    for lang in &languages {
        let _ = load_cached_site(root, "site", Some(lang), &identity, || {
            render_site_lang(&model, lang)
        });
    }
    let _ = load_cached_site(root, "book", None, &identity, || {
        render_book(&model, &ExecutableDocsData::default())
    });
}

fn render_cache_path(root: &Path, context: &ActionContext) -> PathBuf {
    ActionStore::default_root(root)
        .join(format!("v{STORE_FORMAT_VERSION}"))
        .join("receipts")
        .join(format!("{}.json", context.key()))
}

#[cfg(test)]
fn site_cache_path(root: &Path, lang: &str, model: &FixtureIdentity) -> PathBuf {
    render_cache_path(root, &render_context(root, "site", Some(lang), model))
}

#[cfg(test)]
fn book_cache_path(root: &Path, model: &FixtureIdentity) -> PathBuf {
    render_cache_path(root, &render_context(root, "book", None, model))
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
    use std::fs;

    fn model_identity() -> FixtureIdentity {
        FixtureIdentity {
            receipt_digest: "model-receipt".to_string(),
            product_digest: "model-product".to_string(),
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
        let cold = load_cached_site(&root, "site", Some(ENGLISH), &identity, || built.clone());
        assert_eq!(cold, built, "the cold miss returns the built site");
        assert!(path.is_file(), "the miss wrote the envelope");
        let warm = load_cached_site(&root, "site", Some(ENGLISH), &identity, || {
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
        let _ = load_cached_site(&root, "site", Some(ENGLISH), &identity, || {
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
        let _ = load_cached_site(&root, "book", None, &identity, || {
            panic!("a present corrupt action must not rebuild")
        });
    }
}
