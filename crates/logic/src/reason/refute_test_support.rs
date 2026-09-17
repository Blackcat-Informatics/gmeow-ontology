// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Isolated synthetic-family helpers; production uses the native joint engine.
use super::*;

/// Test-only decision summary for the isolated selected-family controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Decision {
    /// The fragment argument proves the case CONSISTENT — no clash is materialized;
    /// the deciding family is promoted from a withheld gap to `decided`.
    Consistent,
    /// The fragment argument proves the case INCONSISTENT — each
    /// [`Witness::clashes`] entry is materialized as a `type(?i, owl:Nothing)`
    /// witness the verdict reads off.
    Inconsistent,
}

/// Test-only counted bound used by selected-family witness controls.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CountBound {
    /// Whether the bound was a lower, upper, or exact constraint.
    pub kind: BoundKind,
    /// The source bound, independent of host pointer width or candidate count.
    pub value: u128,
    /// The property (or datatype) the bound was carried on, as a bare IRI.
    pub on_property: String,
}

/// Test-only local-clash projection of a selected family's actual proof.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NothingClash {
    /// The individual forced into `owl:Nothing`.
    pub individual: String,
    /// The named-graph world the clash holds in.
    pub world: String,
    /// The deciding rule name recorded on the materialized witness axiom.
    pub rule_name: String,
    /// The clash premises `(subject, predicate, object)`, cited on the witness.
    pub premises: Vec<(String, String, String)>,
}

/// Test-only witness summary retaining native contextual and source evidence.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct WitnessEvidence {
    /// Source-owned conflicts, retained even when no local membership follows.
    pub contextual_conflicts: Vec<ContextualConflict>,
    /// Input/capability evidence from other selected worlds; never a semantic clash.
    pub source_boundaries: Vec<FragmentBoundary>,
    /// The distinct individuals the counting / case-split argument enumerated.
    pub counted_individuals: BTreeSet<String>,
    /// The numeric bound proven violated, for a counting / datatype family.
    pub violated_bound: Option<CountBound>,
    /// The disjunction branch that closed under refutation, for a case-split
    /// family (a bare class IRI or the canonical branch key).
    pub closed_branch: Option<String>,
}

/// Test-only selected-family summary; production retains native proof identities.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Witness {
    /// The certified-complete family whose sub-decider closed the case.
    pub family: FragmentFamily,
    /// The clashes materialized on an `Inconsistent` decision (empty otherwise).
    pub clashes: BTreeSet<NothingClash>,
    /// The structured completeness evidence.
    pub evidence: WitnessEvidence,
}

/// Test-only decision/boundary sum for isolated selected-family controls.
/// The production result retains every family outcome instead of this summary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefutationCertificate {
    /// The case lies inside the certified-complete fragment; `decision` is the
    /// proven (in)consistency and `witness` is its structured evidence.
    InFragment {
        /// The proven (in)consistency.
        decision: Decision,
        /// The structured, shippable witness.
        witness: Witness,
    },
    /// The case lies outside the certified-complete fragment; `reason` is the
    /// structured boundary (which shape put it out).
    OutOfFragment {
        /// The structured withhold reason.
        reason: FragmentBoundary,
    },
}

/// Convert an isolated test algorithm's explicit obligations to its test summary.
/// Production admission and execution retain the shared native outcomes directly.
pub(crate) fn certify_membership(
    family: FragmentFamily,
    obstructions: BTreeSet<String>,
    decide: impl FnOnce() -> (Decision, Witness),
) -> RefutationCertificate {
    if obstructions.is_empty() {
        let (decision, witness) = decide();
        debug_assert_eq!(
            witness.family, family,
            "a sub-decider's witness family must match the certified family"
        );
        RefutationCertificate::InFragment { decision, witness }
    } else {
        RefutationCertificate::OutOfFragment {
            reason: FragmentBoundary::Uncertified {
                family,
                obstructions,
            },
        }
    }
}
