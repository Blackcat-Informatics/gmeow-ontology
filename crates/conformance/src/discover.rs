// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Case discovery for the conformance corpus.
//!
//! The pipeline producer selects every `profile.json` sentinel, validates its
//! anatomy, and routes the declared lane before native execution. The authenticated
//! golden consumer repeats only read-only inventory and anatomy checks.
//! [`discover_cases`] is the standalone walk used by maintenance reporting.
//!
//! * `input.logic.ttl` is required.
//! * `profile.json` is required and must be a JSON object (malformed ⇒ hard fail).
//! * `input.nq` is optional (world-indexed cases supply it; projection-only cases
//!   do not).
//!
//! ## External-corpus seam
//!
//! Discovery is intentionally **category-agnostic**: the producer selects every
//! `profile.json` under `cases/`, so a future `cases/external/<corpus>/<case>/`
//! group is auto-discovered the moment it adopts the same per-case anatomy. The
//! SZS/manifest ingestion adapter that lowers third-party corpora INTO that
//! anatomy plugs into the same explicit producer.

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

#[path = "discover.tests.rs"]
#[cfg(test)]
mod tests;
