// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3 bindings for the DSL SHACL validation seam.
//!
//! The Python-facing DSL loaders (`mapping_dsl`, `test_dsl`) used to delegate to
//! `gmeow_tools.dsl_validate`, which built the merged graph in Rust but ran the
//! SHACL engine and formatted violations in Python. This module provides the
//! canonical native entry point so the formatting and provenance enrichment live
//! in one place — the Rust side — eliminating the dual-authority bug (#937,
//! Principle 4).
//!
//! The actual merge / SHACL / provenance work lives in the engine helper in
//! [`crate::dsl_shacl`]; this module only adapts its structured findings into
//! the legacy formatted strings the Python callers expect.

use std::path::PathBuf;

use gmeow_diagnostics::Finding;
use pyo3::prelude::*;

/// Format one structured DSL finding into the legacy string form expected by
/// Python callers.
///
/// ```text
/// focus=<focusNode> | path=<resultPath> | msg=<message> | source=<file>
/// ```
///
/// `path`, `msg`, and `source` are omitted when not applicable. The special
/// "non-conforming with no results" guard is returned as a bare message so the
/// legacy boundary stays identical to the original implementation.
fn format_dsl_finding(finding: &Finding) -> String {
    // The engine's fallback guard has no focus node; preserve its raw message.
    if finding.code.ends_with(".nonconforming")
        && finding
            .primary_location()
            .and_then(|l| l.logical.as_deref())
            .is_none()
    {
        return finding.message.clone();
    }

    let mut parts: Vec<String> = Vec::new();

    if let Some(logical) = finding
        .primary_location()
        .and_then(|l| l.logical.as_deref())
    {
        parts.push(format!("focus={logical}"));
    }

    for related in &finding.related_locations {
        if let Some(logical) = related.logical.as_deref() {
            if let Some(path_iri) = logical.strip_prefix("path ") {
                parts.push(format!("path={path_iri}"));
                break;
            }
        }
    }

    if !finding.message.is_empty() {
        parts.push(format!("msg={}", finding.message));
    }

    if let Some(source) = finding.primary_location().and_then(|l| l.path.as_deref()) {
        parts.push(format!("source={source}"));
    }

    if parts.is_empty() {
        finding.message.clone()
    } else {
        parts.join(" | ")
    }
}

/// Internal Rust entry point for DSL SHACL validation.
///
/// This helper is used by both the public `#[pyfunction]` and the Rust unit
/// tests so the tests exercise the same code path without needing a Python
/// runtime (Principle 4, RUST-FIRST, #937).
fn validate_dsl_shacl_inner(dsl_paths: &[String], shapes_ttl: &str) -> Result<Vec<String>, String> {
    if dsl_paths.is_empty() {
        return Err("validate_dsl_shacl: paths to validate must not be empty".to_string());
    }

    let paths: Vec<PathBuf> = dsl_paths.iter().map(PathBuf::from).collect();
    let findings = crate::dsl_shacl::validate_dsl(&paths, shapes_ttl, "dsl")?;

    if findings.is_empty() {
        return Ok(Vec::new());
    }

    Ok(findings.iter().map(format_dsl_finding).collect())
}

/// Validate merged DSL Turtle sources against a SHACL shapes graph and return
/// formatted violation strings.
///
/// `dsl_paths` are processed in order; every named subject is mapped to the
/// first path it appears in. The merged graph is validated against
/// `shapes_ttl` using the native `gmeow_shacl` engine. Each non-conforming
/// result is formatted as:
///
/// ```text
/// focus=<focusNode> | path=<resultPath> | msg=<message> | source=<file>
/// ```
///
/// `path`, `msg`, and `source` are omitted when not applicable. An empty list
/// means the DSL graph conforms.
///
/// # Errors
///
/// Returns a Python `ValueError` on parse/merge/validate failures — a hard
/// fail, never a silent conformant result (P11/§11).
#[pyfunction]
pub fn validate_dsl_shacl(dsl_paths: Vec<String>, shapes_ttl: String) -> PyResult<Vec<String>> {
    validate_dsl_shacl_inner(&dsl_paths, &shapes_ttl)
        .map_err(pyo3::exceptions::PyValueError::new_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Write;

    fn write_tmp(name: &str, contents: &str) -> PathBuf {
        let path = std::env::temp_dir().join(name);
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(contents.as_bytes()).unwrap();
        path
    }

    fn simple_shapes() -> String {
        r#"@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix ex: <https://example.org/> .

ex:NodeShape a sh:NodeShape ;
    sh:targetClass ex:Thing ;
    sh:property [
        sh:path ex:name ;
        sh:minCount 1 ;
        sh:severity sh:Violation ;
        sh:message "Missing ex:name" ;
    ] .
"#
        .to_owned()
    }

    #[test]
    fn clean_graph_returns_empty_violations() {
        let ttl = write_tmp(
            "gmeow_py_dsl_clean.ttl",
            "@prefix ex: <https://example.org/> .\n\
             ex:alice a ex:Thing ; ex:name \"Alice\" .\n",
        );
        let violations =
            validate_dsl_shacl_inner(&[path_arg(&ttl)], &simple_shapes()).expect("must succeed");
        assert!(
            violations.is_empty(),
            "expected no violations, got {violations:?}"
        );
    }

    #[test]
    fn malformed_cell_carries_focus_path_msg_and_source() {
        let ttl = write_tmp(
            "gmeow_py_dsl_bad.ttl",
            "@prefix ex: <https://example.org/> .\n\
             ex:bob a ex:Thing .\n",
        );
        let violations =
            validate_dsl_shacl_inner(&[path_arg(&ttl)], &simple_shapes()).expect("must succeed");
        assert!(!violations.is_empty(), "expected at least one violation");
        let msg = violations.join("\n");
        assert!(msg.contains("focus=https://example.org/bob"), "{msg}");
        assert!(msg.contains("path="), "{msg}");
        assert!(msg.contains("msg="), "{msg}");
        assert!(msg.contains(&format!("source={}", ttl.display())), "{msg}");
    }

    #[test]
    fn provenance_first_seen_wins() {
        let a = write_tmp(
            "gmeow_py_dsl_prov_a.ttl",
            "@prefix ex: <https://example.org/> .\n\
             ex:shared a ex:Thing ; ex:name \"A\" .\n",
        );
        let b = write_tmp(
            "gmeow_py_dsl_prov_b.ttl",
            "@prefix ex: <https://example.org/> .\n\
             ex:shared a ex:Thing ; ex:name \"B\" .\n",
        );
        // Remove the required name from both; the violation should attribute to file a.
        std::fs::write(
            &a,
            "@prefix ex: <https://example.org/> .\nex:shared a ex:Thing .\n",
        )
        .unwrap();
        std::fs::write(
            &b,
            "@prefix ex: <https://example.org/> .\nex:shared a ex:Thing .\n",
        )
        .unwrap();
        let violations = validate_dsl_shacl_inner(&[path_arg(&a), path_arg(&b)], &simple_shapes())
            .expect("must succeed");
        assert_eq!(
            violations.len(),
            1,
            "expected one violation for shared subject"
        );
        assert!(
            violations[0].contains(&format!("source={}", a.display())),
            "{}",
            violations[0]
        );
    }

    #[test]
    fn parse_error_hard_fails_with_path() {
        let bad = write_tmp("gmeow_py_dsl_bad_syntax.ttl", "this is not turtle @@@ <<<");
        let err =
            validate_dsl_shacl_inner(&[path_arg(&bad)], &simple_shapes()).expect_err("must fail");
        assert!(err.contains(&bad.display().to_string()), "{err}");
    }

    #[test]
    fn legacy_format_sample_matches_python_output() {
        // Proves parity with the deleted Python `_format_violations` helper:
        // focus=<focusNode> | path=<resultPath> | msg=<message> | source=<file>
        let ttl = write_tmp(
            "gmeow_py_dsl_legacy_sample.ttl",
            "@prefix ex: <https://example.org/> .\n\
             ex:bob a ex:Thing .\n",
        );
        let violations =
            validate_dsl_shacl_inner(&[path_arg(&ttl)], &simple_shapes()).expect("must succeed");
        assert_eq!(violations.len(), 1);
        let expected = format!(
            "focus=https://example.org/bob | path=https://example.org/name | \
             msg=Missing ex:name | source={}",
            ttl.display()
        );
        assert_eq!(violations[0], expected);
    }

    fn path_arg(path: &std::path::Path) -> String {
        path.to_string_lossy().into_owned()
    }
}
