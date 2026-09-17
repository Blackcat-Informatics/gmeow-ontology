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
//! Model discovery and rendering belong to the explicit producer. The integration
//! tests run in separate processes and share its selected model and render receipts.
//! Their action identities come from the authenticated selector, never from the
//! consumer's checkout or a search for any available cached product.
//!
//! This module renders the site for EVERY available language and the default mdBook
//! render ONCE in the explicit [`prime`] producer and stores each in a content-addressed
//! disk cache. [`load_site`], [`load_site_lang`], and [`load_book`] are strict consumers:
//! a missing receipt hard-fails and can never trigger a render or model rebuild.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_action_cache::selection::SelectedAction;
use gmeow_action_cache::{
    ActionCacheError, ActionContext, ActionInput, ActionReceipt, ActionStore, STORE_FORMAT_VERSION,
    StoreLimits,
};
use serde::{Deserialize, Serialize};

use crate::mdbook::render_book;
use crate::render::{Site, render_site_lang};
use gmeow_docs_model::exec::ExecutableDocsData;
// `load` is RE-EXPORTED, not merely imported: the model cache lives in
// `gmeow-docs-model` so a consumer can share it without linking this crate's renderer
// (which `include_bytes!`s ~19 MB of wasm), but a caller that already depends on the
// renderer should not have to name a second crate to get the model.
use gmeow_docs_model::fixture::{
    DocsFixtureSelector, FixtureIdentity, load_or_build_with_identity, model_identity,
    payload_digest, produced_model_selection, selected_fixtures, verify_payload,
};
pub use gmeow_docs_model::fixture::{load, load_or_build};
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

fn read_only_action_store(root: &Path) -> ActionStore {
    ActionStore::open_existing_read_only(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .unwrap_or_else(|error| {
        panic!("open authenticated docs-render action cache read-only: {error}")
    })
}

fn render_context(
    artifact: &str,
    language: Option<&str>,
    model: &FixtureIdentity,
) -> ActionContext {
    let mut context = ActionContext::new(
        "docs-fixture",
        format!("render-{artifact}"),
        model.producer.clone(),
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

/// Load the exact authenticated rendered static site for `lang`.
/// A miss is terminal and never invokes a renderer.
#[must_use]
pub fn load_site_lang(root: &Path, lang: &str) -> Site {
    let model = model_identity(root);
    load_cached_site(root, "site", Some(lang), &model)
}

/// Load the exact authenticated default mdBook render. A miss is terminal.
#[must_use]
pub fn load_book(root: &Path) -> Site {
    let model = model_identity(root);
    load_cached_site(root, "book", None, &model)
}

/// Load one authenticated render action. Missing and corrupt entries both fail closed.
fn load_cached_site(
    root: &Path,
    artifact: &str,
    language: Option<&str>,
    model: &FixtureIdentity,
) -> Site {
    let fixtures = selected_fixtures(root)
        .unwrap_or_else(|error| panic!("admit selected docs fixtures: {error}"));
    let selected = fixtures
        .renders
        .get(&render_selector_key(artifact, language))
        .unwrap_or_else(|| panic!("requested docs render is absent from the producer selector"));
    load_selected_site(root, artifact, language, model, selected)
}

fn load_selected_site(
    root: &Path,
    artifact: &str,
    language: Option<&str>,
    model: &FixtureIdentity,
    selected: &SelectedAction,
) -> Site {
    let store = read_only_action_store(root);
    let context = render_context(artifact, language, model);
    assert_eq!(
        selected.context, context,
        "selected docs render context mismatch"
    );
    let cache_path = render_cache_path(root, &context);
    let entry = store
        .get::<RenderActionPayload>(&context)
        .unwrap_or_else(|error| {
            panic!(
                "corrupt docs-fixture {artifact} action cache at {}: {error}",
                cache_path.display()
            )
        })
        .unwrap_or_else(|| {
            panic!(
                "authenticated docs {artifact} fixture is absent; tests may not rebuild the corpus"
            )
        });
    selected
        .verify(&entry.receipt)
        .unwrap_or_else(|error| panic!("selected docs render receipt mismatch: {error}"));
    validate_render_receipt(artifact, language, model, &entry.receipt).unwrap_or_else(|error| {
        panic!(
            "corrupt docs-fixture {artifact} action cache at {}: {error}",
            cache_path.display()
        )
    });
    let cached: CachedSite = serde_json::from_slice(&entry.bytes).unwrap_or_else(|error| {
        panic!(
            "corrupt docs-fixture {artifact} action cache at {}: docs payload JSON is corrupt: {error}",
            cache_path.display()
        )
    });
    cached.into_site(&cache_path)
}

/// Produce or load one render action. This is reachable only from explicit producer
/// operations; test-facing loaders call [`load_cached_site`] instead.
fn load_or_build_cached_site(
    root: &Path,
    artifact: &str,
    language: Option<&str>,
    model: &FixtureIdentity,
    build: impl FnOnce() -> Site,
) -> Site {
    let store = action_store(root);
    load_or_build_cached_site_in_store(&store, root, artifact, language, model, build).0
}

fn load_or_build_cached_site_in_store(
    store: &ActionStore,
    root: &Path,
    artifact: &str,
    language: Option<&str>,
    model: &FixtureIdentity,
    build: impl FnOnce() -> Site,
) -> (Site, bool, SelectedAction) {
    let context = render_context(artifact, language, model);
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
            Ok(Some((
                cached.into_site(&cache_path),
                SelectedAction::from_receipt(&entry.receipt),
            )))
        },
        || {
            let site = build();
            let cached = CachedSite::from_site(&site);
            let bytes = serde_json::to_vec(&cached)?;
            let receipt = store.publish(
                &context,
                cached.digest.clone(),
                render_payload(artifact, language, model),
                &bytes,
            )?;
            Ok((site, SelectedAction::from_receipt(&receipt)))
        },
    );
    let outcome = outcome.unwrap_or_else(|error| {
        panic!(
            "corrupt docs-fixture {artifact} action cache at {}: {error}",
            cache_path.display()
        )
    });
    (outcome.value.0, outcome.built, outcome.value.1)
}

/// Producer counterpart of [`load_site`].
#[must_use]
pub fn load_site_or_build(root: &Path) -> Site {
    load_site_lang_or_build(root, ENGLISH)
}

/// Producer counterpart of [`load_site_lang`].
#[must_use]
pub fn load_site_lang_or_build(root: &Path, lang: &str) -> Site {
    let (model, identity) = load_or_build_with_identity(root);
    load_or_build_cached_site(root, "site", Some(lang), &identity, || {
        render_site_lang(&model, lang)
    })
}

/// Producer counterpart of [`load_book`].
#[must_use]
pub fn load_book_or_build(root: &Path) -> Site {
    let (model, identity) = load_or_build_with_identity(root);
    load_or_build_cached_site(root, "book", None, &identity, || {
        render_book(&model, &ExecutableDocsData::default())
    })
}

/// Build the model, the rendered site for every available language, and the
/// default mdBook render, writing each cache if it is not already present. Run
/// once before a batch of tests so none of them pays the (contended) model build
/// or any render.
///
/// Every warm action is authenticated before the primer returns. A missing action
/// recomputes; a present corrupt action hard-fails rather than being hidden by a
/// sentinel file. Independent render nodes share one admitted store and execute in
/// memory-bounded batches; the selected concurrency scales down to one on small CI
/// runners instead of creating a fixed host-wide cap.
#[must_use]
pub fn prime(root: &Path) -> PrimeObservation {
    let (model, identity) = load_or_build_with_identity(root);
    let selected_model = produced_model_selection(root)
        .unwrap_or_else(|error| panic!("select produced docs model: {error}"));
    assert_eq!(selected_model.receipt_digest, identity.receipt_digest);
    let mut languages = model.available_languages.clone();
    languages.push(ENGLISH.to_string());
    languages.sort();
    languages.dedup();
    let mut tasks = languages.into_iter().map(Some).collect::<Vec<_>>();
    tasks.push(None);
    let parallelism = render_parallelism(tasks.len());
    let mut built = 0;
    let mut renders = BTreeMap::new();
    let store = action_store(root);
    for batch in tasks.chunks(parallelism) {
        let results = std::thread::scope(|scope| {
            let handles = batch
                .iter()
                .map(|language| {
                    let store = &store;
                    let model = &model;
                    let identity = &identity;
                    scope.spawn(move || {
                        let artifact = if language.is_some() { "site" } else { "book" };
                        let (_, was_built, selected) = load_or_build_cached_site_in_store(
                            store,
                            root,
                            artifact,
                            language.as_deref(),
                            identity,
                            || {
                                if let Some(language) = language.as_deref() {
                                    render_site_lang(model, language)
                                } else {
                                    render_book(model, &ExecutableDocsData::default())
                                }
                            },
                        );
                        (
                            render_selector_key(artifact, language.as_deref()),
                            was_built,
                            selected,
                        )
                    })
                })
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|handle| handle.join().expect("docs render worker"))
                .collect::<Vec<_>>()
        });
        for (key, was_built, selected) in results {
            built += usize::from(was_built);
            assert!(
                renders.insert(key, selected).is_none(),
                "duplicate docs render selection"
            );
        }
    }
    PrimeObservation {
        action_count: tasks.len(),
        built,
        receipt_hits: tasks.len().saturating_sub(built),
        parallelism,
        selector: DocsFixtureSelector {
            schema_version: 1,
            model: selected_model,
            renders,
        },
    }
}

fn render_selector_key(artifact: &str, language: Option<&str>) -> String {
    language.map_or_else(
        || artifact.to_string(),
        |language| format!("{artifact}:{language}"),
    )
}

/// Observational cache/scheduling telemetry from one explicit docs-fixture producer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrimeObservation {
    /// Exact actions emitted or admitted by this producer invocation.
    pub selector: DocsFixtureSelector,
    /// Number of independent render actions selected by the model.
    pub action_count: usize,
    /// Actions recomputed after an authenticated miss.
    pub built: usize,
    /// Actions restored from already-authenticated receipts.
    pub receipt_hits: usize,
    /// Maximum render actions admitted concurrently for this host.
    pub parallelism: usize,
}

const MIN_RENDER_WORKER_BYTES: u64 = 4 * 1024 * 1024 * 1024;

fn render_parallelism(task_count: usize) -> usize {
    let cpus = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1);
    let by_memory = available_memory_bytes()
        .map(|bytes| {
            usize::try_from((bytes / 2) / MIN_RENDER_WORKER_BYTES)
                .unwrap_or(usize::MAX)
                .max(1)
        })
        .unwrap_or(1);
    task_count.max(1).min(cpus).min(by_memory)
}

fn available_memory_bytes() -> Option<u64> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    meminfo.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        if fields.next()? != "MemAvailable:" {
            return None;
        }
        fields.next()?.parse::<u64>().ok()?.checked_mul(1024)
    })
}

fn render_cache_path(root: &Path, context: &ActionContext) -> PathBuf {
    ActionStore::default_root(root)
        .join(format!("v{STORE_FORMAT_VERSION}"))
        .join("receipts")
        .join(format!("{}.json", context.key()))
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
#[path = "fixture_test_support.rs"]
mod test_support;
#[cfg(test)]
use test_support::{book_cache_path, site_cache_path};

#[path = "fixture.tests.rs"]
#[cfg(test)]
mod tests;
