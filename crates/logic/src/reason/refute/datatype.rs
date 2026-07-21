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
//!   inconsistent), and whether a required literal satisfies the facets.
//! * **`owl:oneOf` datatype enumerations** — distinct-value counting across the
//!   whole rational tower (`xsd:decimal`/`xsd:integer`/`owl:rational` share one
//!   value space, so `"0.5"^^xsd:decimal` and `"1/2"^^owl:rational` are ONE value)
//!   through the exact-ℚ [`gmeow_math::Rational`] core.
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

use gmeow_math::Rational;
use purrdf::{RdfDataset, RdfLiteral, RdfTerm};

use super::{
    BoundKind, CountBound, Decision, FragmentFamily, NothingClash, RefutationCertificate, Witness,
    WitnessEvidence, certify_membership, is_rational_tower, parse_rational, resource_key,
    world_key,
};

// ── IRI constants (local to the subsolver) ──────────────────────────────────────
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
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
/// `owl:rational` tower shares one value space (`gmeow_math::Rational`); IEEE
/// `xsd:float`/`xsd:double` are their own (bit-distinct) value spaces; strings and
/// booleans are theirs.
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
            Self::Rat(q) => Some(format!("Q:{}", q.ratio_string())),
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
        facets: Facets,
    },
    /// `owl:datatypeComplementOf D` over the (infinite) literal universe.
    Complement(Box<Dt>),
    /// `owl:oneOf` datatype enumeration of exact values.
    Enumeration(Vec<Value>),
}

/// The facet bundle carried by an `owl:withRestrictions` datatype.
#[derive(Clone, Debug, Default)]
struct Facets {
    min_inclusive: Option<Value>,
    max_inclusive: Option<Value>,
    min_exclusive: Option<Value>,
    max_exclusive: Option<Value>,
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
            (self.min_inclusive.as_ref(), false, true),
            (self.min_exclusive.as_ref(), true, true),
            (self.max_inclusive.as_ref(), false, false),
            (self.max_exclusive.as_ref(), true, false),
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
        // Numeric-range facet emptiness, base-dispatched.
        match base {
            Dt::Float => self.float_range_empty::<f32>(),
            Dt::Double => self.float_range_empty::<f64>(),
            Dt::IntRange { lo, hi } => self.int_range_empty(*lo, *hi),
            Dt::DenseRational => self.dense_range_empty(),
            _ => Tri::Unknown,
        }
    }

    fn cardinality(&self, base: &Dt) -> Card {
        if self.emptiness(base) == Tri::Yes {
            return Card::Finite(0);
        }
        // A non-empty facet-restricted INTEGER range has an exact, arithmetic
        // count; other non-empty facet spaces are only known non-empty (Unknown
        // exact count), which suffices for a `required == 1` obligation.
        if let Dt::IntRange { lo, hi } = base {
            let elo = self.effective_int_lo(*lo);
            let ehi = self.effective_int_hi(*hi);
            if let (Some(lo), Some(hi)) = (elo, ehi) {
                return if hi < lo {
                    Card::Finite(0)
                } else {
                    Card::Finite((hi - lo + 1) as u128)
                };
            }
            return Card::Infinite;
        }
        Card::Unknown
    }

    fn float_range_empty<F: FloatLike>(&self) -> Tri {
        // The smallest value satisfying the lower bound, then test it against the
        // upper bound. Because the IEEE grid is discrete and monotone, if the
        // least lower-satisfying value fails the upper bound the whole set is empty.
        let lo = match (self.min_inclusive.as_ref(), self.min_exclusive.as_ref()) {
            (Some(v), None) => match F::from_value(v) {
                Some(f) => Some((f, false)),
                None => return Tri::Unknown,
            },
            (None, Some(v)) => match F::from_value(v) {
                Some(f) => Some((f, true)),
                None => return Tri::Unknown,
            },
            (None, None) => None,
            (Some(_), Some(_)) => return Tri::Unknown,
        };
        let hi = match (self.max_inclusive.as_ref(), self.max_exclusive.as_ref()) {
            (Some(v), None) => match F::from_value(v) {
                Some(f) => Some((f, false)),
                None => return Tri::Unknown,
            },
            (None, Some(v)) => match F::from_value(v) {
                Some(f) => Some((f, true)),
                None => return Tri::Unknown,
            },
            (None, None) => None,
            (Some(_), Some(_)) => return Tri::Unknown,
        };
        let (Some((lo, lo_excl)), Some((hi, hi_excl))) = (lo, hi) else {
            // A half-bounded IEEE range is non-empty (the grid is dense enough).
            return Tri::No;
        };
        if lo.is_nan() || hi.is_nan() {
            return Tri::Unknown;
        }
        let least = if lo_excl { F::next_up(lo) } else { lo };
        let fits_upper = if hi_excl { least.lt(hi) } else { least.le(hi) };
        if fits_upper { Tri::No } else { Tri::Yes }
    }

    fn effective_int_lo(&self, base_lo: Option<i128>) -> Option<i128> {
        let facet_lo = self.min_inclusive.as_ref().and_then(int_value).or_else(|| {
            self.min_exclusive
                .as_ref()
                .and_then(int_value)
                .map(|v| v + 1)
        });
        match (base_lo, facet_lo) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    fn effective_int_hi(&self, base_hi: Option<i128>) -> Option<i128> {
        let facet_hi = self.max_inclusive.as_ref().and_then(int_value).or_else(|| {
            self.max_exclusive
                .as_ref()
                .and_then(int_value)
                .map(|v| v - 1)
        });
        match (base_hi, facet_hi) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    fn int_range_empty(&self, base_lo: Option<i128>, base_hi: Option<i128>) -> Tri {
        // A facet bound that is not an integer would need real reasoning we do not
        // do here; if any numeric facet is present but non-integer, withhold.
        for facet in [
            &self.min_inclusive,
            &self.max_inclusive,
            &self.min_exclusive,
            &self.max_exclusive,
        ] {
            if let Some(v) = facet
                && int_value(v).is_none()
            {
                return Tri::Unknown;
            }
        }
        match (
            self.effective_int_lo(base_lo),
            self.effective_int_hi(base_hi),
        ) {
            (Some(lo), Some(hi)) if lo > hi => Tri::Yes,
            _ => Tri::No,
        }
    }

    fn dense_range_empty(&self) -> Tri {
        let lo = self
            .min_inclusive
            .as_ref()
            .map(|v| (v, false))
            .or_else(|| self.min_exclusive.as_ref().map(|v| (v, true)));
        let hi = self
            .max_inclusive
            .as_ref()
            .map(|v| (v, false))
            .or_else(|| self.max_exclusive.as_ref().map(|v| (v, true)));
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

/// Abstraction over `f32`/`f64` for the discrete-grid emptiness test.
trait FloatLike: Copy {
    fn from_value(v: &Value) -> Option<Self>;
    fn is_nan(self) -> bool;
    fn next_up(self) -> Self;
    fn lt(self, other: Self) -> bool;
    fn le(self, other: Self) -> bool;
}

impl FloatLike for f32 {
    fn from_value(v: &Value) -> Option<Self> {
        match v {
            Value::F32(f) => Some(*f),
            _ => None,
        }
    }
    fn is_nan(self) -> bool {
        f32::is_nan(self)
    }
    fn next_up(self) -> Self {
        f32::next_up(self)
    }
    fn lt(self, other: Self) -> bool {
        self < other
    }
    fn le(self, other: Self) -> bool {
        self <= other
    }
}

impl FloatLike for f64 {
    fn from_value(v: &Value) -> Option<Self> {
        match v {
            Value::F64(f) => Some(*f),
            _ => None,
        }
    }
    fn is_nan(self) -> bool {
        f64::is_nan(self)
    }
    fn next_up(self) -> Self {
        f64::next_up(self)
    }
    fn lt(self, other: Self) -> bool {
        self < other
    }
    fn le(self, other: Self) -> bool {
        self <= other
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
            let predicate = quad.predicate.as_str();
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
    fn resolve(&self, node: &str) -> Result<Dt, String> {
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
                facets,
            });
        }
        if let Some(list_head) = self.one_of.get(node) {
            let mut values = Vec::new();
            for member in self.list_members(list_head) {
                match &member {
                    RdfTerm::Literal(l) => values.push(
                        parse_value(l)
                            .ok_or_else(|| format!("unparsable enumeration member in <{node}>"))?,
                    ),
                    _ => return Err(format!("non-literal enumeration member in <{node}>")),
                }
            }
            return Ok(Dt::Enumeration(values));
        }
        Err(format!("unrecognized datatype <{node}>"))
    }

    fn parse_facets(&self, list_head: &str) -> Result<Facets, String> {
        let mut facets = Facets::default();
        for member in self.list_members(list_head) {
            let Some(node) = resource_key(&member) else {
                return Err("non-resource facet in owl:withRestrictions".to_owned());
            };
            let Some((prop, lit)) = self.facet_carrier.get(&node) else {
                return Err(format!("facet node <{node}> carries no facet"));
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
                other => return Err(format!("unrecognized facet <{other}>")),
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

    /// The datatype value-space obligations present in the EDB — one per
    /// `(world, subject, datatype-property)` that carries a value-space constraint
    /// (a type-restriction, or an `rdfs:range` to a constrained datatype) plus any
    /// cardinality / existential requirement and asserted literal values.
    fn obligations(&self) -> Vec<Obligation> {
        // Candidate `(world, subject, prop)` keys → the restriction nodes that
        // constrain that property on that subject (possibly empty for a pure
        // range-membership obligation).
        let mut keys: BTreeMap<(String, String, String), BTreeSet<String>> = BTreeMap::new();

        // Source 1 — an individual directly typed to a datatype-property restriction.
        for ((world, individual), classes) in &self.types {
            for class in classes {
                if let Some(r) = self.restrictions.get(class)
                    && let Some(p) = &r.on_property
                    && self.datatype_props.contains(p)
                {
                    keys.entry((world.clone(), individual.clone(), p.clone()))
                        .or_default()
                        .insert(class.clone());
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

        // Resolve the effective value space. Zero constraints ⇒ the universe
        // (rdfs:Literal, infinite). More than one distinct constraining datatype ⇒
        // a real intersection we do not compute soundly ⇒ obstruction.
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
        let space = match spaces.len() {
            0 => None,
            1 => match model.resolve(&spaces[0]) {
                Ok(dt) => Some(dt),
                Err(reason) => return Outcome::Obstructed(reason),
            },
            _ => {
                return Outcome::Obstructed(format!(
                    "intersection of {} distinct datatype constraints on <{}> is outside the \
                     certified fragment",
                    spaces.len(),
                    self.property
                ));
            }
        };

        // Membership: every asserted literal value must lie in the value space.
        if let Some(space) = &space
            && let Some(lits) = model.values.get(&value_key)
        {
            for lit in lits {
                let Some(v) = parse_value(lit) else {
                    return Outcome::Obstructed(format!(
                        "unparsable literal on <{}>",
                        self.property
                    ));
                };
                match space.contains(&v) {
                    Tri::Yes => {}
                    Tri::No => {
                        return self.clash(model, None);
                    }
                    Tri::Unknown => {
                        return Outcome::Obstructed(format!(
                            "literal membership in the value space of <{}> is undecidable",
                            self.property
                        ));
                    }
                }
            }
        }

        // Counting: the value space must hold `required` distinct values.
        if required >= 1 {
            let Some(space) = &space else {
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

// ── Literal parsing ─────────────────────────────────────────────────────────────

/// Parse a non-negative-integer cardinality literal.
fn parse_usize(lit: &RdfLiteral) -> Option<usize> {
    lit.lexical_form.trim().parse::<usize>().ok()
}

/// Parse an xsd length-facet literal (a non-negative integer).
fn parse_len_facet(lit: &RdfLiteral) -> Result<usize, String> {
    lit.lexical_form
        .trim()
        .parse::<usize>()
        .map_err(|_| format!("non-integer length facet {:?}", lit.lexical_form))
}

/// Parse a numeric facet literal into an exact [`Value`] (float/double stay IEEE).
fn parse_num_facet(lit: &RdfLiteral) -> Result<Value, String> {
    parse_value(lit)
        .filter(|v| matches!(v, Value::Rat(_) | Value::F32(_) | Value::F64(_)))
        .ok_or_else(|| format!("non-numeric facet literal {:?}", lit.lexical_form))
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
        Some(OWL_RATIONAL) => parse_rational(lexical).map(Value::Rat),
        Some(dt) if dt == OWL_REAL => parse_rational(lexical).map(Value::Rat),
        Some(dt) if is_rational_tower(dt) => Rational::parse_decimal(lexical).ok().map(Value::Rat),
        Some(_) => None,
    }
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
            min_exclusive: Some(Value::F32(0.0)),
            max_exclusive: Some(Value::F32("1.401298464324817e-45".parse().unwrap())),
            ..Facets::default()
        };
        let dt = Dt::Facet {
            base: Box::new(Dt::Float),
            facets,
        };
        assert_eq!(dt.emptiness(), Tri::Yes);
        // A wider range is non-empty.
        let wide = Dt::Facet {
            base: Box::new(Dt::Float),
            facets: Facets {
                min_exclusive: Some(Value::F32(0.0)),
                max_exclusive: Some(Value::F32(1.0)),
                ..Facets::default()
            },
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
            facets: Facets {
                min_length: Some(5),
                max_length: Some(3),
                ..Facets::default()
            },
        };
        assert_eq!(empty.emptiness(), Tri::Yes);
        let ok = Dt::Facet {
            base: Box::new(Dt::Str),
            facets: Facets {
                min_length: Some(2),
                max_length: Some(5),
                ..Facets::default()
            },
        };
        assert_eq!(ok.emptiness(), Tri::No);
    }

    #[test]
    fn pattern_facet_withholds() {
        let dt = Dt::Facet {
            base: Box::new(Dt::Str),
            facets: Facets {
                has_pattern: true,
                ..Facets::default()
            },
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
