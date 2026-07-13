// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The single proof-carrying enrichment entry point over a consumer diagnostic
//! [`Report`] — attaches rule identity (catalog help URIs) and DSL-authored
//! remediation to every finding.
//!
//! Both the CLI validate/verify path
//! ([`crate::data_validate::run`](crate::data_validate::run)) and the pipeline
//! validate stage (`crates/pipeline/src/stages/validate.rs`) call
//! [`enrich_findings`] over their respective reports so the two surfaces cannot
//! drift: neither one is allowed to run only part of the enrichment pass.

use gmeow_errors::Report;

/// The single proof-carrying enrichment pass over a consumer diagnostic report:
/// attaches rule identity (help URIs) and DSL-authored remediation to every finding.
/// Reused by the CLI validate/verify path and the pipeline validate stage so the two
/// surfaces cannot drift. Honest absence: a code with no authored remediation carries none.
pub fn enrich_findings(report: &mut Report) {
    crate::rule_catalog::populate_rules(report);
    crate::remediation::attach_remediations(report);
}
