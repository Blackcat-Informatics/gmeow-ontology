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

/// The single shared vendored-corpus root — the parent of every vendored corpus
/// family (`external/` correctness suites, `bench/` performance corpora). This is the
/// same physical path as [`cases_root`], named as an intentional abstraction so the
/// two families are one root by design, not two constants that happen to agree.
pub fn vendored_corpus_root() -> PathBuf {
    cases_root()
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

/// The stable case identifier: the case directory path RELATIVE to the `cases/`
/// root.
///
/// For the standard two-level layout this is `<category>/<case>` (byte-identical to
/// the historical `parent.name/name` form). For a vendored external corpus at the
/// documented three-level depth it is `external/<corpus>/<case>` — so the
/// `external/` prefix is preserved and two corpora cannot collide with a non-external
/// category. The id is computed by joining every path component AFTER the last
/// `cases` component, which works for both absolute paths (the `conformance-report`
/// binary) and the relative paths the datatest harness passes.
pub fn case_id(case_dir: &Path) -> String {
    let comps: Vec<&str> = case_dir
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();
    if let Some(pos) = comps.iter().rposition(|c| *c == "cases") {
        let rel = &comps[pos + 1..];
        if !rel.is_empty() {
            return rel.join("/");
        }
    }
    // Fallback for paths with no `cases` component (e.g. synthetic temp dirs in
    // unit tests): the historical `parent.name/name` form.
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

    #[test]
    fn case_id_preserves_external_prefix_for_three_level_cases() {
        // A vendored external corpus case keeps its `external/` prefix.
        let dir = Path::new("/repo/conformance/logic/cases/external/w3c-mini/clash");
        assert_eq!(case_id(dir), "external/w3c-mini/clash");
    }

    #[test]
    fn case_id_works_on_relative_harness_paths() {
        // The datatest harness passes paths relative to the crate dir.
        let dir = Path::new("../../conformance/logic/cases/foundation/free-role");
        assert_eq!(case_id(dir), "foundation/free-role");
    }
}
