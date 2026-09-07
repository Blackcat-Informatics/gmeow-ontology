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

/// The outcome of resolving one revision to a commit sha.
///
/// A tri-state for the same reason [`BaseRef`] and [`BaseFile`] are: "this revision does not
/// name a commit here" (the root commit's `HEAD^`) is a different answer from "git could not
/// be asked", and collapsing them would let a broken checkout read as a legitimate absence.
enum GitRev {
    /// The revision resolved to this commit.
    Sha(String),
    /// The revision names no commit in this checkout.
    Missing,
    /// git could not be run, or failed for any other reason.
    Failed(String),
}

/// Resolve one revision to a commit sha in `root` (local, no network).
fn rev_parse(root: &Path, spec: &str) -> GitRev {
    match std::process::Command::new("git")
        .current_dir(root)
        .env("LC_ALL", "C")
        .args(["rev-parse", "--verify", "--quiet", spec])
        .output()
    {
        Ok(out) if out.status.success() => {
            let sha = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            if sha.is_empty() {
                GitRev::Missing
            } else {
                GitRev::Sha(sha)
            }
        }
        Ok(_) => GitRev::Missing,
        Err(err) => GitRev::Failed(format!("could not run git: {err}")),
    }
}

/// Whether the working tree carries anything uncommitted.
enum TreeState {
    /// Nothing uncommitted: tracked modifications and untracked files are both absent.
    Clean,
    /// Something uncommitted — that content is the delta a gate should grade.
    Dirty,
    /// git could not be asked.
    Failed(String),
}

/// Whether the working tree carries nothing uncommitted (tracked modifications OR untracked
/// files), which is what decides between the two "branch IS the base" readings below.
///
/// Untracked files count: a gate whose comparand set is built from `diff` PLUS `ls-files
/// --others` would otherwise call a tree carrying a brand-new file "clean" and step past the
/// very commit that file should be graded against.
fn working_tree_state(root: &Path) -> TreeState {
    match std::process::Command::new("git")
        .current_dir(root)
        .env("LC_ALL", "C")
        .args(["status", "--porcelain"])
        .output()
    {
        Ok(out) if out.status.success() => {
            if String::from_utf8_lossy(&out.stdout).trim().is_empty() {
                TreeState::Clean
            } else {
                TreeState::Dirty
            }
        }
        Ok(out) => TreeState::Failed(format!(
            "`git status --porcelain` failed ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        )),
        Err(err) => TreeState::Failed(format!("could not run git: {err}")),
    }
}

/// Resolve `git merge-base HEAD origin/main` in `root` (local, no network).
///
/// A clone that never fetched `origin/main` yields [`BaseRef::NoUpstream`]; any other
/// failure to resolve the merge base yields [`BaseRef::Unresolvable`].
///
/// # When the branch IS the base
///
/// On a PUSH to `main`, `HEAD` *is* `origin/main`, so the merge base is `HEAD` itself. What
/// the gate should compare against then depends on whether the working tree carries
/// anything, and the two cases are genuinely different comparisons rather than one rule:
///
/// * **Dirty tree** — the uncommitted changes ARE the delta being graded, so `HEAD` is the
///   right comparand and is returned unchanged. This is a developer editing on `main`, and
///   it is also how the ratchet fixtures are built: commit the base state, point
///   `origin/main` at it, then write the scenario into the working tree.
/// * **Clean tree** — there is nothing uncommitted, so comparing `HEAD` against itself
///   grades the empty set. That is not a legitimate skip and not a pass: it is the gate
///   looking at nothing, which is precisely what these gates exist to refuse. The honest
///   comparand is `HEAD`'s FIRST PARENT — on a squash-merged `main`, exactly the landed
///   commit's own diff.
///
/// A clean `HEAD` with no parent (the root commit) is the one case where no prior committed
/// state genuinely exists, and is reported as [`BaseRef::NoUpstream`] so [`ci_declared`]
/// decides its severity.
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
                return BaseRef::Unresolvable(
                    "`git merge-base HEAD origin/main` resolved no commit".to_owned(),
                );
            }
            // The branch has not diverged from upstream. If the working tree is dirty those
            // uncommitted changes are the delta, so HEAD stays the comparand; only when
            // there is nothing uncommitted does comparing HEAD to itself grade nothing, and
            // the first parent becomes the honest base. See the doc comment above.
            let head = match rev_parse(root, "HEAD") {
                GitRev::Sha(head) => head,
                GitRev::Missing => {
                    return BaseRef::Unresolvable(
                        "`HEAD` does not name a commit in this checkout".to_owned(),
                    );
                }
                GitRev::Failed(why) => return BaseRef::Unresolvable(why),
            };
            if sha != head {
                return BaseRef::Resolved(sha);
            }
            match working_tree_state(root) {
                TreeState::Dirty => return BaseRef::Resolved(sha),
                TreeState::Clean => {}
                TreeState::Failed(why) => return BaseRef::Unresolvable(why),
            }
            match rev_parse(root, "HEAD^") {
                GitRev::Sha(parent) => BaseRef::Resolved(parent),
                GitRev::Missing => BaseRef::NoUpstream(
                    "HEAD is the merge base AND has no parent (the root commit), so no prior \
                     committed state exists to compare against"
                        .to_owned(),
                ),
                GitRev::Failed(why) => BaseRef::Unresolvable(why),
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

    /// Run one git command in `dir`, asserting it succeeded.
    fn git_in(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .current_dir(dir)
            .env("LC_ALL", "C")
            .args(args)
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// A throwaway repository with identity configured and signing off, so the assertions
    /// below never depend on the developer's git config or key material.
    fn temp_repo() -> tempfile::TempDir {
        let td = tempfile::TempDir::new().expect("temp dir");
        git_in(td.path(), &["init", "--quiet"]);
        git_in(td.path(), &["config", "user.email", "t@example.invalid"]);
        git_in(td.path(), &["config", "user.name", "Test"]);
        git_in(td.path(), &["config", "commit.gpgsign", "false"]);
        td
    }

    /// Commit one file and return the new commit's sha.
    fn commit_file(dir: &Path, name: &str, body: &str) -> String {
        std::fs::write(dir.join(name), body).expect("write");
        git_in(dir, &["add", name]);
        git_in(dir, &["commit", "--quiet", "-m", name]);
        git_in(dir, &["rev-parse", "HEAD"])
    }

    /// Point `refs/remotes/origin/main` at a commit, standing in for a fetched upstream.
    fn set_origin_main(dir: &Path, sha: &str) {
        git_in(dir, &["update-ref", "refs/remotes/origin/main", sha]);
    }

    /// The ordinary branch case is untouched: a branch that has moved past its upstream
    /// still resolves to the merge base, so PR runs keep grading the branch's own diff.
    #[test]
    fn a_diverged_branch_resolves_to_the_merge_base() {
        let td = temp_repo();
        let root = td.path();
        let base = commit_file(root, "a.txt", "one");
        set_origin_main(root, &base);
        let head = commit_file(root, "b.txt", "two");
        assert_ne!(base, head);

        match resolve_base_ref(root) {
            BaseRef::Resolved(sha) => assert_eq!(
                sha, base,
                "a diverged branch is compared against the merge base"
            ),
            other => panic!("expected Resolved({base}), got {other:?}"),
        }
    }

    /// The regression this function exists to prevent: on a PUSH to `main`, HEAD *is*
    /// `origin/main`, so the merge base is HEAD and every branch-versus-base gate would
    /// compare HEAD against itself and grade the empty set. Stepping back to the first
    /// parent keeps the comparison real — the landed commit's own diff — which is what
    /// makes the gate meaningful on main instead of failing there by construction.
    #[test]
    fn a_push_to_main_resolves_to_the_first_parent_rather_than_grading_nothing() {
        let td = temp_repo();
        let root = td.path();
        let parent = commit_file(root, "a.txt", "one");
        let head = commit_file(root, "b.txt", "two");
        // HEAD is upstream: exactly the state of a push to `main`.
        set_origin_main(root, &head);

        match resolve_base_ref(root) {
            BaseRef::Resolved(sha) => {
                assert_eq!(sha, parent, "the comparand is HEAD's first parent");
                assert_ne!(sha, head, "the comparand is never HEAD itself");
            }
            other => panic!("expected Resolved({parent}), got {other:?}"),
        }

        // The point of the fix: the diff against that comparand is NOT empty, so a leg
        // asserting it has something to grade passes on main.
        let changed = git_in(root, &["diff", "--name-only", &parent]);
        assert_eq!(
            changed, "b.txt",
            "the landed commit's own diff is what main grades"
        );
    }

    /// The counterpart the first cut of this function got wrong: when HEAD is the base but
    /// the working tree is DIRTY, those uncommitted changes are the delta being graded, so
    /// HEAD must stay the comparand. This is a developer editing on `main`, and it is also
    /// exactly how the slice-quality ratchet fixtures are built — commit the base state,
    /// point `origin/main` at it, then write the scenario into the working tree. Stepping to
    /// the first parent there would compare against the wrong commit, and in a fixture whose
    /// base is the root commit it would report "no prior state" for a comparison that is
    /// perfectly well defined.
    #[test]
    fn a_dirty_tree_at_the_base_still_grades_against_head() {
        let td = temp_repo();
        let root = td.path();
        let base = commit_file(root, "a.txt", "one");
        set_origin_main(root, &base);
        // Uncommitted: the fixture pattern.
        std::fs::write(root.join("b.txt"), "two").expect("write");

        match resolve_base_ref(root) {
            BaseRef::Resolved(sha) => assert_eq!(
                sha, base,
                "a dirty tree is graded against HEAD, never its parent"
            ),
            other => panic!("expected Resolved({base}), got {other:?}"),
        }
    }

    /// The one case where no prior committed state genuinely exists. It is reported as
    /// NoUpstream rather than Unresolvable so `ci_declared` decides the severity, exactly
    /// as it does for a bare clone.
    #[test]
    fn a_root_commit_that_is_its_own_base_has_no_prior_state() {
        let td = temp_repo();
        let root = td.path();
        let only = commit_file(root, "a.txt", "one");
        set_origin_main(root, &only);

        match resolve_base_ref(root) {
            BaseRef::NoUpstream(why) => assert!(
                why.contains("root commit"),
                "the skip must name why no comparand exists: {why}"
            ),
            other => panic!("expected NoUpstream, got {other:?}"),
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
