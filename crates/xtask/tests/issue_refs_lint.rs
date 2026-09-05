// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Hermetic behavioral coverage for the authored-prose provenance policy.
//!
//! Each synthetic repository has a clean `origin/main` reference. Surface-specific
//! controls advance that reference past their fixture so only the named scanner can
//! catch it; the branch-added-line control deliberately leaves its fixture untracked.
//! No test uses network access or produces corpus data.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/xtask has a grandparent")
        .to_path_buf()
}

struct FixtureRepo {
    dir: tempfile::TempDir,
}

impl FixtureRepo {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        std::fs::create_dir(dir.path().join(".git-template"))
            .expect("the isolated Git template directory is creatable");
        git(dir.path(), &["init", "-q", "-b", "main"]);
        git(dir.path(), &["config", "user.name", "Lint Fixture"]);
        git(
            dir.path(),
            &["config", "user.email", "lint-fixture@example.invalid"],
        );
        git(dir.path(), &["config", "commit.gpgsign", "false"]);
        git(dir.path(), &["config", "tag.gpgsign", "false"]);
        std::fs::write(dir.path().join("README"), "clean baseline\n")
            .expect("baseline is writable");
        git(dir.path(), &["add", "README"]);
        git(dir.path(), &["commit", "-q", "-m", "baseline"]);
        git(
            dir.path(),
            &["update-ref", "refs/remotes/origin/main", "HEAD"],
        );
        Self { dir }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn write(&self, path: &str, body: &str) {
        let path = self.path().join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("fixture parent is creatable");
        }
        std::fs::write(path, body).expect("fixture is writable");
    }

    fn commit_as_origin(&self, path: &str) {
        git(self.path(), &["add", "--", path]);
        self.commit_index_as_origin();
    }

    fn commit_index_as_origin(&self) {
        git(self.path(), &["commit", "-q", "-m", "fixture"]);
        git(
            self.path(),
            &["update-ref", "refs/remotes/origin/main", "HEAD"],
        );
    }
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TEMPLATE_DIR", root.join(".git-template"))
        .env_remove("GIT_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_WORK_TREE")
        .output()
        .expect("git is available to the hermetic fixture");
    assert!(
        output.status.success(),
        "git {args:?} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn lint(root: &Path) -> Output {
    Command::new(repo_root().join("scripts/lint-issue-refs.sh"))
        .arg(root)
        .current_dir(root)
        .output()
        .expect("the lint script runs")
}

fn assert_lint(root: &Path, expected: i32) {
    let output = lint(root);
    assert_eq!(
        output.status.code(),
        Some(expected),
        "unexpected lint result\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn rejects_process_provenance_on_every_authored_surface() {
    let cases = [
        ("src/line.rs", "fn f() {} // issue 42\n"),
        ("src/hash_line.rs", "fn f() {} // issue #42\n"),
        ("src/block.rs", "/* reviewer R12 */\nfn f() {}\n"),
        (
            "src/review_tool.rs",
            "fn f() {} // CodeRabbit requested this\n",
        ),
        (
            "src/review_tool_block.rs",
            "/* Gemini Code Assist requested this */\nfn f() {}\n",
        ),
        (
            "docs/reviewer_plural.md",
            "The Geminis' framing was retained here.\n",
        ),
        ("crates/pipeline/src/authored.rs", "fn f() {} // issue 42\n"),
        ("docs/note.md", "Delivery is tracked in pull request #42.\n"),
        (".agents/note.md", "See #42 before editing.\n"),
        ("Makefile", "all:\n\t@# Task 7 retained here\n\t@true\n"),
        (
            "helper.sh",
            "#!/bin/sh\n# CodeRabbit requested this\ntrue\n",
        ),
        ("config.toml", "history = \"gap 12\"\n"),
        (
            "review.md",
            "coderabbitai[bot] and Gemini Code Assist approved.\n",
        ),
    ];

    for (path, body) in cases {
        let repo = FixtureRepo::new();
        repo.write(path, body);
        repo.commit_as_origin(path);
        assert_lint(repo.path(), 1);
    }
}

#[test]
fn rejects_process_provenance_in_branch_added_unscanned_types() {
    let repo = FixtureRepo::new();
    repo.write(
        "ontology.ttl",
        "<urn:s> <urn:p> \"Finding F12 belongs in the tracker\" .\n",
    );
    assert_lint(repo.path(), 1);
}

#[test]
fn tracked_authored_files_are_scanned_even_when_an_ignore_rule_matches() {
    let repo = FixtureRepo::new();
    repo.write(".gitignore", "docs/\n");
    repo.write("docs/ignored.md", "The delivery follows issue 42.\n");
    git(repo.path(), &["add", ".gitignore"]);
    git(repo.path(), &["add", "-f", "--", "docs/ignored.md"]);
    repo.commit_index_as_origin();

    assert_lint(repo.path(), 1);
}

#[test]
fn rust_block_scan_stops_at_each_comment_boundary() {
    let repo = FixtureRepo::new();
    repo.write(
        "src/boundary.rs",
        "/* stable invariant */\nconst TEXT: &str = \"issue 42\";\n/* another invariant */\n",
    );
    repo.commit_as_origin("src/boundary.rs");

    assert_lint(repo.path(), 0);
}

/// Structural exclusions: reproducible build products, scratch state, and
/// secrets, all of which are named because they are properties of *this*
/// repository's layout rather than of whatever tools a developer has installed.
///
/// Foreign trees are no longer part of this list — they are derived, and are
/// covered by the untracked-and-ignored fixture below.
#[test]
fn excludes_only_reproducible_products_and_scratch_state() {
    let paths = [
        ".git/private-note",
        ".worktrees/other/note.md",
        ".venv/note.md",
        "__pycache__/note.md",
        ".pytest_cache/note.md",
        ".ruff_cache/note.md",
        ".mypy_cache/note.md",
        ".tox/note.md",
        ".cache/note.md",
        "generated/note.md",
        "target/note.md",
        "build/note.md",
        "out/note.md",
        "dist/note.md",
        "ontology-docs/note.md",
        "nested/ontology-docs/note.md",
        "docs/_generated/note.md",
        "htmlcov/note.md",
        "package.egg-info/note.md",
        "bytecode.pyc",
        "pending.snap.new",
        ".coverage",
        "lcov.info",
        "llms.txt",
        "rustc-ice-fixture.txt",
        ".DS_Store",
        "nested/.DS_Store",
        "scratch.swp",
        ".stamps/note.md",
        "nested/.stamps/note.md",
        ".tmp/note.md",
        "nested/.tmp/note.md",
        ".gmeow-tmp-fixture/note.md",
        "nested/.gmeow-tmp-fixture/note.md",
        "node_modules/pkg/note.md",
        "mutants.out/note.md",
        "pipeline/note.md",
        ".mcp.json",
        ".worktree",
        "nested/.worktree",
        "nested/.coverage",
        "nested/lcov.info",
        "nested/llms.txt",
        "nested/.mcp.json",
        "nested/catalog-v001.xml",
        "keys/signing.secret",
        "keys/signing.secret.asc",
        "keys/signing.tmp",
        "catalog-v001.xml",
        "packages/python/gmeow_models/note.md",
    ];

    let repo = FixtureRepo::new();
    for path in paths {
        repo.write(path, "issue 42\n");
    }
    assert_lint(repo.path(), 0);
}

/// Foreign trees — local tool checkouts, agent runtime state, vendored caches —
/// are excluded by a *derived* rule rather than by name: a path that is both
/// untracked and ignored is not authored content of this repository.
///
/// The tool names that the gate used to enumerate are deliberately absent from
/// this fixture. That the rule needs no vendor name is the point of it: the
/// repository should not have to carry the identity of software it does not
/// ship, and the gate should not need an edit when the next tool appears.
///
/// This asserts only the excluding direction. The conjunct that makes the rule
/// safe — a *tracked* file is still scanned even when an ignore rule matches
/// it — is pinned separately by
/// `tracked_authored_files_are_scanned_even_when_an_ignore_rule_matches`.
/// Weakening the rule to "ignored" alone would red that test, which is exactly
/// the guard rail wanted here.
#[test]
fn foreign_trees_are_excluded_by_being_untracked_and_ignored_not_by_name() {
    let repo = FixtureRepo::new();
    repo.write(
        ".gitignore",
        "vendor-tool/\n.vendor-tool-state/\nscratch.md\n",
    );
    repo.commit_as_origin(".gitignore");

    // Untracked AND ignored: a local tool checkout, its runtime state, and a
    // scratch file. Policy-as-code tooling is full of tracker references, and
    // none of it is this repository's content.
    repo.write("vendor-tool/README.md", "See issue 42 for the rationale.\n");
    repo.write(".vendor-tool-state/run.log", "applied issue 42\n");
    repo.write("scratch.md", "issue 42\n");

    assert_lint(repo.path(), 0);
}

#[test]
fn untracked_file_without_final_newline_preserves_the_next_file_identity() {
    let repo = FixtureRepo::new();
    repo.write("first.txt", "clean without a final newline");
    repo.write(
        "second.ttl",
        "<urn:s> <urn:p> \"Finding F12 belongs in the tracker\" .\n",
    );

    let output = lint(repo.path());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(1),
        "unexpected lint result\nstdout:\n{}\nstderr:\n{stderr}",
        String::from_utf8_lossy(&output.stdout),
    );
    assert!(
        stderr.contains("second.ttl:1:"),
        "diagnostic lost the second file identity:\n{stderr}"
    );
    assert!(
        !stderr.contains("first.txt:1:"),
        "diagnostic was attributed to the preceding file:\n{stderr}"
    );
}

#[test]
fn accepts_narrow_technical_and_governance_controls() {
    let repo = FixtureRepo::new();
    repo.write(
        "docs/controls.md",
        concat!(
            "Unicode conformance follows UAX #15 and UTS #39.\n",
            "Principle 18 and competency G11 remain authoritative.\n",
            "The local section is [§2.1](#21-deliberate-rejections).\n",
            "The RDF IRI is <https://example.test/ns#42>.\n",
        ),
    );
    repo.write(
        "assets/theme.css",
        concat!(
            ":root { color: #123; }\n",
            ".a { background: #1234; }\n",
            ".b { border-color: #123456; }\n",
            ".c { outline-color: #12345678; }\n",
        ),
    );
    repo.write(
        "AGENTS.md",
        "Read both automated CodeRabbit and human reviews before finalization.\n",
    );
    repo.write(
        "metadata/references.ttl",
        concat!(
            "<urn:c1> rdfs:label ",
            "\"GitHub PR review comment discussion_r123 by CodeRabbit\" .\n",
            "<urn:c2> rdfs:label ",
            "\"GitHub comment issuecomment-456 by CodeRabbit\" .\n",
            "<urn:c3> gmeow:authority \"CodeRabbit\" .\n",
        ),
    );
    repo.write(
        "slices/grounding/logic/design/LOGIC-REFERENCES.md",
        "Journal of Semantic Review 78(1), 1–10.\n",
    );
    repo.write(
        ".deficiencies",
        "Audit pointer: Blackcat-Informatics/gmeow-ontology#1655\n",
    );
    repo.write(
        "crates/xtask/tests/issue_refs_lint.rs",
        "// issue 42 is an exact lint fixture\n",
    );

    assert_lint(repo.path(), 0);
}

#[test]
fn contextual_controls_do_not_create_general_exemptions() {
    let cases = [
        ("docs/plain.md", "See #15.\n"),
        ("docs/plain.md", "Unqualified swatch #123456.\n"),
        (".deficiencies", "Unresolved issue 42.\n"),
        (
            ".deficiencies",
            "Unresolved Blackcat-Informatics/gmeow-ontology#1655.\n",
        ),
        ("AGENTS.md", "CodeRabbit approved this.\n"),
        (
            "metadata/references.ttl",
            "<urn:citation> rdfs:comment \"CodeRabbit\" .\n",
        ),
        ("crates/xtask/tests/another.rs", "fn f() {} // issue 42\n"),
        (
            "slices/grounding/logic/design/LOGIC-REFERENCES.md",
            "Review 78 is delivery history.\n",
        ),
    ];

    for (path, body) in cases {
        let repo = FixtureRepo::new();
        repo.write(path, body);
        assert_lint(repo.path(), 1);
    }
}

#[test]
fn cargo_lock_is_excluded_from_the_toml_scan() {
    // Cargo.lock is a resolver product: the TOML leg excludes it by name. Commit
    // it as origin so only the surface scanners (not the branch-added leg) apply,
    // proving the exclusion rather than the missing-remote skip.
    let repo = FixtureRepo::new();
    repo.write("Cargo.lock", "# pin retained for issue 42 compatibility\n");
    repo.commit_as_origin("Cargo.lock");
    assert_lint(repo.path(), 0);
}

#[test]
fn the_real_repository_is_clean() {
    assert_lint(&repo_root(), 0);
}
