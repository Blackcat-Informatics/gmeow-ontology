// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Prime exact expensive-stage fixtures before nextest isolates tests into processes.

use std::path::PathBuf;
use std::time::Instant;

use purrdf::ContentDigest;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Scope {
    All,
    ProducerIndependent,
    ProducerBound,
}

impl Scope {
    fn parse(value: &str) -> Self {
        match value {
            "all" => Self::All,
            "producer-independent" => Self::ProducerIndependent,
            "producer-bound" => Self::ProducerBound,
            _ => panic!(
                "--scope must be all, producer-independent, or producer-bound; got {value:?}"
            ),
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::ProducerIndependent => "producer-independent",
            Self::ProducerBound => "producer-bound",
        }
    }

    const fn includes_stages(self) -> bool {
        matches!(self, Self::All | Self::ProducerIndependent)
    }

    const fn includes_bundle(self) -> bool {
        matches!(self, Self::All | Self::ProducerBound)
    }
}

fn main() {
    let args = parse_args();
    let scope = args.scope;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("crate is at <repo>/crates/pipeline")
        .to_path_buf();
    let jobs = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1);
    let total_started = Instant::now();
    let mut observations = Vec::new();
    if scope.includes_stages() {
        for stage_id in ["stage-compile-logic", "stage-mappings", "stage-slice-brief"] {
            let started = Instant::now();
            let fixture = gmeow_pipeline::fixture::stage_fixture(&root, jobs, stage_id)
                .unwrap_or_else(|error| panic!("prime exact {stage_id} fixture: {error}"));
            let mode = if fixture.outcome.built {
                "built"
            } else {
                "hydrated"
            };
            println!(
                "pipeline fixture: stage={} mode={mode} action={} receipt={} bytes={}",
                fixture.outcome.product.stage_id,
                fixture.outcome.receipt.action_key,
                fixture.outcome.receipt.digest(),
                fixture.outcome.transferred_bytes,
            );
            observations.push(serde_json::json!({
                "stage": stage_id,
                "built": fixture.outcome.built,
                "elapsed_ms": started.elapsed().as_millis(),
                "transferred_bytes": fixture.outcome.transferred_bytes,
                "receipt": fixture.outcome.receipt,
            }));
        }
    }
    let bundle_observation = scope.includes_bundle().then(|| {
        let bundle_started = Instant::now();
        let bundle_path = root.join("generated/dist/gmeow.gts");
        let bundle_bytes = std::fs::read(&bundle_path).unwrap_or_else(|error| {
            panic!("read producer bundle {}: {error}", bundle_path.display())
        });
        let actual_source_digest = ContentDigest::of(&bundle_bytes).to_hex();
        let expected_source_digest = args
            .expected_source_digest
            .as_deref()
            .expect("a producer-bound primer requires --expected-source-digest");
        assert_eq!(
            actual_source_digest, expected_source_digest,
            "producer-bound fixture source digest does not match the selected generated bundle"
        );
        let cache_root = args
            .bundle_import_cache_root
            .as_deref()
            .expect("a producer-bound primer requires --bundle-import-cache-root");
        let bundle_import =
            gmeow_bundle_import::import_graph_preserving_cached(cache_root, &bundle_bytes)
                .unwrap_or_else(|error| panic!("prime exact shipped-bundle import: {error}"));
        let bundle_mode = if bundle_import.built {
            "built"
        } else {
            "hydrated"
        };
        println!(
            "bundle import fixture: mode={bundle_mode} action={} source={} receipt={} bytes={}",
            bundle_import.receipt.action_key,
            bundle_import.receipt.source_digest,
            bundle_import.receipt.receipt_digest(),
            bundle_import.transferred_bytes,
        );
        let observation = serde_json::json!({
            "fixture": "bundle-import",
            "built": bundle_import.built,
            "elapsed_ms": bundle_started.elapsed().as_millis(),
            "transferred_bytes": bundle_import.transferred_bytes,
            "receipt": bundle_import.receipt,
        });
        drop(bundle_import.dataset);
        observation
    });
    if let Some(path) = args.timings_path {
        let payload = serde_json::json!({
            "schema_version": 1,
            "command": "prime-pipeline-test-fixtures",
            "scope": scope.name(),
            "jobs": jobs,
            "deterministic_work": {
                "fixture_count": observations.len() + if bundle_observation.is_some() { 1 } else { 0 },
                "stage_receipts": observations.iter().map(|entry| &entry["receipt"]).collect::<Vec<_>>(),
                "bundle_import_receipt": bundle_observation.as_ref().map(|entry| &entry["receipt"]),
            },
            "observations": {
                "total_elapsed_ms": total_started.elapsed().as_millis(),
                "fixtures": observations,
                "bundle_import": bundle_observation,
            },
        });
        write_json_atomic(&path, &payload)
            .unwrap_or_else(|error| panic!("write fixture telemetry {}: {error}", path.display()));
    }
}

struct Args {
    scope: Scope,
    timings_path: Option<PathBuf>,
    bundle_import_cache_root: Option<PathBuf>,
    expected_source_digest: Option<String>,
}

fn parse_args() -> Args {
    let mut scope = Scope::All;
    let mut scope_seen = false;
    let mut timings_path = None;
    let mut bundle_import_cache_root = None;
    let mut expected_source_digest = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--scope" => {
                assert!(!scope_seen, "--scope may be supplied only once");
                scope = Scope::parse(&args.next().expect("--scope requires a value"));
                scope_seen = true;
            }
            "--timings-json" => {
                assert!(
                    timings_path.is_none(),
                    "--timings-json may be supplied only once"
                );
                timings_path = Some(PathBuf::from(
                    args.next().expect("--timings-json requires a path"),
                ));
            }
            "--bundle-import-cache-root" => {
                assert!(
                    bundle_import_cache_root.is_none(),
                    "--bundle-import-cache-root may be supplied only once"
                );
                bundle_import_cache_root = Some(PathBuf::from(
                    args.next()
                        .expect("--bundle-import-cache-root requires a path"),
                ));
            }
            "--expected-source-digest" => {
                assert!(
                    expected_source_digest.is_none(),
                    "--expected-source-digest may be supplied only once"
                );
                expected_source_digest = Some(
                    args.next()
                        .expect("--expected-source-digest requires a digest"),
                );
            }
            _ => panic!("unexpected fixture-primer argument {argument:?}"),
        }
    }
    if scope.includes_bundle() {
        assert!(
            bundle_import_cache_root.is_some(),
            "--bundle-import-cache-root is required for scope {}",
            scope.name()
        );
        assert!(
            expected_source_digest.is_some(),
            "--expected-source-digest is required for scope {}",
            scope.name()
        );
    }
    Args {
        scope,
        timings_path,
        bundle_import_cache_root,
        expected_source_digest,
    }
}

fn write_json_atomic(path: &std::path::Path, value: &serde_json::Value) -> std::io::Result<()> {
    use std::io::Write as _;

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
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
