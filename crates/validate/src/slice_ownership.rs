// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3-free projection of the native `gmeow-slice` `OwnershipReport` into
//! canonical diagnostics `Finding`s.
//!
//! `gmeow-slice` runs the ownership + dependency analysis and produces a fully
//! structured [`OwnershipReport`] (`OwnershipDiagnostic` kinds, the offending
//! term/slice IRIs, edge evidence). Until now the Python boundary
//! (`validate.native_ownership_errors`) collapsed it to a `Vec<String>` and merged
//! the flat strings into `ValidationResult.errors` — a mid-pipeline fidelity trim
//! (`.goals` MAXIMAL INFORMATION FLOW). This module — which `gmeow-validate`
//! already links alongside both `gmeow-slice` and `gmeow-errors`, so no new
//! crate edge is introduced — projects the report into `Finding`s instead,
//! preserving the structure and the validate-gate severity split.
//!
//! Severity preserves the gate exactly: ownership **defects** (a term claimed by
//! multiple slices, a declared≠physical mismatch, an unowned term) are `Error` —
//! they keep `make validate` failing as the retired strings did. Undeclared /
//! stale dependency **observations** are ALSO `Error` (R4/AC1): an undeclared
//! cross-slice semantic reference or a stale declaration is a real ownership-
//! discipline violation, not a soft advisory, and must fail `make validate` too
//! — the one exception is a covered co-foundational grounding peering crossing
//! a registered seam, which [`crate::slice_peerage::peerage_aware_ownership_findings`]
//! suppresses before this severity is ever seen at a gate site. Only
//! unparsable-query stays `Warning` — a parse failure is a diagnostic-quality
//! gap in the analyzer's own query reading, not an authored-content defect.

use gmeow_errors::{Finding, Location, Severity};
use purrdf::slice::{OwnershipDiagnostic, OwnershipReport, OwnershipStatus};

/// The tool / SARIF-rule namespace for every slice-ownership finding.
const TOOL: &str = "slice-ownership";

/// Project an [`OwnershipReport`] into canonical findings, deterministically
/// ordered (the `Report` re-sorts by severity, but a stable per-code/message
/// order keeps the projection itself reproducible).
pub fn ownership_findings(report: &OwnershipReport) -> Vec<Finding> {
    let mut findings = Vec::new();

    // Unowned terms are surfaced via the per-term ownership table status (exactly
    // as the retired `ownership_errors()` did); the analyzer ALSO emits an
    // `OwnershipDiagnostic::Unowned`, which is skipped below to avoid double count.
    let mut unowned: Vec<&purrdf::slice::TermOwnership> = report
        .ownership
        .values()
        .filter(|o| matches!(o.status, OwnershipStatus::Unowned))
        .collect();
    unowned.sort_by(|a, b| a.term.as_str().cmp(b.term.as_str()));
    for owner in unowned {
        findings.push(finding(
            Severity::Error,
            crate::codes::SLICE_OWNERSHIP_UNOWNED,
            format!(
                "{term} rdfs:isDefinedBy {declared} — no slice physically \
                 defines this term",
                term = owner.term.as_str(),
                declared = owner.declared_owner,
            ),
            Some(owner.term.as_str().to_string()),
        ));
    }

    for diag in &report.diagnostics {
        if let Some(f) = diagnostic_finding(diag) {
            findings.push(f);
        }
    }

    findings.sort_by(|a, b| {
        (a.severity as u8, &a.code, &a.message).cmp(&(b.severity as u8, &b.code, &b.message))
    });
    findings
}

/// Map one [`OwnershipDiagnostic`] to a finding (or `None` for `Unowned`, which
/// the ownership-table pass above already covers). `pub(crate)` so
/// [`crate::slice_peerage::peerage_aware_ownership_findings`] can reuse the exact
/// same projection for the `Uncovered` verdict (an undeclared dependency that is
/// not a co-foundational peering crossing) — the message/severity must never
/// drift between the two callers.
pub(crate) fn diagnostic_finding(diag: &OwnershipDiagnostic) -> Option<Finding> {
    match diag {
        OwnershipDiagnostic::Conflict { term, claimants } => Some(finding(
            Severity::Error,
            crate::codes::SLICE_OWNERSHIP_CONFLICT,
            format!(
                "{term} rdfs:isDefinedBy is claimed by multiple slices {claimants:?} \
                 — a term must have exactly one owning slice",
                term = term.as_str(),
            ),
            Some(term.as_str().to_string()),
        )),
        OwnershipDiagnostic::Mismatch {
            term,
            declared,
            physical,
        } => Some(finding(
            Severity::Error,
            crate::codes::SLICE_OWNERSHIP_MISMATCH,
            format!(
                "{term} rdfs:isDefinedBy {declared} — must equal the owning slice \
                 IRI {physical}",
                term = term.as_str(),
            ),
            Some(term.as_str().to_string()),
        )),
        OwnershipDiagnostic::UndeclaredDependency {
            from_slice,
            to_slice,
            edge_kind,
        } => Some(finding(
            Severity::Error,
            crate::codes::SLICE_OWNERSHIP_UNDECLARED_DEPENDENCY,
            format!(
                "{from_slice} depends on {to_slice} ({edge_kind:?}) with no \
                 gmeow:sliceDependsOn declaration",
            ),
            Some(from_slice.to_string()),
        )),
        OwnershipDiagnostic::StaleDependency {
            from_slice,
            to_slice,
        } => Some(finding(
            Severity::Error,
            crate::codes::SLICE_OWNERSHIP_STALE_DEPENDENCY,
            format!(
                "{from_slice} declares gmeow:sliceDependsOn {to_slice} but no \
                 artifact references it",
            ),
            Some(from_slice.to_string()),
        )),
        OwnershipDiagnostic::UnparseableQuery {
            slice,
            logical_path,
            message,
        } => Some(finding(
            Severity::Warning,
            crate::codes::SLICE_OWNERSHIP_UNPARSEABLE_QUERY,
            format!("{slice}: query {logical_path} failed to parse — {message}"),
            Some(slice.to_string()),
        )),
        // Covered by the ownership-table pass — do not double-count.
        OwnershipDiagnostic::Unowned { .. } => None,
    }
}

/// Build one slice-ownership finding, hanging the offending IRI off a logical
/// location for SARIF grouping. `pub(crate)` so [`crate::slice_peerage`] emits
/// its `slice-ownership.peered-unregistered-seam` finding under the same
/// `TOOL` namespace rather than forking a second SARIF-rule tool string.
pub(crate) fn finding(
    severity: Severity,
    code: &str,
    message: String,
    logical: Option<String>,
) -> Finding {
    let mut f = Finding::new(severity, code, message).with_tool(TOOL);
    if let Some(iri) = logical {
        f.add_location(Location::new(None, None, None, Some(iri)));
    }
    f
}

#[path = "slice_ownership.tests.rs"]
#[cfg(test)]
mod tests;
