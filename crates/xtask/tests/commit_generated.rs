// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Behavioural gate for `scripts/commit-generated.sh`.
//!
//! `make commit` materializes through the pipeline's single producer
//! (`check-sync` in update mode) and then hands off to this script to list
//! generator-owned paths, stage whichever exist, and commit. The script
//! previously lived inline in the `commit` recipe body; it moved out so the
//! recipe stays within checkmake's target-body length limit. This file proves
//! the extracted script's BEHAVIOUR, and that the recipe never interpolates the
//! commit message into shell text.
//!
//! Each test runs the script against a disposable scratch git repository (never
//! the real worktree) so `git add`/`git commit` are safe to actually execute,
//! and against a fake `gmeow-dev` stand-in (never the real, expensive binary) so
//! `sync --list-paths` is fully controlled per test.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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

/// A disposable git repository, isolated from the real worktree, used to
/// exercise the script's `git add`/`git diff`/`git commit` calls for real.
///
/// The scratch root is a `tempfile::TempDir`: its suffix comes from the OS
/// CSPRNG rather than a guessable pid, it is created with `mkdir` (so it can
/// never be resolved through a pre-planted symlink), and it is removed even
/// while unwinding from a failed assertion.
struct ScratchRepo {
    dir: PathBuf,
    _tmp: tempfile::TempDir,
}

impl ScratchRepo {
    fn new(label: &str) -> Self {
        let tmp = tempfile::Builder::new()
            .prefix(&format!("gmeow-commit-generated-test-{label}-"))
            .tempdir()
            .expect("create scratch repo dir");
        let dir = tmp.path().to_path_buf();
        let repo = Self { dir, _tmp: tmp };
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

/// Proves the fix for the Makefile-level shell-interpolation finding at the
/// point that actually matters: MESSAGE is now handed to the script through
/// the process ENVIRONMENT (Make's target-specific `export`), exactly like
/// `run_script`/`Command::env` do here — never through recipe text a shell
/// re-parses. A MESSAGE containing a double quote, semicolon, and `#` must
/// therefore commit VERBATIM, and the embedded `touch` must never run.
#[test]
fn message_with_shell_metacharacters_lands_verbatim_and_does_not_execute() {
    let repo = ScratchRepo::new("shell-metacharacter-message");
    repo.write_file("generated/output.txt", "generated content\n");
    let gmeow_dev = repo.fake_gmeow_dev(&["generated/output.txt"]);
    let marker_home = tempfile::tempdir().expect("create marker dir");
    let marker = marker_home.path().join("pwned-proof");
    let payload = format!(r#""; touch {}; #"#, marker.display());
    let out = run_script(
        repo.path(),
        &[
            ("GMEOW_DEV", gmeow_dev.to_str().unwrap()),
            ("MESSAGE", payload.as_str()),
        ],
    );
    let marker_existed = marker.exists();
    let _ = fs::remove_file(&marker);
    assert!(
        out.status.success(),
        "a hostile MESSAGE must not break the commit: stdout={} stderr={}",
        stdout_of(&out),
        stderr_of(&out)
    );
    assert!(
        !marker_existed,
        "the embedded `touch` must never execute — the script must treat \
         MESSAGE as inert data, not shell text"
    );
    assert_eq!(
        repo.log().trim(),
        payload,
        "the commit message must carry MESSAGE byte-for-byte, including its \
         shell metacharacters"
    );
}

/// Proves the fix for the "pre-staged sweep" finding: a caller who already
/// has unrelated work staged for a different commit must never have that
/// work silently folded into a commit labelled as generated-artifact sync.
#[test]
fn pre_staged_unrelated_changes_are_rejected_not_swept_in() {
    let repo = ScratchRepo::new("pre-staged-unrelated");
    repo.write_file("generated/output.txt", "generated content\n");
    repo.write_file("unrelated-work-in-progress.txt", "not generator-owned\n");
    repo.git(&["add", "unrelated-work-in-progress.txt"]);
    let gmeow_dev = repo.fake_gmeow_dev(&["generated/output.txt"]);
    let out = run_script(
        repo.path(),
        &[
            ("GMEOW_DEV", gmeow_dev.to_str().unwrap()),
            ("MESSAGE", "chore: synchronize checked-in artifacts"),
        ],
    );
    assert!(
        !out.status.success(),
        "pre-staged unrelated changes must be rejected, not silently swept in: \
         stdout={} stderr={}",
        stdout_of(&out),
        stderr_of(&out)
    );
    assert!(
        stderr_of(&out).contains("unrelated-work-in-progress.txt"),
        "the failure must name the offending pre-staged path; got stderr: {}",
        stderr_of(&out)
    );
    assert!(
        repo.log().is_empty(),
        "no commit should have been created when refusing"
    );
    assert!(
        !repo.cached_diff_is_empty(),
        "the caller's pre-staged file must remain staged, untouched, for the \
         caller to commit separately"
    );
}

#[test]
fn the_makefile_commit_target_delegates_to_the_extracted_script() {
    let makefile = std::fs::read_to_string(repo_root().join("Makefile")).expect("Makefile");
    // `MESSAGE`/`GMEOW_DEV` must reach the script through the process ENVIRONMENT
    // (target-specific `export`), never through recipe TEXT: Make expands
    // `$(MESSAGE)` textually before the recipe's shell parses the line, so a
    // MESSAGE containing a shell metacharacter (quote, backtick, `;`, `$(...)`)
    // previously broke out of the quoting and executed. The recipe line itself
    // must therefore invoke the script with no inline `VAR="..."` assignment at
    // all — see `message_with_shell_metacharacters_lands_verbatim_and_does_not_execute`
    // and `dry_run_never_places_message_text_in_recipe_output` for the exploit
    // regression proofs.
    assert!(
        makefile.contains("commit: export GMEOW_DEV := $(GMEOW_DEV)"),
        "the commit target must export GMEOW_DEV via a target-specific variable, \
         not interpolate it into recipe text; Makefile was: {makefile:?}"
    );
    assert!(
        makefile.contains("commit: export MESSAGE := $(MESSAGE)"),
        "the commit target must export MESSAGE via a target-specific variable, \
         not interpolate it into recipe text; Makefile was: {makefile:?}"
    );
    assert!(
        makefile.contains("\n\t@scripts/commit-generated.sh"),
        "the commit recipe must invoke the script directly with no inline \
         VAR=\"...\" assignment (that round-trip through the shell's quoting is \
         exactly the injection this wiring closes); Makefile was: {makefile:?}"
    );
    assert!(
        !makefile.contains(r#"MESSAGE="$(MESSAGE)""#),
        "no recipe line may textually interpolate $(MESSAGE) into shell-parsed \
         text anywhere in the Makefile"
    );
}

#[test]
fn dry_run_never_places_message_text_in_recipe_output() {
    // `-n` still resolves the real `$(MAKE) check-sync` sub-make recursion (GNU
    // make always executes recipe lines that reference $(MAKE), even under -n)
    // but that sub-make ALSO inherits -n via MAKEFLAGS, so nothing is ever
    // actually run — see `make_commit_still_resolves` for the same property. This
    // makes it safe to drive with a real Makefile invocation rather than a
    // hand-rolled stand-in, and to assert on the exact text Make would hand to
    // a shell if this were a live run.
    let payload = r#""; touch /tmp/pwned-dry-run-proof; #"#;
    let out = Command::new("make")
        .arg("-n")
        .arg("commit")
        .arg(format!("MESSAGE={payload}"))
        .arg("GMEOW_DEV=/bin/true")
        .current_dir(repo_root())
        .env_remove("CI")
        .output()
        .expect("make is part of this repository's toolchain");
    assert!(
        out.status.success(),
        "a hostile MESSAGE must not break recipe resolution: stdout={} stderr={}",
        stdout_of(&out),
        stderr_of(&out)
    );
    let stdout = stdout_of(&out);
    assert!(
        !stdout.contains("touch /tmp/pwned"),
        "the payload text must never appear in the recipe output Make would \
         hand to a shell — it must travel only through the exported \
         environment; got stdout: {stdout}"
    );
    assert!(
        !Path::new("/tmp/pwned-dry-run-proof").exists(),
        "a dry run must never actually execute anything"
    );
}

#[test]
fn make_commit_still_resolves() {
    // The end-to-end regression: `make commit` is a documented workflow and must
    // survive the extraction. `-n` still executes `$(MAKE)` lines, so this
    // exercises the real sub-make recursion into the single producer,
    // `check-sync` — never the poisoned `regen`, which refuses unconditionally.
    let out = Command::new("make")
        .arg("-n")
        .arg("commit")
        .current_dir(repo_root())
        .env_remove("CI")
        .output()
        .expect("make is part of this repository's toolchain");
    assert!(
        out.status.success(),
        "`make commit` must resolve through the single producer: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
