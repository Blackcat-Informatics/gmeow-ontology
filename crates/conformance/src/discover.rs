// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Case discovery for the conformance corpus.
//!
//! The `datatest-stable` harness in `tests/conformance.rs` performs the actual
//! filesystem glob (one nextest case per `profile.json` sentinel) and calls
//! [`validate_case`] on each discovered directory. [`discover_cases`] is the
//! standalone walk used by the parity/unit tests and any non-harness caller
//! (the retired Python `discover_cases` it supersedes was removed):
//!
//! * `input.logic.ttl` is required.
//! * `profile.json` is required and must be a JSON object (malformed ⇒ hard fail).
//! * `input.nq` is optional (world-indexed cases supply it; projection-only cases
//!   do not).
//!
//! ## External-corpus seam
//!
//! Discovery is intentionally **category-agnostic**: the harness globs every
//! `profile.json` under `cases/`, so a future `cases/external/<corpus>/<case>/`
//! group is auto-discovered the moment it adopts the same per-case anatomy. The
//! SZS/manifest ingestion adapter that lowers third-party corpora INTO that
//! anatomy is the scope; this harness is the discovery host it plugs into.

use std::path::{Path, PathBuf};

use gmeow_errors::Diag;

use crate::error::{CaseAnatomy, Io, ProfileInvalid};

/// A discovered, validated conformance case.
#[derive(Debug, Clone)]
pub struct ConformanceCase {
    /// Absolute path to the case directory.
    pub case_dir: PathBuf,
    /// The `<category>/<case>` identifier.
    pub case_id: String,
    /// The parsed `profile.json` (guaranteed to be a JSON object).
    pub profile: serde_json::Value,
}

/// Validate that `case_dir` is a runnable conformance case.
///
/// Returns the [`ConformanceCase`] on success, or a human-readable error string
/// describing the first missing/malformed required artifact (hard-fail, no
/// silent skip — verification-honesty). This is the harness entry point: the
/// glob already proved `profile.json` exists, so a missing `input.logic.ttl` here
/// is a malformed case and a hard failure.
pub fn validate_case(case_dir: &Path) -> gmeow_errors::Result<ConformanceCase> {
    let case_id = crate::paths::case_id(case_dir);

    let input = case_dir.join("input.logic.ttl");
    if !input.is_file() {
        return Err(Diag::of_kind(CaseAnatomy {
            detail: format!(
                "case {case_id}: input.logic.ttl not found at {}",
                input.display()
            ),
        }));
    }

    let profile = read_profile_object(&case_id, case_dir)?;
    Ok(ConformanceCase {
        case_dir: case_dir.to_path_buf(),
        case_id,
        profile,
    })
}

/// Discover every conformance case under `cases_root`.
///
/// Walks the tree at ANY depth in sorted order: a directory is a case iff it
/// holds BOTH `input.logic.ttl` and `profile.json`. This recovers the standard
/// two-level `cases/<category>/<case>/` layout AND the three-level vendored
/// `cases/external/<corpus>/<case>/` layout, matching the recursive glob the
/// `datatest-stable` harness already uses — so external cases reach the
/// `conformance-report` binary / release artifact too (maximal information flow),
/// not just the test gate. A directory missing either sentinel is recursed into; a
/// directory holding both IS a case and is NOT descended into (so a case's vendored
/// `source/` subtree is never mistaken for nested cases). A present-but-malformed
/// `profile.json` is a hard failure.
///
/// # Errors
/// Returns an error if `cases_root` is not a directory, or any discovered case's
/// `profile.json` is unreadable / not a JSON object.
pub fn discover_cases(cases_root: &Path) -> gmeow_errors::Result<Vec<ConformanceCase>> {
    if !cases_root.is_dir() {
        return Err(Diag::of_kind(CaseAnatomy {
            detail: format!(
                "conformance cases directory does not exist: {}. \
                 Expected conformance/logic/cases/ to be present.",
                cases_root.display()
            ),
        }));
    }

    let mut found = Vec::new();
    collect_cases(cases_root, &mut found)?;
    Ok(found)
}

/// Recursively collect cases under `dir`. A directory holding both sentinels is a
/// case (and is not descended into); otherwise its subdirectories are walked in
/// sorted order.
fn collect_cases(dir: &Path, found: &mut Vec<ConformanceCase>) -> gmeow_errors::Result<()> {
    let input = dir.join("input.logic.ttl");
    let profile_path = dir.join("profile.json");
    // Sentinels must be FILES (not e.g. a directory named `profile.json`), matching
    // `validate_case`'s `is_file()` check — `exists()` would false-match a directory.
    if input.is_file() && profile_path.is_file() {
        let case_id = crate::paths::case_id(dir);
        let profile = read_profile_object(&case_id, dir)?;
        found.push(ConformanceCase {
            case_dir: dir.to_path_buf(),
            case_id,
            profile,
        });
        return Ok(());
    }
    for sub in sorted_subdirs(dir)? {
        collect_cases(&sub, found)?;
    }
    Ok(())
}

/// Read `<case_dir>/profile.json`, requiring it to be a readable JSON object.
fn read_profile_object(case_id: &str, case_dir: &Path) -> gmeow_errors::Result<serde_json::Value> {
    let profile_path = case_dir.join("profile.json");
    let text = std::fs::read_to_string(&profile_path).map_err(|e| {
        Diag::of_kind(Io {
            detail: format!("case {case_id}: cannot read profile.json: {e}"),
        })
    })?;
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
        Diag::of_kind(ProfileInvalid {
            detail: format!("case {case_id}: cannot parse profile.json: {e}"),
        })
    })?;
    if !value.is_object() {
        return Err(Diag::of_kind(ProfileInvalid {
            detail: format!(
                "case {case_id}: profile.json must be a JSON object, got {}",
                json_type_name(&value)
            ),
        }));
    }
    Ok(value)
}

/// The immediate subdirectories of `dir`, sorted by path for deterministic order.
fn sorted_subdirs(dir: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
    let mut subdirs: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| {
            Diag::of_kind(Io {
                detail: format!("cannot read directory {}: {e}", dir.display()),
            })
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|p| p.is_dir())
        .collect();
    subdirs.sort();
    Ok(subdirs)
}

/// A short human label for a JSON value's kind (for error messages).
fn json_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The live corpus root, anchored at the crate manifest dir.
    fn corpus_cases_root() -> PathBuf {
        crate::paths::cases_root()
    }

    #[test]
    fn discovers_the_full_corpus() {
        // Canonical discovery test: every case under projections/ (and the rest of the corpus)
        // is discovered. (The retired tests/test_logic_runner.py::TestDiscoverCases covered the
        // same case before being removed.)
        let cases = discover_cases(&corpus_cases_root()).expect("discovery ok");
        let ids: Vec<&str> = cases.iter().map(|c| c.case_id.as_str()).collect();
        assert!(
            ids.iter().any(|id| id.starts_with("projections/")),
            "no projection cases among {ids:?}"
        );
        // Every discovered case carries a JSON-object profile and an input file.
        for case in &cases {
            assert!(case.profile.is_object());
            assert!(case.case_dir.join("input.logic.ttl").is_file());
        }
        // The corpus is non-trivial (sanity floor; the exact count is asserted by
        // the harness against the Python baseline, not pinned here).
        assert!(cases.len() >= 20, "unexpectedly few cases: {}", cases.len());

        // Recursion guard: the discovered id SET must be unchanged except for
        // `external/**` additions. A standard case id is exactly `<category>/<case>`
        // (one slash); only a vendored corpus case may be deeper, and then it MUST be
        // `external/<corpus>/<case>` (two slashes). This catches the recursion
        // accidentally vacuuming up a stray nested directory under a non-external
        // category as a bogus 3-component case.
        for id in &ids {
            let depth = id.matches('/').count();
            if id.starts_with("external/") {
                assert_eq!(
                    depth, 2,
                    "external case id must be external/<corpus>/<case>: {id}"
                );
            } else {
                assert_eq!(
                    depth, 1,
                    "non-external case id must be <category>/<case>: {id}"
                );
            }
        }
    }

    #[test]
    fn discovers_three_level_external_case_and_skips_source_subtree() {
        // A vendored external corpus lives at cases/external/<corpus>/<case>/ — three
        // levels deep. The recursive walk must find it (the binary path missed it
        // before), assign the external/-prefixed id, and NOT mistake the case's
        // own `source/` subtree for a nested case.
        let tmp = tempfile::tempdir().expect("create temp dir");
        let cases = tmp.path().join("cases");
        let case = cases.join("external").join("w3c-mini").join("clash");
        std::fs::create_dir_all(case.join("source")).expect("mkdir case+source");
        std::fs::write(case.join("input.logic.ttl"), "").expect("input");
        std::fs::write(case.join("profile.json"), "{}").expect("profile");
        // A decoy under source/ that would be a case if recursion didn't stop at the
        // case dir.
        std::fs::write(case.join("source").join("input.logic.ttl"), "").expect("decoy input");
        std::fs::write(case.join("source").join("profile.json"), "{}").expect("decoy profile");

        let found = discover_cases(&cases).expect("discovery ok");
        let ids: Vec<&str> = found.iter().map(|c| c.case_id.as_str()).collect();
        assert_eq!(ids, vec!["external/w3c-mini/clash"], "got {ids:?}");
    }

    #[test]
    fn hard_fails_on_missing_cases_dir() {
        // Ports TestDiscoverCases::test_hard_fails_on_missing_cases_dir.
        let missing = corpus_cases_root().join("__definitely_absent__");
        assert!(discover_cases(&missing).is_err());
    }

    #[test]
    fn validate_case_rejects_dir_without_input() {
        // A profile.json-bearing dir missing input.logic.ttl is a hard failure.
        let tmp = tempfile::tempdir().expect("create temp dir");
        let case = tmp.path().join("cat").join("case");
        std::fs::create_dir_all(&case).expect("mkdir");
        std::fs::write(case.join("profile.json"), "{}").expect("write");
        let err = validate_case(&case).unwrap_err();
        assert!(err.message().contains("input.logic.ttl not found"));
    }

    #[test]
    fn validate_case_rejects_non_object_profile() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let case = tmp.path().join("cat").join("case");
        std::fs::create_dir_all(&case).expect("mkdir");
        std::fs::write(case.join("input.logic.ttl"), "").expect("write input");
        std::fs::write(case.join("profile.json"), "[1, 2, 3]").expect("write profile");
        let err = validate_case(&case).unwrap_err();
        assert!(err.message().contains("must be a JSON object"));
    }
}
