// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Once-per-run action cache for the documentation model.
//!
//! Building a [`DocsModel`] via [`DocsModel::discover`] walks the whole slice
//! catalog, parses every `module.ttl`, and folds the i18n catalogs (~12 s). That
//! cost is paid by anything that needs the live model: the gmeow-docs integration
//! suite has ~40 tests that each need it and the test runner executes every test in
//! its own process, `gmeow-dev doc-lint` needs it, and `gmeow-slice-quality`'s
//! `DocMaturity` axis needs it once per repo root. A fresh `discover()` per consumer
//! is paid dozens of times, and when many start at once the concurrent builds
//! contend and each takes far longer than a single build would.
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
//! that build/serialize/render the fixture — the latter DERIVED from the manifests
//! as the transitive local-dependency closure of `crates/docs`, so a crate that joins
//! the build joins the key with nothing to remember. Data, renderer, schema, and
//! local dependency changes therefore invalidate it without relying on a manual
//! version bump. Publication, integrity, quota GC, and cross-process build election
//! come from the workspace's single `gmeow-action-cache` authority.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

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
}

impl FixtureIdentity {
    fn from_receipt(receipt: &ActionReceipt<DocsActionPayload>) -> Self {
        Self {
            receipt_digest: receipt.digest(),
            product_digest: receipt.product_digest.clone(),
        }
    }
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
    let input_digest = cache_key(root);
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
    let context = model_context(root);
    let cache_path = cache_path(root);
    let store = read_only_action_store(root).map_err(|error| {
        DocsError::FixtureUnavailable(format!(
            "authenticated docs action store is unavailable without mutation: {error}"
        ))
    })?;
    match probe_model(&store, &context, &cache_path) {
        Ok(Some(hit)) => Ok(hit),
        Ok(None) => Err(DocsError::FixtureUnavailable(format!(
            "no receipt for action {}; run the explicit corpus producer before starting tests",
            context.key()
        ))),
        Err(error) => panic!(
            "corrupt docs-fixture action cache at {}: {error}",
            cache_path.display()
        ),
    }
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
    let context = model_context(root);
    match store.inspect::<DocsActionPayload>(&context) {
        Ok(Some(receipt)) => {
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
    ActionStore::default_root(root)
        .join(format!("v{STORE_FORMAT_VERSION}"))
        .join("receipts")
        .join(format!("{}.json", model_context(root).key()))
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

/// Content-address the inputs `discover()` reads. The key folds (in order): a
/// salt of the crate version + model schema version, then the sorted
/// `(relative-path, bytes)` of every file under the discovery roots and under
/// every crate in the DERIVED implementation closure ([`fixture_crate_dirs`]).
/// Any slice / shape / i18n / metadata edit changes the key, as does an edit to
/// any crate compiled into the model, the renderer, or the model schema.
///
/// The key is shared by every fixture artifact — this crate's model cache and
/// `gmeow_docs::fixture`'s per-language site and mdBook caches — which is why the
/// derived closure is rooted at `crates/docs` (see [`fixture_crate_dirs`]) even
/// though the model itself is built here. `crates/docs` depends on this crate, so the
/// model's own implementation closure is a SUBSET of what is folded: the model key is
/// never under-approximated, and a renderer-only edit over-invalidates the model
/// cache by one rebuild rather than under-invalidating the site cache by serving a
/// stale render.
///
/// # Panics
/// When a collected input file cannot be read — the tree changed under the walk, or a
/// permission failure; either way the key would be a lie.
#[must_use]
pub fn cache_key(root: &Path) -> String {
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
    // The implementation roots close the stale-cache hole left by the old
    // crate-version-only salt (normal source edits do not bump Cargo's package
    // version on every commit) AND the path-dependency hole: a `path = "../…"`
    // dependency carries no content hash in `Cargo.lock`, so nothing else in the
    // key moves when one is edited. The crate set is DERIVED from the manifests
    // ([`fixture_crate_dirs`]) rather than listed here, so a new local dependency
    // joins the key by construction; `CRATE_INPUT_SUBPATHS` folds each crate's
    // `src/`, `assets/`, `templates/`, `Cargo.toml` and `build.rs`, which is what
    // carries gmeow-docs' own non-source render inputs into the key too.
    for crate_dir in fixture_crate_dirs(root) {
        collect_crate_inputs(&crate_dir, &mut files);
    }
    // Individual files discover() reads directly, plus `Cargo.lock` — which pins
    // every REGISTRY and GIT dependency by checksum/rev (purrdf included), the
    // half of the dependency graph the path-dependency closure does not cover.
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

/// The per-crate subpaths whose bytes decide what a crate compiles to: its
/// sources, its manifest, its build script, and the asset / template trees its
/// `include_str!` / `include_bytes!` sites read. Anything else in a crate
/// directory (`tests`, `benches`, `examples`, `target`) builds the crate's own
/// tests, never the library the fixture is produced by.
const CRATE_INPUT_SUBPATHS: [&str; 5] = ["src", "assets", "templates", "Cargo.toml", "build.rs"];

/// Derived/runtime directories nested below an otherwise-authored asset tree.
///
/// `console/pkg` is the hundreds-of-megabytes package staging tree emitted by the
/// console producer, and `smoke/node_modules` is the Playwright installation. Neither
/// is read by the docs library or compiled into the cached model/site/book, and both
/// are explicitly gitignored at their owning boundary. Folding them into every warm
/// test process would make an output mutate its own input key and repeatedly hash a
/// large non-input tree.
const CRATE_INPUT_EXCLUDED_SUBPATHS: [&str; 5] = [
    // The docs/docs-model fixture modules decide cache admission and persistence;
    // neither changes the model/site/book bytes whose identities they guard.
    "src/fixture.rs",
    "assets/console/pkg",
    "assets/console/smoke",
    "assets/console/tests",
    "assets/tests",
];

/// The repo-root-relative crate directory the implementation closure is rooted at.
///
/// `crates/docs`, not `crates/docs-model`, and deliberately so: the key computed here
/// is shared by the model cache AND by `gmeow_docs::fixture`'s renderer-only site and
/// mdBook caches, so it must cover the renderer's bytes or a template edit would serve
/// a stale rendered site. `crates/docs` depends on `crates/docs-model`, so rooting the
/// closure there is a superset of the model's own closure — over-invalidation, which
/// costs a rebuild, never under-invalidation, which serves stale bytes.
///
/// This is a PATH, not a dependency: nothing here links `gmeow-docs`, and this crate
/// remains a leaf with respect to the renderer.
const FIXTURE_CLOSURE_ROOT: [&str; 2] = ["crates", "docs"];

/// Every local crate whose sources are compiled into the fixture artifacts, as
/// crate directories under `root`.
///
/// This is DERIVED, not declared: it is the transitive closure of `path = "…"`
/// dependency edges starting at [`FIXTURE_CLOSURE_ROOT`], read straight out of the
/// manifests. A hand-maintained mirror of this closure is exactly what let a crate split
/// move the whole documentation model into `crates/docs-model` while the cache key kept
/// hashing the crates it used to live in — an edit to the moved code did not
/// invalidate the cache, so a stale model was served. A derived closure cannot rot:
/// adding a dependency to any crate in it adds that crate's bytes to the key on the
/// next run, with nothing to remember.
///
/// Dev-dependency sections are NOT followed. A dev-dependency is linked into a
/// crate's own tests, never into the library that builds the model / site / book,
/// so its bytes cannot change a cached artifact (`gmeow-mcp` — `gmeow-docs`' test-only
/// query executor — is the live example, and it is also why the model half of this
/// fixture lives here: `gmeow-mcp` depends on `gmeow-slice-quality`, so a
/// `gmeow-slice-quality -> gmeow-docs` edge would close a first-party cycle).
fn fixture_crate_dirs(root: &Path) -> BTreeSet<PathBuf> {
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
    let mut queue = vec![
        FIXTURE_CLOSURE_ROOT
            .iter()
            .fold(root.to_path_buf(), |dir, segment| dir.join(segment)),
    ];
    while let Some(dir) = queue.pop() {
        let dir = normalize_lexically(&dir);
        if !seen.insert(dir.clone()) {
            continue;
        }
        // A crate directory with no readable manifest contributes its own bytes
        // (it is already in `seen`) but no edges — the shape a synthetic test root
        // takes, and a hard-fail here would make the key un-computable for it.
        let Ok(manifest) = fs::read_to_string(dir.join("Cargo.toml")) else {
            continue;
        };
        for dep in manifest_path_deps(&manifest) {
            queue.push(dir.join(dep));
        }
    }
    seen
}

/// The `path = "…"` values of every NON-dev dependency section of a manifest, in
/// declaration order. Sections are tracked by header: any `[…dependencies]` table
/// (plain, `[build-dependencies]`, `[target.'cfg(…)'.dependencies]`) contributes,
/// and its `dev-` counterpart does not. Optional dependencies are followed — a
/// feature this build does not enable can only over-approximate the key, and
/// over-invalidation costs a rebuild while under-invalidation serves stale bytes.
fn manifest_path_deps(manifest: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let mut in_deps = false;
    for line in manifest.lines() {
        let line = line.trim();
        if let Some(header) = line.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
            in_deps = header.ends_with("dependencies") && !header.ends_with("dev-dependencies");
            continue;
        }
        if !in_deps || line.starts_with('#') {
            continue;
        }
        // Scan every `path` occurrence on the line: a dependency whose NAME
        // contains "path" must not shadow the real key (`pathfinder = "1"` has no
        // `=` `"` after its "path", so it falls through to the next occurrence).
        for (idx, _) in line.match_indices("path") {
            let rest = line[idx + "path".len()..].trim_start();
            let Some(rest) = rest.strip_prefix('=') else {
                continue;
            };
            let Some(rest) = rest.trim_start().strip_prefix('"') else {
                continue;
            };
            let Some(end) = rest.find('"') else {
                continue;
            };
            deps.push(rest[..end].to_string());
            break;
        }
    }
    deps
}

/// Resolve `.` / `..` components textually (no filesystem, no symlink resolution),
/// so `crates/docs/../docs-model` and `crates/docs-model` are the same key — and
/// therefore hash their bytes once — however a manifest spelled the edge.
fn normalize_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !out.pop() {
                    out.push(component);
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Collect the hashable inputs of one crate directory: each [`CRATE_INPUT_SUBPATHS`]
/// entry that exists, walked recursively when it is a directory.
fn collect_crate_inputs(crate_dir: &Path, out: &mut Vec<PathBuf>) {
    for sub in CRATE_INPUT_SUBPATHS {
        let path = crate_dir.join(sub);
        if path.is_dir() {
            collect_crate_files(crate_dir, &path, out);
        } else if path.is_file() {
            out.push(path);
        }
    }
}

fn collect_crate_files(crate_dir: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let relative = dir.strip_prefix(crate_dir).unwrap_or(dir);
    if CRATE_INPUT_EXCLUDED_SUBPATHS
        .iter()
        .any(|excluded| relative == Path::new(excluded) || relative.starts_with(excluded))
    {
        return;
    }
    collect_files_with(crate_dir, dir, out, true);
}

/// Recursively collect every regular file under `dir` (absent dir → no files).
fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    collect_files_with(dir, dir, out, false);
}

fn collect_files_with(root: &Path, dir: &Path, out: &mut Vec<PathBuf>, crate_inputs: bool) {
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
            if crate_inputs {
                collect_crate_files(root, &path, out);
            } else {
                collect_files_with(root, &path, out, false);
            }
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

#[cfg(test)]
mod tests {
    //! Hermetic tests for the cache machinery itself — no model build (those are
    //! the gmeow-docs integration suite's job). They pin the model envelope round
    //! trip, the content-addressing contract, the derived implementation closure,
    //! and the integrity-violation panic so a key/envelope regression fails here,
    //! not as a confusing downstream golden.
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

    /// **The write→read proof, across the real serde boundary.** A model envelope
    /// published by [`ActionStore`] and read back by the loader that serves warm hits
    /// must VERIFY.
    ///
    /// An in-memory `from_model(..).into_model(..)` round trip cannot see this class:
    /// the digest is folded over a re-serialization of the payload, so a payload field
    /// that does not survive JSON — one whose `skip_serializing_if` has no matching
    /// `default`, or a map whose iteration order is not the wire order — folds to one
    /// value before the write and another after the read, and the guard fires on every
    /// warm hit even though nothing was edited. That is exactly the failure the SHACL
    /// verdict's own self-digest shipped with (its digest was folded over the pre-render
    /// report while the file carried the normalized one), so the docs fixture's analogous
    /// guard is proven here rather than assumed. The renderer's `CachedSite` twin of this
    /// proof lives beside its envelope in `gmeow_docs::fixture`.
    #[test]
    fn a_model_envelope_written_to_disk_verifies_when_read_back() {
        let (_tmp, root) = temp_root("disk-round-trip");
        let model_path = cache_path(&root);
        let model = DocsModel::default();
        let cached = CachedModel::from_model(&model);
        let bytes = serde_json::to_vec(&cached).unwrap();
        let context = model_context(&root);
        let store = action_store(&root);
        store
            .publish(
                &context,
                cached.digest.clone(),
                model_payload(&context),
                &bytes,
            )
            .unwrap();
        let hit = store
            .get::<DocsActionPayload>(&context)
            .unwrap()
            .expect("warm model action");
        let recovered = decode_model(&model_path, &hit.bytes).unwrap();
        assert_eq!(
            recovered.available_languages, model.available_languages,
            "the reattached i18n fields survive the disk round trip"
        );
    }

    /// The model envelope carries the payload guard, over the whole reconstructed payload.
    ///
    /// This is the whole point of the payload digest: the key content-addresses the
    /// INPUTS, so editing the cached OUTPUT leaves it satisfied. `.cache/` is gitignored
    /// and persists, so an entry edited once would keep being served — and the
    /// `DocMaturity` quality axis reads its coverage computation straight out of it.
    #[test]
    #[should_panic(expected = "tampered docs-fixture model cache")]
    fn an_edited_model_envelope_is_refused() {
        let mut cached = CachedModel::from_model(&DocsModel::default());
        // The hand-edit: claim a language the builder never found.
        cached.body.available_languages.push("klingon".to_string());
        let _ = cached.into_model(Path::new("<in-memory>"));
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

    #[test]
    fn derived_console_trees_do_not_join_the_fixture_key() {
        let (_tmp, root) = temp_root("derived-console");
        let assets = root.join("crates/docs/assets");
        fs::create_dir_all(assets.join("console/pkg")).unwrap();
        fs::create_dir_all(assets.join("console/smoke/node_modules/tool")).unwrap();
        fs::write(assets.join("console/pkg/gmeow.gts"), b"derived-v1").unwrap();
        fs::write(
            assets.join("console/smoke/node_modules/tool/index.js"),
            b"installed-v1",
        )
        .unwrap();
        let base = cache_key(&root);

        fs::write(assets.join("console/pkg/gmeow.gts"), b"derived-v2").unwrap();
        fs::write(
            assets.join("console/smoke/node_modules/tool/index.js"),
            b"installed-v2",
        )
        .unwrap();
        assert_eq!(
            base,
            cache_key(&root),
            "producer output and installed test dependencies are not fixture inputs"
        );

        fs::write(assets.join("gmeow.css"), b"authored asset").unwrap();
        assert_ne!(
            base,
            cache_key(&root),
            "an authored renderer asset must still invalidate the fixture"
        );
    }

    /// The repository root of THIS checkout (`crates/docs-model/` → up two).
    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("crates/docs-model has a grandparent")
            .to_path_buf()
    }

    /// THE cache-correctness gate: a dependency crate's bytes are in the key, and a
    /// NEWLY ADDED dependency edge joins the key with nothing to remember.
    ///
    /// The hole this pins is not hypothetical: the key once hashed a hand-written
    /// crate list, a crate split moved the documentation model into a crate absent
    /// from that list, and every later edit to the moved code read back a stale
    /// cached model. The closure is derived from the manifests now, so this test
    /// fails for any list-based key regression.
    #[test]
    fn a_dependency_crates_sources_join_the_cache_key() {
        let (_tmp, root) = temp_root("dep-closure");
        let docs = root.join("crates/docs");
        fs::create_dir_all(docs.join("src")).unwrap();
        fs::create_dir_all(root.join("crates/alpha/src")).unwrap();
        fs::write(
            docs.join("Cargo.toml"),
            b"[dependencies]\ngmeow-alpha = { path = \"../alpha\" }\n",
        )
        .unwrap();
        fs::write(root.join("crates/alpha/src/lib.rs"), b"alpha v1").unwrap();

        let base = cache_key(&root);
        fs::write(root.join("crates/alpha/src/lib.rs"), b"alpha v2").unwrap();
        let edited = cache_key(&root);
        assert_ne!(
            base, edited,
            "editing a dependency crate's source must invalidate the fixture cache"
        );

        // A brand-new dependency edge — the exact change that silently rotted a
        // hand-written list — joins the key on the next run.
        fs::create_dir_all(root.join("crates/beta/src")).unwrap();
        fs::write(root.join("crates/beta/src/lib.rs"), b"beta v1").unwrap();
        let unreferenced = cache_key(&root);
        assert_eq!(
            edited, unreferenced,
            "a crate nothing depends on is not compiled in and must not be hashed"
        );
        fs::write(
            docs.join("Cargo.toml"),
            b"[dependencies]\ngmeow-alpha = { path = \"../alpha\" }\n\
              gmeow-beta = { path = \"../beta\" }\n",
        )
        .unwrap();
        let with_beta = cache_key(&root);
        assert_ne!(
            unreferenced, with_beta,
            "declaring the dependency must pull the new crate into the key"
        );
        fs::write(root.join("crates/beta/src/lib.rs"), b"beta v2").unwrap();
        let beta_edited = cache_key(&root);
        assert_ne!(
            with_beta, beta_edited,
            "the newly declared crate's later edits must invalidate the cache too"
        );

        // A dependency crate's MANIFEST is hashed too (`CRATE_INPUT_SUBPATHS`): a
        // feature flip or a version bump there changes what compiles into the model
        // without touching a single `.rs` byte.
        fs::write(root.join("crates/beta/Cargo.toml"), b"[package]\n").unwrap();
        assert_ne!(
            beta_edited,
            cache_key(&root),
            "editing a dependency crate's Cargo.toml must invalidate the cache"
        );
    }

    /// A dev-dependency is linked into a crate's TESTS, never into the library that
    /// builds the cached model / site / book, so its bytes must not be in the key —
    /// otherwise every edit to the test-only `gmeow-mcp` query executor would throw
    /// away the whole fixture.
    #[test]
    fn dev_dependencies_are_not_hashed() {
        let (_tmp, root) = temp_root("dev-deps");
        let docs = root.join("crates/docs");
        fs::create_dir_all(docs.join("src")).unwrap();
        fs::create_dir_all(root.join("crates/testonly/src")).unwrap();
        fs::write(
            docs.join("Cargo.toml"),
            b"[dev-dependencies]\ngmeow-testonly = { path = \"../testonly\" }\n",
        )
        .unwrap();
        fs::write(root.join("crates/testonly/src/lib.rs"), b"v1").unwrap();
        let base = cache_key(&root);
        fs::write(root.join("crates/testonly/src/lib.rs"), b"v2").unwrap();
        assert_eq!(
            base,
            cache_key(&root),
            "a dev-dependency must not be hashed"
        );
    }

    /// The derived closure over the LIVE manifests is genuinely transitively closed
    /// and reaches the documentation model. A crate that declares a path dependency
    /// the closure does not contain would be a crate whose edits are invisible to
    /// the cache — the defect class this whole derivation exists to make impossible.
    ///
    /// It also pins the direction of the split: the closure is rooted at the RENDERER
    /// (`crates/docs`) and reaches THIS crate, so the renderer's bytes are folded into
    /// the key its site/book caches hang off, and the model crate's bytes are folded
    /// into the key its own cache hangs off.
    #[test]
    fn live_manifest_closure_is_closed_and_reaches_the_model() {
        let root = repo_root();
        let dirs = fixture_crate_dirs(&root);
        assert!(
            dirs.contains(&root.join("crates/docs")),
            "the renderer crate is the closure root and must be hashed: {dirs:?}"
        );
        assert!(
            dirs.contains(&root.join("crates/docs-model")),
            "the documentation model crate must be in the hashed closure: {dirs:?}"
        );
        for dir in &dirs {
            let Ok(manifest) = fs::read_to_string(dir.join("Cargo.toml")) else {
                continue;
            };
            for dep in manifest_path_deps(&manifest) {
                let resolved = normalize_lexically(&dir.join(&dep));
                assert!(
                    dirs.contains(&resolved),
                    "{} depends on {dep} but {} is not hashed into the fixture cache key",
                    dir.display(),
                    resolved.display()
                );
            }
        }
    }

    #[test]
    fn manifest_path_deps_reads_every_non_dev_section() {
        let manifest = "\
[package]\n\
name = \"x\"\n\
[dependencies]\n\
serde.workspace = true\n\
gmeow-a = { path = \"../a\" }\n\
# gmeow-commented = { path = \"../commented\" }\n\
pathological = \"1\"\n\
[target.'cfg(not(target_arch = \"wasm32\"))'.dependencies]\n\
gmeow-b = { path = \"../b\" }\n\
[build-dependencies]\n\
gmeow-c = { path = \"../c\" }\n\
[dev-dependencies]\n\
gmeow-d = { path = \"../d\" }\n\
[target.'cfg(unix)'.dev-dependencies]\n\
gmeow-e = { path = \"../e\" }\n";
        assert_eq!(
            manifest_path_deps(manifest),
            vec!["../a".to_string(), "../b".to_string(), "../c".to_string()],
        );
    }

    #[test]
    fn lexical_normalization_dedupes_equivalent_crate_paths() {
        assert_eq!(
            normalize_lexically(Path::new("/repo/crates/docs/../docs-model")),
            PathBuf::from("/repo/crates/docs-model")
        );
        assert_eq!(
            normalize_lexically(Path::new("/repo/./crates/./ns")),
            PathBuf::from("/repo/crates/ns")
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
    fn model_cache_path_is_the_shared_action_receipt() {
        let (_tmp, root) = temp_root("paths");
        let path = cache_path(&root);
        assert_eq!(path.parent().unwrap().file_name().unwrap(), "receipts");
        let name = path.file_name().unwrap().to_string_lossy();
        assert_eq!(name.len(), 69, "64 hex digits plus .json");
        assert!(name.ends_with(".json"));
        assert!(name[..64].bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    #[test]
    #[should_panic(expected = "load authenticated docs model")]
    fn present_but_corrupt_model_cache_panics() {
        let (_tmp, root) = temp_root("corrupt-model");
        let cp = cache_path(&root);
        fs::create_dir_all(cp.parent().unwrap()).unwrap();
        fs::write(&cp, b"{ not valid json").unwrap();
        let _ = load(&root);
    }
}
