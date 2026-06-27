// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The canonical external-result → runner-verdict mapping table (#753).
//!
//! This is THE single surface mapping third-party standard-suite outcomes (TPTP
//! SZS status tokens and W3C `mf:` entailment test kinds) onto the runner's
//! [`crate::serialize::VerdictStatus`]. Keeping the table in one module means the
//! SZS parser and the manifest parser share exactly one source of truth, and a new
//! corpus is a data addition here — never a scattered re-decision.
//!
//! The mapping (from the #753 issue body):
//!
//! | external outcome                              | runner verdict |
//! |-----------------------------------------------|----------------|
//! | `Theorem` / `Unsatisfiable` (entailment holds / no model) | `inconsistent` |
//! | `Satisfiable` / `CounterSatisfiable` (a model exists)     | `consistent`   |
//! | `Unknown` / budget-tripped (undecided)                    | `incomplete`   |

use crate::serialize::VerdictStatus;

/// A normalized external problem outcome, abstracting over the concrete SZS token
/// or W3C entailment kind. The adapter parses a source into one of these, then maps
/// it onto a [`VerdictStatus`] for the runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalOutcome {
    /// The conjecture is entailed / the axiom set has no model — the reduction
    /// `premises ∧ ¬conclusion` is UNSATISFIABLE. TPTP `Theorem` / `Unsatisfiable`
    /// / `ContradictoryAxioms`; W3C `mf:PositiveEntailment`.
    Inconsistent,
    /// A model exists / entailment fails. TPTP `Satisfiable` / `CounterSatisfiable`;
    /// W3C `mf:NegativeEntailment`.
    Consistent,
    /// The prover did not decide the problem (gave up / out of resources / unknown)
    /// — the budget-tripped branch. TPTP `Unknown` / `GaveUp` / `Timeout` /
    /// `ResourceOut`.
    Incomplete,
}

impl ExternalOutcome {
    /// The runner verdict status this outcome lowers to.
    pub fn verdict_status(self) -> VerdictStatus {
        match self {
            ExternalOutcome::Inconsistent => VerdictStatus::Inconsistent,
            ExternalOutcome::Consistent => VerdictStatus::Consistent,
            ExternalOutcome::Incomplete => VerdictStatus::Incomplete,
        }
    }
}

/// Map a TPTP SZS *status* ontology token to a normalized [`ExternalOutcome`].
///
/// Only the well-defined SZS values whose model-theoretic meaning maps cleanly are
/// recognized; an unrecognized token is a HARD error (no silent default — the case
/// author must extend this table deliberately).
pub fn outcome_for_szs(token: &str) -> Result<ExternalOutcome, String> {
    match token {
        // Entailment holds / inconsistent axiom set.
        "Theorem" | "Unsatisfiable" | "ContradictoryAxioms" => Ok(ExternalOutcome::Inconsistent),
        // A (counter-)model exists.
        "Satisfiable" | "CounterSatisfiable" => Ok(ExternalOutcome::Consistent),
        // Undecided / resource-bounded.
        "Unknown" | "GaveUp" | "Timeout" | "ResourceOut" => Ok(ExternalOutcome::Incomplete),
        other => Err(format!(
            "unknown TPTP SZS status token {other:?}; the #753 mapping table recognises \
             Theorem|Unsatisfiable|ContradictoryAxioms (inconsistent), \
             Satisfiable|CounterSatisfiable (consistent), \
             Unknown|GaveUp|Timeout|ResourceOut (incomplete)"
        )),
    }
}

#[cfg(test)]
mod tests {
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
        assert!(err.contains("unknown TPTP SZS status token"), "{err}");
        // No casing leniency — SZS tokens are case-sensitive.
        assert!(outcome_for_szs("theorem").is_err());
    }
}
