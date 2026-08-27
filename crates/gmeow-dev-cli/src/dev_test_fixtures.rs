// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Strict producer/consumer boundary for corpus-backed test fixtures.
//!
//! This lives in the already-built `gmeow-dev` maintenance binary. Keeping it out of
//! standalone examples avoids a second Cargo feature/build lineage after nextest has
//! compiled the workspace.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use gmeow_docs::i18n::ENGLISH;
use gmeow_errors::Diag;
use purrdf::ContentDigest;

use crate::{TestFixtureMode, TestFixtureScope};

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
    let jobs = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1);
    let mut stage_observations = Vec::new();
    let mut stage_manifest_observation = None;

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
        let warm = gmeow_pipeline::fixture::reuse_stage_fixture_manifest(root)
            .map_err(|error| fail(format!("admit warm authenticated stage DAG: {error}")))?;
        let (manifest, receipts, timings) = if let Some(warm) = warm {
            println!(
                "test fixture producer: phase=pipeline-stages mode=receipt-hit state=admitted"
            );
            (warm.manifest, warm.receipts, None)
        } else {
            let run = gmeow_pipeline::fixture::prime_stage_fixtures(
                root,
                jobs,
                gmeow_pipeline::fixture::AUTHENTICATED_TEST_STAGE_IDS,
            )
            .map_err(|error| fail(format!("produce authenticated stage DAG: {error}")))?;
            let manifest =
                gmeow_pipeline::fixture::publish_stage_fixture_manifest(root, &run.stage_receipts)
                    .map_err(|error| {
                        fail(format!("publish authenticated stage selector: {error}"))
                    })?;
            (manifest, run.stage_receipts, Some(run.stage_timings))
        };
        println!(
            "pipeline fixture selector interim: path={} sha256={} stages={}",
            manifest.path.display(),
            manifest.sha256,
            manifest.stage_count
        );
        stage_manifest_observation = Some(serde_json::json!({
            "path": manifest.path.strip_prefix(root).unwrap_or(&manifest.path),
            "sha256": manifest.sha256,
            "stage_count": manifest.stage_count,
        }));
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
        println!("test fixture producer: phase=pipeline-stages state=complete");
        Some(serde_json::json!({
            "fixture": "pipeline-stage-phase",
            "elapsed_ms": stage_phase_started.elapsed().as_millis(),
            "stage_count": stage_observations.len(),
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
        let variants = gmeow_validate::data_validate::shape_corpus_variants_from_gts(&bundle)
            .map_err(|error| fail(format!("extract selected-bundle shape variants: {error}")))?;
        let view = gmeow_pipeline::bundle_blobs::Bundle::from_snapshot(&bundle)
            .map_err(|error| fail(format!("open selected bundle blob view: {error}")))?;
        let required_blob = |rep: &str| -> FixtureResult<Vec<u8>> {
            view.blob_by_rep(rep)
                .map_err(|error| fail(format!("decode selected-bundle {rep}: {error}")))?
                .ok_or_else(|| fail(format!("selected bundle omitted required {rep} blob")))
        };
        let required_member = |rep: &str, member: &str| -> FixtureResult<Vec<u8>> {
            view.archive(rep)
                .map_err(|error| fail(format!("decode selected-bundle {rep}: {error}")))?
                .remove(member)
                .ok_or_else(|| {
                    fail(format!(
                        "selected-bundle {rep} omitted required member {member}"
                    ))
                })
        };
        let artifacts = vec![
            (
                "validate-conformance-shapes.ttl",
                variants.conformance.into_bytes(),
            ),
            (
                "validate-domain-conformance-shapes.ttl",
                variants.domain_conformance.into_bytes(),
            ),
            (
                "validate-production-shapes.ttl",
                variants.production.into_bytes(),
            ),
            (
                "validate-queries.ustar",
                required_blob(gmeow_pipeline::bundle_blobs::REP_QUERIES)?,
            ),
            (
                "validate-mappings.ustar",
                required_blob(gmeow_pipeline::bundle_blobs::REP_MAPPINGS)?,
            ),
            (
                "validate-constraint-shapes.ttl",
                required_member(
                    gmeow_pipeline::bundle_blobs::REP_SHAPES,
                    "generated/shapes/constraint-shapes.ttl",
                )?,
            ),
            (
                "validate-linkml.yaml",
                required_member(
                    "generated-opaque-archive",
                    "generated/schemas/gmeow.linkml.yaml",
                )?,
            ),
            (
                "validate-statements-owl.ttl",
                required_member(
                    "statements-archive",
                    "generated/statements/gmeow-statements.owl.ttl",
                )?,
            ),
        ];
        let mut artifact_observations = Vec::with_capacity(artifacts.len());
        let mut artifact_publications = BTreeMap::new();
        for (name, bytes) in artifacts {
            let publication = gmeow_bundle_import::publish_authenticated_corpus_artifact(
                root, &bundle, name, &bytes,
            )
            .map_err(|error| fail(format!("publish authenticated {name}: {error}")))?;
            println!(
                "corpus artifact fixture: name={name} action={} receipt={} bytes={}",
                publication.action_key,
                publication.receipt_digest,
                bytes.len(),
            );
            artifact_observations.push(serde_json::json!({
                "name": name,
                "receipt": publication.receipt_digest,
                "action": publication.action_key,
                "bytes": bytes.len(),
            }));
            if artifact_publications
                .insert(name.to_string(), publication)
                .is_some()
            {
                return Err(fail(format!(
                    "duplicate authenticated corpus artifact name {name}"
                )));
            }
        }
        let import = gmeow_bundle_import::admit_graph_preserving_cached(cache_root, &bundle)
            .map_err(|error| fail(format!("admit exact bundle import: {error}")))?;
        println!(
            "bundle import fixture: mode={} action={} source={} receipt={} bytes={}",
            if import.built { "built" } else { "hydrated" },
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
    let (bundle_observation, finalized_selector) =
        if let Some((observation, selector)) = bundle_phase {
            let finalized = publish_bundle_fixture_selector(root, &selector)?;
            println!(
                "test fixture selector finalized: path={} sha256={}",
                finalized.path.display(),
                finalized.sha256,
            );
            (Some(observation), Some(finalized))
        } else {
            (None, None)
        };

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
                "stage_fixture_manifest": finalized_selector
                    .as_ref()
                    .map(|identity| serde_json::json!({
                        "path": identity.path.strip_prefix(root).unwrap_or(&identity.path),
                        "sha256": identity.sha256,
                    }))
                    .or(stage_manifest_observation),
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
    Ok(())
}

fn verify(
    root: &Path,
    scope: TestFixtureScope,
    bundle_cache_root: Option<&Path>,
    expected_source_digest: Option<&str>,
) -> FixtureResult<()> {
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
        for name in [
            "validate-conformance-shapes.ttl",
            "validate-domain-conformance-shapes.ttl",
            "validate-production-shapes.ttl",
            "validate-queries.ustar",
            "validate-mappings.ustar",
            "validate-constraint-shapes.ttl",
            "validate-linkml.yaml",
            "validate-statements-owl.ttl",
        ] {
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

fn publish_bundle_fixture_selector(
    root: &Path,
    bundle: &gmeow_bundle_import::BundleFixtureSelector,
) -> FixtureResult<FinalizedFixtureSelector> {
    let path = root.join(gmeow_pipeline::fixture::STAGE_FIXTURE_MANIFEST_RELATIVE_PATH);
    let bytes = std::fs::read(&path).map_err(|error| {
        fail(format!(
            "read pipeline fixture selector {} before bundle binding: {error}",
            path.display()
        ))
    })?;
    let mut value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| fail(format!("decode pipeline fixture selector: {error}")))?;
    if value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        != Some(2)
    {
        return Err(fail(
            "pipeline fixture selector is not schema 2 before bundle binding",
        ));
    }
    let object = value
        .as_object_mut()
        .ok_or_else(|| fail("pipeline fixture selector root is not an object"))?;
    object.insert(
        "bundle_import".to_string(),
        serde_json::to_value(bundle)
            .map_err(|error| fail(format!("encode bundle fixture selector: {error}")))?,
    );
    write_json_atomic(&path, &value).map_err(|error| {
        fail(format!(
            "publish bundle-bound fixture selector {}: {error}",
            path.display()
        ))
    })?;
    let finalized = std::fs::read(&path).map_err(|error| {
        fail(format!(
            "read finalized fixture selector {}: {error}",
            path.display()
        ))
    })?;
    Ok(FinalizedFixtureSelector {
        path,
        sha256: ContentDigest::of(&finalized).to_hex(),
    })
}

fn write_json_atomic(path: &Path, value: &serde_json::Value) -> std::io::Result<()> {
    use std::io::Write as _;

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut temporary, value).map_err(std::io::Error::other)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .map(|_| ())
}
