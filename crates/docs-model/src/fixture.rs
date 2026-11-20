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

mod action_payload_json;
#[cfg(test)]
mod catalog_inputs_tests;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use gmeow_action_cache::selection::{SelectedAction, load_manifest};
use gmeow_action_cache::{
    ActionCacheError, ActionContext, ActionInput, ActionReceipt, ActionStore, FileKind,
    ProducerIdentity, STORE_FORMAT_VERSION, StoreLimits,
};
use gmeow_errors::{Diag, PreservedDiagnostic};
use purrdf_hash::sha1::Sha1;
use serde::{Deserialize, Serialize};

use crate::i18n::{Translations, UiCatalog};
use crate::model::{COMPETENCY_QUERY_ROOTS, DocsError, DocsModel};

const MODEL_CODEC: &str = "docs-model-json-2";
const MODEL_INPUT_CLOSURE: &str = "docs-model-input-closure";
const CATALOG_ARTIFACTS: [(&str, &str); 2] = [
    (
        "stage-constraint-catalog",
        "generated/catalog/constraint-catalog.nq",
    ),
    (
        "stage-term-manifest",
        "generated/catalog/term-content-manifest.nq",
    ),
];

#[derive(Debug, Clone, PartialEq, Eq)]
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
    struct Envelope {
        docs: DocsFixtureSelector,
    }
    impl purrdf_lex::json::record::FromJson for Envelope {
        fn from_json(
            value: &purrdf_lex::json::Value,
        ) -> Result<Self, purrdf_lex::json::record::DecodeError> {
            let mut fields = purrdf_lex::json::record::Record::new(value, "struct Envelope")?;
            // Other selector extensions belong to their respective consumers.
            Ok(Self {
                docs: fields.required("docs")?,
            })
        }
    }
    let selected = load_manifest::<Envelope>(root)?.docs;
    if selected.schema_version != 1 {
        return Err(ActionCacheError::message(
            "docs fixture selector schema mismatch",
        ));
    }
    validate_model_context(&selected.model.context)?;
    Ok(selected)
}

/// Verify the selected model against current authored inputs and catalog actions.
/// Used by the explicit read-only verifier before the test runner starts.
///
/// # Errors
/// Refuses a disk-sourced model or a model built with different inputs.
pub fn verify_current_catalog_model(
    root: &Path,
    selected: &DocsFixtureSelector,
    inputs: Vec<ActionInput>,
) -> gmeow_errors::Result<()> {
    let current_digest = cache_key_for_paths(root, &fixture_implementation(root)?, &[])?;
    verify_catalog_model_context(&selected.model.context, current_digest, inputs)
        .map_err(Into::into)
}

fn verify_catalog_model_context(
    context: &ActionContext,
    authored_digest: String,
    inputs: Vec<ActionInput>,
) -> Result<(), ActionCacheError> {
    let expected = model_context_for_inputs(authored_digest, inputs);
    if expected.inputs.len() != 1 + 2 * CATALOG_ARTIFACTS.len() || *context != expected {
        return Err(ActionCacheError::message(
            "selected docs model differs from current authored inputs or authenticated catalogs",
        ));
    }
    Ok(())
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

/// Open the shared action store for explicit model and render fixture producers.
///
/// # Errors
/// Returns cache format or filesystem failures.
pub fn open_action_store(root: &Path) -> Result<ActionStore, ActionCacheError> {
    ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
}

fn read_only_action_store(root: &Path) -> Result<ActionStore, ActionCacheError> {
    ActionStore::open_existing_read_only(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
}

fn model_context(root: &Path) -> gmeow_errors::Result<ActionContext> {
    cache_key_with_implementation(root, &fixture_implementation(root)?)
        .map(model_context_for_digest)
}

fn model_cache_error(error: impl Into<Diag>, context: impl Into<String>) -> DocsError {
    DocsError::FixtureCache(PreservedDiagnostic(error.into().with_context(context)))
}

fn model_context_for_digest(input_digest: String) -> ActionContext {
    model_context_for_inputs(input_digest, Vec::new())
}

fn model_context_for_inputs(input_digest: String, mut inputs: Vec<ActionInput>) -> ActionContext {
    inputs.push(ActionInput::Raw {
        logical_path: MODEL_INPUT_CLOSURE.to_owned(),
        file_kind: FileKind::Aggregate,
        executable: false,
        digest: input_digest.clone(),
    });
    ActionContext::new(
        "docs-fixture",
        "model",
        ProducerIdentity::new(input_digest),
        MODEL_CODEC,
        inputs,
    )
}

fn validate_model_context(context: &ActionContext) -> Result<(), ActionCacheError> {
    let catalogs: Vec<_> = context
        .inputs
        .iter()
        .filter(|input| {
            !matches!(
                input,
                ActionInput::Raw { logical_path, .. } if logical_path == MODEL_INPUT_CLOSURE
            )
        })
        .cloned()
        .collect();
    let complete_catalogs = catalogs.len() == 2 * CATALOG_ARTIFACTS.len()
        && CATALOG_ARTIFACTS.iter().all(|(stage, path)| {
            catalogs.iter().any(|input| {
                matches!(
                    input,
                    ActionInput::Raw {
                        logical_path,
                        file_kind: FileKind::File,
                        executable: false,
                        ..
                    } if logical_path.as_str() == *path
                )
            }) && catalogs.iter().any(|input| {
                matches!(
                    input,
                    ActionInput::Upstream {
                        producer,
                        entity: Some(entity),
                        receipt_digest: Some(_),
                        ..
                    } if producer.as_str() == *stage && entity.as_str() == *path
                )
            })
        });
    let valid_catalogs = catalogs.is_empty() || complete_catalogs;
    let expected = model_context_for_inputs(context.implementation.digest.clone(), catalogs);
    if !valid_catalogs || *context != expected {
        return Err(ActionCacheError::message(
            "selected docs model context mismatch",
        ));
    }
    Ok(())
}

fn model_payload(context: &ActionContext) -> DocsActionPayload {
    DocsActionPayload {
        schema_version: 1,
        artifact: "model".to_string(),
        input_digest: context
            .inputs
            .iter()
            .find_map(|input| match input {
                ActionInput::Raw {
                    logical_path,
                    digest,
                    ..
                } if logical_path == MODEL_INPUT_CLOSURE => Some(digest.clone()),
                _ => None,
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
    cached.into_model(cache_path)
}

fn probe_model(
    store: &ActionStore,
    context: &ActionContext,
    cache_path: &Path,
) -> Result<Option<ProducedModel>, ActionCacheError> {
    let Some(entry) = store.get::<DocsActionPayload>(context)? else {
        return Ok(None);
    };
    validate_model_receipt(context, &entry.receipt)?;
    let model = decode_model(cache_path, &entry.bytes)?;
    Ok(Some(ProducedModel::new(model, &entry.receipt)))
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
    validate_model_context(context)?;
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

/// Load or produce the documentation model for an explicit producer operation.
/// Test code must use [`load`] or [`try_load`] instead.
#[must_use]
pub fn load_or_build(root: &Path) -> DocsModel {
    load_or_build_with_identity(root).0
}

/// Producer counterpart of [`load_with_identity`].
#[must_use]
pub fn load_or_build_with_identity(root: &Path) -> (DocsModel, FixtureIdentity) {
    let produced = prepare_model_from_sources(root)
        .unwrap_or_else(|error| panic!("build docs model from live slices: {error}"));
    (produced.model, produced.identity)
}

/// Producer counterpart of [`try_load`].
pub fn try_load_or_build(root: &Path) -> Result<DocsModel, DocsError> {
    prepare_model_from_sources(root).map(|produced| produced.model)
}

/// Produce or admit the source model and retain its exact selected action.
///
/// # Errors
/// Returns model discovery, cache admission, and cache publication failures.
pub fn prepare_model_from_sources(root: &Path) -> Result<ProducedModel, DocsError> {
    let context = model_context(root)
        .map_err(|error| model_cache_error(error, "authenticate documentation fixture inputs"))?;
    prepare_model(root, context, || DocsModel::discover(root))
}

/// The model and exact selected action returned by the cache producer.
pub struct ProducedModel {
    pub model: DocsModel,
    pub identity: FixtureIdentity,
    pub selection: SelectedAction,
}

impl ProducedModel {
    fn new(model: DocsModel, receipt: &ActionReceipt<DocsActionPayload>) -> Self {
        Self {
            model,
            identity: FixtureIdentity::from_receipt(receipt),
            selection: SelectedAction::from_receipt(receipt),
        }
    }
}

/// Produce the exact docs model from authenticated in-memory catalog outputs.
/// Catalog action inputs commit their receipts and artifact bytes.
///
/// # Errors
/// Refuses incomplete catalog identities and model discovery failures.
pub fn prepare_model_from_catalogs(
    root: &Path,
    term_manifest: &[u8],
    constraint_catalog: &[u8],
    inputs: Vec<ActionInput>,
) -> Result<ProducedModel, DocsError> {
    let input_digest = fixture_implementation(root)
        .and_then(|implementation| cache_key_for_paths(root, &implementation, &[]))
        .map_err(|error| model_cache_error(error, "authenticate documentation catalog inputs"))?;
    let context = model_context_for_inputs(input_digest, inputs);
    validate_model_context(&context)
        .map_err(|error| DocsError::FixtureUnavailable(error.to_string()))?;
    if context.inputs.len() != 1 + 2 * CATALOG_ARTIFACTS.len() {
        return Err(DocsError::FixtureUnavailable(
            "live docs fixture requires both authenticated catalog inputs".to_owned(),
        ));
    }
    verify_catalog_bytes(&context, [constraint_catalog, term_manifest])?;
    prepare_model(root, context, || {
        DocsModel::discover_with_manifest_and_catalog(root, term_manifest, constraint_catalog)
    })
}

fn verify_catalog_bytes(context: &ActionContext, artifacts: [&[u8]; 2]) -> Result<(), DocsError> {
    for ((_, path), bytes) in CATALOG_ARTIFACTS.into_iter().zip(artifacts) {
        let input = ActionInput::Raw {
            logical_path: path.to_owned(),
            file_kind: FileKind::File,
            executable: false,
            digest: purrdf::ContentDigest::of(bytes).to_hex(),
        };
        if !context.inputs.contains(&input) {
            return Err(DocsError::FixtureUnavailable(format!(
                "catalog bytes differ from the selected digest for {path}"
            )));
        }
    }
    Ok(())
}

fn prepare_model(
    root: &Path,
    context: ActionContext,
    discover: impl FnOnce() -> Result<DocsModel, DocsError>,
) -> Result<ProducedModel, DocsError> {
    let store = open_action_store(root)
        .map_err(|error| model_cache_error(error, "open documentation action cache"))?;
    let key = context.key();
    let cache_path = cache_path_for_context(root, &context);
    let outcome = store.coordinate::<_, ModelCacheError, _, _>(
        &key,
        || probe_model(&store, &context, &cache_path).map_err(ModelCacheError::from),
        || {
            let model = discover().map_err(ModelCacheError::Build)?;
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
            Ok(ProducedModel::new(model, &receipt))
        },
    );
    match outcome {
        Ok(outcome) => Ok(outcome.value),
        Err(ModelCacheError::Build(error)) => Err(error),
        Err(ModelCacheError::Cache(error)) => Err(model_cache_error(
            error,
            format!("admit or produce action at {}", cache_path.display()),
        )),
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
    let store = open_action_store(root)
        .unwrap_or_else(|error| panic!("open bounded docs-fixture action cache: {error}"));
    let context = model_context(root)
        .unwrap_or_else(|error| panic!("authenticate documentation fixture inputs: {error}"));
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
    let context = model_context_for_digest(cache_key(root));
    cache_path_for_context(root, &context)
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
/// Callers supply their canonical payload re-serialization, rather than raw envelope
/// bytes. The digest is invariant to envelope formatting while sensitive to every
/// payload value a reader consumes. Each payload's existing codec owns those bytes.
///
/// This is the single authority for every fixture envelope's digest, model and renderer
/// artifacts alike — a second copy in the renderer crate is exactly the two-sources-of-truth
/// defect the payload digest exists to catch.
///
#[must_use]
pub fn payload_digest(label: &str, canonical_payload: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(b"gmeow-docs-fixture-payload\x1f");
    hasher.update(label.as_bytes());
    hasher.update(b"\x1e");
    hasher.update(canonical_payload);
    purrdf_hash::hex::encode(&hasher.finalize())
}

/// Require the canonical payload bytes to match the envelope's declared digest.
///
/// # Errors
/// A digest mismatch reports the corrupt cache entry and its path.
pub fn verify_payload(
    cache_path: &Path,
    label: &str,
    declared: &str,
    canonical_payload: &[u8],
) -> Result<(), ActionCacheError> {
    let live = payload_digest(label, canonical_payload);
    if live == declared {
        return Ok(());
    }
    Err(ActionCacheError::message(format!(
        "tampered docs-fixture {label} cache at {}: it declares payload digest {declared} but \
         its content folds to {live}. The entry was edited after it was written — remove the \
         named corrupt action through cache maintenance to rebuild it; an edited cache entry \
         is never served",
        cache_path.display(),
    )))
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

impl CachedModelBody {
    /// Preserve the model's existing Serde payload bytes and serialization failure.
    fn canonical_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_else(|error| {
            panic!("serializing the model cache payload for its digest: {error}")
        })
    }
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
            digest: payload_digest("model", &body.canonical_bytes()),
            body,
        }
    }

    /// Reconstruct the model, first proving the envelope carries the payload it claims.
    fn into_model(self, cache_path: &Path) -> Result<DocsModel, ActionCacheError> {
        verify_payload(
            cache_path,
            "model",
            &self.digest,
            &self.body.canonical_bytes(),
        )?;
        let CachedModelBody {
            mut model,
            available_languages,
            translations,
            ui_catalog,
        } = self.body;
        model.available_languages = available_languages;
        model.translations = translations;
        model.ui_catalog = ui_catalog;
        Ok(model)
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
    fixture_implementation(root)
        .and_then(|implementation| cache_key_with_implementation(root, &implementation))
        .unwrap_or_else(|error| panic!("authenticate documentation fixture inputs: {error}"))
}

fn fixture_implementation(root: &Path) -> gmeow_errors::Result<String> {
    let inventory =
        gmeow_action_cache::executable::current_source_inventory(root, "crates/docs/Cargo.toml")?;
    inventory.digest().map_err(Into::into)
}

fn cache_key_with_implementation(
    root: &Path,
    implementation: &str,
) -> gmeow_errors::Result<String> {
    let paths = CATALOG_ARTIFACTS.map(|(_, path)| path);
    cache_key_for_paths(root, implementation, &paths)
}

fn cache_key_for_paths(
    root: &Path,
    implementation: &str,
    catalog_paths: &[&str],
) -> gmeow_errors::Result<String> {
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
        collect_files(&root.join(dir), &mut files)?;
    }
    files.extend(
        crate::ontology_catalogs::dsl_input_files(root)
            .map_err(|error| error.with_context("authenticate fixture DSL translation inputs"))?,
    );
    // Runtime files discover() reads directly. Registry and Git dependencies are
    // already bound by selected lock records in the shared implementation inventory.
    for file in [
        "docs/four-boxes.md",
        "metadata/gmeow-self.ttl",
        "dsl/mappings/mapping-sets.ttl",
    ]
    .into_iter()
    .chain(catalog_paths.iter().copied())
    {
        let p = root.join(file);
        if p.is_file() {
            files.push(p);
        }
    }
    files.sort();
    files.dedup();

    for path in &files {
        let rel = path.strip_prefix(root).unwrap_or(path);
        // Normalize separators so the key is identical across platforms for the
        // same repository state (Windows `\` vs Unix `/` would otherwise diverge).
        let rel = rel.to_string_lossy().replace('\\', "/");
        hasher.update(rel.as_bytes());
        hasher.update(b"\x1f");
        let bytes = fs::read(path).map_err(|error| {
            Diag::from(error).with_context(format!("hash fixture input {}", path.display()))
        })?;
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
        hasher.update(b"\x1e");
    }

    Ok(purrdf_hash::hex::encode(&hasher.finalize()))
}

/// Recursively collect every regular file under `dir` (absent dir → no files).
fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> gmeow_errors::Result<()> {
    let walk_error = |error| {
        Diag::from(error).with_context(format!("walk fixture input directory {}", dir.display()))
    };
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(walk_error(error)),
    };
    for entry in entries {
        let entry = entry.map_err(walk_error)?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| {
            Diag::from(error).with_context(format!("read fixture input type {}", path.display()))
        })?;
        if file_type.is_dir() {
            collect_files(&path, out)?;
        } else if file_type.is_file() {
            out.push(path);
        } else {
            return Err(ActionCacheError::message(format!(
                "fixture input {} must be a regular file or directory",
                path.display()
            ))
            .into());
        }
    }
    Ok(())
}

#[path = "fixture.tests.rs"]
#[cfg(test)]
mod tests;

#[path = "fixture.cq_rationale.tests.rs"]
#[cfg(test)]
mod cq_rationale_tests;
