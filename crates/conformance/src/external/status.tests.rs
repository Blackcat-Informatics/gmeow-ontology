// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn szs_inconsistent_branch() {
    for t in ["Theorem", "Unsatisfiable", "ContradictoryAxioms"] {
        assert_eq!(
            outcome_for_szs(t).unwrap(),
            ExternalOutcome::Inconsistent,
            "{t}"
        );
        assert_eq!(
            outcome_for_szs(t).unwrap().verdict_status(),
            VerdictStatus::Inconsistent
        );
    }
}

#[test]
fn szs_consistent_branch() {
    for t in ["Satisfiable", "CounterSatisfiable"] {
        assert_eq!(
            outcome_for_szs(t).unwrap(),
            ExternalOutcome::Consistent,
            "{t}"
        );
    }
}

#[test]
fn szs_incomplete_branch() {
    for t in ["Unknown", "GaveUp", "Timeout", "ResourceOut"] {
        assert_eq!(
            outcome_for_szs(t).unwrap(),
            ExternalOutcome::Incomplete,
            "{t}"
        );
    }
}

#[test]
fn unknown_szs_token_hard_fails() {
    let err = outcome_for_szs("Banana").unwrap_err();
    assert!(
        err.message().contains("unknown TPTP SZS status token"),
        "{err}"
    );
    // No casing leniency — SZS tokens are case-sensitive.
    assert!(outcome_for_szs("theorem").is_err());
}
