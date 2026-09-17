// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only support retained outside the production source inventory.

use super::*;

impl CfStatus {
    /// Canonical lowercase serialization (the historical conformance answer string).
    /// Retained only for the [`cf_status_string`] round-trip cross-check (the public
    /// status now projects from the typed result).
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            CfStatus::Ok => "ok",
            CfStatus::Partial => "partial",
            CfStatus::Exhausted => "exhausted",
            CfStatus::Unknown => "unknown",
            CfStatus::Incomplete => "incomplete",
        }
    }
}
