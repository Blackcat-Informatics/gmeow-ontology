// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! TPTP SZS status ingestion.
//!
//! Reads the TPTP result line — `% SZS status <Status> [for <name>]` — and maps the
//! status token onto a normalized [`ExternalOutcome`] via the shared
//! [`crate::external::status`] table. To subsume real-world prover output we accept
//! both the spaced `% SZS status` and the no-space `%SZS status` comment forms (some
//! tooling emits the latter). Hard-fail (no-optionality): a source with no
//! `SZS status` line, or with an unrecognised status token, is an error.

use gmeow_errors::Diag;

use crate::error::SzsStatus;
use crate::external::status::{ExternalOutcome, outcome_for_szs};

/// Extract the raw SZS status token from a TPTP result document.
///
/// Matches the first line of the form `% SZS status <Token>` (optionally followed
/// by `for <name>` and other result-context fields, which are ignored). Returns the
/// bare token (e.g. `"Theorem"`).
///
/// # Errors
/// Returns `Err` when no `% SZS status` line is present or the line carries no token.
pub fn parse_szs_status(source: &str) -> gmeow_errors::Result<String> {
    for line in source.lines() {
        // The SZS result line is a TPTP comment: `% SZS status <Token> [for <name>]`.
        // Strip the leading `%` comment marker, then token-split the remainder so BOTH
        // `% SZS status X` and `%SZS status X` reduce to `[SZS, status, X]`. Token-split
        // also folds whitespace runs and a trailing-trimmed token, and keeps
        // `% SZS statusX` from false-matching (its second token is `statusX`, not
        // `status`).
        let Some(rest) = line.trim_start().strip_prefix('%') else {
            continue;
        };
        let mut it = rest.split_whitespace();
        if it.next() == Some("SZS") && it.next() == Some("status") {
            return match it.next() {
                Some(token) => Ok(token.to_string()),
                None => Err(Diag::of_kind(SzsStatus {
                    detail: "malformed `% SZS status` line: no status token".to_string(),
                })),
            };
        }
    }
    Err(Diag::of_kind(SzsStatus {
        detail: "no `% SZS status` line found in the TPTP source".to_string(),
    }))
}

/// Parse a TPTP SZS source and map it onto a normalized [`ExternalOutcome`].
pub fn outcome_from_szs(source: &str) -> gmeow_errors::Result<ExternalOutcome> {
    outcome_for_szs(&parse_szs_status(source)?)
}

#[path = "szs.tests.rs"]
#[cfg(test)]
mod tests;
