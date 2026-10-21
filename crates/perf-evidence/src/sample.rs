// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Run one report-only paired performance sample with exact source/tool/host identity.
//!
//! This is deliberately not a correctness gate. It records deterministic identity and
//! child-process resource observations in separate JSON objects; the repeated paired
//! protocol and median comparison live in docs/rust-test-performance.md.

use crate::{PerfResult, write_json_atomic};
use purrdf_lex::json::{self, Object, Value, record::ToJson as _};

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

const SCHEMA_VERSION: u32 = 2;

#[derive(Debug)]
struct Args {
    pair_id: String,
    variant: String,
    sample_index: u32,
    cache_state: String,
    cache_classes: BTreeMap<String, String>,
    partial_change: Option<String>,
    node_class: String,
    runner_image: Option<String>,
    queued_unix_ms: Option<u128>,
    output: PathBuf,
    work_telemetry: Option<PathBuf>,
    identity_receipts: Vec<(String, PathBuf)>,
    cache_roots: Vec<(String, PathBuf)>,
    output_roots: Vec<(String, PathBuf)>,
    command: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
struct Usage {
    user_us: u128,
    system_us: u128,
    max_rss_kib: u64,
    minor_faults: u64,
    major_faults: u64,
    input_blocks: u64,
    output_blocks: u64,
    voluntary_context_switches: u64,
    involuntary_context_switches: u64,
}

impl Usage {
    fn delta(self, before: Self) -> Value {
        Value::from(
            Object::new()
                .with(
                    "user_cpu_ms",
                    self.user_us.saturating_sub(before.user_us) / 1_000,
                )
                .with(
                    "system_cpu_ms",
                    self.system_us.saturating_sub(before.system_us) / 1_000,
                )
                .with("max_rss_kib", self.max_rss_kib.to_json())
                .with(
                    "pre_sample_children_max_rss_kib",
                    before.max_rss_kib.to_json(),
                )
                .with(
                    "minor_faults",
                    (self.minor_faults.saturating_sub(before.minor_faults)).to_json(),
                )
                .with(
                    "major_faults",
                    (self.major_faults.saturating_sub(before.major_faults)).to_json(),
                )
                .with(
                    "filesystem_input_blocks",
                    (self.input_blocks.saturating_sub(before.input_blocks)).to_json(),
                )
                .with(
                    "filesystem_output_blocks",
                    (self.output_blocks.saturating_sub(before.output_blocks)).to_json(),
                )
                .with(
                    "voluntary_context_switches",
                    (self
                        .voluntary_context_switches
                        .saturating_sub(before.voluntary_context_switches))
                    .to_json(),
                )
                .with(
                    "involuntary_context_switches",
                    (self
                        .involuntary_context_switches
                        .saturating_sub(before.involuntary_context_switches))
                    .to_json(),
                )
                .with(
                    "scope",
                    "waited command process tree via getrusage(RUSAGE_CHILDREN)".to_json(),
                ),
        )
    }
}

/// Execute the report-only sampler CLI, preserving the sampled command's exit code.
pub fn run_cli() {
    match parse_args().and_then(run) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("perf-sample: {error}");
            std::process::exit(2);
        }
    }
}

fn parse_args() -> PerfResult<Args> {
    parse_args_from(std::env::args_os().skip(1))
}

fn parse_args_from(arguments: impl IntoIterator<Item = OsString>) -> PerfResult<Args> {
    let mut pair_id = None;
    let mut variant = None;
    let mut sample_index = None;
    let mut cache_state = None;
    let mut cache_classes = BTreeMap::new();
    let mut partial_change = None;
    let mut node_class = None;
    let mut runner_image = None;
    let mut queued_unix_ms = None;
    let mut output = None;
    let mut work_telemetry = None;
    let mut identity_receipts = Vec::new();
    let mut cache_roots = Vec::new();
    let mut output_roots = Vec::new();
    let mut command = Vec::new();
    let mut args = arguments
        .into_iter()
        .map(|argument| {
            argument
                .into_string()
                .map_err(|_| "arguments must be UTF-8".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter();
    while let Some(argument) = args.next() {
        if argument == "--" {
            command.extend(args);
            break;
        }
        let value = |args: &mut std::vec::IntoIter<String>, name: &str| {
            args.next()
                .ok_or_else(|| format!("{name} requires a value"))
        };
        match argument.as_str() {
            "--pair-id" => pair_id = Some(value(&mut args, "--pair-id")?),
            "--variant" => variant = Some(value(&mut args, "--variant")?),
            "--sample-index" => {
                let raw = value(&mut args, "--sample-index")?;
                sample_index = Some(
                    raw.parse::<u32>()
                        .map_err(|error| format!("invalid --sample-index {raw:?}: {error}"))?,
                );
            }
            "--cache-state" => cache_state = Some(value(&mut args, "--cache-state")?),
            "--cargo-cache-state" => {
                cache_classes.insert(
                    "cargo".to_string(),
                    value(&mut args, "--cargo-cache-state")?,
                );
            }
            "--sync-cache-state" => {
                cache_classes.insert(
                    "sync_manifest".to_string(),
                    value(&mut args, "--sync-cache-state")?,
                );
            }
            "--pipeline-cache-state" => {
                cache_classes.insert(
                    "pipeline".to_string(),
                    value(&mut args, "--pipeline-cache-state")?,
                );
            }
            "--fixture-cache-state" => {
                cache_classes.insert(
                    "fixture".to_string(),
                    value(&mut args, "--fixture-cache-state")?,
                );
            }
            "--bundle-import-cache-state" => {
                cache_classes.insert(
                    "bundle_import".to_string(),
                    value(&mut args, "--bundle-import-cache-state")?,
                );
            }
            "--nextest-archive-cache-state" => {
                cache_classes.insert(
                    "nextest_archive".to_string(),
                    value(&mut args, "--nextest-archive-cache-state")?,
                );
            }
            "--partial-change" => {
                partial_change = Some(value(&mut args, "--partial-change")?);
            }
            "--node-class" => node_class = Some(value(&mut args, "--node-class")?),
            "--runner-image" => runner_image = Some(value(&mut args, "--runner-image")?),
            "--queued-unix-ms" => {
                let raw = value(&mut args, "--queued-unix-ms")?;
                queued_unix_ms = Some(
                    raw.parse::<u128>()
                        .map_err(|error| format!("invalid --queued-unix-ms {raw:?}: {error}"))?,
                );
            }
            "--output" => output = Some(PathBuf::from(value(&mut args, "--output")?)),
            "--work-telemetry" => {
                work_telemetry = Some(PathBuf::from(value(&mut args, "--work-telemetry")?));
            }
            "--identity-receipt" => identity_receipts.push(parse_named_path(
                &value(&mut args, "--identity-receipt")?,
                "--identity-receipt",
            )?),
            "--cache-root" => cache_roots.push(parse_named_path(
                &value(&mut args, "--cache-root")?,
                "--cache-root",
            )?),
            "--output-root" => output_roots.push(parse_named_path(
                &value(&mut args, "--output-root")?,
                "--output-root",
            )?),
            _ => {
                return Err(
                    format!("unknown argument {argument:?}; separate the command with --").into(),
                );
            }
        }
    }

    let pair_id = required(pair_id, "--pair-id")?;
    let variant = required(variant, "--variant")?;
    if !matches!(variant.as_str(), "baseline" | "candidate") {
        return Err("--variant must be baseline or candidate".into());
    }
    let sample_index = required(sample_index, "--sample-index")?;
    if sample_index == 0 {
        return Err("--sample-index is one-based".into());
    }
    let cache_state = required(cache_state, "--cache-state")?;
    if !matches!(cache_state.as_str(), "cold" | "warm" | "partial") {
        return Err("--cache-state must be cold, warm, or partial".into());
    }
    for (required_class, option) in [
        ("cargo", "--cargo-cache-state"),
        ("sync_manifest", "--sync-cache-state"),
        ("pipeline", "--pipeline-cache-state"),
        ("fixture", "--fixture-cache-state"),
        ("bundle_import", "--bundle-import-cache-state"),
        ("nextest_archive", "--nextest-archive-cache-state"),
    ] {
        if !cache_classes.contains_key(required_class) {
            return Err(format!("missing required {option}").into());
        }
    }
    for (class, state) in &cache_classes {
        if !matches!(
            state.as_str(),
            "cold" | "warm" | "partial" | "absent" | "not-applicable"
        ) {
            return Err(format!(
                "cache class {class} has invalid state {state:?}; expected cold, warm, partial, absent, or not-applicable"
            )
            .into());
        }
    }
    match (cache_state.as_str(), partial_change.as_ref()) {
        ("partial", None) => {
            return Err("--cache-state partial requires --partial-change".into());
        }
        ("partial", Some(change)) if !valid_partial_change(change) => {
            return Err(
                "--partial-change must be KIND:PATH:SHA256 with a 64-digit lowercase digest".into(),
            );
        }
        ("cold" | "warm", Some(_)) => {
            return Err("--partial-change is valid only for a partial sample".into());
        }
        _ => {}
    }
    let node_class = required(node_class, "--node-class")?;
    let output = required(output, "--output")?;
    if command.is_empty() {
        return Err("a command is required after --".into());
    }
    Ok(Args {
        pair_id,
        variant,
        sample_index,
        cache_state,
        cache_classes,
        partial_change,
        node_class,
        runner_image,
        queued_unix_ms,
        output,
        work_telemetry,
        identity_receipts,
        cache_roots,
        output_roots,
        command,
    })
}

fn parse_named_path(raw: &str, option: &str) -> PerfResult<(String, PathBuf)> {
    let (name, path) = raw
        .split_once('=')
        .ok_or_else(|| format!("{option} must be NAME=PATH"))?;
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        || path.is_empty()
    {
        return Err(format!("{option} must be NAME=PATH with a portable non-empty name").into());
    }
    Ok((name.to_string(), PathBuf::from(path)))
}

fn valid_partial_change(value: &str) -> bool {
    let mut parts = value.rsplitn(2, ':');
    let digest = parts.next().unwrap_or_default();
    let prefix = parts.next().unwrap_or_default();
    let Some((kind, path)) = prefix.split_once(':') else {
        return false;
    };
    !kind.is_empty()
        && !path.is_empty()
        && digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn required<T>(value: Option<T>, name: &str) -> PerfResult<T> {
    Ok(value.ok_or_else(|| format!("missing required {name}"))?)
}

fn run(args: Args) -> PerfResult<i32> {
    let root =
        PathBuf::from(run_text(Path::new("."), "git", &["rev-parse", "--show-toplevel"])?.trim());
    let status = run_text(
        &root,
        "git",
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )?;
    if !status.trim().is_empty() {
        return Err(format!(
            "measurement requires a clean worktree so its source identity is exact:\n{status}"
        )
        .into());
    }

    let git_head = run_text(&root, "git", &["rev-parse", "HEAD"])?;
    let git_tree = run_text(&root, "git", &["rev-parse", "HEAD^{tree}"])?;
    let cargo_lock_sha256 = sha256_file(&root.join("Cargo.lock"))?;
    let dependency_resolution_sha256 = dependency_resolution_digest(&root)?;
    let pipeline_build_fingerprint = pipeline_build_fingerprint(&root, &args.identity_receipts)?;
    let rustc = run_text(&root, "rustc", &["-Vv"])?;
    let cargo = run_text(&root, "cargo", &["--version", "--verbose"])?;
    let nextest = run_text(&root, "cargo", &["nextest", "--version"])?;
    let host = host_identity();
    let runner_image = args.runner_image.or_else(|| {
        let os = std::env::var("ImageOS").ok()?;
        let version = std::env::var("ImageVersion").ok();
        Some(version.map_or(os.clone(), |version| format!("{os}@{version}")))
    });
    // GENERATED-READ-OK: this report-only performance audit hashes the materialized
    // generated product for sample identity; it never contributes to the carrier.
    let generated_tree = root.join("generated");
    let generated_identity = generated_tree
        .is_dir()
        .then(|| tree_census(&generated_tree))
        .transpose()?;
    let identity_receipts = named_file_receipts(&root, &args.identity_receipts)?;
    let cache_census = named_tree_receipts(&root, &args.cache_roots)?;
    let load_before = read_trimmed("/proc/loadavg");
    let usage_before = child_usage()?;
    let started_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock predates UNIX epoch: {error}"))?
        .as_millis();
    let started = Instant::now();
    let exit_status = Command::new(&args.command[0])
        .args(&args.command[1..])
        .current_dir(&root)
        .status()
        .map_err(|error| format!("run measured command {:?}: {error}", args.command))?;
    let wall_ms = started.elapsed().as_millis();
    let usage_after = child_usage()?;
    let load_after = read_trimmed("/proc/loadavg");

    let work_telemetry = args
        .work_telemetry
        .as_ref()
        .map(|path| {
            let path = if path.is_absolute() {
                path.clone()
            } else {
                root.join(path)
            };
            let bytes = fs::read(&path)
                .map_err(|error| format!("read work telemetry {}: {error}", path.display()))?;
            let value = json::record::read_document(&bytes)
                .map_err(|error| format!("parse work telemetry {}: {error}", path.display()))?;
            Ok::<_, String>(Value::from(
                Object::new()
                    .with(
                        "path",
                        (path
                            .strip_prefix(&root)
                            .unwrap_or(&path)
                            .display()
                            .to_string())
                        .to_json(),
                    )
                    .with("sha256", (sha256_bytes(&bytes)).to_json())
                    .with("payload", value.to_json()),
            ))
        })
        .transpose()?;
    // Cache roots above bind pre-command warmth. Explicit output roots bind bytes
    // created by the measured operation (notably nextest archive compression) without
    // re-hashing every large input cache a second time. The sample file itself is
    // written only after this census, avoiding a self-referential tree digest when its
    // parent directory is measured.
    let output_census = named_tree_receipts(&root, &args.output_roots)?;
    let proof_inventory = proof_inventory(work_telemetry.as_ref(), &args.command);
    let causal_counters = causal_counters(work_telemetry.as_ref());

    let resource_usage = usage_after.delta(usage_before);
    let cpu_ms = resource_usage
        .get("user_cpu_ms")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .saturating_add(
            resource_usage
                .get("system_cpu_ms")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        );
    let logical_cpus = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1);
    #[allow(clippy::cast_precision_loss)]
    let aggregate_cpu_utilization_pct = if wall_ms == 0 {
        0.0
    } else {
        cpu_ms as f64 * 100.0 / wall_ms as f64
    };
    #[allow(clippy::cast_precision_loss)]
    let host_normalized_cpu_utilization_pct = aggregate_cpu_utilization_pct / logical_cpus as f64;
    let queue_ms = args
        .queued_unix_ms
        .map(|queued| started_unix_ms.saturating_sub(queued));

    let payload = Value::from(
        Object::new()
            .with("schema_version", SCHEMA_VERSION.to_json())
            .with("command", "perf-sample".to_json())
            .with(
                "sample_identity",
                Value::from(
                    Object::new()
                        .with("pair_id", args.pair_id.to_json())
                        .with("variant", args.variant.to_json())
                        .with("sample_index", args.sample_index.to_json())
                        .with("cache_state", args.cache_state.to_json())
                        .with("cache_classes", args.cache_classes.to_json())
                        .with("partial_change", args.partial_change.to_json())
                        .with("node_class", args.node_class.to_json())
                        .with("runner_image", runner_image.to_json())
                        .with("started_unix_ms", started_unix_ms)
                        .with("measured_command", args.command.to_json())
                        .with("worktree", (root.display().to_string()).to_json())
                        .with("git_head", (git_head.trim()).to_json())
                        .with("git_tree", (git_tree.trim()).to_json())
                        .with("cargo_lock_sha256", cargo_lock_sha256.to_json())
                        .with(
                            "dependency_resolution_sha256",
                            dependency_resolution_sha256.to_json(),
                        )
                        .with(
                            "pipeline_build_fingerprint",
                            pipeline_build_fingerprint.to_json(),
                        )
                        .with("rustc_vv", (rustc.trim()).to_json())
                        .with("cargo_version", (cargo.trim()).to_json())
                        .with("nextest_version", (nextest.trim()).to_json())
                        .with(
                            "build_environment",
                            Value::from(
                                Object::new()
                                    .with(
                                        "CARGO_BUILD_TARGET",
                                        (std::env::var("CARGO_BUILD_TARGET").ok()).to_json(),
                                    )
                                    .with(
                                        "CARGO_TARGET_DIR",
                                        (std::env::var("CARGO_TARGET_DIR").ok()).to_json(),
                                    )
                                    .with("RUSTFLAGS", (std::env::var("RUSTFLAGS").ok()).to_json())
                                    .with(
                                        "CARGO_ENCODED_RUSTFLAGS",
                                        (std::env::var("CARGO_ENCODED_RUSTFLAGS").ok()).to_json(),
                                    ),
                            ),
                        )
                        .with("host", host.to_json())
                        .with("generated_tree", generated_identity.to_json())
                        .with("identity_receipts", identity_receipts.to_json())
                        .with("cache_census", cache_census.to_json()),
                ),
            )
            .with(
                "deterministic_work",
                Value::from(
                    Object::new()
                        .with("command_telemetry", work_telemetry.to_json())
                        .with("output_census", output_census.to_json())
                        .with("proof_inventory", proof_inventory.to_json())
                        .with("causal_counters", causal_counters.to_json()),
                ),
            )
            .with(
                "observations",
                Value::from(
                    Object::new()
                        .with("wall_ms", wall_ms)
                        .with("queue_ms", queue_ms)
                        .with(
                            "aggregate_cpu_utilization_pct",
                            aggregate_cpu_utilization_pct.to_json(),
                        )
                        .with(
                            "host_normalized_cpu_utilization_pct",
                            host_normalized_cpu_utilization_pct.to_json(),
                        )
                        .with("resource_usage", resource_usage.to_json())
                        .with("loadavg_before", load_before.to_json())
                        .with("loadavg_after", load_after.to_json())
                        .with("exit", (exit_observation(exit_status)).to_json()),
                ),
            ),
    );
    write_json_atomic(&args.output, &payload)?;
    println!("{}", args.output.display());
    Ok(exit_status.code().unwrap_or(1))
}

fn proof_inventory(work_telemetry: Option<&Value>, command: &[String]) -> Vec<String> {
    let mut inventory = work_telemetry
        .and_then(|receipt| receipt.pointer("/payload/commands"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("command").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if inventory.is_empty() {
        inventory.push(command.join(" "));
    }
    inventory.sort();
    inventory.dedup();
    inventory
}

/// Project counters shared by the legacy and current perf-gate telemetry schemas.
///
/// The baseline predates explicit `executed_stage_count`, but its sync record still
/// carries one top-level `stage:stage-*` timing per executed production stage. Count
/// only those top-level rows (never nested phase timings), while current telemetry uses
/// the producer's explicit count. This makes the comparison an observation of actual
/// work from each run rather than a source-version lookup or hand-written estimate.
fn causal_counters(work_telemetry: Option<&Value>) -> Value {
    let mut counters = Object::new();
    // A direct host command has no CI job group that can fall back to compiling the
    // source producer independently. Hosted run receipts populate the same counter
    // from the authored workflow.
    counters.insert(
        "source_producer_fallback_job_groups".to_string(),
        Value::from(0_u64),
    );

    let Some(payload) = work_telemetry.and_then(|receipt| receipt.get("payload")) else {
        return Value::Object(counters);
    };
    let Some(sync) = payload
        .get("commands")
        .and_then(Value::as_array)
        .and_then(|commands| {
            commands
                .iter()
                .find(|command| command.get("command").and_then(Value::as_str) == Some("sync"))
        })
    else {
        return Value::Object(counters);
    };

    let pipeline_runs = sync
        .pointer("/observations/pipeline_runs")
        .or_else(|| sync.get("pipeline_runs"))
        .and_then(Value::as_u64);
    if let Some(pipeline_runs) = pipeline_runs {
        counters.insert("pipeline_runs".to_string(), Value::from(pipeline_runs));
    }

    let explicit_stage_count = sync
        .pointer("/observations/executed_stage_count")
        .or_else(|| sync.get("executed_stage_count"))
        .and_then(Value::as_u64);
    let stage_count = explicit_stage_count
        .or_else(|| (pipeline_runs == Some(0)).then_some(0))
        .or_else(|| legacy_executed_stage_count(sync));
    if let Some(stage_count) = stage_count {
        counters.insert(
            "pipeline_stage_executions".to_string(),
            Value::from(stage_count),
        );
    }

    Value::Object(counters)
}

fn legacy_executed_stage_count(sync: &Value) -> Option<u64> {
    let timings = sync.get("timings")?.as_array()?;
    let stages = timings
        .iter()
        .filter_map(|timing| timing.get("phase").and_then(Value::as_str))
        .filter_map(|phase| phase.strip_prefix("stage:"))
        .filter(|stage_and_phase| !stage_and_phase.contains('/'))
        .collect::<std::collections::BTreeSet<_>>();
    (!stages.is_empty()).then(|| u64::try_from(stages.len()).unwrap_or(u64::MAX))
}

fn run_text(root: &Path, program: &str, args: &[&str]) -> PerfResult<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|error| format!("run {program} {args:?}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{program} {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)
        .map_err(|error| format!("{program} output is not UTF-8: {error}"))?)
}

fn sha256_file(path: &Path) -> PerfResult<String> {
    use std::io::Read as _;

    let mut file = fs::File::open(path)
        .map_err(|error| format!("open {} for hashing: {error}", path.display()))?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("hash {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(bytes);
    format!("{:x}", hash.finalize())
}

/// Resolve the producer build identity from an immutable generation receipt.
///
/// CI carries the authenticated producer receipt into every measured shard. Local
/// source-backed measurements use the sync manifest written by that same producer. A
/// retained baseline manifest can be named explicitly as the `producer` identity so it
/// remains outside the live sync-cache path. A cold cache miss must still provide one
/// of those authorities; guessing from the sampler's own build would bind the
/// observation to the wrong executable.
fn pipeline_build_fingerprint(
    root: &Path,
    identity_receipts: &[(String, PathBuf)],
) -> PerfResult<String> {
    if let Some((_, path)) = identity_receipts
        .iter()
        .find(|(name, _)| name == "producer")
    {
        let path = resolve(root, path);
        return build_fingerprint_from(
            &path,
            &[
                "/payload/build_identity/fingerprint",
                "/build_identity/fingerprint",
                "/build_fingerprint",
            ],
        )?
        .ok_or_else(|| {
            format!(
                "explicit producer identity {} lacks a supported build fingerprint",
                path.display()
            )
            .into()
        });
    }

    let candidates = [
        (
            root.join("dist/producer-receipt.json"),
            "/payload/build_identity/fingerprint",
        ),
        (
            root.join(".cache/gmeow-sync/manifests/generated-default.json"),
            "/build_fingerprint",
        ),
    ];
    for (path, pointer) in candidates {
        if let Some(fingerprint) = build_fingerprint_from(&path, &[pointer])? {
            return Ok(fingerprint);
        }
    }
    Err("no producer build identity found; pass --identity-receipt producer=PATH, provide dist/producer-receipt.json, or materialize the exact generated sync manifest before sampling".into())
}

fn build_fingerprint_from(path: &Path, pointers: &[&str]) -> PerfResult<Option<String>> {
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(path)
        .map_err(|error| format!("read producer identity {}: {error}", path.display()))?;
    let receipt: Value = json::record::read_document(&bytes)
        .map_err(|error| format!("parse producer identity {}: {error}", path.display()))?;
    let Some(fingerprint) = pointers
        .iter()
        .find_map(|pointer| receipt.pointer(pointer).and_then(Value::as_str))
    else {
        return Ok(None);
    };
    if fingerprint.len() != 64
        || !fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(format!(
            "producer identity {} has malformed build fingerprint {fingerprint:?}",
            path.display()
        )
        .into());
    }
    Ok(Some(fingerprint.to_string()))
}

fn dependency_resolution_digest(root: &Path) -> PerfResult<String> {
    let output = Command::new("cargo")
        .args(["metadata", "--locked", "--format-version", "1"])
        .current_dir(root)
        .output()
        .map_err(|error| format!("run cargo metadata: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata --locked failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    dependency_resolution_digest_from(&output.stdout)
}

fn dependency_resolution_digest_from(bytes: &[u8]) -> PerfResult<String> {
    let metadata: Value = json::record::read_document(bytes)
        .map_err(|error| format!("parse cargo metadata: {error}"))?;
    let packages = metadata
        .get("packages")
        .and_then(Value::as_array)
        .ok_or("cargo metadata lacks packages")?;
    let mut identities = packages
        .iter()
        .filter_map(|package| {
            let source = package.get("source")?.as_str()?;
            Some(format!(
                "{}\0{}\0{}\0{}",
                package.get("name")?.as_str()?,
                package.get("version")?.as_str()?,
                source,
                package
                    .get("checksum")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            ))
        })
        .collect::<Vec<_>>();
    identities.sort();
    identities.dedup();
    let mut digest = Sha256::new();
    digest.update(b"gmeow:external-dependency-resolution:v1\0");
    for identity in identities {
        digest.update(identity.as_bytes());
        digest.update([0]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn named_file_receipts(
    root: &Path,
    inputs: &[(String, PathBuf)],
) -> PerfResult<BTreeMap<String, Value>> {
    let mut receipts = BTreeMap::new();
    for (name, path) in inputs {
        if receipts.contains_key(name) {
            return Err(format!("duplicate identity receipt name {name:?}").into());
        }
        let resolved = resolve(root, path);
        let file_bytes = fs::metadata(&resolved)
            .map_err(|error| format!("stat identity receipt {}: {error}", resolved.display()))?
            .len();
        let parsed_json = fs::read(&resolved)
            .ok()
            .and_then(|bytes| json::record::read_document(&bytes).ok());
        receipts.insert(
            name.clone(),
            Value::from(
                Object::new()
                    .with("path", (display_path(root, &resolved)).to_json())
                    .with("sha256", (sha256_file(&resolved)?).to_json())
                    .with("bytes", file_bytes.to_json())
                    .with("json", parsed_json.to_json()),
            ),
        );
    }
    Ok(receipts)
}

fn named_tree_receipts(
    root: &Path,
    inputs: &[(String, PathBuf)],
) -> PerfResult<BTreeMap<String, Value>> {
    let mut receipts = BTreeMap::new();
    for (name, path) in inputs {
        if receipts.contains_key(name) {
            return Err(format!("duplicate cache root name {name:?}").into());
        }
        let resolved = resolve(root, path);
        let census = if resolved.exists() {
            let census = tree_census(&resolved)?;
            Value::from(
                Object::new()
                    .with("exists", true.to_json())
                    .with("path", (display_path(root, &resolved)).to_json())
                    .with("tree", census.to_json()),
            )
        } else {
            Value::from(
                Object::new()
                    .with("exists", false.to_json())
                    .with("path", (display_path(root, &resolved)).to_json()),
            )
        };
        receipts.insert(name.clone(), census);
    }
    Ok(receipts)
}

fn resolve(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn tree_census(root: &Path) -> PerfResult<Value> {
    let mut files = Vec::new();
    collect_tree_files(root, root, &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut tree = Sha256::new();
    tree.update(b"gmeow:tree-census:v1\0");
    let mut total_bytes = 0_u64;
    for (path, resolved) in &files {
        let bytes = fs::metadata(resolved)
            .map_err(|error| format!("stat {}: {error}", resolved.display()))?
            .len();
        let digest = sha256_file(resolved)?;
        tree.update(path.as_bytes());
        tree.update([0]);
        tree.update(digest.as_bytes());
        tree.update([0]);
        tree.update(bytes.to_string().as_bytes());
        tree.update([0]);
        total_bytes = total_bytes.saturating_add(bytes);
    }
    Ok(Value::from(
        Object::new()
            .with("sha256", (format!("{:x}", tree.finalize())).to_json())
            .with("file_count", (files.len()).to_json())
            .with("bytes", total_bytes.to_json()),
    ))
}

fn collect_tree_files(
    root: &Path,
    current: &Path,
    files: &mut Vec<(String, PathBuf)>,
) -> PerfResult<()> {
    let mut entries = fs::read_dir(current)
        .map_err(|error| format!("read cache tree {}: {error}", current.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("read cache tree {}: {error}", current.display()))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let file_type = entry
            .file_type()
            .map_err(|error| format!("inspect {}: {error}", entry.path().display()))?;
        let path = entry.path();
        if file_type.is_symlink() {
            return Err(format!("tree census refuses symbolic link {}", path.display()).into());
        }
        if file_type.is_dir() {
            collect_tree_files(root, &path, files)?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|error| format!("relativize {}: {error}", path.display()))?
                .to_str()
                .ok_or_else(|| format!("non-UTF-8 tree path {}", path.display()))?
                .replace(std::path::MAIN_SEPARATOR, "/");
            files.push((relative, path));
        }
    }
    Ok(())
}

fn read_trimmed(path: &str) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
}

fn host_identity() -> Value {
    let cpu_model = fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|cpuinfo| {
            cpuinfo.lines().find_map(|line| {
                line.strip_prefix("model name")
                    .and_then(|rest| rest.split_once(':'))
                    .map(|(_, value)| value.trim().to_string())
            })
        });
    let mem_total_kib = fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|meminfo| {
            meminfo.lines().find_map(|line| {
                line.strip_prefix("MemTotal:")
                    .and_then(|rest| rest.split_whitespace().next())
                    .and_then(|value| value.parse::<u64>().ok())
            })
        });
    Value::from(
        Object::new()
            .with("os", (std::env::consts::OS).to_json())
            .with("arch", (std::env::consts::ARCH).to_json())
            .with(
                "logical_cpus",
                (std::thread::available_parallelism().map(|n| n.get()).ok()).to_json(),
            )
            .with("cpu_model", cpu_model.to_json())
            .with("mem_total_kib", mem_total_kib.to_json())
            .with(
                "kernel_release",
                (read_trimmed("/proc/sys/kernel/osrelease")).to_json(),
            )
            .with(
                "github_runner_name",
                (std::env::var("RUNNER_NAME").ok()).to_json(),
            )
            .with(
                "github_run_id",
                (std::env::var("GITHUB_RUN_ID").ok()).to_json(),
            )
            .with(
                "github_run_attempt",
                (std::env::var("GITHUB_RUN_ATTEMPT").ok()).to_json(),
            ),
    )
}

fn exit_observation(status: ExitStatus) -> Value {
    use std::os::unix::process::ExitStatusExt;
    Value::from(
        Object::new()
            .with("success", (status.success()).to_json())
            .with("code", (status.code()).to_json())
            .with("signal", (status.signal()).to_json()),
    )
}

fn child_usage() -> PerfResult<Usage> {
    let mut raw = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    // SAFETY: raw points to writable storage for exactly one libc::rusage; the
    // kernel initializes it on success, checked by the return value before assume_init.
    let result = unsafe { libc::getrusage(libc::RUSAGE_CHILDREN, raw.as_mut_ptr()) };
    if result != 0 {
        return Err(format!(
            "getrusage(RUSAGE_CHILDREN): {}",
            std::io::Error::last_os_error()
        )
        .into());
    }
    // SAFETY: a zero return from getrusage guarantees the output struct was initialized.
    let raw = unsafe { raw.assume_init() };
    Ok(Usage {
        user_us: timeval_us(raw.ru_utime),
        system_us: timeval_us(raw.ru_stime),
        max_rss_kib: nonnegative(raw.ru_maxrss),
        minor_faults: nonnegative(raw.ru_minflt),
        major_faults: nonnegative(raw.ru_majflt),
        input_blocks: nonnegative(raw.ru_inblock),
        output_blocks: nonnegative(raw.ru_oublock),
        voluntary_context_switches: nonnegative(raw.ru_nvcsw),
        involuntary_context_switches: nonnegative(raw.ru_nivcsw),
    })
}

fn timeval_us(value: libc::timeval) -> u128 {
    u128::try_from(value.tv_sec)
        .unwrap_or(0)
        .saturating_mul(1_000_000)
        .saturating_add(u128::try_from(value.tv_usec).unwrap_or(0))
}

fn nonnegative<T>(value: T) -> u64
where
    i128: From<T>,
{
    u64::try_from(i128::from(value)).unwrap_or(0)
}

#[cfg(test)]
#[path = "sample.tests.rs"]
mod tests;
