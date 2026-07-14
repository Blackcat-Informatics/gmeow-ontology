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
//! This module lives OUTSIDE [`crate::reason`] on purpose (mirroring
//! [`crate::entail_oracle`]): it composes the reasoner without adding a rule to it,
//! so it does not perturb [`crate::reason::native_contract_hash`].
//!
//! ## The conclusion-shape calculus (the one negation waist)
//!
//! A conclusion is a set of RDF triples. Each triple is normalized into a
//! [`ConclusionShape`] and negated by one shared [`negate`] primitive:
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
//! ## Sound fresh-symbol minting (the soundness floor)
//!
//! The fresh complement/witness IRIs are minted in a reserved namespace
//! ([`ENTAIL_RESERVED_NS`]) with a blake3-of-input suffix, and [`Minter::new`]
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
//! an [`EntailmentGap`] carrying a structured [`GapShape`] token, never a silent
//! skip and never a guessed verdict.

use std::collections::BTreeSet;
use std::sync::Arc;

use purrdf::{RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm, TermRef};

use gmeow_errors::Diag;

/// The RDF `type` predicate.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// The RDFS `subClassOf` predicate.
const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
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

/// A conclusion triple the native DL fragment can soundly refute.
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
    /// A role / property assertion (a bare `a P b`, `rdfs:subPropertyOf`,
    /// domain/range, …). Role negation is not EL-expressible, so it cannot be refuted.
    /// Reasoner-fragment gap.
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
    /// ([`gmeow_conformance::divergence::emit_capability_gap_nq`]) to mint the
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
    /// A role / property assertion (a bare `a P b`, `rdfs:subPropertyOf`,
    /// domain/range, …). Role negation is not EL-expressible, so it cannot be refuted.
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

/// The refutation triples for one conclusion shape (all IRIs).
///
/// Unioning these into the premise's world yields an EDB that is inconsistent iff
/// the premise entails the shape.
#[must_use]
pub fn negate(shape: &ConclusionShape, minter: &Minter) -> Vec<(String, String, String)> {
    match shape {
        ConclusionShape::GroundType { subject, class } => {
            let c_bar = minter.complement(class);
            vec![
                (class.clone(), OWL_DISJOINTWITH.to_string(), c_bar.clone()),
                (subject.clone(), RDF_TYPE.to_string(), c_bar),
            ]
        }
        ConclusionShape::SubClassOf { sub, sup } => {
            let d_bar = minter.complement(sup);
            let w = minter.witness(sub);
            vec![
                (w.clone(), RDF_TYPE.to_string(), sub.clone()),
                (sup.clone(), OWL_DISJOINTWITH.to_string(), d_bar.clone()),
                (w, RDF_TYPE.to_string(), d_bar),
            ]
        }
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

/// Classify one conclusion triple into a refutable [`ConclusionShape`], or the
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
        // rdfs:subPropertyOf, domain/range, and any bare role/data assertion `a P b`
        // conclude a property relationship whose negation (role complement) is not
        // EL-expressible — an honest role-assertion gap.
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
            Ok(shape) => shapes.push(shape),
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
    /// A conjunctive multi-triple conclusion: decidable (by [`dl_entails`]) but as
    /// *n* independent consistency checks, so it cannot be frozen as one `input.nq`.
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
        [shape] => {
            let mut input_iris: BTreeSet<String> = BTreeSet::new();
            collect_iris(premise, &mut input_iris);
            collect_iris(conclusion, &mut input_iris);
            let minter = Minter::new(&input_iris)?;
            Ok(VendorReduction::Single(negate(shape, &minter)))
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

    // Every component must be entailed (conjunction): decide each independently.
    for shape in &shapes {
        let negation = negate(shape, &minter);
        let edb = build_world_edb(premise, &negation)?;
        let verdict = crate::reason::dl_consistency(edb.as_ref())?;
        if !verdict.gaps.is_empty() {
            let codes: Vec<&str> = verdict.gaps.iter().map(|g| g.code.as_str()).collect();
            return Ok(EntailmentVerdict::Gap(EntailmentGap {
                shape: GapShape::NativeCoverage,
                detail: format!(
                    "native DL coverage gap(s) {codes:?} on the reduced EDB — the engine cannot \
                     honestly decide this entailment"
                ),
            }));
        }
        if verdict.consistent {
            // This component has a counter-model, so the conjunction is not entailed.
            return Ok(EntailmentVerdict::NotEntailed);
        }
        // Inconsistent ⇒ this component is entailed; continue to the next.
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
