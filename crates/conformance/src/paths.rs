// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Path resolution for the conformance harness.
//!
//! The harness is anchored at this crate's manifest directory
//! (`crates/conformance`) via `CARGO_MANIFEST_DIR`, so it never relies on the
//! process working directory. From there it derives the repository root and the
//! `conformance/logic/cases/` corpus tree.

use std::path::{Path, PathBuf};

/// The repository root, derived from this crate's manifest directory at compile
/// time (`crates/conformance/../..`).
///
/// # Panics
///
/// Panics if the canonical repo root does not exist, which can only happen if
/// the crate is built outside the repository tree — an impossible state for the
/// harness.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root (crates/conformance/../..) must exist")
}

/// The conformance corpus root (`conformance/logic`).
pub fn conformance_root() -> PathBuf {
    repo_root().join("conformance").join("logic")
}

/// The cases tree under the conformance root (`conformance/logic/cases`).
pub fn cases_root() -> PathBuf {
    conformance_root().join("cases")
}

/// The owning case directory for a discovered `profile.json` sentinel file.
///
/// A case lives at `.../cases/<category>/<case>/profile.json`, so the case
/// directory is the sentinel's parent.
pub fn case_dir(profile_json: &Path) -> PathBuf {
    let dir = profile_json.parent();
    debug_assert!(
        dir.is_some(),
        "case_dir expects .../<category>/<case>/profile.json, got {}",
        profile_json.display()
    );
    dir.unwrap_or(profile_json).to_path_buf()
}

/// The stable `<category>/<case>` identifier for a case directory.
///
/// Mirrors the Python runner's `case_id = f"{case_dir.parent.name}/{case_dir.name}"`.
pub fn case_id(case_dir: &Path) -> String {
    let name = case_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("<unknown>");
    let category = case_dir
        .parent()
        .and_then(Path::file_name)
        .and_then(|s| s.to_str())
        .unwrap_or("<unknown>");
    format!("{category}/{name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_dir_is_the_sentinel_parent() {
        let sentinel = Path::new("/repo/conformance/logic/cases/foundation/free-role/profile.json");
        assert_eq!(
            case_dir(sentinel),
            Path::new("/repo/conformance/logic/cases/foundation/free-role")
        );
    }

    #[test]
    fn case_id_is_category_slash_case() {
        let dir = Path::new("/repo/conformance/logic/cases/foundation/free-role");
        assert_eq!(case_id(dir), "foundation/free-role");
    }
}
