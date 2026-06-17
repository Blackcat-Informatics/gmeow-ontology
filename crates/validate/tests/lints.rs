// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for the `gmeow-validate` lints (#579).
//!
//! These exercise the PyO3-free engine API directly: [`store::parse_file`]
//! (syntax checking) and [`store::sameas_violations`] (the Principle 5 ban).
//! Inline Turtle fixtures keep each case self-contained.

use std::path::PathBuf;

use gmeow_validate::store;

/// The GMEOW vocabulary namespace (mirrors `config.NAMESPACE`); supplied to the
/// lint here exactly as Python passes `str(NAMESPACE)`.
const NS: &str = "https://blackcatinformatics.ca/gmeow/";

fn write_tmp(name: &str, contents: &str) -> PathBuf {
    let path = std::env::temp_dir().join(name);
    std::fs::write(&path, contents).unwrap();
    path
}

/// Syntax-error case: a malformed Turtle file must parse-error.
#[test]
fn syntax_error_is_detected() {
    let path = write_tmp(
        "gmeow_validate_it_syntax_bad.ttl",
        "@prefix ex: <https://example.org/> .\nex:a ex:p   .  # missing object\n<<< garbage",
    );
    let result = store::parse_file(&path);
    std::fs::remove_file(&path).ok();
    assert!(result.is_err(), "malformed Turtle must be a syntax error");
}

/// A well-formed file parses with no error (the no-violation baseline).
#[test]
fn good_turtle_parses_clean() {
    let path = write_tmp(
        "gmeow_validate_it_syntax_good.ttl",
        "@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\n",
    );
    let result = store::parse_file(&path);
    std::fs::remove_file(&path).ok();
    assert!(result.is_ok(), "well-formed Turtle must parse");
}

/// Banned-sameAs case: `owl:sameAs` to an external entity is a violation.
#[test]
fn banned_external_sameas_is_a_violation() {
    let path = write_tmp(
        "gmeow_validate_it_sameas_bad.ttl",
        "@prefix ex: <https://example.org/> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         ex:a owl:sameAs ex:b .\n",
    );
    let quads = store::parse_file(&path).expect("fixture must parse");
    std::fs::remove_file(&path).ok();
    let violations = store::sameas_violations(&quads, NS, &[]);
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
    let path = write_tmp(
        "gmeow_validate_it_sameas_allow.ttl",
        "@prefix ex: <https://example.org/> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         ex:a owl:sameAs ex:b .\n",
    );
    let quads = store::parse_file(&path).expect("fixture must parse");
    std::fs::remove_file(&path).ok();
    let allowlist = vec![(
        "https://example.org/a".to_owned(),
        "https://example.org/b".to_owned(),
    )];
    let violations = store::sameas_violations(&quads, NS, &allowlist);
    assert!(
        violations.is_empty(),
        "an allowlisted (subject, object) pair must not be a violation"
    );
}

/// GMEOW-internal-sameAs case: `owl:sameAs` between two GMEOW-namespaced terms
/// is allowed (the ban targets external-entity merges only).
#[test]
fn gmeow_internal_sameas_passes() {
    let path = write_tmp(
        "gmeow_validate_it_sameas_internal.ttl",
        &format!(
            "@prefix gmeow: <{NS}> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             gmeow:A owl:sameAs gmeow:B .\n"
        ),
    );
    let quads = store::parse_file(&path).expect("fixture must parse");
    std::fs::remove_file(&path).ok();
    let violations = store::sameas_violations(&quads, NS, &[]);
    assert!(
        violations.is_empty(),
        "GMEOW-internal owl:sameAs must be allowed"
    );
}
