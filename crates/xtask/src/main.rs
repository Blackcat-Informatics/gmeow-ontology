// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! A small, dependency-free DAG runner for the full host gate. It owns the
//! HOST-GLOBAL gate lock (`make check` and a standalone `make check-sync` mutually
//! exclude across every worktree on the machine — see [`host_lock_path`]), and
//! schedules the gate's tasks
//! concurrently under their ACCURATE dependencies, without imposing a thread cap on
//! any child tool.
//!
//! # Dependency doctrine
//!
//! A task depends on `sync` if and only if it READS a `generated/` artifact. Every
//! edge in [`CHECK_DAG`] is justified in a comment naming the exact read. Tasks that
//! only read authored sources (`slices/`, `crates/`, `docs/`, `i18n/`, `shapes/`)
//! are [`ROOT`] and start immediately, concurrently with the synchronization pass
//! itself.

mod evidence;

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, Metadata, OpenOptions, TryLockError};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

const LOCK_ROOT_ENV: &str = "GMEOW_TASK_LOCK_ROOT";
const LOCK_TOKEN_ENV: &str = "GMEOW_TASK_LOCK_TOKEN";

/// The HOST-GLOBAL gate-lock path. `make check` and the single producer target
/// (`make check-sync`, which runs `gmeow-dev sync`) take THIS lock, so at most ONE
/// GMEOW gate runs on the entire host at a time — regardless of worktree —
/// and concurrent gates in sibling worktrees can never interfere (shared build/target
/// contention, disk pressure, or a mid-flight bundle regeneration another worktree
/// reads).
///
/// There is deliberately NO override. A gate takes every CPU on the box by design, so
/// two concurrent gates are slower in aggregate than two serialized ones, and a gate
/// starved of CPU goes red on timing rather than on content — which teaches everyone to
/// re-run reds instead of reading them. On a shared machine this lock is the QUEUE, and
/// an escape hatch from it is a way for one worktree to take the host from everybody
/// else. CI needs no hatch either: a runner is single-tenant, so it acquires this lock
/// uncontended. Kept byte-identical to the `gmeow-dev` copy so both agree.
///
/// # Why `/var/tmp`, not `/tmp`
///
/// The lock must be (a) one file for the whole host — every user, every worktree — and
/// (b) durable for the whole life of a gate run. `/tmp` satisfies (a) but not (b): on
/// this platform it is a tmpfs, and a tmpfs clear (or a `systemd-tmpfiles` age-out
/// under a multi-hour run) deletes the lock file out from under a LIVE holder, after
/// which a second gate creates a fresh inode and both run — exactly the interference
/// this lock exists to prevent. `/var/tmp` is the POSIX durable sibling: world-writable
/// and sticky on every supported host (so (a) is unchanged), disk-backed and preserved
/// across reboots (so (b) holds), and outside every worktree — so neither the
/// checkout-reset daemon nor `git clean` can remove it. The inode-identity check in
/// [`HostGateLock::acquire`] closes the residual window even if the file IS swapped.
fn host_lock_path() -> PathBuf {
    PathBuf::from("/var/tmp/gmeow-task/host-runner.lock")
}

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

#[derive(Clone, Copy)]
struct Task {
    name: &'static str,
    target: &'static str,
    dependencies: &'static [&'static str],
}

/// No dependency: the task reads only authored sources, so it starts in the first
/// scheduling wave — concurrently with `sync` itself.
const ROOT: &[&str] = &[];

/// The task reads a `generated/` artifact and therefore cannot start until the
/// synchronization pass has materialized it.
const AFTER_SYNC: &[&str] = &["sync"];

/// The task needs compiled workspace test binaries. `rust-build` itself depends on
/// `sync` (the consumer crates' `build.rs` embeds `generated/dist/gmeow.gts`), so this
/// edge transitively carries the generated-tree dependency as well.
const AFTER_RUST_BUILD: &[&str] = &["rust-build"];

/// Corpus fixtures are produced by an explicit DAG node after the test binaries exist.
/// Test runners depend on this node but never invoke it themselves.
const AFTER_TEST_FIXTURES: &[&str] = &["test-fixtures"];

const FINAL_DEPS: &[&str] = &[
    "check-lint",
    "crate-check",
    "i18n-lint",
    "rust-build",
    "test-fixtures",
    "clippy",
    "nextest",
    "doctests",
    "validate",
    "medium-gate",
    "constitution-check",
    "audit",
    "wikidata",
    "coverage",
    "reason-verify",
    "console-test",
    "console",
    "lint-alignment",
    "doc-lint",
    "slice-quality-gate",
];

/// The local `make check` gate.
///
/// Every `AFTER_SYNC` edge below names the exact `generated/` read that forces it;
/// every `ROOT` task was verified to read authored sources only. The breadth-dominated
/// lanes (`acceptance`, `wasm-parity`, `console-smoke`, `bench-soak`) live in `make heavy`,
/// not here.
const CHECK_DAG: &[Task] = &[
    // The producer. Materializes `generated/` (bundle + fanout) from authored sources.
    Task {
        name: "sync",
        target: "check-sync",
        dependencies: ROOT,
    },
    // pre-commit hygiene over the git-tracked tree. `/generated/` is gitignored in
    // full (zero tracked files), so no hook can see a generated artifact.
    Task {
        name: "check-lint",
        target: "check-lint",
        dependencies: ROOT,
    },
    // Crate layering + repo-static source scans over `crates/`, `slices/`, `dsl/`,
    // the in-memory docs-loss lattice, and the vendored-asset attestations under
    // `crates/docs/assets/`. No `generated/` read.
    Task {
        name: "crate-check",
        target: "crate-check",
        dependencies: ROOT,
    },
    // `i18n_compile::lint_po_files` walks `slices/**/*.po` plus authored TTL only.
    Task {
        name: "i18n-lint",
        target: "i18n-lint",
        dependencies: ROOT,
    },
    // `crates/gmeow-cli/build.rs` and `crates/lsp/build.rs` resolve
    // `generated/dist/gmeow.gts` and hard-fail when it is absent, so compiling the
    // workspace requires the materialized bundle.
    Task {
        name: "rust-build",
        target: "rust-build",
        dependencies: AFTER_SYNC,
    },
    // The sole pre-test fixture producer. Its separately compiled executables publish
    // exact test-profile action receipts and the generated-bundle import product. Every
    // test-facing loader is read-only and fails if this node did not complete.
    Task {
        name: "test-fixtures",
        target: "produce-test-fixtures",
        dependencies: AFTER_RUST_BUILD,
    },
    Task {
        name: "clippy",
        target: "clippy",
        dependencies: AFTER_RUST_BUILD,
    },
    Task {
        name: "nextest",
        target: "nextest",
        dependencies: AFTER_TEST_FIXTURES,
    },
    Task {
        name: "doctests",
        target: "doctests",
        dependencies: AFTER_RUST_BUILD,
    },
    // Two post-sync reads. (1) The shape union
    // (`purrdf::shapes::shape_union::load_shapes` via `ValidateOptions::shape_union_root`)
    // includes `generated/shapes/*.ttl` and fails closed when that directory is empty.
    // (2) The whole-corpus merged-SHACL verdict is CONSUMED from `stage-validate`'s
    // `generated/diagnostics/shacl.json` rather than recomputed; `gmeow-dev validate`
    // hard-fails unless that record's `shaclInputDigest` matches the authored sources
    // plus the committed shape union as they stand on disk.
    Task {
        name: "validate",
        target: "validate",
        dependencies: AFTER_SYNC,
    },
    // The MEDIUM axis's gate over the MATERIALIZED bundle. It is its own task rather
    // than a clause of `validate` because it audits the artifact's WIRE — the codec
    // catalog, the in-band dictionary table, every frame's decode, every envelope's
    // digests — which the ontology validation lane never reads.
    Task {
        name: "medium-gate",
        target: "medium-gate",
        dependencies: AFTER_SYNC,
    },
    // `governance/constitution.ttl` cites `generated/` artifacts (shapes, shacl-af,
    // metadata, the crosscheck report, the cost ledger); `constitution::check_references`
    // raises `stale-artifact` for every cited path that does not exist on disk.
    Task {
        name: "constitution-check",
        target: "constitution-check",
        dependencies: AFTER_SYNC,
    },
    // `scoreboards::claim_audit` -> `shapes_turtle`, which unions `generated/shapes`
    // (fail-closed when empty) and requires the generated core-prefix set.
    Task {
        name: "audit",
        target: "audit",
        dependencies: AFTER_SYNC,
    },
    // `mapping_eval::wikidata_mapping_syntax(root/"generated/mappings")`.
    Task {
        name: "wikidata",
        target: "wikidata",
        dependencies: AFTER_SYNC,
    },
    // `coverage::run_coverage(.., root/"generated/mappings", ..)`.
    Task {
        name: "coverage",
        target: "coverage",
        dependencies: AFTER_SYNC,
    },
    // Reads the `generated/dist/gmeow.gts` snapshot and re-derives its
    // `graph/reasoning` projection.
    Task {
        name: "reason-verify",
        target: "reason-verify",
        dependencies: AFTER_SYNC,
    },
    // The standalone console's DOM-free acceptance lane. It drives the SHIPPED wasm
    // engine over the SHIPPED bundle, so it depends on the synchronized tree; its
    // assertions (the derived pane set, the recorded round-trip failure, the hard error
    // on a missing asset, the quoted-triple annotations, the conjecture selector, the
    // wasm export subset) are gate blockers, not a smoke test. It stays on `make check`
    // — unlike `wasm-parity`, it builds nothing: it executes the already-vendored
    // `crates/docs/assets/mcp-core/` bytes, so its cost tracks the change under test.
    Task {
        name: "console-test",
        target: "console-test",
        dependencies: AFTER_SYNC,
    },
    // Assemble the standalone console deterministically on the local gate. The focused
    // DOM-free `console-test` above exercises the shipped wasm bytes against the synchronized
    // bundle. The 41-case browser/package sweep (`console-smoke`) is breadth-dominated — it
    // drives the whole read surface, offline and perturbed trees, and a real installed npm
    // tarball — so it runs on every PR as its own `make heavy` matrix branch instead of
    // extending every local edit's critical path.
    Task {
        name: "console",
        target: "console",
        dependencies: AFTER_SYNC,
    },
    // `correspondence_soundness` audits `generated/mappings/*.sssom.tsv`,
    // `generated/projections/*.edoal.ttl`, and the generated FnO catalog.
    Task {
        name: "lint-alignment",
        target: "lint-alignment",
        dependencies: AFTER_SYNC,
    },
    // The documentation model and the rendered English site come from the
    // bounded content-addressed `.cache/gmeow-sync/actions/` store (the model half in
    // `gmeow_docs_model::fixture`, the rendered half in `gmeow_docs::fixture`), whose key
    // folds `generated/catalog/constraint-catalog.nq` and
    // `generated/catalog/term-content-manifest.nq` — the same two files
    // `DocsModel::discover` reads, and which it HARD-fails without.
    Task {
        name: "doc-lint",
        target: "doc-lint",
        dependencies: AFTER_SYNC,
    },
    // The gate's committed floors/ceilings are projected from the ontology-resident
    // rubric (`gmeow_slice_quality::load_repo_rubric` over the slices' authored
    // `module.ttl`), NOT from a `generated/` file —
    // `generated/governance/slice-quality-axis-floors.tsv` is only echoed as a human
    // pointer inside a per-axis floor violation message and is never read. The forcing
    // read is now the RECORDED grade vector: the gate loads
    // `generated/quality/gmeow.quality-assessment.nt` (`stage-source-load`'s scoring
    // sweep, projected) instead of re-scoring all 84 slices, and hard-fails unless that
    // record's `gmeow:versionFingerprint` matches the authored sources on disk.
    Task {
        name: "slice-quality-gate",
        target: "slice-quality-gate",
        dependencies: AFTER_SYNC,
    },
    Task {
        name: "compliance-report",
        target: "compliance-report",
        dependencies: FINAL_DEPS,
    },
];

/// Whether `pid` names a live process on this host.
fn pid_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

/// The `pid=` field of a lock owner record, when present and parseable.
fn record_pid(record: &str) -> Option<u32> {
    record
        .split_whitespace()
        .find_map(|field| field.strip_prefix("pid="))
        .and_then(|pid| pid.parse().ok())
}

fn same_file(a: &Metadata, b: &Metadata) -> bool {
    a.dev() == b.dev() && a.ino() == b.ino()
}

fn open_rw(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
}

/// Open the host gate-lock file read-write, reaping a provably stale one.
///
/// The lock directory is sticky + world-writable so a gate started by ANY user on the
/// host contends on the SAME file. The failure mode that needs reaping is a leftover
/// file whose permissions deny `open(O_RDWR)` to everyone but its (long-dead) creator:
/// nobody can take that lock and nobody can release it, so the gate is bricked for the
/// whole host. When the open fails AND the file's recorded owner pid is readable AND
/// that pid is not alive, the record is provably stale — unlink and recreate.
///
/// The reap is deliberately narrow. A live recorded owner is never reaped, and an
/// UNREADABLE record is never reaped either: without a readable owner we cannot prove
/// staleness, and unlinking a file a live process holds an `flock` on would let two
/// gates run at once. That case is reported with explicit remediation instead.
fn open_lock_file(path: &Path) -> Option<File> {
    if let Some(dir) = path.parent() {
        if let Err(error) = std::fs::create_dir_all(dir) {
            eprintln!("xtask: create {}: {error}", dir.display());
            return None;
        }
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o1777));
    }
    let open_error = match open_rw(path) {
        Ok(file) => {
            // World-writable so a cross-user holder can still record its owner line;
            // the flock below is the actual host-wide mutual-exclusion guarantee.
            let _ = file.set_permissions(std::fs::Permissions::from_mode(0o0666));
            return Some(file);
        }
        Err(error) => error,
    };
    let Ok(record) = std::fs::read_to_string(path) else {
        eprintln!(
            "xtask: cannot open host gate lock {}: {open_error}; its owner record is also \
             unreadable, so staleness cannot be proven. Remove the file by hand once no GMEOW \
             gate is running on this host.",
            path.display()
        );
        return None;
    };
    match record_pid(&record) {
        Some(pid) if pid_alive(pid) => {
            eprintln!(
                "xtask: cannot open host gate lock {}: {open_error}; it is held by live pid {pid} \
                 ({})",
                path.display(),
                record.trim()
            );
            return None;
        }
        Some(pid) => {
            if let Err(error) = std::fs::remove_file(path) {
                eprintln!(
                    "xtask: host gate lock {} names dead owner pid {pid} but cannot be reaped: \
                     {error}",
                    path.display()
                );
                return None;
            }
            eprintln!(
                "xtask: reaped a stale host gate lock at {} (recorded owner pid {pid} is not \
                 alive)",
                path.display()
            );
        }
        None => {
            eprintln!(
                "xtask: cannot open host gate lock {}: {open_error}; its owner record names no \
                 pid ({:?}), so staleness cannot be proven. Remove the file by hand once no \
                 GMEOW gate is running on this host.",
                path.display(),
                record.trim()
            );
            return None;
        }
    }
    match open_rw(path) {
        Ok(file) => {
            let _ = file.set_permissions(std::fs::Permissions::from_mode(0o0666));
            Some(file)
        }
        Err(error) => {
            eprintln!(
                "xtask: open host gate lock {} after reap: {error}",
                path.display()
            );
            None
        }
    }
}

struct HostGateLock {
    file: File,
}

impl HostGateLock {
    fn acquire(root: &Path) -> Option<Self> {
        let path = host_lock_path();
        // Bounded retry. Each attempt either wins the `flock` on the file that is
        // CURRENTLY at `path` — proven by comparing the held descriptor's (dev, ino)
        // against a fresh stat of the path — or discovers the file was swapped
        // underneath it (a reap by a sibling gate, or an out-of-band `rm`) and starts
        // over. Without this identity check a swap would let the swapper and the
        // previous holder both believe they own the host.
        for _ in 0..3 {
            let mut file = open_lock_file(&path)?;
            match file.try_lock() {
                Ok(()) => {
                    let held = file.metadata().ok();
                    let current = std::fs::metadata(&path).ok();
                    let identical = match (&held, &current) {
                        (Some(held), Some(current)) => same_file(held, current),
                        _ => false,
                    };
                    if !identical {
                        let _ = file.unlock();
                        continue;
                    }
                    let owner = format!(
                        "pid={} purpose=check root={}\n",
                        std::process::id(),
                        root.display()
                    );
                    // Owner line is diagnostic only (a cross-user pre-existing file may
                    // deny the write); the flock is what makes the gate host-atomic, so
                    // a failed owner write must NOT drop the lock.
                    let _ = file
                        .set_len(0)
                        .and_then(|()| file.seek(SeekFrom::Start(0)).map(|_| ()))
                        .and_then(|()| file.write_all(owner.as_bytes()))
                        .and_then(|()| file.flush());
                    return Some(Self { file });
                }
                Err(TryLockError::WouldBlock) => {
                    let mut owner = String::new();
                    let _ = file.seek(SeekFrom::Start(0));
                    let _ = file.read_to_string(&mut owner);
                    eprintln!(
                        "xtask: another GMEOW gate is already running on this host: {}",
                        owner.trim()
                    );
                    if record_pid(&owner).is_some_and(|pid| !pid_alive(pid)) {
                        eprintln!(
                            "xtask: the recorded owner pid is no longer alive, but the lock IS \
                             held: the kernel releases a dead process's flock, so this is a stale \
                             RECORD written by a holder that could not update it — not a stale \
                             lock. Waiting is correct; reclaiming would run two gates at once."
                        );
                    }
                    return None;
                }
                Err(TryLockError::Error(error)) => {
                    eprintln!("xtask: acquire host gate lock: {error}");
                    return None;
                }
            }
        }
        eprintln!(
            "xtask: host gate lock {} was replaced repeatedly while acquiring it; refusing to run \
             rather than risk two concurrent gates",
            path.display()
        );
        None
    }
}

impl Drop for HostGateLock {
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
            // `--explain` is a DRY RUN: it prints the schedule and exits without
            // spawning a single child. It therefore does NOT take the host gate lock —
            // taking the machine's one gate slot to print a plan is pure queue theft.
            if explain {
                explain_plan(jobs);
                return ExitCode::SUCCESS;
            }
            run_check(jobs, timings_json.as_deref())
        }
        // Read-only: hashes the task registry and toolchain files and writes a receipt.
        // It runs no gate task, so it takes no host gate lock.
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
        // Read-only: prints the DAG. Takes no host gate lock.
        "list" => {
            for task in CHECK_DAG {
                println!("{} <- {}", task.name, task.dependencies.join(", "));
            }
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!(
                "usage: cargo xtask check [--explain] [--timings-json PATH] [-j N]\n       cargo xtask receipt create --out PATH\n       cargo xtask list"
            );
            ExitCode::from(2)
        }
    }
}

/// The scheduling waves of [`CHECK_DAG`]: wave 0 is every task with no unmet
/// dependency, wave N+1 is every task whose dependencies all land in waves 0..=N.
/// This is the shape the runner actually executes, so it is also the shape the
/// critical path is read off.
fn plan_waves() -> Vec<Vec<&'static str>> {
    let mut placed: BTreeSet<&'static str> = BTreeSet::new();
    let mut waves = Vec::new();
    while placed.len() < CHECK_DAG.len() {
        let wave = CHECK_DAG
            .iter()
            .filter(|task| !placed.contains(task.name))
            .filter(|task| {
                task.dependencies
                    .iter()
                    .all(|dependency| placed.contains(dependency))
            })
            .map(|task| task.name)
            .collect::<Vec<_>>();
        assert!(!wave.is_empty(), "CHECK_DAG has a dependency cycle");
        placed.extend(wave.iter().copied());
        waves.push(wave);
    }
    waves
}

fn explain_plan(jobs: usize) {
    println!(
        "xtask check plan ({} tasks, up to {jobs} concurrent)",
        CHECK_DAG.len()
    );
    for (index, wave) in plan_waves().into_iter().enumerate() {
        println!("  wave {index}: {}", wave.join(", "));
    }
    println!("(dry run: no host gate lock taken, no task executed)");
}

fn run_check(jobs: usize, timings_json: Option<&Path>) -> ExitCode {
    let root = workspace_root();
    let Some(_lock) = HostGateLock::acquire(&root) else {
        return ExitCode::FAILURE;
    };
    let canonical = root.canonicalize().unwrap_or_else(|_| root.clone());
    let token = format!("{}-{}", std::process::id(), monotonic_token());
    let mut pending = CHECK_DAG
        .iter()
        .map(|task| task.name)
        .collect::<BTreeSet<_>>();
    let mut running: BTreeMap<&str, (Child, Instant)> = BTreeMap::new();
    let mut passed = BTreeSet::new();
    let mut failed = BTreeSet::new();
    let mut timings: BTreeMap<&str, (&str, u128)> = BTreeMap::new();

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
        && let Err(error) = write_timings(&root, path, &timings, succeeded)
    {
        eprintln!("xtask: write timings: {error}");
        return ExitCode::FAILURE;
    }
    if succeeded {
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

fn write_timings(
    root: &Path,
    path: &Path,
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
        "{{\n  \"schema\": \"gmeow-check-timings-v1\",\n  \"succeeded\": {succeeded},\n  \"tasks\": [\n"
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_dependency_names_a_real_task() {
        let names = CHECK_DAG
            .iter()
            .map(|task| task.name)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            names.len(),
            CHECK_DAG.len(),
            "CHECK_DAG repeats a task name"
        );
        for task in CHECK_DAG {
            for dependency in task.dependencies {
                assert!(
                    names.contains(dependency),
                    "{} depends on unknown task {dependency}",
                    task.name
                );
            }
        }
    }

    #[test]
    fn the_plan_is_acyclic_and_covers_every_task() {
        let waves = plan_waves();
        let scheduled = waves.iter().flatten().copied().collect::<BTreeSet<_>>();
        assert_eq!(scheduled.len(), CHECK_DAG.len());
    }

    /// `sync` is the gate's longest single stage, so it must NOT be a blanket
    /// prerequisite. Every task that reads only authored sources starts in wave 0,
    /// concurrently with `sync` itself.
    #[test]
    fn sync_is_not_a_blanket_prerequisite() {
        let wave_zero = plan_waves()
            .first()
            .expect("the plan has at least one wave")
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        for name in ["sync", "check-lint", "crate-check", "i18n-lint"] {
            assert!(
                wave_zero.contains(name),
                "{name} reads no generated/ artifact and must start immediately"
            );
        }
    }

    /// The monolithic `rust-gate` node is split. Non-corpus lanes remain siblings
    /// under `rust-build`; the corpus-consuming runner waits on the explicit producer.
    #[test]
    fn the_rust_lanes_are_independent_siblings() {
        for name in ["clippy", "doctests"] {
            assert_eq!(
                task(name).dependencies,
                AFTER_RUST_BUILD,
                "{name} must depend on rust-build and nothing else"
            );
        }
        assert_eq!(task("test-fixtures").dependencies, AFTER_RUST_BUILD);
        assert_eq!(task("nextest").dependencies, AFTER_TEST_FIXTURES);
        assert!(
            !CHECK_DAG
                .iter()
                .any(|task| matches!(task.name, "carrier-purity" | "coherence-gate-teeth")),
            "carrier/coherence proofs must run inside the one nextest inventory"
        );
        assert!(
            !CHECK_DAG.iter().any(|task| task.name == "rust-gate"),
            "the monolithic rust-gate node must not be scheduled alongside its parts"
        );
    }

    /// The breadth-dominated lanes belong to `make heavy`, not the per-commit gate.
    #[test]
    fn the_heavy_lanes_are_not_scheduled_by_check() {
        for name in ["acceptance", "wasm-parity", "console-smoke", "bench-soak"] {
            assert!(
                !CHECK_DAG.iter().any(|task| task.name == name),
                "{name} moved to `make heavy` and must not reappear in CHECK_DAG"
            );
        }
    }

    #[test]
    fn the_final_task_waits_for_every_other_task() {
        let expected = CHECK_DAG
            .iter()
            .map(|task| task.name)
            .filter(|name| *name != "compliance-report" && *name != "sync")
            .collect::<BTreeSet<_>>();
        let declared = FINAL_DEPS.iter().copied().collect::<BTreeSet<_>>();
        assert_eq!(
            declared, expected,
            "compliance-report must wait for every other gate task (sync is transitive)"
        );
    }

    #[test]
    fn the_host_lock_lives_on_durable_shared_storage() {
        let path = host_lock_path();
        assert!(
            path.starts_with("/var/tmp"),
            "the host gate lock must live on durable, host-shared storage: {}",
            path.display()
        );
        assert!(
            !path.starts_with("/tmp/"),
            "the host gate lock must not live on tmpfs"
        );
    }

    #[test]
    fn owner_records_round_trip_their_pid() {
        assert_eq!(record_pid("pid=1234 purpose=check root=/x\n"), Some(1234));
        assert_eq!(record_pid("purpose=check root=/x"), None);
        assert_eq!(record_pid(""), None);
        assert!(pid_alive(std::process::id()));
        // pid 0 is never a userspace process on Linux.
        assert!(!pid_alive(0));
    }

    /// Every task's `target` is a real Makefile RULE. The scheduler spawns
    /// `make <target>`; an undeclared target fails the child, which at least reports —
    /// but a target that exists only as a `.PHONY` entry with no rule would "succeed"
    /// with `make: Nothing to be done`, so the RULE line is what is asserted here.
    #[test]
    fn every_task_target_has_a_makefile_rule() {
        let makefile = std::fs::read_to_string(workspace_root().join("Makefile"))
            .expect("the workspace Makefile is readable");
        let rules: BTreeSet<&str> = makefile
            .lines()
            .filter(|line| !line.starts_with(['\t', ' ', '#']))
            .filter_map(|line| line.split_once(':'))
            .filter(|(name, rest)| {
                !name.is_empty()
                    && !rest.starts_with('=')
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            })
            .map(|(name, _)| name)
            .collect();
        let missing = CHECK_DAG
            .iter()
            .map(|task| task.target)
            .filter(|target| !rules.contains(target))
            .collect::<Vec<_>>();
        assert!(
            missing.is_empty(),
            "these CHECK_DAG targets have no Makefile rule: {missing:?}"
        );
    }
}
