// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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
fn parses_status_without_space_after_percent() {
    // `%SZS status X` (no space after the comment marker) is emitted by some
    // tooling — subsume it rather than hard-fail on a genuinely-decided problem.
    let src = "%SZS status Theorem for tiny\n";
    assert_eq!(parse_szs_status(src).unwrap(), "Theorem");
    assert_eq!(
        outcome_from_szs(src).unwrap(),
        ExternalOutcome::Inconsistent
    );
}

#[test]
fn parses_status_with_extra_internal_and_leading_whitespace() {
    let src = "   %   SZS   status   Satisfiable\n";
    assert_eq!(parse_szs_status(src).unwrap(), "Satisfiable");
}

#[test]
fn statusx_does_not_false_match() {
    // `% SZS statusX` is not a status line (second token is `statusX`, not
    // `status`) — it must still hard-fail rather than parse `X` / `for` as a token.
    let err = parse_szs_status("% SZS statusX Theorem\n").unwrap_err();
    assert!(err.message().contains("no `% SZS status` line"), "{err}");
}

#[test]
fn missing_status_line_hard_fails() {
    let err = parse_szs_status("fof(a, axiom, p).\n").unwrap_err();
    assert!(err.message().contains("no `% SZS status` line"), "{err}");
}

#[test]
fn empty_token_hard_fails() {
    let err = parse_szs_status("% SZS status \n").unwrap_err();
    assert!(err.message().contains("no status token"), "{err}");
}

#[test]
fn unknown_token_propagates_hard_fail() {
    assert!(outcome_from_szs("% SZS status Bogus\n").is_err());
}
