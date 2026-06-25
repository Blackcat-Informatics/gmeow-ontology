// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3 binding for native DSL SHACL validation.
//!
//! The Rust engine owns merge, SHACL validation, provenance enrichment, and
//! legacy string formatting. Python receives only the surface call.

use std::path::PathBuf;

use gmeow_diagnostics::Finding;
use pyo3::prelude::*;

fn format_dsl_finding(finding: &Finding) -> String {
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

fn validate_dsl_shacl_inner(dsl_paths: &[String], shapes_ttl: &str) -> Result<Vec<String>, String> {
    if dsl_paths.is_empty() {
        return Err("validate_dsl_shacl: paths to validate must not be empty".to_owned());
    }

    let paths: Vec<PathBuf> = dsl_paths.iter().map(PathBuf::from).collect();
    let findings = crate::dsl_shacl::validate_dsl(&paths, shapes_ttl, "dsl")?;
    Ok(findings.iter().map(format_dsl_finding).collect())
}

/// Validate merged DSL Turtle sources against a SHACL shapes graph.
///
/// Returns formatted violation strings in the legacy Python shape:
/// `focus=<focusNode> | path=<resultPath> | msg=<message> | source=<file>`.
/// An empty list means the DSL graph conforms.
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
        let path = std::env::temp_dir().join(format!("{name}_{}", std::process::id()));
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(contents.as_bytes()).unwrap();
        path
    }

    fn simple_shapes() -> String {
        r#"@prefix sh: <http://www.w3.org/ns/shacl#> .
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
        std::fs::remove_file(&ttl).ok();
        assert!(violations.is_empty(), "got {violations:?}");
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
        let msg = violations.join("\n");
        std::fs::remove_file(&ttl).ok();
        assert!(msg.contains("focus=https://example.org/bob"), "{msg}");
        assert!(msg.contains("path=https://example.org/name"), "{msg}");
        assert!(msg.contains("msg=Missing ex:name"), "{msg}");
        assert!(msg.contains("source="), "{msg}");
    }

    #[test]
    fn parse_error_hard_fails_with_path() {
        let bad = write_tmp("gmeow_py_dsl_bad_syntax.ttl", "this is not turtle @@@ <<<");
        let err =
            validate_dsl_shacl_inner(&[path_arg(&bad)], &simple_shapes()).expect_err("must fail");
        std::fs::remove_file(&bad).ok();
        assert!(err.contains(&bad.display().to_string()), "{err}");
    }

    #[test]
    fn legacy_format_sample_matches_deleted_python_output() {
        let ttl = write_tmp(
            "gmeow_py_dsl_legacy_sample.ttl",
            "@prefix ex: <https://example.org/> .\n\
             ex:bob a ex:Thing .\n",
        );
        let violations =
            validate_dsl_shacl_inner(&[path_arg(&ttl)], &simple_shapes()).expect("must succeed");
        let expected = format!(
            "focus=https://example.org/bob | path=https://example.org/name | \
             msg=Missing ex:name | source={}",
            ttl.display()
        );
        std::fs::remove_file(&ttl).ok();
        assert_eq!(violations, vec![expected]);
    }

    fn path_arg(path: &std::path::Path) -> String {
        path.to_string_lossy().into_owned()
    }
}
