// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared native DSL SHACL validation engine (#937, GAP-001).
//!
//! This module is the single Rust authority for validating merged Turtle DSL
//! sources (mapping, statement, or test DSL) against a SHACL shapes graph. It
//! is called from both the standalone Python seam ([`crate::py_dsl`]) and the
//! full validation orchestration ([`crate::validate_all`]), eliminating the
//! duplicated parse/merge/shape-store/provenance logic that previously existed
//! in those two call sites.

use std::collections::HashMap;
use std::path::PathBuf;

use gmeow_diagnostics::{Finding, Location, Severity};
use oxigraph::model::Term;

use crate::dsl;
use crate::findings::finding_from_shacl;
use crate::store;

/// Validate a merged set of DSL Turtle sources against a SHACL shapes graph.
///
/// `paths` are processed in order; the first file in which a named subject
/// appears is recorded as its source file. All triples are merged into one
/// graph and validated against the native `gmeow_shacl` engine using
/// `shapes_ttl`.
///
/// Each non-conforming result is converted to a structured [`Finding`], with
/// the authored source file attached as the primary physical location for
/// named focus nodes. The returned findings carry `tool = "{label}-dsl"` and
/// use the label in the fallback non-conforming code.
///
/// # Errors
///
/// Returns `Err(message)` on parse, merge, store-build, or shape-parse
/// failures — a hard fail, never a silent conformant result (P11/§11).
pub fn validate_dsl(
    paths: &[PathBuf],
    shapes_ttl: &str,
    label: &str,
) -> Result<Vec<Finding>, String> {
    let merge = dsl::merge_with_provenance(paths)?;
    let data_store = store::build_store_from_nt(&merge.data_nt)?;
    let shapes = gmeow_shacl::engine::parse_shapes(shapes_ttl)?;
    let report = gmeow_shacl::engine::validate(&data_store, &shapes);
    let focus_to_file: HashMap<String, String> = merge.focus_to_file.into_iter().collect();
    Ok(dsl_findings(&report, &focus_to_file, label))
}

/// Convert a DSL SHACL validation report into structured findings.
///
/// Source files are attributed via `focus_to_file`: whenever the focus node is
/// a named IRI present in the map, the source path becomes the primary
/// [`Location::path`]. Blank-node focus nodes carry no source attribution.
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

    // Preserve the original "non-conforming with no results" guard so a failed
    // graph never validates silently when the engine reports zero results.
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
