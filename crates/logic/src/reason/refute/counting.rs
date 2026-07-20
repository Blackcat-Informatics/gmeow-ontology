// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Family 2/6a/7 — the counting / arithmetic-feasibility refutation sub-decider.
//!
//! This is the SECOND real sub-decider registered in [`super::SUB_DECIDERS`]
//! (after [`super::datatype`]). It decides, soundly and completely for a
//! precisely-characterized fragment, three families of counting obligations the
//! native forward chase ([`crate::reason::dl`]) withholds:
//!
//! * **Family 2 — number / cardinality counting.** A `min`/`max`/exact cardinality
//!   restriction sitting in a class-DEFINITION position (`rdfs:subClassOf` /
//!   `owl:equivalentClass` superclass, possibly nested through `owl:intersectionOf`)
//!   is satisfiable iff, per property, its effective bounds do not collapse
//!   (`min > max`). A collapsed bound on a POPULATED class forces every instance
//!   into `owl:Nothing`; an uncollapsed one is consistent. The fragment is the pure
//!   cardinality TBox: a case is certified consistent only when every construct
//!   present is cardinality scaffolding (no existentials, nominals, disjointness, or
//!   identity vocabulary that could interact with the count).
//! * **Family 6a — inverse-functional / functional identity collapse.** An
//!   `owl:InverseFunctionalProperty` merges the subjects that share a value (the
//!   classic `1 = 2` collapse); `owl:FunctionalProperty` merges the values that
//!   share a subject; `owl:inverseOf` propagates assertions across the inverse; and
//!   `owl:sameAs` seeds merges directly. A merged pair asserted
//!   `owl:differentFrom` (or co-listed distinct in `owl:AllDifferent`) is a clash.
//!   `owl:InverseFunctionalProperty` was NEVER promoted before this decider — the
//!   chase carries no identity-merge rule — so this wires the real IFP `sameAs`
//!   propagation and its clash. The fragment is the pure assertional / identity
//!   ABox: no class-construction vocabulary may be present.
//! * **Family 7 — `owl:hasSelf` membership refutation.** A `∃p.Self`
//!   self-restriction is `owl:disjointWith` a class `C`; an individual bearing the
//!   self-edge (`x p x`) that is also typed `C` inhabits both disjoint classes — a
//!   clash the chase misses because it does not infer self-membership from the edge.
//!
//! Anything the decider cannot prove complete returns
//! [`super::RefutationCertificate::OutOfFragment`] with a precise
//! [`super::FragmentBoundary`] obstruction — NEVER a guess. Unbounded / anti-counting
//! obligations that mix a counting construct with an existential, nominal, or
//! property-chain construct the decider does not fold into the count are bounded OUT
//! (the whole-case completeness gate refuses them), so a `Consistent` verdict is
//! sound by construction: a case is certified consistent only when EVERY construct
//! present lies inside the family fragment that engaged.
//!
//! Every collection is `BTreeSet`/`BTreeMap`/sorted-`Vec` ordered so a certificate
//! is byte-stable (the native contract hash and reasoning goldens depend on it).

use std::collections::{BTreeMap, BTreeSet};

use gmeow_math::Rational;
use purrdf::{RdfDataset, RdfLiteral, RdfTerm};

use super::{
    Decision, FragmentFamily, NothingClash, RefutationCertificate, Witness, WitnessEvidence,
    certify_membership,
};
use crate::facts::skolem_iri;

// ── IRI constants ────────────────────────────────────────────────────────────────
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";
const RDFS_SEE_ALSO: &str = "http://www.w3.org/2000/01/rdf-schema#seeAlso";
const RDFS_IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";

const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_RESTRICTION: &str = "http://www.w3.org/2002/07/owl#Restriction";
const OWL_ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
const OWL_ON_CLASS: &str = "http://www.w3.org/2002/07/owl#onClass";
const OWL_ON_DATA_RANGE: &str = "http://www.w3.org/2002/07/owl#onDataRange";
const OWL_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#cardinality";
const OWL_MIN_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#minCardinality";
const OWL_MAX_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#maxCardinality";
const OWL_INTERSECTION_OF: &str = "http://www.w3.org/2002/07/owl#intersectionOf";
const OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";
const OWL_HAS_SELF: &str = "http://www.w3.org/2002/07/owl#hasSelf";

const OWL_INVERSE_FUNCTIONAL_PROPERTY: &str =
    "http://www.w3.org/2002/07/owl#InverseFunctionalProperty";
const OWL_FUNCTIONAL_PROPERTY: &str = "http://www.w3.org/2002/07/owl#FunctionalProperty";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const OWL_NAMED_INDIVIDUAL: &str = "http://www.w3.org/2002/07/owl#NamedIndividual";
const OWL_THING: &str = "http://www.w3.org/2002/07/owl#Thing";
const OWL_ONTOLOGY: &str = "http://www.w3.org/2002/07/owl#Ontology";
const OWL_INVERSE_OF: &str = "http://www.w3.org/2002/07/owl#inverseOf";
const OWL_SAME_AS: &str = "http://www.w3.org/2002/07/owl#sameAs";
const OWL_DIFFERENT_FROM: &str = "http://www.w3.org/2002/07/owl#differentFrom";
const OWL_ALL_DIFFERENT: &str = "http://www.w3.org/2002/07/owl#AllDifferent";
const OWL_DISTINCT_MEMBERS: &str = "http://www.w3.org/2002/07/owl#distinctMembers";
const OWL_MEMBERS: &str = "http://www.w3.org/2002/07/owl#members";

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
const XSD_FLOAT: &str = "http://www.w3.org/2001/XMLSchema#float";
const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";
const OWL_RATIONAL: &str = "http://www.w3.org/2002/07/owl#rational";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";

const RULE_CARDINALITY: &str = "refute:counting-cardinality";
const RULE_IDENTITY: &str = "refute:counting-inverse-functional";
const RULE_HAS_SELF: &str = "refute:counting-has-self";

/// The scaffolding predicates the Family-2 cardinality fragment fully handles. A
/// case carrying ANY predicate outside this allowlist is refused a `Consistent`
/// certificate (an assertion, a nominal, a disjointness, or an identity axiom could
/// interact with the count in a way this decider does not fold in). A collapsed
/// bound is still refuted (a clash is decisive) regardless.
const ALLOWED_CARDINALITY_PREDICATES: &[&str] = &[
    RDF_TYPE,
    RDF_FIRST,
    RDF_REST,
    RDFS_SUBCLASSOF,
    RDFS_LABEL,
    RDFS_COMMENT,
    RDFS_SEE_ALSO,
    RDFS_IS_DEFINED_BY,
    OWL_ON_PROPERTY,
    OWL_ON_CLASS,
    OWL_ON_DATA_RANGE,
    OWL_CARDINALITY,
    OWL_MIN_CARDINALITY,
    OWL_MAX_CARDINALITY,
    OWL_INTERSECTION_OF,
    OWL_EQUIVALENT_CLASS,
];

/// Class-construction / restriction vocabulary whose presence takes a case OUT of
/// the pure assertional/identity fragment the Family-6a and Family-7 consistency
/// certificates require. Property assertions carry arbitrary predicates, so the
/// identity/has-self fragments cannot allowlist predicates — instead the mere
/// presence of any of these blocks a `Consistent` verdict (a clash is still
/// decisive). Identity vocabulary (`owl:sameAs`/`differentFrom`/`AllDifferent`/
/// `inverseOf`) is deliberately NOT here — it is part of the fragment.
const CLASS_CONSTRUCTION_PREDICATES: &[&str] = &[
    RDFS_SUBCLASSOF,
    OWL_EQUIVALENT_CLASS,
    OWL_DISJOINT_WITH,
    OWL_ON_PROPERTY,
    OWL_ON_CLASS,
    OWL_ON_DATA_RANGE,
    OWL_HAS_SELF,
    OWL_CARDINALITY,
    OWL_MIN_CARDINALITY,
    OWL_MAX_CARDINALITY,
    "http://www.w3.org/2002/07/owl#qualifiedCardinality",
    "http://www.w3.org/2002/07/owl#minQualifiedCardinality",
    "http://www.w3.org/2002/07/owl#maxQualifiedCardinality",
    "http://www.w3.org/2002/07/owl#someValuesFrom",
    "http://www.w3.org/2002/07/owl#allValuesFrom",
    "http://www.w3.org/2002/07/owl#hasValue",
    "http://www.w3.org/2002/07/owl#oneOf",
    "http://www.w3.org/2002/07/owl#unionOf",
    OWL_INTERSECTION_OF,
    "http://www.w3.org/2002/07/owl#complementOf",
    "http://www.w3.org/2002/07/owl#disjointUnionOf",
    "http://www.w3.org/2002/07/owl#hasKey",
    "http://www.w3.org/2002/07/owl#propertyChainAxiom",
    "http://www.w3.org/2002/07/owl#propertyDisjointWith",
    "http://www.w3.org/2000/01/rdf-schema#domain",
    "http://www.w3.org/2000/01/rdf-schema#range",
];

// ── The registered sub-decider entrypoint ───────────────────────────────────────

/// The [`super::SubDecider`] for the counting / arithmetic-feasibility family.
///
/// Tries the three family analyses. A proven clash from any family is DECISIVE
/// (`Inconsistent`, materializing an `owl:Nothing` witness) regardless of the
/// others. Otherwise a `Consistent` verdict is licensed only when every engaged
/// family certified consistency with no obstruction — a family that engaged but
/// could not prove its whole-case completeness bound refuses the case into an
/// `OutOfFragment` withhold.
pub(crate) fn decide(edb: &RdfDataset) -> Option<RefutationCertificate> {
    let model = Model::scan(edb);
    let outcomes = [
        analyze_cardinality(&model),
        analyze_identity(&model),
        analyze_has_self(&model),
    ];

    let mut engaged = false;
    let mut clashes: BTreeSet<NothingClash> = BTreeSet::new();
    let mut counted: BTreeSet<String> = BTreeSet::new();
    let mut obstructions: BTreeSet<String> = BTreeSet::new();
    for outcome in outcomes {
        match outcome {
            Verdict::NotEngaged => {}
            Verdict::Consistent => engaged = true,
            Verdict::Inconsistent { clashes: cs } => {
                engaged = true;
                for c in cs {
                    counted.insert(c.individual.clone());
                    clashes.insert(c);
                }
            }
            Verdict::Withhold(obs) => {
                engaged = true;
                obstructions.extend(obs);
            }
        }
    }

    if !engaged {
        return None;
    }

    // A proven clash is decisive — the ontology IS inconsistent regardless of any
    // obstruction a sibling family raised. Only a Consistent decision needs an
    // obstruction-free evaluation across every engaged family.
    if !clashes.is_empty() {
        return Some(certify_membership(
            FragmentFamily::Counting,
            BTreeSet::new(),
            move || {
                (
                    Decision::Inconsistent,
                    Witness {
                        family: FragmentFamily::Counting,
                        clashes,
                        evidence: WitnessEvidence {
                            counted_individuals: counted,
                            violated_bound: None,
                            closed_branch: None,
                        },
                    },
                )
            },
        ));
    }

    Some(certify_membership(
        FragmentFamily::Counting,
        obstructions,
        || {
            (
                Decision::Consistent,
                Witness {
                    family: FragmentFamily::Counting,
                    clashes: BTreeSet::new(),
                    evidence: WitnessEvidence::default(),
                },
            )
        },
    ))
}

/// True iff the Family-2 cardinality analysis DECIDES `edb` (a `Consistent` or
/// `Inconsistent` verdict). The coverage coordinator ([`crate::reason::dl`])
/// consults this to keep the cardinality families `decided` — and to narrow their
/// class-definition withhold — exactly when the decider has completely decided them.
pub(crate) fn decides_cardinality(edb: &RdfDataset) -> bool {
    matches!(
        analyze_cardinality(&Model::scan(edb)),
        Verdict::Consistent | Verdict::Inconsistent { .. }
    )
}

/// True iff the Family-6a inverse-functional / identity analysis DECIDES `edb`.
/// Gates the promotion of `owl:InverseFunctionalProperty` out of the never-decided
/// gap set (it carries no native identity-merge rule).
pub(crate) fn decides_identity(edb: &RdfDataset) -> bool {
    matches!(
        analyze_identity(&Model::scan(edb)),
        Verdict::Consistent | Verdict::Inconsistent { .. }
    )
}

/// True iff the Family-7 `owl:hasSelf` analysis DECIDES `edb`. Gates narrowing the
/// `hasSelf` refutation-shape withhold.
pub(crate) fn decides_has_self(edb: &RdfDataset) -> bool {
    matches!(
        analyze_has_self(&Model::scan(edb)),
        Verdict::Consistent | Verdict::Inconsistent { .. }
    )
}

/// The per-family analysis result.
enum Verdict {
    /// The family shape is absent — the family does not engage.
    NotEngaged,
    /// The family proves the case (its fragment) CONSISTENT.
    Consistent,
    /// The family proves a clash — the case is INCONSISTENT.
    Inconsistent { clashes: BTreeSet<NothingClash> },
    /// The family shape is present but its completeness bound did not close.
    Withhold(BTreeSet<String>),
}

// ── Family 2 — cardinality counting ──────────────────────────────────────────────

fn analyze_cardinality(m: &Model) -> Verdict {
    // Engage iff some restriction carries a plain (unqualified) cardinality bound.
    let engaged = m
        .restrictions
        .values()
        .any(|r| r.min.is_some() || r.max.is_some() || r.exact.is_some());
    if !engaged {
        return Verdict::NotEngaged;
    }

    // Collapsed-bound clashes on populated classes are decisive regardless of purity.
    let clashes = cardinality_clashes(m);
    if !clashes.is_empty() {
        return Verdict::Inconsistent { clashes };
    }

    // Whole-case completeness: every predicate present must be cardinality
    // scaffolding, and no identity/characteristic type-object may interact with the
    // count. Anything else is an honest obstruction.
    let mut obstructions: BTreeSet<String> = BTreeSet::new();
    for predicate in &m.predicates {
        if !ALLOWED_CARDINALITY_PREDICATES.contains(&predicate.as_str()) {
            obstructions.insert(format!(
                "cardinality fragment: unhandled predicate <{predicate}> may interact with the count"
            ));
        }
    }
    for object in &m.type_objects {
        if object == OWL_INVERSE_FUNCTIONAL_PROPERTY || object == OWL_FUNCTIONAL_PROPERTY {
            obstructions.insert(format!(
                "cardinality fragment: property characteristic <{object}> may interact with the count"
            ));
        }
    }
    if obstructions.is_empty() {
        Verdict::Consistent
    } else {
        Verdict::Withhold(obstructions)
    }
}

/// Every `owl:Nothing` clash forced by a collapsed cardinality bound on a populated
/// class. For each class `C` reachable-as-superclass of a restriction set, the
/// effective per-property bounds are `min = max(all mins ∪ exacts)` and
/// `max = min(all maxes ∪ exacts)`; when `min > max` the class is unsatisfiable, so
/// every individual directly typed `C` is forced into `owl:Nothing`.
fn cardinality_clashes(m: &Model) -> BTreeSet<NothingClash> {
    let mut clashes: BTreeSet<NothingClash> = BTreeSet::new();
    for ((world, class), nodes) in m.class_restrictions() {
        // Per property: effective (min, max).
        let mut per_property: BTreeMap<String, (Option<u128>, Option<u128>)> = BTreeMap::new();
        for node in &nodes {
            let Some(r) = m.restrictions.get(&(world.clone(), node.clone())) else {
                continue;
            };
            let Some(property) = &r.on_property else {
                continue;
            };
            let entry = per_property.entry(property.clone()).or_insert((None, None));
            for lower in [r.min, r.exact].into_iter().flatten() {
                entry.0 = Some(entry.0.map_or(lower, |cur: u128| cur.max(lower)));
            }
            for upper in [r.max, r.exact].into_iter().flatten() {
                entry.1 = Some(entry.1.map_or(upper, |cur: u128| cur.min(upper)));
            }
        }
        let collapsed_property = per_property.iter().find_map(|(property, (lo, hi))| {
            matches!((lo, hi), (Some(lo), Some(hi)) if lo > hi).then(|| property.clone())
        });
        let Some(property) = collapsed_property else {
            continue;
        };
        // The class is unsatisfiable; each individual directly typed `C` clashes.
        for individual in m.instances_of(&world, &class) {
            let mut premises: Vec<(String, String, String)> =
                vec![(individual.clone(), RDF_TYPE.to_owned(), class.clone())];
            for node in &nodes {
                premises.push((class.clone(), RDFS_SUBCLASSOF.to_owned(), node.clone()));
                premises.push((node.clone(), OWL_ON_PROPERTY.to_owned(), property.clone()));
            }
            premises.sort();
            premises.dedup();
            clashes.insert(NothingClash {
                individual,
                world: world.clone(),
                rule_name: RULE_CARDINALITY.to_owned(),
                premises,
            });
        }
    }
    clashes
}

// ── Family 6a — inverse-functional / functional identity collapse ────────────────

fn analyze_identity(m: &Model) -> Verdict {
    if m.inverse_functional_props.is_empty() {
        return Verdict::NotEngaged;
    }

    // Identity-merge clashes are decisive regardless of purity.
    let clashes = identity_clashes(m);
    if !clashes.is_empty() {
        return Verdict::Inconsistent { clashes };
    }

    // Whole-case completeness: the pure assertional / identity ABox. Any
    // class-construction vocabulary could add merges or unsatisfiability this
    // assertional analysis does not fold in.
    let mut obstructions: BTreeSet<String> = BTreeSet::new();
    for predicate in &m.predicates {
        if CLASS_CONSTRUCTION_PREDICATES.contains(&predicate.as_str()) {
            obstructions.insert(format!(
                "identity fragment: class-construction predicate <{predicate}> is outside the \
                 assertional fragment"
            ));
        }
    }
    if m.type_objects.contains(OWL_RESTRICTION) {
        obstructions.insert(
            "identity fragment: an owl:Restriction is outside the assertional fragment".to_owned(),
        );
    }
    if obstructions.is_empty() {
        Verdict::Consistent
    } else {
        Verdict::Withhold(obstructions)
    }
}

/// Every `owl:Nothing` clash forced by an inverse-functional / functional identity
/// merge colliding with an asserted distinctness. The `sameAs` closure is the
/// least fixed point of: explicit `owl:sameAs`; two subjects of an
/// `owl:InverseFunctionalProperty` sharing a value; two values of an
/// `owl:FunctionalProperty` sharing a subject; both directions of an
/// `owl:inverseOf` assertion. A merged pair asserted `owl:differentFrom`, or two
/// merged members of an `owl:AllDifferent` list, is a clash.
fn identity_clashes(m: &Model) -> BTreeSet<NothingClash> {
    let mut uf = UnionFind::default();
    // Register every individual so a lone assertion still has a class.
    for (world, subject, _predicate, object) in &m.assertions {
        uf.make(&ind_key(world, subject));
        if let Obj::Res(o) = object {
            uf.make(&ind_key(world, o));
        }
    }
    for (world, a, b) in &m.same_as {
        uf.union(&ind_key(world, a), &ind_key(world, b));
    }

    // Augment assertions with the inverse direction: `p inverseOf q` and `s p o`
    // (o a resource) ⇒ `o q s`.
    let mut assertions = m.assertions.clone();
    for (world, subject, predicate, object) in &m.assertions {
        if let Obj::Res(o) = object {
            for (p, q) in &m.inverse_of {
                if p == predicate {
                    assertions.push((
                        world.clone(),
                        o.clone(),
                        q.clone(),
                        Obj::Res(subject.clone()),
                    ));
                }
                if q == predicate {
                    assertions.push((
                        world.clone(),
                        o.clone(),
                        p.clone(),
                        Obj::Res(subject.clone()),
                    ));
                }
            }
        }
    }

    // Fixed point: IFP merges subjects sharing a (canonicalized) value; a functional
    // property merges the resource values sharing a subject. Canonicalizing through
    // the union-find lets a merge cascade (a merged value re-groups its subjects).
    loop {
        let mut changed = false;
        // IFP: group subjects by (world, property, canonical value).
        let mut by_value: BTreeMap<(String, String, String), Vec<String>> = BTreeMap::new();
        for (world, subject, predicate, object) in &assertions {
            if !m.inverse_functional_props.contains(predicate) {
                continue;
            }
            let value_key = match object {
                Obj::Res(o) => uf.find(&ind_key(world, o)),
                Obj::Lit(k) => format!("lit\u{1f}{k}"),
            };
            by_value
                .entry((world.clone(), predicate.clone(), value_key))
                .or_default()
                .push(ind_key(world, subject));
        }
        for members in by_value.values() {
            for pair in members.windows(2) {
                changed |= uf.union(&pair[0], &pair[1]);
            }
        }
        // Functional: group resource values by (world, property, canonical subject).
        let mut by_subject: BTreeMap<(String, String, String), Vec<String>> = BTreeMap::new();
        for (world, subject, predicate, object) in &assertions {
            if !m.functional_props.contains(predicate) {
                continue;
            }
            if let Obj::Res(o) = object {
                let subject_key = uf.find(&ind_key(world, subject));
                by_subject
                    .entry((world.clone(), predicate.clone(), subject_key))
                    .or_default()
                    .push(ind_key(world, o));
            }
        }
        for members in by_subject.values() {
            for pair in members.windows(2) {
                changed |= uf.union(&pair[0], &pair[1]);
            }
        }
        if !changed {
            break;
        }
    }

    let mut clashes: BTreeSet<NothingClash> = BTreeSet::new();
    let mut record = |world: &str, a: &str, b: &str| {
        if uf.find(&ind_key(world, a)) == uf.find(&ind_key(world, b)) {
            let mut premises = vec![(a.to_owned(), OWL_DIFFERENT_FROM.to_owned(), b.to_owned())];
            premises.sort();
            clashes.insert(NothingClash {
                individual: a.to_owned(),
                world: world.to_owned(),
                rule_name: RULE_IDENTITY.to_owned(),
                premises,
            });
        }
    };
    for (world, a, b) in &m.different_from {
        record(world, a, b);
    }
    for (world, distinct) in &m.all_different {
        for i in 0..distinct.len() {
            for j in (i + 1)..distinct.len() {
                record(world, &distinct[i], &distinct[j]);
            }
        }
    }
    clashes
}

// ── Family 7 — owl:hasSelf membership refutation ─────────────────────────────────

fn analyze_has_self(m: &Model) -> Verdict {
    let engaged = m.restrictions.values().any(|r| r.has_self);
    if !engaged {
        return Verdict::NotEngaged;
    }

    let clashes = has_self_clashes(m);
    if !clashes.is_empty() {
        return Verdict::Inconsistent { clashes };
    }

    // Whole-case completeness for a `Consistent` verdict: the self-restriction and
    // its neighbourhood must be the only class-construction present. A property
    // chain, existential, nominal, or equivalence definition is outside the
    // certified fragment (it can populate a class the self-restriction contradicts).
    const HAS_SELF_CONSISTENT_FORBIDDEN: &[&str] = &[
        "http://www.w3.org/2002/07/owl#someValuesFrom",
        "http://www.w3.org/2002/07/owl#allValuesFrom",
        "http://www.w3.org/2002/07/owl#hasValue",
        "http://www.w3.org/2002/07/owl#oneOf",
        "http://www.w3.org/2002/07/owl#unionOf",
        OWL_INTERSECTION_OF,
        "http://www.w3.org/2002/07/owl#complementOf",
        "http://www.w3.org/2002/07/owl#propertyChainAxiom",
        "http://www.w3.org/2002/07/owl#hasKey",
        OWL_CARDINALITY,
        OWL_MIN_CARDINALITY,
        OWL_MAX_CARDINALITY,
        OWL_EQUIVALENT_CLASS,
        OWL_INVERSE_OF,
    ];
    let mut obstructions: BTreeSet<String> = BTreeSet::new();
    for predicate in &m.predicates {
        if HAS_SELF_CONSISTENT_FORBIDDEN.contains(&predicate.as_str()) {
            obstructions.insert(format!(
                "hasSelf fragment: <{predicate}> can populate a class the self-restriction \
                 contradicts — outside the certified fragment"
            ));
        }
    }
    if obstructions.is_empty() {
        Verdict::Consistent
    } else {
        Verdict::Withhold(obstructions)
    }
}

/// Every `owl:Nothing` clash forced by a self-edge inhabiting a `∃p.Self`
/// self-restriction that is `owl:disjointWith` a class the individual also holds.
fn has_self_clashes(m: &Model) -> BTreeSet<NothingClash> {
    let mut clashes: BTreeSet<NothingClash> = BTreeSet::new();
    for ((world, node), r) in &m.restrictions {
        if !r.has_self {
            continue;
        }
        let Some(property) = &r.on_property else {
            continue;
        };
        // Classes disjoint with this self-restriction node.
        let disjoint_classes: BTreeSet<&String> = m
            .disjoint_with
            .iter()
            .filter(|(w, _, _)| w == world)
            .filter_map(|(_, a, b)| {
                if a == node {
                    Some(b)
                } else if b == node {
                    Some(a)
                } else {
                    None
                }
            })
            .collect();
        if disjoint_classes.is_empty() {
            continue;
        }
        // Individuals bearing the self-edge `x p x`.
        for (w, subject, predicate, object) in &m.assertions {
            if w != world || predicate != property {
                continue;
            }
            let Obj::Res(o) = object else { continue };
            if o != subject {
                continue;
            }
            // `subject ∈ ∃p.Self`. A clash arises for each disjoint class it holds.
            let holds = m
                .types
                .get(&(world.clone(), subject.clone()))
                .map(|classes| {
                    disjoint_classes
                        .iter()
                        .filter(|c| classes.contains(**c))
                        .cloned()
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            for class in holds {
                let mut premises = vec![
                    (subject.clone(), predicate.clone(), subject.clone()),
                    (node.clone(), OWL_ON_PROPERTY.to_owned(), property.clone()),
                    (subject.clone(), RDF_TYPE.to_owned(), class.clone()),
                    (class.clone(), OWL_DISJOINT_WITH.to_owned(), node.clone()),
                ];
                premises.sort();
                premises.dedup();
                clashes.insert(NothingClash {
                    individual: subject.clone(),
                    world: world.clone(),
                    rule_name: RULE_HAS_SELF.to_owned(),
                    premises,
                });
            }
        }
    }
    clashes
}

// ── The EDB model ────────────────────────────────────────────────────────────────

#[derive(Default)]
struct Restr {
    on_property: Option<String>,
    min: Option<u128>,
    max: Option<u128>,
    exact: Option<u128>,
    has_self: bool,
}

/// An assertion object: a resource (IRI/blank) key, or a canonical literal value key.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Obj {
    Res(String),
    Lit(String),
}

#[derive(Default)]
struct Model {
    /// `(world, individual) → asserted rdf:type class IRIs`.
    types: BTreeMap<(String, String), BTreeSet<String>>,
    /// Restriction nodes keyed `(world, node)`.
    restrictions: BTreeMap<(String, String), Restr>,
    /// `(world, class) → rdfs:subClassOf / owl:equivalentClass superclass targets`.
    superclasses: BTreeMap<(String, String), BTreeSet<String>>,
    /// `(world, node) → owl:intersectionOf list head`.
    intersection_of: BTreeMap<(String, String), String>,
    /// list head `(world, head) → ordered resource members`.
    lists: BTreeMap<(String, String), Vec<String>>,
    /// Property IRIs typed with each characteristic.
    inverse_functional_props: BTreeSet<String>,
    functional_props: BTreeSet<String>,
    /// `owl:inverseOf` pairs `(p, q)` (world-agnostic).
    inverse_of: BTreeSet<(String, String)>,
    /// Property assertions `(world, subject, predicate, object)`.
    assertions: Vec<(String, String, String, Obj)>,
    /// `owl:sameAs` / `owl:differentFrom` pairs `(world, a, b)`.
    same_as: Vec<(String, String, String)>,
    different_from: Vec<(String, String, String)>,
    /// `owl:AllDifferent` distinct-member sets `(world, members)`.
    all_different: Vec<(String, Vec<String>)>,
    /// `(world, class) → owl:disjointWith targets`, symmetric pairs `(world, a, b)`.
    disjoint_with: Vec<(String, String, String)>,
    /// Every predicate IRI present (for the whole-case completeness gate).
    predicates: BTreeSet<String>,
    /// Every `rdf:type` object IRI present.
    type_objects: BTreeSet<String>,
}

impl Model {
    fn scan(edb: &RdfDataset) -> Self {
        let mut m = Model::default();
        // Raw list edges for the resource-member list walk.
        let mut first: BTreeMap<(String, String), RdfTerm> = BTreeMap::new();
        let mut rest: BTreeMap<(String, String), String> = BTreeMap::new();
        // AllDifferent nodes → their member/distinctMembers list heads.
        let mut all_different_heads: Vec<(String, String)> = Vec::new();
        let mut all_different_nodes: BTreeSet<(String, String)> = BTreeSet::new();

        let object_props_and_data: &[&str] = &[OWL_OBJECT_PROPERTY, OWL_DATATYPE_PROPERTY];

        for quad in edb.owned_quads() {
            let world = world_key(&quad.graph_name);
            let predicate = quad.predicate.clone();
            let Some(subject) = resource_key(&quad.subject) else {
                continue;
            };
            m.predicates.insert(predicate.clone());
            match predicate.as_str() {
                RDF_TYPE => {
                    if let Some(object) = resource_key(&quad.object) {
                        m.type_objects.insert(object.clone());
                        match object.as_str() {
                            OWL_INVERSE_FUNCTIONAL_PROPERTY => {
                                m.inverse_functional_props.insert(subject.clone());
                            }
                            OWL_FUNCTIONAL_PROPERTY => {
                                m.functional_props.insert(subject.clone());
                            }
                            OWL_ALL_DIFFERENT => {
                                all_different_nodes.insert((world.clone(), subject.clone()));
                            }
                            OWL_RESTRICTION | OWL_CLASS | OWL_THING | OWL_ONTOLOGY
                            | OWL_NAMED_INDIVIDUAL => {}
                            _ if object_props_and_data.contains(&object.as_str()) => {}
                            _ => {
                                m.types
                                    .entry((world.clone(), subject.clone()))
                                    .or_default()
                                    .insert(object.clone());
                            }
                        }
                    }
                }
                OWL_ON_PROPERTY => {
                    if let Some(v) = resource_key(&quad.object) {
                        m.restrictions
                            .entry((world.clone(), subject.clone()))
                            .or_default()
                            .on_property = Some(v);
                    }
                }
                OWL_CARDINALITY => {
                    if let Some(n) = literal_usize(&quad.object) {
                        m.restrictions
                            .entry((world.clone(), subject.clone()))
                            .or_default()
                            .exact = Some(n);
                    }
                }
                OWL_MIN_CARDINALITY => {
                    if let Some(n) = literal_usize(&quad.object) {
                        m.restrictions
                            .entry((world.clone(), subject.clone()))
                            .or_default()
                            .min = Some(n);
                    }
                }
                OWL_MAX_CARDINALITY => {
                    if let Some(n) = literal_usize(&quad.object) {
                        m.restrictions
                            .entry((world.clone(), subject.clone()))
                            .or_default()
                            .max = Some(n);
                    }
                }
                OWL_HAS_SELF => {
                    if matches!(&quad.object, RdfTerm::Literal(l) if is_true_literal(l)) {
                        m.restrictions
                            .entry((world.clone(), subject.clone()))
                            .or_default()
                            .has_self = true;
                    }
                }
                RDFS_SUBCLASSOF | OWL_EQUIVALENT_CLASS => {
                    if let Some(v) = resource_key(&quad.object) {
                        m.superclasses
                            .entry((world.clone(), subject.clone()))
                            .or_default()
                            .insert(v);
                    }
                }
                OWL_INTERSECTION_OF => {
                    if let Some(v) = resource_key(&quad.object) {
                        m.intersection_of
                            .insert((world.clone(), subject.clone()), v);
                    }
                }
                OWL_INVERSE_OF => {
                    if let Some(v) = resource_key(&quad.object) {
                        m.inverse_of.insert((subject.clone(), v));
                    }
                }
                OWL_SAME_AS => {
                    if let Some(v) = resource_key(&quad.object) {
                        m.same_as.push((world.clone(), subject.clone(), v));
                    }
                }
                OWL_DIFFERENT_FROM => {
                    if let Some(v) = resource_key(&quad.object) {
                        m.different_from.push((world.clone(), subject.clone(), v));
                    }
                }
                OWL_DISJOINT_WITH => {
                    if let Some(v) = resource_key(&quad.object) {
                        m.disjoint_with.push((world.clone(), subject.clone(), v));
                    }
                }
                OWL_DISTINCT_MEMBERS | OWL_MEMBERS => {
                    if let Some(v) = resource_key(&quad.object) {
                        all_different_heads.push((world.clone(), v));
                    }
                }
                RDF_FIRST => {
                    first.insert((world.clone(), subject.clone()), quad.object.clone());
                }
                RDF_REST => {
                    if let Some(v) = resource_key(&quad.object) {
                        rest.insert((world.clone(), subject.clone()), v);
                    }
                }
                RDFS_LABEL | RDFS_COMMENT | RDFS_SEE_ALSO | RDFS_IS_DEFINED_BY => {}
                _ => {
                    // A property assertion (any other predicate). Objects may be a
                    // resource or a canonical literal value.
                    let object = match &quad.object {
                        RdfTerm::Literal(l) => Some(Obj::Lit(literal_value_key(l))),
                        other => resource_key(other).map(Obj::Res),
                    };
                    if let Some(object) = object {
                        m.assertions.push((
                            world.clone(),
                            subject.clone(),
                            predicate.clone(),
                            object,
                        ));
                    }
                }
            }
        }

        // Walk resource-member lists to nil.
        let heads: BTreeSet<(String, String)> = first.keys().cloned().collect();
        for head in heads {
            let mut node = head.clone();
            let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
            let mut members: Vec<String> = Vec::new();
            while seen.insert(node.clone()) {
                let Some(f) = first.get(&node) else { break };
                if let Some(r) = resource_key(f) {
                    members.push(r);
                }
                match rest.get(&node) {
                    Some(next) if next != RDF_NIL => node = (node.0.clone(), next.clone()),
                    _ => break,
                }
            }
            m.lists.insert(head, members);
        }

        // Resolve AllDifferent member lists.
        let _ = all_different_nodes; // membership recorded; the list carries the members
        for (world, head) in all_different_heads {
            if let Some(members) = m.lists.get(&(world.clone(), head))
                && members.len() >= 2
            {
                m.all_different.push((world, members.clone()));
            }
        }
        m
    }

    /// The individuals that populate `class` in `world` — directly typed `class`, or
    /// typed a subclass whose `rdfs:subClassOf` / `owl:equivalentClass` up-closure
    /// reaches `class`. An instance of a subclass of an unsatisfiable class is itself
    /// forced empty, so populatedness must follow the class hierarchy (sound-by-
    /// construction, not merely on the direct-typing cases the targets exercise).
    fn instances_of(&self, world: &str, class: &str) -> Vec<String> {
        self.types
            .iter()
            .filter(|((w, _), classes)| {
                w == world
                    && classes
                        .iter()
                        .any(|typed| self.subclass_reaches(world, typed, class))
            })
            .map(|((_, individual), _)| individual.clone())
            .collect()
    }

    /// Whether `start ⊑* target` via `rdfs:subClassOf` / `owl:equivalentClass`
    /// superclass edges (reflexive: `start == target` reaches immediately).
    fn subclass_reaches(&self, world: &str, start: &str, target: &str) -> bool {
        if start == target {
            return true;
        }
        let mut worklist = vec![start.to_owned()];
        let mut seen: BTreeSet<String> = BTreeSet::new();
        while let Some(node) = worklist.pop() {
            if !seen.insert(node.clone()) {
                continue;
            }
            if node == target {
                return true;
            }
            if let Some(supers) = self.superclasses.get(&(world.to_owned(), node)) {
                worklist.extend(supers.iter().cloned());
            }
        }
        false
    }

    /// `(world, class) → the restriction nodes reachable as its superclass`
    /// (directly, or as `owl:intersectionOf` members, nested).
    fn class_restrictions(&self) -> BTreeMap<(String, String), BTreeSet<String>> {
        let mut out: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
        for ((world, class), supers) in &self.superclasses {
            let mut nodes: BTreeSet<String> = BTreeSet::new();
            let mut worklist: Vec<String> = supers.iter().cloned().collect();
            let mut seen: BTreeSet<String> = BTreeSet::new();
            while let Some(node) = worklist.pop() {
                if !seen.insert(node.clone()) {
                    continue;
                }
                if self
                    .restrictions
                    .contains_key(&(world.clone(), node.clone()))
                {
                    nodes.insert(node.clone());
                }
                if let Some(head) = self.intersection_of.get(&(world.clone(), node.clone()))
                    && let Some(members) = self.lists.get(&(world.clone(), head.clone()))
                {
                    worklist.extend(members.iter().cloned());
                }
            }
            if !nodes.is_empty() {
                out.insert((world.clone(), class.clone()), nodes);
            }
        }
        out
    }
}

// ── Union-find over individual keys ──────────────────────────────────────────────

#[derive(Default)]
struct UnionFind {
    parent: BTreeMap<String, String>,
}

impl UnionFind {
    fn make(&mut self, x: &str) {
        self.parent
            .entry(x.to_owned())
            .or_insert_with(|| x.to_owned());
    }

    fn find(&mut self, x: &str) -> String {
        self.make(x);
        let mut root = x.to_owned();
        while let Some(p) = self.parent.get(&root) {
            if p == &root {
                break;
            }
            root = p.clone();
        }
        // Path compression.
        let mut node = x.to_owned();
        while node != root {
            let next = self
                .parent
                .get(&node)
                .cloned()
                .unwrap_or_else(|| node.clone());
            self.parent.insert(node.clone(), root.clone());
            node = next;
        }
        root
    }

    /// Union `a` and `b`; returns whether the two were previously in distinct sets.
    fn union(&mut self, a: &str, b: &str) -> bool {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == rb {
            return false;
        }
        // Deterministic: the lexicographically smaller root wins.
        let (root, child) = if ra <= rb { (ra, rb) } else { (rb, ra) };
        self.parent.insert(child, root);
        true
    }
}

fn ind_key(world: &str, individual: &str) -> String {
    format!("{world}\u{1f}{individual}")
}

// ── Term / literal helpers ───────────────────────────────────────────────────────

fn resource_key(term: &RdfTerm) -> Option<String> {
    match term {
        RdfTerm::Iri(iri) => Some(iri.clone()),
        RdfTerm::BlankNode(id) => Some(skolem_iri(id)),
        RdfTerm::Literal(_) | RdfTerm::Triple(_) => None,
    }
}

fn world_key(graph: &Option<RdfTerm>) -> String {
    match graph {
        Some(RdfTerm::Iri(iri)) => iri.clone(),
        Some(RdfTerm::BlankNode(id)) => skolem_iri(id),
        _ => crate::reason::rl::DEFAULT_WORLD.to_owned(),
    }
}

fn literal_usize(term: &RdfTerm) -> Option<u128> {
    match term {
        RdfTerm::Literal(l) => l.lexical_form.trim().parse::<u128>().ok(),
        _ => None,
    }
}

fn is_true_literal(l: &RdfLiteral) -> bool {
    matches!(l.lexical_form.trim(), "true" | "1")
        && l.datatype.as_deref().is_none_or(|d| d == XSD_BOOLEAN)
}

/// A canonical value key for a literal so value-equal literals from different
/// lexical spaces (the `xsd:decimal`/`xsd:integer`/`owl:rational` tower shares one
/// value space) collide under a single IFP value group. Two literals denote the
/// same value iff their keys are equal; a lexically-different but value-equal pair
/// therefore merges its IFP subjects (soundness for the collapse clash).
fn literal_value_key(l: &RdfLiteral) -> String {
    if let Some(lang) = &l.language {
        return format!("S\u{1f}{}\u{1f}{lang}", l.lexical_form);
    }
    let lexical = l.lexical_form.trim();
    match l.datatype.as_deref() {
        None | Some(XSD_STRING) | Some(RDF_LANG_STRING) => format!("S\u{1f}{}", l.lexical_form),
        Some(XSD_BOOLEAN) => match lexical {
            "true" | "1" => "B\u{1f}true".to_owned(),
            "false" | "0" => "B\u{1f}false".to_owned(),
            other => format!("Braw\u{1f}{other}"),
        },
        Some(XSD_FLOAT) => lexical
            .parse::<f32>()
            .map(|f| format!("F32\u{1f}{}", f.to_bits()))
            .unwrap_or_else(|_| format!("F32raw\u{1f}{lexical}")),
        Some(XSD_DOUBLE) => lexical
            .parse::<f64>()
            .map(|f| format!("F64\u{1f}{}", f.to_bits()))
            .unwrap_or_else(|_| format!("F64raw\u{1f}{lexical}")),
        Some(OWL_RATIONAL) => parse_rational(lexical)
            .map(|q| format!("Q\u{1f}{}", q.ratio_string()))
            .unwrap_or_else(|| format!("Qraw\u{1f}{lexical}")),
        Some(dt) if is_rational_tower(dt) => Rational::parse_decimal(lexical)
            .ok()
            .map(|q| format!("Q\u{1f}{}", q.ratio_string()))
            .unwrap_or_else(|| format!("Qraw\u{1f}{lexical}")),
        Some(dt) => format!("D\u{1f}{dt}\u{1f}{}", l.lexical_form),
    }
}

fn parse_rational(text: &str) -> Option<Rational> {
    if let Some((num, den)) = text.split_once('/') {
        let num: i128 = num.trim().parse().ok()?;
        let den: i128 = den.trim().parse().ok()?;
        Rational::new(num, den).ok()
    } else {
        Rational::parse_decimal(text).ok()
    }
}

fn is_rational_tower(dt: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use purrdf::{RdfDatasetBuilder, RdfLiteral, RdfQuad};

    const W: &str = "http://ex/w";
    const XSD_NNI: &str = "http://www.w3.org/2001/XMLSchema#nonNegativeInteger";

    fn quad(s: &str, p: &str, o: &str) -> RdfQuad {
        RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
    }
    fn typed_lit_quad(s: &str, p: &str, value: &str, dt: &str) -> RdfQuad {
        RdfQuad::new(
            RdfTerm::iri(s),
            p,
            RdfTerm::Literal(RdfLiteral::typed(value, dt)),
        )
        .in_graph(RdfTerm::iri(W))
    }

    fn dataset(quads: Vec<RdfQuad>) -> std::sync::Arc<RdfDataset> {
        let mut b = RdfDatasetBuilder::new();
        for q in quads {
            b.push_owned_quad(&q);
        }
        b.freeze().expect("freeze")
    }

    fn is_inconsistent(edb: &RdfDataset) -> bool {
        matches!(
            decide(edb),
            Some(RefutationCertificate::InFragment {
                decision: Decision::Inconsistent,
                ..
            })
        )
    }
    fn is_consistent(edb: &RdfDataset) -> bool {
        matches!(
            decide(edb),
            Some(RefutationCertificate::InFragment {
                decision: Decision::Consistent,
                ..
            })
        )
    }
    fn withholds(edb: &RdfDataset) -> bool {
        matches!(
            decide(edb),
            Some(RefutationCertificate::OutOfFragment { .. })
        )
    }

    #[test]
    fn empty_edb_does_not_engage() {
        let edb = RdfDatasetBuilder::new().freeze().unwrap();
        assert!(decide(edb.as_ref()).is_none());
    }

    #[test]
    fn consistent_min_max_cardinality_certifies() {
        // C ⊑ (min 1 p), C ⊑ (max 1 p) — pure cardinality, satisfiable.
        let edb = dataset(vec![
            quad("http://ex/C", RDF_TYPE, OWL_CLASS),
            quad("http://ex/p", RDF_TYPE, OWL_OBJECT_PROPERTY),
            quad("http://ex/C", RDFS_SUBCLASSOF, "http://ex/r1"),
            quad("http://ex/C", RDFS_SUBCLASSOF, "http://ex/r2"),
            quad("http://ex/r1", RDF_TYPE, OWL_RESTRICTION),
            typed_lit_quad("http://ex/r1", OWL_MIN_CARDINALITY, "1", XSD_NNI),
            quad("http://ex/r1", OWL_ON_PROPERTY, "http://ex/p"),
            quad("http://ex/r2", RDF_TYPE, OWL_RESTRICTION),
            typed_lit_quad("http://ex/r2", OWL_MAX_CARDINALITY, "1", XSD_NNI),
            quad("http://ex/r2", OWL_ON_PROPERTY, "http://ex/p"),
        ]);
        assert!(is_consistent(edb.as_ref()));
        assert!(decides_cardinality(edb.as_ref()));
    }

    #[test]
    fn collapsed_bound_on_populated_class_clashes() {
        // C ⊑ (min 2 p) ⊓ (max 1 p), i:C — unsatisfiable populated class.
        let edb = dataset(vec![
            quad("http://ex/C", RDF_TYPE, OWL_CLASS),
            quad("http://ex/i", RDF_TYPE, "http://ex/C"),
            quad("http://ex/C", RDFS_SUBCLASSOF, "http://ex/r1"),
            quad("http://ex/C", RDFS_SUBCLASSOF, "http://ex/r2"),
            quad("http://ex/r1", RDF_TYPE, OWL_RESTRICTION),
            typed_lit_quad("http://ex/r1", OWL_MIN_CARDINALITY, "2", XSD_NNI),
            quad("http://ex/r1", OWL_ON_PROPERTY, "http://ex/p"),
            quad("http://ex/r2", RDF_TYPE, OWL_RESTRICTION),
            typed_lit_quad("http://ex/r2", OWL_MAX_CARDINALITY, "1", XSD_NNI),
            quad("http://ex/r2", OWL_ON_PROPERTY, "http://ex/p"),
        ]);
        assert!(is_inconsistent(edb.as_ref()));
    }

    #[test]
    fn ifp_merge_without_distinctness_is_consistent() {
        // s1 p o, s2 p o, p IFP — s1 = s2, no differentFrom ⇒ consistent.
        let edb = dataset(vec![
            quad("http://ex/p", RDF_TYPE, OWL_INVERSE_FUNCTIONAL_PROPERTY),
            quad("http://ex/s1", "http://ex/p", "http://ex/o"),
            quad("http://ex/s2", "http://ex/p", "http://ex/o"),
        ]);
        assert!(is_consistent(edb.as_ref()));
        assert!(decides_identity(edb.as_ref()));
    }

    #[test]
    fn ifp_merge_with_differentfrom_clashes() {
        // s1 p o, s2 p o, p IFP, s1 differentFrom s2 ⇒ inconsistent (1 = 2 collapse).
        let edb = dataset(vec![
            quad("http://ex/p", RDF_TYPE, OWL_INVERSE_FUNCTIONAL_PROPERTY),
            quad("http://ex/s1", "http://ex/p", "http://ex/o"),
            quad("http://ex/s2", "http://ex/p", "http://ex/o"),
            quad("http://ex/s1", OWL_DIFFERENT_FROM, "http://ex/s2"),
        ]);
        assert!(is_inconsistent(edb.as_ref()));
    }

    #[test]
    fn ifp_literal_merge_matches_by_value() {
        // Two subjects sharing a literal value on an IFP merge (data-valued IFP).
        let edb = dataset(vec![
            quad("http://ex/p", RDF_TYPE, OWL_INVERSE_FUNCTIONAL_PROPERTY),
            typed_lit_quad("http://ex/s1", "http://ex/p", "123", XSD_STRING),
            typed_lit_quad("http://ex/s2", "http://ex/p", "123", XSD_STRING),
            quad("http://ex/s1", OWL_DIFFERENT_FROM, "http://ex/s2"),
        ]);
        assert!(is_inconsistent(edb.as_ref()));
    }

    #[test]
    fn ifp_with_class_construction_withholds() {
        // The same IFP but a subClassOf/disjoint construction present — outside the
        // pure assertional fragment ⇒ withhold rather than guess.
        let edb = dataset(vec![
            quad("http://ex/p", RDF_TYPE, OWL_INVERSE_FUNCTIONAL_PROPERTY),
            quad("http://ex/s1", "http://ex/p", "http://ex/o"),
            quad("http://ex/A", RDFS_SUBCLASSOF, "http://ex/B"),
        ]);
        assert!(withholds(edb.as_ref()));
        assert!(!decides_identity(edb.as_ref()));
    }

    #[test]
    fn has_self_disjoint_self_edge_clashes() {
        // R = ∃p.Self, C disjointWith R, x p x, x:C ⇒ x ∈ Nothing.
        let edb = dataset(vec![
            quad("http://ex/C", RDF_TYPE, OWL_CLASS),
            quad("http://ex/C", OWL_DISJOINT_WITH, "http://ex/R"),
            quad("http://ex/R", RDF_TYPE, OWL_RESTRICTION),
            typed_lit_quad("http://ex/R", OWL_HAS_SELF, "true", XSD_BOOLEAN),
            quad("http://ex/R", OWL_ON_PROPERTY, "http://ex/p"),
            quad("http://ex/p", RDF_TYPE, OWL_OBJECT_PROPERTY),
            quad("http://ex/x", "http://ex/p", "http://ex/x"),
            quad("http://ex/x", RDF_TYPE, "http://ex/C"),
        ]);
        assert!(is_inconsistent(edb.as_ref()));
        assert!(decides_has_self(edb.as_ref()));
    }

    #[test]
    fn determinism_byte_stable() {
        let edb = dataset(vec![
            quad("http://ex/p", RDF_TYPE, OWL_INVERSE_FUNCTIONAL_PROPERTY),
            quad("http://ex/s1", "http://ex/p", "http://ex/o"),
            quad("http://ex/s2", "http://ex/p", "http://ex/o"),
            quad("http://ex/s1", OWL_DIFFERENT_FROM, "http://ex/s2"),
        ]);
        let a = format!("{:?}", decide(edb.as_ref()));
        let b = format!("{:?}", decide(edb.as_ref()));
        assert_eq!(a, b);
    }
}
