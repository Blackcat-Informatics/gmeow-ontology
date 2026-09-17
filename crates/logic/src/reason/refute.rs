// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native refutation evidence and source-admission contracts.
//!
//! Counting, datatype and class producers execute against the joint native store,
//! governor and proof registry. Their retained per-world outcomes distinguish
//! supported conflicts from completion, source refusals and capability boundaries.
//! A success from one family cannot replace another family's evidence. The result
//! facade validates that complete execution record before deriving a verdict.
//!
//! [`class_diagnostic`] selects the explicit class operation on the same native
//! driver. [`PreparedClassAnalysis`] exposes source-owner admission without making
//! a consistency claim. Invalid selected grammar fails admission before inference;
//! well-formed unsupported constructs retain separate capability evidence.
//!
//! [`decided_fragments`], [`source_admission_contracts`] and [`retained_boundaries`]
//! describe the native contract independently of any particular run. Authenticated
//! producer observations compare the authored registry to
//! [`native_fragment_registry`] in both directions. [`boundary_diag_ledger`]
//! preserves every retained cause and grades malformed input separately from a
//! valid unsupported capability. Ordered evidence provides deterministic transport;
//! neither a registry entry nor a digest independently proves a theorem.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_errors::{
    Diag, DiagLedger, FindingCategory, Grade, Severity, StageId, Standpoint, register_code,
};
use gmeow_math::Rational;
use purrdf::RdfTerm;
use serde::{Deserialize, Serialize};

use crate::facts::skolem_iri;

/// Datatype value-space producer on the shared native store.
pub(crate) mod datatype;

/// Counting, identity and arithmetic-feasibility producers on the shared store.
pub(crate) mod counting;

/// Prepared class source admission and bounded contextual case analysis.
pub(crate) mod casesplit;
/// Shared proof identities and complete per-world native family observations.
pub(crate) mod native;
pub use native::{
    NativeAnalysisUsage, NativeBoundEvidence, NativeClosureStatus, NativeFamilyCompletion,
    NativeFamilyLedger, NativeFamilyObstruction, NativeFamilyOutcome, NativeObligationScope,
    NativeObstructionKind, NativeProofId, NativeProofNode, NativeProofOrigin, NativeRead,
    NativeReadKind, NativeRefutationFamily, NativeSourceTerms, NativeSupportedClash,
};
mod diagnostic;
pub use diagnostic::{ClassDiagnosticObservation, ClassDiagnosticOutcome, class_diagnostic};

pub(crate) use casesplit::execution::{
    completion_reads as class_completion_reads, positive_reads as class_positive_reads,
    preparation_reads as class_preparation_reads,
};
pub use casesplit::{
    ClassAdmissionObservation, ClassAdmissionSourceWorld, ClassAdmissionWorld,
    ClassExecutionOutcome, ClassSourceRefusal, PreparedClassAnalysis,
};

mod proof;
pub use proof::{
    ContextualConflict, RefutationAssumption, RefutationBranch, RefutationClash,
    RefutationExpression, RefutationPremise, RefutationProof, RefutationSourceIssue,
};

// ── Shared term / world / value helpers (used by every family sub-decider) ──────

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

/// Stable family identities for retained fragment-boundary evidence. These names
/// classify a boundary; they do not select execution order or certify a whole run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FragmentFamily {
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

/// The kind of a counted cardinality bound the fragment argument turned on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum BoundKind {
    /// A `min` / `minQualifiedCardinality` lower bound.
    Min,
    /// A `max` / `maxQualifiedCardinality` upper bound.
    Max,
    /// An exact `cardinality` / `qualifiedCardinality` bound.
    Exact,
}

/// The structured reason a case lies OUTSIDE the certified-complete fragment. Free
/// of any process references — it names the construct/shape that put the case out
/// of the fragment, deterministically ordered.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FragmentBoundary {
    /// Exact source grammar could not be admitted. This is not a model contradiction.
    SourceAdmission {
        /// The selected context owning the failed source definition.
        world: String,
        /// The actual source rows; missing fields are described rather than invented.
        premises: Vec<RefutationPremise>,
        /// Input or supported-fragment requirement that failed.
        issue: RefutationSourceIssue,
    },
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
            Self::SourceAdmission { .. } => "source-admission",
            Self::Uncertified { .. } => "uncertified",
            Self::Combined(_) => "combined",
        }
    }

    /// A deterministic, message-INDEPENDENT structural key over the boundary's
    /// content, used as the finding focus so two distinct boundaries never
    /// hash-cons-merge and no withhold is dropped.
    fn focus_key(&self) -> String {
        match self {
            Self::SourceAdmission {
                world,
                premises,
                issue,
            } => format!(
                "source-admission\u{1f}{}",
                crate::physical::metadata_identity(
                    "gmeow-refutation-source-boundary-v1",
                    &(world, issue, premises)
                )
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
            ),
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
            Self::SourceAdmission { world, issue, .. } => {
                format!("source admission in context <{world}>: {}", issue.detail())
            }
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

/// Project every retained source or capability cause through the diagnostics ledger.
/// Malformed selected input is a structural error. Valid unsupported constructs
/// remain explicit capability findings; combining them cannot erase either grade.
/// This fold consumes existing evidence and never re-runs source admission.
pub(crate) fn boundary_diag_ledger(reason: &FragmentBoundary) -> DiagLedger {
    fn attach(reason: &FragmentBoundary, ledger: &mut DiagLedger) {
        if let FragmentBoundary::Combined(boundaries) = reason {
            for boundary in boundaries {
                attach(boundary, ledger);
            }
            return;
        }
        let (severity, category) = match reason {
            FragmentBoundary::SourceAdmission { issue, .. }
                if issue.refusal_class() == ClassSourceRefusal::Invalid =>
            {
                (
                    Severity::Error,
                    FindingCategory::ModelingDisciplineViolation,
                )
            }
            _ => (Severity::Info, FindingCategory::UnsupportedSemanticFeature),
        };
        let code = register_code(&format!(
            "reason.{REFUTATION_KERNEL_CATEGORY}.{}",
            reason.code_suffix()
        ));
        let grade = Grade::new(severity, category, Standpoint::Binding);
        let focus = [REFUTATION_KERNEL_CATEGORY, reason.focus_key().as_str()].join(FOCUS_SEP);
        let diag = Diag::new(code, grade, reason.detail()).with_focus(focus);
        ledger.attach(diag, StageId::new(REFUTATION_KERNEL_STAGE));
    }
    let mut ledger = DiagLedger::new();
    attach(reason, &mut ledger);
    ledger
}

// ─────────────────────────────────────────────────────────────────────────────
// The kernel's decidability surface as a first-class, shipped registry.
//
// This registry is the SINGLE SOURCE OF TRUTH for which construct families the
// kernel decides (and under which refutation pattern), and which constructs it
// deliberately RETAINS as honest withholds. `slices/grounding/logic/module.ttl`
// ships it as `logic:DecidedFragment` / `logic:RefutationPattern` /
// `logic:expressivenessBoundary` individuals. The authenticated pipeline consumer
// proves the producer's source observation equals `native_fragment_registry()`.
// Every string here is a technical fragment, completeness or boundary characterization.
//
// The registry values are the executable counterpart of the authored manifest.
// The public CLI reads the manifest from the shipped bundle, while the runtime
// joint runtime retains the outcomes of every selected native producer. The read-only native registry
// needs no source checkout or corpus construction.
// ─────────────────────────────────────────────────────────────────────────────

/// A refutation pattern: the decision-procedure schema a decided construct family
/// closes under. Several families may share one pattern (a cardinality count and a
/// `hasSelf` self-edge are both [`RefutationPattern::CountingPigeonhole`]).
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
    /// A finite closed-set (nominal enumeration) intersection emptiness.
    NominalClash,
}

impl RefutationPattern {
    /// Every pattern variant, in canonical [`RefutationPattern::slug`] order — the
    /// closed set the shipped `logic:RefutationPattern` individuals must match.
    pub(crate) const ALL: &'static [RefutationPattern] = &[
        RefutationPattern::CountingPigeonhole,
        RefutationPattern::ValueSpaceCardinality,
        RefutationPattern::CaseSplitExhaustion,
        RefutationPattern::ComplementClash,
        RefutationPattern::ArithmeticEqualityCollapse,
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
            Self::NominalClash => "nominal-clash",
        }
    }
}

/// One decided construct family: a stable `id` (the local name of its shipped
/// `logic:DecidedFragment` individual), the [`RefutationPattern`] it closes under,
/// and a short TECHNICAL completeness-bound characterization.
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FragmentBoundaryRecord {
    /// The stable kebab-case boundary id / shipped record local name.
    pub(crate) id: &'static str,
    /// The technical reason the construct lies outside the certified fragment.
    pub(crate) reason: &'static str,
}

/// The certified-complete construct families — ONE entry per decided family,
/// returned sorted by `id` (deterministic). This is the authoritative source the
/// shipped `logic:DecidedFragment` manifest projects. Source grammar admission
/// remains a separate operation and cannot establish a semantic decision.
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
            bound: "A datatype whose value-space capacity is provably smaller than the distinct \
                    values required by a cardinality bound; complete because native datatype \
                    definitions and primitive capacity evidence give a sufficient finite upper \
                    bound for the pigeonhole contradiction.",
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

pub const CLASS_EXPRESSION_SOURCE_ADMISSION_ID: &str = "nativeClassExpressionListAdmission";

/// One selected source grammar contract. Admission records describe which
/// source structures an operation accepts; they are not semantic decision families.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SourceAdmissionContractRecord {
    pub(crate) id: &'static str,
    pub(crate) requirement: &'static str,
}

/// Exact source-owned class/list admission exported alongside the semantic registry.
/// A descriptive entry does not itself admit an input or establish a contradiction.
pub(crate) fn source_admission_contracts() -> Vec<SourceAdmissionContractRecord> {
    vec![SourceAdmissionContractRecord {
        id: CLASS_EXPRESSION_SOURCE_ADMISSION_ID,
        requirement: "Selected class-expression operators and typed distinct-member declarations own their operands and reachable finite lists in one native world. Owned lists require an unambiguous first/rest path to rdf:nil, with no fields on that terminator; unrelated list-shaped data selects no admission contract. Malformed selected grammar fails admission; well-formed constructs outside the selected operand or definition fragment remain explicit capability refusals. Every refusal retains its exact owner and reachable source path; none is an object-level contradiction or a claim of semantic completeness.",
    }]
}

/// The constructs the kernel deliberately RETAINS as honest withholds — ONE entry
/// per retained-withhold construct, returned sorted by `id` (deterministic). Each
/// carries a technical fragment-boundary reason; the shipped
/// `logic:expressivenessBoundary` records project these.
pub(crate) fn retained_boundaries() -> Vec<FragmentBoundaryRecord> {
    let mut boundaries = vec![
        FragmentBoundaryRecord {
            id: "finite-journal-infinite-trace",
            reason: "Finite journal evaluation quantifies only over authenticated observed positions. An open prefix does not determine its unobserved continuation, and finalizing one observation does not prove a claim over an infinite trace. Infinite-trace temporal validity is outside this certified finite fragment.",
        },
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
            reason: "A configuration coupling an existential filler with a number or qualified-cardinality bound lies outside the admitted joint fragment when its combined source obligations or termination requirements are unsupported. Co-occurrence alone is not a boundary: admitted witness generation and counting share one world-scoped fixed point. This does not certify arbitrary full-DL combinations.",
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

/// Native fragment inventory or its observed authored projection. Exact equality
/// checks both directions, including undeclared patterns and stray characterizations.
/// This descriptive surface does not authorize execution or optimization rewrites.
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeFragmentRegistry {
    /// Stable local names of every refutation pattern.
    pub pattern_ids: BTreeSet<String>,
    /// Stable local names of every decided fragment.
    pub decided_ids: BTreeSet<String>,
    /// Stable local names of every selected source admission contract.
    pub source_admission_ids: BTreeSet<String>,
    /// Exact source selection and admission requirements, keyed by contract local name.
    pub source_admission_requirements: BTreeMap<String, String>,
    /// Stable local names of every retained expressiveness boundary.
    pub boundary_ids: BTreeSet<String>,
    /// Deciding pattern, keyed by fragment local name.
    pub deciding_patterns: BTreeMap<String, String>,
    /// Technical completeness bound, keyed by fragment local name.
    pub completeness_bounds: BTreeMap<String, String>,
    /// Technical reason for withholding, keyed by boundary local name.
    pub boundary_reasons: BTreeMap<String, String>,
}

impl NativeFragmentRegistry {
    /// Read the authored registry once from its parsed native source. Source admission
    /// records are structurally checked separately from semantic capability claims.
    pub fn observe(dataset: &purrdf::RdfDataset) -> Result<Self, gmeow_errors::Diag> {
        use gmeow_ns::LOGIC_NS;
        use purrdf::TermRef;
        let decided_class = format!("{LOGIC_NS}DecidedFragment");
        let pattern_class = format!("{LOGIC_NS}RefutationPattern");
        let admission_class = format!("{LOGIC_NS}SourceAdmissionContract");
        let admission_requirement = format!("{LOGIC_NS}sourceAdmissionRequirement");
        let decides_under = format!("{LOGIC_NS}decidesUnderPattern");
        let boundary_pred = format!("{LOGIC_NS}expressivenessBoundary");
        let completeness_bound = format!("{LOGIC_NS}fragmentCompletenessBound");
        let boundary_reason = format!("{LOGIC_NS}fragmentBoundaryReason");
        let mut surface = Self::default();
        for quad in dataset
            .quads()
            .chain(dataset.reifier_quads())
            .chain(dataset.annotation_quads())
        {
            let predicate_value = dataset.resolve(quad.p);
            let object = dataset.resolve(quad.o);
            let selects_admission = matches!(predicate_value, TermRef::Iri(p) if p == admission_requirement)
                || (matches!(
                    predicate_value,
                    TermRef::Iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#type")
                ) && matches!(object, TermRef::Iri(o) if o == admission_class));
            if selects_admission
                && !matches!(dataset.resolve(quad.s), TermRef::Iri(s) if s.starts_with(LOGIC_NS))
            {
                return Err(registry_error(
                    "a shipped source admission record must name its logic individual",
                ));
            }
            let TermRef::Iri(subject) = dataset.resolve(quad.s) else {
                continue;
            };
            let Some(local) = subject.strip_prefix(LOGIC_NS) else {
                continue;
            };
            let TermRef::Iri(predicate) = predicate_value else {
                continue;
            };
            match predicate {
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type" => {
                    if matches!(object, TermRef::Iri(iri) if iri == decided_class) {
                        surface.decided_ids.insert(local.to_owned());
                    } else if matches!(object, TermRef::Iri(iri) if iri == pattern_class) {
                        surface.pattern_ids.insert(local.to_owned());
                    } else if matches!(object, TermRef::Iri(iri) if iri == admission_class) {
                        surface.source_admission_ids.insert(local.to_owned());
                    }
                }
                predicate if predicate == decides_under => {
                    let TermRef::Iri(pattern) = object else {
                        return Err(registry_error(&format!(
                            "{subject} {predicate} must name a logic pattern"
                        )));
                    };
                    let Some(pattern) = pattern.strip_prefix(LOGIC_NS) else {
                        return Err(registry_error(&format!(
                            "{subject} {predicate} must name a logic pattern"
                        )));
                    };
                    insert_registry_value(
                        &mut surface.deciding_patterns,
                        local,
                        predicate,
                        pattern,
                    )?;
                }
                predicate if predicate == boundary_pred => {
                    surface.boundary_ids.insert(local.to_owned());
                }
                predicate if predicate == admission_requirement => {
                    let TermRef::Literal {
                        lexical,
                        datatype,
                        language: None,
                        direction: None,
                    } = object
                    else {
                        return Err(registry_error(&format!(
                            "{subject} {predicate} must be an untagged xsd:string literal"
                        )));
                    };
                    if !matches!(
                        dataset.resolve(datatype),
                        TermRef::Iri("http://www.w3.org/2001/XMLSchema#string")
                    ) {
                        return Err(registry_error(&format!(
                            "{subject} {predicate} must be an untagged xsd:string literal"
                        )));
                    }
                    if lexical.trim().is_empty() {
                        return Err(registry_error(&format!(
                            "{subject} {predicate} must be nonempty"
                        )));
                    }
                    insert_registry_value(
                        &mut surface.source_admission_requirements,
                        local,
                        predicate,
                        lexical,
                    )?;
                }
                predicate if predicate == completeness_bound || predicate == boundary_reason => {
                    let TermRef::Literal { lexical, .. } = object else {
                        return Err(registry_error(&format!(
                            "{subject} {predicate} must be a literal"
                        )));
                    };
                    let values = if predicate == completeness_bound {
                        &mut surface.completeness_bounds
                    } else {
                        &mut surface.boundary_reasons
                    };
                    insert_registry_value(values, local, predicate, lexical)?;
                }
                _ => {}
            }
        }
        if surface
            .source_admission_requirements
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            != surface.source_admission_ids
        {
            return Err(registry_error(
                "every source admission contract requires exactly one characterization, with no untyped characterizations",
            ));
        }
        if !surface
            .source_admission_ids
            .is_disjoint(&surface.decided_ids)
            || !surface
                .source_admission_ids
                .is_disjoint(&surface.pattern_ids)
            || !surface
                .source_admission_ids
                .is_disjoint(&surface.boundary_ids)
        {
            return Err(registry_error(
                "source admission contracts cannot also claim a semantic fragment, pattern or boundary",
            ));
        }
        Ok(surface)
    }
}

fn insert_registry_value(
    values: &mut BTreeMap<String, String>,
    local: &str,
    predicate: &str,
    value: &str,
) -> Result<(), gmeow_errors::Diag> {
    use gmeow_ns::LOGIC_NS;
    if let Some(previous) = values.insert(local.to_owned(), value.to_owned())
        && previous != value
    {
        return Err(registry_error(&format!(
            "{LOGIC_NS}{local} has conflicting values for {predicate}"
        )));
    }
    Ok(())
}

fn registry_error(detail: &str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Physical {
        detail: detail.to_owned(),
    })
}

/// Read the native kernel's registry without loading or producing a corpus.
#[must_use]
pub fn native_fragment_registry() -> NativeFragmentRegistry {
    let mut registry = NativeFragmentRegistry {
        pattern_ids: RefutationPattern::ALL
            .iter()
            .map(|pattern| pattern.slug().to_owned())
            .collect(),
        ..NativeFragmentRegistry::default()
    };
    for fragment in decided_fragments() {
        registry.decided_ids.insert(fragment.id.to_owned());
        registry
            .deciding_patterns
            .insert(fragment.id.to_owned(), fragment.pattern.slug().to_owned());
        registry
            .completeness_bounds
            .insert(fragment.id.to_owned(), fragment.bound.to_owned());
    }
    for contract in source_admission_contracts() {
        registry.source_admission_ids.insert(contract.id.to_owned());
        registry
            .source_admission_requirements
            .insert(contract.id.to_owned(), contract.requirement.to_owned());
    }
    for boundary in retained_boundaries() {
        registry.boundary_ids.insert(boundary.id.to_owned());
        registry
            .boundary_reasons
            .insert(boundary.id.to_owned(), boundary.reason.to_owned());
    }
    registry
}
#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "refute_test_support.rs"]
mod test_support;
#[cfg(test)]
pub(crate) use test_support::certify_membership;
#[cfg(test)]
pub use test_support::{
    CountBound, Decision, NothingClash, RefutationCertificate, Witness, WitnessEvidence,
};
