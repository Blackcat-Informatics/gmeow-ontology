// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Supported contextual refutation, independently of local membership publication.

use purrdf::TermValue;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// An actual asserted source statement, including its original native context.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RefutationPremise {
    /// Original subject, with its native blank-node scope intact.
    #[serde(with = "crate::term_serde")]
    pub subject: TermValue,
    /// Exact asserted predicate IRI, before role interpretation.
    pub predicate: String,
    /// Original object, including literal or embedded-triple identity.
    #[serde(with = "crate::term_serde")]
    pub object: TermValue,
    /// Original asserting graph; `None` is the selected default context.
    #[serde(with = "crate::term_serde::optional")]
    pub graph: Option<TermValue>,
}

/// The positive contradiction that closes a tableau branch.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RefutationClash {
    /// Membership in the empty class.
    Bottom,
    /// Positive and negative membership in the same class.
    OpposedClass(String),
    /// An equality path identifies explicitly distinct resources.
    EqualityDistinctness,
    /// Membership in an explicitly empty nominal enumeration.
    EmptyEnumeration,
    /// Every alternative is contradicted by supported labels.
    ExhaustedDisjunction,
}

/// The admitted tableau expression in negation-normal form.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RefutationExpression {
    /// The universal class.
    Top,
    /// The empty class.
    Bottom,
    /// Positive membership in a named class or opaque expression node.
    Pos(String),
    /// Negative membership in a named class or opaque expression node.
    Neg(String),
    /// Conjunction of admitted expressions.
    And(Vec<Self>),
    /// Finite disjunction of admitted expressions.
    Or(Vec<Self>),
    /// Equality with at least one of these resources.
    Nominals(Vec<String>),
    /// An unsupported expression; it cannot establish a consistent model.
    Blocked,
}

/// A hypothetical alternative, separated from asserted source premises.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RefutationAssumption {
    /// The selected resource inhabits this disjunct in this branch only.
    Membership {
        /// Execution identity of the selected resource.
        subject: String,
        /// The selected finite disjunct.
        expression: RefutationExpression,
    },
    /// The nominal membership chooses this equality in this branch only.
    Equality {
        /// Execution identity of the membership subject.
        subject: String,
        /// Execution identity of the selected enumeration member.
        member: String,
    },
}

/// One closed alternative of an exhaustive finite choice.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RefutationBranch {
    /// The assumption discharged by this branch's refutation.
    pub assumption: RefutationAssumption,
    /// The supported contradiction under exactly this alternative.
    pub proof: RefutationProof,
}

/// A structural input or capability failure, never a model contradiction.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RefutationSourceIssue {
    /// The list terminator carries member or tail fields.
    MalformedNil,
    /// A selected list node has multiple distinct values for a functional field.
    ConflictingListField {
        /// The exact execution identity of the list node.
        subject: String,
        /// The selected first/rest predicate.
        predicate: String,
    },
    /// Multiple definitions require conjunctive semantics outside this admission.
    ExpressionMultiplicity {
        /// The exact execution identity of the expression owner.
        subject: String,
        /// The selected defining predicate.
        predicate: String,
    },
    /// An actual selected defining operator has a subject outside the resource
    /// class-expression fragment. Its embedded statement is never asserted.
    UnsupportedExpressionOwner {
        #[serde(with = "crate::term_serde")]
        /// Original selected owner, including its complete quoted native term.
        owner: TermValue,
        /// Actual role-selected defining predicate.
        predicate: String,
    },
    /// A selected class definition needs a resource-valued class or list operand.
    UnsupportedExpressionOperand {
        /// The execution identity of the definition owner.
        owner: String,
        /// The selected class-definition predicate.
        predicate: String,
        /// The exact native operand that this resource fragment cannot interpret.
        #[serde(with = "crate::term_serde")]
        operand: TermValue,
    },
    /// A finite class/nominal list contains a value outside this resource fragment.
    UnsupportedListMember {
        /// The exact execution identity of the definition owner.
        owner: String,
        /// The exact execution identity of the list head.
        head: String,
    },
    /// A selected list path returns to a node already on that path.
    CyclicList {
        /// The exact execution identity of the expression or declaration owner.
        owner: String,
        /// The exact execution identity of the selected list head.
        head: String,
    },
    /// A selected definition does not provide the required finite-list fields.
    IncompleteList {
        /// The exact execution identity of the expression or declaration owner.
        owner: String,
        /// The exact execution identity of the list head.
        head: String,
    },
}

impl RefutationSourceIssue {
    /// Classify the selected source requirement without parsing diagnostic prose.
    /// Malformed owned list structure invalidates input; a well-formed expression
    /// outside the implemented owner/operand fragment is a capability refusal.
    #[must_use]
    pub const fn refusal_class(&self) -> super::ClassSourceRefusal {
        match self {
            Self::MalformedNil
            | Self::ConflictingListField { .. }
            | Self::CyclicList { .. }
            | Self::IncompleteList { .. } => super::ClassSourceRefusal::Invalid,
            Self::ExpressionMultiplicity { .. }
            | Self::UnsupportedExpressionOwner { .. }
            | Self::UnsupportedExpressionOperand { .. }
            | Self::UnsupportedListMember { .. } => super::ClassSourceRefusal::Unsupported,
        }
    }

    pub(super) fn detail(&self) -> String {
        match self {
            Self::MalformedNil => "rdf:nil cannot own list member or tail fields".to_owned(),
            Self::ConflictingListField { subject, predicate } => {
                format!("list <{subject}> has multiple distinct values for <{predicate}>")
            }
            Self::ExpressionMultiplicity { subject, predicate } => format!(
                "multiple definitions of <{subject}> through <{predicate}> require admitted conjunctive expression semantics"
            ),
            Self::UnsupportedExpressionOwner { predicate, .. } => format!(
                "selected definition through <{predicate}> has a non-resource owner outside the admitted class-expression fragment"
            ),
            Self::UnsupportedExpressionOperand {
                owner, predicate, ..
            } => format!(
                "selected definition <{owner}> through <{predicate}> has a non-resource operand outside the admitted class-expression fragment"
            ),
            Self::UnsupportedListMember { owner, head } => format!(
                "selected definition <{owner}> has a non-resource member at <{head}> outside the admitted class/nominal fragment"
            ),
            Self::CyclicList { owner, head } => {
                format!("selected definition <{owner}> has a cyclic list at <{head}>")
            }
            Self::IncompleteList { owner, head } => {
                format!("selected definition <{owner}> has no complete finite list at <{head}>")
            }
        }
    }
}

/// A finite supported refutation. Branch assumptions never become source facts.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RefutationProof {
    /// This branch proves a contradiction on exactly these equal resources.
    Conflict {
        /// The closing native contradiction.
        kind: RefutationClash,
        /// Resource identities in the supported equality class.
        subjects: BTreeSet<String>,
        /// Exact shared native proof nodes supporting this branch.
        support: Vec<super::NativeProofId>,
        /// Original asserted leaves of that native support, without fabricated rows.
        premises: Vec<RefutationPremise>,
    },
    /// Every live alternative of the source-owned finite choice closed.
    /// The choice premises also include support for alternatives pruned before branching.
    Cases {
        /// Exact shared native proofs for the finite choice and eliminated alternatives.
        support: Vec<super::NativeProofId>,
        /// Complete ordered live alternative inventory before any branch executes.
        alternatives: Vec<RefutationAssumption>,
        /// Source definition and support for alternatives already eliminated.
        choice: Vec<RefutationPremise>,
        /// Each live alternative and its closed branch.
        branches: Vec<RefutationBranch>,
    },
}

impl RefutationProof {
    /// Resources whose local contradiction is justified in every closed branch.
    #[must_use]
    pub fn local_subjects(&self) -> BTreeSet<String> {
        match self {
            Self::Conflict { subjects, .. } => subjects.clone(),
            Self::Cases { branches, .. } => {
                let mut branches = branches.iter();
                let Some(first) = branches.next() else {
                    return BTreeSet::new();
                };
                branches.fold(first.proof.local_subjects(), |prior, next| {
                    prior
                        .intersection(&next.proof.local_subjects())
                        .cloned()
                        .collect()
                })
            }
        }
    }

    /// The exact native proof nodes used by every frame of the finite argument.
    pub fn native_support(&self) -> Vec<super::NativeProofId> {
        match self {
            Self::Conflict { support, .. } => support.clone(),
            Self::Cases {
                support, branches, ..
            } => support
                .iter()
                .copied()
                .chain(
                    branches
                        .iter()
                        .flat_map(|branch| branch.proof.native_support()),
                )
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        }
    }

    /// The exact source support of the whole exhaustive argument.
    #[must_use]
    pub fn premises(&self) -> BTreeSet<RefutationPremise> {
        match self {
            Self::Conflict { premises, .. } => premises.iter().cloned().collect(),
            Self::Cases {
                choice, branches, ..
            } => choice
                .iter()
                .cloned()
                .chain(branches.iter().flat_map(|branch| branch.proof.premises()))
                .collect(),
        }
    }
}

/// A model conflict remains owned by its selected world even without a local head.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ContextualConflict {
    /// The selected execution world owning the conflict.
    pub world: String,
    /// The complete finite closing argument; independent of local publication.
    pub proof: RefutationProof,
}

impl ContextualConflict {
    /// Validate the exact scope and finite framing of an authenticated native proof.
    /// This checks retained engine evidence, not arbitrary external theorem soundness.
    pub fn validate(&self, ledger: &super::NativeFamilyLedger) -> gmeow_errors::Result<()> {
        if self.world != ledger.world {
            return Err(proof_error("contextual conflict belongs to another world"));
        }
        self.proof.validate(ledger)
    }
}

fn proof_error(detail: &str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason {
        detail: detail.to_owned(),
    })
}

impl RefutationProof {
    /// Every frame names existing native support and retains its exact source leaves.
    fn validate(&self, ledger: &super::NativeFamilyLedger) -> gmeow_errors::Result<()> {
        let (support, source) = match self {
            Self::Conflict {
                subjects,
                premises,
                support,
                ..
            } => {
                if subjects.is_empty() || subjects.iter().any(String::is_empty) {
                    return Err(proof_error(
                        "closing branch has no supported resource identity",
                    ));
                }
                (support, premises)
            }
            Self::Cases {
                choice,
                support,
                alternatives,
                branches,
            } => {
                if alternatives.len() < 2
                    || alternatives.len() != branches.len()
                    || alternatives.iter().collect::<BTreeSet<_>>().len() != alternatives.len()
                    || alternatives
                        .iter()
                        .zip(branches)
                        .any(|(expected, branch)| expected != &branch.assumption)
                {
                    return Err(proof_error(
                        "closed finite choice lost an alternative or branch frame",
                    ));
                }
                for branch in branches {
                    branch.proof.validate(ledger)?;
                }
                (support, choice)
            }
        };
        if support.is_empty()
            || support.windows(2).any(|pair| pair[0] >= pair[1])
            || source.iter().any(|row| row.graph != ledger.graph)
            || ledger.source_leaves(support)? != *source
        {
            return Err(proof_error(
                "closing proof lost its exact native support or original source leaves",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "proof_test_support.rs"]
mod test_support;
