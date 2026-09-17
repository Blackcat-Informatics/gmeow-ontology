// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Strict producer/consumer boundary for corpus-backed test fixtures.
//!
//! Production uses the authenticated O3/full-LTO `gmeow-dev` executable. Test
//! binaries independently consume its selected receipts, even across Cargo profiles.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use gmeow_docs::i18n::ENGLISH;
use gmeow_errors::Diag;
use purrdf::ContentDigest;

use crate::{TestFixtureMode, TestFixtureScope};

mod bundle_artifacts;

#[cfg(test)]
mod selection_tests;

type FixtureResult<T> = gmeow_errors::Result<T>;

fn fail(detail: impl std::fmt::Display) -> Diag {
    crate::error::sync(detail)
}

impl TestFixtureScope {
    const fn includes_stages(self) -> bool {
        matches!(self, Self::All | Self::ProducerIndependent)
    }

    const fn includes_docs(self) -> bool {
        matches!(self, Self::All | Self::ProducerBound)
    }

    const fn includes_bundle(self) -> bool {
        matches!(self, Self::All | Self::ProducerBound | Self::Bundle)
    }

    const fn includes_slice_specs(self) -> bool {
        matches!(self, Self::All | Self::ProducerBound)
    }

    const fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::ProducerIndependent => "producer-independent",
            Self::ProducerBound => "producer-bound",
            Self::Bundle => "bundle",
            Self::ConformanceHeavy => "conformance-heavy",
            Self::WasmCodebook => "wasm-codebook",
        }
    }
}

pub(crate) fn run(
    mode: TestFixtureMode,
    scope: TestFixtureScope,
    timings_path: Option<&Path>,
    bundle_cache_root: Option<&Path>,
    expected_source_digest: Option<&str>,
) -> i32 {
    let root = crate::dev_common::project_root();
    if scope == TestFixtureScope::WasmCodebook {
        let result = match mode {
            TestFixtureMode::Verify => verify_wasm_codebook(&root),
            TestFixtureMode::Produce => Err(fail(
                "wasm-codebook is a read-only selector; the explicit pipeline producer emits its artifact",
            )),
        };
        return result.map_or_else(
            |error| crate::dev_common::fail(format!("wasm codebook: {error}")),
            |()| 0,
        );
    }
    let result = match mode {
        TestFixtureMode::Produce => produce(
            &root,
            scope,
            timings_path,
            bundle_cache_root,
            expected_source_digest,
        ),
        TestFixtureMode::Verify => verify(&root, scope, bundle_cache_root, expected_source_digest),
    };
    match result {
        Ok(()) => 0,
        Err(error) => crate::dev_common::fail(format!("test fixtures: {error}")),
    }
}

/// Execute an isolated declarative-spec worker admitted by the explicit producer.
///
/// This command is intentionally absent from every Make/test surface. The parent
/// producer binds the child to the exact compiled implementation fingerprint through
/// `GMEOW_SLICE_SPEC_WORKER_AUTHORITY`; an unbound direct invocation fails closed.
pub(crate) fn run_slice_spec_worker(
    kind: &str,
    specs: &[std::path::PathBuf],
    workers: usize,
) -> i32 {
    let expected = gmeow_slicetest::BUILD_FINGERPRINT;
    if gmeow_pipeline::cache::PRODUCER_BUILD_CONTRACT.is_empty() {
        return crate::dev_common::fail("slice-spec-worker requires an optimized producer build");
    }
    let authority = std::env::var("GMEOW_SLICE_SPEC_WORKER_AUTHORITY").unwrap_or_default();
    if authority != expected {
        return crate::dev_common::fail(
            "slice-spec-worker lacks the exact parent producer authority",
        );
    }
    if workers == 0 {
        return crate::dev_common::fail("slice-spec-worker --workers must be positive");
    }
    let kind = match gmeow_slicetest::repository::SliceSpecKind::parse(kind) {
        Ok(kind) => kind,
        Err(error) => return crate::dev_common::fail(format!("slice-spec-worker: {error}")),
    };
    let root = crate::dev_common::project_root();
    match gmeow_slicetest::repository::execute_worker(&root, expected, kind, specs, workers) {
        Ok(()) => 0,
        Err(error) => crate::dev_common::fail(format!("slice-spec-worker: {error}")),
    }
}

fn require_bundle_args<'a>(
    scope: TestFixtureScope,
    bundle_cache_root: Option<&'a Path>,
    expected_source_digest: Option<&'a str>,
) -> FixtureResult<(&'a Path, &'a str)> {
    let cache_root = bundle_cache_root.ok_or_else(|| {
        fail(format!(
            "--bundle-import-cache-root is required for scope {}",
            scope.name()
        ))
    })?;
    let digest = expected_source_digest.ok_or_else(|| {
        fail(format!(
            "--expected-source-digest is required for scope {}",
            scope.name()
        ))
    })?;
    Ok((cache_root, digest))
}

fn verify_wasm_codebook(root: &Path) -> FixtureResult<()> {
    use std::io::Read as _;
    let path = gmeow_lang_bridge::gmn1_codec::native::GENERATED_PATH;
    gmeow_pipeline::fixture::verify_stage_fixtures(root, &["stage-mappings"])?;
    let expected =
        gmeow_action_cache::selection::source_artifacts::load(root, "stage-mappings", path)
            .map_err(fail)?;
    let mut actual = Vec::new();
    std::fs::File::open(root.join(path))
        .map_err(fail)?
        .take(expected.len() as u64 + 1)
        .read_to_end(&mut actual)
        .map_err(fail)?;
    if actual != expected {
        return Err(fail(
            "generated native codebook differs from the authenticated producer selection",
        ));
    }
    println!(
        "native wasm codebook verified: artifact={path} bytes={}",
        actual.len()
    );
    Ok(())
}

fn selected_bundle(root: &Path, expected: &str) -> FixtureResult<Vec<u8>> {
    let path = root.join("generated/dist/gmeow.gts");
    let bytes = std::fs::read(&path)
        .map_err(|error| fail(format!("read selected bundle {}: {error}", path.display())))?;
    let actual = ContentDigest::of(&bytes).to_hex();
    if actual != expected {
        return Err(fail(format!(
            "selected bundle identity mismatch: expected {expected}, actual {actual}"
        )));
    }
    Ok(bytes)
}

fn produce(
    root: &Path,
    scope: TestFixtureScope,
    timings_path: Option<&Path>,
    bundle_cache_root: Option<&Path>,
    expected_source_digest: Option<&str>,
) -> FixtureResult<()> {
    let started = Instant::now();
    if scope == TestFixtureScope::ConformanceHeavy {
        let selected = gmeow_pipeline::stages::conformance::heavy::produce(root)?;
        let prepared = prepare_fixture_selector_fields(
            root,
            None,
            BTreeMap::from([(
                "conformance_heavy",
                serde_json::to_value(&selected).map_err(fail)?,
            )]),
        )?;
        if let Some(path) = timings_path {
            write_json_atomic(
                path,
                &serde_json::json!({
                    "schema_version": 1,
                    "scope": scope.name(),
                    "elapsed_ms": started.elapsed().as_millis(),
                    "selected_action": selected,
                    "manifest_sha256": prepared.sha256,
                }),
            )
            .map_err(fail)?;
        }
        let finalized = prepared.publish()?;
        println!(
            "exhaustive conformance selector: path={} sha256={} action={}",
            finalized.path.display(),
            finalized.sha256,
            selected.context.key()
        );
        return Ok(());
    }
    let jobs = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1);
    let mut stage_observations = Vec::new();
    let mut stage_selection = None;
    let mut docs_selector = None;

    // Run the repository verdict in a fresh process footprint, before docs and hydrated
    // stage products have populated allocator arenas. The verdict's own scheduler is
    // memory-admitted; keeping it first also means an expensive miss cannot be OOM-killed
    // merely because independent producer phases retained otherwise reusable pages.
    let slice_spec_observation = if scope.includes_slice_specs() {
        println!("test fixture producer: phase=slice-specs state=started");
        let slice_started = Instant::now();
        let outcome = gmeow_slicetest::repository::produce_repository_verdict(
            root,
            gmeow_slicetest::BUILD_FINGERPRINT,
        )
        .map_err(|error| fail(format!("produce authenticated slice-spec verdict: {error}")))?;
        println!(
            "slice-spec fixture: mode={} action={} receipt={} inputs={} specs={} competency={} structural={} conformance={} flagships={}",
            if outcome.built {
                "built"
            } else {
                "receipt-hit"
            },
            outcome.action_key,
            outcome.receipt_digest,
            outcome.verdict.input_files,
            outcome.verdict.spec_files(),
            outcome.verdict.competency_files,
            outcome.verdict.structural_files,
            outcome.verdict.conformance_files,
            outcome.verdict.flagship_manifests,
        );
        println!("test fixture producer: phase=slice-specs state=complete");
        Some(serde_json::json!({
            "fixture": "slice-spec-verdict",
            "built": outcome.built,
            "elapsed_ms": slice_started.elapsed().as_millis(),
            "action_key": outcome.action_key,
            "receipt_digest": outcome.receipt_digest,
            "verdict": outcome.verdict,
        }))
    } else {
        None
    };

    let docs_observation = if scope.includes_docs() {
        println!("test fixture producer: phase=docs state=started");
        let docs_started = Instant::now();
        let prime = gmeow_docs::fixture::prime(root);
        println!(
            "docs fixture: actions={} built={} receipt-hits={} parallelism={}",
            prime.action_count, prime.built, prime.receipt_hits, prime.parallelism
        );
        println!("test fixture producer: phase=docs state=complete");
        docs_selector = Some(prime.selector);
        Some(serde_json::json!({
            "fixture": "docs",
            "elapsed_ms": docs_started.elapsed().as_millis(),
            "actions": prime.action_count,
            "built": prime.built,
            "receipt_hits": prime.receipt_hits,
            "parallelism": prime.parallelism,
        }))
    } else {
        None
    };

    let pipeline_stage_phase_observation = if scope.includes_stages() {
        let stage_phase_started = Instant::now();
        println!(
            "test fixture producer: phase=pipeline-stages state=started targets={} jobs={jobs}",
            gmeow_pipeline::fixture::AUTHENTICATED_TEST_STAGE_IDS.len()
        );
        let warm = gmeow_pipeline::fixture::reuse_stage_fixture_candidate(root)
            .map_err(|error| fail(format!("admit warm authenticated stage DAG: {error}")))?;
        let (receipts, timings) = if let Some(warm) = warm {
            println!(
                "test fixture producer: phase=pipeline-stages mode=receipt-hit state=admitted"
            );
            (warm.receipts, None)
        } else {
            let run = gmeow_pipeline::fixture::prime_stage_fixtures(
                root,
                jobs,
                gmeow_pipeline::fixture::AUTHENTICATED_TEST_STAGE_IDS,
            )
            .map_err(|error| fail(format!("produce authenticated stage DAG: {error}")))?;
            (run.stage_receipts, Some(run.stage_timings))
        };
        stage_selection = Some(
            gmeow_pipeline::fixture::prepare_stage_fixture_candidate(root, &receipts)
                .map_err(|error| fail(format!("prepare authenticated stage selection: {error}")))?,
        );
        for &stage_id in gmeow_pipeline::fixture::AUTHENTICATED_TEST_STAGE_IDS {
            let receipt = receipts
                .iter()
                .find(|receipt| receipt.context.stage_id == stage_id)
                .ok_or_else(|| fail(format!("fixture DAG emitted no receipt for {stage_id}")))?;
            let timing = timings
                .as_ref()
                .and_then(|timings| timings.iter().find(|timing| timing.stage_id == stage_id));
            let transferred_bytes = timing.map_or(0, |timing| {
                timing
                    .cache_read_bytes
                    .saturating_add(timing.cache_write_bytes)
            });
            let mode = timing.map_or("receipt-hit", |timing| {
                if timing.cached { "hydrated" } else { "built" }
            });
            println!(
                "pipeline fixture: stage={} mode={} action={} receipt={} bytes={}",
                stage_id,
                mode,
                receipt.action_key,
                receipt.digest(),
                transferred_bytes,
            );
            stage_observations.push(serde_json::json!({
                "stage": stage_id,
                "built": timing.is_some_and(|timing| !timing.cached),
                "elapsed_ms": timing.map_or(0, |timing| timing.elapsed_ms),
                "transferred_bytes": transferred_bytes,
                "cache_outcome": timing.map_or("receipt-hit", |timing| timing.cache_outcome.as_str()),
                "receipt": receipt,
            }));
        }
        let source_exports = gmeow_pipeline::fixture::export_source_artifacts(root, &receipts)
            .map_err(|error| fail(format!("export source-stage observations: {error}")))?;
        stage_selection
            .as_mut()
            .and_then(serde_json::Value::as_object_mut)
            .ok_or_else(|| fail("prepared stage selection is not an object"))?
            .insert(
                "source_artifacts".to_owned(),
                serde_json::to_value(&source_exports.source_artifacts).map_err(fail)?,
            );
        println!("test fixture producer: phase=pipeline-stages state=complete");
        Some(serde_json::json!({
            "fixture": "pipeline-stage-phase",
            "elapsed_ms": stage_phase_started.elapsed().as_millis(),
            "stage_count": stage_observations.len(),
            "source_artifact_count": source_exports.source_artifacts.values().map(BTreeMap::len).sum::<usize>(),
            "built": stage_observations.iter().filter(|entry| entry["built"] == true).count(),
        }))
    } else {
        None
    };

    let bundle_phase = if scope.includes_bundle() {
        println!("test fixture producer: phase=bundle-bound state=started");
        let bundle_started = Instant::now();
        let (cache_root, expected) =
            require_bundle_args(scope, bundle_cache_root, expected_source_digest)?;
        let bundle = selected_bundle(root, expected)?;
        let (mut import, cold_import) =
            gmeow_bundle_import::admit_graph_preserving_cached_with_blobs(
                cache_root,
                &bundle,
                bundle_artifacts::BLOB_SELECTORS,
                bundle_artifacts::blob_limits(),
            )
            .map_err(|error| fail(format!("admit exact bundle import: {error}")))?;
        let sources = bundle_artifacts::Sources::new(&bundle, cold_import);
        // Sources owns the same cold dataset allocation alongside its selected
        // blobs; the admission's extra Arc is no longer needed for reporting.
        drop(import.produced_dataset.take());
        let mut artifact_observations = Vec::with_capacity(bundle_artifacts::NAMES.len());
        let mut artifact_publications = BTreeMap::new();
        for &name in bundle_artifacts::NAMES {
            let started = Instant::now();
            let admission = gmeow_bundle_import::admit_authenticated_corpus_artifact(
                root,
                &import.receipt,
                gmeow_pipeline::cache::PRODUCER_BUILD_CONTRACT,
                name,
                || sources.produce(name),
            )
            .map_err(|error| fail(format!("admit authenticated {name}: {error}")))?;
            let publication = admission.publication;
            println!(
                "corpus artifact fixture: name={name} mode={} action={} receipt={} bytes={}",
                if admission.built {
                    "built"
                } else {
                    "authenticated"
                },
                publication.action_key,
                publication.receipt_digest,
                publication.product_bytes,
            );
            artifact_observations.push(serde_json::json!({
                "name": name, "built": admission.built, "elapsed_ms": started.elapsed().as_millis(),
                "receipt": publication.receipt_digest, "action": publication.action_key,
                "bytes": publication.product_bytes,
            }));
            if artifact_publications
                .insert(name.to_owned(), publication)
                .is_some()
            {
                return Err(fail(format!(
                    "duplicate authenticated corpus artifact name {name}"
                )));
            }
        }
        drop(sources);
        println!(
            "bundle import fixture: mode={} action={} source={} receipt={} bytes={}",
            if import.built {
                "built"
            } else {
                "authenticated"
            },
            import.receipt.action_key,
            import.receipt.source_digest,
            import.receipt.receipt_digest(),
            import.transferred_bytes,
        );
        let selector = gmeow_bundle_import::BundleFixtureSelector {
            schema_version: 1,
            receipt_digest: import.receipt.receipt_digest(),
            receipt: import.receipt.clone(),
            corpus_artifacts: artifact_publications,
        };
        let observation = serde_json::json!({
            "fixture": "bundle-import",
            "built": import.built,
            "elapsed_ms": bundle_started.elapsed().as_millis(),
            "transferred_bytes": import.transferred_bytes,
            "receipt": import.receipt,
            "corpus_artifacts": artifact_observations,
        });
        println!("test fixture producer: phase=bundle-bound state=complete");
        Some((observation, selector))
    } else {
        None
    };
    let (bundle_observation, bundle_selector) = match bundle_phase {
        Some((observation, selector)) => (Some(observation), Some(selector)),
        None => (None, None),
    };
    let prepared_selector = prepare_fixture_selector(
        root,
        stage_selection,
        bundle_selector.as_ref(),
        docs_selector.as_ref(),
    )?;

    if let Some(path) = timings_path {
        let value = serde_json::json!({
            "schema_version": 1,
            "command": "gmeow-dev test-fixtures produce",
            "scope": scope.name(),
            "jobs": jobs,
            "deterministic_work": {
                "fixture_count": stage_observations.len()
                    + usize::from(slice_spec_observation.is_some())
                    + usize::from(bundle_observation.is_some()),
                "stage_receipts": stage_observations.iter().map(|entry| &entry["receipt"]).collect::<Vec<_>>(),
                "stage_fixture_manifest": {
                    "path": prepared_selector.path.strip_prefix(root).unwrap_or(&prepared_selector.path),
                    "sha256": prepared_selector.sha256,
                },
                "slice_spec_receipt": slice_spec_observation.as_ref().map(|entry| &entry["receipt_digest"]),
                "bundle_import_receipt": bundle_observation.as_ref().map(|entry| &entry["receipt"]),
            },
            "observations": {
                "total_elapsed_ms": started.elapsed().as_millis(),
                "fixtures": stage_observations,
                "slice_specs": slice_spec_observation,
                "docs": docs_observation,
                "pipeline_stage_phase": pipeline_stage_phase_observation,
                "bundle_import": bundle_observation,
            },
        });
        write_json_atomic(path, &value).map_err(|error| {
            fail(format!(
                "write fixture telemetry {}: {error}",
                path.display()
            ))
        })?;
    }
    let finalized_selector = prepared_selector.publish()?;
    println!(
        "test fixture selector finalized: path={} sha256={}",
        finalized_selector.path.display(),
        finalized_selector.sha256,
    );
    Ok(())
}

fn verify(
    root: &Path,
    scope: TestFixtureScope,
    bundle_cache_root: Option<&Path>,
    expected_source_digest: Option<&str>,
) -> FixtureResult<()> {
    if scope == TestFixtureScope::ConformanceHeavy {
        let observations = gmeow_pipeline::stages::conformance::heavy::load(root)?;
        println!(
            "exhaustive conformance observations verified: consistency_cases={} class_diagnostic_cases={}",
            observations.consistency.len(),
            observations.class_diagnostics.len(),
        );
        return Ok(());
    }
    if scope.includes_docs() {
        println!("test fixture verifier: phase=docs state=started");
        let (model, identity) = gmeow_docs_model::fixture::load_with_identity(root);
        let mut languages = model.available_languages.clone();
        languages.push(ENGLISH.to_string());
        languages.sort();
        languages.dedup();
        for language in &languages {
            let site = gmeow_docs::fixture::load_site_lang(root, language);
            println!(
                "docs fixture verified: artifact=site language={language} files={}",
                site.files.len()
            );
        }
        let book = gmeow_docs::fixture::load_book(root);
        println!(
            "docs fixture verified: artifact=book files={} model-receipt={} model-product={}",
            book.files.len(),
            identity.receipt_digest,
            identity.product_digest
        );
        println!("test fixture verifier: phase=docs state=complete");
    }

    if scope.includes_stages() {
        println!(
            "test fixture verifier: phase=pipeline-stages state=started targets={}",
            gmeow_pipeline::fixture::AUTHENTICATED_TEST_STAGE_IDS.len()
        );
        let receipts = gmeow_pipeline::fixture::verify_stage_fixtures(
            root,
            gmeow_pipeline::fixture::AUTHENTICATED_TEST_STAGE_IDS,
        )
        .map_err(|error| fail(format!("authenticate pipeline stage fixtures: {error}")))?;
        for (&stage_id, receipt) in gmeow_pipeline::fixture::AUTHENTICATED_TEST_STAGE_IDS
            .iter()
            .zip(receipts)
        {
            println!(
                "pipeline fixture verified: stage={stage_id} action={} receipt={}",
                receipt.action_key,
                receipt.digest(),
            );
        }
        for (artifact, bytes) in gmeow_pipeline::fixture::verify_source_artifacts(root)? {
            println!("source artifact verified: artifact={artifact} bytes={bytes}");
        }
        println!("test fixture verifier: phase=pipeline-stages state=complete");
    }

    if scope.includes_slice_specs() {
        println!("test fixture verifier: phase=slice-specs state=started");
        let outcome =
            gmeow_slicetest::repository::verify_cached(root, gmeow_slicetest::BUILD_FINGERPRINT)
                .map_err(|error| {
                    fail(format!(
                        "authenticate slice-spec verdict read-only: {error}"
                    ))
                })?;
        if outcome.built {
            return Err(fail(
                "read-only verifier unexpectedly built a slice-spec verdict",
            ));
        }
        println!(
            "slice-spec fixture verified: action={} receipt={} inputs={} specs={}",
            outcome.action_key,
            outcome.receipt_digest,
            outcome.verdict.input_files,
            outcome.verdict.spec_files(),
        );
        println!("test fixture verifier: phase=slice-specs state=complete");
    }

    if scope.includes_bundle() {
        println!("test fixture verifier: phase=bundle-bound state=started");
        let (cache_root, expected) =
            require_bundle_args(scope, bundle_cache_root, expected_source_digest)?;
        let bundle = selected_bundle(root, expected)?;
        let import = gmeow_bundle_import::load_graph_preserving_cached(cache_root, &bundle)
            .map_err(|error| fail(format!("load exact bundle import read-only: {error}")))?;
        if import.built {
            return Err(fail(
                "read-only verifier unexpectedly built a bundle import",
            ));
        }
        let mut shape_bytes = Vec::new();
        for &name in bundle_artifacts::NAMES {
            let bytes = gmeow_bundle_import::load_authenticated_corpus_artifact(root, name)
                .map_err(|error| fail(format!("load authenticated {name}: {error}")))?;
            if bytes.is_empty() {
                return Err(fail(format!(
                    "authenticated corpus artifact {name} is empty"
                )));
            }
            shape_bytes.push(bytes.len());
        }
        println!(
            "pipeline fixture verified: fixture=bundle-import action={} receipt={} bytes={} shape-bytes={shape_bytes:?}",
            import.receipt.action_key,
            import.receipt.receipt_digest(),
            import.transferred_bytes,
        );
        drop(import.dataset);
        println!("test fixture verifier: phase=bundle-bound state=complete");
    }
    Ok(())
}

struct FinalizedFixtureSelector {
    path: std::path::PathBuf,
    sha256: String,
}

fn prepare_fixture_selector(
    root: &Path,
    stage_selection: Option<serde_json::Value>,
    bundle: Option<&gmeow_bundle_import::BundleFixtureSelector>,
    docs: Option<&gmeow_docs_model::fixture::DocsFixtureSelector>,
) -> FixtureResult<PreparedFixtureSelector> {
    let mut fields = BTreeMap::new();
    if let Some(bundle) = bundle {
        fields.insert(
            "bundle_import",
            serde_json::to_value(bundle)
                .map_err(|error| fail(format!("encode bundle fixture selector: {error}")))?,
        );
    }
    if let Some(docs) = docs {
        fields.insert(
            "docs",
            serde_json::to_value(docs)
                .map_err(|error| fail(format!("encode docs fixture selector: {error}")))?,
        );
    }
    prepare_fixture_selector_fields(root, stage_selection, fields)
}

/// Prepare selected extensions without changing the runner's current credentials.
/// Source exports are already part of a fresh stage selection; later selected
/// operations preserve those bindings until their own final publication succeeds.
fn prepare_fixture_selector_fields(
    root: &Path,
    stage_selection: Option<serde_json::Value>,
    fields: BTreeMap<&str, serde_json::Value>,
) -> FixtureResult<PreparedFixtureSelector> {
    let path = root.join(gmeow_pipeline::fixture::STAGE_FIXTURE_MANIFEST_RELATIVE_PATH);
    let mut value = match stage_selection {
        Some(value) => value,
        None => {
            let bytes = std::fs::read(&path).map_err(|error| {
                fail(format!(
                    "read published fixture prefix {}: {error}",
                    path.display()
                ))
            })?;
            serde_json::from_slice(&bytes)
                .map_err(|error| fail(format!("decode published fixture prefix: {error}")))?
        }
    };
    if value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        != Some(2)
    {
        return Err(fail(
            "pipeline fixture selection is not schema 2 before publication",
        ));
    }
    let object = value
        .as_object_mut()
        .ok_or_else(|| fail("pipeline fixture selector root is not an object"))?;
    for (name, value) in fields {
        object.insert(name.to_owned(), value);
    }
    let bytes = encode_json(&value)
        .map_err(|error| fail(format!("encode complete fixture selector: {error}")))?;
    let sha256 = ContentDigest::of(&bytes).to_hex();
    Ok(PreparedFixtureSelector {
        path,
        bytes,
        sha256,
    })
}

/// A selected operation publishes only after every fallible preparation,
/// including requested telemetry, has completed. The bytes are encoded once.
struct PreparedFixtureSelector {
    path: std::path::PathBuf,
    bytes: Vec<u8>,
    sha256: String,
}

impl PreparedFixtureSelector {
    fn publish(self) -> FixtureResult<FinalizedFixtureSelector> {
        write_bytes_atomic(&self.path, &self.bytes).map_err(|error| {
            fail(format!(
                "publish complete fixture selector {}: {error}",
                self.path.display()
            ))
        })?;
        Ok(FinalizedFixtureSelector {
            path: self.path,
            sha256: self.sha256,
        })
    }
}

fn encode_json(value: &serde_json::Value) -> std::io::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn write_json_atomic(path: &Path, value: &serde_json::Value) -> std::io::Result<()> {
    write_bytes_atomic(path, &encode_json(value)?)
}

fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .map(|_| ())
}
