// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Behavioural gate for `scripts/commit-generated.sh`.
//!
//! `make commit` regenerates (via a sub-make, so the regen-guard never fires —
//! see the comment in the Makefile's `commit` recipe) and then hands off to this
//! script to list generator-owned paths, stage whichever exist, and commit. The
//! script previously lived inline in the `commit` recipe body; it moved out so
//! the recipe stays within checkmake's target-body length limit. This file
//! proves the extracted script's BEHAVIOUR is unchanged, mirroring the approach
//! `crates/xtask/tests/regen_guard.rs` takes for `scripts/regen-guard.sh`.
//!
//! Each test runs the script against a disposable scratch git repository (never
//! the real worktree) so `git add`/`git commit` are safe to actually execute,
//! and against a fake `gmeow-dev` stand-in (never the real, expensive binary) so
//! `sync --list-paths` is fully controlled per test.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

/// The repository root: this crate is `<root>/crates/xtask`.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/xtask has a grandparent")
        .to_path_buf()
}

fn script_path() -> PathBuf {
    repo_root().join("scripts/commit-generated.sh")
}

static SCRATCH_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A disposable git repository, isolated from the real worktree, used to
/// exercise the script's `git add`/`git diff`/`git commit` calls for real.
struct ScratchRepo {
    dir: PathBuf,
}

impl ScratchRepo {
    fn new(label: &str) -> Self {
        let n = SCRATCH_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "gmeow-commit-generated-test-{label}-{}-{n}",
            std::process::id()
        ));
        // A stale directory from a killed prior run must not corrupt this run.
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch repo dir");
        let repo = Self { dir };
        repo.git(&["init", "-q"]);
        repo.git(&[
            "config",
            "user.email",
            "commit-generated-test@example.invalid",
        ]);
        repo.git(&["config", "user.name", "commit-generated-test"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo
    }

    fn git(&self, args: &[&str]) -> Output {
        let out = Command::new("git")
            .args(args)
            .current_dir(&self.dir)
            .output()
            .expect("git is part of this repository's toolchain");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        out
    }

    fn path(&self) -> &Path {
        &self.dir
    }

    /// Write a file at `rel` (relative to the repo root), creating parents.
    fn write_file(&self, rel: &str, contents: &str) {
        let full = self.dir.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("create parent dirs");
        }
        fs::write(&full, contents).expect("write scratch file");
    }

    /// Install a fake `gmeow-dev` stand-in whose `sync --list-paths` prints
    /// exactly `paths`, one per line, regardless of the arguments it is called
    /// with. Returns the path to hand to the script as `GMEOW_DEV`.
    fn fake_gmeow_dev(&self, paths: &[&str]) -> PathBuf {
        let tools_dir = self.dir.join(".fake-tools");
        fs::create_dir_all(&tools_dir).expect("create fake tool dir");
        let list_file = tools_dir.join("listed-paths.txt");
        fs::write(&list_file, paths.join("\n")).expect("write fake path list");
        let script = tools_dir.join("fake-gmeow-dev.sh");
        fs::write(
            &script,
            "#!/usr/bin/env bash\nset -euo pipefail\ndir=\"$(cd \"$(dirname \"$0\")\" && pwd)\"\ncat \"$dir/listed-paths.txt\"\n",
        )
        .expect("write fake gmeow-dev");
        let mut perms = fs::metadata(&script)
            .expect("stat fake gmeow-dev")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("chmod fake gmeow-dev");
        script
    }

    /// Commit subject lines, oldest first. Returns an empty string on a
    /// HEAD-less repo (no commits yet) rather than treating that as an error —
    /// `git log` itself fails closed there, but "no commits" is a valid state
    /// this suite deliberately exercises (the nothing-to-commit case).
    fn log(&self) -> String {
        let out = Command::new("git")
            .args(["log", "--format=%s"])
            .current_dir(&self.dir)
            .output()
            .expect("git is part of this repository's toolchain");
        if !out.status.success() {
            return String::new();
        }
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn cached_diff_is_empty(&self) -> bool {
        Command::new("git")
            .args(["diff", "--cached", "--quiet"])
            .current_dir(&self.dir)
            .status()
            .expect("git diff runs")
            .success()
    }
}

impl Drop for ScratchRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Run the script with a FULLY CONTROLLED environment: `GMEOW_DEV` and
/// `MESSAGE` are cleared first, so ambient environment state cannot mask a
/// regression in either required-input case.
fn run_script(dir: &Path, vars: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(script_path());
    cmd.current_dir(dir);
    for key in ["GMEOW_DEV", "MESSAGE"] {
        cmd.env_remove(key);
    }
    for (key, value) in vars {
        cmd.env(key, value);
    }
    cmd.output().expect("the script is executable")
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn missing_gmeow_dev_fails_with_a_clear_message() {
    let repo = ScratchRepo::new("missing-gmeow-dev");
    let out = run_script(repo.path(), &[("MESSAGE", "chore: test")]);
    assert!(
        !out.status.success(),
        "an unset GMEOW_DEV must fail closed, not open"
    );
    assert!(
        stderr_of(&out).contains("GMEOW_DEV"),
        "the failure must name the missing input; got: {}",
        stderr_of(&out)
    );
}

#[test]
fn missing_message_fails_with_a_clear_message() {
    let repo = ScratchRepo::new("missing-message");
    let gmeow_dev = repo.fake_gmeow_dev(&[]);
    let out = run_script(repo.path(), &[("GMEOW_DEV", gmeow_dev.to_str().unwrap())]);
    assert!(
        !out.status.success(),
        "an unset MESSAGE must fail closed, not open"
    );
    assert!(
        stderr_of(&out).contains("MESSAGE"),
        "the failure must name the missing input; got: {}",
        stderr_of(&out)
    );
}

#[test]
fn nothing_to_commit_exits_non_zero_with_the_expected_message() {
    let repo = ScratchRepo::new("nothing-to-commit");
    // No listed paths at all, and nothing else staged: the index has no diff
    // against HEAD (even a HEAD-less repo), so the script must refuse.
    let gmeow_dev = repo.fake_gmeow_dev(&[]);
    let out = run_script(
        repo.path(),
        &[
            ("GMEOW_DEV", gmeow_dev.to_str().unwrap()),
            ("MESSAGE", "chore: test"),
        ],
    );
    assert!(
        !out.status.success(),
        "nothing staged must exit non-zero: stdout={} stderr={}",
        stdout_of(&out),
        stderr_of(&out)
    );
    assert!(
        stdout_of(&out).contains("Nothing to commit."),
        "got stdout: {}",
        stdout_of(&out)
    );
    assert!(repo.log().is_empty(), "no commit should have been created");
}

#[test]
fn a_listed_existing_path_is_staged_and_committed() {
    let repo = ScratchRepo::new("stage-and-commit");
    repo.write_file("generated/output.txt", "generated content\n");
    let gmeow_dev = repo.fake_gmeow_dev(&["generated/output.txt"]);
    let out = run_script(
        repo.path(),
        &[
            ("GMEOW_DEV", gmeow_dev.to_str().unwrap()),
            ("MESSAGE", "chore: synchronize checked-in artifacts"),
        ],
    );
    assert!(
        out.status.success(),
        "a real staged change must commit successfully: stdout={} stderr={}",
        stdout_of(&out),
        stderr_of(&out)
    );
    assert_eq!(
        repo.log().trim(),
        "chore: synchronize checked-in artifacts",
        "the commit must carry the given MESSAGE"
    );
    assert!(
        repo.cached_diff_is_empty(),
        "the listed path must actually be committed, leaving nothing staged"
    );
}

#[test]
fn a_listed_path_that_does_not_exist_is_skipped_not_errored() {
    let repo = ScratchRepo::new("skip-missing-path");
    repo.write_file("generated/output.txt", "generated content\n");
    // One real path and one that was listed but never materialized (e.g. an
    // output variant that legitimately did not regenerate this run).
    let gmeow_dev = repo.fake_gmeow_dev(&["generated/output.txt", "generated/does-not-exist.txt"]);
    let out = run_script(
        repo.path(),
        &[
            ("GMEOW_DEV", gmeow_dev.to_str().unwrap()),
            ("MESSAGE", "chore: synchronize checked-in artifacts"),
        ],
    );
    assert!(
        out.status.success(),
        "a missing listed path must be skipped, not treated as an error: stdout={} stderr={}",
        stdout_of(&out),
        stderr_of(&out)
    );
    assert_eq!(repo.log().trim(), "chore: synchronize checked-in artifacts");
}

#[test]
fn unstaged_changes_after_commit_print_a_warning_but_still_succeed() {
    let repo = ScratchRepo::new("warn-unstaged");
    repo.write_file("generated/output.txt", "generated content\n");
    repo.write_file("untouched.txt", "not generator-owned\n");
    repo.git(&["add", "untouched.txt"]);
    repo.git(&["commit", "-q", "-m", "seed"]);
    // Now dirty `untouched.txt` without listing it, so it stays unstaged while
    // the listed generated path is what gets committed.
    repo.write_file("untouched.txt", "modified out of band\n");
    let gmeow_dev = repo.fake_gmeow_dev(&["generated/output.txt"]);
    let out = run_script(
        repo.path(),
        &[
            ("GMEOW_DEV", gmeow_dev.to_str().unwrap()),
            ("MESSAGE", "chore: synchronize checked-in artifacts"),
        ],
    );
    assert!(
        out.status.success(),
        "leftover unstaged changes must warn, not fail: stdout={} stderr={}",
        stdout_of(&out),
        stderr_of(&out)
    );
    assert!(
        stdout_of(&out).contains("Warning: unstaged changes remain"),
        "got stdout: {}",
        stdout_of(&out)
    );
}

#[test]
fn the_makefile_commit_target_delegates_to_the_extracted_script() {
    let makefile = std::fs::read_to_string(repo_root().join("Makefile")).expect("Makefile");
    assert!(
        makefile.contains(
            r#"GMEOW_DEV="$(GMEOW_DEV)" MESSAGE="$(MESSAGE)" scripts/commit-generated.sh"#
        ),
        "the commit recipe must hand GMEOW_DEV and MESSAGE to the extracted script explicitly, \
         not re-derive them inside the script"
    );
}

#[test]
fn make_commit_still_resolves() {
    // The end-to-end regression: `make commit` is a documented workflow and must
    // survive the extraction. `-n` still executes `$(MAKE)` lines, so this
    // exercises the real sub-make recursion into `regen`.
    let out = Command::new("make")
        .arg("-n")
        .arg("commit")
        .current_dir(repo_root())
        .env_remove("CI")
        .env_remove("REGEN_ACK")
        .env_remove("REGEN_INTERNAL")
        .output()
        .expect("make is part of this repository's toolchain");
    assert!(
        out.status.success(),
        "`make commit` must not be blocked by the regen guard: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
