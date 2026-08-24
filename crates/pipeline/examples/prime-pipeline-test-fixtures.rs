// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Prime exact expensive-stage fixtures before nextest isolates tests into processes.

use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let timings_path = parse_timings_path();
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
    if let Some(path) = timings_path {
        let payload = serde_json::json!({
            "schema_version": 1,
            "command": "prime-pipeline-test-fixtures",
            "jobs": jobs,
            "deterministic_work": {
                "fixture_count": observations.len(),
                "receipts": observations.iter().map(|entry| &entry["receipt"]).collect::<Vec<_>>(),
            },
            "observations": {
                "total_elapsed_ms": total_started.elapsed().as_millis(),
                "fixtures": observations,
            },
        });
        write_json_atomic(&path, &payload)
            .unwrap_or_else(|error| panic!("write fixture telemetry {}: {error}", path.display()));
    }
}

fn parse_timings_path() -> Option<PathBuf> {
    let mut args = std::env::args().skip(1);
    let argument = args.next()?;
    assert_eq!(
        argument, "--timings-json",
        "only --timings-json is accepted"
    );
    let path = PathBuf::from(args.next().expect("--timings-json requires a path"));
    assert!(args.next().is_none(), "unexpected fixture-primer arguments");
    Some(path)
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
