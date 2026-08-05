// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native entailment by refutation: `A ⊨ C` iff `A ∪ ¬C` is inconsistent.
//!
//! Entailment is a *fundamental* reasoning operation, decided here as a thin
//! composition over the native DL consistency calculus
//! ([`crate::reason::dl_consistency`]) — NOT a second reasoning path. To decide
//! whether a premise graph `A` entails a conclusion `C`, we negate `C` by
//! refutation, union the negation into the premise's world, and ask the native DL
//! clash rule whether the result is inconsistent: if it is, every model of `A`
//! satisfies `C`, so `A ⊨ C`.
//!
//! This module lives OUTSIDE [`crate::reason`] on purpose: it composes the
//! reasoner without adding a rule to it, so it does not perturb
//! [`crate::reason::native_contract_hash`].
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
//! the [`negate`](crate::entail::negate)/[`crate::reason::dl_consistency`] refutation
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

/// The single world IRI every reduced-EDB quad is scoped under. The native chase
/// reasons over named graphs (worlds) and drops default-graph triples, so premise
/// and negation alike are re-scoped here into one world for the consistency check.
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

/// Classify every conclusion triple into a refutable [`ConclusionShape`], returning
/// the first structured [`EntailmentGap`] shape on any un-refutable triple. An empty
/// conclusion yields an empty vector (trivially entailed — `A ⊨ ∅`).
fn classify_conclusion(conclusion: &RdfDataset) -> Result<Vec<ConclusionShape>, EntailmentGap> {
    let mut shapes: Vec<ConclusionShape> = Vec::new();
    for q in conclusion.quads() {
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
            // same shape, and each shape drives one (expensive) `dl_consistency`
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
/// empty conclusion is [`VendorReduction::Gap`].
///
/// # Errors
/// Hard-fails only on the reserved-namespace soundness guard.
pub fn reduce_for_vendoring(
    premise: &RdfDataset,
    conclusion: &RdfDataset,
) -> Result<VendorReduction, Diag> {
    let shapes = match classify_conclusion(conclusion) {
        Ok(shapes) => shapes,
        Err(gap) => return Ok(VendorReduction::Gap(gap)),
    };
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

/// Collect every IRI (subject, predicate, object, graph) in `ds` into `out`.
fn collect_iris(ds: &RdfDataset, out: &mut BTreeSet<String>) {
    for q in ds.quads() {
        for id in [q.s, q.p, q.o] {
            if let TermRef::Iri(iri) = ds.resolve(id) {
                out.insert(iri.to_owned());
            }
        }
        if let Some(g) = q.g
            && let TermRef::Iri(iri) = ds.resolve(g)
        {
            out.insert(iri.to_owned());
        }
    }
}

/// Build the reduced EDB for one refutation goal: every premise quad re-scoped into
/// [`ENTAIL_WORLD`], unioned with the negation triples (also world-scoped).
fn build_world_edb(
    premise: &RdfDataset,
    negation: &[(String, String, String)],
) -> Result<Arc<RdfDataset>, Diag> {
    let world = RdfTerm::iri(ENTAIL_WORLD);
    let mut builder = RdfDatasetBuilder::new();
    for q in premise.quads() {
        let TermRef::Iri(pred) = premise.resolve(q.p) else {
            // A non-IRI predicate is not well-formed RDF; skip it defensively.
            continue;
        };
        let pred = pred.to_owned();
        let subject = premise.to_owned_term(q.s);
        let object = premise.to_owned_term(q.o);
        let quad = RdfQuad::new(subject, pred, object).in_graph(world.clone());
        builder.push_owned_quad(&quad);
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
fn decide_subproperty(premise: &RdfDataset, sub: &str, sup: &str) -> SubPropertyDecision {
    // rdfs6 reflexivity, and the universal super-properties.
    if sub == sup || sup == OWL_TOP_OBJECT_PROPERTY || sup == OWL_TOP_DATA_PROPERTY {
        return SubPropertyDecision::Entailed;
    }

    // Build the property-hierarchy graph and the restricted-vocabulary gate in one pass.
    // Determinism: BTreeMap/BTreeSet keep edges sorted; reachability is order-independent.
    let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut restricted = true;
    for q in premise.quads() {
        if q.g.is_some() {
            // A named-graph quad is not an asserted default-graph property axiom; its
            // content is unaccounted for by the reachability closure.
            restricted = false;
            continue;
        }
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
/// entailed (`A ⊨ ∅`).
///
/// # Errors
/// Hard-fails ([`Diag`]) only on a soundness-guard violation (a reserved-namespace
/// collision in the input vocabulary) or an internal reasoner / dataset-build error.
/// A conclusion outside the refutable fragment is a [`EntailmentVerdict::Gap`], NOT an
/// error.
pub fn dl_entails(
    premise: &RdfDataset,
    conclusion: &RdfDataset,
) -> Result<EntailmentVerdict, Diag> {
    // Classify every conclusion component; any un-refutable shape makes the whole
    // (conjunctive) conclusion an honest gap.
    let shapes = match classify_conclusion(conclusion) {
        Ok(shapes) => shapes,
        Err(gap) => return Ok(EntailmentVerdict::Gap(gap)),
    };

    // A ⊨ ∅ — the empty conclusion is trivially entailed.
    if shapes.is_empty() {
        return Ok(EntailmentVerdict::Entailed);
    }

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
        match shape {
            ConclusionShape::SubPropertyOf { sub, sup } => {
                match decide_subproperty(premise, sub, sup) {
                    // Entailed ⇒ this component holds; continue to the next.
                    SubPropertyDecision::Entailed => {}
                    // A counter-model exists (restricted premise), so the conjunction fails.
                    SubPropertyDecision::NotEntailed => {
                        return Ok(EntailmentVerdict::NotEntailed);
                    }
                    // Unreachable but the premise is not a pure hierarchy — honest gap.
                    SubPropertyDecision::Undecidable(detail) => {
                        return Ok(EntailmentVerdict::Gap(EntailmentGap {
                            shape: GapShape::NativeCoverage,
                            detail,
                        }));
                    }
                }
            }
            _ => {
                let negation = negate(shape, &minter)?;
                let edb = build_world_edb(premise, &negation)?;
                let verdict = crate::reason::dl_consistency(edb.as_ref())?;
                if !verdict.gaps.is_empty() {
                    let codes: Vec<&str> = verdict.gaps.iter().map(|g| g.code.as_str()).collect();
                    return Ok(EntailmentVerdict::Gap(EntailmentGap {
                        shape: GapShape::NativeCoverage,
                        detail: format!(
                            "native DL coverage gap(s) {codes:?} on the reduced EDB — the engine \
                             cannot honestly decide this entailment"
                        ),
                    }));
                }
                if verdict.consistent {
                    // This component has a counter-model, so the conjunction is not entailed.
                    return Ok(EntailmentVerdict::NotEntailed);
                }
                // Inconsistent ⇒ this component is entailed; continue to the next.
            }
        }
    }

    Ok(EntailmentVerdict::Entailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RDF_XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

    fn dataset(nq: &str) -> Arc<RdfDataset> {
        purrdf::parse_dataset(nq.as_bytes(), "application/n-quads", None)
            .unwrap_or_else(|e| panic!("N-Quads parse failed: {e}\n{nq}"))
    }

    /// Premise `a ⊑ b`, `x ∈ a` entails `x ∈ b`.
    #[test]
    fn ground_type_positive_entailment_is_entailed() {
        let premise = dataset(
            "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/a> .\n\
             <http://ex/a> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://ex/b> .\n",
        );
        let conclusion = dataset(
            "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/b> .\n",
        );
        assert_eq!(
            dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap(),
            EntailmentVerdict::Entailed
        );
    }

    /// Premise `x ∈ a` alone does NOT entail `x ∈ b`.
    #[test]
    fn ground_type_non_entailment_is_not_entailed() {
        let premise = dataset(
            "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/a> .\n",
        );
        let conclusion = dataset(
            "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/b> .\n",
        );
        assert_eq!(
            dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap(),
            EntailmentVerdict::NotEntailed
        );
    }

    /// Premise `a ⊑ b`, `b ⊑ c` entails `a ⊑ c` (subsumption, via a fresh witness).
    #[test]
    fn subclass_positive_entailment_is_entailed() {
        let premise = dataset(
            "<http://ex/a> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://ex/b> .\n\
             <http://ex/b> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://ex/c> .\n",
        );
        let conclusion = dataset(
            "<http://ex/a> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://ex/c> .\n",
        );
        assert_eq!(
            dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap(),
            EntailmentVerdict::Entailed
        );
    }

    /// `a ⊑ b` does NOT entail `a ⊑ c`.
    #[test]
    fn subclass_non_entailment_is_not_entailed() {
        let premise = dataset(
            "<http://ex/a> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://ex/b> .\n",
        );
        let conclusion = dataset(
            "<http://ex/a> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://ex/c> .\n",
        );
        assert_eq!(
            dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap(),
            EntailmentVerdict::NotEntailed
        );
    }

    /// A premise authored in the CANONICAL `logic:` subsumption vocabulary entails the
    /// same memberships and subsumptions as one authored in its `rdfs:` projection.
    ///
    /// This is the consumer-visible statement of
    /// [`crate::reason::edb_predicate_spellings`]: `gmeow entails` composes over
    /// [`crate::reason::dl_consistency`], which folds from the ONE chase
    /// [`crate::reason::build_edb_facts`] feeds. Every `module.ttl` authors subsumption
    /// as `logic:subClassOf` (Principle 17 — `rdfs:` is one of its lossy projections),
    /// so without the EDB-boundary lowering a consumer asking "is this class a
    /// `math:MathConformanceFailure`?" of the shipped bundle gets `not-entailed`: the
    /// enforcement fires while the taxonomy stays dark.
    #[test]
    fn canonical_logic_subsumption_is_entailment_equivalent_to_its_rdfs_projection() {
        const LOGIC_SUBCLASS: &str = "https://blackcatinformatics.ca/logic/subClassOf";
        let premise = dataset(&format!(
            "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/a> .\n\
             <http://ex/a> <{LOGIC_SUBCLASS}> <http://ex/b> .\n\
             <http://ex/b> <{LOGIC_SUBCLASS}> <http://ex/c> .\n"
        ));
        let membership = dataset(
            "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/c> .\n",
        );
        let subsumption = dataset(
            "<http://ex/a> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://ex/c> .\n",
        );
        assert_eq!(
            dl_entails(premise.as_ref(), membership.as_ref()).unwrap(),
            EntailmentVerdict::Entailed,
            "x ∈ c must follow from a canonically-spelled a ⊑ b ⊑ c"
        );
        assert_eq!(
            dl_entails(premise.as_ref(), subsumption.as_ref()).unwrap(),
            EntailmentVerdict::Entailed,
            "a ⊑ c must follow from a canonically-spelled a ⊑ b ⊑ c"
        );

        // The lowering ADDS the taxonomy; it does not make everything entailed.
        let unrelated = dataset(
            "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/d> .\n",
        );
        assert_eq!(
            dl_entails(premise.as_ref(), unrelated.as_ref()).unwrap(),
            EntailmentVerdict::NotEntailed
        );
    }

    /// A multi-triple conjunctive conclusion is entailed iff EVERY component is.
    #[test]
    fn multi_triple_conjunction_all_entailed_is_entailed() {
        let premise = dataset(
            "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/a> .\n\
             <http://ex/a> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://ex/b> .\n\
             <http://ex/a> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://ex/c> .\n",
        );
        let conclusion = dataset(
            "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/b> .\n\
             <http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/c> .\n",
        );
        assert_eq!(
            dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap(),
            EntailmentVerdict::Entailed
        );
    }

    /// A multi-triple conclusion with one un-entailed component is NOT entailed
    /// (the conjunction, not a disjunction).
    #[test]
    fn multi_triple_conjunction_one_failing_is_not_entailed() {
        let premise = dataset(
            "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/a> .\n\
             <http://ex/a> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://ex/b> .\n",
        );
        let conclusion = dataset(
            "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/b> .\n\
             <http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/c> .\n",
        );
        assert_eq!(
            dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap(),
            EntailmentVerdict::NotEntailed
        );
    }

    /// An empty conclusion is trivially entailed.
    #[test]
    fn empty_conclusion_is_entailed() {
        let premise = dataset(
            "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/a> .\n",
        );
        let conclusion = dataset("");
        assert_eq!(
            dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap(),
            EntailmentVerdict::Entailed
        );
    }

    /// A blank-node conclusion subject is an existential-witness gap, not a verdict.
    #[test]
    fn blank_node_conclusion_is_existential_gap() {
        let premise = dataset(
            "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/a> .\n",
        );
        let conclusion =
            dataset("_:b <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/a> .\n");
        let v = dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap();
        assert!(
            matches!(
                v,
                EntailmentVerdict::Gap(EntailmentGap {
                    shape: GapShape::ExistentialWitness,
                    ..
                })
            ),
            "{v:?}"
        );
    }

    /// A role/property-assertion conclusion is a role-assertion gap (role negation is
    /// not EL-expressible).
    #[test]
    fn role_assertion_conclusion_is_role_gap() {
        let premise = dataset("<http://ex/a> <http://ex/knows> <http://ex/b> .\n");
        let conclusion = dataset("<http://ex/a> <http://ex/knows> <http://ex/b> .\n");
        let v = dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap();
        assert!(
            matches!(
                v,
                EntailmentVerdict::Gap(EntailmentGap {
                    shape: GapShape::RoleAssertion,
                    ..
                })
            ),
            "{v:?}"
        );
    }

    /// A conclusion typing an individual to a literal is malformed.
    #[test]
    fn literal_class_conclusion_is_malformed_gap() {
        let premise = dataset(
            "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/a> .\n",
        );
        let conclusion = dataset(&format!(
            "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \"oops\"^^<{RDF_XSD_STRING}> .\n"
        ));
        let v = dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap();
        assert!(
            matches!(
                v,
                EntailmentVerdict::Gap(EntailmentGap {
                    shape: GapShape::Malformed,
                    ..
                })
            ),
            "{v:?}"
        );
    }

    /// SOUNDNESS FLOOR: a premise that already mentions a reserved-namespace IRI is
    /// rejected — a minted complement could otherwise collide and flip the verdict.
    #[test]
    fn reserved_namespace_input_hard_fails() {
        let premise = dataset(&format!(
            "<{ENTAIL_RESERVED_NS}complement-deadbeefdeadbeef> \
             <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/a> .\n"
        ));
        let conclusion = dataset(
            "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/a> .\n",
        );
        let err = dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap_err();
        assert!(
            err.message().contains("reserved entailment IRI"),
            "expected a reserved-namespace hard fail, got {err}"
        );
    }

    /// [`CapabilityGapShape::ontology_individual_local`] is the single naming
    /// authority for the `gmeow:GapShape` individuals: every variant maps to a
    /// distinct local name, and [`CapabilityGapShape::is_reasoner_fragment_gap`]
    /// is true for exactly the first four (reasoner-fragment gaps), false only for
    /// `VendoringMultiGoal` (a vendoring-format limit, not a reasoner gap).
    #[test]
    fn ontology_individual_locals_are_five_distinct_and_match_fragment_gap_flag() {
        let locals: BTreeSet<&'static str> = CapabilityGapShape::ALL
            .iter()
            .map(CapabilityGapShape::ontology_individual_local)
            .collect();
        assert_eq!(
            locals.len(),
            5,
            "all 5 CapabilityGapShape variants must map to distinct gmeow:GapShape locals"
        );
        for shape in &CapabilityGapShape::ALL[..4] {
            assert!(
                shape.is_reasoner_fragment_gap(),
                "{shape:?} must be a reasoner-fragment gap"
            );
        }
        assert!(
            !CapabilityGapShape::VendoringMultiGoal.is_reasoner_fragment_gap(),
            "VendoringMultiGoal is a vendoring-format limit, not a reasoner-fragment gap"
        );
    }

    /// Positive transitive subproperty: `P ⊑ Q`, `Q ⊑ R` entails `P ⊑ R` by
    /// reflexive-transitive reachability (rdfs5).
    #[test]
    fn subproperty_transitive_positive_is_entailed() {
        let premise = dataset(&format!(
            "<http://ex/P> <{RDFS_SUBPROPERTYOF}> <http://ex/Q> .\n\
             <http://ex/Q> <{RDFS_SUBPROPERTYOF}> <http://ex/R> .\n"
        ));
        let conclusion = dataset(&format!(
            "<http://ex/P> <{RDFS_SUBPROPERTYOF}> <http://ex/R> .\n"
        ));
        assert_eq!(
            dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap(),
            EntailmentVerdict::Entailed
        );
    }

    /// Reflexive subproperty: `P ⊑ P` is always entailed (rdfs6), regardless of edges.
    #[test]
    fn subproperty_reflexive_is_entailed() {
        let premise = dataset(&format!(
            "<http://ex/P> <{RDFS_SUBPROPERTYOF}> <http://ex/Q> .\n"
        ));
        let conclusion = dataset(&format!(
            "<http://ex/P> <{RDFS_SUBPROPERTYOF}> <http://ex/P> .\n"
        ));
        assert_eq!(
            dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap(),
            EntailmentVerdict::Entailed
        );
    }

    /// `owl:equivalentProperty` is mutual subproperty: `P ≡ Q` entails BOTH `P ⊑ Q` and
    /// `Q ⊑ P`.
    #[test]
    fn subproperty_equivalent_property_both_directions_are_entailed() {
        let premise = dataset(&format!(
            "<http://ex/P> <{OWL_EQUIVALENT_PROPERTY}> <http://ex/Q> .\n"
        ));
        let p_sub_q = dataset(&format!(
            "<http://ex/P> <{RDFS_SUBPROPERTYOF}> <http://ex/Q> .\n"
        ));
        let q_sub_p = dataset(&format!(
            "<http://ex/Q> <{RDFS_SUBPROPERTYOF}> <http://ex/P> .\n"
        ));
        assert_eq!(
            dl_entails(premise.as_ref(), p_sub_q.as_ref()).unwrap(),
            EntailmentVerdict::Entailed
        );
        assert_eq!(
            dl_entails(premise.as_ref(), q_sub_p.as_ref()).unwrap(),
            EntailmentVerdict::Entailed
        );
    }

    /// Negative subproperty in a restricted (pure-hierarchy) premise: `P ⊑ Q` does NOT
    /// entail the reverse `Q ⊑ P` — unreachable, so a sound `NotEntailed`.
    #[test]
    fn subproperty_unreachable_restricted_is_not_entailed() {
        let premise = dataset(&format!(
            "<http://ex/P> <{RDFS_SUBPROPERTYOF}> <http://ex/Q> .\n"
        ));
        let conclusion = dataset(&format!(
            "<http://ex/Q> <{RDFS_SUBPROPERTYOF}> <http://ex/P> .\n"
        ));
        assert_eq!(
            dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap(),
            EntailmentVerdict::NotEntailed
        );
    }

    /// Unrelated subproperty edges do not entail the conclusion: a restricted premise
    /// with only `X ⊑ Y` does NOT entail `P ⊑ Q`.
    #[test]
    fn subproperty_unrelated_restricted_is_not_entailed() {
        let premise = dataset(&format!(
            "<http://ex/X> <{RDFS_SUBPROPERTYOF}> <http://ex/Y> .\n"
        ));
        let conclusion = dataset(&format!(
            "<http://ex/P> <{RDFS_SUBPROPERTYOF}> <http://ex/Q> .\n"
        ));
        assert_eq!(
            dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap(),
            EntailmentVerdict::NotEntailed
        );
    }

    /// SOUNDNESS GATE: when the premise carries a property-relating construct beyond
    /// `subPropertyOf`/`equivalentProperty` (here an `owl:propertyChainAxiom`), an
    /// unreachable conclusion is an honest `native-coverage` GAP — NEVER a guessed
    /// `NotEntailed`, because the chain axiom could derive further subproperty facts.
    #[test]
    fn subproperty_property_construct_makes_unreachable_a_gap() {
        const OWL_PROPERTY_CHAIN_AXIOM: &str = "http://www.w3.org/2002/07/owl#propertyChainAxiom";
        let premise = dataset(&format!(
            "<http://ex/P> <{RDFS_SUBPROPERTYOF}> <http://ex/Q> .\n\
             <http://ex/R> <{OWL_PROPERTY_CHAIN_AXIOM}> _:chain .\n"
        ));
        // `Q ⊑ P` is unreachable, but the chain axiom voids the restricted-vocabulary gate.
        let conclusion = dataset(&format!(
            "<http://ex/Q> <{RDFS_SUBPROPERTYOF}> <http://ex/P> .\n"
        ));
        let v = dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap();
        assert!(
            matches!(
                v,
                EntailmentVerdict::Gap(EntailmentGap {
                    shape: GapShape::NativeCoverage,
                    ..
                })
            ),
            "{v:?}"
        );
    }

    /// The same soundness gate fires for `owl:inverseOf` (another derivation-capable
    /// property construct): an unreachable subproperty conclusion is a GAP, not a verdict.
    #[test]
    fn subproperty_inverse_of_construct_makes_unreachable_a_gap() {
        const OWL_INVERSE_OF: &str = "http://www.w3.org/2002/07/owl#inverseOf";
        let premise = dataset(&format!(
            "<http://ex/P> <{RDFS_SUBPROPERTYOF}> <http://ex/Q> .\n\
             <http://ex/P> <{OWL_INVERSE_OF}> <http://ex/Pinv> .\n"
        ));
        let conclusion = dataset(&format!(
            "<http://ex/Q> <{RDFS_SUBPROPERTYOF}> <http://ex/P> .\n"
        ));
        let v = dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap();
        assert!(
            matches!(
                v,
                EntailmentVerdict::Gap(EntailmentGap {
                    shape: GapShape::NativeCoverage,
                    ..
                })
            ),
            "{v:?}"
        );
    }

    /// `negate` HARD-FAILS on a subproperty shape: it is decided by reachability, never
    /// refutation, so a caller that routes it to `negate` violates the contract and must
    /// not receive a silent empty/garbage negation.
    #[test]
    fn negate_refuses_subproperty_shape() {
        let minter = Minter::new(&BTreeSet::new()).unwrap();
        let shape = ConclusionShape::SubPropertyOf {
            sub: "http://ex/P".to_string(),
            sup: "http://ex/Q".to_string(),
        };
        let err = negate(&shape, &minter).unwrap_err();
        assert!(
            err.message().contains("subproperty conclusion"),
            "expected a subproperty-invariant hard fail, got {err}"
        );
    }

    /// A subproperty conclusion with a literal superproperty is malformed (not a
    /// reachability edge and not refutable).
    #[test]
    fn subproperty_literal_object_is_malformed_gap() {
        let premise = dataset(&format!(
            "<http://ex/P> <{RDFS_SUBPROPERTYOF}> <http://ex/Q> .\n"
        ));
        let conclusion = dataset(&format!(
            "<http://ex/P> <{RDFS_SUBPROPERTYOF}> \"oops\"^^<{RDF_XSD_STRING}> .\n"
        ));
        let v = dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap();
        assert!(
            matches!(
                v,
                EntailmentVerdict::Gap(EntailmentGap {
                    shape: GapShape::Malformed,
                    ..
                })
            ),
            "{v:?}"
        );
    }

    /// The minter is deterministic and content-addressed: same class → same symbol,
    /// different classes → different symbols, and complement ≠ witness.
    #[test]
    fn minted_symbols_are_deterministic_and_distinct() {
        let minter = Minter::new(&BTreeSet::new()).unwrap();
        assert_eq!(
            minter.complement("http://ex/a"),
            minter.complement("http://ex/a")
        );
        assert_ne!(
            minter.complement("http://ex/a"),
            minter.complement("http://ex/b")
        );
        assert_ne!(
            minter.complement("http://ex/a"),
            minter.witness("http://ex/a")
        );
        assert!(
            minter
                .complement("http://ex/a")
                .starts_with(ENTAIL_RESERVED_NS)
        );
    }
}
