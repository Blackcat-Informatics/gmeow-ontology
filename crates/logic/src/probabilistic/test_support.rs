// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only support retained outside the production source inventory.

use super::*;

impl ProbStatus {
    /// Canonical lowercase wire string (retained only for the [`prob_status_string`]
    /// round-trip cross-check; the public status projects from the typed result).
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ProbStatus::Ok => "ok",
            ProbStatus::Unknown => "unknown",
        }
    }
}
