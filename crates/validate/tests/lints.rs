// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for the `gmeow-validate` lints.
//!
//! These exercise the PyO3-free engine API directly: [`store::parse_file_dataset`]
//! (syntax checking) and [`store::sameas_violations`] (the Principle 5 ban).
//! Inline Turtle fixtures keep each case self-contained.

use std::path::PathBuf;

use gmeow_validate::store;

/// The GMEOW vocabulary namespace (mirrors `config.NAMESPACE`); supplied to the
/// lint here exactly as Python passes `str(NAMESPACE)`.
const NS: &str = "https://blackcatinformatics.ca/gmeow/";

/// Write `contents` to `name` inside a fresh RAII temp directory.
///
/// The returned [`tempfile::TempDir`] owns the directory: it is removed on drop,
/// including on panic and early return. Bind it to a named `_tmp` (never a bare
/// `_`, which would drop it immediately) so it outlives the path. The file *name*
/// is preserved because the parser dispatches on the `.ttl` extension.
fn write_tmp(name: &str, contents: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join(name);
    std::fs::write(&path, contents).unwrap();
    (dir, path)
}

/// Syntax-error case: a malformed Turtle file must parse-error.
#[test]
fn syntax_error_is_detected() {
    let (_tmp, path) = write_tmp(
        "gmeow_validate_it_syntax_bad.ttl",
        "@prefix ex: <https://example.org/> .\nex:a ex:p   .  # missing object\n<<< garbage",
    );
    let result = store::parse_file_dataset(&path);
    assert!(result.is_err(), "malformed Turtle must be a syntax error");
}

/// A well-formed file parses with no error (the no-violation baseline).
#[test]
fn good_turtle_parses_clean() {
    let (_tmp, path) = write_tmp(
        "gmeow_validate_it_syntax_good.ttl",
        "@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\n",
    );
    let result = store::parse_file_dataset(&path);
    assert!(result.is_ok(), "well-formed Turtle must parse");
}

/// Banned-sameAs case: `owl:sameAs` to an external entity is a violation.
#[test]
fn banned_external_sameas_is_a_violation() {
    let (_tmp, path) = write_tmp(
        "gmeow_validate_it_sameas_bad.ttl",
        "@prefix ex: <https://example.org/> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         ex:a owl:sameAs ex:b .\n",
    );
    let ds = store::parse_file_dataset(&path).expect("fixture must parse");
    let violations = store::sameas_violations(&ds, NS, &[]);
    assert_eq!(
        violations.len(),
        1,
        "one external owl:sameAs must be banned"
    );
    assert_eq!(
        violations[0],
        (
            "https://example.org/a".to_owned(),
            "https://example.org/b".to_owned()
        )
    );
}

/// Allowlisted-sameAs case: an explicit `(subject, object)` allowlist entry
/// suppresses the violation.
#[test]
fn allowlisted_external_sameas_passes() {
    let (_tmp, path) = write_tmp(
        "gmeow_validate_it_sameas_allow.ttl",
        "@prefix ex: <https://example.org/> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         ex:a owl:sameAs ex:b .\n",
    );
    let ds = store::parse_file_dataset(&path).expect("fixture must parse");
    let allowlist = vec![(
        "https://example.org/a".to_owned(),
        "https://example.org/b".to_owned(),
    )];
    let violations = store::sameas_violations(&ds, NS, &allowlist);
    assert!(
        violations.is_empty(),
        "an allowlisted (subject, object) pair must not be a violation"
    );
}

/// GMEOW-internal-sameAs case: `owl:sameAs` between two GMEOW-namespaced terms
/// is allowed (the ban targets external-entity merges only).
#[test]
fn gmeow_internal_sameas_passes() {
    let (_tmp, path) = write_tmp(
        "gmeow_validate_it_sameas_internal.ttl",
        &format!(
            "@prefix gmeow: <{NS}> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             gmeow:A owl:sameAs gmeow:B .\n"
        ),
    );
    let ds = store::parse_file_dataset(&path).expect("fixture must parse");
    let violations = store::sameas_violations(&ds, NS, &[]);
    assert!(
        violations.is_empty(),
        "GMEOW-internal owl:sameAs must be allowed"
    );
}
