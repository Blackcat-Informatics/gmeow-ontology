// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Behavioural gate for `scripts/regen-guard.sh`.
//!
//! `make regen` runs no gate and costs a full pipeline pass, so a bare direct
//! invocation must HARD-FAIL while every legitimate caller passes silently. The
//! guard itself is shell — `make regen` is the clean-clone bootstrap path, where
//! no generated bundle exists yet and the consumer CLIs therefore cannot compile,
//! so a guard that must refuse in milliseconds cannot depend on `cargo build`.
//! Its behaviour is nonetheless proven here, in Rust, on the `rust` lane.
//!
//! The regression these tests exist for: the guard previously lived inline in the
//! `regen` recipe, where a line-continuation defect (`exit 1 \` with no `;`) made
//! bash read `exit 1 fi`, leaving the `if` unterminated. Because bash parses a
//! whole compound command before executing any of it, the recipe failed
//! UNCONDITIONALLY — including in CI, which invokes `make regen` directly — and
//! that single defect took every downstream CI job with it.

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

fn guard_path() -> PathBuf {
    repo_root().join("scripts/regen-guard.sh")
}

/// Run the guard with a FULLY CONTROLLED environment: every variable the guard
/// consults is cleared first, so an ambient `CI=true` (as on any CI runner)
/// cannot mask a regression in the direct-invocation case.
fn run_guard(vars: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(guard_path());
    cmd.current_dir(repo_root());
    for key in [
        "CI",
        "REGEN_ACK",
        "REGEN_INTERNAL",
        "GMEOW_MAKELEVEL",
        "MAKELEVEL",
    ] {
        cmd.env_remove(key);
    }
    for (key, value) in vars {
        cmd.env(key, value);
    }
    cmd.output().expect("the guard script is executable")
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn a_bare_direct_invocation_hard_fails_and_explains_itself() {
    let out = run_guard(&[("GMEOW_MAKELEVEL", "0")]);
    assert!(
        !out.status.success(),
        "a bare `make regen` MUST fail; the guard exited {:?}",
        out.status.code()
    );
    let stderr = stderr_of(&out);
    assert!(
        stderr.contains("only REGENERATES"),
        "the refusal must explain what regen does and does not do; got: {stderr}"
    );
    assert!(
        stderr.contains("REGEN_ACK=1"),
        "the refusal must advertise the deliberate escape, or it is a dead end; got: {stderr}"
    );
}

#[test]
fn an_unset_makelevel_is_treated_as_a_direct_invocation() {
    // Fail-closed: absence of the marker must not be read as "a sub-make said so".
    let out = run_guard(&[]);
    assert!(
        !out.status.success(),
        "an unset GMEOW_MAKELEVEL must fail closed, not open"
    );
}

#[test]
fn ci_passes_silently() {
    // GitHub Actions always sets CI=true, and the `generation` job invokes
    // `make regen` DIRECTLY at MAKELEVEL 0. A hard fail here breaks CI by
    // construction — which is exactly the outage this guard replaced.
    let out = run_guard(&[("GMEOW_MAKELEVEL", "0"), ("CI", "true")]);
    assert!(
        out.status.success(),
        "CI must not be blocked: {}",
        stderr_of(&out)
    );
    assert!(
        stderr_of(&out).is_empty(),
        "CI must not be spammed with the steering banner"
    );
}

#[test]
fn the_deliberate_human_escape_passes() {
    let out = run_guard(&[("GMEOW_MAKELEVEL", "0"), ("REGEN_ACK", "1")]);
    assert!(
        out.status.success(),
        "REGEN_ACK=1 must pass: {}",
        stderr_of(&out)
    );
}

#[test]
fn an_in_makefile_caller_passes() {
    let out = run_guard(&[("GMEOW_MAKELEVEL", "0"), ("REGEN_INTERNAL", "1")]);
    assert!(
        out.status.success(),
        "REGEN_INTERNAL=1 must pass: {}",
        stderr_of(&out)
    );
}

#[test]
fn a_sub_make_passes() {
    let out = run_guard(&[("GMEOW_MAKELEVEL", "1")]);
    assert!(
        out.status.success(),
        "install/build/docs/release recurse into regen via $(MAKE): {}",
        stderr_of(&out)
    );
}

#[test]
fn the_makefile_passes_makes_own_expansion_not_the_environment() {
    // GNU make exports MAKELEVEL to a recipe's children ALREADY INCREMENTED, so a
    // recipe of the top-level make sees MAKELEVEL=1 while `$(MAKELEVEL)` expands
    // to 0. Reading the environment variable directly would silently never fire
    // the guard — the recipe must hand over make's own expansion.
    let makefile = std::fs::read_to_string(repo_root().join("Makefile")).expect("Makefile");
    assert!(
        makefile.contains(r#"GMEOW_MAKELEVEL="$(MAKELEVEL)" scripts/regen-guard.sh"#),
        "the regen recipe must pass make's OWN MAKELEVEL expansion to the guard"
    );
}

#[test]
fn commit_does_not_take_regen_as_a_same_level_prerequisite() {
    // A prerequisite executes at the SAME MAKELEVEL as its target, so `commit: regen`
    // would trip the direct-invocation guard for every human running `make commit`.
    // Regeneration must recurse via $(MAKE), as install/release/docs already do.
    let makefile = std::fs::read_to_string(repo_root().join("Makefile")).expect("Makefile");
    let commit_line = makefile
        .lines()
        .find(|line| line.starts_with("commit:"))
        .expect("the Makefile declares a `commit` target");
    let target_and_prereqs = commit_line.split("##").next().unwrap_or(commit_line);
    assert!(
        !target_and_prereqs
            .split_whitespace()
            .any(|word| word == "regen"),
        "`commit` must not list `regen` as a prerequisite; found: {commit_line}"
    );
}

#[test]
fn make_commit_still_resolves() {
    // The end-to-end regression: `make commit` is a documented workflow and must
    // survive the guard. `-n` still executes `$(MAKE)` lines, so this exercises the
    // real sub-make recursion into `regen`.
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
