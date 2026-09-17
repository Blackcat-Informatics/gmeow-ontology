// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native entailment by refutation: `A ⊨ C` iff `A ∪ ¬C` is inconsistent.
//!
//! Entailment is a *fundamental* reasoning operation, decided here as a thin
//! composition over the native DL consistency calculus
//! ([`crate::reason::reason_all`]) — NOT a second reasoning path. To decide
//! whether a premise graph `A` entails a conclusion `C`, we negate `C` by
//! refutation, union the negation into the premise's world, and ask the native DL
//! clash rule whether the result is inconsistent: if it is, every model of `A`
//! satisfies `C`, so `A ⊨ C`.
//!
//! The selected profile is a single, unspecified default assertion context.
//! Named graphs and explicit context coordinates require an admitted context
//! selection this operation does not implement; they return a native-coverage
//! gap before reduction. No union-of-contexts entailment is implicit. Native
//! RDF 1.2 reifier and annotation evidence stays with admitted premises.
//!
//! This module composes the native reasoner without adding an inference rule.
//! Its admission and reduction behavior belongs to the public native engine
//! contract. An [`EntailmentVerdict`] is still an operation result, not a
//! complete source-coverage certificate or an authorization for rewriting.
//!
//! ## The conclusion-shape calculus (the one negation waist)
//!
//! A conclusion is a set of RDF triples. Each triple is normalized into a
//! [`ConclusionShape`](crate::entail::ConclusionShape) and negated by one shared
//! [`negate`](crate::entail::negate) primitive:
//!
//! * a ground membership `a rdf:type C` → assert a counter-model `a ∈ C̄` with
//!   `C owl:disjointWith C̄` (`C̄` a fresh complement); the EDB clashes iff `A ⊨ C(a)`;
//! * a subsumption `C rdfs:subClassOf D` → its negation `∃x.(C(x) ∧ ¬D(x))`,
//!   witnessed by one fresh individual `w`: `w ∈ C`, `w ∈ D̄`, `D owl:disjointWith D̄`.
//!
//! A **multi-triple** ground conclusion `{t₁ … tₙ}` is entailed iff `A ⊨ tᵢ` for
//! EVERY `i` — decided as *n* INDEPENDENT refutations, each `A ∪ ¬tᵢ`, all of which
//! must be inconsistent. (Unioning every negation into one EDB would instead test a
//! disjunction — a clash on any single `tᵢ` — which is wrong, so each component is
//! its own consistency check.)
//!
//! ## One non-refutation route: `rdfs:subPropertyOf` by hierarchy reachability
//!
//! Refuting `P ⊑ Q` needs a counter-model `∃x,y.(P(x,y) ∧ ¬Q(x,y))`, whose role
//! complement `¬Q` is NOT EL-expressible — so subproperty entailment cannot go through
//! the [`negate`](crate::entail::negate)/[`crate::reason::reason_all`] refutation
//! waist at all. It is
//! instead decided directly (`decide_subproperty`) by REFLEXIVE-TRANSITIVE
//! reachability over the premise's property hierarchy: `A ⊨ (P ⊑ Q)` iff `Q` is reachable
//! from `P` along asserted `rdfs:subPropertyOf` edges and `owl:equivalentProperty`
//! (mutual) edges, plus the reflexive `P ⊑ P` and the universal `owl:top{Object,Data}Property`
//! super-property. `Entailed` this way is unconditionally sound (rdfs5 transitivity +
//! rdfs6 reflexivity are valid, and RDFS ⊨ ⊆ OWL ⊨). `NotEntailed` is sound ONLY when the
//! premise is a PURE property hierarchy (every default-graph triple is a simple
//! `subPropertyOf`/`equivalentProperty` edge over IRIs) — such a theory is always
//! satisfiable and its closure is the exact set of entailed subproperty facts. If the
//! premise carries any other property-relating construct (a property chain, an inverse, a
//! characteristic type, a property-expression endpoint, a named-graph or class-level
//! axiom that could derive further subproperty facts or make the premise inconsistent),
//! the reachability closure is not a complete account, so an unreachable `Q` yields an
//! honest [`GapShape::NativeCoverage`](crate::entail::GapShape::NativeCoverage) gap — never a guessed `NotEntailed`.
//!
//! ## Sound fresh-symbol minting (the soundness floor)
//!
//! The fresh complement/witness IRIs are minted in a reserved namespace
//! ([`ENTAIL_RESERVED_NS`](crate::entail::ENTAIL_RESERVED_NS)) with a blake3-of-input suffix, and [`Minter::new`](crate::entail::Minter::new)
//! HARD-FAILS if the premise∪conclusion vocabulary already contains any reserved
//! IRI. This is load-bearing for soundness: a premise legitimately mentioning a
//! would-be complement IRI must never collide with a minted one, because a
//! collision could mask a real clash (a false `consistent`) or invent a spurious one
//! (a false `inconsistent`) — either way flipping the entailment verdict. A
//! plain string suffix (as an earlier TPTP-only lowerer used) is unsound for RDF,
//! where IRIs routinely contain arbitrary characters.
//!
//! ## The fragment boundary is a structured gap, never a wrong answer
//!
//! A conclusion shape the native DL fragment cannot soundly refute — a role /
//! property assertion (role negation is not EL-expressible), a blank-node
//! (existential) subject/object that needs Skolemization, a malformed triple — is
//! an [`EntailmentGap`](crate::entail::EntailmentGap) carrying a structured [`GapShape`](crate::entail::GapShape) token, never a silent
//! skip and never a guessed verdict.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use purrdf::{RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm, TermRef};

use gmeow_errors::Diag;

mod admission;
use admission::{AdmittedDefaultGraph, assertions};

/// The RDF `type` predicate.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// The RDFS `subClassOf` predicate.
const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
/// The RDFS `subPropertyOf` predicate (decided by hierarchy reachability, not refutation).
const RDFS_SUBPROPERTYOF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
/// The OWL `equivalentProperty` predicate (mutual subproperty: `P ≡ Q` ⟺ `P ⊑ Q ∧ Q ⊑ P`).
const OWL_EQUIVALENT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#equivalentProperty";
/// The OWL universal super object-property (every property is its subproperty).
const OWL_TOP_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#topObjectProperty";
/// The OWL universal super data-property (every property is its subproperty).
const OWL_TOP_DATA_PROPERTY: &str = "http://www.w3.org/2002/07/owl#topDataProperty";
/// The OWL `disjointWith` predicate (drives the native DL clash rule).
const OWL_DISJOINTWITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";

/// The reserved namespace every minted refutation symbol (complement / witness)
/// lives in. No input vocabulary may contain an IRI in this namespace — the minter
/// enforces that ([`Minter::new`]) so a minted symbol can never collide with a real one.
pub const ENTAIL_RESERVED_NS: &str = "https://blackcatinformatics.ca/logic/entail/reserved#";

/// The execution world for the admitted default assertion context. This is a
/// placement of one context, never a union of source named graphs. The original
/// premise remains unchanged; the reduction carries its native statement layer.
pub const ENTAIL_WORLD: &str = "https://blackcatinformatics.ca/logic/entail/world";

fn entail_err(detail: String) -> Diag {
    Diag::of_kind(crate::error::Reason { detail })
}

/// A conclusion triple the native DL fragment can soundly DECIDE — either by refutation
/// ([`ConclusionShape::GroundType`] / [`ConclusionShape::SubClassOf`], negated through the
/// shared [`negate`] waist) or, for [`ConclusionShape::SubPropertyOf`], by hierarchy
/// REACHABILITY ([`decide_subproperty`]) since role-complement refutation is not
/// EL-expressible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConclusionShape {
    /// A ground class membership `a rdf:type C`.
    GroundType {
        /// The individual `a`.
        subject: String,
        /// The class `C`.
        class: String,
    },
    /// A class subsumption `C rdfs:subClassOf D`.
    SubClassOf {
        /// The subclass `C`.
        sub: String,
        /// The superclass `D`.
        sup: String,
    },
    /// A property subsumption `P rdfs:subPropertyOf Q` (both IRIs). Decided by
    /// reflexive-transitive REACHABILITY over the premise's property hierarchy
    /// ([`decide_subproperty`]), NOT by refutation — [`negate`] refuses it.
    SubPropertyOf {
        /// The subproperty `P`.
        sub: String,
        /// The superproperty `Q`.
        sup: String,
    },
}

/// The closed taxonomy backing every `gmeow:gapShape` wire token — the 1:1 image of
/// the ontology's closed `gmeow:GapShape` value class, and the SINGLE authority any
/// producer of a `gap_shape` string (native reduction, vendoring, or a hand-written
/// case fixture) must go through. No other code may mint a `gap_shape` literal.
///
/// Four variants are reasoner-FRAGMENT gaps — a conclusion shape the native DL
/// fragment cannot soundly refute at all ([`GapShape`] mirrors exactly these four,
/// see [`GapShape::as_capability_shape`]). The fifth, [`CapabilityGapShape::VendoringMultiGoal`],
/// is NOT a fragment gap: `dl_entails` decides a conjunctive multi-triple conclusion
/// perfectly well (as *n* independent refutations); it is only the frozen,
/// single-`input.nq` vendoring format that cannot freeze a conjunction as one EDB.
/// Conflating the two — labelling a vendoring-format limit as a reasoner gap — was a
/// prior bug this enum forecloses: see [`CapabilityGapShape::is_reasoner_fragment_gap`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityGapShape {
    /// A role / property assertion (a bare `a P b`, a `rdfs:domain`/`rdfs:range`
    /// axiom, …). Role negation is not EL-expressible, so it cannot be refuted.
    /// (`rdfs:subPropertyOf` is NOT here: it is decided by hierarchy reachability, not
    /// refutation — see [`ConclusionShape::SubPropertyOf`].) Reasoner-fragment gap.
    RoleAssertion,
    /// A blank-node (existential) subject or object, which needs Skolem-witness
    /// semantics outside the ground-refutation fragment. Reasoner-fragment gap.
    ExistentialWitness,
    /// The native DL engine reported a coverage gap on the reduced EDB — it cannot
    /// honestly decide the reduction (distinct from a shape it refused up front).
    /// Reasoner-fragment gap.
    NativeCoverage,
    /// A malformed conclusion triple (e.g. a literal where a class IRI is required).
    /// Reasoner-fragment gap.
    Malformed,
    /// A conjunctive multi-triple conclusion. NOT a reasoner-fragment gap — `dl_entails`
    /// decides it (as *n* independent refutations, see [`VendorReduction::MultiGoal`]) —
    /// it is a VENDORING-FORMAT limit: a conjunctive conclusion cannot be frozen as one
    /// single-EDB `input.nq`, so the vendoring lane records it as a gap even though the
    /// reasoner itself has no trouble with it.
    VendoringMultiGoal,
}

impl CapabilityGapShape {
    /// Every variant, in wire-token order, for exhaustive validation / enumeration
    /// (e.g. rendering the valid-token set in a hard-fail diagnostic).
    pub const ALL: [CapabilityGapShape; 5] = [
        CapabilityGapShape::RoleAssertion,
        CapabilityGapShape::ExistentialWitness,
        CapabilityGapShape::NativeCoverage,
        CapabilityGapShape::Malformed,
        CapabilityGapShape::VendoringMultiGoal,
    ];

    /// The stable enumerated wire token for this gap shape (the `gmeow:gapShape` value).
    #[must_use]
    pub fn as_token(&self) -> &'static str {
        match self {
            CapabilityGapShape::RoleAssertion => "role-assertion",
            CapabilityGapShape::ExistentialWitness => "existential-witness",
            CapabilityGapShape::NativeCoverage => "native-coverage",
            CapabilityGapShape::Malformed => "malformed",
            CapabilityGapShape::VendoringMultiGoal => "vendoring-multi-goal",
        }
    }

    /// Parse a wire token back into its [`CapabilityGapShape`] (the exhaustive inverse
    /// of [`Self::as_token`]), or `None` if `s` is not one of the closed taxonomy's
    /// tokens. The validation gate every ingested `gap_shape` string must pass.
    #[must_use]
    pub fn from_token(s: &str) -> Option<Self> {
        match s {
            "role-assertion" => Some(CapabilityGapShape::RoleAssertion),
            "existential-witness" => Some(CapabilityGapShape::ExistentialWitness),
            "native-coverage" => Some(CapabilityGapShape::NativeCoverage),
            "malformed" => Some(CapabilityGapShape::Malformed),
            "vendoring-multi-goal" => Some(CapabilityGapShape::VendoringMultiGoal),
            _ => None,
        }
    }

    /// `true` iff this shape is a genuine reasoner-fragment gap (a conclusion shape
    /// the native DL fragment cannot soundly refute at all) rather than a
    /// vendoring-format limit. `false` only for [`CapabilityGapShape::VendoringMultiGoal`].
    #[must_use]
    pub fn is_reasoner_fragment_gap(&self) -> bool {
        !matches!(self, CapabilityGapShape::VendoringMultiGoal)
    }

    /// The local name of the `gmeow:GapShape` OWL individual this variant reifies as —
    /// the SINGLE authority tying the ontology's closed `gmeow:GapShape` value class to
    /// this enum, so `slices/core/diagnostics/module.ttl` and the Rust taxonomy can
    /// never drift apart. Used by the conformance reifier
    /// (`gmeow_conformance::divergence::emit_capability_gap_nq`) to mint the
    /// `gmeow:gapShape` object IRI (`{GMEOW}{local}`).
    #[must_use]
    pub fn ontology_individual_local(&self) -> &'static str {
        match self {
            CapabilityGapShape::RoleAssertion => "GapShapeRoleAssertion",
            CapabilityGapShape::ExistentialWitness => "GapShapeExistentialWitness",
            CapabilityGapShape::NativeCoverage => "GapShapeNativeCoverage",
            CapabilityGapShape::Malformed => "GapShapeMalformed",
            CapabilityGapShape::VendoringMultiGoal => "GapShapeVendoringMultiGoal",
        }
    }
}

/// The structured reason a conclusion falls outside the soundly-refutable fragment.
///
/// The token ([`GapShape::as_token`]) is the enumerated `gmeow:gapShape` value the
/// conformance reifier records — so "which conclusion shapes can the native reasoner
/// grade, and which it honestly cannot" is queryable data, not a free-text string.
///
/// This enum classifies only reasoner-FRAGMENT gaps (see [`classify`] /
/// [`classify_conclusion`]) — it deliberately has no `VendoringMultiGoal` variant,
/// because a conjunctive multi-triple conclusion is NOT a fragment gap (`dl_entails`
/// decides it fine); only the single-EDB vendoring format cannot freeze it. Every
/// wire token, including the vendoring-only one, is minted through
/// [`CapabilityGapShape`] — see [`Self::as_capability_shape`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapShape {
    /// A role / property assertion (a bare `a P b`, a `rdfs:domain`/`rdfs:range`
    /// axiom, …). Role negation is not EL-expressible, so it cannot be refuted.
    /// (`rdfs:subPropertyOf` is NOT here: it is decided by hierarchy reachability, not
    /// refutation — see [`ConclusionShape::SubPropertyOf`].)
    RoleAssertion,
    /// A blank-node (existential) subject or object, which needs Skolem-witness
    /// semantics outside the ground-refutation fragment.
    ExistentialWitness,
    /// The native DL engine reported a coverage gap on the reduced EDB — it cannot
    /// honestly decide the reduction (distinct from a shape it refused up front).
    NativeCoverage,
    /// A malformed conclusion triple (e.g. a literal where a class IRI is required).
    Malformed,
}

impl GapShape {
    /// The corresponding [`CapabilityGapShape`] — the single authority this enum
    /// delegates its wire token to (see [`Self::as_token`]).
    #[must_use]
    pub fn as_capability_shape(&self) -> CapabilityGapShape {
        match self {
            GapShape::RoleAssertion => CapabilityGapShape::RoleAssertion,
            GapShape::ExistentialWitness => CapabilityGapShape::ExistentialWitness,
            GapShape::NativeCoverage => CapabilityGapShape::NativeCoverage,
            GapShape::Malformed => CapabilityGapShape::Malformed,
        }
    }

    /// The stable enumerated wire token for this gap shape, delegated through
    /// [`CapabilityGapShape`] (the single `gmeow:gapShape` token authority) so this
    /// and every other producer stay byte-identical by construction.
    #[must_use]
    pub fn as_token(&self) -> &'static str {
        self.as_capability_shape().as_token()
    }
}

/// A conclusion the native fragment cannot soundly refute: a structured shape plus a
/// human detail. Never a silent skip, never a guessed verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntailmentGap {
    /// The structured shape of the gap (the reified `gmeow:gapShape` token).
    pub shape: GapShape,
    /// A human-readable detail for the ledger row / diagnostic.
    pub detail: String,
}

/// The native entailment verdict for `A ⊨ C`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntailmentVerdict {
    /// `A ⊨ C`: every model of the premise satisfies the conclusion (the reduced
    /// EDB was inconsistent for every conclusion component).
    Entailed,
    /// `A ⊭ C`: some model of the premise falsifies the conclusion (a reduced EDB
    /// was consistent).
    NotEntailed,
    /// The conclusion is outside the soundly-refutable fragment.
    Gap(EntailmentGap),
}

impl EntailmentVerdict {
    /// The stable wire token for this verdict.
    #[must_use]
    pub fn as_token(&self) -> &'static str {
        match self {
            EntailmentVerdict::Entailed => "entailed",
            EntailmentVerdict::NotEntailed => "not-entailed",
            EntailmentVerdict::Gap(_) => "gap",
        }
    }
}

/// One native entailment decision together with the work actually performed.
///
/// `decisions` counts conclusion components inspected before the conjunctive
/// question was settled. `steps` is the sum of the native inference budget consumed
/// by the refutation runs among those components. Reachability-only property
/// hierarchy components count as decisions but consume no native inference steps.
/// `budget` is present only when every executed refutation run declared a finite
/// allowance; the ordinary installed operation is unbounded and therefore reports
/// `None` rather than manufacturing a ceiling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntailmentAssessment {
    /// The operation-specific answer.
    pub verdict: EntailmentVerdict,
    /// Conclusion components inspected before the conjunction was settled.
    pub decisions: u64,
    /// Native inference steps consumed by the executed refutation runs.
    pub steps: u64,
    /// Sum of finite per-run allowances, or `None` when any run was unbounded.
    pub budget: Option<u64>,
}

impl EntailmentAssessment {
    fn new(verdict: EntailmentVerdict, decisions: u64, steps: u64, budget: Option<u64>) -> Self {
        Self {
            verdict,
            decisions,
            steps,
            budget,
        }
    }
}

/// A sound fresh-symbol minter for one entailment check.
///
/// Constructed from the premise∪conclusion vocabulary; [`Minter::new`] hard-fails if
/// any input IRI is already in [`ENTAIL_RESERVED_NS`], which guarantees every symbol
/// this minter produces is disjoint from the input vocabulary (soundness floor).
#[derive(Debug, Clone)]
pub struct Minter {
    _private: (),
}

impl Minter {
    /// Build a minter over the given input vocabulary.
    ///
    /// # Errors
    /// Hard-fails if any input IRI is within [`ENTAIL_RESERVED_NS`] — a collision
    /// with the reserved minting namespace could flip a verdict, so it is never
    /// tolerated (no-optionality).
    pub fn new(input_iris: &BTreeSet<String>) -> Result<Self, Diag> {
        for iri in input_iris {
            if iri.starts_with(ENTAIL_RESERVED_NS) {
                return Err(entail_err(format!(
                    "input vocabulary contains a reserved entailment IRI {iri:?} (namespace \
                     {ENTAIL_RESERVED_NS:?}); minted refutation symbols would collide, which \
                     could flip the entailment verdict — refusing (soundness floor)"
                )));
            }
        }
        Ok(Self { _private: () })
    }

    /// The fresh complement class for `class` (disjoint, content-addressed).
    fn complement(&self, class: &str) -> String {
        let h = blake3::hash(class.as_bytes()).to_hex();
        format!("{ENTAIL_RESERVED_NS}complement-{}", &h[..16])
    }

    /// The fresh witness individual for refuting a subsumption whose antecedent is `class`.
    fn witness(&self, class: &str) -> String {
        let h = blake3::hash(class.as_bytes()).to_hex();
        format!("{ENTAIL_RESERVED_NS}witness-{}", &h[..16])
    }
}

/// The refutation triples for one *refutable* conclusion shape (all IRIs).
///
/// Unioning these into the premise's world yields an EDB that is inconsistent iff
/// the premise entails the shape.
///
/// # Contract
/// Only [`ConclusionShape::GroundType`] and [`ConclusionShape::SubClassOf`] are decided
/// by refutation. [`ConclusionShape::SubPropertyOf`] is decided by hierarchy REACHABILITY
/// ([`decide_subproperty`]), never by negation — its role-complement `¬Q` is not
/// EL-expressible, so there is no sound refutation EDB for it. Callers MUST route a
/// subproperty conclusion through the reachability decider; passing one here is an
/// internal-invariant violation and HARD-FAILS rather than silently returning an
/// empty/garbage negation (which could mask a real clash and flip the verdict).
///
/// # Errors
/// Hard-fails ([`Diag`]) iff `shape` is a [`ConclusionShape::SubPropertyOf`].
pub fn negate(
    shape: &ConclusionShape,
    minter: &Minter,
) -> Result<Vec<(String, String, String)>, Diag> {
    match shape {
        ConclusionShape::GroundType { subject, class } => {
            let c_bar = minter.complement(class);
            Ok(vec![
                (class.clone(), OWL_DISJOINTWITH.to_string(), c_bar.clone()),
                (subject.clone(), RDF_TYPE.to_string(), c_bar),
            ])
        }
        ConclusionShape::SubClassOf { sub, sup } => {
            let d_bar = minter.complement(sup);
            let w = minter.witness(sub);
            Ok(vec![
                (w.clone(), RDF_TYPE.to_string(), sub.clone()),
                (sup.clone(), OWL_DISJOINTWITH.to_string(), d_bar.clone()),
                (w, RDF_TYPE.to_string(), d_bar),
            ])
        }
        ConclusionShape::SubPropertyOf { sub, sup } => Err(entail_err(format!(
            "negate() called on a subproperty conclusion {sub:?} ⊑ {sup:?}: subproperty \
             entailment is decided by hierarchy reachability (decide_subproperty), not \
             refutation — role negation is not EL-expressible, so there is no sound \
             refutation EDB. This is an internal-invariant violation; refusing to guess."
        ))),
    }
}

/// One owned conclusion node: only the distinction the shape calculus needs.
enum Node {
    Iri(String),
    Blank,
    Literal,
}

fn node_of(term: TermRef<'_>) -> Node {
    match term {
        TermRef::Iri(iri) => Node::Iri(iri.to_owned()),
        TermRef::Blank { .. } => Node::Blank,
        TermRef::Literal { .. } => Node::Literal,
        TermRef::Triple { .. } => Node::Literal, // a quoted triple is not a refutable subject/object here
    }
}

/// Classify one conclusion triple into a decidable [`ConclusionShape`] (a refutable
/// `GroundType`/`SubClassOf`, or a reachability-decided `SubPropertyOf`), or the
/// structured [`GapShape`] explaining why it is outside the fragment.
fn classify(subject: &Node, predicate: &str, object: &Node) -> Result<ConclusionShape, GapShape> {
    if matches!(subject, Node::Blank) || matches!(object, Node::Blank) {
        return Err(GapShape::ExistentialWitness);
    }
    match predicate {
        RDF_TYPE => match (subject, object) {
            (Node::Iri(s), Node::Iri(o)) => Ok(ConclusionShape::GroundType {
                subject: s.clone(),
                class: o.clone(),
            }),
            _ => Err(GapShape::Malformed),
        },
        RDFS_SUBCLASSOF => match (subject, object) {
            (Node::Iri(s), Node::Iri(o)) => Ok(ConclusionShape::SubClassOf {
                sub: s.clone(),
                sup: o.clone(),
            }),
            _ => Err(GapShape::Malformed),
        },
        // rdfs:subPropertyOf between two IRIs is NOT a refutation shape (role-complement
        // negation is not EL-expressible), but it IS decidable by hierarchy reachability
        // over the premise's property graph — route it to the non-refutation decider.
        // A blank endpoint is already an ExistentialWitness gap (caught above); a literal
        // endpoint (a property expression / malformed axiom) stays a Malformed gap.
        RDFS_SUBPROPERTYOF => match (subject, object) {
            (Node::Iri(s), Node::Iri(o)) => Ok(ConclusionShape::SubPropertyOf {
                sub: s.clone(),
                sup: o.clone(),
            }),
            _ => Err(GapShape::Malformed),
        },
        // domain/range and any bare role/data assertion `a P b` conclude a property
        // relationship whose negation (role complement) is not EL-expressible — an honest
        // role-assertion gap.
        _ => Err(GapShape::RoleAssertion),
    }
}

/// Classify every asserted native conclusion row into a [`ConclusionShape`],
/// returning the first structured [`EntailmentGap`] on an un-refutable statement.
/// Only a conclusion with no ordinary, reifier or annotation rows is empty.
fn classify_conclusion(
    admitted: &AdmittedDefaultGraph<'_>,
) -> Result<Vec<ConclusionShape>, EntailmentGap> {
    let conclusion = admitted.dataset();
    let mut shapes: Vec<ConclusionShape> = Vec::new();
    for q in assertions(conclusion) {
        let TermRef::Iri(pred) = conclusion.resolve(q.p) else {
            return Err(EntailmentGap {
                shape: GapShape::Malformed,
                detail: "conclusion triple has a non-IRI predicate".to_string(),
            });
        };
        let subject = node_of(conclusion.resolve(q.s));
        let object = node_of(conclusion.resolve(q.o));
        match classify(&subject, pred, &object) {
            // Deduplicate: distinct-but-equivalent conclusion triples classify to the
            // same shape, and each shape drives one native consistency
            // refutation in `dl_entails`. Grading a shape twice is redundant work with
            // no change in verdict, so collapse equal shapes here.
            Ok(shape) => {
                if !shapes.contains(&shape) {
                    shapes.push(shape);
                }
            }
            Err(gap_shape) => {
                return Err(EntailmentGap {
                    shape: gap_shape,
                    detail: format!(
                        "conclusion component on predicate {pred:?} is outside the \
                         soundly-refutable fragment ({})",
                        gap_shape.as_token()
                    ),
                });
            }
        }
    }
    Ok(shapes)
}

/// The single-goal reduction of a conclusion for VENDORING it as one committed
/// consistency case whose `input.nq` is `premise ∪ ¬C`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VendorReduction {
    /// A single-triple conclusion: the negation triples to union with the premise's
    /// world. The reduced EDB (premise ∪ these) is inconsistent iff the premise
    /// entails the conclusion — a single native consistency check.
    Single(Vec<(String, String, String)>),
    /// A conclusion [`dl_entails`] DECIDES but that has no single frozen consistency
    /// `input.nq` — either a conjunctive multi-triple conclusion (decided as *n*
    /// independent consistency checks) or a `rdfs:subPropertyOf` conclusion (decided by
    /// hierarchy reachability, not a consistency reduction at all). Neither can be frozen
    /// as one `input.nq`, so the vendoring lane records it as a decidable-but-not-freezable
    /// case rather than a [`VendorReduction::Single`] refutation case.
    MultiGoal,
    /// The conclusion is outside the soundly-refutable fragment.
    Gap(EntailmentGap),
}

/// Reduce a conclusion for vendoring as one committed consistency case.
///
/// A single refutable triple yields [`VendorReduction::Single`] (the negation triples
/// to union with the premise's world); a conjunctive multi-triple conclusion is
/// [`VendorReduction::MultiGoal`] (decidable, but not as one EDB); an un-refutable or
/// empty conclusion is [`VendorReduction::Gap`]. The same default-context admission
/// as [`dl_entails`] runs before returning any nonempty reduction.
///
/// # Errors
/// Hard-fails only on the reserved-namespace soundness guard.
pub fn reduce_for_vendoring(
    premise: &RdfDataset,
    conclusion: &RdfDataset,
) -> Result<VendorReduction, Diag> {
    let admitted_conclusion = match AdmittedDefaultGraph::new(conclusion, "conclusion") {
        Ok(admitted) => admitted,
        Err(gap) => return Ok(VendorReduction::Gap(gap)),
    };
    let shapes = match classify_conclusion(&admitted_conclusion) {
        Ok(shapes) => shapes,
        Err(gap) => return Ok(VendorReduction::Gap(gap)),
    };
    if !shapes.is_empty()
        && let Err(gap) = AdmittedDefaultGraph::new(premise, "premise")
    {
        return Ok(VendorReduction::Gap(gap));
    }
    match shapes.as_slice() {
        [] => Ok(VendorReduction::Gap(EntailmentGap {
            shape: GapShape::Malformed,
            detail: "empty conclusion (nothing to vendor as a refutation case)".to_string(),
        })),
        // A subproperty conclusion is decided by reachability, not by a consistency
        // reduction, so it has no `Single` refutation EDB to freeze — bucket it with the
        // other decidable-but-not-freezable conclusions (never call `negate` on it).
        [ConclusionShape::SubPropertyOf { .. }] => Ok(VendorReduction::MultiGoal),
        [shape] => {
            let mut input_iris: BTreeSet<String> = BTreeSet::new();
            collect_iris(premise, &mut input_iris);
            collect_iris(conclusion, &mut input_iris);
            let minter = Minter::new(&input_iris)?;
            Ok(VendorReduction::Single(negate(shape, &minter)?))
        }
        _ => Ok(VendorReduction::MultiGoal),
    }
}

/// Collect every live IRI, including native annotations, quotations and literal
/// datatypes. The reserved-symbol guard cannot ignore an input carrier.
fn collect_iris(ds: &RdfDataset, out: &mut BTreeSet<String>) {
    let mut pending = Vec::new();
    let mut visited = BTreeSet::new();
    for quad in assertions(ds) {
        pending.extend([quad.s, quad.p, quad.o]);
        pending.extend(quad.g);
        while let Some(term) = pending.pop() {
            if !visited.insert(term) {
                continue;
            }
            match ds.resolve(term) {
                TermRef::Iri(iri) => {
                    out.insert(iri.to_owned());
                }
                TermRef::Triple { s, p, o } => pending.extend([s, p, o]),
                TermRef::Literal { datatype, .. } => pending.push(datatype),
                TermRef::Blank { .. } => {}
            }
        }
    }
}

/// Place the admitted default premise in [`ENTAIL_WORLD`] for one refutation
/// goal. Keep native reifiers, annotations and source locations; quoting a triple
/// does not assert it. No source named graph can pass the admission boundary.
fn build_world_edb(
    admitted: &AdmittedDefaultGraph<'_>,
    negation: &[(String, String, String)],
) -> Result<Arc<RdfDataset>, Diag> {
    let world = RdfTerm::iri(ENTAIL_WORLD);
    let premise = admitted.dataset();
    let mut builder = RdfDatasetBuilder::new();
    for quad in premise.owned_quads() {
        builder.push_owned_quad(&quad.in_graph(world.clone()));
    }
    for reifier in premise.owned_reifiers() {
        builder.push_owned_reifier(&reifier.in_graph(Some(world.clone())));
    }
    for annotation in premise.owned_annotations() {
        builder.push_owned_annotation(&annotation.in_graph(Some(world.clone())));
    }
    for (s, p, o) in negation {
        let quad = RdfQuad::new(RdfTerm::iri(s.clone()), p.clone(), RdfTerm::iri(o.clone()))
            .in_graph(world.clone());
        builder.push_owned_quad(&quad);
    }
    builder
        .freeze()
        .map_err(|e| entail_err(format!("reduced entailment EDB failed to build: {e}")))
}

/// Execute one independently reduced goal in the admitted default-context profile.
/// The domain authority belongs to this reduction, not to every physical graph
/// present in a prepared input. The fixed domain authority is independent of the
/// component's changing source content, which native input admission binds separately.
fn reason_refutation(
    admitted: &AdmittedDefaultGraph<'_>,
    negation: &[(String, String, String)],
) -> Result<crate::result::ReasoningResult, Diag> {
    use crate::reason::{DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld};

    let edb = build_world_edb(admitted, negation)?;
    let input = crate::reason::prepare_reasoning_input(edb.as_ref())?;
    const AUTHORITY: &str = "gmeow.entail.default-refutation.v1";
    let selection = serde_json::to_vec(&(
        AUTHORITY,
        DomainProfile::NonemptyObjectDomainV1,
        ENTAIL_WORLD,
    ))
    .map_err(|error| entail_err(format!("entailment domain identity: {error}")))?;
    let domains = SelectedDomains::new([SelectedLogicalWorld::new(
        LogicalGraph::Named(purrdf::TermValue::iri(ENTAIL_WORLD)),
        DomainProfile::NonemptyObjectDomainV1,
        AUTHORITY.to_owned(),
        *blake3::hash(&selection).as_bytes(),
    )?])?;
    crate::reason::reason_all(input, &domains)
}

/// The verdict of the non-refutation subproperty-reachability decider
/// ([`decide_subproperty`]).
#[derive(Debug, Clone, PartialEq, Eq)]
enum SubPropertyDecision {
    /// `A ⊨ (P ⊑ Q)`: `Q` is in `P`'s reflexive-transitive property-hierarchy closure
    /// (or `Q` is a universal super-property). Unconditionally sound.
    Entailed,
    /// `A ⊭ (P ⊑ Q)`: `Q` is unreachable AND the premise is a pure property hierarchy
    /// (only simple `subPropertyOf`/`equivalentProperty` IRI edges), which is always
    /// satisfiable and whose closure is the exact set of entailed subproperty facts.
    NotEntailed,
    /// `Q` is unreachable but the premise carries a property-relating construct that
    /// could derive further subproperty facts (or make the premise inconsistent), so the
    /// reachability closure is not a complete account — refuse to guess `NotEntailed`.
    Undecidable(String),
}

/// Decide `premise ⊨ (sub rdfs:subPropertyOf sup)` by REFLEXIVE-TRANSITIVE REACHABILITY
/// over the premise's property hierarchy — a SOUND, non-refutation procedure (role
/// negation, which refutation would need, is not EL-expressible).
///
/// # Procedure
/// 1. `Entailed` immediately if `sub == sup` (rdfs6 reflexivity: `P ⊑ P` always) or `sup`
///    is `owl:topObjectProperty`/`owl:topDataProperty` (the universal super-property).
/// 2. Build a directed graph over property IRIs from the premise's DEFAULT-GRAPH triples:
///    each `S rdfs:subPropertyOf T` is an edge `S → T`; each `S owl:equivalentProperty T`
///    is edges `S → T` AND `T → S` (equivalence = mutual subproperty). `Entailed` iff
///    `sup` is reachable from `sub` in the reflexive-transitive closure of that graph.
/// 3. If `sup` is unreachable, the verdict depends on whether the premise is a PURE
///    property hierarchy — every default-graph triple is a simple
///    `subPropertyOf`/`equivalentProperty` edge over IRIs (`restricted`). If so →
///    `NotEntailed`; otherwise → `Undecidable`.
///
/// # Soundness
/// * `Entailed` is unconditionally sound: rdfs5 (subPropertyOf transitivity) and rdfs6
///   (reflexivity) are valid entailment rules, `owl:equivalentProperty` licenses both
///   inclusions, and RDFS ⊨ ⊆ OWL ⊨, so every `Entailed` edge is a genuine entailment.
/// * `NotEntailed` is sound ONLY under the `restricted` gate: a premise whose property
///   axioms are exactly a set of simple `subPropertyOf`/`equivalentProperty` IRI edges is
///   ALWAYS satisfiable (interpret every property as the full domain² relation), so it is
///   never ex-falso; and the reflexive-transitive closure of those edges is EXACTLY the
///   set of subproperty facts it entails, so an unreachable `sup` has a counter-model.
///   Any other construct — a property chain, an inverse, a characteristic type, a
///   `subPropertyOf` over a property EXPRESSION (blank/literal endpoint), a named-graph
///   quad, or a class-level axiom that could make the premise inconsistent — breaks that
///   completeness/consistency guarantee, so we return `Undecidable` (an honest gap),
///   never a guessed `NotEntailed`.
fn decide_subproperty(
    admitted: &AdmittedDefaultGraph<'_>,
    sub: &str,
    sup: &str,
) -> SubPropertyDecision {
    let premise = admitted.dataset();
    // rdfs6 reflexivity, and the universal super-properties.
    if sub == sup || sup == OWL_TOP_OBJECT_PROPERTY || sup == OWL_TOP_DATA_PROPERTY {
        return SubPropertyDecision::Entailed;
    }

    // Build the property-hierarchy graph and the restricted-vocabulary gate in one pass.
    // Determinism: BTreeMap/BTreeSet keep edges sorted; reachability is order-independent.
    let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut restricted = true;
    for q in assertions(premise) {
        debug_assert!(
            q.g.is_none(),
            "source context was admitted before reachability"
        );
        let (TermRef::Iri(pred), TermRef::Iri(s), TermRef::Iri(o)) = (
            premise.resolve(q.p),
            premise.resolve(q.s),
            premise.resolve(q.o),
        ) else {
            // A default-graph triple with a blank/literal endpoint — e.g. a
            // `subPropertyOf` over a property EXPRESSION (blank-node inverse/chain), or a
            // property-chain list — is not a simple edge and can derive subproperty facts
            // outside the closure.
            restricted = false;
            continue;
        };
        if pred == RDFS_SUBPROPERTYOF {
            edges.entry(s.to_owned()).or_default().insert(o.to_owned());
        } else if pred == OWL_EQUIVALENT_PROPERTY {
            edges.entry(s.to_owned()).or_default().insert(o.to_owned());
            edges.entry(o.to_owned()).or_default().insert(s.to_owned());
        } else {
            // Any other predicate (owl:propertyChainAxiom, owl:inverseOf, an rdf:type
            // characteristic assertion, a class/type axiom, …) could derive further
            // subproperty facts or render the premise inconsistent.
            restricted = false;
        }
    }

    // Reflexive-transitive reachability of `sup` from `sub` over the edge graph.
    let mut visited: BTreeSet<String> = BTreeSet::new();
    let mut stack: Vec<String> = vec![sub.to_owned()];
    while let Some(cur) = stack.pop() {
        if cur == sup {
            return SubPropertyDecision::Entailed;
        }
        if !visited.insert(cur.clone()) {
            continue;
        }
        if let Some(succ) = edges.get(&cur) {
            for t in succ {
                if !visited.contains(t) {
                    stack.push(t.clone());
                }
            }
        }
    }

    if restricted {
        SubPropertyDecision::NotEntailed
    } else {
        SubPropertyDecision::Undecidable(format!(
            "cannot decide {sub:?} ⊑ {sup:?}: {sup:?} is not reachable in the premise's \
             subPropertyOf/equivalentProperty closure, but the premise carries a \
             property-relating construct (a property chain, inverse, characteristic type, \
             property-expression endpoint, named-graph quad, or other axiom) that could \
             derive further subproperty facts — refusing to guess NotEntailed"
        ))
    }
}

/// Decide whether `premise` entails `conclusion` by refutation over the native DL
/// consistency calculus.
///
/// The conclusion's triples are each normalized to a [`ConclusionShape`] and negated;
/// the premise entails the conclusion iff EVERY component's reduced EDB
/// (`premise ∪ ¬component`) is inconsistent. An empty conclusion is trivially
/// entailed (`A ⊨ ∅`); this tautology says nothing about premise consistency or
/// source coverage. Nonempty goals require the single default-context profile:
/// named graphs or explicit standpoint/time/module/modality/path selections yield
/// a native-coverage gap. Native annotations remain asserted rows, and reifier
/// bindings do not assert their quoted propositions.
///
/// # Errors
/// Hard-fails ([`Diag`]) only on a soundness-guard violation (a reserved-namespace
/// collision in the input vocabulary) or an internal reasoner / dataset-build error.
/// A conclusion outside the refutable fragment is a [`EntailmentVerdict::Gap`], NOT an
/// error.
pub fn dl_entailment_assessment(
    premise: &RdfDataset,
    conclusion: &RdfDataset,
) -> Result<EntailmentAssessment, Diag> {
    let mut decisions = 0_u64;
    let mut steps = 0_u64;
    let mut aggregate_budget = Some(0_u64);
    let mut refutation_runs = 0_u64;
    let admitted_conclusion = match AdmittedDefaultGraph::new(conclusion, "conclusion") {
        Ok(admitted) => admitted,
        Err(gap) => {
            return Ok(EntailmentAssessment::new(
                EntailmentVerdict::Gap(gap),
                decisions,
                steps,
                None,
            ));
        }
    };
    // Classify every conclusion component; any un-refutable shape makes the whole
    // (conjunctive) conclusion an honest gap.
    let shapes = match classify_conclusion(&admitted_conclusion) {
        Ok(shapes) => shapes,
        Err(gap) => {
            return Ok(EntailmentAssessment::new(
                EntailmentVerdict::Gap(gap),
                decisions,
                steps,
                None,
            ));
        }
    };

    // A ⊨ ∅ — the empty conclusion is trivially entailed.
    if shapes.is_empty() {
        return Ok(EntailmentAssessment::new(
            EntailmentVerdict::Entailed,
            decisions,
            steps,
            None,
        ));
    }
    let admitted_premise = match AdmittedDefaultGraph::new(premise, "premise") {
        Ok(admitted) => admitted,
        Err(gap) => {
            return Ok(EntailmentAssessment::new(
                EntailmentVerdict::Gap(gap),
                decisions,
                steps,
                None,
            ));
        }
    };

    // Build the sound minter over the whole input vocabulary (hard-fail on a reserved
    // collision).
    let mut input_iris: BTreeSet<String> = BTreeSet::new();
    collect_iris(premise, &mut input_iris);
    collect_iris(conclusion, &mut input_iris);
    let minter = Minter::new(&input_iris)?;

    // Every component must be entailed (conjunction): decide each independently. A
    // subproperty component is decided by hierarchy reachability (non-refutation); every
    // other (refutable) component is decided by the negation → consistency refutation.
    for shape in &shapes {
        decisions = decisions.saturating_add(1);
        match shape {
            ConclusionShape::SubPropertyOf { sub, sup } => {
                match decide_subproperty(&admitted_premise, sub, sup) {
                    // Entailed ⇒ this component holds; continue to the next.
                    SubPropertyDecision::Entailed => {}
                    // A counter-model exists (restricted premise), so the conjunction fails.
                    SubPropertyDecision::NotEntailed => {
                        return Ok(EntailmentAssessment::new(
                            EntailmentVerdict::NotEntailed,
                            decisions,
                            steps,
                            if refutation_runs == 0 {
                                None
                            } else {
                                aggregate_budget
                            },
                        ));
                    }
                    // Unreachable but the premise is not a pure hierarchy — honest gap.
                    SubPropertyDecision::Undecidable(detail) => {
                        return Ok(EntailmentAssessment::new(
                            EntailmentVerdict::Gap(EntailmentGap {
                                shape: GapShape::NativeCoverage,
                                detail,
                            }),
                            decisions,
                            steps,
                            if refutation_runs == 0 {
                                None
                            } else {
                                aggregate_budget
                            },
                        ));
                    }
                }
            }
            _ => {
                let negation = negate(shape, &minter)?;
                let result = reason_refutation(&admitted_premise, &negation)?;
                refutation_runs = refutation_runs.saturating_add(1);
                let consumed = result.provenance.consumed_budget;
                steps = steps.saturating_add(consumed.consumed);
                aggregate_budget = match (aggregate_budget, consumed.allowance) {
                    (Some(total), Some(allowance)) => Some(total.saturating_add(allowance)),
                    _ => None,
                };
                let verdict = result.native_verdict()?;
                if !verdict.gaps.is_empty() || !verdict.coverage.unsupported.is_empty() {
                    let codes: Vec<&str> = verdict.gaps.iter().map(|g| g.code.as_str()).collect();
                    return Ok(EntailmentAssessment::new(
                        EntailmentVerdict::Gap(EntailmentGap {
                            shape: GapShape::NativeCoverage,
                            detail: format!(
                                "native DL coverage gap(s) {codes:?}, unsupported constructs {:?} \
                                 on the reduced EDB — the engine cannot honestly decide this entailment",
                                verdict.coverage.unsupported,
                            ),
                        }),
                        decisions,
                        steps,
                        aggregate_budget,
                    ));
                }
                if verdict.consistent {
                    // This component has a counter-model, so the conjunction is not entailed.
                    return Ok(EntailmentAssessment::new(
                        EntailmentVerdict::NotEntailed,
                        decisions,
                        steps,
                        aggregate_budget,
                    ));
                }
                // Inconsistent ⇒ this component is entailed; continue to the next.
            }
        }
    }

    Ok(EntailmentAssessment::new(
        EntailmentVerdict::Entailed,
        decisions,
        steps,
        aggregate_budget,
    ))
}

/// Decide whether `premise` entails `conclusion`, returning only the historical
/// operation verdict. This delegates to [`dl_entailment_assessment`] so the verdict
/// and measured-report paths cannot drift into separate executions.
pub fn dl_entails(
    premise: &RdfDataset,
    conclusion: &RdfDataset,
) -> Result<EntailmentVerdict, Diag> {
    dl_entailment_assessment(premise, conclusion).map(|assessment| assessment.verdict)
}

#[path = "entail.tests.rs"]
#[cfg(test)]
mod tests;
