// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! A small, dependency-free DAG runner for the full host gate. It owns the
//! worktree lock, runs synchronization and Rust preparation once, then schedules
//! independent gates concurrently without imposing a thread cap on any child
//! tool.

mod evidence;

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

const LOCK_ROOT_ENV: &str = "GMEOW_TASK_LOCK_ROOT";
const LOCK_TOKEN_ENV: &str = "GMEOW_TASK_LOCK_TOKEN";
const TOOLCHAIN_RECEIPT_FILES: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    ".cargo/config.toml",
    "Makefile",
    "crates/xtask/src/main.rs",
    "crates/xtask/src/evidence.rs",
    ".github/workflows/ci.yml",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum CheckProfile {
    Full,
    Impact,
}

#[derive(Clone, Copy)]
struct Task {
    name: &'static str,
    target: &'static str,
    dependencies: &'static [&'static str],
}

const ROOT: &[&str] = &[];
const AFTER_SYNC: &[&str] = &["sync"];
const AFTER_RUST_BUILD: &[&str] = &["rust-build"];
const AFTER_REASON: &[&str] = &["reason-verify"];
const FINAL_DEPS: &[&str] = &[
    "check-lint",
    "rust-gate",
    "validate",
    "constitution-check",
    "crate-check",
    "audit",
    "wikidata",
    "coverage",
    "acceptance",
    "reason-verify",
    "lint-alignment",
    "i18n-lint",
    "doc-lint",
    "coherence-gate-teeth",
    "slice-quality-gate",
    "bench-soak",
];

const CHECK_DAG: &[Task] = &[
    Task {
        name: "sync",
        target: "check-sync",
        dependencies: ROOT,
    },
    Task {
        name: "check-lint",
        target: "check-lint",
        dependencies: AFTER_SYNC,
    },
    Task {
        name: "rust-build",
        target: "rust-build",
        dependencies: AFTER_SYNC,
    },
    Task {
        name: "rust-gate",
        target: "rust-gate",
        dependencies: AFTER_RUST_BUILD,
    },
    Task {
        name: "validate",
        target: "validate",
        dependencies: AFTER_SYNC,
    },
    Task {
        name: "constitution-check",
        target: "constitution-check",
        dependencies: AFTER_SYNC,
    },
    Task {
        name: "crate-check",
        target: "crate-check",
        dependencies: AFTER_SYNC,
    },
    Task {
        name: "audit",
        target: "audit",
        dependencies: AFTER_SYNC,
    },
    Task {
        name: "wikidata",
        target: "wikidata",
        dependencies: AFTER_SYNC,
    },
    Task {
        name: "coverage",
        target: "coverage",
        dependencies: AFTER_SYNC,
    },
    Task {
        name: "acceptance",
        target: "acceptance",
        dependencies: AFTER_SYNC,
    },
    Task {
        name: "reason-verify",
        target: "reason-verify",
        dependencies: AFTER_SYNC,
    },
    Task {
        name: "lint-alignment",
        target: "lint-alignment",
        dependencies: AFTER_SYNC,
    },
    Task {
        name: "i18n-lint",
        target: "i18n-lint",
        dependencies: AFTER_SYNC,
    },
    Task {
        name: "doc-lint",
        target: "doc-lint",
        dependencies: AFTER_SYNC,
    },
    Task {
        name: "coherence-gate-teeth",
        target: "coherence-gate-teeth",
        dependencies: AFTER_REASON,
    },
    Task {
        name: "slice-quality-gate",
        target: "slice-quality-gate",
        dependencies: AFTER_SYNC,
    },
    Task {
        name: "bench-soak",
        target: "bench-soak",
        dependencies: AFTER_SYNC,
    },
    Task {
        name: "compliance-report",
        target: "compliance-report",
        dependencies: FINAL_DEPS,
    },
];

struct WorktreeLock {
    file: File,
}

impl WorktreeLock {
    fn acquire(root: &Path) -> Option<Self> {
        let dir = root.join(".cache/gmeow-task");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            eprintln!("xtask: create {}: {e}", dir.display());
            return None;
        }
        let path = dir.join("runner.lock");
        let mut file = match OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
        {
            Ok(file) => file,
            Err(e) => {
                eprintln!("xtask: open {}: {e}", path.display());
                return None;
            }
        };
        match file.try_lock() {
            Ok(()) => {
                let owner = format!(
                    "pid={} purpose=check root={}\n",
                    std::process::id(),
                    root.display()
                );
                if let Err(e) = file
                    .set_len(0)
                    .and_then(|()| file.seek(SeekFrom::Start(0)).map(|_| ()))
                    .and_then(|()| file.write_all(owner.as_bytes()))
                    .and_then(|()| file.flush())
                {
                    eprintln!("xtask: initialize worktree lock: {e}");
                    return None;
                }
                Some(Self { file })
            }
            Err(TryLockError::WouldBlock) => {
                let mut owner = String::new();
                let _ = file.seek(SeekFrom::Start(0));
                let _ = file.read_to_string(&mut owner);
                eprintln!("xtask: worktree task already running: {}", owner.trim());
                None
            }
            Err(TryLockError::Error(e)) => {
                eprintln!("xtask: acquire worktree lock: {e}");
                None
            }
        }
    }
}

impl Drop for WorktreeLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".to_string());
    match command.as_str() {
        "check" => {
            let mut jobs = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
            let mut profile = CheckProfile::Impact;
            let mut base = None;
            let mut explain = false;
            let mut timings_json = None;
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "-j" | "--jobs" => {
                        let Some(value) = args.next() else {
                            eprintln!("xtask check: {arg} requires a positive integer");
                            return ExitCode::from(2);
                        };
                        jobs = match value.parse::<usize>() {
                            Ok(value) if value > 0 => value,
                            _ => {
                                eprintln!("xtask check: invalid jobs value {value:?}");
                                return ExitCode::from(2);
                            }
                        };
                    }
                    "--profile" => {
                        let Some(value) = args.next() else {
                            eprintln!("xtask check: --profile requires full or impact");
                            return ExitCode::from(2);
                        };
                        profile = match value.as_str() {
                            "full" => CheckProfile::Full,
                            "impact" => CheckProfile::Impact,
                            _ => {
                                eprintln!("xtask check: unknown profile {value:?}");
                                return ExitCode::from(2);
                            }
                        };
                    }
                    "--base" => {
                        let Some(value) = args.next() else {
                            eprintln!("xtask check: --base requires a git revision");
                            return ExitCode::from(2);
                        };
                        base = Some(value);
                    }
                    "--explain" => explain = true,
                    "--timings-json" => {
                        let Some(value) = args.next() else {
                            eprintln!("xtask check: --timings-json requires a path");
                            return ExitCode::from(2);
                        };
                        timings_json = Some(PathBuf::from(value));
                    }
                    _ => {
                        eprintln!("xtask check: unknown argument {arg:?}");
                        return ExitCode::from(2);
                    }
                }
            }
            run_check(
                jobs,
                profile,
                base.as_deref(),
                explain,
                timings_json.as_deref(),
            )
        }
        "receipt" => {
            if args.next().as_deref() != Some("create") {
                eprintln!("usage: cargo xtask receipt create --out PATH");
                return ExitCode::from(2);
            }
            let mut out = None;
            while let Some(arg) = args.next() {
                if arg != "--out" {
                    eprintln!("xtask receipt create: unknown argument {arg:?}");
                    return ExitCode::from(2);
                }
                let Some(value) = args.next() else {
                    eprintln!("xtask receipt create: --out requires a path");
                    return ExitCode::from(2);
                };
                out = Some(PathBuf::from(value));
            }
            let Some(out) = out else {
                eprintln!("xtask receipt create: --out is required");
                return ExitCode::from(2);
            };
            let root = workspace_root();
            let out = if out.is_absolute() {
                out
            } else {
                root.join(out)
            };
            let (registry, toolchain) = match evidence_digests(&root) {
                Ok(digests) => digests,
                Err(error) => {
                    eprintln!("xtask receipt create: {error}");
                    return ExitCode::FAILURE;
                }
            };
            let tasks = CHECK_DAG.iter().map(|task| task.name).collect::<Vec<_>>();
            match evidence::create_receipt(&root, &out, &registry, &toolchain, &tasks) {
                Ok(()) => {
                    println!("wrote check receipt {}", out.display());
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("xtask receipt create: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        "list" => {
            for task in CHECK_DAG {
                println!("{} <- {}", task.name, task.dependencies.join(", "));
            }
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!(
                "usage: cargo xtask check [--profile impact|full] [--base REV] [--explain] [--timings-json PATH] [-j N]\n       cargo xtask receipt create --out PATH\n       cargo xtask list"
            );
            ExitCode::from(2)
        }
    }
}

fn run_check(
    jobs: usize,
    requested_profile: CheckProfile,
    explicit_base: Option<&str>,
    explain: bool,
    timings_json: Option<&Path>,
) -> ExitCode {
    let root = workspace_root();
    let Some(_lock) = WorktreeLock::acquire(&root) else {
        return ExitCode::FAILURE;
    };
    let canonical = root.canonicalize().unwrap_or_else(|_| root.clone());
    let token = format!("{}-{}", std::process::id(), monotonic_token());
    let all = CHECK_DAG
        .iter()
        .map(|task| task.name)
        .collect::<BTreeSet<_>>();
    let mut effective_profile = "full";
    let mut evidence_base = None;
    let selected = if requested_profile == CheckProfile::Full {
        all.clone()
    } else {
        match evidence_digests(&root).and_then(|(registry, toolchain)| {
            let names = CHECK_DAG.iter().map(|task| task.name).collect::<Vec<_>>();
            evidence::verified_impact_decision(&root, explicit_base, &registry, &toolchain, &names)
        }) {
            Ok(decision) => {
                effective_profile = "impact";
                eprintln!(
                    "xtask: verified base receipt {} ({} changed paths, {} selected tasks)",
                    decision.base,
                    decision.changed_paths.len(),
                    decision.selected.len()
                );
                if explain {
                    for path in &decision.changed_paths {
                        eprintln!("xtask: IMPACT path {path}");
                    }
                    for (name, reasons) in &decision.reasons {
                        eprintln!(
                            "xtask: SELECT {name} <- {}",
                            reasons.iter().cloned().collect::<Vec<_>>().join(", ")
                        );
                    }
                }
                evidence_base = Some(decision.base);
                all.iter()
                    .copied()
                    .filter(|name| decision.selected.contains(*name))
                    .collect()
            }
            Err(error) => {
                effective_profile = "full-fallback";
                eprintln!("xtask: impact receipt unavailable ({error}); running full profile");
                all.clone()
            }
        }
    };

    let mut pending = selected.clone();
    let mut running: BTreeMap<&str, (Child, Instant)> = BTreeMap::new();
    let mut passed = all.difference(&selected).copied().collect::<BTreeSet<_>>();
    let mut failed = BTreeSet::new();
    let mut timings: BTreeMap<&str, (&str, u128)> = BTreeMap::new();
    for name in &passed {
        eprintln!(
            "xtask: REUSE {name} (verified base {})",
            evidence_base.as_deref().unwrap_or("receipt")
        );
        timings.insert(name, ("reused", 0));
    }

    while !pending.is_empty() || !running.is_empty() {
        let blocked = pending
            .iter()
            .copied()
            .filter(|name| {
                let task = task(name);
                task.dependencies
                    .iter()
                    .any(|dependency| failed.contains(dependency))
            })
            .collect::<Vec<_>>();
        for name in blocked {
            pending.remove(name);
            failed.insert(name);
            timings.insert(name, ("skipped", 0));
            eprintln!("xtask: SKIP {name} (dependency failed)");
        }

        let ready = pending
            .iter()
            .copied()
            .filter(|name| {
                task(name)
                    .dependencies
                    .iter()
                    .all(|dependency| passed.contains(dependency))
            })
            .take(jobs.saturating_sub(running.len()))
            .collect::<Vec<_>>();
        for name in ready {
            pending.remove(name);
            let spec = task(name);
            eprintln!("xtask: START {name}");
            let child = Command::new("make")
                .arg(spec.target)
                .current_dir(&root)
                .env(LOCK_ROOT_ENV, &canonical)
                .env(LOCK_TOKEN_ENV, &token)
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn();
            match child {
                Ok(child) => {
                    running.insert(name, (child, Instant::now()));
                }
                Err(e) => {
                    eprintln!("xtask: FAIL {name}: cannot spawn make: {e}");
                    failed.insert(name);
                    timings.insert(name, ("failed", 0));
                }
            }
        }

        let mut finished = Vec::new();
        for (&name, (child, started)) in &mut running {
            match child.try_wait() {
                Ok(Some(status)) => {
                    finished.push((name, status.success(), started.elapsed().as_millis()));
                }
                Ok(None) => {}
                Err(e) => {
                    eprintln!("xtask: FAIL {name}: wait error: {e}");
                    finished.push((name, false, started.elapsed().as_millis()));
                }
            }
        }
        for (name, success, elapsed_ms) in finished {
            running.remove(name);
            if success {
                passed.insert(name);
                timings.insert(name, ("passed", elapsed_ms));
                eprintln!("xtask: PASS {name}");
            } else {
                failed.insert(name);
                timings.insert(name, ("failed", elapsed_ms));
                eprintln!("xtask: FAIL {name}");
            }
        }
        if !running.is_empty() {
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    let succeeded = failed.is_empty();
    if let Some(path) = timings_json
        && let Err(error) = write_timings(
            &root,
            path,
            effective_profile,
            evidence_base.as_deref(),
            &timings,
            succeeded,
        )
    {
        eprintln!("xtask: write timings: {error}");
        return ExitCode::FAILURE;
    }
    if succeeded {
        println!("all checks passed ({effective_profile}; Docker-free, Java-free)");
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "xtask: failed tasks: {}",
            failed.into_iter().collect::<Vec<_>>().join(", ")
        );
        ExitCode::FAILURE
    }
}

fn write_timings(
    root: &Path,
    path: &Path,
    profile: &str,
    base: Option<&str>,
    timings: &BTreeMap<&str, (&str, u128)>,
    succeeded: bool,
) -> gmeow_errors::Result<()> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| evidence::failure(format!("create {}: {error}", parent.display())))?;
    }
    let mut body = format!(
        "{{\n  \"schema\": \"gmeow-check-timings-v1\",\n  \"profile\": \"{profile}\",\n  \"base\": {},\n  \"succeeded\": {succeeded},\n  \"tasks\": [\n",
        base.map_or_else(|| "null".to_owned(), |base| format!("\"{base}\""))
    );
    for (index, (name, (status, elapsed_ms))) in timings.iter().enumerate() {
        if index > 0 {
            body.push_str(",\n");
        }
        body.push_str(&format!(
            "    {{\"name\": \"{name}\", \"status\": \"{status}\", \"elapsed_ms\": {elapsed_ms}}}"
        ));
    }
    body.push_str("\n  ]\n}\n");
    std::fs::write(&path, body)
        .map_err(|error| evidence::failure(format!("write {}: {error}", path.display())))
}

fn evidence_digests(root: &Path) -> gmeow_errors::Result<(String, String)> {
    let mut registry = String::new();
    for task in CHECK_DAG {
        registry.push_str(task.name);
        registry.push('\0');
        registry.push_str(task.target);
        registry.push('\0');
        registry.push_str(&task.dependencies.join(","));
        registry.push('\n');
    }
    Ok((
        evidence::hash_registry(root, &registry)?,
        evidence::digest_files(root, TOOLCHAIN_RECEIPT_FILES)?,
    ))
}

fn task(name: &str) -> &'static Task {
    CHECK_DAG
        .iter()
        .find(|task| task.name == name)
        .expect("DAG task exists")
}

fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .expect("xtask is compiled under <workspace>/crates/xtask")
}

fn monotonic_token() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}
