// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Developer-MCP diagnostic kinds.
//!
//! The four repo-reading tools fail hard, never softly: a reasoner that will not
//! run, a Constitution that will not read. Each is a
//! [`DiagKind`](gmeow_errors::DiagKind) under the `mcp-dev.*` code namespace, kept
//! DISTINCT from `gmeow-mcp`'s `mcp.*` catalog so a failure of a repo-anchored tool
//! is greppable as such — a consumer server can never raise one of these.
//!
//! [`MCP_DEV_DIAG_CODES`] and [`register_all`] are this crate's single, complete
//! catalog.

use gmeow_errors::{Code, FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

define_diag_kind! {
    /// A hard defect raised by one of the repo-reading MCP developer tools: the
    /// native reasoner refusing the bundle's carrier graph, or the checked-out
    /// Constitution failing to read.
    pub struct McpDev { message: String }
    code = "mcp-dev.error";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "mcp dev tool error: {}", message;
    failure_class = "https://blackcatinformatics.ca/logic/DevToolSurfaceFailure";
}

/// The complete developer-MCP diagnostic-code catalog, in registration order.
pub const MCP_DEV_DIAG_CODES: &[&str] = &[McpDev::CODE];

/// Eagerly intern every developer-MCP diagnostic code (idempotent).
pub fn register_all() -> Vec<Code> {
    vec![McpDev::register()]
}

#[path = "error.tests.rs"]
#[cfg(test)]
mod tests;
