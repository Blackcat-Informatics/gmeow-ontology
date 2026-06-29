// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared native DSL SHACL validation engine (#937).
//!
//! This module is the single Rust authority for validating merged Turtle DSL
//! sources against a SHACL shapes graph. It is called from both the standalone
//! Python extension seam and the full validation orchestration, so merge,
//! validation, and focus-to-source provenance cannot drift.

use std::collections::HashMap;
use std::path::PathBuf;

use gmeow_diagnostics::{Finding, Location, Severity};
use gmeow_shacl::term::Term;

use crate::dsl;
use crate::findings::finding_from_shacl;
use crate::store;

/// Validate a merged set of DSL Turtle sources against a SHACL shapes graph.
///
/// `paths` are processed in order. The first file in which a named subject
/// appears is recorded as that focus node's source file. Non-conforming results
/// are converted to structured findings with `tool = "{label}-dsl"`.
///
/// # Errors
///
/// Returns `Err(message)` on parse, merge, store-build, shape-parse, or SHACL
/// failures. The DSL gate is fail-hard and never silently conforms.
pub fn validate_dsl(
    paths: &[PathBuf],
    shapes_ttl: &str,
    label: &str,
) -> Result<Vec<Finding>, String> {
    let merge = dsl::merge_with_provenance(paths)?;
    let data_store = store::build_store_from_nt(&merge.data_nt)?;
    let shapes = gmeow_shacl::engine::parse_shapes(shapes_ttl)?;
    let report = crate::store::shacl_validate_store(&data_store, &shapes);
    let focus_to_file: HashMap<String, String> = merge.focus_to_file.into_iter().collect();
    Ok(dsl_findings(&report, &focus_to_file, label))
}

fn dsl_findings(
    report: &gmeow_shacl::report::ValidationReport,
    focus_to_file: &HashMap<String, String>,
    label: &str,
) -> Vec<Finding> {
    let tool = format!("{label}-dsl");
    let mut findings: Vec<Finding> = report
        .results
        .iter()
        .map(|result| {
            let mut finding = finding_from_shacl(result);
            finding.tool = Some(tool.clone());
            if let Term::NamedNode(node) = &result.focus_node {
                if let Some(source) = focus_to_file.get(node.as_str()) {
                    if let Some(primary) = finding.locations.first_mut() {
                        primary.path = Some(source.clone());
                    } else {
                        finding.add_location(Location {
                            path: Some(source.clone()),
                            ..Location::default()
                        });
                    }
                }
            }
            finding
        })
        .collect();

    if findings.is_empty() && !report.conforms {
        findings.push(
            Finding::new(
                Severity::Error,
                format!("{label}-dsl.nonconforming"),
                "SHACL validation failed: non-conforming with no results",
            )
            .with_tool(tool),
        );
    }

    findings
}
