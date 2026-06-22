// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Case discovery for the conformance corpus.
//!
//! The `datatest-stable` harness in `tests/conformance.rs` performs the actual
//! filesystem glob (one nextest case per `profile.json` sentinel) and calls
//! [`validate_case`] on each discovered directory. [`discover_cases`] is the
//! standalone walk mirroring the retired Python `discover_cases` contract — used
//! by the parity/unit tests and any non-harness caller:
//!
//! * `input.logic.ttl` is required.
//! * `profile.json` is required and must be a JSON object (malformed ⇒ hard fail).
//! * `input.nq` is optional (world-indexed cases supply it; projection-only cases
//!   do not).
//!
//! ## External-corpus seam (#752 / #753)
//!
//! Discovery is intentionally **category-agnostic**: the harness globs every
//! `profile.json` under `cases/`, so a future `cases/external/<corpus>/<case>/`
//! group is auto-discovered the moment it adopts the same per-case anatomy. The
//! SZS/manifest ingestion adapter that lowers third-party corpora INTO that
//! anatomy is the scope of #753; this harness is the discovery host it plugs into.

use std::path::{Path, PathBuf};

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
/// is a malformed case and a hard failure (matching the Python runner's `run`).
pub fn validate_case(case_dir: &Path) -> Result<ConformanceCase, String> {
    let case_id = crate::paths::case_id(case_dir);

    let input = case_dir.join("input.logic.ttl");
    if !input.is_file() {
        return Err(format!(
            "case {case_id}: input.logic.ttl not found at {}",
            input.display()
        ));
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
/// Walks up to two levels (`cases/<category>/<case>/`) in sorted order, mirroring
/// the Python `discover_cases`: a directory is a case iff it holds BOTH
/// `input.logic.ttl` and `profile.json` (a directory missing either is skipped,
/// not failed). A present-but-malformed `profile.json` is a hard failure.
///
/// # Errors
/// Returns an error if `cases_root` is not a directory, or any discovered case's
/// `profile.json` is unreadable / not a JSON object.
pub fn discover_cases(cases_root: &Path) -> Result<Vec<ConformanceCase>, String> {
    if !cases_root.is_dir() {
        return Err(format!(
            "conformance cases directory does not exist: {}. \
             Expected conformance/logic/cases/ to be present.",
            cases_root.display()
        ));
    }

    let mut found = Vec::new();
    for category_dir in sorted_subdirs(cases_root)? {
        for case_dir in sorted_subdirs(&category_dir)? {
            let input = case_dir.join("input.logic.ttl");
            let profile_path = case_dir.join("profile.json");
            if !input.exists() || !profile_path.exists() {
                continue;
            }
            let case_id = crate::paths::case_id(&case_dir);
            let profile = read_profile_object(&case_id, &case_dir)?;
            found.push(ConformanceCase {
                case_dir,
                case_id,
                profile,
            });
        }
    }
    Ok(found)
}

/// Read `<case_dir>/profile.json`, requiring it to be a readable JSON object.
fn read_profile_object(case_id: &str, case_dir: &Path) -> Result<serde_json::Value, String> {
    let profile_path = case_dir.join("profile.json");
    let text = std::fs::read_to_string(&profile_path)
        .map_err(|e| format!("case {case_id}: cannot read profile.json: {e}"))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("case {case_id}: cannot parse profile.json: {e}"))?;
    if !value.is_object() {
        return Err(format!(
            "case {case_id}: profile.json must be a JSON object, got {}",
            json_type_name(&value)
        ));
    }
    Ok(value)
}

/// The immediate subdirectories of `dir`, sorted by path for deterministic order.
fn sorted_subdirs(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut subdirs: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("cannot read directory {}: {e}", dir.display()))?
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
        // Ports tests/test_logic_runner.py::TestDiscoverCases::test_discovers_projection_cases:
        // the projection cases (and the rest of the corpus) are discovered.
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
        let tmp = std::env::temp_dir().join(format!("gmeow-conf-test-{}", std::process::id()));
        let case = tmp.join("cat").join("case");
        std::fs::create_dir_all(&case).expect("mkdir");
        std::fs::write(case.join("profile.json"), "{}").expect("write");
        let err = validate_case(&case).unwrap_err();
        assert!(err.contains("input.logic.ttl not found"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn validate_case_rejects_non_object_profile() {
        let tmp =
            std::env::temp_dir().join(format!("gmeow-conf-test-nonobj-{}", std::process::id()));
        let case = tmp.join("cat").join("case");
        std::fs::create_dir_all(&case).expect("mkdir");
        std::fs::write(case.join("input.logic.ttl"), "").expect("write input");
        std::fs::write(case.join("profile.json"), "[1, 2, 3]").expect("write profile");
        let err = validate_case(&case).unwrap_err();
        assert!(err.contains("must be a JSON object"));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
