// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact, cross-process fixtures for tests that assert on expensive pipeline output.
//!
//! Test-facing loaders in this module are consumers only. They authenticate products
//! already admitted by the production DAG and hard-fail on a miss. They never execute a
//! stage, dependency, scheduler, or fallback producer. [`prime_stage_fixtures`] is the
//! explicitly named producer entry point used by the pre-test producer stage; test code
//! is forbidden from calling it.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_action_cache::{
    ActionContext, ActionInput, ActionStore, ProducerIdentity, STORE_FORMAT_VERSION, StoreLimits,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::cache::{
    BUILD_FEATURES, BUILD_FINGERPRINT, BUILD_PROFILE, BUILD_TARGET, FixtureOutcome, PipelineCache,
    StageReceipt, TOOLCHAIN_FINGERPRINT, stage_key,
};
use crate::loader::bind;
use crate::node::{CachePolicy, StageProduct, StageStability};
use crate::registry::default_registry;
use crate::run::full_spec;
use crate::scheduler::{
    RunContext, action_key_context_from_receipts_cached, dependency_closure, run_targets,
};

const STAGE_FIXTURE_MANIFEST_SCHEMA_VERSION: u32 = 2;
const SETTLED_SOURCE_LOAD_WITNESS_CODEC: &str = "json:settled-source-load-witness:v1";

/// Producer-written selector for the exact pipeline actions admitted to tests.
pub const STAGE_FIXTURE_MANIFEST_RELATIVE_PATH: &str =
    ".cache/gmeow-sync/test-fixture-manifest-v2.json";

/// Runner-supplied path to the immutable stage-fixture selector.
pub const STAGE_FIXTURE_MANIFEST_PATH_ENV: &str = "GMEOW_TEST_FIXTURE_MANIFEST";

/// Runner-supplied SHA-256 of the immutable stage-fixture selector.
pub const STAGE_FIXTURE_MANIFEST_SHA256_ENV: &str = "GMEOW_TEST_FIXTURE_MANIFEST_SHA256";

/// Complete set of production-stage identities consumed by tests.
///
/// The explicit pre-test producer and the read-only verifier both iterate this
/// list. Keeping the ownership here prevents a test from adding a stage-backed
/// fixture without also making that product an authenticated DAG prerequisite.
pub const AUTHENTICATED_TEST_STAGE_IDS: &[&str] = &[
    "stage-conformance",
    "stage-source-load",
    "stage-compile-logic",
    "stage-export-constraint-shapes",
    "stage-mappings",
    "stage-slice-brief",
    "stage-validate-result-shape-composition",
    "stage-export-agreement",
    "stage-constraint-catalog",
    "stage-export-frame-shapes",
    "stage-term-manifest",
    "stage-export-apache",
    "stage-export-bench",
    "stage-export-catalog",
    "stage-export-cost-ledger",
    "stage-export-evals",
    "stage-export-json-schema",
    "stage-export-matrix",
    "stage-export-metadata",
    "stage-export-pydantic",
    "stage-export-profiles",
    "stage-export-governance-floors",
    "stage-export-projection-ceilings",
    "stage-export-references",
    "stage-export-research-objects",
    "stage-export-result-shapes",
    "stage-export-schemas",
    "stage-export-skos-surface",
    "stage-export-lpg",
];

/// One exact production-stage fixture selected by the pre-test producer manifest.
#[derive(Debug)]
pub struct StageFixture {
    /// The verified stage product, receipt, and build/hydration telemetry.
    pub outcome: FixtureOutcome,
}

#[derive(Debug, Serialize, Deserialize)]
struct StageFixtureManifest {
    schema_version: u32,
    build_fingerprint: String,
    /// Every receipt in the selected targets' transitive producer closure.
    ///
    /// Persistent receipts authenticate their product blobs. Recompute-aggregate
    /// receipts carry the deterministic product/entity identities needed to derive
    /// downstream action contexts without hydrating their carriers on a warm run.
    closure_receipts: BTreeMap<String, StageReceipt>,
    stages: BTreeMap<String, StageReceipt>,
}

/// Authenticated identity of the scheduler's one deterministic mid-DAG carrier trim.
///
/// `stage-source-load` is persisted with its diagnostics span table because the early
/// validation consumers require it. After their last declared level, the scheduler
/// removes that transient blob before `stage-snapshot` consumes the product. The cache
/// receipt therefore authenticates the pre-trim digest, while snapshot's action context
/// correctly names the post-trim digest. This tiny derived action witnesses that exact
/// transition without hydrating the multi-gigabyte RDF carrier on every warm admission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SettledSourceLoadWitness {
    schema_version: u32,
    source_action_key: String,
    source_receipt_digest: String,
    source_product_digest: String,
    settled_product_digest: String,
}

/// Identity of the producer-written stage-fixture selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageFixtureManifestIdentity {
    /// Worktree-local path written atomically by the explicit producer.
    pub path: PathBuf,
    /// SHA-256 the runner must provide to every consumer process.
    pub sha256: String,
    /// Exact number of selected stage actions.
    pub stage_count: usize,
}

/// A warm producer admission proved from current raw inputs and exact cached receipts.
#[derive(Debug)]
pub struct ReusedStageFixtures {
    /// Identity the runner must pass to every test process.
    pub manifest: StageFixtureManifestIdentity,
    /// Selected receipts in [`AUTHENTICATED_TEST_STAGE_IDS`] order.
    pub receipts: Vec<StageReceipt>,
}

fn fixture_miss(stage_id: &str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: stage_id.to_string(),
        message: concat!(
            "authenticated corpus fixture is absent; tests are consumers and may not ",
            "run any producer or rebuild fallback"
        )
        .to_string(),
    })
}

fn fixture_manifest_error(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "test-fixture-manifest".to_string(),
        message: message.into(),
    })
}

fn warm_manifest_miss(reason: impl std::fmt::Display) -> Option<ReusedStageFixtures> {
    eprintln!("pipeline fixture warm admission: miss reason={reason}");
    None
}

fn action_cache_error(error: gmeow_action_cache::ActionCacheError) -> gmeow_errors::Diag {
    fixture_manifest_error(format!("lifecycle witness action cache: {error}"))
}

fn settled_source_load_context(source: &StageReceipt) -> ActionContext {
    let mut implementation = ProducerIdentity::new(BUILD_FINGERPRINT);
    implementation.toolchain = Some(TOOLCHAIN_FINGERPRINT.to_string());
    implementation.target = Some(BUILD_TARGET.to_string());
    implementation.profile = Some(BUILD_PROFILE.to_string());
    implementation.features = BUILD_FEATURES
        .split(',')
        .filter(|feature| !feature.is_empty())
        .map(str::to_owned)
        .collect();
    ActionContext::new(
        "pipeline-lifecycle",
        "stage-source-load:settled",
        implementation,
        SETTLED_SOURCE_LOAD_WITNESS_CODEC,
        vec![ActionInput::Upstream {
            producer: source.context.stage_id.clone(),
            entity: Some(crate::stages::carrier::REP_SPAN_TABLE.to_string()),
            receipt_digest: Some(source.digest()),
            product_digest: source.product_digest.clone(),
        }],
    )
    .with_dimension("transition", "strip-after-last-span-consumer")
}

fn validate_settled_source_load_witness(
    source: &StageReceipt,
    common_product_digest: &str,
    witness: &SettledSourceLoadWitness,
    bytes: &[u8],
) -> Result<String, gmeow_errors::Diag> {
    let expected_bytes = serde_json::to_vec(witness)
        .map_err(|error| fixture_manifest_error(format!("encode lifecycle witness: {error}")))?;
    if witness.schema_version != 1
        || witness.source_action_key != source.action_key
        || witness.source_receipt_digest != source.digest()
        || witness.source_product_digest != source.product_digest
        || witness.settled_product_digest != common_product_digest
        || expected_bytes != bytes
    {
        return Err(fixture_manifest_error(
            "settled source-load witness does not match its authenticated source receipt",
        ));
    }
    Ok(witness.settled_product_digest.clone())
}

/// Resolve or produce the tiny authenticated witness for source-load's post-span
/// identity. A witness miss hydrates ONLY the already-produced source-load cache entry,
/// applies the scheduler's deterministic trim, and publishes the resulting digest. It
/// never executes a stage or reads/rebuilds the authored corpus.
fn settled_source_load_digest(
    root: &Path,
    cache: &PipelineCache,
    source: &StageReceipt,
) -> Result<String, gmeow_errors::Diag> {
    let context = settled_source_load_context(source);
    let store_root = ActionStore::default_root(root);
    let read_store = ActionStore::open_existing_read_only(
        &store_root,
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(action_cache_error)?;
    if let Some(entry) = read_store
        .get::<SettledSourceLoadWitness>(&context)
        .map_err(action_cache_error)?
    {
        return validate_settled_source_load_witness(
            source,
            &entry.receipt.product_digest,
            &entry.receipt.payload,
            &entry.bytes,
        );
    }
    drop(read_store);

    let hit = cache.get(&source.context)?.ok_or_else(|| {
        fixture_manifest_error("source-load cache entry vanished during admission")
    })?;
    if hit.receipt != *source {
        return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
            expected: source.digest(),
            actual: hit.receipt.digest(),
        }));
    }
    let stripped = crate::bundle::strip_rep_blob(
        hit.product.bundle(),
        crate::stages::carrier::REP_SPAN_TABLE,
    )?;
    let settled = StageProduct::from_bundle(hit.product.stage_id.clone(), Arc::new(stripped));
    let witness = SettledSourceLoadWitness {
        schema_version: 1,
        source_action_key: source.action_key.clone(),
        source_receipt_digest: source.digest(),
        source_product_digest: source.product_digest.clone(),
        settled_product_digest: settled.digest.clone(),
    };
    let bytes = serde_json::to_vec(&witness)
        .map_err(|error| fixture_manifest_error(format!("encode lifecycle witness: {error}")))?;
    let write_store = ActionStore::open_existing_writable(
        &store_root,
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(action_cache_error)?;
    let receipt = write_store
        .publish(
            &context,
            witness.settled_product_digest.clone(),
            witness.clone(),
            &bytes,
        )
        .map_err(action_cache_error)?;
    validate_settled_source_load_witness(source, &receipt.product_digest, &receipt.payload, &bytes)
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn validate_stage_fixture_manifest(
    manifest: &StageFixtureManifest,
) -> Result<(), gmeow_errors::Diag> {
    if manifest.schema_version != STAGE_FIXTURE_MANIFEST_SCHEMA_VERSION {
        return Err(fixture_manifest_error(format!(
            "schema {} != expected {STAGE_FIXTURE_MANIFEST_SCHEMA_VERSION}",
            manifest.schema_version
        )));
    }
    if manifest.build_fingerprint.len() != 64
        || !manifest
            .build_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(fixture_manifest_error(
            "producer build fingerprint is not a SHA-256 identity",
        ));
    }

    let expected: BTreeSet<&str> = AUTHENTICATED_TEST_STAGE_IDS.iter().copied().collect();
    let actual: BTreeSet<&str> = manifest.stages.keys().map(String::as_str).collect();
    if actual != expected {
        let missing = expected.difference(&actual).copied().collect::<Vec<_>>();
        let extra = actual.difference(&expected).copied().collect::<Vec<_>>();
        return Err(fixture_manifest_error(format!(
            "selected stage set mismatch: missing={missing:?} extra={extra:?}"
        )));
    }

    for (stage_id, receipt) in &manifest.stages {
        if receipt.context.stage_id != *stage_id {
            return Err(fixture_manifest_error(format!(
                "selector key {stage_id} carries receipt for {}",
                receipt.context.stage_id
            )));
        }
        if receipt.context.build.fingerprint != manifest.build_fingerprint {
            return Err(fixture_manifest_error(format!(
                "{stage_id} receipt build fingerprint {} != selected producer {}",
                receipt.context.build.fingerprint, manifest.build_fingerprint
            )));
        }
        let expected_action = stage_key(&receipt.context);
        if receipt.action_key != expected_action {
            return Err(fixture_manifest_error(format!(
                "{stage_id} action key {} != context-derived {expected_action}",
                receipt.action_key
            )));
        }
        if receipt.stability != StageStability::StablePrefix.iri()
            || receipt.cache_disposition != CachePolicy::Persistent.iri()
        {
            return Err(fixture_manifest_error(format!(
                "{stage_id} selector is not a stable persistent action: {} / {}",
                receipt.stability, receipt.cache_disposition
            )));
        }
        if manifest.closure_receipts.get(stage_id) != Some(receipt) {
            return Err(fixture_manifest_error(format!(
                "selected {stage_id} receipt is absent from or differs from the producer closure"
            )));
        }
    }

    for (stage_id, receipt) in &manifest.closure_receipts {
        if receipt.context.stage_id != *stage_id {
            return Err(fixture_manifest_error(format!(
                "closure key {stage_id} carries receipt for {}",
                receipt.context.stage_id
            )));
        }
        if receipt.context.build.fingerprint != manifest.build_fingerprint {
            return Err(fixture_manifest_error(format!(
                "{stage_id} closure receipt build fingerprint {} != selected producer {}",
                receipt.context.build.fingerprint, manifest.build_fingerprint
            )));
        }
        let expected_action = stage_key(&receipt.context);
        if receipt.action_key != expected_action {
            return Err(fixture_manifest_error(format!(
                "{stage_id} closure action key {} != context-derived {expected_action}",
                receipt.action_key
            )));
        }
    }
    Ok(())
}

/// Atomically publish the exact selected receipts emitted by the explicit producer.
///
/// The returned SHA-256 is runner state, not a discoverable fallback: every test
/// process must receive it through [`STAGE_FIXTURE_MANIFEST_SHA256_ENV`].
pub fn publish_stage_fixture_manifest(
    root: &Path,
    run_receipts: &[StageReceipt],
) -> Result<StageFixtureManifestIdentity, gmeow_errors::Diag> {
    let selected: BTreeSet<&str> = AUTHENTICATED_TEST_STAGE_IDS.iter().copied().collect();
    let mut closure_receipts = BTreeMap::new();
    let mut stages = BTreeMap::new();
    for receipt in run_receipts {
        let stage_id = receipt.context.stage_id.as_str();
        if closure_receipts
            .insert(stage_id.to_string(), receipt.clone())
            .is_some()
        {
            return Err(fixture_manifest_error(format!(
                "producer emitted duplicate closure receipt for {stage_id}"
            )));
        }
        if selected.contains(stage_id) {
            stages.insert(stage_id.to_string(), receipt.clone());
        }
    }
    let manifest = StageFixtureManifest {
        schema_version: STAGE_FIXTURE_MANIFEST_SCHEMA_VERSION,
        build_fingerprint: BUILD_FINGERPRINT.to_string(),
        closure_receipts,
        stages,
    };
    validate_stage_fixture_manifest(&manifest)?;

    let mut bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| fixture_manifest_error(format!("encode selector: {error}")))?;
    bytes.push(b'\n');
    let digest = sha256(&bytes);
    let path = root.join(STAGE_FIXTURE_MANIFEST_RELATIVE_PATH);
    let parent = path
        .parent()
        .ok_or_else(|| fixture_manifest_error("selector path has no parent"))?;
    std::fs::create_dir_all(parent).map_err(|error| {
        fixture_manifest_error(format!(
            "create selector directory {}: {error}",
            parent.display()
        ))
    })?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|error| {
        fixture_manifest_error(format!("create selector temporary file: {error}"))
    })?;
    temporary
        .write_all(&bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| fixture_manifest_error(format!("write selector: {error}")))?;
    temporary.persist(&path).map_err(|error| {
        fixture_manifest_error(format!(
            "publish selector {}: {}",
            path.display(),
            error.error
        ))
    })?;

    Ok(StageFixtureManifestIdentity {
        path,
        sha256: digest,
        stage_count: manifest.stages.len(),
    })
}

/// Admit an unchanged producer closure without hydrating or rebuilding any carrier.
///
/// The prior manifest is only a candidate. This function rebinds the current authored
/// DAG, re-hashes every declared raw input, derives every action context from its exact
/// upstream receipts, and authenticates every stable/persistent cache blob through
/// [`PipelineCache::inspect_receipt`]. A normal miss or source/code/DAG change returns
/// `Ok(None)` so the explicit producer can execute the DAG once. Corrupt cache content
/// remains a hard error.
pub fn reuse_stage_fixture_manifest(
    root: &Path,
) -> Result<Option<ReusedStageFixtures>, gmeow_errors::Diag> {
    let path = root.join(STAGE_FIXTURE_MANIFEST_RELATIVE_PATH);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(warm_manifest_miss("selector-not-found"));
        }
        Err(error) => {
            return Err(fixture_manifest_error(format!(
                "read warm selector {}: {error}",
                path.display()
            )));
        }
    };
    let manifest: StageFixtureManifest = match serde_json::from_slice(&bytes) {
        Ok(manifest) => manifest,
        Err(error) => {
            return Ok(warm_manifest_miss(format_args!(
                "selector-decode error={error}"
            )));
        }
    };
    if manifest.schema_version != STAGE_FIXTURE_MANIFEST_SCHEMA_VERSION
        || manifest.build_fingerprint != BUILD_FINGERPRINT
    {
        return Ok(warm_manifest_miss("selector-schema-or-build-fingerprint"));
    }
    validate_stage_fixture_manifest(&manifest)?;

    let spec = full_spec();
    let graph = spec.validate()?;
    let bound = bind(&spec, &graph, &default_registry())?;
    let targets = AUTHENTICATED_TEST_STAGE_IDS
        .iter()
        .map(|stage| (*stage).to_string())
        .collect::<BTreeSet<_>>();
    let expected_closure = dependency_closure(&bound, &targets)?;
    let actual_closure = manifest
        .closure_receipts
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    if actual_closure != expected_closure {
        return Ok(warm_manifest_miss("dependency-closure-changed"));
    }

    let by_id = bound
        .iter()
        .map(|stage| (stage.id(), stage))
        .collect::<BTreeMap<_, _>>();
    let cache = PipelineCache::open_existing_default_read_only(root)?;
    let mut admitted = BTreeMap::new();
    let mut raw_input_digests = BTreeMap::new();
    for stage_id in graph
        .order()
        .into_iter()
        .filter(|stage_id| expected_closure.contains(stage_id))
    {
        let stage = by_id
            .get(stage_id.as_str())
            .expect("bound graph contains every closure stage");
        let upstream = stage
            .consumes()
            .iter()
            .map(|producer| {
                admitted
                    .get(producer)
                    .cloned()
                    .map(|receipt| (producer.clone(), receipt))
                    .ok_or_else(|| {
                        fixture_manifest_error(format!(
                            "warm closure reached {stage_id} before producer {producer}"
                        ))
                    })
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let mut current_context = action_key_context_from_receipts_cached(
            stage.as_ref(),
            root,
            &upstream,
            &mut raw_input_digests,
        )?;
        if stage_id == "stage-snapshot" {
            let source = upstream.get("stage-source-load").ok_or_else(|| {
                fixture_manifest_error(
                    "stage-snapshot warm admission has no stage-source-load receipt",
                )
            })?;
            let settled_digest = settled_source_load_digest(root, &cache, source)?;
            let source_row = current_context
                .upstream
                .iter_mut()
                .find(|row| row.producer == "stage-source-load" && row.entity.is_none())
                .ok_or_else(|| {
                    fixture_manifest_error(
                        "stage-snapshot action context has no whole stage-source-load input",
                    )
                })?;
            source_row.digest = settled_digest;
            current_context.upstream.sort();
            current_context.upstream.dedup();
        }
        let recorded = manifest
            .closure_receipts
            .get(&stage_id)
            .expect("closure set equality proved receipt presence");
        if recorded.context != current_context
            || recorded.stability != stage.stability().iri()
            || recorded.cache_disposition != stage.cache_policy().iri()
        {
            let recorded_upstream = recorded
                .context
                .upstream
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>();
            let current_upstream = current_context
                .upstream
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>();
            let recorded_raw = recorded
                .context
                .raw_inputs
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>();
            let current_raw = current_context
                .raw_inputs
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>();
            eprintln!(
                "pipeline fixture warm admission: stage={stage_id} recorded-only-upstream={:?} current-only-upstream={:?} recorded-only-raw={:?} current-only-raw={:?}",
                recorded_upstream
                    .difference(&current_upstream)
                    .collect::<Vec<_>>(),
                current_upstream
                    .difference(&recorded_upstream)
                    .collect::<Vec<_>>(),
                recorded_raw.difference(&current_raw).collect::<Vec<_>>(),
                current_raw.difference(&recorded_raw).collect::<Vec<_>>(),
            );
            return Ok(warm_manifest_miss(format_args!(
                "stage-context-changed stage={stage_id}"
            )));
        }
        if stage.stability() == StageStability::StablePrefix
            && stage.cache_policy() == CachePolicy::Persistent
        {
            let Some(cached) = cache.inspect_receipt(&current_context)? else {
                return Ok(warm_manifest_miss(format_args!(
                    "stage-cache-entry-absent stage={stage_id}"
                )));
            };
            if cached != *recorded {
                return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                    expected: recorded.digest(),
                    actual: cached.digest(),
                }));
            }
        }
        admitted.insert(stage_id, recorded.clone());
    }

    let receipts = AUTHENTICATED_TEST_STAGE_IDS
        .iter()
        .map(|stage_id| {
            admitted
                .get(*stage_id)
                .cloned()
                .expect("selected targets are members of the admitted closure")
        })
        .collect();
    Ok(Some(ReusedStageFixtures {
        manifest: StageFixtureManifestIdentity {
            path,
            sha256: sha256(&bytes),
            stage_count: manifest.stages.len(),
        },
        receipts,
    }))
}

fn load_stage_fixture_manifest(root: &Path) -> Result<StageFixtureManifest, gmeow_errors::Diag> {
    let selected_path = std::env::var_os(STAGE_FIXTURE_MANIFEST_PATH_ENV)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            fixture_manifest_error(format!(
                "{STAGE_FIXTURE_MANIFEST_PATH_ENV} is required; tests may not discover or produce a fallback selector"
            ))
        })?;
    let selected_path = PathBuf::from(selected_path);
    let path = if selected_path.is_absolute() {
        selected_path
    } else {
        root.join(selected_path)
    };
    let expected = std::env::var(STAGE_FIXTURE_MANIFEST_SHA256_ENV)
        .ok()
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| {
            fixture_manifest_error(format!(
                "{STAGE_FIXTURE_MANIFEST_SHA256_ENV} must select one exact SHA-256; tests may not infer it"
            ))
        })?
        .to_ascii_lowercase();
    let bytes = std::fs::read(&path).map_err(|error| {
        fixture_manifest_error(format!("read selector {}: {error}", path.display()))
    })?;
    let actual = sha256(&bytes);
    if actual != expected {
        return Err(fixture_manifest_error(format!(
            "selector identity mismatch: expected {expected}, actual {actual}"
        )));
    }
    let manifest: StageFixtureManifest = serde_json::from_slice(&bytes)
        .map_err(|error| fixture_manifest_error(format!("decode selector: {error}")))?;
    validate_stage_fixture_manifest(&manifest)?;
    Ok(manifest)
}

fn selected_stage_receipt(root: &Path, stage_id: &str) -> Result<StageReceipt, gmeow_errors::Diag> {
    load_stage_fixture_manifest(root)?
        .stages
        .remove(stage_id)
        .ok_or_else(|| fixture_miss(stage_id))
}

/// Load one authenticated stable/persistent production-stage action.
///
/// This is the test-facing API. It hydrates the exact action named by the
/// producer-authored selector and fails closed if the selector or action is absent. It
/// never traverses dependencies, invokes [`crate::scheduler::run_targets`], or invokes
/// [`crate::node::Stage::run`].
pub fn stage_fixture(
    root: &Path,
    _jobs: usize,
    stage_id: &str,
) -> Result<StageFixture, gmeow_errors::Diag> {
    let expected_receipt = selected_stage_receipt(root, stage_id)?;
    let cache = PipelineCache::open_existing_default_read_only(root)?;
    let hit = cache
        .get(&expected_receipt.context)?
        .ok_or_else(|| fixture_miss(stage_id))?;
    if hit.receipt != expected_receipt {
        return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
            expected: expected_receipt.digest(),
            actual: hit.receipt.digest(),
        }));
    }
    let outcome = FixtureOutcome {
        product: hit.product,
        receipt: hit.receipt,
        built: false,
        transferred_bytes: hit.hydrated_bytes,
    };
    Ok(StageFixture { outcome })
}

/// Produce real stable/persistent stage actions in one DAG traversal before any
/// test process starts.
///
/// This API exists only for the explicit fixture-producer executable. The complete
/// target set is scheduled together, so shared dependencies execute or hydrate once
/// rather than once per requested fixture. Tests must use [`stage_fixture`], whose
/// miss path is terminal.
pub fn prime_stage_fixtures(
    root: &Path,
    jobs: usize,
    stage_ids: &[&str],
) -> Result<crate::scheduler::RunResult, gmeow_errors::Diag> {
    let spec = full_spec();
    let graph = spec.validate()?;
    let bound = bind(&spec, &graph, &default_registry())?;
    if stage_ids.is_empty() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "test-corpus-producer".to_string(),
            message: "fixture producer target set must not be empty".to_string(),
        }));
    }
    for stage_id in stage_ids {
        let stage = bound
            .iter()
            .find(|stage| stage.id() == *stage_id)
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: (*stage_id).to_string(),
                    message: format!("production DAG does not bind {stage_id}"),
                })
            })?;
        if stage.stability() != StageStability::StablePrefix
            || stage.cache_policy() != CachePolicy::Persistent
        {
            return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: stage.id().to_string(),
                message: format!(
                    "cross-process fixture requires stable persistent admission, got {} / {}",
                    stage.stability().iri(),
                    stage.cache_policy().iri()
                ),
            }));
        }
    }

    let targets: BTreeSet<String> = stage_ids.iter().map(|stage| (*stage).to_string()).collect();
    let mut context = RunContext::open(root, jobs)?;
    run_targets(&graph, &bound, &mut context, &targets)
}

/// The exact producer-selected mappings fixture.
pub type MappingsFixture = StageFixture;

/// Load the authenticated `stage-mappings` action without producing it.
pub fn mappings_fixture(root: &Path, jobs: usize) -> Result<MappingsFixture, gmeow_errors::Diag> {
    stage_fixture(root, jobs, "stage-mappings")
}

/// Authenticate one produced stage fixture without hydrating its RDF carrier.
///
/// The producer selector supplies the exact action identity, then
/// [`PipelineCache::inspect_receipt`] hashes its complete product blob without restoring
/// the dataset, indexes, or typed handles. An individual test that needs the carrier
/// still calls [`stage_fixture`].
pub fn verify_stage_fixture(
    root: &Path,
    stage_id: &str,
) -> Result<StageReceipt, gmeow_errors::Diag> {
    verify_stage_fixtures(root, &[stage_id]).map(|mut receipts| receipts.remove(0))
}

/// Authenticate several selected actions with one manifest read and one cache handle.
pub fn verify_stage_fixtures(
    root: &Path,
    stage_ids: &[&str],
) -> Result<Vec<StageReceipt>, gmeow_errors::Diag> {
    let mut manifest = load_stage_fixture_manifest(root)?;
    let cache = PipelineCache::open_existing_default_read_only(root)?;
    let mut verified = Vec::with_capacity(stage_ids.len());
    for stage_id in stage_ids {
        let expected_receipt = manifest
            .stages
            .remove(*stage_id)
            .ok_or_else(|| fixture_miss(stage_id))?;
        let actual_receipt = cache
            .inspect_receipt(&expected_receipt.context)?
            .ok_or_else(|| fixture_miss(stage_id))?;
        if actual_receipt != expected_receipt {
            return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
                expected: expected_receipt.digest(),
                actual: actual_receipt.digest(),
            }));
        }
        verified.push(actual_receipt);
    }
    Ok(verified)
}

/// Load one production stage's committed artifact lane without producing it.
///
/// A warm action authenticates the same receipt/product blob as the scheduler but does
/// not restore the packed RDF dataset. A miss is terminal: tests never recompute.
pub fn stage_artifacts(
    root: &Path,
    _jobs: usize,
    stage_id: &str,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let expected_receipt = selected_stage_receipt(root, stage_id)?;
    // The producer fingerprint is already a typed action-key field. Opening a
    // fingerprint-suffixed directory here would create a reader-only namespace,
    // miss every receipt written by the scheduler/primer, and force full product
    // hydration on every artifact-only fixture request.
    let cache = PipelineCache::open_existing_default_read_only(root)?;
    let hit = cache
        .get_artifacts(&expected_receipt.context)?
        .ok_or_else(|| fixture_miss(stage_id))?;
    if hit.receipt != expected_receipt {
        return Err(gmeow_errors::Diag::of_kind(crate::error::CacheMismatch {
            expected: expected_receipt.digest(),
            actual: hit.receipt.digest(),
        }));
    }
    Ok(hit.artifacts)
}

/// Load one exact artifact from an authenticated production-stage product.
///
/// This is the concise consumer seam for output-focused tests. It inherits the
/// manifest-selected, fail-closed behavior of [`stage_artifacts`].
pub fn authenticated_artifact(
    root: &Path,
    stage_id: &str,
    artifact_path: &str,
) -> Result<Vec<u8>, gmeow_errors::Diag> {
    stage_artifacts(root, 1, stage_id)?
        .remove(artifact_path)
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: stage_id.to_string(),
                message: format!("authenticated stage product carries no artifact {artifact_path}"),
            })
        })
}

/// Load and return the mappings stage's authenticated artifact lane.
pub fn mapping_artifacts(
    root: &Path,
    jobs: usize,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    stage_artifacts(root, jobs, "stage-mappings")
}
