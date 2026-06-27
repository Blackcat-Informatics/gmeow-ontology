// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! TPTP SZS status ingestion (#753).
//!
//! Reads the standard TPTP result line — `% SZS status <Status> [for <name>]` —
//! and maps the status token onto a normalized [`ExternalOutcome`] via the shared
//! [`crate::external::status`] table. Hard-fail (no-optionality): a source with no
//! `% SZS status` line, or with an unrecognised status token, is an error.

use crate::external::status::{outcome_for_szs, ExternalOutcome};

/// Extract the raw SZS status token from a TPTP result document.
///
/// Matches the first line of the form `% SZS status <Token>` (optionally followed
/// by `for <name>` and other result-context fields, which are ignored). Returns the
/// bare token (e.g. `"Theorem"`).
///
/// # Errors
/// Returns `Err` when no `% SZS status` line is present or the line carries no token.
pub fn parse_szs_status(source: &str) -> Result<String, String> {
    for line in source.lines() {
        // The SZS result line is a TPTP comment: `% SZS status <Token> [for <name>]`.
        // Token-split so whitespace runs and a trailing-trimmed token are handled and
        // `% SZS statusX` cannot false-match.
        let mut it = line.split_whitespace();
        if it.next() == Some("%") && it.next() == Some("SZS") && it.next() == Some("status") {
            return match it.next() {
                Some(token) => Ok(token.to_string()),
                None => Err("malformed `% SZS status` line: no status token".to_string()),
            };
        }
    }
    Err("no `% SZS status` line found in the TPTP source".to_string())
}

/// Parse a TPTP SZS source and map it onto a normalized [`ExternalOutcome`].
pub fn outcome_from_szs(source: &str) -> Result<ExternalOutcome, String> {
    outcome_for_szs(&parse_szs_status(source)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const THEOREM: &str = "\
        % A tiny TPTP problem.\n\
        fof(a, axiom, p).\n\
        fof(c, conjecture, p).\n\
        % SZS status Theorem for tiny\n";

    #[test]
    fn parses_status_with_for_suffix() {
        assert_eq!(parse_szs_status(THEOREM).unwrap(), "Theorem");
        assert_eq!(
            outcome_from_szs(THEOREM).unwrap(),
            ExternalOutcome::Inconsistent
        );
    }

    #[test]
    fn parses_status_without_suffix() {
        let src = "% SZS status Satisfiable\n";
        assert_eq!(parse_szs_status(src).unwrap(), "Satisfiable");
        assert_eq!(outcome_from_szs(src).unwrap(), ExternalOutcome::Consistent);
    }

    #[test]
    fn missing_status_line_hard_fails() {
        let err = parse_szs_status("fof(a, axiom, p).\n").unwrap_err();
        assert!(err.contains("no `% SZS status` line"), "{err}");
    }

    #[test]
    fn empty_token_hard_fails() {
        let err = parse_szs_status("% SZS status \n").unwrap_err();
        assert!(err.contains("no status token"), "{err}");
    }

    #[test]
    fn unknown_token_propagates_hard_fail() {
        assert!(outcome_from_szs("% SZS status Bogus\n").is_err());
    }
}
