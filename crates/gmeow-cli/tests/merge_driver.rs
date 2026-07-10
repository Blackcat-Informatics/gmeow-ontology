// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Merge-driver bootstrap process test — the bundle is a merge fixed point.
//!
//! Faithful native port of the retired `tests/test_git_merge_driver.py`. Drives
//! REAL `git` via `std::process::Command` inside a fresh, throwaway
//! `tempfile::TempDir`, running the REAL `scripts/bootstrap-git-merge-drivers.sh`
//! and copying the REAL repo `.gitattributes` — no mocking, no faked git state.
//!
//! Pins two invariants:
//! 1. The bootstrap script actually configures `merge.ours.driver=true` locally
//!    (the guard `git rev-parse --is-inside-work-tree` must pass inside a real
//!    `git init` repo, so the script does real work rather than early-exiting).
//! 2. With `.gitattributes` marking `generated/dist/gmeow.gts` as `merge=ours`
//!    and the driver configured, a genuine conflicting three-way merge between
//!    two branches that both touched the bundle resolves cleanly to the current
//!    branch's content — the bundle is a fixed point under merge, never a
//!    conflict, never a combination of the two sides.

use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Output};

/// The repo root (this crate's manifest dir, two levels up) — a real git
/// worktree, used to locate the real bootstrap script and `.gitattributes`.
fn repo_root() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    path.canonicalize()
        .unwrap_or_else(|e| panic!("canonicalize repo root {}: {e}", path.display()))
}

/// Run `git <args>` in `repo`, returning the full `Output` (never panics on a
/// non-zero exit — callers assert what they need).
fn git(repo: &Path, args: &[&str]) -> Output {
    StdCommand::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap_or_else(|e| panic!("run git {args:?} in {}: {e}", repo.display()))
}

/// Run `git <args>` in `repo` and panic with full stdout/stderr if it did not
/// exit 0 — for setup steps that must succeed for the test to be meaningful.
fn git_ok(repo: &Path, args: &[&str]) {
    let output = git(repo, args);
    assert!(
        output.status.success(),
        "git {args:?} in {} failed: status={:?}\nstdout={}\nstderr={}",
        repo.display(),
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// `git init -b main` plus the local identity config later commits need.
fn init_repo(repo: &Path) {
    git_ok(repo, &["init", "-b", "main"]);
    git_ok(
        repo,
        &["config", "--local", "user.email", "agent@example.invalid"],
    );
    git_ok(
        repo,
        &["config", "--local", "user.name", "GMEOW Agent Test"],
    );
}

/// Run the real repo's bootstrap script with `cwd` = `repo`.
fn run_bootstrap(repo: &Path) -> Output {
    let script = repo_root().join("scripts/bootstrap-git-merge-drivers.sh");
    assert!(
        script.is_file(),
        "bootstrap script must exist at {}",
        script.display()
    );
    StdCommand::new("bash")
        .arg(&script)
        .current_dir(repo)
        .output()
        .unwrap_or_else(|e| panic!("run bootstrap script in {}: {e}", repo.display()))
}

/// `git config --local --get <key>` in `repo`, trimmed. Panics if the key is
/// unset (a missing config is a test failure, not an empty-string result).
fn git_config_get(repo: &Path, key: &str) -> String {
    let output = git(repo, &["config", "--local", "--get", key]);
    assert!(
        output.status.success(),
        "git config --local --get {key} in {} must succeed (key must be set): stderr={}",
        repo.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

#[test]
fn bootstrap_configures_ours_merge_driver() {
    let dir = tempfile::TempDir::new().expect("create temp dir");
    let repo = dir.path();
    init_repo(repo);

    let output = run_bootstrap(repo);
    assert!(
        output.status.success(),
        "bootstrap script must exit 0 inside a real git worktree: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let driver = git_config_get(repo, "merge.ours.driver");
    assert_eq!(
        driver, "true",
        "merge.ours.driver must be configured to the no-op 'true' driver"
    );

    let name = git_config_get(repo, "merge.ours.name");
    assert!(
        name.contains("generated binary artifacts"),
        "merge.ours.name must describe the generated-binary-artifacts intent, got: {name:?}"
    );
}

#[test]
fn generated_bundle_merge_keeps_current_side() {
    let dir = tempfile::TempDir::new().expect("create temp dir");
    let repo = dir.path();
    init_repo(repo);

    // Copy the REAL repo's .gitattributes so `generated/dist/gmeow.gts` is
    // genuinely marked `merge=ours` in this throwaway repo too.
    let real_gitattributes = repo_root().join(".gitattributes");
    assert!(
        real_gitattributes.is_file(),
        ".gitattributes must exist at {}",
        real_gitattributes.display()
    );
    std::fs::copy(&real_gitattributes, repo.join(".gitattributes"))
        .expect("copy .gitattributes into temp repo");

    let bundle_rel = Path::new("generated/dist/gmeow.gts");
    let bundle_abs = repo.join(bundle_rel);
    std::fs::create_dir_all(bundle_abs.parent().expect("parent dir")).expect("mkdir -p");
    std::fs::write(&bundle_abs, b"base").expect("write base bundle");

    git_ok(repo, &["add", "."]);
    git_ok(repo, &["commit", "-m", "base"]);

    // Configure the `ours` driver locally via the real bootstrap script.
    let bootstrap_output = run_bootstrap(repo);
    assert!(
        bootstrap_output.status.success(),
        "bootstrap script must succeed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&bootstrap_output.stdout),
        String::from_utf8_lossy(&bootstrap_output.stderr)
    );

    git_ok(repo, &["switch", "-c", "side"]);
    std::fs::write(&bundle_abs, b"side").expect("write side bundle");
    git_ok(repo, &["commit", "-am", "side"]);

    git_ok(repo, &["switch", "main"]);
    std::fs::write(&bundle_abs, b"main").expect("write main bundle");
    git_ok(repo, &["commit", "-am", "main"]);

    let merge_output = git(repo, &["merge", "side"]);
    assert!(
        merge_output.status.success(),
        "merge of conflicting bundle edits must auto-resolve via the ours driver: \
         status={:?}\nstdout={}\nstderr={}",
        merge_output.status,
        String::from_utf8_lossy(&merge_output.stdout),
        String::from_utf8_lossy(&merge_output.stderr)
    );

    let merged_bytes = std::fs::read(&bundle_abs).expect("read merged bundle");
    assert_eq!(
        merged_bytes, b"main",
        "the ours driver must keep the current (main) side's content, not side's and not a \
         combination of the two"
    );

    let status_output = git(repo, &["status", "--porcelain"]);
    assert!(
        status_output.status.success(),
        "git status --porcelain must succeed: stderr={}",
        String::from_utf8_lossy(&status_output.stderr)
    );
    let status_text = String::from_utf8_lossy(&status_output.stdout);
    assert!(
        status_text.trim().is_empty(),
        "working tree must be fully clean after the merge (no lingering conflict markers or \
         unstaged state), got: {status_text:?}"
    );
}
