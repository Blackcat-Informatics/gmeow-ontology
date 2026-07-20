// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The unified fragment-certified refutation kernel.
//!
//! The native forward chase ([`crate::reason::dl`]) decides OWL 2 consistency by
//! materializing `type(?i, owl:Nothing)` and honestly WITHHOLDS ("incomplete")
//! on the beyond-Horn constructs it cannot forward-derive — disjunction
//! case-splits, complement refutation, cardinality/nominal counting, and datatype
//! value-space counting. This kernel decides a precisely-characterized COMPLETE
//! fragment of exactly those withheld constructs and honestly withholds outside
//! it. It never loops and never guesses: a family sub-decider returns
//! [`RefutationCertificate::InFragment`] **only** when a completeness bound is
//! proven, and [`RefutationCertificate::OutOfFragment`] with a structured
//! [`FragmentBoundary`] otherwise (the least-cost-sufficient membership idiom of
//! [`crate::physical::ChaseAdmission::certify`]).
//!
//! The kernel is a registration seam: [`refute`] tries the registered per-family
//! sub-deciders in order, and the first `InFragment` wins. The datatype
//! value-space, counting, and case-split/complement deciders slot into
//! [`SUB_DECIDERS`] as they are built; until then the kernel is inert (it decides
//! nothing and materializes nothing) yet is still CALLED on every production
//! closure, so its wiring is exercised rather than dark.
//!
//! Every structured type here orders its collections with `BTreeSet`/`BTreeMap`/
//! sorted `Vec` so a certificate is a byte-stable canonical value: the native
//! contract hash and the reasoning goldens depend on that determinism.
//!
//! The membership-certificate helper ([`certify_membership`]) and the
//! ledger-boundary derivation ([`boundary_diag_ledger`]) are the forward-facing
//! kernel API the per-family sub-deciders (Tasks 3/4/5) register against; they are
//! fully exercised by this module's unit tests but not yet by the inert production
//! registry, so — like the sibling contract-hashed engine modules (`rule_ir`,
//! `seminaive`, `wellfounded`) — this module keeps a crate-internal `dead_code`
//! allowance rather than widening or prematurely wiring them.
#![allow(dead_code)]

use std::collections::BTreeSet;

use gmeow_errors::{
    Diag, DiagLedger, FindingCategory, Grade, Severity, StageId, Standpoint, register_code,
};
use purrdf::RdfDataset;

/// The certified-complete construct families the kernel decides. Each name is the
/// stable identity a family sub-decider (Tasks 3/4/5) registers under and that
/// [`crate::reason::dl::classify_coverage`] promotes on an `InFragment{Consistent}`
/// decision. The order is the canonical decider order (datatype → counting →
/// case-split); it is never derived from declaration position by accident because
/// the variants are declared in that same intended order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum FragmentFamily {
    /// Datatype value-space counting: a facet-restricted datatype whose value
    /// space is provably too small for the distinct values forced onto it.
    DatatypeValueSpace,
    /// Number/cardinality counting: `min`/`max`/exact (qualified) cardinality
    /// bounds decided by counting distinct fillers under the identity stance.
    Counting,
    /// Case-split / complement refutation: a bounded disjunction / negated class
    /// expression every branch of which closes under refutation.
    CaseSplit,
}

impl FragmentFamily {
    /// The stable kebab-case identity used in ledger codes and coverage promotion.
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::DatatypeValueSpace => "datatype-value-space",
            Self::Counting => "counting",
            Self::CaseSplit => "case-split",
        }
    }

    /// A human-readable English label for a boundary's detail message.
    const fn label(self) -> &'static str {
        match self {
            Self::DatatypeValueSpace => "datatype value-space counting",
            Self::Counting => "number/cardinality counting",
            Self::CaseSplit => "case-split / complement refutation",
        }
    }
}

/// The decided (in)consistency of an in-fragment case. Distinguishing the two
/// keeps a `consistent` decision (which promotes a family through coverage) from
/// an `inconsistent` decision (which materializes an `owl:Nothing` clash witness).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Decision {
    /// The fragment argument proves the case CONSISTENT — no clash is materialized;
    /// the deciding family is promoted from a withheld gap to `decided`.
    Consistent,
    /// The fragment argument proves the case INCONSISTENT — each
    /// [`Witness::clashes`] entry is materialized as a `type(?i, owl:Nothing)`
    /// witness the verdict reads off.
    Inconsistent,
}

/// The kind of a counted cardinality bound the fragment argument turned on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum BoundKind {
    /// A `min` / `minQualifiedCardinality` lower bound.
    Min,
    /// A `max` / `maxQualifiedCardinality` upper bound.
    Max,
    /// An exact `cardinality` / `qualifiedCardinality` bound.
    Exact,
}

/// A structured counted-cardinality bound: the shippable evidence that a counting
/// or datatype value-space argument violated a specific numeric bound on a
/// specific property. Kept as a value (never a rendered string) so a downstream
/// consumer can reason over the bound.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CountBound {
    /// Whether the bound was a lower, upper, or exact constraint.
    pub(crate) kind: BoundKind,
    /// The numeric bound value.
    pub(crate) value: usize,
    /// The property (or datatype) the bound was carried on, as a bare IRI.
    pub(crate) on_property: String,
}

/// One `type(?i, owl:Nothing, ?w)` clash an `InFragment{Inconsistent}` decision
/// materializes. It carries the individual forced empty, its world, the deciding
/// rule name, and the clash premises — exactly the shape
/// [`crate::reason::dl`]'s `add_inferred_fact` needs to record the witness with
/// full provenance. Ordered structurally so a set of clashes is byte-stable.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct NothingClash {
    /// The individual forced into `owl:Nothing`.
    pub(crate) individual: String,
    /// The named-graph world the clash holds in.
    pub(crate) world: String,
    /// The deciding rule name recorded on the materialized witness axiom.
    pub(crate) rule_name: String,
    /// The clash premises `(subject, predicate, object)`, cited on the witness.
    pub(crate) premises: Vec<(String, String, String)>,
}

/// The structured completeness evidence backing an in-fragment decision — the
/// counted individuals, the violated bound, and/or the case-split branch that
/// closed. A shippable value, NOT a display string; empty fields simply do not
/// apply to the deciding family.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct WitnessEvidence {
    /// The distinct individuals the counting / case-split argument enumerated.
    pub(crate) counted_individuals: BTreeSet<String>,
    /// The numeric bound proven violated, for a counting / datatype family.
    pub(crate) violated_bound: Option<CountBound>,
    /// The disjunction branch that closed under refutation, for a case-split
    /// family (a bare class IRI or the canonical branch key).
    pub(crate) closed_branch: Option<String>,
}

/// The structured, shippable witness of an in-fragment decision: which family
/// closed it, the `owl:Nothing` clashes it materializes (empty for a consistent
/// decision), and the completeness evidence. Deterministically ordered throughout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Witness {
    /// The certified-complete family whose sub-decider closed the case.
    pub(crate) family: FragmentFamily,
    /// The clashes materialized on an `Inconsistent` decision (empty otherwise).
    pub(crate) clashes: BTreeSet<NothingClash>,
    /// The structured completeness evidence.
    pub(crate) evidence: WitnessEvidence,
}

/// The structured reason a case lies OUTSIDE the certified-complete fragment. Free
/// of any process references — it names the construct/shape that put the case out
/// of the fragment, deterministically ordered.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum FragmentBoundary {
    /// No registered sub-decider recognized the case's shape — the kernel did not
    /// engage. The Task-2 steady state (no family decides yet) and the honest edge
    /// for any construct no decider claims.
    NoDeciderEngaged,
    /// A family's shape is present but the completeness bound could not be
    /// certified, so the case lies outside the certified-complete fragment. The
    /// `obstructions` are the deterministically-sorted structural reasons the bound
    /// did not close, mirroring [`crate::physical::ChaseAdmission::Uncertified`].
    Uncertified {
        /// The family whose completeness bound did not close.
        family: FragmentFamily,
        /// The sorted structural obstructions that blocked certification.
        obstructions: BTreeSet<String>,
    },
    /// Several sub-deciders each withheld; every disjoint per-family boundary is
    /// retained, sorted.
    Combined(BTreeSet<FragmentBoundary>),
}

impl FragmentBoundary {
    /// The stable kebab-case code suffix naming the boundary shape.
    fn code_suffix(&self) -> &'static str {
        match self {
            Self::NoDeciderEngaged => "no-decider-engaged",
            Self::Uncertified { .. } => "uncertified",
            Self::Combined(_) => "combined",
        }
    }

    /// A deterministic, message-INDEPENDENT structural key over the boundary's
    /// content, used as the finding focus so two distinct boundaries never
    /// hash-cons-merge and no withhold is dropped.
    fn focus_key(&self) -> String {
        match self {
            Self::NoDeciderEngaged => "no-decider-engaged".to_owned(),
            Self::Uncertified {
                family,
                obstructions,
            } => {
                let mut key = format!("uncertified\u{1f}{}", family.code());
                for obstruction in obstructions {
                    key.push('\u{1f}');
                    key.push_str(obstruction);
                }
                key
            }
            Self::Combined(inner) => {
                let mut key = String::from("combined");
                for boundary in inner {
                    key.push('\u{1f}');
                    key.push_str(&boundary.focus_key());
                }
                key
            }
        }
    }

    /// Deterministic English detail, free of any process references.
    fn detail(&self) -> String {
        match self {
            Self::NoDeciderEngaged => "no fragment sub-decider recognized the case; it lies \
                 outside the certified-complete refutation fragment"
                .to_owned(),
            Self::Uncertified {
                family,
                obstructions,
            } => format!(
                "the {} family shape is present but completeness could not be certified \
                 ({} obstruction(s): {})",
                family.label(),
                obstructions.len(),
                obstructions
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
            Self::Combined(inner) => format!(
                "{} disjoint fragment boundaries: {}",
                inner.len(),
                inner.iter().map(Self::detail).collect::<Vec<_>>().join(" | ")
            ),
        }
    }
}

/// The certificate a refutation-kernel run produces for a whole EDB.
///
/// Exactly one of two shapes: an in-fragment DECISION with its structured witness,
/// or an out-of-fragment WITHHOLD with its structured boundary. There is no third
/// "maybe" — the kernel refuses (withholds) rather than guess.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RefutationCertificate {
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

/// The least-cost-sufficient fragment-membership certificate, modeled on
/// [`crate::physical::ChaseAdmission::certify`]: a family sub-decider proposes a
/// completeness obligation as the (deterministically-sorted) `obstructions` that
/// would block a complete decision. When NONE remain, the case is admitted
/// `InFragment` and `decide` yields the decision + structured witness; otherwise
/// the obstructions refuse it into a family-scoped `OutOfFragment`. This is the
/// single "refuse rather than loop-or-guess" gate every sub-decider passes
/// through.
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

/// A registered per-family sub-decider: given the whole EDB it returns `None` when
/// the family's shape is absent (it does not engage), `Some(InFragment)` when it
/// proves a decision, or `Some(OutOfFragment)` when the shape is present but it
/// cannot certify completeness (an honest family-scoped withhold).
type SubDecider = fn(&RdfDataset) -> Option<RefutationCertificate>;

/// The registered sub-deciders, tried in order; the first `InFragment` wins.
///
/// Tasks 3/4/5 register the datatype value-space, counting, and
/// case-split/complement deciders here. Task 2 registers NONE, so the kernel is
/// inert: [`refute`] returns `OutOfFragment{NoDeciderEngaged}` for every input and
/// materializes nothing — a strict no-op on real closures.
const SUB_DECIDERS: &[SubDecider] = &[];

/// Decide the certified-complete refutation fragment for `edb`.
///
/// Tries the registered [`SUB_DECIDERS`] in order and returns the first
/// `InFragment` decision. When none decides, the withholds are combined into one
/// `OutOfFragment` boundary (a single family's `Uncertified`, or `NoDeciderEngaged`
/// when nothing engaged, or `Combined` when several families each withheld).
pub(crate) fn refute(edb: &RdfDataset) -> RefutationCertificate {
    refute_with(edb, SUB_DECIDERS)
}

/// The registry-parameterized core of [`refute`], so a test can drive it with a
/// toy decider slice without registering one into production.
fn refute_with(edb: &RdfDataset, deciders: &[SubDecider]) -> RefutationCertificate {
    let mut boundaries: BTreeSet<FragmentBoundary> = BTreeSet::new();
    for decider in deciders {
        match decider(edb) {
            Some(certificate @ RefutationCertificate::InFragment { .. }) => return certificate,
            Some(RefutationCertificate::OutOfFragment { reason }) => {
                boundaries.insert(reason);
            }
            None => {}
        }
    }
    let reason = match boundaries.len() {
        0 => FragmentBoundary::NoDeciderEngaged,
        1 => boundaries
            .into_iter()
            .next()
            .expect("a length-1 set yields one element"),
        _ => FragmentBoundary::Combined(boundaries),
    };
    RefutationCertificate::OutOfFragment { reason }
}

/// The ledger category stamped on a refutation-kernel boundary finding.
///
/// A sibling of [`crate::reason::ledger::EXISTENTIAL_CHASE_CATEGORY`], DISJOINT
/// from every DL/EL crosscheck category (`"subsumption"`, `"consistency"`,
/// `"external-corpus"`), so a kernel withhold is scoped OUT of the committed DL/EL
/// crosscheck corpus whose gate asserts `gapCount == 0` (that corpus reconstructs
/// its gaps from the shared model's unsupported constructs, never from these
/// boundary findings).
pub(crate) const REFUTATION_KERNEL_CATEGORY: &str = "refutation-kernel";

/// The [`StageId`] every refutation-kernel boundary witness is attached under.
const REFUTATION_KERNEL_STAGE: &str = "reason.refutation-kernel";

/// The ASCII unit separator (`U+001F`) joining a boundary's structural fields into
/// a message-independent finding focus. It cannot occur in an IRI, a family code,
/// or an obstruction label, so the joined key is unambiguous.
const FOCUS_SEP: &str = "\u{1f}";

/// Derive a ledger-identified boundary finding from an `OutOfFragment` reason.
///
/// Mirrors [`crate::reason::ledger::divergence_diag_ledger`]: it interns the
/// structured boundary into a fresh [`DiagLedger`] through the single diagnostics
/// substrate, stamped with [`REFUTATION_KERNEL_CATEGORY`] so the withhold stays
/// OUT of the `gapCount == 0` DL/EL crosscheck. A fragment boundary is an honest
/// "outside the certified fragment" — a [`FindingCategory::UnsupportedSemanticFeature`],
/// which is Coherent and can NEVER gate — so surfacing a kernel withhold can never
/// fail a lane (it is scoped out by BOTH its Coherent category and its disjoint
/// [`REFUTATION_KERNEL_CATEGORY`]).
pub(crate) fn boundary_diag_ledger(reason: &FragmentBoundary) -> DiagLedger {
    let mut ledger = DiagLedger::new();
    let stage = StageId::new(REFUTATION_KERNEL_STAGE);
    let code = register_code(&format!(
        "reason.{REFUTATION_KERNEL_CATEGORY}.{}",
        reason.code_suffix()
    ));
    let grade = Grade::new(
        Severity::Info,
        FindingCategory::UnsupportedSemanticFeature,
        Standpoint::Binding,
    );
    let focus = [REFUTATION_KERNEL_CATEGORY, reason.focus_key().as_str()].join(FOCUS_SEP);
    let diag = Diag::new(code, grade, reason.detail()).with_focus(focus);
    ledger.attach(diag, stage);
    ledger
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::GateVerdict;
    use purrdf::RdfDatasetBuilder;

    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

    fn empty_edb() -> std::sync::Arc<RdfDataset> {
        RdfDatasetBuilder::new()
            .freeze()
            .expect("an empty dataset is valid")
    }

    /// A toy always-decidable sub-decider: it proves a fixed INCONSISTENT case,
    /// exercising the `InFragment{Inconsistent}` path with a structured witness.
    fn toy_inconsistent(_edb: &RdfDataset) -> Option<RefutationCertificate> {
        let clash = NothingClash {
            individual: "http://ex/i".to_owned(),
            world: "http://ex/w".to_owned(),
            rule_name: "refute:toy-counting".to_owned(),
            premises: vec![(
                "http://ex/i".to_owned(),
                RDF_TYPE.to_owned(),
                "http://ex/A".to_owned(),
            )],
        };
        Some(certify_membership(
            FragmentFamily::Counting,
            BTreeSet::new(),
            || {
                (
                    Decision::Inconsistent,
                    Witness {
                        family: FragmentFamily::Counting,
                        clashes: [clash].into_iter().collect(),
                        evidence: WitnessEvidence {
                            counted_individuals: ["http://ex/a".to_owned(), "http://ex/b".to_owned()]
                                .into_iter()
                                .collect(),
                            violated_bound: Some(CountBound {
                                kind: BoundKind::Max,
                                value: 1,
                                on_property: "http://ex/p".to_owned(),
                            }),
                            closed_branch: None,
                        },
                    },
                )
            },
        ))
    }

    /// A toy sub-decider whose family shape is present but which cannot certify
    /// completeness — it withholds with a structured, sorted boundary.
    fn toy_withholds(_edb: &RdfDataset) -> Option<RefutationCertificate> {
        let obstructions: BTreeSet<String> = [
            "unbounded max cardinality on <http://ex/p>".to_owned(),
            "min 2 > max 1 on <http://ex/q>".to_owned(),
        ]
        .into_iter()
        .collect();
        Some(certify_membership(
            FragmentFamily::Counting,
            obstructions,
            || unreachable!("a withhold never decides"),
        ))
    }

    // (4a) A hand-built in-fragment case yields `InFragment` with the correct
    // decision and a deterministic structured witness.
    #[test]
    fn in_fragment_case_yields_decision_and_structured_witness() {
        let edb = empty_edb();
        let certificate = refute_with(edb.as_ref(), &[toy_inconsistent]);
        let RefutationCertificate::InFragment { decision, witness } = certificate else {
            panic!("the toy decider must land in-fragment: {certificate:?}");
        };
        assert_eq!(decision, Decision::Inconsistent);
        assert_eq!(witness.family, FragmentFamily::Counting);
        // The structured witness carries the counted individuals, the violated
        // bound, and the clash — never a rendered string.
        assert_eq!(witness.clashes.len(), 1);
        let clash = witness.clashes.iter().next().expect("one clash");
        assert_eq!(clash.individual, "http://ex/i");
        assert_eq!(clash.world, "http://ex/w");
        assert_eq!(
            witness.evidence.counted_individuals,
            ["http://ex/a".to_owned(), "http://ex/b".to_owned()]
                .into_iter()
                .collect()
        );
        assert_eq!(
            witness.evidence.violated_bound,
            Some(CountBound {
                kind: BoundKind::Max,
                value: 1,
                on_property: "http://ex/p".to_owned(),
            })
        );
    }

    // (4b) An out-of-fragment case yields `OutOfFragment` (never a decision) with a
    // ledger-identified boundary that can never gate.
    #[test]
    fn out_of_fragment_case_yields_ledger_identified_boundary() {
        let edb = empty_edb();
        let certificate = refute_with(edb.as_ref(), &[toy_withholds]);
        let RefutationCertificate::OutOfFragment { reason } = &certificate else {
            panic!("a withhold must never be a decision: {certificate:?}");
        };
        assert!(matches!(
            reason,
            FragmentBoundary::Uncertified {
                family: FragmentFamily::Counting,
                ..
            }
        ));

        // The boundary derives a ledger-identified finding stamped with the
        // disjoint kernel category, at the Coherent UnsupportedSemanticFeature
        // grade, so it can NEVER gate the DL/EL crosscheck.
        let ledger = boundary_diag_ledger(reason);
        let findings = ledger.findings("reason");
        assert_eq!(findings.len(), 1, "one boundary finding: {findings:?}");
        let finding = &findings[0];
        assert_eq!(
            finding.category,
            Some(FindingCategory::UnsupportedSemanticFeature)
        );
        assert!(
            finding.code.contains(REFUTATION_KERNEL_CATEGORY),
            "code carries the disjoint kernel category: {}",
            finding.code
        );
        assert_eq!(
            ledger.verdict(),
            GateVerdict::Collected,
            "a kernel boundary is Coherent and can never gate"
        );

        // The kernel category is disjoint from every DL/EL crosscheck category.
        assert_ne!(REFUTATION_KERNEL_CATEGORY, "consistency");
        assert_ne!(REFUTATION_KERNEL_CATEGORY, "subsumption");
        assert_ne!(REFUTATION_KERNEL_CATEGORY, "external-corpus");
        assert_ne!(
            REFUTATION_KERNEL_CATEGORY,
            crate::reason::ledger::EXISTENTIAL_CHASE_CATEGORY
        );
    }

    // The Task-2 production registry decides nothing: an empty decider slice (and
    // the real `refute`) withholds with `NoDeciderEngaged` — the inert steady
    // state that keeps every current verdict unchanged.
    #[test]
    fn empty_registry_withholds_no_decider_engaged() {
        let edb = empty_edb();
        assert_eq!(
            refute_with(edb.as_ref(), &[]),
            RefutationCertificate::OutOfFragment {
                reason: FragmentBoundary::NoDeciderEngaged,
            }
        );
        assert_eq!(
            refute(edb.as_ref()),
            RefutationCertificate::OutOfFragment {
                reason: FragmentBoundary::NoDeciderEngaged,
            },
            "the registered SUB_DECIDERS are empty in Task 2, so production is inert"
        );
    }

    // Two withholding families combine into a sorted `Combined` boundary.
    #[test]
    fn multiple_withholds_combine_deterministically() {
        fn toy_case_split(_edb: &RdfDataset) -> Option<RefutationCertificate> {
            Some(certify_membership(
                FragmentFamily::CaseSplit,
                ["unbounded disjunction".to_owned()].into_iter().collect(),
                || unreachable!(),
            ))
        }
        let edb = empty_edb();
        let certificate = refute_with(edb.as_ref(), &[toy_withholds, toy_case_split]);
        let RefutationCertificate::OutOfFragment {
            reason: FragmentBoundary::Combined(inner),
        } = &certificate
        else {
            panic!("two withholds must combine: {certificate:?}");
        };
        assert_eq!(inner.len(), 2, "one boundary per withholding family");
    }

    // (4c) Determinism: the same input yields byte-identical certificate output
    // across two runs (canonical `BTreeSet`/sorted ordering makes the Debug
    // rendering byte-stable).
    #[test]
    fn certificate_output_is_byte_identical_across_runs() {
        let edb = empty_edb();
        let first = format!("{:?}", refute_with(edb.as_ref(), &[toy_inconsistent]));
        let second = format!("{:?}", refute_with(edb.as_ref(), &[toy_inconsistent]));
        assert_eq!(first, second, "in-fragment certificate must be byte-stable");

        let first_boundary = format!("{:?}", refute_with(edb.as_ref(), &[toy_withholds]));
        let second_boundary = format!("{:?}", refute_with(edb.as_ref(), &[toy_withholds]));
        assert_eq!(
            first_boundary, second_boundary,
            "out-of-fragment boundary must be byte-stable"
        );
    }
}
