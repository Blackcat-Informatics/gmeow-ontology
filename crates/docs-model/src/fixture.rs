// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Once-per-run action cache for the documentation model.
//!
//! Building a [`DocsModel`] via [`DocsModel::discover`] walks the whole slice
//! catalog, parses every `module.ttl`, and folds the i18n catalogs (~12 s). That
//! cost belongs to the explicit producer. The documentation integration tests,
//! `gmeow-dev doc-lint`, and `gmeow-slice-quality`'s `DocMaturity` axis share the
//! resulting model. Tests run in separate processes and admit only the exact
//! producer-selected action, without discovering sources or rebuilding the model.
//!
//! [`load`] / [`try_load`] are strict test-facing consumers: they load an exact
//! authenticated model or fail closed, and never build on a miss. The explicitly named
//! [`load_or_build`] / [`try_load_or_build`] producer APIs build the model once and store
//! it in a content-addressed disk cache before test processes start. `gmeow-docs` layers the renderer-only
//! artifacts (the per-language site and the mdBook source tree) on top of the SAME
//! action DAG and bounded store — see `gmeow_docs::fixture`. The split is a
//! layering one: this crate is a leaf with respect to the renderer, so the model
//! half is reachable from every model consumer (`gmeow-slice-quality` included)
//! without dragging the renderer's 13.6 MB of vendored wasm — or a dependency cycle
//! — along with it.
//!
//! The cache key is salted with the crate version and the model schema version,
//! then folds both every input `discover()` reads and the implementation sources
//! that build/serialize/render the fixture. The authenticated producer's exact
//! Cargo unit and Rust module selection owns the `crates/docs` implementation
//! closure, including fixture construction and embedded assets. Data, renderer,
//! schema, and selected dependency changes invalidate it without a manual
//! version bump. Publication, integrity, quota GC, and cross-process build election
//! come from the workspace's single `gmeow-action-cache` authority.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use gmeow_action_cache::selection::{SelectedAction, load_manifest};
use gmeow_action_cache::{
    ActionCacheError, ActionContext, ActionInput, ActionReceipt, ActionStore, FileKind,
    ProducerIdentity, STORE_FORMAT_VERSION, StoreLimits,
};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::i18n::{Translations, UiCatalog};
use crate::model::{COMPETENCY_QUERY_ROOTS, DocsError, DocsModel};

const MODEL_CODEC: &str = "docs-model-json-2";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DocsActionPayload {
    schema_version: u32,
    artifact: String,
    input_digest: String,
}

/// Receipt identity consumed by downstream render actions without hydrating the
/// serialized model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureIdentity {
    pub receipt_digest: String,
    pub product_digest: String,
    /// Exact implementation/input identity chosen by the producer, not the consumer.
    pub producer: ProducerIdentity,
}

impl FixtureIdentity {
    fn from_receipt(receipt: &ActionReceipt<DocsActionPayload>) -> Self {
        Self {
            receipt_digest: receipt.digest(),
            product_digest: receipt.product_digest.clone(),
            producer: receipt.context.implementation.clone(),
        }
    }
}

/// Documentation actions bound into the runner-authenticated corpus selector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocsFixtureSelector {
    pub schema_version: u32,
    pub model: SelectedAction,
    /// `site:<language>` and `book` actions, all bound to the same model receipt.
    pub renders: BTreeMap<String, SelectedAction>,
}

/// Read the producer-selected documentation actions without inspecting live sources.
pub fn selected_fixtures(root: &Path) -> Result<DocsFixtureSelector, ActionCacheError> {
    #[derive(Deserialize)]
    struct Envelope {
        docs: DocsFixtureSelector,
    }
    let selected = load_manifest::<Envelope>(root)?.docs;
    if selected.schema_version != 1 {
        return Err(ActionCacheError::message(
            "docs fixture selector schema mismatch",
        ));
    }
    let context = &selected.model.context;
    if *context != model_context_for_digest(context.implementation.digest.clone()) {
        return Err(ActionCacheError::message(
            "selected docs model context mismatch",
        ));
    }
    Ok(selected)
}

enum ModelCacheError {
    Cache(ActionCacheError),
    Build(DocsError),
}

impl From<ActionCacheError> for ModelCacheError {
    fn from(error: ActionCacheError) -> Self {
        Self::Cache(error)
    }
}

fn action_store(root: &Path) -> ActionStore {
    ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .unwrap_or_else(|error| panic!("open bounded docs-fixture action cache: {error}"))
}

fn read_only_action_store(root: &Path) -> Result<ActionStore, ActionCacheError> {
    ActionStore::open_existing_read_only(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
}

fn model_context(root: &Path) -> ActionContext {
    model_context_for_digest(cache_key(root))
}

fn model_context_for_digest(input_digest: String) -> ActionContext {
    ActionContext::new(
        "docs-fixture",
        "model",
        ProducerIdentity::new(input_digest.clone()),
        MODEL_CODEC,
        vec![ActionInput::Raw {
            logical_path: "docs-model-input-closure".to_string(),
            file_kind: FileKind::Aggregate,
            executable: false,
            digest: input_digest,
        }],
    )
}

fn model_payload(context: &ActionContext) -> DocsActionPayload {
    DocsActionPayload {
        schema_version: 1,
        artifact: "model".to_string(),
        input_digest: context
            .inputs
            .iter()
            .find_map(|input| match input {
                ActionInput::Raw { digest, .. } => Some(digest.clone()),
                ActionInput::Upstream { .. } => None,
            })
            .expect("model action has its aggregate input"),
    }
}

fn validate_model_receipt(
    context: &ActionContext,
    receipt: &ActionReceipt<DocsActionPayload>,
) -> Result<(), ActionCacheError> {
    let expected = model_payload(context);
    if receipt.payload != expected {
        return Err(ActionCacheError::message(format!(
            "docs model receipt payload mismatch: expected {expected:?}, actual {:?}",
            receipt.payload
        )));
    }
    Ok(())
}

fn decode_model(cache_path: &Path, bytes: &[u8]) -> Result<DocsModel, ActionCacheError> {
    let cached: CachedModel = serde_json::from_slice(bytes).map_err(|error| {
        ActionCacheError::message(format!("docs model payload JSON is corrupt: {error}"))
    })?;
    Ok(cached.into_model(cache_path))
}

fn probe_model(
    store: &ActionStore,
    context: &ActionContext,
    cache_path: &Path,
) -> Result<Option<(DocsModel, FixtureIdentity)>, ActionCacheError> {
    let Some(entry) = store.get::<DocsActionPayload>(context)? else {
        return Ok(None);
    };
    validate_model_receipt(context, &entry.receipt)?;
    let identity = FixtureIdentity::from_receipt(&entry.receipt);
    let model = decode_model(cache_path, &entry.bytes)?;
    Ok(Some((model, identity)))
}

/// Load the exact authenticated documentation model produced before this consumer.
/// A miss is terminal and never falls through to [`DocsModel::discover`].
#[must_use]
pub fn load(root: &Path) -> DocsModel {
    load_with_identity(root).0
}

/// Load the model together with the immutable receipt identity used by downstream
/// documentation render actions.
#[must_use]
pub fn load_with_identity(root: &Path) -> (DocsModel, FixtureIdentity) {
    try_load_with_identity(root)
        .unwrap_or_else(|error| panic!("load authenticated docs model: {error}"))
}

/// [`load`], but reporting an absent authenticated fixture as an error.
pub fn try_load(root: &Path) -> Result<DocsModel, DocsError> {
    try_load_with_identity(root).map(|(model, _)| model)
}

fn try_load_with_identity(root: &Path) -> Result<(DocsModel, FixtureIdentity), DocsError> {
    let selected = selected_fixtures(root)
        .map_err(|error| DocsError::FixtureUnavailable(error.to_string()))?;
    load_selected_model(root, &selected.model)
        .map_err(|error| DocsError::FixtureUnavailable(error.to_string()))
}

fn load_selected_model(
    root: &Path,
    selected: &SelectedAction,
) -> Result<(DocsModel, FixtureIdentity), ActionCacheError> {
    let context = &selected.context;
    if *context != model_context_for_digest(context.implementation.digest.clone()) {
        return Err(ActionCacheError::message(
            "selected docs model context mismatch",
        ));
    }
    let store = read_only_action_store(root)?;
    let entry = store.get::<DocsActionPayload>(context)?.ok_or_else(|| {
        ActionCacheError::message(format!(
            "selected docs model action {} is absent",
            context.key()
        ))
    })?;
    selected.verify(&entry.receipt)?;
    validate_model_receipt(context, &entry.receipt)?;
    let identity = FixtureIdentity::from_receipt(&entry.receipt);
    let model = decode_model(&store.receipt_path(&context.key()), &entry.bytes)?;
    Ok((model, identity))
}

/// Select the already-produced model for binding into a test fixture manifest.
/// This producer-side operation authenticates the current input key without building.
pub fn produced_model_selection(root: &Path) -> Result<SelectedAction, ActionCacheError> {
    let context = model_context(root);
    let store = read_only_action_store(root).map_err(|error| {
        ActionCacheError::message(format!(
            "authenticated docs action store is unavailable without mutation: {error}"
        ))
    })?;
    let receipt = store
        .inspect::<DocsActionPayload>(&context)?
        .ok_or_else(|| {
            ActionCacheError::message(format!(
                "current docs model action {} is absent",
                context.key()
            ))
        })?;
    validate_model_receipt(&context, &receipt)?;
    Ok(SelectedAction::from_receipt(&receipt))
}

/// Load or produce the documentation model for an explicit producer operation.
/// Test code must use [`load`] or [`try_load`] instead.
#[must_use]
pub fn load_or_build(root: &Path) -> DocsModel {
    load_or_build_with_identity(root).0
}

/// Producer counterpart of [`load_with_identity`].
#[must_use]
pub fn load_or_build_with_identity(root: &Path) -> (DocsModel, FixtureIdentity) {
    try_load_or_build_with_identity(root)
        .unwrap_or_else(|error| panic!("build docs model from live slices: {error}"))
}

/// Producer counterpart of [`try_load`].
pub fn try_load_or_build(root: &Path) -> Result<DocsModel, DocsError> {
    try_load_or_build_with_identity(root).map(|(model, _)| model)
}

fn try_load_or_build_with_identity(root: &Path) -> Result<(DocsModel, FixtureIdentity), DocsError> {
    let store = action_store(root);
    let context = model_context(root);
    let key = context.key();
    let cache_path = cache_path(root);
    let outcome = store.coordinate::<_, ModelCacheError, _, _>(
        &key,
        || probe_model(&store, &context, &cache_path).map_err(ModelCacheError::from),
        || {
            let model = DocsModel::discover(root).map_err(ModelCacheError::Build)?;
            let cached = CachedModel::from_model(&model);
            let bytes = serde_json::to_vec(&cached).map_err(ActionCacheError::from)?;
            let receipt = store
                .publish(
                    &context,
                    cached.digest.clone(),
                    model_payload(&context),
                    &bytes,
                )
                .map_err(ModelCacheError::from)?;
            Ok((model, FixtureIdentity::from_receipt(&receipt)))
        },
    );
    match outcome {
        Ok(outcome) => Ok(outcome.value),
        Err(ModelCacheError::Build(error)) => Err(error),
        Err(ModelCacheError::Cache(error)) => {
            panic!(
                "corrupt docs-fixture action cache at {}: {error}",
                cache_path.display()
            )
        }
    }
}

/// Authenticate the model action and return its receipt identity without
/// deserializing the model. A miss is terminal.
#[must_use]
pub fn model_identity(root: &Path) -> FixtureIdentity {
    let store = read_only_action_store(root)
        .unwrap_or_else(|error| panic!("open authenticated docs model store read-only: {error}"));
    let selected = selected_fixtures(root)
        .unwrap_or_else(|error| panic!("admit selected docs fixtures: {error}"));
    let context = selected.model.context.clone();
    match store.inspect::<DocsActionPayload>(&context) {
        Ok(Some(receipt)) => {
            selected
                .model
                .verify(&receipt)
                .unwrap_or_else(|error| panic!("selected docs model receipt mismatch: {error}"));
            validate_model_receipt(&context, &receipt)
                .unwrap_or_else(|error| panic!("corrupt docs-fixture model receipt: {error}"));
            FixtureIdentity::from_receipt(&receipt)
        }
        Ok(None) => {
            panic!("authenticated docs model fixture is absent; tests may not rebuild the corpus")
        }
        Err(error) => panic!("corrupt docs-fixture model action cache: {error}"),
    }
}

/// Producer counterpart of [`model_identity`].
#[must_use]
pub fn model_identity_or_build(root: &Path) -> FixtureIdentity {
    let store = action_store(root);
    let context = model_context(root);
    match store.inspect::<DocsActionPayload>(&context) {
        Ok(Some(receipt)) => {
            validate_model_receipt(&context, &receipt)
                .unwrap_or_else(|error| panic!("corrupt docs-fixture model receipt: {error}"));
            FixtureIdentity::from_receipt(&receipt)
        }
        Ok(None) => load_or_build_with_identity(root).1,
        Err(error) => panic!("corrupt docs-fixture model action cache: {error}"),
    }
}

/// The on-disk cache path for the model built from the inputs under `root`.
///
/// The renderer-side artifacts (`gmeow_docs::fixture`'s per-language site and mdBook
/// caches) hang off the same [`cache_key`] with their own suffixes, so a single key
/// governs the whole fixture set.
#[must_use]
pub fn cache_path(root: &Path) -> PathBuf {
    cache_path_for_context(root, &model_context(root))
}

fn cache_path_for_context(root: &Path, context: &ActionContext) -> PathBuf {
    ActionStore::default_root(root)
        .join(format!("v{STORE_FORMAT_VERSION}"))
        .join("receipts")
        .join(format!("{}.json", context.key()))
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
///
/// This is the single authority for every fixture envelope's digest, model and renderer
/// artifacts alike — a second copy in the renderer crate is exactly the two-sources-of-truth
/// defect the payload digest exists to catch.
///
/// # Panics
/// When `payload` will not serialize — a serde regression, never a runtime condition.
#[must_use]
pub fn payload_digest<T: Serialize>(label: &str, payload: &T) -> String {
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
///
/// # Panics
/// When the live fold of `payload` differs from `declared`.
pub fn verify_payload<T: Serialize>(cache_path: &Path, label: &str, declared: &str, payload: &T) {
    let live = payload_digest(label, payload);
    assert!(
        live == declared,
        "tampered docs-fixture {label} cache at {}: it declares payload digest {declared} but \
         its content folds to {live}. The entry was edited after it was written — remove the \
         named corrupt action through cache maintenance to rebuild it; an edited cache entry \
         is never served",
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

/// Content-address discovery inputs and the exact selected renderer/model implementation.
///
/// The already-authenticated optimized producer supplies Cargo's selected dependency
/// closure. Whole production modules and explicit embedded assets are bound by the
/// shared source inventory; test modules and development-only dependencies are not.
/// Runtime corpus discovery roots remain separately selected data inputs.
///
/// # Panics
/// If the producer receipt is missing/stale or a selected input cannot be read.
#[must_use]
pub fn cache_key(root: &Path) -> String {
    let implementation =
        gmeow_action_cache::executable::current_source_inventory(root, "crates/docs/Cargo.toml")
            .and_then(|inventory| {
                inventory
                    .digest()
                    .map_err(|error| ActionCacheError::message(error.to_string()))
            })
            .unwrap_or_else(|error| panic!("authenticate docs fixture implementation: {error}"));
    cache_key_with_implementation(root, &implementation)
}

fn cache_key_with_implementation(root: &Path, implementation: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(b"gmeow-docs-fixture-v2\x1f");
    hasher.update(implementation.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(env!("CARGO_PKG_VERSION").as_bytes());
    hasher.update(b"\x1f");
    hasher.update(DocsModel::VERSION.as_bytes());
    hasher.update(b"\x1e");

    let mut files: Vec<PathBuf> = Vec::new();
    // Data roots discover() walks recursively. `queries` is the shared repo-root
    // SPARQL tree a `gmeow:cqQueryFile` may resolve into (T2:
    // `apply_competency_query_text`) alongside the per-slice `.rq` files already
    // covered by the `slices` walk — and `COMPETENCY_QUERY_ROOTS` is the enforced
    // boundary that keeps that resolution inside this hashed set. It is read from
    // [`crate::model`] directly: the boundary the model ENFORCES and the boundary this
    // key WALKS are the same constant, never a mirrored copy that could drift.
    for dir in COMPETENCY_QUERY_ROOTS
        .iter()
        .map(|r| r.trim_end_matches('/'))
        .chain(["shapes", "i18n"])
    {
        collect_files(&root.join(dir), &mut files);
    }
    // Runtime files discover() reads directly. Registry and Git dependencies are
    // already bound by selected lock records in the shared implementation inventory.
    for file in [
        "docs/four-boxes.md",
        "metadata/gmeow-self.ttl",
        "dsl/mappings/mapping-sets.ttl",
        "generated/catalog/constraint-catalog.nq",
        "generated/catalog/term-content-manifest.nq",
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
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("walking fixture input directory {}: {error}", dir.display()),
    };
    for entry in entries {
        let entry = entry.unwrap_or_else(|error| {
            panic!("walking fixture input directory {}: {error}", dir.display())
        });
        let path = entry.path();
        let file_type = entry.file_type().unwrap_or_else(|error| {
            panic!("reading fixture input type {}: {error}", path.display())
        });
        if file_type.is_dir() {
            collect_files(&path, out);
        } else if file_type.is_file() {
            out.push(path);
        } else {
            panic!(
                "fixture input {} is neither a regular file nor directory; symlinks and special files require an explicit typed key policy",
                path.display()
            );
        }
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

#[path = "fixture.tests.rs"]
#[cfg(test)]
mod tests;
