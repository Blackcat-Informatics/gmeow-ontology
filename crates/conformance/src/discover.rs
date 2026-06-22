// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Case discovery for the conformance corpus.
//!
//! The `datatest-stable` harness in `tests/conformance.rs` performs the actual
//! filesystem glob (one nextest case per `profile.json` sentinel). This module
//! hosts the per-case *validation* a discovered directory must satisfy to be a
//! runnable case, mirroring the Python runner's `discover_cases` contract:
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

use std::path::Path;

/// A discovered, validated conformance case.
#[derive(Debug, Clone)]
pub struct ConformanceCase {
    /// Absolute path to the case directory.
    pub case_dir: std::path::PathBuf,
    /// The `<category>/<case>` identifier.
    pub case_id: String,
}

/// Validate that `case_dir` is a runnable conformance case.
///
/// Returns the [`ConformanceCase`] on success, or a human-readable error string
/// describing the first missing/malformed required artifact (hard-fail, no
/// silent skip — verification-honesty).
pub fn validate_case(case_dir: &Path) -> Result<ConformanceCase, String> {
    let case_id = crate::paths::case_id(case_dir);

    let input = case_dir.join("input.logic.ttl");
    if !input.is_file() {
        return Err(format!(
            "case {case_id}: input.logic.ttl not found at {}",
            input.display()
        ));
    }

    // profile.json must exist, be readable, and be a JSON object. A non-object
    // (array/string/number) is a malformed profile — a hard failure, never a skip.
    let profile_path = case_dir.join("profile.json");
    let profile_text = std::fs::read_to_string(&profile_path)
        .map_err(|e| format!("case {case_id}: cannot read profile.json: {e}"))?;
    let value: serde_json::Value = serde_json::from_str(&profile_text)
        .map_err(|e| format!("case {case_id}: cannot parse profile.json: {e}"))?;
    if !value.is_object() {
        return Err(format!(
            "case {case_id}: profile.json must be a JSON object, got {}",
            json_type_name(&value)
        ));
    }

    Ok(ConformanceCase {
        case_dir: case_dir.to_path_buf(),
        case_id,
    })
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
