// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! A small, dependency-free DAG runner for the full host gate. It owns the
//! worktree lock, runs synchronization once, then schedules independent gates
//! concurrently without imposing a thread cap on any child tool.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::time::Duration;

const LOCK_ROOT_ENV: &str = "GMEOW_TASK_LOCK_ROOT";
const LOCK_TOKEN_ENV: &str = "GMEOW_TASK_LOCK_TOKEN";

#[derive(Clone, Copy)]
struct Task {
    name: &'static str,
    target: &'static str,
    dependencies: &'static [&'static str],
}

const ROOT: &[&str] = &[];
const AFTER_SYNC: &[&str] = &["sync"];
const AFTER_REASON: &[&str] = &["reason-verify"];
const FINAL_DEPS: &[&str] = &[
    "check-lint",
    "rust-gate",
    "gts-frame-profile-gate",
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
        target: "sync",
        dependencies: ROOT,
    },
    Task {
        name: "check-lint",
        target: "check-lint",
        dependencies: AFTER_SYNC,
    },
    Task {
        name: "rust-gate",
        target: "rust-gate",
        dependencies: AFTER_SYNC,
    },
    Task {
        name: "gts-frame-profile-gate",
        target: "gts-frame-profile-gate",
        dependencies: AFTER_SYNC,
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
            while let Some(arg) = args.next() {
                if arg == "-j" || arg == "--jobs" {
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
                } else {
                    eprintln!("xtask check: unknown argument {arg:?}");
                    return ExitCode::from(2);
                }
            }
            run_check(jobs)
        }
        "list" => {
            for task in CHECK_DAG {
                println!("{} <- {}", task.name, task.dependencies.join(", "));
            }
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("usage: cargo xtask check [-j N]\n       cargo xtask list");
            ExitCode::from(2)
        }
    }
}

fn run_check(jobs: usize) -> ExitCode {
    let root = workspace_root();
    let Some(_lock) = WorktreeLock::acquire(&root) else {
        return ExitCode::FAILURE;
    };
    let canonical = root.canonicalize().unwrap_or_else(|_| root.clone());
    let token = format!("{}-{}", std::process::id(), monotonic_token());
    let mut pending = CHECK_DAG
        .iter()
        .map(|task| task.name)
        .collect::<BTreeSet<_>>();
    let mut running: BTreeMap<&str, Child> = BTreeMap::new();
    let mut passed = BTreeSet::new();
    let mut failed = BTreeSet::new();

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
                    running.insert(name, child);
                }
                Err(e) => {
                    eprintln!("xtask: FAIL {name}: cannot spawn make: {e}");
                    failed.insert(name);
                }
            }
        }

        let mut finished = Vec::new();
        for (&name, child) in &mut running {
            match child.try_wait() {
                Ok(Some(status)) => finished.push((name, status.success())),
                Ok(None) => {}
                Err(e) => {
                    eprintln!("xtask: FAIL {name}: wait error: {e}");
                    finished.push((name, false));
                }
            }
        }
        for (name, success) in finished {
            running.remove(name);
            if success {
                passed.insert(name);
                eprintln!("xtask: PASS {name}");
            } else {
                failed.insert(name);
                eprintln!("xtask: FAIL {name}");
            }
        }
        if !running.is_empty() {
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    if failed.is_empty() {
        println!("all checks passed (Docker-free, Java-free)");
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "xtask: failed tasks: {}",
            failed.into_iter().collect::<Vec<_>>().join(", ")
        );
        ExitCode::FAILURE
    }
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
