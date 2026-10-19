// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Wasm-clean SHACL finding projection shared by the repo-free Tier-1 consumer path
//! ([`crate::data_validate`]) and the native slice-authoring dev gate
//! ([`crate::validate_all`]).
//!
//! This projection carries no reasoner, filesystem, threading, or PyO3 coupling —
//! only the canonical `gmeow_errors` Finding/Report model and the SHACL → Finding
//! bridge — so they compile for `wasm32-unknown-unknown`. They live here, apart from
//! the native-only orchestration in `validate_all`, so the Tier-1 core can reuse them
//! without dragging rayon / the native reasoner into the wasm build.

use gmeow_errors::{Finding, FindingCategory, Location, Severity};

use crate::findings::{FailureClassIndex, finding_from_shacl};

/// Convert a SHACL [`ValidationReport`](purrdf::shapes::report::ValidationReport) into
/// structured findings via the [`finding_from_shacl`] bridge, optionally tagging each
/// with the example/DSL source (`origin`) as the finding's primary path so SARIF and
/// the `gmeow:` RDF projection can attribute it.
///
/// `classes` is the shapes graph's `gmeow:enforcesFailureClass` index, so every finding
/// NAMES the typed conformance failure its violated law declares.
pub(crate) fn shacl_findings_from_report(
    report: &purrdf::shapes::report::ValidationReport,
    origin: Option<&str>,
    classes: &FailureClassIndex,
) -> Vec<Finding> {
    let mut findings: Vec<Finding> = report
        .results
        .iter()
        .map(|result| {
            let mut finding = finding_from_shacl(result, classes)
                .with_category(FindingCategory::DataShapeViolation);
            // Attribute the example/DSL source file as the finding's PRIMARY
            // physical location (a repo-relative path), keeping the focus-node
            // IRI as that location's logical anchor. SARIF `artifactLocation.uri`
            // must be repo-relative — an absolute IRI is rejected by GitHub
            // code-scanning — so the file, not the IRI, is the physical artifact.
            if let Some(origin) = origin {
                if let Some(primary) = finding.locations.first_mut() {
                    primary.path = Some(origin.to_owned());
                } else {
                    finding.add_location(Location {
                        path: Some(origin.to_owned()),
                        ..Location::default()
                    });
                }
            }
            finding
        })
        .collect();
    // Preserve the original "non-conforming with no results" guard so a failed
    // graph never validates silently when the engine reports zero results.
    if findings.is_empty() && !report.conforms {
        let mut finding = Finding::new(
            Severity::Error,
            crate::codes::SHACL_NONCONFORMING,
            "SHACL validation failed: non-conforming with no results",
        )
        .with_tool("shacl")
        .with_category(FindingCategory::DataShapeViolation);
        if let Some(origin) = origin {
            finding.add_location(Location {
                path: Some(origin.to_owned()),
                ..Location::default()
            });
        }
        findings.push(finding);
    }
    findings
}
