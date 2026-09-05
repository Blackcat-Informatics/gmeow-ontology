// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared native DSL SHACL validation engine.
//!
//! This module is the single Rust authority for validating merged Turtle DSL
//! sources against a SHACL shapes graph. Its sole caller is the validation
//! orchestration (`validate_all::check_dsl`), which the live `gmeow-dev validate`
//! gate drives with the committed central-DSL surfaces resolved by
//! [`crate::dsl_coverage::authored_dsl_shacl_inputs`], so merge, validation, and
//! focus-to-source provenance cannot drift.

use std::collections::HashMap;
use std::path::PathBuf;

use gmeow_errors::{Finding, Location, Severity};
use purrdf::shapes::term::Term;

use crate::dsl;
use crate::findings::{FailureClassIndex, finding_from_shacl};
use crate::store;

/// Validate a merged set of DSL Turtle sources against a SHACL shapes graph.
///
/// `paths` are processed in order. The first file in which a named subject
/// appears is recorded as that focus node's source file. Non-conforming results
/// are converted to structured findings with `tool = "{label}-dsl"`.
///
/// # Errors
///
/// Returns `Err` on parse, merge, store-build, shape-parse, or SHACL
/// failures. The DSL gate is fail-hard and never silently conforms.
pub fn validate_dsl(
    paths: &[PathBuf],
    shapes_ttl: &str,
    label: &str,
) -> gmeow_errors::Result<Vec<Finding>> {
    let merge = dsl::merge_with_provenance(paths)?;
    let shapes = purrdf::shapes::engine::parse_shapes(shapes_ttl, None).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            detail: format!("SHACL shapes failed to parse: {e}"),
        })
    })?;
    // The SAME shapes text, read a second time as a plain dataset for its
    // `gmeow:enforcesFailureClass` annotations, so a DSL finding names the typed
    // conformance failure its shape declares and not only the component code.
    let shapes_dataset = purrdf::parse_dataset(shapes_ttl.as_bytes(), "text/turtle", None)
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                detail: format!("SHACL shapes failed to parse as a dataset: {e}"),
            })
        })?;
    let classes = FailureClassIndex::from_shapes_dataset(&shapes_dataset);
    let report = store::shacl_validate_dataset(&merge.dataset, &shapes);
    let focus_to_file: HashMap<String, String> = merge.focus_to_file.into_iter().collect();
    Ok(dsl_findings(&report, &focus_to_file, label, &classes))
}

fn dsl_findings(
    report: &purrdf::shapes::report::ValidationReport,
    focus_to_file: &HashMap<String, String>,
    label: &str,
    classes: &FailureClassIndex,
) -> Vec<Finding> {
    let tool = format!("{label}-dsl");
    let mut findings: Vec<Finding> = report
        .results
        .iter()
        .map(|result| {
            let mut finding = finding_from_shacl(result, classes);
            finding.tool = Some(tool.clone());
            if let Term::NamedNode(node) = &result.focus_node
                && let Some(source) = focus_to_file.get(node.as_str())
            {
                if let Some(primary) = finding.locations.first_mut() {
                    primary.path = Some(source.clone());
                } else {
                    finding.add_location(Location {
                        path: Some(source.clone()),
                        ..Location::default()
                    });
                }
            }
            finding
        })
        .collect();

    if findings.is_empty() && !report.conforms {
        findings.push(
            Finding::new(
                Severity::Error,
                format!("{label}{}", crate::codes::DSL_NONCONFORMING_SUFFIX),
                "SHACL validation failed: non-conforming with no results",
            )
            .with_tool(tool),
        );
    }

    findings
}
