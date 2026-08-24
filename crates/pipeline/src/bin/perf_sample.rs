// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Run one report-only paired performance sample with exact source/tool/host identity.
//!
//! This is deliberately not a correctness gate. It records deterministic identity and
//! child-process resource observations in separate JSON objects; the repeated paired
//! protocol and median comparison live in docs/rust-test-performance.md.

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
    fn delta(self, before: Self) -> serde_json::Value {
        serde_json::json!({
            "user_cpu_ms": self.user_us.saturating_sub(before.user_us) / 1_000,
            "system_cpu_ms": self.system_us.saturating_sub(before.system_us) / 1_000,
            "max_rss_kib": self.max_rss_kib,
            "pre_sample_children_max_rss_kib": before.max_rss_kib,
            "minor_faults": self.minor_faults.saturating_sub(before.minor_faults),
            "major_faults": self.major_faults.saturating_sub(before.major_faults),
            "filesystem_input_blocks": self.input_blocks.saturating_sub(before.input_blocks),
            "filesystem_output_blocks": self.output_blocks.saturating_sub(before.output_blocks),
            "voluntary_context_switches": self
                .voluntary_context_switches
                .saturating_sub(before.voluntary_context_switches),
            "involuntary_context_switches": self
                .involuntary_context_switches
                .saturating_sub(before.involuntary_context_switches),
            "scope": "waited command process tree via getrusage(RUSAGE_CHILDREN)",
        })
    }
}

fn main() {
    match parse_args().and_then(run) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("perf-sample: {error}");
            std::process::exit(2);
        }
    }
}

fn parse_args() -> Result<Args, String> {
    parse_args_from(std::env::args_os().skip(1))
}

fn parse_args_from(arguments: impl IntoIterator<Item = OsString>) -> Result<Args, String> {
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
            _ => {
                return Err(format!(
                    "unknown argument {argument:?}; separate the command with --"
                ));
            }
        }
    }

    let pair_id = required(pair_id, "--pair-id")?;
    let variant = required(variant, "--variant")?;
    if !matches!(variant.as_str(), "baseline" | "candidate") {
        return Err("--variant must be baseline or candidate".to_string());
    }
    let sample_index = required(sample_index, "--sample-index")?;
    if sample_index == 0 {
        return Err("--sample-index is one-based".to_string());
    }
    let cache_state = required(cache_state, "--cache-state")?;
    if !matches!(cache_state.as_str(), "cold" | "warm" | "partial") {
        return Err("--cache-state must be cold, warm, or partial".to_string());
    }
    for (required_class, option) in [
        ("cargo", "--cargo-cache-state"),
        ("sync_manifest", "--sync-cache-state"),
        ("pipeline", "--pipeline-cache-state"),
        ("fixture", "--fixture-cache-state"),
    ] {
        if !cache_classes.contains_key(required_class) {
            return Err(format!("missing required {option}"));
        }
    }
    for (class, state) in &cache_classes {
        if !matches!(
            state.as_str(),
            "cold" | "warm" | "partial" | "absent" | "not-applicable"
        ) {
            return Err(format!(
                "cache class {class} has invalid state {state:?}; expected cold, warm, partial, absent, or not-applicable"
            ));
        }
    }
    match (cache_state.as_str(), partial_change.as_ref()) {
        ("partial", None) => {
            return Err("--cache-state partial requires --partial-change".to_string());
        }
        ("partial", Some(change)) if !valid_partial_change(change) => {
            return Err(
                "--partial-change must be KIND:PATH:SHA256 with a 64-digit lowercase digest"
                    .to_string(),
            );
        }
        ("cold" | "warm", Some(_)) => {
            return Err("--partial-change is valid only for a partial sample".to_string());
        }
        _ => {}
    }
    let node_class = required(node_class, "--node-class")?;
    let output = required(output, "--output")?;
    if command.is_empty() {
        return Err("a command is required after --".to_string());
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
        command,
    })
}

fn parse_named_path(raw: &str, option: &str) -> Result<(String, PathBuf), String> {
    let (name, path) = raw
        .split_once('=')
        .ok_or_else(|| format!("{option} must be NAME=PATH"))?;
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        || path.is_empty()
    {
        return Err(format!(
            "{option} must be NAME=PATH with a portable non-empty name"
        ));
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

fn required<T>(value: Option<T>, name: &str) -> Result<T, String> {
    value.ok_or_else(|| format!("missing required {name}"))
}

fn run(args: Args) -> Result<i32, String> {
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
        ));
    }

    let git_head = run_text(&root, "git", &["rev-parse", "HEAD"])?;
    let git_tree = run_text(&root, "git", &["rev-parse", "HEAD^{tree}"])?;
    let cargo_lock_sha256 = sha256_file(&root.join("Cargo.lock"))?;
    let dependency_resolution_sha256 = dependency_resolution_digest(&root)?;
    let rustc = run_text(&root, "rustc", &["-Vv"])?;
    let cargo = run_text(&root, "cargo", &["--version", "--verbose"])?;
    let nextest = run_text(&root, "cargo", &["nextest", "--version"])?;
    let host = host_identity();
    let runner_image = args.runner_image.or_else(|| {
        let os = std::env::var("ImageOS").ok()?;
        let version = std::env::var("ImageVersion").ok();
        Some(version.map_or(os.clone(), |version| format!("{os}@{version}")))
    });
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
            let value = serde_json::from_slice::<serde_json::Value>(&bytes)
                .map_err(|error| format!("parse work telemetry {}: {error}", path.display()))?;
            Ok::<_, String>(serde_json::json!({
                "path": path.strip_prefix(&root).unwrap_or(&path).display().to_string(),
                "sha256": sha256_bytes(&bytes),
                "payload": value,
            }))
        })
        .transpose()?;

    let resource_usage = usage_after.delta(usage_before);
    let cpu_ms = resource_usage
        .get("user_cpu_ms")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
        .saturating_add(
            resource_usage
                .get("system_cpu_ms")
                .and_then(serde_json::Value::as_u64)
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

    let payload = serde_json::json!({
        "schema_version": SCHEMA_VERSION,
        "command": "perf-sample",
        "sample_identity": {
            "pair_id": args.pair_id,
            "variant": args.variant,
            "sample_index": args.sample_index,
            "cache_state": args.cache_state,
            "cache_classes": args.cache_classes,
            "partial_change": args.partial_change,
            "node_class": args.node_class,
            "runner_image": runner_image,
            "started_unix_ms": started_unix_ms,
            "measured_command": args.command,
            "worktree": root.display().to_string(),
            "git_head": git_head.trim(),
            "git_tree": git_tree.trim(),
            "cargo_lock_sha256": cargo_lock_sha256,
            "dependency_resolution_sha256": dependency_resolution_sha256,
            "pipeline_build_fingerprint": gmeow_pipeline::cache::BUILD_FINGERPRINT,
            "rustc_vv": rustc.trim(),
            "cargo_version": cargo.trim(),
            "nextest_version": nextest.trim(),
            "build_environment": {
                "CARGO_BUILD_TARGET": std::env::var("CARGO_BUILD_TARGET").ok(),
                "CARGO_TARGET_DIR": std::env::var("CARGO_TARGET_DIR").ok(),
                "RUSTFLAGS": std::env::var("RUSTFLAGS").ok(),
                "CARGO_ENCODED_RUSTFLAGS": std::env::var("CARGO_ENCODED_RUSTFLAGS").ok(),
            },
            "host": host,
            "generated_tree": generated_identity,
            "identity_receipts": identity_receipts,
            "cache_census": cache_census,
        },
        "deterministic_work": {
            "command_telemetry": work_telemetry,
        },
        "observations": {
            "wall_ms": wall_ms,
            "queue_ms": queue_ms,
            "aggregate_cpu_utilization_pct": aggregate_cpu_utilization_pct,
            "host_normalized_cpu_utilization_pct": host_normalized_cpu_utilization_pct,
            "resource_usage": resource_usage,
            "loadavg_before": load_before,
            "loadavg_after": load_after,
            "exit": exit_observation(exit_status),
        },
    });
    write_json_atomic(&args.output, &payload)?;
    println!("{}", args.output.display());
    Ok(exit_status.code().unwrap_or(1))
}

fn run_text(root: &Path, program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|error| format!("run {program} {args:?}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{program} {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("{program} output is not UTF-8: {error}"))
}

fn sha256_file(path: &Path) -> Result<String, String> {
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

fn dependency_resolution_digest(root: &Path) -> Result<String, String> {
    let output = Command::new("cargo")
        .args(["metadata", "--locked", "--format-version", "1"])
        .current_dir(root)
        .output()
        .map_err(|error| format!("run cargo metadata: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata --locked failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("parse cargo metadata: {error}"))?;
    let packages = metadata
        .get("packages")
        .and_then(serde_json::Value::as_array)
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
                    .and_then(serde_json::Value::as_str)
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
) -> Result<BTreeMap<String, serde_json::Value>, String> {
    let mut receipts = BTreeMap::new();
    for (name, path) in inputs {
        if receipts.contains_key(name) {
            return Err(format!("duplicate identity receipt name {name:?}"));
        }
        let resolved = resolve(root, path);
        let file_bytes = fs::metadata(&resolved)
            .map_err(|error| format!("stat identity receipt {}: {error}", resolved.display()))?
            .len();
        let parsed_json = fs::read(&resolved)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
        receipts.insert(
            name.clone(),
            serde_json::json!({
                "path": display_path(root, &resolved),
                "sha256": sha256_file(&resolved)?,
                "bytes": file_bytes,
                "json": parsed_json,
            }),
        );
    }
    Ok(receipts)
}

fn named_tree_receipts(
    root: &Path,
    inputs: &[(String, PathBuf)],
) -> Result<BTreeMap<String, serde_json::Value>, String> {
    let mut receipts = BTreeMap::new();
    for (name, path) in inputs {
        if receipts.contains_key(name) {
            return Err(format!("duplicate cache root name {name:?}"));
        }
        let resolved = resolve(root, path);
        let census = if resolved.exists() {
            let census = tree_census(&resolved)?;
            serde_json::json!({
                "exists": true,
                "path": display_path(root, &resolved),
                "tree": census,
            })
        } else {
            serde_json::json!({
                "exists": false,
                "path": display_path(root, &resolved),
            })
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

fn tree_census(root: &Path) -> Result<serde_json::Value, String> {
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
    Ok(serde_json::json!({
        "sha256": format!("{:x}", tree.finalize()),
        "file_count": files.len(),
        "bytes": total_bytes,
    }))
}

fn collect_tree_files(
    root: &Path,
    current: &Path,
    files: &mut Vec<(String, PathBuf)>,
) -> Result<(), String> {
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
            return Err(format!(
                "tree census refuses symbolic link {}",
                path.display()
            ));
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

fn host_identity() -> serde_json::Value {
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
    serde_json::json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "logical_cpus": std::thread::available_parallelism().map(|n| n.get()).ok(),
        "cpu_model": cpu_model,
        "mem_total_kib": mem_total_kib,
        "kernel_release": read_trimmed("/proc/sys/kernel/osrelease"),
        "github_runner_name": std::env::var("RUNNER_NAME").ok(),
        "github_run_id": std::env::var("GITHUB_RUN_ID").ok(),
        "github_run_attempt": std::env::var("GITHUB_RUN_ATTEMPT").ok(),
    })
}

fn exit_observation(status: ExitStatus) -> serde_json::Value {
    use std::os::unix::process::ExitStatusExt;
    serde_json::json!({
        "success": status.success(),
        "code": status.code(),
        "signal": status.signal(),
    })
}

fn child_usage() -> Result<Usage, String> {
    let mut raw = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    // SAFETY: raw points to writable storage for exactly one libc::rusage; the
    // kernel initializes it on success, checked by the return value before assume_init.
    let result = unsafe { libc::getrusage(libc::RUSAGE_CHILDREN, raw.as_mut_ptr()) };
    if result != 0 {
        return Err(format!(
            "getrusage(RUSAGE_CHILDREN): {}",
            std::io::Error::last_os_error()
        ));
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

fn write_json_atomic(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| format!("create output directory {}: {error}", parent.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("create temporary sample in {}: {error}", parent.display()))?;
    serde_json::to_writer_pretty(&mut temporary, value)
        .map_err(|error| format!("serialize performance sample: {error}"))?;
    use std::io::Write as _;
    temporary
        .write_all(b"\n")
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|error| format!("flush performance sample: {error}"))?;
    temporary.persist(path).map_err(|error| {
        format!(
            "publish performance sample {}: {}",
            path.display(),
            error.error
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_args(cache_state: &str) -> Vec<OsString> {
        [
            "--pair-id",
            "pair",
            "--variant",
            "candidate",
            "--sample-index",
            "1",
            "--cache-state",
            cache_state,
            "--cargo-cache-state",
            cache_state,
            "--sync-cache-state",
            cache_state,
            "--pipeline-cache-state",
            cache_state,
            "--fixture-cache-state",
            cache_state,
            "--node-class",
            "node",
            "--output",
            "sample.json",
        ]
        .into_iter()
        .map(OsString::from)
        .collect()
    }

    #[test]
    fn partial_samples_require_a_named_path_and_digest() {
        let mut missing = base_args("partial");
        missing.extend([OsString::from("--"), OsString::from("true")]);
        assert!(parse_args_from(missing).is_err());

        let mut complete = base_args("partial");
        complete.extend([
            OsString::from("--partial-change"),
            OsString::from(format!("irrelevant:slices/example.ttl:{}", "a".repeat(64))),
            OsString::from("--"),
            OsString::from("true"),
        ]);
        let parsed = parse_args_from(complete).unwrap();
        assert_eq!(parsed.cache_state, "partial");
        assert_eq!(parsed.cache_classes["pipeline"], "partial");
    }

    #[test]
    fn the_four_cache_classes_are_not_collapsed_into_one_label() {
        let mut args = base_args("warm");
        let sync_option = args
            .iter()
            .position(|arg| arg == "--sync-cache-state")
            .unwrap();
        args.drain(sync_option..=sync_option + 1);
        args.extend([OsString::from("--"), OsString::from("true")]);
        assert_eq!(
            parse_args_from(args).unwrap_err(),
            "missing required --sync-cache-state"
        );
    }

    #[test]
    fn tree_census_binds_paths_bytes_and_content() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("nested")).unwrap();
        fs::write(directory.path().join("b"), b"two").unwrap();
        fs::write(directory.path().join("nested/a"), b"one").unwrap();
        let first = tree_census(directory.path()).unwrap();
        fs::write(directory.path().join("nested/a"), b"ONE").unwrap();
        let second = tree_census(directory.path()).unwrap();
        assert_eq!(first["file_count"], 2);
        assert_eq!(first["bytes"], 6);
        assert_ne!(first["sha256"], second["sha256"]);
    }
}
