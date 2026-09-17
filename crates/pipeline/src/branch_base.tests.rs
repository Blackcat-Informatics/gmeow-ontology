// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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
