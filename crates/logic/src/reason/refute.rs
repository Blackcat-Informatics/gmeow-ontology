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
//! kernel API the per-family sub-deciders register against. The kernel is ALSO the
//! single source of truth for its own decidability surface: [`decided_fragments`]
//! and [`retained_boundaries`] enumerate, respectively, the certified-complete
//! construct families (each keyed to a [`RefutationPattern`] and a technical
//! completeness bound) and the constructs the kernel deliberately RETAINS as honest
//! withholds. `slices/grounding/logic/module.ttl` ships that surface as
//! `logic:DecidedFragment` / `logic:RefutationPattern` / `logic:expressivenessBoundary`
//! individuals, and the agreement test [`tests::module_ttl_projects_the_kernel_registry`]
//! proves the manifest is EXACTLY a projection of this registry (drift in either
//! direction fails). The production reason path consumes a family-scoped withhold
//! through [`production_boundary_findings`], routing its boundary through the
//! diagnostics substrate under [`REFUTATION_KERNEL_CATEGORY`] into
//! [`crate::reason::dl::DlVerdict::boundary_findings`], so the kernel's honest
//! "outside the certified fragment" is tied to a real verdict rather than dark.

use std::collections::BTreeSet;

use gmeow_errors::{
    Diag, DiagLedger, FindingCategory, Grade, Severity, StageId, Standpoint, register_code,
};
use gmeow_math::Rational;
use purrdf::{RdfDataset, RdfTerm};

use crate::facts::skolem_iri;

/// Family 5 — the datatype value-space sub-decider (Task 3, the first REAL family).
pub(crate) mod datatype;

/// Families 2/6a/7 — the counting / arithmetic-feasibility sub-decider (Task 4).
pub(crate) mod counting;

/// Families 1/3/6b (+ entangled Family 4) — the bounded case-split / complement /
/// union-disjoint / malformed-list sub-decider (Task 5).
pub(crate) mod casesplit;

// ── Shared term / world / value helpers (used by every family sub-decider) ──────

/// The XSD namespace prefix, shared by the datatype value-space and counting
/// deciders' rational-tower classification.
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

/// Canonicalize an RDF term into its resource key: the IRI itself, or a stable
/// skolem IRI for a blank node. `None` for a literal or RDF-star triple term
/// (neither names a resource).
pub(crate) fn resource_key(term: &RdfTerm) -> Option<String> {
    match term {
        RdfTerm::Iri(iri) => Some(iri.clone()),
        RdfTerm::BlankNode(id) => Some(skolem_iri(id)),
        RdfTerm::Literal(_) | RdfTerm::Triple(_) => None,
    }
}

/// Canonicalize a quad's named-graph term into its world key: the IRI itself, a
/// stable skolem IRI for a blank node, or the default world when absent.
pub(crate) fn world_key(graph: &Option<RdfTerm>) -> String {
    match graph {
        Some(RdfTerm::Iri(iri)) => iri.clone(),
        Some(RdfTerm::BlankNode(id)) => skolem_iri(id),
        _ => crate::reason::rl::DEFAULT_WORLD.to_owned(),
    }
}

/// Parse an `owl:rational` lexical form (`num/den` or an integer) into an exact
/// [`Rational`].
pub(crate) fn parse_rational(text: &str) -> Option<Rational> {
    if let Some((num, den)) = text.split_once('/') {
        let num: i128 = num.trim().parse().ok()?;
        let den: i128 = den.trim().parse().ok()?;
        Rational::new(num, den).ok()
    } else {
        Rational::parse_decimal(text).ok()
    }
}

/// Whether `dt` is a member of the `xsd:decimal`/`xsd:integer` tower the exact-ℚ
/// value space models.
pub(crate) fn is_rational_tower(dt: &str) -> bool {
    matches!(
        dt.strip_prefix(XSD),
        Some(
            "decimal"
                | "integer"
                | "long"
                | "int"
                | "short"
                | "byte"
                | "nonNegativeInteger"
                | "positiveInteger"
                | "nonPositiveInteger"
                | "negativeInteger"
                | "unsignedLong"
                | "unsignedInt"
                | "unsignedShort"
                | "unsignedByte"
        )
    )
}

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
    // The `max`-bound evidence variant of the shippable [`CountBound`] value. The
    // datatype value-space decider currently emits only `Min`/`Exact` bounds and the
    // counting decider reads maxima structurally rather than minting a `CountBound`,
    // so `Max` is exercised through the kernel's own unit tests only; it stays a
    // first-class variant because a `CountBound` is a shippable evidence value a
    // downstream consumer reasons over, and a max-cardinality violation is a real one.
    #[allow(dead_code)]
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
                obstructions.iter().cloned().collect::<Vec<_>>().join("; ")
            ),
            Self::Combined(inner) => format!(
                "{} disjoint fragment boundaries: {}",
                inner.len(),
                inner
                    .iter()
                    .map(Self::detail)
                    .collect::<Vec<_>>()
                    .join(" | ")
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
/// Task 3 registers the datatype value-space decider ([`datatype::decide`], Family
/// 5); Task 4 registers the counting / arithmetic-feasibility decider
/// ([`counting::decide`], Families 2/6a/7); Task 5 adds the case-split/complement
/// decider. Each decider returns `None` when its family shape is absent, so a
/// closure carrying no datatype value-space or counting obligation still withholds
/// with `NoDeciderEngaged` — the kernel decides only the fragment a registered
/// family proves complete.
const SUB_DECIDERS: &[SubDecider] = &[datatype::decide, counting::decide, casesplit::decide];

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

// ─────────────────────────────────────────────────────────────────────────────
// The kernel's decidability surface as a first-class, shipped registry.
//
// This registry is the SINGLE SOURCE OF TRUTH for which construct families the
// kernel decides (and under which refutation pattern), and which constructs it
// deliberately RETAINS as honest withholds. `slices/grounding/logic/module.ttl`
// ships it as `logic:DecidedFragment` / `logic:RefutationPattern` /
// `logic:expressivenessBoundary` individuals; the agreement test
// [`tests::module_ttl_projects_the_kernel_registry`] proves the manifest is exactly
// this registry's projection. Every string here is a TECHNICAL fragment /
// completeness / boundary characterization — never a process or issue reference.
//
// The registry types and functions are the shipped, forward-facing kernel API,
// consumed by the Part C agreement test (`module_ttl_projects_the_kernel_registry`,
// under `#[cfg(test)]`) and the forthcoming Task 8 `gmeow` CLI surface — not yet by a
// non-test production caller. Each therefore carries a NARROW, item-scoped
// `#[allow(dead_code)]` (never the blanket module allowance, which was removed): they
// are a genuine registry the ontology manifest projects, not dead scaffold.
// ─────────────────────────────────────────────────────────────────────────────

/// A refutation pattern: the decision-procedure schema a decided construct family
/// closes under. Several families may share one pattern (a cardinality count and a
/// `hasSelf` self-edge are both [`RefutationPattern::CountingPigeonhole`]).
#[allow(dead_code)] // Shipped registry API — consumed by the agreement test + Task 8 CLI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RefutationPattern {
    /// A finite pigeonhole count of distinct fillers / edges against a numeric bound.
    CountingPigeonhole,
    /// A finite datatype value-space cardinality bounding the distinct-literal count.
    ValueSpaceCardinality,
    /// An exhaustive, terminating case-split over a bounded disjunction.
    CaseSplitExhaustion,
    /// A propositional complement clash (`C` and `¬C` in a definition position).
    ComplementClash,
    /// An equality / inequality arithmetic collapse over a finite set of named
    /// individuals (an (inverse-)functional identity forced against a distinctness).
    ArithmeticEqualityCollapse,
    /// A decidable metamodel malformation of the finite triple set (a broken list).
    MalformedMetamodel,
    /// A finite closed-set (nominal enumeration) intersection emptiness.
    NominalClash,
}

#[allow(dead_code)] // Shipped registry API — consumed by the agreement test + Task 8 CLI.
impl RefutationPattern {
    /// Every pattern variant, in canonical [`RefutationPattern::slug`] order — the
    /// closed set the shipped `logic:RefutationPattern` individuals must match.
    pub(crate) const ALL: &'static [RefutationPattern] = &[
        RefutationPattern::CountingPigeonhole,
        RefutationPattern::ValueSpaceCardinality,
        RefutationPattern::CaseSplitExhaustion,
        RefutationPattern::ComplementClash,
        RefutationPattern::ArithmeticEqualityCollapse,
        RefutationPattern::MalformedMetamodel,
        RefutationPattern::NominalClash,
    ];

    /// The stable kebab-case slug — the local name of the pattern's shipped
    /// `logic:RefutationPattern` individual.
    pub(crate) const fn slug(self) -> &'static str {
        match self {
            Self::CountingPigeonhole => "counting-pigeonhole",
            Self::ValueSpaceCardinality => "value-space-cardinality",
            Self::CaseSplitExhaustion => "case-split-exhaustion",
            Self::ComplementClash => "complement-clash",
            Self::ArithmeticEqualityCollapse => "arithmetic-equality-collapse",
            Self::MalformedMetamodel => "malformed-metamodel",
            Self::NominalClash => "nominal-clash",
        }
    }
}

/// One decided construct family: a stable `id` (the local name of its shipped
/// `logic:DecidedFragment` individual), the [`RefutationPattern`] it closes under,
/// and a short TECHNICAL completeness-bound characterization.
#[allow(dead_code)] // Shipped registry API — consumed by the agreement test + Task 8 CLI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct DecidedFragment {
    /// The stable kebab-case fragment id / shipped individual local name.
    pub(crate) id: &'static str,
    /// The refutation pattern the family closes under.
    pub(crate) pattern: RefutationPattern,
    /// The technical completeness bound, free of any process reference.
    pub(crate) bound: &'static str,
}

/// One deliberately-RETAINED withhold: a construct the kernel does NOT decide, with
/// a stable `id` (its shipped `logic:expressivenessBoundary`-record local name) and
/// a TECHNICAL fragment-boundary `reason`.
#[allow(dead_code)] // Shipped registry API — consumed by the agreement test + Task 8 CLI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FragmentBoundaryRecord {
    /// The stable kebab-case boundary id / shipped record local name.
    pub(crate) id: &'static str,
    /// The technical reason the construct lies outside the certified fragment.
    pub(crate) reason: &'static str,
}

/// The certified-complete construct families — ONE entry per decided family,
/// returned sorted by `id` (deterministic). This is the authoritative source the
/// shipped `logic:DecidedFragment` manifest projects. Families 6a (arithmetic
/// identity collapse) and 6b (malformed list) are distinct patterns, so each is its
/// own entry (the "seven construct families" fold Family 6's two sub-families).
#[allow(dead_code)] // Shipped registry API — consumed by the agreement test + Task 8 CLI.
pub(crate) fn decided_fragments() -> Vec<DecidedFragment> {
    let mut fragments = vec![
        DecidedFragment {
            id: "complement-refutation",
            pattern: RefutationPattern::ComplementClash,
            bound: "A class expression forced into both C and its complement not-C in a \
                    class-definition position; complete because complementhood is decided \
                    propositionally, every branch that types an individual into not-C closing \
                    against a derivable C-membership with no model search.",
        },
        DecidedFragment {
            id: "number-cardinality-counting",
            pattern: RefutationPattern::CountingPigeonhole,
            bound: "A min, max, or exact (qualified) cardinality bound on a populated class; \
                    complete because the distinct fillers under the identity stance are finitely \
                    counted and a collapsed bound (min N greater than max M, or N distinct forced \
                    fillers exceeding max M) is a pigeonhole violation with no unbounded search.",
        },
        DecidedFragment {
            id: "union-disjoint-case-split",
            pattern: RefutationPattern::CaseSplitExhaustion,
            bound: "A bounded union C subClassOf (D1 or ... or Dn) whose members are pairwise \
                    disjoint; complete because the finite disjunction is exhaustively case-split \
                    and every branch closes under refutation in a terminating propositional \
                    decision.",
        },
        DecidedFragment {
            id: "nominal-enumeration-counting",
            pattern: RefutationPattern::NominalClash,
            bound: "An individual typed into two or more pairwise-disjoint OWL oneOf enumerations; \
                    complete because nominal membership is a finite closed-set intersection whose \
                    emptiness is decided by counting, with no anonymous-individual generation.",
        },
        DecidedFragment {
            id: "datatype-value-space",
            pattern: RefutationPattern::ValueSpaceCardinality,
            bound: "A facet-restricted datatype whose finite value-space cardinality is provably \
                    smaller than the distinct literals a cardinality bound forces onto it; complete \
                    because the value-space count is derived from the math-grounded \
                    finite-cardinality table, bounding the pigeonhole exactly.",
        },
        DecidedFragment {
            id: "inverse-functional-identity-collapse",
            pattern: RefutationPattern::ArithmeticEqualityCollapse,
            bound: "An inverse-functional or functional property forcing two OWL differentFrom (or \
                    distinct-nominal) individuals to be identified; complete because the identity \
                    collapse is a decidable equality / inequality arithmetic over a finite set of \
                    named individuals.",
        },
        DecidedFragment {
            id: "malformed-rdf-list",
            pattern: RefutationPattern::MalformedMetamodel,
            bound: "An rdf:nil node bearing rdf:first or rdf:rest (a structurally broken RDF list); \
                    complete because list well-formedness is a decidable metamodel property of the \
                    finite triple set, independent of object-level entailment.",
        },
        DecidedFragment {
            id: "has-self-membership",
            pattern: RefutationPattern::CountingPigeonhole,
            bound: "An OWL hasSelf (exists p.Self) restriction in a refutation position where a \
                    self-edge x p x forces membership disjoint with a held class; complete because \
                    self-membership is a single reflexive-edge count with no unbounded quantifier \
                    alternation.",
        },
    ];
    fragments.sort();
    fragments
}

/// The constructs the kernel deliberately RETAINS as honest withholds — ONE entry
/// per retained-withhold construct, returned sorted by `id` (deterministic). Each
/// carries a technical fragment-boundary reason; the shipped
/// `logic:expressivenessBoundary` records project these.
#[allow(dead_code)] // Shipped registry API — consumed by the agreement test + Task 8 CLI.
pub(crate) fn retained_boundaries() -> Vec<FragmentBoundaryRecord> {
    let mut boundaries = vec![
        FragmentBoundaryRecord {
            id: "xsd-pattern-facet",
            reason: "An xsd:pattern facet requires the XML Schema regular-expression dialect, with \
                     its Unicode block and category escapes and XSD-specific quantifier semantics, \
                     which is not the host platform regular-expression language; the value-space \
                     emptiness it induces cannot be decided without an XSD regex evaluator, so it \
                     lies outside the certified fragment.",
        },
        FragmentBoundaryRecord {
            id: "non-binary-property-chain",
            reason: "A property chain of length other than two (an n-ary role composition) does not \
                     reduce to the binary role composition the counting and identity deciders \
                     certify; its closure couples an unbounded number of role edges, so it lies \
                     outside the certified fragment.",
        },
        FragmentBoundaryRecord {
            id: "entangled-existential-cardinality",
            reason: "A configuration entangling an existential OWL someValuesFrom filler with a \
                     number or qualified-cardinality bound on the same property couples witness \
                     generation with counting; the family sub-deciders certify each in isolation \
                     only, so the entangled full-DL case lies outside the certified fragment.",
        },
        FragmentBoundaryRecord {
            id: "rdf12-nested-triple-term",
            reason: "The statement-metadata lowering decomposes an RDF 1.2 reifier's rdf:reifies \
                     term into logic:reifiedStatementSubject / logic:reifiedStatementPredicate / \
                     logic:reifiedStatementObject, which is exact for a statement whose subject \
                     and object are IRIs, blank nodes or literals and preserves exactly two \
                     things it cannot: a NESTED triple term (a reified statement whose own \
                     subject or object is itself a triple term has no non-term component to \
                     decompose into, so it is not lowered and no component edge is emitted for \
                     it), and the reified statement's identity AS A TERM (the three components \
                     are joinable, but nothing in the fact surface denotes the statement itself, \
                     so a rule may quantify over the components and may not quantify over the \
                     statement).",
        },
    ];
    boundaries.sort();
    boundaries
}

/// Route a family-scoped kernel withhold into the reasoner finding output.
///
/// Runs the kernel over `edb`; when it lands OUTSIDE its certified-complete fragment
/// with a FAMILY-SCOPED boundary (an `Uncertified` / `Combined` reason — a family
/// shape was present but its completeness bound did not close), derives the
/// ledger-identified finding through [`boundary_diag_ledger`] so the withhold is
/// carried on [`crate::reason::dl::DlVerdict::boundary_findings`] under
/// [`REFUTATION_KERNEL_CATEGORY`], tied to the same input that produces the verdict.
///
/// The `NoDeciderEngaged` steady state (no family shape engaged — the committed
/// bundle and every gated corpus input) yields NO finding, so this is a strict
/// no-op there and changes no verdict; a decision (`InFragment`) likewise yields
/// none. The finding is a Coherent `UnsupportedSemanticFeature` at Info severity and
/// can NEVER gate (see [`boundary_diag_ledger`]).
pub(crate) fn production_boundary_findings(edb: &RdfDataset) -> Vec<gmeow_errors::Finding> {
    match refute(edb) {
        RefutationCertificate::OutOfFragment { reason }
            if !matches!(reason, FragmentBoundary::NoDeciderEngaged) =>
        {
            boundary_diag_ledger(&reason).findings("reason")
        }
        _ => Vec::new(),
    }
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
                            counted_individuals: [
                                "http://ex/a".to_owned(),
                                "http://ex/b".to_owned(),
                            ]
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

    // An empty decider slice withholds with `NoDeciderEngaged`. The production
    // `refute` now registers the datatype value-space decider (Task 3), which
    // returns `None` on an EDB carrying no datatype value-space obligation, so an
    // empty EDB still withholds `NoDeciderEngaged` — the family engages only on its
    // shape, never on a closure that does not carry it.
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
            "the datatype value-space decider does not engage on an empty EDB"
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

    /// SINGLE SOURCE OF TRUTH: the shipped `logic:DecidedFragment` /
    /// `logic:RefutationPattern` / `logic:expressivenessBoundary` manifest in
    /// `slices/grounding/logic/module.ttl` is EXACTLY the projection of this kernel's
    /// [`decided_fragments`] / [`retained_boundaries`] registry (mirrors the datatype
    /// family's `rust_finite_cardinality_table_projects_the_math_grounding`). Drift in
    /// either direction — the Rust registry gaining/losing an entry, or the slice
    /// editing an id, pattern, bound, or reason — fails here, so the ontology manifest
    /// can never silently diverge from the kernel that decides.
    #[test]
    fn module_ttl_projects_the_kernel_registry() {
        use purrdf::{NativeRdfFormat, RdfTerm, dataset_from_bytes};
        use std::collections::BTreeMap;

        const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
        let decided_class = format!("{LOGIC_NS}DecidedFragment");
        let pattern_class = format!("{LOGIC_NS}RefutationPattern");
        let decides_under = format!("{LOGIC_NS}decidesUnderPattern");
        let completeness_bound = format!("{LOGIC_NS}fragmentCompletenessBound");
        let boundary_pred = format!("{LOGIC_NS}expressivenessBoundary");
        let boundary_reason = format!("{LOGIC_NS}fragmentBoundaryReason");

        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../slices/grounding/logic/module.ttl"
        );
        let bytes = std::fs::read(path).expect("read the logic grounding slice");
        let dataset =
            dataset_from_bytes(&bytes, NativeRdfFormat::Turtle).expect("parse logic module.ttl");

        let local = |iri: &str| iri.strip_prefix(LOGIC_NS).map(str::to_owned);

        let mut ttl_pattern_individuals: BTreeSet<String> = BTreeSet::new();
        let mut ttl_decided_subjects: BTreeSet<String> = BTreeSet::new();
        let mut decides: BTreeMap<String, String> = BTreeMap::new();
        let mut bound: BTreeMap<String, String> = BTreeMap::new();
        let mut boundary_subjects: BTreeSet<String> = BTreeSet::new();
        let mut reason: BTreeMap<String, String> = BTreeMap::new();

        for quad in dataset.owned_quads() {
            let RdfTerm::Iri(subject) = &quad.subject else {
                continue;
            };
            let Some(subj) = local(subject) else {
                continue;
            };
            match quad.predicate.as_str() {
                RDF_TYPE => {
                    if let RdfTerm::Iri(o) = &quad.object {
                        if *o == decided_class {
                            ttl_decided_subjects.insert(subj);
                        } else if *o == pattern_class {
                            ttl_pattern_individuals.insert(subj);
                        }
                    }
                }
                p if p == decides_under => {
                    if let RdfTerm::Iri(o) = &quad.object
                        && let Some(pl) = local(o)
                    {
                        decides.insert(subj, pl);
                    }
                }
                p if p == completeness_bound => {
                    if let RdfTerm::Literal(l) = &quad.object {
                        bound.insert(subj, l.lexical_form.clone());
                    }
                }
                p if p == boundary_pred => {
                    boundary_subjects.insert(subj);
                }
                p if p == boundary_reason => {
                    if let RdfTerm::Literal(l) = &quad.object {
                        reason.insert(subj, l.lexical_form.clone());
                    }
                }
                _ => {}
            }
        }

        // (1) `logic:RefutationPattern` individuals ≡ every `RefutationPattern` slug.
        let rust_patterns: BTreeSet<String> = RefutationPattern::ALL
            .iter()
            .map(|p| p.slug().to_owned())
            .collect();
        assert_eq!(
            ttl_pattern_individuals, rust_patterns,
            "logic:RefutationPattern individuals must match RefutationPattern::ALL slugs"
        );

        // (2) `logic:DecidedFragment` individuals ≡ `decided_fragments()`: id set,
        // deciding pattern per id, and completeness bound per id — bidirectionally.
        let rust_fragments = decided_fragments();
        let rust_ids: BTreeSet<String> = rust_fragments.iter().map(|f| f.id.to_owned()).collect();
        assert_eq!(
            ttl_decided_subjects, rust_ids,
            "logic:DecidedFragment individuals must match decided_fragments() ids"
        );
        for f in &rust_fragments {
            assert_eq!(
                decides.get(f.id).map(String::as_str),
                Some(f.pattern.slug()),
                "fragment {} logic:decidesUnderPattern must match the kernel pattern",
                f.id
            );
            assert_eq!(
                bound.get(f.id).map(String::as_str),
                Some(f.bound),
                "fragment {} logic:fragmentCompletenessBound must match the kernel bound",
                f.id
            );
        }
        assert_eq!(
            decides.keys().cloned().collect::<BTreeSet<_>>(),
            rust_ids,
            "no logic:decidesUnderPattern outside the decided-fragment set"
        );
        assert_eq!(
            bound.keys().cloned().collect::<BTreeSet<_>>(),
            rust_ids,
            "no logic:fragmentCompletenessBound outside the decided-fragment set"
        );

        // (3) `logic:expressivenessBoundary` records ≡ `retained_boundaries()`: id set
        // plus technical reason per id — bidirectionally.
        let rust_boundaries = retained_boundaries();
        let rust_boundary_ids: BTreeSet<String> =
            rust_boundaries.iter().map(|b| b.id.to_owned()).collect();
        assert_eq!(
            boundary_subjects, rust_boundary_ids,
            "logic:expressivenessBoundary records must match retained_boundaries() ids"
        );
        for b in &rust_boundaries {
            assert_eq!(
                reason.get(b.id).map(String::as_str),
                Some(b.reason),
                "boundary {} logic:fragmentBoundaryReason must match the kernel reason",
                b.id
            );
        }
        assert_eq!(
            reason.keys().cloned().collect::<BTreeSet<_>>(),
            rust_boundary_ids,
            "no logic:fragmentBoundaryReason outside the retained-boundary set"
        );
    }

    // ── (R2) Determinism: the kernel is byte-stable on real decided inputs ────────

    /// Read one committed conformance `input.nq` (relative to this crate's manifest
    /// dir) into a frozen dataset.
    fn read_case_edb(rel: &str) -> std::sync::Arc<RdfDataset> {
        use purrdf::{NativeRdfFormat, dataset_from_bytes};
        let path = format!("{}/{rel}", env!("CARGO_MANIFEST_DIR"));
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
        dataset_from_bytes(&bytes, NativeRdfFormat::NQuads)
            .unwrap_or_else(|e| panic!("parse {path}: {e}"))
    }

    /// (R2) DETERMINISM — running the kernel on a fixed input TWICE yields
    /// byte-identical certificate / witness output. This exercises the DATATYPE
    /// value-space decider (the length-facet fixture) and the COUNTING decider (the
    /// `owl:hasSelf` fixture) on real committed production inputs, complementing the
    /// per-decider `determinism_byte_stable` unit tests in `datatype`/`counting`/
    /// `casesplit`. Both fixtures land IN-FRAGMENT, so this pins the witness output
    /// (clashes + structured evidence), not merely an empty boundary. The structured
    /// types order their collections with `BTreeSet`/`BTreeMap`, so the byte-stable
    /// `Debug` rendering is the observable pin on that determinism.
    #[test]
    fn kernel_output_is_byte_stable_on_datatype_and_counting_inputs() {
        for rel in [
            // Family 5 — datatype value-space (length-facet emptiness) decider.
            "../../conformance/logic/cases/datatype-value-space/length-facet-empty/input.nq",
            // Family 7 — the counting decider's owl:hasSelf refutation witness.
            "../../conformance/logic/cases/external/w3c-owl2-full-decided/\
             footnote-not-about-self/input.nq",
        ] {
            let edb = read_case_edb(rel);
            let first = format!("{:?}", refute(edb.as_ref()));
            let second = format!("{:?}", refute(edb.as_ref()));
            assert_eq!(
                first, second,
                "kernel certificate must be byte-stable for {rel}"
            );
            assert!(
                first.contains("InFragment"),
                "{rel} must be DECIDED in-fragment so the pin covers real witness output: {first}"
            );
        }
    }

    // ── (R3) Refusal: the kernel withholds at its certified-fragment edge ─────────

    /// (R3) REFUSAL — an adversarial input that EXCEEDS the kernel's certified
    /// fragment bound must be REFUSED (`OutOfFragment`), never decided. Here a
    /// (populated) cardinality restriction is ENTANGLED with an `owl:someValuesFrom`
    /// existential on the same property: the counting decider certifies cardinality
    /// counting only in ISOLATION, so the entangled full-DL configuration lies
    /// outside its certified-complete fragment (the shipped
    /// `entangled-existential-cardinality` retained boundary). The kernel must
    /// WITHHOLD with a structured, family-scoped boundary rather than hang, loop, or
    /// truncate to a wrong decided verdict — the soundness-by-construction edge. The
    /// withhold must ALSO route to a real production finding via
    /// `production_boundary_findings`, so the honest "outside the fragment" is tied
    /// to the reasoner output rather than dark.
    #[test]
    fn entangled_cardinality_exceeds_fragment_bound_and_is_refused() {
        use purrdf::{RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};

        const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
        const OWL_RESTRICTION: &str = "http://www.w3.org/2002/07/owl#Restriction";
        const OWL_ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
        const OWL_MIN_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#minCardinality";
        const OWL_SOME_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#someValuesFrom";
        const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
        const XSD_NNI: &str = "http://www.w3.org/2001/XMLSchema#nonNegativeInteger";
        const W: &str = "http://ex/w";

        let iri_q = |s: &str, p: &str, o: &str| {
            RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
        };

        let mut b = RdfDatasetBuilder::new();
        for q in [
            // A populated class C with a min-1 cardinality restriction on p …
            iri_q("http://ex/C", RDF_TYPE, OWL_CLASS),
            iri_q("http://ex/i", RDF_TYPE, "http://ex/C"),
            iri_q("http://ex/C", RDFS_SUBCLASSOF, "http://ex/r1"),
            iri_q("http://ex/r1", RDF_TYPE, OWL_RESTRICTION),
            iri_q("http://ex/r1", OWL_ON_PROPERTY, "http://ex/p"),
            // … ENTANGLED with a someValuesFrom existential on the SAME property.
            iri_q("http://ex/C", RDFS_SUBCLASSOF, "http://ex/r2"),
            iri_q("http://ex/r2", RDF_TYPE, OWL_RESTRICTION),
            iri_q("http://ex/r2", OWL_ON_PROPERTY, "http://ex/p"),
            iri_q("http://ex/r2", OWL_SOME_VALUES_FROM, "http://ex/D"),
        ] {
            b.push_owned_quad(&q);
        }
        b.push_owned_quad(
            &RdfQuad::new(
                RdfTerm::iri("http://ex/r1"),
                OWL_MIN_CARDINALITY,
                RdfTerm::Literal(RdfLiteral::typed("1", XSD_NNI)),
            )
            .in_graph(RdfTerm::iri(W)),
        );
        let edb = b.freeze().expect("freeze the entangled edb");

        let certificate = refute(edb.as_ref());
        assert!(
            matches!(certificate, RefutationCertificate::OutOfFragment { .. }),
            "the kernel MUST refuse (withhold) at its certified-fragment edge, never decide: \
             {certificate:?}"
        );

        // The refusal is a FAMILY-SCOPED boundary (a shape engaged but its
        // completeness bound did not close), so it routes to a real production
        // finding rather than the dark `NoDeciderEngaged` steady state.
        let findings = production_boundary_findings(edb.as_ref());
        assert!(
            !findings.is_empty(),
            "an entangled-cardinality withhold must surface a family-scoped boundary finding"
        );
    }

    // ── No process references in the kernel registry itself (R: acceptance) ───────

    /// The kernel registry's OWN technical strings — every `decided_fragments()`
    /// completeness bound and every `retained_boundaries()` reason — are free of any
    /// PROCESS REFERENCE (`#<digit>`, `issue`, a bare `PR` token, or `per #`,
    /// case-insensitive). The conformance gate proves the same over the shipped
    /// `module.ttl` projection; this pins the source registry directly, so a process
    /// reference can enter neither the kernel nor its manifest.
    #[test]
    fn kernel_registry_strings_carry_no_process_reference() {
        fn process_reference(text: &str) -> Option<&'static str> {
            let lower = text.to_ascii_lowercase();
            let bytes = lower.as_bytes();
            for i in 0..bytes.len() {
                if bytes[i] == b'#' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
                    return Some("#<digit>");
                }
            }
            if lower.contains("issue") {
                return Some("issue");
            }
            if lower.contains("per #") {
                return Some("per #");
            }
            let is_word = |c: u8| c.is_ascii_alphanumeric();
            for i in 0..bytes.len().saturating_sub(1) {
                if bytes[i] == b'p'
                    && bytes[i + 1] == b'r'
                    && (i == 0 || !is_word(bytes[i - 1]))
                    && (i + 2 >= bytes.len() || !is_word(bytes[i + 2]))
                {
                    return Some("PR");
                }
            }
            None
        }

        let mut failures: Vec<String> = Vec::new();
        for f in decided_fragments() {
            if let Some(pat) = process_reference(f.bound) {
                failures.push(format!("decided fragment {:?} bound carries {pat:?}", f.id));
            }
        }
        for boundary in retained_boundaries() {
            if let Some(pat) = process_reference(boundary.reason) {
                failures.push(format!(
                    "retained boundary {:?} reason carries {pat:?}",
                    boundary.id
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "kernel registry technical strings must be free of process references:\n  • {}",
            failures.join("\n  • ")
        );
    }
}
