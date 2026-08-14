// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The BRANCH-VERSUS-MERGE-BASE comparand, and the one predicate that decides whether an
//! absent comparand is a legitimate skip or a hard failure.
//!
//! Several gates in this workspace are defined as a COMPARISON against `git merge-base
//! HEAD origin/main` — the slice-quality floor/ceiling ratchets in `gmeow-dev-cli`, and
//! the model-facing freeze legs in `crates/pipeline/tests`. All of them need exactly the
//! same three answers, and two copies of them would be two notions of "what this branch
//! is being compared to":
//!
//! * [`resolve_base_ref`] — the base commit, in a TRI-STATE that separates "there is no
//!   upstream in this checkout" from "the comparand could not be obtained";
//! * [`git_show_base`] — one file's bytes at that commit, separating a genuinely-absent
//!   path from any other git failure;
//! * [`git_ls_tree`] — which files existed there at all, so a comparand set is the BASE
//!   tree's rather than the working tree's;
//! * [`ci_declared`] — whether the run declares itself an automated one, which is what
//!   turns a bare-clone skip into unfinished work.
//!
//! Every call is LOCAL: `rev-parse`, `merge-base`, `show` and `ls-tree` never touch the
//! network, so a gate can resolve its comparand offline. CI builds the PR merged into
//! `main`, so `origin/main` is present there by construction.

use std::path::Path;

/// The merge-base comparand a branch-versus-base gate is defined against.
#[derive(Debug, Clone)]
pub enum BaseRef {
    /// The resolved merge-base commit the working state is compared against.
    Resolved(String),
    /// `origin/main` genuinely does not exist as a ref in this checkout — the only case
    /// where "no prior committed state is reachable" is expected rather than broken. A
    /// LOUD skip in a bare local clone, and unfinished work anywhere [`ci_declared`]
    /// holds.
    NoUpstream(String),
    /// `origin/main` exists (or ref existence could not be checked) but the comparand
    /// could not be obtained. HARD FAIL: the gate cannot perform the comparison it is
    /// defined to perform, and passing there would let a regression through unseen.
    Unresolvable(String),
}

/// Resolve `git merge-base HEAD origin/main` in `root` (local, no network).
///
/// A clone that never fetched `origin/main` yields [`BaseRef::NoUpstream`]; any other
/// failure to resolve the merge base yields [`BaseRef::Unresolvable`].
#[must_use]
pub fn resolve_base_ref(root: &Path) -> BaseRef {
    match std::process::Command::new("git")
        .current_dir(root)
        .env("LC_ALL", "C")
        .args(["rev-parse", "--verify", "--quiet", "origin/main"])
        .output()
    {
        Ok(out) if out.status.success() => {}
        Ok(_) => {
            return BaseRef::NoUpstream(
                "`origin/main` does not exist as a ref in this checkout (no upstream fetched)"
                    .to_owned(),
            );
        }
        Err(err) => return BaseRef::Unresolvable(format!("could not run git: {err}")),
    }
    match std::process::Command::new("git")
        .current_dir(root)
        .env("LC_ALL", "C")
        .args(["merge-base", "HEAD", "origin/main"])
        .output()
    {
        Ok(out) if out.status.success() => {
            let sha = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            if sha.is_empty() {
                BaseRef::Unresolvable(
                    "`git merge-base HEAD origin/main` resolved no commit".to_owned(),
                )
            } else {
                BaseRef::Resolved(sha)
            }
        }
        Ok(out) => BaseRef::Unresolvable(format!(
            "`git merge-base HEAD origin/main` failed ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        )),
        Err(err) => BaseRef::Unresolvable(format!("could not run git: {err}")),
    }
}

/// The outcome of reading one file at the merge base.
#[derive(Debug, Clone)]
pub enum BaseFile {
    /// The blob contents at the base commit.
    Contents(String),
    /// The path did not exist at the base — a brand-new file, which cannot have regressed
    /// because there was nothing to regress from.
    Absent,
    /// `git show` failed for any reason OTHER than an absent path — a HARD FAIL.
    Error(String),
}

/// Read `<base>:<rel>` in `root` via `git show` (local, no network).
///
/// A path-absent error is distinguished from any other git failure by git's well-known
/// fatal messages, so a genuinely new file is a skip while a bad object or a broken repo
/// is a hard fail.
#[must_use]
pub fn git_show_base(root: &Path, base: &str, rel: &str) -> BaseFile {
    let spec = format!("{base}:{rel}");
    match std::process::Command::new("git")
        .current_dir(root)
        .env("LC_ALL", "C")
        .args(["show", &spec])
        .output()
    {
        Ok(out) if out.status.success() => {
            BaseFile::Contents(String::from_utf8_lossy(&out.stdout).into_owned())
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("does not exist in") || stderr.contains("exists on disk, but not in")
            {
                BaseFile::Absent
            } else {
                BaseFile::Error(format!(
                    "`git show {spec}` failed ({}): {}",
                    out.status,
                    stderr.trim()
                ))
            }
        }
        Err(err) => BaseFile::Error(format!("could not run `git show {spec}`: {err}")),
    }
}

/// The outcome of listing part of the merge-base tree.
#[derive(Debug, Clone)]
pub enum BaseTree {
    /// The repo-relative paths, exactly as `git ls-tree` reported them.
    Paths(Vec<String>),
    /// git could not answer the question — a HARD FAIL.
    Error(String),
}

/// List every path under `pathspecs` at `base` (local, no network).
///
/// `git ls-tree` does not error on a pathspec that matches nothing, so a directory that is
/// genuinely absent at `base` (a brand-new tree) yields an empty list with a SUCCESSFUL
/// exit. A non-zero exit means git itself could not answer and is [`BaseTree::Error`],
/// never a silent "nothing there" — the two are indistinguishable to a caller that reads
/// both as an empty set, and one of them hides a regression.
#[must_use]
pub fn git_ls_tree(root: &Path, base: &str, pathspecs: &[&str]) -> BaseTree {
    let mut args: Vec<&str> = vec!["ls-tree", "-r", "--name-only", base, "--"];
    args.extend_from_slice(pathspecs);
    let out = match std::process::Command::new("git")
        .current_dir(root)
        .env("LC_ALL", "C")
        .args(&args)
        .output()
    {
        Ok(out) => out,
        Err(err) => {
            return BaseTree::Error(format!(
                "could not run `git ls-tree {base} -- {}`: {err}",
                pathspecs.join(" ")
            ));
        }
    };
    if !out.status.success() {
        return BaseTree::Error(format!(
            "`git ls-tree {base} -- {}` failed ({}): {}",
            pathspecs.join(" "),
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    BaseTree::Paths(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}

/// Whether this run DECLARES itself an automated one, by the truthiness of `CI`.
///
/// The single reading of that variable in the workspace. An automated run is one where
/// nobody watches a skip go by, so it is the exact condition under which
/// [`BaseRef::NoUpstream`] stops being a legitimate bare-clone skip and becomes a gate
/// that passed by looking at nothing.
///
/// `CI` is treated as a boolean whose false spellings are the shell's: unset, empty, `0`,
/// `false`, `off`, `no` (ASCII-case-insensitive, surrounding whitespace ignored).
#[must_use]
pub fn ci_declared() -> bool {
    std::env::var("CI").is_ok_and(|value| is_true(&value))
}

/// The truthiness of one `CI` spelling, split out of [`ci_declared`] so the table below
/// is pinned by a PURE function — reading it out of the ambient environment inside a test
/// would make the assertion depend on the machine the suite happens to run on.
fn is_true(value: &str) -> bool {
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "0" | "false" | "off" | "no"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The truthiness table, pinned in both directions: a gate that read `CI=0` as "this
    /// is CI" would hard-fail a developer's bare clone, and one that read `CI=1` as "not
    /// CI" would let the skip it is meant to forbid straight back in.
    #[test]
    fn the_ci_truthiness_table_is_the_shell_s() {
        for value in ["", "0", "false", "FALSE", " off ", "no", " NO "] {
            assert!(!is_true(value), "CI={value:?} must not read as a CI run");
        }
        for value in ["1", "true", "TRUE", "yes", "on", "github"] {
            assert!(is_true(value), "CI={value:?} must read as a CI run");
        }
    }

    #[test]
    fn an_absent_path_at_the_base_is_an_absence_rather_than_an_error() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .expect("workspace root");
        let BaseRef::Resolved(base) = resolve_base_ref(&root) else {
            println!("SKIP: no origin/main in this checkout");
            return;
        };
        assert!(
            matches!(
                git_show_base(&root, &base, "no/such/path/at/the/base.txt"),
                BaseFile::Absent
            ),
            "a path git reports as missing must read as Absent, never as an Error"
        );
        assert!(
            matches!(
                git_show_base(&root, &base, "Cargo.toml"),
                BaseFile::Contents(_)
            ),
            "the workspace manifest predates every merge base"
        );
    }
}
