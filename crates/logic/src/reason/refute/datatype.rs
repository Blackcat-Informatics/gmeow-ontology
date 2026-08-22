// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Family 5 — the datatype value-space refutation sub-decider.
//!
//! This is the first REAL sub-decider registered in [`super::SUB_DECIDERS`]. It
//! decides, soundly and completely for its fragment, the datatype value-space
//! obligations the native forward chase ([`crate::reason::dl`]) withholds:
//!
//! * **value-space cardinality counting** — a datatype property required (by an
//!   exact/`min` cardinality, or an existential `someValuesFrom`) to carry `N`
//!   distinct values drawn from a value space of size `M < N` is inconsistent
//!   (pigeonhole). The finite value-space cardinality of a named xsd datatype
//!   (`xsd:byte` = 256) is DERIVED from the `math:`-grounded facts authored in
//!   `slices/grounding/math/module.ttl` — see [`finite_named_cardinality`] and the
//!   projection-proof unit test `rust_finite_cardinality_table_projects_the_math_grounding`.
//! * **xsd facet satisfaction** on `owl:withRestrictions` constrained datatypes —
//!   numeric RANGE (`min`/`maxInclusive`/`Exclusive`, IEEE-float-discrete aware) and
//!   string LENGTH (`length`/`minLength`/`maxLength`): whether the constrained
//!   literal set is EMPTY (an empty datatype an individual must inhabit is
//!   inconsistent), and whether a required literal satisfies the facets. The numeric
//!   RANGE emptiness/cardinality decision over the IEEE float/double grids and the
//!   integer strata is DELEGATED to purrdf's exact interval algebra
//!   ([`purrdf::xsd::range`]), gated on [`purrdf::xsd::range::is_exactly_decided`]: purrdf
//!   is the value-space range decider (Task 4b). Facet ranges over
//!   `owl:rational`/`owl:real` endpoints — purrdf's named `Undecided` residue — and
//!   value-space MEMBERSHIP over the full exact-ℚ tower stay the NATIVE residue decision
//!   (purrdf's `XsdValue` does not model those endpoints, so purrdf is not
//!   at-least-as-capable there; dropping the native decision would flip a decided W3C
//!   case to incomplete). The purrdf delegation returns a proof ONLY where purrdf is
//!   exactly-decided and native already DECIDED; it withholds (`Tri::Unknown`) exactly
//!   where the native path withheld, so it never widens coverage. The slice-grounded
//!   [`FINITE_NAMED_CARDINALITY`] table remains the sole cardinality authority for a
//!   NAMED finite datatype (Task 4c).
//! * **`owl:oneOf` datatype enumerations** — distinct-value counting across the
//!   whole rational tower (`xsd:decimal`/`xsd:integer`/`owl:rational` share one
//!   value space, so `"0.5"^^xsd:decimal` and `"1/2"^^owl:rational` are ONE value)
//!   through the exact-ℚ [`purrdf::xsd::rational::Rational`] value-space identity — its
//!   structural reduced `Eq` is the authority for whether two lexical forms denote one
//!   value (Task 4a).
//! * **`owl:datatypeComplementOf`** value-space membership where decidable.
//!
//! Everything the subsolver cannot prove complete returns
//! [`super::RefutationCertificate::OutOfFragment`] with a precise
//! [`super::FragmentBoundary`] obstruction — NEVER a guess. `xsd:pattern` facets
//! are deliberately bounded OUT: sound XSD-pattern value-space reasoning requires
//! the XML-Schema regular-expression dialect (whose anchoring and constructs differ
//! from any host regex engine), so a pattern facet is a structured obstruction, not
//! a silently-wrong decision.

use std::collections::{BTreeMap, BTreeSet};

use purrdf::xsd::range::{self, Cardinality, DataRange, Facet, Satisfiability};
use purrdf::xsd::rational::Rational;
use purrdf::xsd::{XsdDatatype, XsdValue};
use purrdf::{RdfDataset, RdfLiteral, RdfTerm};

use super::{
    BoundKind, CountBound, Decision, FragmentFamily, NothingClash, RefutationCertificate, Witness,
    WitnessEvidence, certify_membership, is_rational_tower, parse_rational, resource_key,
    world_key,
};

// ── IRI constants (local to the subsolver) ──────────────────────────────────────
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const RDFS_SUB_CLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const OWL_ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
const OWL_SOME_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#someValuesFrom";
const OWL_ALL_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#allValuesFrom";
const OWL_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#cardinality";
const OWL_MIN_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#minCardinality";
const OWL_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#qualifiedCardinality";
const OWL_MIN_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#minQualifiedCardinality";
const OWL_MAX_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#maxCardinality";
const OWL_MAX_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#maxQualifiedCardinality";
const OWL_ON_DATATYPE: &str = "http://www.w3.org/2002/07/owl#onDatatype";
const OWL_WITH_RESTRICTIONS: &str = "http://www.w3.org/2002/07/owl#withRestrictions";
const OWL_DATATYPE_COMPLEMENT_OF: &str = "http://www.w3.org/2002/07/owl#datatypeComplementOf";
const OWL_ONE_OF: &str = "http://www.w3.org/2002/07/owl#oneOf";
const OWL_RATIONAL: &str = "http://www.w3.org/2002/07/owl#rational";
const OWL_REAL: &str = "http://www.w3.org/2002/07/owl#real";
const OWL_FUNCTIONAL_PROPERTY: &str = "http://www.w3.org/2002/07/owl#FunctionalProperty";
const OWL_INVERSE_FUNCTIONAL_PROPERTY: &str =
    "http://www.w3.org/2002/07/owl#InverseFunctionalProperty";

const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
const XSD_FLOAT: &str = "http://www.w3.org/2001/XMLSchema#float";
const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";
const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";

const XSD_MIN_INCLUSIVE: &str = "http://www.w3.org/2001/XMLSchema#minInclusive";
const XSD_MAX_INCLUSIVE: &str = "http://www.w3.org/2001/XMLSchema#maxInclusive";
const XSD_MIN_EXCLUSIVE: &str = "http://www.w3.org/2001/XMLSchema#minExclusive";
const XSD_MAX_EXCLUSIVE: &str = "http://www.w3.org/2001/XMLSchema#maxExclusive";
const XSD_LENGTH: &str = "http://www.w3.org/2001/XMLSchema#length";
const XSD_MIN_LENGTH: &str = "http://www.w3.org/2001/XMLSchema#minLength";
const XSD_MAX_LENGTH: &str = "http://www.w3.org/2001/XMLSchema#maxLength";
const XSD_PATTERN: &str = "http://www.w3.org/2001/XMLSchema#pattern";

const RULE_NAME: &str = "refute:datatype-value-space";

/// The predicates the datatype value-space fragment fully consumes and proves
/// complete over — exactly the vocabulary [`Model::scan`] interprets (the
/// value-space shape, cardinality/existential scaffolding, and facet plumbing) plus
/// the RDF-list plumbing that carries `owl:oneOf` enumerations and
/// `owl:withRestrictions` facet lists. A case carrying ANY predicate outside this
/// allowlist — and not a declared `owl:DatatypeProperty` value edge (see the dynamic
/// `Model::datatype_props` check at the call site) — is refused a `Consistent`
/// certificate: a class-construction axiom (`owl:complementOf`/`unionOf`/
/// `disjointWith`/`equivalentClass`/`subClassOf`), an identity axiom
/// (`owl:sameAs`/`differentFrom`/`inverseOf`), or an object-property assertion could
/// carry an inconsistency (or a further constraint) this per-obligation analysis
/// never inspects, so its mere presence is an honest obstruction rather than a
/// silent guess. A proven CLASH is unaffected by this gate — it stays decisive
/// regardless (mirrors [`super::counting::ALLOWED_CARDINALITY_PREDICATES`]).
const ALLOWED_DATATYPE_PREDICATES: &[&str] = &[
    RDF_TYPE,
    RDF_FIRST,
    RDF_REST,
    OWL_ON_PROPERTY,
    OWL_SOME_VALUES_FROM,
    OWL_ALL_VALUES_FROM,
    OWL_CARDINALITY,
    OWL_MIN_CARDINALITY,
    OWL_QUALIFIED_CARDINALITY,
    OWL_MIN_QUALIFIED_CARDINALITY,
    OWL_MAX_CARDINALITY,
    OWL_MAX_QUALIFIED_CARDINALITY,
    RDFS_RANGE,
    OWL_ON_DATATYPE,
    OWL_WITH_RESTRICTIONS,
    OWL_DATATYPE_COMPLEMENT_OF,
    OWL_ONE_OF,
    XSD_MIN_INCLUSIVE,
    XSD_MAX_INCLUSIVE,
    XSD_MIN_EXCLUSIVE,
    XSD_MAX_EXCLUSIVE,
    XSD_LENGTH,
    XSD_MIN_LENGTH,
    XSD_MAX_LENGTH,
    XSD_PATTERN,
];

/// The `math:`-grounded finite value-space cardinalities of named datatypes.
///
/// The AUTHORITATIVE source is `slices/grounding/math/module.ttl` (a
/// `math:FiniteSet` individual per datatype carrying `math:hasCardinality
/// math:cardinalityFinite` + `math:quantityValue` and `rdfs:seeAlso` the xsd
/// datatype). This table is the reasoner-side PROJECTION of those facts; the unit
/// test `rust_finite_cardinality_table_projects_the_math_grounding` parses the math
/// slice and asserts the two agree bidirectionally, so there is exactly ONE
/// authored source of truth and this table is provably its projection (never an
/// independent hand-maintained copy).
const FINITE_NAMED_CARDINALITY: &[(&str, u128)] = &[("http://www.w3.org/2001/XMLSchema#byte", 256)];

/// The finite value-space cardinality of a named datatype, or `None` when the
/// datatype is not a `math:`-grounded finite datatype.
pub(crate) fn finite_named_cardinality(iri: &str) -> Option<u128> {
    FINITE_NAMED_CARDINALITY
        .iter()
        .find(|(dt, _)| *dt == iri)
        .map(|(_, card)| *card)
}

// ── The registered sub-decider entrypoints ──────────────────────────────────────

/// The [`super::SubDecider`] for the datatype value-space family.
pub(crate) fn decide(edb: &RdfDataset) -> Option<RefutationCertificate> {
    let model = Model::scan(edb);
    let obligations = model.obligations();
    if obligations.is_empty() {
        // No datatype value-space shape present — the family does not engage.
        return None;
    }

    let mut clashes: BTreeSet<NothingClash> = BTreeSet::new();
    let mut counted: BTreeSet<String> = BTreeSet::new();
    let mut violated: Option<CountBound> = None;
    let mut obstructions: BTreeSet<String> = BTreeSet::new();

    for ob in &obligations {
        match ob.evaluate(&model) {
            Outcome::Consistent => {}
            Outcome::Clash { clash, bound } => {
                counted.insert(ob.individual.clone());
                if violated.is_none() {
                    violated = bound;
                }
                clashes.insert(clash);
            }
            Outcome::Obstructed(reason) => {
                obstructions.insert(reason);
            }
        }
    }

    // A proven clash is decisive: the ontology IS inconsistent regardless of any
    // obstruction elsewhere, so it is sound to decide `Inconsistent`. Only a
    // Consistent decision requires an obstruction-free evaluation (a complete
    // proof that every datatype obligation is satisfiable).
    if !clashes.is_empty() {
        return Some(certify_membership(
            FragmentFamily::DatatypeValueSpace,
            BTreeSet::new(),
            move || {
                (
                    Decision::Inconsistent,
                    Witness {
                        family: FragmentFamily::DatatypeValueSpace,
                        clashes,
                        evidence: WitnessEvidence {
                            counted_individuals: counted,
                            violated_bound: violated,
                            closed_branch: None,
                        },
                    },
                )
            },
        ));
    }

    // Whole-case completeness: a `Consistent` verdict is licensed only when EVERY
    // predicate present is either datatype value-space scaffolding this decider
    // consumes and proves complete over, or a declared `owl:DatatypeProperty` value
    // edge (the decider's own per-obligation analysis already reasons over its
    // range / value-space membership). Anything else — a class-construction axiom,
    // an identity axiom, an unrelated object-property assertion — could carry an
    // inconsistency (decidable by a sibling family) this per-obligation analysis
    // never inspects, so its mere presence is an honest obstruction. This is the
    // gate that stops a proven-elsewhere `casesplit`/`counting` inconsistency from
    // being masked by a `Consistent` short-circuit (soundness by construction).
    for predicate in &model.predicates {
        if !ALLOWED_DATATYPE_PREDICATES.contains(&predicate.as_str())
            && !model.datatype_props.contains(predicate)
        {
            obstructions.insert(format!(
                "datatype value-space fragment: unhandled predicate <{predicate}> may interact \
                 with the value-space obligation"
            ));
        }
    }
    // A declared `owl:DatatypeProperty` that is ALSO functional / inverse-functional
    // carries an identity-collapse obligation (two distinct literal values forced
    // equal, or a merged-subject pair) this decider does not reason over — mirrors
    // the counting decider's identical property-characteristic gate.
    for object in &model.type_objects {
        if object == OWL_FUNCTIONAL_PROPERTY || object == OWL_INVERSE_FUNCTIONAL_PROPERTY {
            obstructions.insert(format!(
                "datatype value-space fragment: property characteristic <{object}> may interact \
                 with the value-space obligation"
            ));
        }
    }

    Some(certify_membership(
        FragmentFamily::DatatypeValueSpace,
        obstructions,
        || {
            (
                Decision::Consistent,
                Witness {
                    family: FragmentFamily::DatatypeValueSpace,
                    clashes: BTreeSet::new(),
                    evidence: WitnessEvidence::default(),
                },
            )
        },
    ))
}

/// True iff the datatype value-space sub-decider DECIDES `edb` (an in-fragment
/// `Consistent` or `Inconsistent`). The coverage coordinator ([`crate::reason::dl`])
/// consults this to promote the datatype-value-space families out of the honest
/// gap set exactly when — and only when — the subsolver has completely decided
/// them, keeping coverage in agreement with the decider (never wider).
pub(crate) fn decided(edb: &RdfDataset) -> bool {
    matches!(decide(edb), Some(RefutationCertificate::InFragment { .. }))
}

/// The `(world, subject, property)` obligation keys this sub-decider evaluates
/// DEFINITIVELY — every obligation whose evaluation produced a decision (the
/// asserted values satisfy the value space, or a proven value-space clash) rather
/// than an obstruction.
///
/// The per-obligation twin of [`decided`], and the two answer different questions.
/// [`decided`] answers the WHOLE-CASE question the refutation kernel must answer
/// before it may certify an ENTIRE ontology `Consistent`; its predicate allowlist
/// ([`ALLOWED_DATATYPE_PREDICATES`]) therefore makes it false for any ontology that
/// also carries class-construction, identity, or ordinary domain vocabulary — that
/// is, for every real bundle. Construct COVERAGE asks something strictly narrower:
/// did the native path decide what the datatype-facet axioms CONTRIBUTE? That is a
/// per-obligation question, so the coverage coordinator ([`crate::reason::dl`])
/// checks its live facet witnesses against this set.
///
/// A proven CLASH counts as definitively evaluated: [`decide`] returns it as an
/// `InFragment{Inconsistent}` certificate regardless of any obstruction elsewhere,
/// and the coordinator materializes its `owl:Nothing` witness into the closure, so
/// the axiom is decided rather than silently ignored.
pub(crate) fn definitively_evaluated_obligations(
    edb: &RdfDataset,
) -> BTreeSet<(String, String, String)> {
    let model = Model::scan(edb);
    model
        .obligations()
        .iter()
        .filter(|ob| !matches!(ob.evaluate(&model), Outcome::Obstructed(_)))
        .map(|ob| (ob.world.clone(), ob.individual.clone(), ob.property.clone()))
        .collect()
}

/// True iff every `owl:oneOf` node in `edb` is a DATATYPE enumeration — an
/// enumeration all of whose members are LITERALS. The native list reader skips
/// literal members, so a literal `owl:oneOf` is exactly the datatype-value-space
/// enumeration this subsolver owns; an object (individual) enumeration is the
/// native path's and must not be promoted through the subsolver's decision.
pub(crate) fn all_oneof_are_literal_enumerations(edb: &RdfDataset) -> bool {
    let model = Model::scan(edb);
    let mut saw = false;
    for head in model.one_of_heads() {
        saw = true;
        let members = model.list_members(&head);
        if members.is_empty() || members.iter().any(|m| !matches!(m, RdfTerm::Literal(_))) {
            return false;
        }
    }
    saw
}

// ── The exact-value model ───────────────────────────────────────────────────────

/// An exact xsd/owl literal VALUE, canonicalized so that value-equal literals from
/// different lexical spaces compare equal. The whole `xsd:decimal`/`xsd:integer`/
/// `owl:rational` tower shares one value space, carried as
/// [`purrdf::xsd::rational::Rational`] whose reduced `Eq` is the value-space identity
/// authority (Task 4a); IEEE `xsd:float`/`xsd:double` are their own (bit-distinct) value
/// spaces; strings and booleans are theirs.
#[derive(Clone, Debug)]
enum Value {
    Rat(Rational),
    F32(f32),
    F64(f64),
    Str(String),
    Bool(bool),
}

impl Value {
    /// A canonical value key, or `None` for a value (a float) with no cheap exact
    /// canonical key. Two literals denote the same xsd value iff their keys are
    /// equal (`Some` on both).
    fn key(&self) -> Option<String> {
        match self {
            // purrdf's `Rational` is reduced with a positive denominator, so `num/den`
            // is a canonical, injective value-space key: `"0.5"^^xsd:decimal` and
            // `"1/2"^^owl:rational` reduce to the same `1/2` (Task 4a).
            Self::Rat(q) => Some(format!("Q:{}/{}", q.numerator(), q.denominator())),
            Self::Str(s) => Some(format!("S:{s}")),
            Self::Bool(b) => Some(format!("B:{b}")),
            Self::F32(_) | Self::F64(_) => None,
        }
    }
}

/// The three-valued answer to a value-space membership / emptiness question. The
/// subsolver NEVER collapses `Unknown` into a decision — it withholds instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tri {
    Yes,
    No,
    Unknown,
}

impl Tri {
    fn negate(self) -> Self {
        match self {
            Self::Yes => Self::No,
            Self::No => Self::Yes,
            Self::Unknown => Self::Unknown,
        }
    }
}

/// A datatype value-space cardinality.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Card {
    Finite(u128),
    Infinite,
    Unknown,
}

/// A resolved datatype value space.
#[derive(Clone, Debug)]
enum Dt {
    /// A named datatype in the rational tower with `[lo, hi]` integer bounds
    /// (`None` = unbounded on that side), e.g. `xsd:positiveInteger` = `[1, ∞)`.
    IntRange {
        lo: Option<i128>,
        hi: Option<i128>,
    },
    /// A dense named datatype: `xsd:decimal`, `owl:rational`, `owl:real`.
    DenseRational,
    /// A named IEEE datatype.
    Float,
    Double,
    /// The string value space.
    Str,
    /// The boolean value space.
    Bool,
    /// A facet-restricted datatype: a base restricted by numeric-range / length
    /// facets.
    Facet {
        base: Box<Dt>,
        // Boxed: `Facets` carries the purrdf `XsdValue` of every numeric bound (a large
        // value-space enum), so an unboxed `Facets` would make this the dominant `Dt`
        // variant.
        facets: Box<Facets>,
    },
    /// `owl:datatypeComplementOf D` over the (infinite) literal universe.
    Complement(Box<Dt>),
    /// `owl:oneOf` datatype enumeration of exact values.
    Enumeration(Vec<Value>),
}

/// A numeric facet bound.
///
/// Carries the exact native [`Value`] — used by value-space MEMBERSHIP over the full
/// exact-ℚ tower and the dense/rational-endpoint residue, which purrdf's [`XsdValue`]
/// does not model — alongside the purrdf [`XsdValue`] parsed from the SAME literal when
/// the bound lies in a purrdf-modelled value space (`None` for an `owl:rational`/
/// `owl:real` endpoint). The purrdf value drives the delegated range-emptiness/
/// cardinality decision ([`Facets::purrdf_range`]).
#[derive(Clone, Debug)]
struct Bound {
    value: Value,
    xsd: Option<XsdValue>,
}

/// The facet bundle carried by an `owl:withRestrictions` datatype.
#[derive(Clone, Debug, Default)]
struct Facets {
    min_inclusive: Option<Bound>,
    max_inclusive: Option<Bound>,
    min_exclusive: Option<Bound>,
    max_exclusive: Option<Bound>,
    length: Option<usize>,
    min_length: Option<usize>,
    max_length: Option<usize>,
    /// A pattern facet is present — sound XSD-pattern reasoning is outside the
    /// certified fragment, so its mere presence forces an obstruction.
    has_pattern: bool,
}

impl Dt {
    /// Whether `value` lies in this value space.
    fn contains(&self, value: &Value) -> Tri {
        match self {
            Self::IntRange { lo, hi } => match value {
                Value::Rat(q) if q.denominator() == 1 => {
                    let n = q.numerator();
                    let above = lo.is_none_or(|lo| n >= lo);
                    let below = hi.is_none_or(|hi| n <= hi);
                    if above && below { Tri::Yes } else { Tri::No }
                }
                // A non-integer rational, a float, a string, or a boolean is not
                // in an integer value space.
                _ => Tri::No,
            },
            Self::DenseRational => match value {
                Value::Rat(_) => Tri::Yes,
                _ => Tri::No,
            },
            Self::Float => matches!(value, Value::F32(_)).into_tri(),
            Self::Double => matches!(value, Value::F64(_)).into_tri(),
            Self::Str => matches!(value, Value::Str(_)).into_tri(),
            Self::Bool => matches!(value, Value::Bool(_)).into_tri(),
            Self::Facet { base, facets } => match base.contains(value) {
                Tri::No => Tri::No,
                Tri::Unknown => Tri::Unknown,
                Tri::Yes => facets.satisfied(value),
            },
            Self::Complement(inner) => inner.contains(value).negate(),
            Self::Enumeration(values) => {
                let Some(k) = value.key() else {
                    return Tri::Unknown;
                };
                if values
                    .iter()
                    .any(|v| v.key().as_deref() == Some(k.as_str()))
                {
                    Tri::Yes
                } else {
                    Tri::No
                }
            }
        }
    }

    /// Whether this value space is empty.
    fn emptiness(&self) -> Tri {
        match self {
            Self::IntRange { lo, hi } => match (lo, hi) {
                (Some(lo), Some(hi)) if lo > hi => Tri::Yes,
                _ => Tri::No,
            },
            Self::DenseRational | Self::Float | Self::Double | Self::Str | Self::Bool => Tri::No,
            Self::Enumeration(values) => values.is_empty().into_tri(),
            // Complement of a proper (recognized, non-universal) datatype over the
            // infinite literal universe is non-empty.
            Self::Complement(_) => Tri::No,
            Self::Facet { base, facets } => facets.emptiness(base),
        }
    }

    /// The value-space cardinality.
    ///
    /// A NAMED finite datatype's cardinality is NOT computed here from Rust-encoded
    /// bounds — it is looked up from the `math:`-grounded [`finite_named_cardinality`]
    /// table at the call site in [`Obligation::evaluate`], which knows the source
    /// IRI. A bounded [`Dt::IntRange`] reached WITHOUT that IRI context (it can only
    /// be a named datatype) therefore reports `Unknown` rather than minting a second,
    /// Rust-authored cardinality source; an unbounded integer range is `Infinite`.
    fn cardinality(&self) -> Card {
        match self {
            Self::IntRange { lo, hi } => match (lo, hi) {
                (Some(lo), Some(hi)) if hi < lo => {
                    let _ = (lo, hi);
                    Card::Finite(0)
                }
                (Some(_), Some(_)) => Card::Unknown,
                _ => Card::Infinite,
            },
            Self::DenseRational => Card::Infinite,
            // Named IEEE / string spaces are finite-but-enormous / infinite; the
            // subsolver never COUNTS distinct values against them (only emptiness /
            // membership), so an exact cardinality is deliberately Unknown — a
            // counting obligation against them withholds rather than guesses.
            Self::Float | Self::Double => Card::Unknown,
            Self::Str => Card::Infinite,
            Self::Bool => Card::Unknown,
            Self::Enumeration(values) => {
                // Distinct-value count. A float member (no exact key) makes the
                // count unknown (never needed by the certified cases).
                let mut keys: BTreeSet<String> = BTreeSet::new();
                for v in values {
                    match v.key() {
                        Some(k) => {
                            keys.insert(k);
                        }
                        None => return Card::Unknown,
                    }
                }
                Card::Finite(keys.len() as u128)
            }
            Self::Complement(_) => Card::Infinite,
            Self::Facet { base, facets } => facets.cardinality(base),
        }
    }
}

impl Facets {
    fn satisfied(&self, value: &Value) -> Tri {
        // Numeric range facets.
        for (bound, strict, is_min) in [
            (self.min_inclusive.as_ref().map(|b| &b.value), false, true),
            (self.min_exclusive.as_ref().map(|b| &b.value), true, true),
            (self.max_inclusive.as_ref().map(|b| &b.value), false, false),
            (self.max_exclusive.as_ref().map(|b| &b.value), true, false),
        ] {
            let Some(bound) = bound else { continue };
            match numeric_cmp(value, bound) {
                None => return Tri::Unknown,
                Some(ord) => {
                    let ok = match (is_min, strict) {
                        (true, false) => ord.is_ge(),
                        (true, true) => ord.is_gt(),
                        (false, false) => ord.is_le(),
                        (false, true) => ord.is_lt(),
                    };
                    if !ok {
                        return Tri::No;
                    }
                }
            }
        }
        // Length facets (strings).
        if self.length.is_some() || self.min_length.is_some() || self.max_length.is_some() {
            let Value::Str(s) = value else {
                return Tri::No;
            };
            let len = s.chars().count();
            if let Some(exact) = self.length
                && len != exact
            {
                return Tri::No;
            }
            if let Some(min) = self.min_length
                && len < min
            {
                return Tri::No;
            }
            if let Some(max) = self.max_length
                && len > max
            {
                return Tri::No;
            }
        }
        if self.has_pattern {
            return Tri::Unknown;
        }
        Tri::Yes
    }

    fn emptiness(&self, base: &Dt) -> Tri {
        if self.has_pattern {
            return Tri::Unknown;
        }
        // Length-facet emptiness: contradictory bounds make the constrained string
        // space empty regardless of base.
        if self.length.is_some() || self.min_length.is_some() || self.max_length.is_some() {
            let min = self.min_length.or(self.length).unwrap_or(0);
            let max = self.max_length.or(self.length).unwrap_or(usize::MAX);
            let exact_conflict = matches!((self.length, self.min_length), (Some(l), Some(mn)) if l < mn)
                || matches!((self.length, self.max_length), (Some(l), Some(mx)) if l > mx);
            if min > max || exact_conflict {
                return Tri::Yes;
            }
            return Tri::No;
        }
        // Numeric-range facet emptiness, base-dispatched. The IEEE float/double grids and
        // the integer strata are DELEGATED to purrdf's exact interval algebra; a facet
        // range over `owl:rational`/`owl:real` endpoints (purrdf's named `Undecided`
        // residue) stays the native dense decision.
        match base {
            Dt::Float | Dt::Double | Dt::IntRange { .. } => self.purrdf_range_empty(base),
            Dt::DenseRational => self.dense_range_empty(),
            _ => Tri::Unknown,
        }
    }

    fn cardinality(&self, base: &Dt) -> Card {
        if self.emptiness(base) == Tri::Yes {
            return Card::Finite(0);
        }
        // A non-empty facet-restricted INTEGER range has an exact count, DELEGATED to
        // purrdf's interval algebra; other non-empty facet spaces are only known
        // non-empty (Unknown exact count), which suffices for a `required == 1`
        // obligation.
        match base {
            Dt::IntRange { .. } => self.purrdf_range_cardinality(base),
            _ => Card::Unknown,
        }
    }

    /// Build the purrdf [`DataRange`] for this numeric facet set over `base`, or `None`
    /// when the NATIVE path deliberately WITHHELD on this shape — a non-integer bound on
    /// an integer space, a wrong-width / both-sided bound on an IEEE space, or an
    /// `owl:rational`/`owl:real` endpoint purrdf's [`XsdValue`] does not model. Returning
    /// `None` exactly where the native decision withheld is what keeps the delegation
    /// from ever WIDENING coverage; the caller maps it to `Tri::Unknown` / `Card::Unknown`
    /// (the native withhold), never a decision.
    fn purrdf_range(&self, base: &Dt) -> Option<DataRange> {
        match base {
            Dt::IntRange { lo, hi } => {
                // A numeric facet that is not an EXACT integer needs reasoning the native
                // integer path never did (it withheld); preserve that.
                for b in [
                    &self.min_inclusive,
                    &self.max_inclusive,
                    &self.min_exclusive,
                    &self.max_exclusive,
                ] {
                    if let Some(b) = b
                        && int_value(&b.value).is_none()
                    {
                        return None;
                    }
                }
                let mut facets = Vec::new();
                if let Some(lo) = lo {
                    facets.push(Facet::MinInclusive(int_xsd(*lo)?));
                }
                if let Some(hi) = hi {
                    facets.push(Facet::MaxInclusive(int_xsd(*hi)?));
                }
                // The native integer bound resolution took the INCLUSIVE facet over the
                // exclusive one on each side (an ontology carrying both was degenerate);
                // mirror that precedence exactly so the delegation matches the native
                // decision (purrdf would otherwise intersect BOTH).
                if let Some(b) = self.min_inclusive.as_ref() {
                    facets.push(Facet::MinInclusive(int_xsd(int_value(&b.value)?)?));
                } else if let Some(b) = self.min_exclusive.as_ref() {
                    facets.push(Facet::MinExclusive(int_xsd(int_value(&b.value)?)?));
                }
                if let Some(b) = self.max_inclusive.as_ref() {
                    facets.push(Facet::MaxInclusive(int_xsd(int_value(&b.value)?)?));
                } else if let Some(b) = self.max_exclusive.as_ref() {
                    facets.push(Facet::MaxExclusive(int_xsd(int_value(&b.value)?)?));
                }
                Some(DataRange::Restriction {
                    base: XsdDatatype::Integer,
                    facets,
                })
            }
            Dt::Float | Dt::Double => {
                let want = if matches!(base, Dt::Float) {
                    XsdDatatype::Float
                } else {
                    XsdDatatype::Double
                };
                // The native IEEE path withheld when BOTH bounds on one side were present.
                if self.min_inclusive.is_some() && self.min_exclusive.is_some() {
                    return None;
                }
                if self.max_inclusive.is_some() && self.max_exclusive.is_some() {
                    return None;
                }
                let mut facets = Vec::new();
                if let Some(b) = self.min_inclusive.as_ref() {
                    facets.push(Facet::MinInclusive(float_bound(b, want)?));
                }
                if let Some(b) = self.min_exclusive.as_ref() {
                    facets.push(Facet::MinExclusive(float_bound(b, want)?));
                }
                if let Some(b) = self.max_inclusive.as_ref() {
                    facets.push(Facet::MaxInclusive(float_bound(b, want)?));
                }
                if let Some(b) = self.max_exclusive.as_ref() {
                    facets.push(Facet::MaxExclusive(float_bound(b, want)?));
                }
                Some(DataRange::Restriction { base: want, facets })
            }
            _ => None,
        }
    }

    /// Value-space RANGE emptiness DELEGATED to purrdf's exact interval algebra. Returns
    /// purrdf's PROOF only when purrdf is [`range::is_exactly_decided`]; otherwise — and
    /// wherever [`Facets::purrdf_range`] declined (the native withhold shapes) — withholds
    /// with `Tri::Unknown`, so the delegation never widens coverage.
    fn purrdf_range_empty(&self, base: &Dt) -> Tri {
        let Some(range) = self.purrdf_range(base) else {
            return Tri::Unknown;
        };
        if !range::is_exactly_decided(&range) {
            return Tri::Unknown;
        }
        match range::satisfiability(&range) {
            Satisfiability::Empty => Tri::Yes,
            Satisfiability::Inhabited => Tri::No,
            Satisfiability::Undecided => Tri::Unknown,
        }
    }

    /// Value-space RANGE cardinality DELEGATED to purrdf, gated the same way as
    /// [`Facets::purrdf_range_empty`]. `AtLeast`/`Undecided` (a lower bound, not an exact
    /// count) map to `Card::Unknown` — a counting obligation withholds rather than guesses.
    fn purrdf_range_cardinality(&self, base: &Dt) -> Card {
        let Some(range) = self.purrdf_range(base) else {
            return Card::Unknown;
        };
        if !range::is_exactly_decided(&range) {
            return Card::Unknown;
        }
        match range::cardinality(&range) {
            Cardinality::Exactly(n) => Card::Finite(u128::from(n)),
            Cardinality::Unbounded => Card::Infinite,
            Cardinality::AtLeast(_) | Cardinality::Undecided => Card::Unknown,
        }
    }

    fn dense_range_empty(&self) -> Tri {
        let lo = self
            .min_inclusive
            .as_ref()
            .map(|b| (&b.value, false))
            .or_else(|| self.min_exclusive.as_ref().map(|b| (&b.value, true)));
        let hi = self
            .max_inclusive
            .as_ref()
            .map(|b| (&b.value, false))
            .or_else(|| self.max_exclusive.as_ref().map(|b| (&b.value, true)));
        let (Some((lo, lo_excl)), Some((hi, hi_excl))) = (lo, hi) else {
            return Tri::No;
        };
        match numeric_cmp(lo, hi) {
            None => Tri::Unknown,
            Some(std::cmp::Ordering::Greater) => Tri::Yes,
            // Equal endpoints: empty unless BOTH inclusive (a single point).
            Some(std::cmp::Ordering::Equal) => (lo_excl || hi_excl).into_tri(),
            // Distinct endpoints: a dense (rational) space always has a point
            // strictly between, so the range is non-empty for any inclusion mix.
            Some(std::cmp::Ordering::Less) => Tri::No,
        }
    }
}

trait IntoTri {
    fn into_tri(self) -> Tri;
}
impl IntoTri for bool {
    fn into_tri(self) -> Tri {
        if self { Tri::Yes } else { Tri::No }
    }
}

/// Compare two numeric values exactly, or `None` when they are not commensurable
/// (a string / cross-space comparison, or a float endpoint we will not coerce).
fn numeric_cmp(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Rat(x), Value::Rat(y)) => Some(x.cmp(y)),
        (Value::F32(x), Value::F32(y)) => x.partial_cmp(y),
        (Value::F64(x), Value::F64(y)) => x.partial_cmp(y),
        _ => None,
    }
}

/// The integer value of `v`, or `None` when it is not an exact integer.
fn int_value(v: &Value) -> Option<i128> {
    match v {
        Value::Rat(q) if q.denominator() == 1 => Some(q.numerator()),
        _ => None,
    }
}

/// An `i128` as a purrdf `xsd:integer` [`XsdValue`] for a purrdf facet bound. Total for
/// every `i128` (`i128::MIN` included) — the lexical round-trip of a decimal integer is
/// exact, so this introduces no float-style parse fragility.
fn int_xsd(n: i128) -> Option<XsdValue> {
    purrdf::xsd::parse(&n.to_string(), XsdDatatype::Integer).ok()
}

/// The purrdf [`XsdValue`] of a numeric facet bound when it lies in the wanted IEEE value
/// space (`xsd:float` / `xsd:double`), or `None` when the bound is an `owl:rational`
/// endpoint (no purrdf value) or comes from another space — exactly the shapes on which
/// the native IEEE range path withheld rather than coerce a cross-space bound.
fn float_bound(b: &Bound, want: XsdDatatype) -> Option<XsdValue> {
    let x = b.xsd.clone()?;
    (x.datatype() == want).then_some(x)
}

// ── EDB scan ────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct Restr {
    on_property: Option<String>,
    some_values_from: Option<String>,
    all_values_from: Option<String>,
    exact: Option<usize>,
    min: Option<usize>,
    /// An UPPER cardinality bound is present. The subsolver's certified fragment is
    /// value-space *emptiness* and *lower-bound* (pigeonhole) counting; a maximum is
    /// an anti-counting obligation (an OVERFLOW clash on too many distinct asserted
    /// literals) it does not decide, so its presence forces an honest obstruction
    /// rather than a possibly-wrong `Consistent`.
    has_max: bool,
}

struct Model {
    /// `(world, individual) → asserted rdf:type class nodes`.
    types: BTreeMap<(String, String), BTreeSet<String>>,
    datatype_props: BTreeSet<String>,
    restrictions: BTreeMap<String, Restr>,
    /// `property → rdfs:range datatype node(s)` (world-agnostic).
    range: BTreeMap<String, BTreeSet<String>>,
    /// `class → asserted rdfs:subClassOf superclass node(s)` (world-agnostic, like
    /// [`Model::restrictions`]: a class expression's identity is its node, and a
    /// TBox axiom in one world still constrains an ABox individual in another —
    /// soundness-first, mirroring the coverage coordinator's facet scan).
    sub_class_of: BTreeMap<String, BTreeSet<String>>,
    /// datatype-def node → its raw datatype-def edges.
    on_datatype: BTreeMap<String, String>,
    with_restrictions: BTreeMap<String, String>,
    complement_of: BTreeMap<String, String>,
    one_of: BTreeMap<String, String>,
    /// facet-carrier node → (facet predicate, literal).
    facet_carrier: BTreeMap<String, (String, RdfLiteral)>,
    /// list head node → ordered members.
    lists: BTreeMap<String, Vec<RdfTerm>>,
    /// `(world, subject, property) → asserted literal values`.
    values: BTreeMap<(String, String, String), Vec<RdfLiteral>>,
    /// Every predicate IRI present (for the whole-case completeness gate).
    predicates: BTreeSet<String>,
    /// Every `rdf:type` object IRI present.
    type_objects: BTreeSet<String>,
}

impl Model {
    fn scan(edb: &RdfDataset) -> Self {
        let mut m = Model {
            types: BTreeMap::new(),
            datatype_props: BTreeSet::new(),
            restrictions: BTreeMap::new(),
            range: BTreeMap::new(),
            sub_class_of: BTreeMap::new(),
            on_datatype: BTreeMap::new(),
            with_restrictions: BTreeMap::new(),
            complement_of: BTreeMap::new(),
            one_of: BTreeMap::new(),
            facet_carrier: BTreeMap::new(),
            lists: BTreeMap::new(),
            values: BTreeMap::new(),
            predicates: BTreeSet::new(),
            type_objects: BTreeSet::new(),
        };
        // raw first/rest edges for the literal-aware list walk.
        let mut first: BTreeMap<String, RdfTerm> = BTreeMap::new();
        let mut rest: BTreeMap<String, String> = BTreeMap::new();

        for quad in edb.owned_quads() {
            let world = world_key(&quad.graph_name);
            // The one lowering point of this scan: a canonical `logic:` class-expression
            // term becomes the `owl:` spelling every arm below matches
            // ([`crate::reason::calculus_term`], the shared table). It
            // must happen BEFORE `m.predicates` / `m.type_objects` are recorded, because
            // those two sets are the whole-case completeness gate: an unlowered
            // `logic:onDatatype` would read as an unknown construct and refuse the case.
            let predicate = crate::reason::calculus_term(&quad.predicate);
            let subject = match resource_key(&quad.subject) {
                Some(s) => s,
                None => continue,
            };
            m.predicates.insert(predicate.to_owned());
            match predicate {
                RDF_TYPE => {
                    if let RdfTerm::Iri(o) = &quad.object
                        && o == OWL_DATATYPE_PROPERTY
                    {
                        m.datatype_props.insert(subject.clone());
                    }
                    if let Some(class) = resource_key(&quad.object) {
                        let class = crate::reason::calculus_term(&class).to_owned();
                        m.type_objects.insert(class.clone());
                        m.types
                            .entry((world.clone(), subject.clone()))
                            .or_default()
                            .insert(class);
                    }
                }
                OWL_ON_PROPERTY => {
                    if let Some(v) = resource_key(&quad.object) {
                        m.restrictions
                            .entry(subject.clone())
                            .or_default()
                            .on_property = Some(v);
                    }
                }
                OWL_SOME_VALUES_FROM => {
                    if let Some(v) = resource_key(&quad.object) {
                        m.restrictions
                            .entry(subject.clone())
                            .or_default()
                            .some_values_from = Some(v);
                    }
                }
                OWL_ALL_VALUES_FROM => {
                    if let Some(v) = resource_key(&quad.object) {
                        m.restrictions
                            .entry(subject.clone())
                            .or_default()
                            .all_values_from = Some(v);
                    }
                }
                OWL_CARDINALITY | OWL_QUALIFIED_CARDINALITY => {
                    if let RdfTerm::Literal(l) = &quad.object
                        && let Some(n) = parse_usize(l)
                    {
                        m.restrictions.entry(subject.clone()).or_default().exact = Some(n);
                    }
                }
                OWL_MIN_CARDINALITY | OWL_MIN_QUALIFIED_CARDINALITY => {
                    if let RdfTerm::Literal(l) = &quad.object
                        && let Some(n) = parse_usize(l)
                    {
                        m.restrictions.entry(subject.clone()).or_default().min = Some(n);
                    }
                }
                OWL_MAX_CARDINALITY | OWL_MAX_QUALIFIED_CARDINALITY => {
                    m.restrictions.entry(subject.clone()).or_default().has_max = true;
                }
                RDFS_RANGE => {
                    if let Some(v) = resource_key(&quad.object) {
                        m.range.entry(subject.clone()).or_default().insert(v);
                    }
                }
                RDFS_SUB_CLASS_OF => {
                    if let Some(v) = resource_key(&quad.object) {
                        m.sub_class_of.entry(subject.clone()).or_default().insert(v);
                    }
                }
                OWL_ON_DATATYPE => {
                    if let Some(v) = resource_key(&quad.object) {
                        m.on_datatype.insert(subject.clone(), v);
                    }
                }
                OWL_WITH_RESTRICTIONS => {
                    if let Some(v) = resource_key(&quad.object) {
                        m.with_restrictions.insert(subject.clone(), v);
                    }
                }
                OWL_DATATYPE_COMPLEMENT_OF => {
                    if let Some(v) = resource_key(&quad.object) {
                        m.complement_of.insert(subject.clone(), v);
                    }
                }
                OWL_ONE_OF => {
                    if let Some(v) = resource_key(&quad.object) {
                        m.one_of.insert(subject.clone(), v);
                    }
                }
                RDF_FIRST => {
                    first.insert(subject.clone(), quad.object.clone());
                }
                RDF_REST => {
                    if let Some(v) = resource_key(&quad.object) {
                        rest.insert(subject.clone(), v);
                    }
                }
                XSD_MIN_INCLUSIVE | XSD_MAX_INCLUSIVE | XSD_MIN_EXCLUSIVE | XSD_MAX_EXCLUSIVE
                | XSD_LENGTH | XSD_MIN_LENGTH | XSD_MAX_LENGTH | XSD_PATTERN => {
                    if let RdfTerm::Literal(l) = &quad.object {
                        m.facet_carrier
                            .insert(subject.clone(), (predicate.to_owned(), l.clone()));
                    }
                }
                _ => {}
            }
            if let RdfTerm::Literal(l) = &quad.object {
                m.values
                    .entry((world.clone(), subject.clone(), quad.predicate.clone()))
                    .or_default()
                    .push(l.clone());
            }
        }

        // Walk every list head to `rdf:nil`, capturing literal AND resource members.
        let heads: BTreeSet<String> = first.keys().cloned().collect();
        for head in heads {
            let mut node = head.clone();
            let mut seen: BTreeSet<String> = BTreeSet::new();
            let mut members: Vec<RdfTerm> = Vec::new();
            while seen.insert(node.clone()) {
                let Some(f) = first.get(&node) else { break };
                members.push(f.clone());
                match rest.get(&node) {
                    Some(next) if next != RDF_NIL => node = next.clone(),
                    _ => break,
                }
            }
            m.lists.insert(head, members);
        }
        m
    }

    fn one_of_heads(&self) -> Vec<String> {
        self.one_of.values().cloned().collect()
    }

    fn list_members(&self, head: &str) -> Vec<RdfTerm> {
        self.lists.get(head).cloned().unwrap_or_default()
    }

    /// Resolve a datatype node/IRI into a value space, or `Err(reason)` when it is
    /// outside the certified fragment (an unknown named datatype, an unrecognized
    /// datatype construct, or an unparsable member).
    fn resolve(&self, node: &str) -> Result<Dt, gmeow_errors::Diag> {
        if let Some(dt) = named_datatype(node) {
            return Ok(dt);
        }
        if let Some(inner) = self.complement_of.get(node) {
            return Ok(Dt::Complement(Box::new(self.resolve(inner)?)));
        }
        if let (Some(base), Some(list_head)) =
            (self.on_datatype.get(node), self.with_restrictions.get(node))
        {
            let base = self.resolve(base)?;
            let facets = self.parse_facets(list_head)?;
            return Ok(Dt::Facet {
                base: Box::new(base),
                facets: Box::new(facets),
            });
        }
        if let Some(list_head) = self.one_of.get(node) {
            let mut values = Vec::new();
            for member in self.list_members(list_head) {
                match &member {
                    RdfTerm::Literal(l) => values.push(parse_value(l).ok_or_else(|| {
                        reason_err(format!("unparsable enumeration member in <{node}>"))
                    })?),
                    _ => {
                        return Err(reason_err(format!(
                            "non-literal enumeration member in <{node}>"
                        )));
                    }
                }
            }
            return Ok(Dt::Enumeration(values));
        }
        Err(reason_err(format!("unrecognized datatype <{node}>")))
    }

    fn parse_facets(&self, list_head: &str) -> Result<Facets, gmeow_errors::Diag> {
        let mut facets = Facets::default();
        for member in self.list_members(list_head) {
            let Some(node) = resource_key(&member) else {
                return Err(reason_err(
                    "non-resource facet in owl:withRestrictions".to_owned(),
                ));
            };
            let Some((prop, lit)) = self.facet_carrier.get(&node) else {
                return Err(reason_err(format!("facet node <{node}> carries no facet")));
            };
            match prop.as_str() {
                XSD_PATTERN => facets.has_pattern = true,
                XSD_MIN_INCLUSIVE => {
                    facets.min_inclusive = Some(parse_num_facet(lit)?);
                }
                XSD_MAX_INCLUSIVE => {
                    facets.max_inclusive = Some(parse_num_facet(lit)?);
                }
                XSD_MIN_EXCLUSIVE => {
                    facets.min_exclusive = Some(parse_num_facet(lit)?);
                }
                XSD_MAX_EXCLUSIVE => {
                    facets.max_exclusive = Some(parse_num_facet(lit)?);
                }
                XSD_LENGTH => facets.length = Some(parse_len_facet(lit)?),
                XSD_MIN_LENGTH => facets.min_length = Some(parse_len_facet(lit)?),
                XSD_MAX_LENGTH => facets.max_length = Some(parse_len_facet(lit)?),
                other => return Err(reason_err(format!("unrecognized facet <{other}>"))),
            }
        }
        Ok(facets)
    }

    /// True iff `node` names an `owl:datatypeComplementOf` / `owl:withRestrictions`
    /// / `owl:oneOf` CONSTRAINED datatype — the shapes this subsolver owns as a
    /// range value space. A plain named datatype range is not one (its membership
    /// is trivial and outside the engage set).
    fn is_constrained_datatype(&self, node: &str) -> bool {
        self.complement_of.contains_key(node)
            || self.with_restrictions.contains_key(node)
            || self.one_of.contains_key(node)
    }

    /// The datatype-property RESTRICTION nodes `seed` inherits: the nodes reachable
    /// from it under asserted `rdfs:subClassOf` (reflexively) that carry an
    /// `owl:onProperty` naming a declared `owl:DatatypeProperty`. Cycle-safe — the
    /// visited set is the frontier guard, so a `C ⊑ D ⊑ C` cycle terminates.
    ///
    /// Only the restriction nodes are returned, not the whole superclass closure: the
    /// result is cached per class across every individual of that class, and the
    /// filtered set is orders of magnitude smaller than the closure on a real
    /// taxonomy.
    fn inherited_datatype_restrictions(&self, seed: &str) -> BTreeSet<String> {
        let mut found: BTreeSet<String> = BTreeSet::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut frontier: Vec<String> = vec![seed.to_owned()];
        while let Some(class) = frontier.pop() {
            if !seen.insert(class.clone()) {
                continue;
            }
            if let Some(r) = self.restrictions.get(&class)
                && let Some(p) = &r.on_property
                && self.datatype_props.contains(p)
            {
                found.insert(class.clone());
            }
            if let Some(supers) = self.sub_class_of.get(&class) {
                frontier.extend(supers.iter().cloned());
            }
        }
        found
    }

    /// The datatype value-space obligations present in the EDB — one per
    /// `(world, subject, datatype-property)` that carries a value-space constraint
    /// (a type-restriction, or an `rdfs:range` to a constrained datatype) plus any
    /// cardinality / existential requirement and asserted literal values.
    fn obligations(&self) -> Vec<Obligation> {
        // Candidate `(world, subject, prop)` keys → the restriction nodes that
        // constrain that property on that subject (possibly empty for a pure
        // range-membership obligation).
        let mut keys: BTreeMap<(String, String, String), BTreeSet<String>> = BTreeMap::new();

        // Source 1 — an individual typed to a datatype-property restriction, either
        // DIRECTLY or through the asserted `rdfs:subClassOf` chain of one of its
        // types. The chain step is the plain RDFS/DL semantics — `x ∈ C` and
        // `C ⊑ R` give `x ∈ R` — and it is the shape production ontologies actually
        // author: a named class carries its value restriction as an anonymous
        // `rdfs:subClassOf` filler, never as a direct `rdf:type` on the individual.
        // Without the step this decider engaged on no production obligation at all
        // and the value-space analysis below was unreachable.
        // The walk is memoized per class: a taxonomy has far fewer classes than
        // individuals, and every individual of a class inherits the same restrictions.
        let mut inherited: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for ((world, individual), classes) in &self.types {
            for class in classes {
                let restrictions = inherited
                    .entry(class.clone())
                    .or_insert_with(|| self.inherited_datatype_restrictions(class));
                for node in restrictions.iter() {
                    let Some(p) = self
                        .restrictions
                        .get(node)
                        .and_then(|r| r.on_property.as_ref())
                    else {
                        continue;
                    };
                    keys.entry((world.clone(), individual.clone(), p.clone()))
                        .or_default()
                        .insert(node.clone());
                }
            }
        }

        // Source 2 — a subject bearing asserted literal values of a datatype
        // property whose `rdfs:range` names a CONSTRAINED datatype (e.g.
        // `owl:datatypeComplementOf`). No `rdf:type` is required.
        for (world, subject, prop) in self.values.keys() {
            if !self.datatype_props.contains(prop) {
                continue;
            }
            let constrained_range = self
                .range
                .get(prop)
                .is_some_and(|rs| rs.iter().any(|dt| self.is_constrained_datatype(dt)));
            if constrained_range {
                keys.entry((world.clone(), subject.clone(), prop.clone()))
                    .or_default();
            }
        }

        keys.into_iter()
            .map(|((world, subject, property), nodes)| {
                let restrs = nodes
                    .iter()
                    .filter_map(|n| self.restrictions.get(n))
                    .map(|r| RestrView {
                        some_values_from: r.some_values_from.clone(),
                        all_values_from: r.all_values_from.clone(),
                        exact: r.exact,
                        min: r.min,
                        has_max: r.has_max,
                    })
                    .collect();
                Obligation {
                    world,
                    individual: subject,
                    property,
                    restriction_nodes: nodes.into_iter().collect(),
                    restrs,
                }
            })
            .collect()
    }
}

#[derive(Clone)]
struct RestrView {
    some_values_from: Option<String>,
    all_values_from: Option<String>,
    exact: Option<usize>,
    min: Option<usize>,
    has_max: bool,
}

struct Obligation {
    world: String,
    individual: String,
    property: String,
    restriction_nodes: Vec<String>,
    restrs: Vec<RestrView>,
}

enum Outcome {
    Consistent,
    Clash {
        clash: NothingClash,
        bound: Option<CountBound>,
    },
    Obstructed(String),
}

impl Obligation {
    fn evaluate(&self, model: &Model) -> Outcome {
        // An UPPER cardinality bound (`max`/`maxQualified`) is an overflow obligation
        // outside the certified fragment (it clashes on too MANY distinct asserted
        // literals — anti-counting the subsolver does not decide). Withhold rather
        // than risk certifying a max-overflow as consistent.
        if self.restrs.iter().any(|r| r.has_max) {
            return Outcome::Obstructed(format!(
                "an upper cardinality bound on <{}> is outside the datatype value-space \
                 certified fragment",
                self.property
            ));
        }

        // The value-space constraints on this (individual, property): the
        // property's rdfs:range, plus any all/someValuesFrom filler on the
        // restrictions this individual is typed to.
        let mut spaces: Vec<String> = Vec::new();
        if let Some(rs) = model.range.get(&self.property) {
            spaces.extend(rs.iter().cloned());
        }
        let mut required: usize = 0;
        for r in &self.restrs {
            if let Some(d) = &r.all_values_from {
                spaces.push(d.clone());
            }
            if let Some(d) = &r.some_values_from {
                spaces.push(d.clone());
                required = required.max(1);
            }
            if let Some(n) = r.exact {
                required = required.max(n);
            }
            if let Some(n) = r.min {
                required = required.max(n);
            }
        }
        spaces.sort();
        spaces.dedup();

        // An EXACT bound is also an upper bound: with asserted literal values present
        // the exact count could be OVERFLOWED (more distinct asserted values than the
        // bound). The subsolver decides the lower/pigeonhole side (space too small) but
        // not the overflow side, so an exact bound WITH asserted values is withheld.
        let value_key = (
            self.world.clone(),
            self.individual.clone(),
            self.property.clone(),
        );
        let has_asserted = model.values.get(&value_key).is_some_and(|v| !v.is_empty());
        if has_asserted && self.restrs.iter().any(|r| r.exact.is_some()) {
            return Outcome::Obstructed(format!(
                "an exact cardinality bound with asserted values on <{}> may overflow — \
                 outside the certified fragment",
                self.property
            ));
        }

        // Resolve every constraining value space. Zero constraints ⇒ the universe
        // (rdfs:Literal, infinite); the effective space is their INTERSECTION.
        //
        // `named_card` is the `math:`-grounded finite cardinality of the sole
        // NAMED datatype constraint, when it has one (e.g. `xsd:byte` = 256). It is
        // the ONLY authority for a named datatype's value-space size — never the
        // Rust-encoded integer bounds.
        let named_card = if spaces.len() == 1 {
            finite_named_cardinality(&spaces[0])
        } else {
            None
        };
        let mut resolved: Vec<Dt> = Vec::with_capacity(spaces.len());
        for node in &spaces {
            match model.resolve(node) {
                Ok(dt) => resolved.push(dt),
                Err(reason) => return Outcome::Obstructed(reason.message().to_string()),
            }
        }

        // MEMBERSHIP: every asserted literal value must lie in EVERY constraining
        // value space. Membership in an intersection is the pointwise conjunction of
        // membership in each conjunct — exact and complete, so several constraints
        // are decided here rather than refused. (A production class routinely carries
        // both an `rdfs:range` and a class-local `∀p.D` on the same property; refusing
        // every such pair left the real facet check unreachable.) `No` on any conjunct
        // is decisive; `Unknown` on any (with no `No`) withholds.
        if !resolved.is_empty()
            && let Some(lits) = model.values.get(&value_key)
        {
            for lit in lits {
                let Some(v) = parse_value(lit) else {
                    return Outcome::Obstructed(format!(
                        "unparsable literal on <{}>",
                        self.property
                    ));
                };
                let mut unknown = false;
                for space in &resolved {
                    match space.contains(&v) {
                        Tri::Yes => {}
                        Tri::No => return self.clash(model, None),
                        Tri::Unknown => unknown = true,
                    }
                }
                if unknown {
                    return Outcome::Obstructed(format!(
                        "literal membership in the value space of <{}> is undecidable",
                        self.property
                    ));
                }
            }
        }

        // Counting: the value space must hold `required` distinct values. The
        // EMPTINESS and CARDINALITY of an intersection are NOT pointwise — two
        // individually non-empty spaces can intersect to nothing — so a counting
        // obligation under more than one constraint stays an honest obstruction.
        if required >= 1 {
            if resolved.len() > 1 {
                return Outcome::Obstructed(format!(
                    "intersection of {} distinct datatype constraints on <{}> is outside the \
                     certified fragment for a counting obligation",
                    resolved.len(),
                    self.property
                ));
            }
            let Some(space) = resolved.first() else {
                // No value-space constraint ⇒ the infinite literal universe holds
                // any finite number of distinct values.
                return Outcome::Consistent;
            };
            match space.emptiness() {
                Tri::Yes => {
                    let bound = CountBound {
                        kind: BoundKind::Min,
                        value: required,
                        on_property: self.property.clone(),
                    };
                    return self.clash(model, Some(bound));
                }
                Tri::Unknown => {
                    return Outcome::Obstructed(format!(
                        "emptiness of the value space of <{}> is undecidable",
                        self.property
                    ));
                }
                Tri::No => {}
            }
            if required >= 2 {
                let card = named_card.map_or_else(|| space.cardinality(), Card::Finite);
                match card {
                    Card::Infinite => {}
                    Card::Finite(k) if k >= required as u128 => {}
                    Card::Finite(k) => {
                        let bound = CountBound {
                            kind: if self.restrs.iter().any(|r| r.exact.is_some()) {
                                BoundKind::Exact
                            } else {
                                BoundKind::Min
                            },
                            value: required,
                            on_property: self.property.clone(),
                        };
                        debug_assert!(k < required as u128);
                        return self.clash(model, Some(bound));
                    }
                    Card::Unknown => {
                        return Outcome::Obstructed(format!(
                            "exact value-space cardinality of <{}> is undecidable for a \
                             count of {required}",
                            self.property
                        ));
                    }
                }
            }
        }
        Outcome::Consistent
    }

    fn clash(&self, model: &Model, bound: Option<CountBound>) -> Outcome {
        let mut premises: Vec<(String, String, String)> = Vec::new();
        for node in &self.restriction_nodes {
            premises.push((self.individual.clone(), RDF_TYPE.to_owned(), node.clone()));
            premises.push((
                node.clone(),
                OWL_ON_PROPERTY.to_owned(),
                self.property.clone(),
            ));
        }
        if let Some(rs) = model.range.get(&self.property) {
            for dt in rs {
                premises.push((self.property.clone(), RDFS_RANGE.to_owned(), dt.clone()));
            }
        }
        premises.sort();
        premises.dedup();
        Outcome::Clash {
            clash: NothingClash {
                individual: self.individual.clone(),
                world: self.world.clone(),
                rule_name: RULE_NAME.to_owned(),
                premises,
            },
            bound,
        }
    }
}

/// Mint a `logic.reason` diagnostic for a value-space/facet resolution failure —
/// the same [`crate::error::Reason`] kind + idiom every other `crate::reason::*`
/// module uses for a premise/term it could not resolve. The preserved `detail`
/// text is the exact obstruction reason a caller folds into an
/// [`Outcome::Obstructed`] message, so converting the error TYPE here changes
/// nothing about the withhold/obstruction DECISION.
fn reason_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason { detail })
}

// ── Literal parsing ─────────────────────────────────────────────────────────────

/// Parse a non-negative-integer cardinality literal.
fn parse_usize(lit: &RdfLiteral) -> Option<usize> {
    lit.lexical_form.trim().parse::<usize>().ok()
}

/// Parse an xsd length-facet literal (a non-negative integer).
fn parse_len_facet(lit: &RdfLiteral) -> Result<usize, gmeow_errors::Diag> {
    lit.lexical_form
        .trim()
        .parse::<usize>()
        .map_err(|_| reason_err(format!("non-integer length facet {:?}", lit.lexical_form)))
}

/// Parse a numeric facet literal into a [`Bound`]: the exact native [`Value`]
/// (float/double stay IEEE) plus the purrdf [`XsdValue`] parsed from the SAME literal
/// when the bound lies in a purrdf-modelled value space (`None` for an
/// `owl:rational`/`owl:real` endpoint), which drives the delegated range decision.
fn parse_num_facet(lit: &RdfLiteral) -> Result<Bound, gmeow_errors::Diag> {
    let value = parse_value(lit)
        .filter(|v| matches!(v, Value::Rat(_) | Value::F32(_) | Value::F64(_)))
        .ok_or_else(|| reason_err(format!("non-numeric facet literal {:?}", lit.lexical_form)))?;
    let xsd = lit.datatype.as_deref().and_then(|dt| {
        purrdf::xsd::parse_by_iri(lit.lexical_form.trim(), dt)
            .ok()
            .flatten()
    });
    Ok(Bound { value, xsd })
}

/// Parse a literal into its exact xsd/owl value, or `None` when it is not a
/// datatype the certified fragment models exactly (so its obligation withholds).
fn parse_value(lit: &RdfLiteral) -> Option<Value> {
    if lit.language.is_some() {
        return Some(Value::Str(lit.lexical_form.clone()));
    }
    let lexical = lit.lexical_form.trim();
    match lit.datatype.as_deref() {
        None | Some(XSD_STRING) | Some(RDF_LANG_STRING) => {
            Some(Value::Str(lit.lexical_form.clone()))
        }
        Some(XSD_BOOLEAN) => match lit.lexical_form.trim() {
            "true" | "1" => Some(Value::Bool(true)),
            "false" | "0" => Some(Value::Bool(false)),
            _ => None,
        },
        Some(XSD_FLOAT) => lexical.parse::<f32>().ok().map(Value::F32),
        Some(XSD_DOUBLE) => lexical.parse::<f64>().ok().map(Value::F64),
        Some(OWL_RATIONAL) | Some(OWL_REAL) => parse_rational_value(lexical),
        Some(dt) if is_rational_tower(dt) => parse_rational_value(lexical),
        Some(_) => None,
    }
}

/// Parse an `xsd:decimal`/`xsd:integer`/`owl:rational`/`owl:real` lexical form into the
/// exact value-space rational whose reduced identity is DECIDED by
/// [`purrdf::xsd::rational::Rational`] — its structural reduced `Eq` is what makes
/// `"0.5"^^xsd:decimal` and `"1/2"^^owl:rational` ONE value (Task 4a). purrdf owns the
/// identity: a ratio lexical goes through [`Rational::parse`], a decimal/integer lexical
/// through purrdf's own `xsd:decimal` value space (`parse_by_iri` + [`Rational::from_xsd`]).
/// The exact-`i128` lexical fallback covers a decimal PAST purrdf's `XsdValue` scale-≤18
/// domain so no value the native path decided is lost — a completeness regression this
/// task forbids; the reduced components are handed back through [`Rational::new`], so the
/// stored identity is still purrdf's.
fn parse_rational_value(lexical: &str) -> Option<Value> {
    let lexical = lexical.trim();
    let rat = if lexical.contains('/') {
        Rational::parse(lexical).ok()
    } else {
        purrdf::xsd::parse_by_iri(lexical, XSD_DECIMAL)
            .ok()
            .flatten()
            .and_then(|v| Rational::from_xsd(&v))
    }
    .or_else(|| {
        let g = parse_rational(lexical)?;
        Rational::new(g.numerator(), g.denominator()).ok()
    })?;
    Some(Value::Rat(rat))
}

/// Resolve a NAMED datatype IRI into a value space.
fn named_datatype(iri: &str) -> Option<Dt> {
    if iri == OWL_RATIONAL || iri == OWL_REAL {
        return Some(Dt::DenseRational);
    }
    if iri == RDF_LANG_STRING {
        return Some(Dt::Str);
    }
    let local = iri.strip_prefix(XSD)?;
    let dt = match local {
        "decimal" | "integer" => Dt::IntRange { lo: None, hi: None }, /* overwritten below for integer */
        _ => match local {
            "string" | "normalizedString" | "token" | "Name" | "NCName" | "language" => Dt::Str,
            "boolean" => Dt::Bool,
            "float" => Dt::Float,
            "double" => Dt::Double,
            "long" => Dt::IntRange {
                lo: Some(i64::MIN as i128),
                hi: Some(i64::MAX as i128),
            },
            "int" => Dt::IntRange {
                lo: Some(i32::MIN as i128),
                hi: Some(i32::MAX as i128),
            },
            "short" => Dt::IntRange {
                lo: Some(i16::MIN as i128),
                hi: Some(i16::MAX as i128),
            },
            "byte" => Dt::IntRange {
                lo: Some(-128),
                hi: Some(127),
            },
            "nonNegativeInteger" => Dt::IntRange {
                lo: Some(0),
                hi: None,
            },
            "positiveInteger" => Dt::IntRange {
                lo: Some(1),
                hi: None,
            },
            "nonPositiveInteger" => Dt::IntRange {
                lo: None,
                hi: Some(0),
            },
            "negativeInteger" => Dt::IntRange {
                lo: None,
                hi: Some(-1),
            },
            "unsignedLong" => Dt::IntRange {
                lo: Some(0),
                hi: Some(u64::MAX as i128),
            },
            "unsignedInt" => Dt::IntRange {
                lo: Some(0),
                hi: Some(u32::MAX as i128),
            },
            "unsignedShort" => Dt::IntRange {
                lo: Some(0),
                hi: Some(u16::MAX as i128),
            },
            "unsignedByte" => Dt::IntRange {
                lo: Some(0),
                hi: Some(255),
            },
            _ => return None,
        },
    };
    // `xsd:decimal` is dense; `xsd:integer` is the unbounded integer range.
    match local {
        "decimal" => Some(Dt::DenseRational),
        "integer" => Some(Dt::IntRange { lo: None, hi: None }),
        _ => Some(dt),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rat(n: i128, d: i128) -> Value {
        Value::Rat(Rational::new(n, d).unwrap())
    }

    fn lit(lexical: &str, datatype: &str) -> RdfLiteral {
        RdfLiteral {
            lexical_form: lexical.to_owned(),
            datatype: Some(datatype.to_owned()),
            language: None,
            direction: None,
        }
    }

    /// A numeric facet [`Bound`] parsed from a lexical + datatype exactly as the EDB scan
    /// builds it — the native [`Value`] AND the purrdf [`XsdValue`] the delegation reads.
    fn fbound(lexical: &str, datatype: &str) -> Bound {
        parse_num_facet(&lit(lexical, datatype)).expect("numeric facet bound")
    }

    #[test]
    fn decimal_and_rational_share_one_value() {
        // "0.5"^^xsd:decimal and "1/2"^^owl:rational denote ONE value.
        let a = parse_value(&lit("0.5", "http://www.w3.org/2001/XMLSchema#decimal")).unwrap();
        let b = parse_value(&lit("1/2", OWL_RATIONAL)).unwrap();
        assert_eq!(a.key(), b.key());
        // 0.3333333333333333 (decimal) and 1/3 (rational) are DISTINCT.
        let c = parse_value(&lit(
            "0.3333333333333333",
            "http://www.w3.org/2001/XMLSchema#decimal",
        ))
        .unwrap();
        let d = parse_value(&lit("1/3", OWL_RATIONAL)).unwrap();
        assert_ne!(c.key(), d.key());
    }

    #[test]
    fn whitespace_padded_numeric_facet_keeps_the_purrdf_view_populated() {
        // A numeric facet's native value and its delegated purrdf xsd view must be
        // parsed from the SAME trimmed lexical. `parse_value` trims, so a padded
        // lexical still yields the native value; the purrdf view is populated ONLY
        // because `parse_num_facet` trims before `parse_by_iri`. Without that trim
        // the padded xsd parse fails and `Bound.xsd` is None — this asserts it is not.
        const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
        let padded = fbound("  5  ", XSD_INTEGER);
        let clean = fbound("5", XSD_INTEGER);
        // Native value agrees between padded and clean (parse_value trims both).
        assert_eq!(padded.value.key(), clean.value.key());
        // Load-bearing: the purrdf xsd view survives the padding via the trimmed lexical.
        assert!(
            padded.xsd.is_some(),
            "padded numeric facet must still populate the purrdf xsd view via the trimmed lexical"
        );
        assert!(
            clean.xsd.is_some(),
            "clean numeric facet populates the purrdf xsd view"
        );
    }

    #[test]
    fn positive_integer_complement_membership() {
        let pos = named_datatype("http://www.w3.org/2001/XMLSchema#positiveInteger").unwrap();
        let complement = Dt::Complement(Box::new(pos));
        assert_eq!(complement.contains(&rat(-1, 1)), Tri::Yes);
        assert_eq!(
            complement.contains(&Value::Str("A string".to_owned())),
            Tri::Yes
        );
        assert_eq!(complement.contains(&rat(5, 1)), Tri::No);
    }

    #[test]
    fn float_discrete_range_is_empty() {
        // (0.0, MIN_POSITIVE_SUBNORMAL) exclusive over the xsd:float grid is empty.
        let facets = Facets {
            min_exclusive: Some(fbound("0.0", XSD_FLOAT)),
            max_exclusive: Some(fbound("1.401298464324817e-45", XSD_FLOAT)),
            ..Facets::default()
        };
        let dt = Dt::Facet {
            base: Box::new(Dt::Float),
            facets: Box::new(facets),
        };
        assert_eq!(dt.emptiness(), Tri::Yes);
        // A wider range is non-empty.
        let wide = Dt::Facet {
            base: Box::new(Dt::Float),
            facets: Box::new(Facets {
                min_exclusive: Some(fbound("0.0", XSD_FLOAT)),
                max_exclusive: Some(fbound("1.0", XSD_FLOAT)),
                ..Facets::default()
            }),
        };
        assert_eq!(wide.emptiness(), Tri::No);
    }

    #[test]
    fn byte_cardinality_from_math_table() {
        assert_eq!(
            finite_named_cardinality("http://www.w3.org/2001/XMLSchema#byte"),
            Some(256)
        );
    }

    #[test]
    fn length_facet_emptiness() {
        let empty = Dt::Facet {
            base: Box::new(Dt::Str),
            facets: Box::new(Facets {
                min_length: Some(5),
                max_length: Some(3),
                ..Facets::default()
            }),
        };
        assert_eq!(empty.emptiness(), Tri::Yes);
        let ok = Dt::Facet {
            base: Box::new(Dt::Str),
            facets: Box::new(Facets {
                min_length: Some(2),
                max_length: Some(5),
                ..Facets::default()
            }),
        };
        assert_eq!(ok.emptiness(), Tri::No);
    }

    #[test]
    fn pattern_facet_withholds() {
        let dt = Dt::Facet {
            base: Box::new(Dt::Str),
            facets: Box::new(Facets {
                has_pattern: true,
                ..Facets::default()
            }),
        };
        assert_eq!(dt.emptiness(), Tri::Unknown);
    }

    /// PROJECTION PROOF: the reasoner's [`FINITE_NAMED_CARDINALITY`] table is
    /// exactly the projection of the `math:`-grounded value-space facts authored in
    /// `slices/grounding/math/module.ttl` — the single source of truth. Any drift in
    /// either direction (the Rust table gaining/losing an entry, or the math slice
    /// changing a count / adding a value-space individual) fails here, so the two
    /// can never silently diverge into independent copies.
    #[test]
    fn rust_finite_cardinality_table_projects_the_math_grounding() {
        use purrdf::{NativeRdfFormat, RdfTerm, dataset_from_bytes};

        const MATH_HAS_CARDINALITY: &str = "https://blackcatinformatics.ca/math/hasCardinality";
        const MATH_CARDINALITY_FINITE: &str =
            "https://blackcatinformatics.ca/math/cardinalityFinite";
        const MATH_QUANTITY_VALUE: &str = "https://blackcatinformatics.ca/math/quantityValue";
        const RDFS_SEE_ALSO: &str = "http://www.w3.org/2000/01/rdf-schema#seeAlso";

        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../slices/grounding/math/module.ttl"
        );
        let bytes = std::fs::read(path).expect("read the math grounding slice");
        let dataset =
            dataset_from_bytes(&bytes, NativeRdfFormat::Turtle).expect("parse math module.ttl");

        // Fold the value-space individuals: a subject with math:hasCardinality
        // math:cardinalityFinite, an rdfs:seeAlso datatype, and a math:quantityValue
        // count → {datatype → count}.
        let mut finite_subject: BTreeSet<String> = BTreeSet::new();
        let mut see_also: BTreeMap<String, String> = BTreeMap::new();
        let mut quantity: BTreeMap<String, u128> = BTreeMap::new();
        for quad in dataset.owned_quads() {
            let RdfTerm::Iri(subject) = &quad.subject else {
                continue;
            };
            match quad.predicate.as_str() {
                MATH_HAS_CARDINALITY => {
                    if matches!(&quad.object, RdfTerm::Iri(o) if o == MATH_CARDINALITY_FINITE) {
                        finite_subject.insert(subject.clone());
                    }
                }
                RDFS_SEE_ALSO => {
                    if let RdfTerm::Iri(o) = &quad.object {
                        see_also.insert(subject.clone(), o.clone());
                    }
                }
                MATH_QUANTITY_VALUE => {
                    if let RdfTerm::Literal(l) = &quad.object
                        && let Ok(n) = l.lexical_form.trim().parse::<u128>()
                    {
                        quantity.insert(subject.clone(), n);
                    }
                }
                _ => {}
            }
        }

        let mut ttl_table: BTreeMap<String, u128> = BTreeMap::new();
        for subject in &finite_subject {
            let datatype = see_also.get(subject).unwrap_or_else(|| {
                panic!("value-space individual <{subject}> has no rdfs:seeAlso datatype")
            });
            let count = quantity.get(subject).unwrap_or_else(|| {
                panic!("value-space individual <{subject}> has no math:quantityValue")
            });
            ttl_table.insert(datatype.clone(), *count);
        }

        let rust_table: BTreeMap<String, u128> = FINITE_NAMED_CARDINALITY
            .iter()
            .map(|(dt, c)| ((*dt).to_owned(), *c))
            .collect();

        assert_eq!(
            rust_table, ttl_table,
            "the Rust FINITE_NAMED_CARDINALITY table must be exactly the projection of the \
             math: value-space facts in slices/grounding/math/module.ttl"
        );
        assert!(
            !ttl_table.is_empty(),
            "the math grounding must author at least one finite datatype value space"
        );
    }
}
