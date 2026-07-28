// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Behavioural gate for `scripts/lint-issue-refs.sh`'s build-surface coverage.
//!
//! Process-flow information belongs in GitHub, never in the repository. The lint
//! originally scanned Rust comments and Markdown only, and two references
//! survived indefinitely in the `Makefile` for exactly that reason: a rule
//! enforced on some file types migrates to the unenforced ones. These tests pin
//! the widened surface so it cannot silently narrow again.
//!
//! The fixtures use a synthetic all-zero three-digit marker rather than a real
//! issue number — writing a real one into a fixture would be the very thing under
//! ban, and naming it here in a comment reds this lint, as it should.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/xtask has a grandparent")
        .to_path_buf()
}

/// Run the lint against a root and return its exit code.
fn lint(root: &Path) -> i32 {
    Command::new(repo_root().join("scripts/lint-issue-refs.sh"))
        .arg(root)
        .current_dir(repo_root())
        .output()
        .expect("the lint script is executable")
        .status
        .code()
        .expect("the lint exits with a code")
}

/// A throwaway fixture tree containing one file.
fn fixture(name: &str, body: &str) -> tempdir::Dir {
    let dir = tempdir::Dir::new();
    std::fs::write(dir.path().join(name), body).expect("fixture is writable");
    dir
}

/// Minimal scoped temp directory — the workspace pulls in no temp-dir crate for
/// `xtask`, and a test helper does not justify a new dependency.
mod tempdir {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    pub struct Dir(PathBuf);

    impl Dir {
        pub fn new() -> Self {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("gmeow-issue-refs-{}-{}", std::process::id(), n));
            std::fs::create_dir_all(&path).expect("temp dir is creatable");
            Self(path)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn a_reference_in_a_makefile_is_rejected() {
    let dir = fixture("Makefile", "all:\n\t@# see #000 for context\n\techo hi\n");
    assert_eq!(
        lint(dir.path()),
        1,
        "a Makefile comment carrying an issue reference must red the lint"
    );
}

#[test]
fn a_reference_in_a_shell_script_is_rejected() {
    let dir = fixture("helper.sh", "#!/bin/sh\n# tracked in #000\necho hi\n");
    assert_eq!(
        lint(dir.path()),
        1,
        "a shell-script comment carrying an issue reference must red the lint"
    );
}

#[test]
fn a_reference_in_a_dot_mk_fragment_is_rejected() {
    let dir = fixture("fragment.mk", "# superseded by #000\nVAR := 1\n");
    assert_eq!(
        lint(dir.path()),
        1,
        "a .mk fragment carrying an issue reference must red the lint"
    );
}

#[test]
fn a_clean_build_surface_passes() {
    let dir = fixture(
        "Makefile",
        "all:\n\t@# no process references here\n\techo hi\n",
    );
    assert_eq!(lint(dir.path()), 0, "a clean fixture must not red the lint");
}

#[test]
fn the_real_repository_is_clean() {
    // The production surface, not a fixture: the tree itself must hold the rule.
    assert_eq!(
        lint(&repo_root()),
        0,
        "the repository carries an issue/PR reference in a scanned file"
    );
}
