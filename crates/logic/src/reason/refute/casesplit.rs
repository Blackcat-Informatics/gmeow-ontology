// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Families 1/3/6b (+ entangled Family 4) — the bounded case-split / complement /
//! union-disjoint / malformed-list refutation sub-decider.
//!
//! This is the THIRD real sub-decider registered in [`super::SUB_DECIDERS`] (after
//! [`super::datatype`] and [`super::counting`]). It decides — soundly, and
//! completely for a precisely-characterized propositional-plus-nominal fragment —
//! four families the native forward chase ([`crate::reason::dl`]) withholds:
//!
//! * **Family 1 — complement refutation.** An `owl:complementOf` class expression
//!   (possibly nested through `owl:intersectionOf`/`owl:unionOf`) forced onto an
//!   individual that also (directly or by case-split) inhabits the complemented
//!   class is a clash. Refutation-by-contradiction on a bounded completion.
//! * **Family 3 — union + disjoint refutation.** `owl:unionOf` membership is a
//!   DISJUNCTION driving a bounded case-split; a branch closes on a clash
//!   (complement, `owl:disjointWith`, `owl:disjointUnionOf` disjointness,
//!   `owl:Nothing`). The case is INCONSISTENT iff every branch closes, CONSISTENT
//!   iff a branch saturates clash-free AND the whole case lies in the
//!   certified-complete fragment.
//! * **Family 6b — malformed `rdf:List`.** `rdf:nil` bearing an `rdf:first` /
//!   `rdf:rest` edge is a structurally-broken list; the enclosing world is
//!   inconsistent.
//! * **Family 4 pickup (entangled).** `owl:oneOf` nominal enumerations drive a
//!   nominal-equality case-split (an individual typed to `{a₁ … aₖ}` is equal to
//!   one of them); merged individuals asserted `owl:differentFrom` (or co-listed in
//!   an `owl:AllDifferent`) clash. Composed with the disjunction case-split this
//!   decides the pure nominal-SAT divergence cases.
//!
//! # Soundness discipline (why `corpus_only` stays 0)
//!
//! An **`Inconsistent`** decision is sound whenever the case-split closes EVERY
//! branch with a sound clash under a bounded, exhaustive exploration: dropping the
//! constructs the tableau does not model only WEAKENS the theory (a superset of
//! models), so an unsatisfiable subset proves the whole case unsatisfiable. A
//! **`Consistent`** decision additionally requires that the whole case lies inside
//! the certified-complete fragment — no construct that could add a clash the
//! saturated open branch did not see. Any beyond-fragment construct (an
//! existential/cardinality/property-characteristic/`rdfs:domain`-`range`/…) blocks a
//! `Consistent` verdict; a bounded-exceeded search blocks any verdict — the decider
//! WITHHOLDS ([`super::RefutationCertificate::OutOfFragment`]) rather than guess.
//!
//! Every collection is `BTreeSet`/`BTreeMap`/sorted-`Vec` ordered so a certificate
//! is byte-stable (the native contract hash and reasoning goldens depend on it).

use std::collections::{BTreeMap, BTreeSet};

use purrdf::{RdfDataset, RdfTerm};

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
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";

const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_RESTRICTION: &str = "http://www.w3.org/2002/07/owl#Restriction";
const OWL_THING: &str = "http://www.w3.org/2002/07/owl#Thing";
const OWL_NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";
const OWL_ONTOLOGY: &str = "http://www.w3.org/2002/07/owl#Ontology";
const OWL_NAMED_INDIVIDUAL: &str = "http://www.w3.org/2002/07/owl#NamedIndividual";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const OWL_ANNOTATION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";

const OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";
const OWL_DISJOINT_UNION_OF: &str = "http://www.w3.org/2002/07/owl#disjointUnionOf";
const OWL_UNION_OF: &str = "http://www.w3.org/2002/07/owl#unionOf";
const OWL_INTERSECTION_OF: &str = "http://www.w3.org/2002/07/owl#intersectionOf";
const OWL_COMPLEMENT_OF: &str = "http://www.w3.org/2002/07/owl#complementOf";
const OWL_ONE_OF: &str = "http://www.w3.org/2002/07/owl#oneOf";
const OWL_SAME_AS: &str = "http://www.w3.org/2002/07/owl#sameAs";
const OWL_DIFFERENT_FROM: &str = "http://www.w3.org/2002/07/owl#differentFrom";
const OWL_ALL_DIFFERENT: &str = "http://www.w3.org/2002/07/owl#AllDifferent";
const OWL_DISTINCT_MEMBERS: &str = "http://www.w3.org/2002/07/owl#distinctMembers";
const OWL_MEMBERS: &str = "http://www.w3.org/2002/07/owl#members";

const RULE_CASESPLIT: &str = "refute:casesplit";
const RULE_MALFORMED_LIST: &str = "refute:casesplit-malformed-list";

/// The predicates whose PRESENCE (in a world) takes the world OUT of the certified
/// propositional-plus-nominal fragment for a `Consistent` verdict: existential /
/// cardinality / property-characteristic / datatype-facet / domain-range constructs
/// the case-split tableau does not fold into its completion. Their presence never
/// blocks an `Inconsistent` verdict (a subset refutation stays sound); it only
/// forbids certifying `Consistent`.
const CONSISTENT_BLOCKING_PREDICATES: &[&str] = &[
    "http://www.w3.org/2002/07/owl#onProperty",
    "http://www.w3.org/2002/07/owl#onClass",
    "http://www.w3.org/2002/07/owl#onDataRange",
    "http://www.w3.org/2002/07/owl#onDatatype",
    "http://www.w3.org/2002/07/owl#someValuesFrom",
    "http://www.w3.org/2002/07/owl#allValuesFrom",
    "http://www.w3.org/2002/07/owl#hasValue",
    "http://www.w3.org/2002/07/owl#hasSelf",
    "http://www.w3.org/2002/07/owl#cardinality",
    "http://www.w3.org/2002/07/owl#minCardinality",
    "http://www.w3.org/2002/07/owl#maxCardinality",
    "http://www.w3.org/2002/07/owl#qualifiedCardinality",
    "http://www.w3.org/2002/07/owl#minQualifiedCardinality",
    "http://www.w3.org/2002/07/owl#maxQualifiedCardinality",
    "http://www.w3.org/2002/07/owl#propertyChainAxiom",
    "http://www.w3.org/2002/07/owl#inverseOf",
    "http://www.w3.org/2002/07/owl#hasKey",
    "http://www.w3.org/2002/07/owl#equivalentProperty",
    "http://www.w3.org/2002/07/owl#propertyDisjointWith",
    "http://www.w3.org/2002/07/owl#equivalentClass",
    "http://www.w3.org/2002/07/owl#withRestrictions",
    "http://www.w3.org/2002/07/owl#datatypeComplementOf",
    RDFS_DOMAIN,
    RDFS_RANGE,
];

/// The `rdf:type` OBJECTS whose presence blocks a `Consistent` verdict — property
/// characteristics / restriction nodes / list-clash carriers the tableau does not
/// model. (`owl:AllDifferent` is deliberately NOT here — it IS handled, expanded
/// into pairwise distinctness.)
const CONSISTENT_BLOCKING_TYPE_OBJECTS: &[&str] = &[
    OWL_RESTRICTION,
    "http://www.w3.org/2002/07/owl#FunctionalProperty",
    "http://www.w3.org/2002/07/owl#InverseFunctionalProperty",
    "http://www.w3.org/2002/07/owl#TransitiveProperty",
    "http://www.w3.org/2002/07/owl#SymmetricProperty",
    "http://www.w3.org/2002/07/owl#AsymmetricProperty",
    "http://www.w3.org/2002/07/owl#IrreflexiveProperty",
    "http://www.w3.org/2002/07/owl#ReflexiveProperty",
    "http://www.w3.org/2002/07/owl#AllDisjointProperties",
    "http://www.w3.org/2002/07/owl#AllDisjointClasses",
    "http://www.w3.org/2002/07/owl#NegativePropertyAssertion",
];

/// The `rdf:type` objects that are DECLARATIONS, never class membership: they never
/// contribute a concept label to their subject.
const DECLARATION_TYPE_OBJECTS: &[&str] = &[
    OWL_CLASS,
    OWL_RESTRICTION,
    OWL_THING,
    OWL_ONTOLOGY,
    OWL_NAMED_INDIVIDUAL,
    OWL_OBJECT_PROPERTY,
    OWL_DATATYPE_PROPERTY,
    OWL_ANNOTATION_PROPERTY,
    "http://www.w3.org/2002/07/owl#FunctionalProperty",
    "http://www.w3.org/2002/07/owl#InverseFunctionalProperty",
    "http://www.w3.org/2002/07/owl#TransitiveProperty",
    "http://www.w3.org/2002/07/owl#SymmetricProperty",
    "http://www.w3.org/2002/07/owl#AsymmetricProperty",
    "http://www.w3.org/2002/07/owl#IrreflexiveProperty",
    "http://www.w3.org/2002/07/owl#ReflexiveProperty",
    OWL_ALL_DIFFERENT,
    "http://www.w3.org/2002/07/owl#AllDisjointProperties",
    "http://www.w3.org/2002/07/owl#AllDisjointClasses",
    "http://www.w3.org/2002/07/owl#NegativePropertyAssertion",
];

/// The search budget (deterministic node-expansions + branches). A search that
/// exceeds it WITHHOLDS (`OutOfFragment`) rather than truncating to a wrong answer.
/// Kept modest: the decider runs on every production reasoning closure (both in the
/// refutation kernel and the coverage gate), so a pathological blow-up must bail to
/// an honest boundary quickly rather than stall the chase. The certified-complete
/// propositional/disjoint-union cases (e.g. the 9-variable `503`/`504` SAT pair)
/// close far inside this bound.
const SEARCH_BUDGET: u64 = 400_000;

/// The maximum datatype-node resolution recursion depth (a cyclic class expression
/// bottoms out into an opaque atom rather than looping).
const RESOLVE_DEPTH: u32 = 64;

// ── The registered sub-decider entrypoint ───────────────────────────────────────

/// The [`super::SubDecider`] for the case-split / complement / union-disjoint /
/// malformed-list family.
///
/// Returns `None` when no case-split shape is present (the family does not engage);
/// otherwise the bounded case-split over each world's completion. A proven clash in
/// ANY world is decisive (`Inconsistent`, materializing `owl:Nothing`); a
/// `Consistent` verdict is licensed only when EVERY world saturated clash-free
/// inside the certified-complete fragment. A bound-exceeded / out-of-fragment world
/// (with no decisive clash) refuses the case into an `OutOfFragment` withhold.
pub(crate) fn decide(edb: &RdfDataset) -> Option<RefutationCertificate> {
    let scan = Scan::of(edb);
    if !scan.engages() {
        return None;
    }

    let mut clashes: BTreeSet<NothingClash> = BTreeSet::new();
    let mut counted: BTreeSet<String> = BTreeSet::new();
    let mut obstructions: BTreeSet<String> = BTreeSet::new();

    for world in scan.worlds.keys() {
        match scan.run_world(world) {
            WorldOutcome::Inconsistent(cs) => {
                for c in cs {
                    counted.insert(c.individual.clone());
                    clashes.insert(c);
                }
            }
            WorldOutcome::Consistent => {}
            WorldOutcome::OutOfFragment(reason) => {
                obstructions.insert(reason);
            }
        }
    }

    // A proven clash is decisive: the ontology IS inconsistent regardless of any
    // obstruction a sibling world raised, so it is sound to decide `Inconsistent`.
    if !clashes.is_empty() {
        return Some(certify_membership(
            FragmentFamily::CaseSplit,
            BTreeSet::new(),
            move || {
                (
                    Decision::Inconsistent,
                    Witness {
                        family: FragmentFamily::CaseSplit,
                        clashes,
                        evidence: WitnessEvidence {
                            counted_individuals: counted,
                            violated_bound: None,
                            closed_branch: Some("all-branches-closed".to_owned()),
                        },
                    },
                )
            },
        ));
    }

    // No clash anywhere — a `Consistent` verdict requires EVERY world to have
    // saturated clash-free inside the certified-complete fragment.
    Some(certify_membership(
        FragmentFamily::CaseSplit,
        obstructions,
        || {
            (
                Decision::Consistent,
                Witness {
                    family: FragmentFamily::CaseSplit,
                    clashes: BTreeSet::new(),
                    evidence: WitnessEvidence::default(),
                },
            )
        },
    ))
}

/// True iff the case-split sub-decider DECIDES `edb` (an in-fragment `Consistent`
/// or `Inconsistent`). The coverage coordinator ([`crate::reason::dl`]) consults
/// this to keep the case-split families (complement / union / oneOf / malformed
/// list) `decided` — narrowing their refutation-shape withholds — exactly when the
/// decider has completely decided them.
pub(crate) fn decides(edb: &RdfDataset) -> bool {
    matches!(decide(edb), Some(RefutationCertificate::InFragment { .. }))
}

// ── Concepts ─────────────────────────────────────────────────────────────────────

/// A class expression in negation-normal form. `Neg` wraps only an atom (a named
/// class or an opaque expression node); every other negation is pushed inward.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Concept {
    Top,
    Bottom,
    /// Membership in a named class / opaque node.
    Pos(String),
    /// Non-membership in a named class / opaque node.
    Neg(String),
    /// Conjunction (`owl:intersectionOf`).
    And(Vec<Concept>),
    /// Disjunction (`owl:unionOf`, `owl:disjointUnionOf` cover).
    Or(Vec<Concept>),
    /// Nominal enumeration (`owl:oneOf` over individuals): the individual is EQUAL
    /// to one of these.
    Nominals(Vec<String>),
    /// A construct the fragment cannot model soundly (e.g. the negation of a
    /// nominal set). It is a no-op label (dropping it only WEAKENS the theory, sound
    /// for refutation) but its presence blocks a `Consistent` verdict.
    Blocked,
}

/// The negation of a concept, pushed to NNF.
fn negate(c: Concept) -> Concept {
    match c {
        Concept::Top => Concept::Bottom,
        Concept::Bottom => Concept::Top,
        Concept::Pos(s) => Concept::Neg(s),
        Concept::Neg(s) => Concept::Pos(s),
        Concept::And(cs) => Concept::Or(cs.into_iter().map(negate).collect()),
        Concept::Or(cs) => Concept::And(cs.into_iter().map(negate).collect()),
        // The complement of a nominal set is not a sound fragment concept.
        Concept::Nominals(_) | Concept::Blocked => Concept::Blocked,
    }
}

fn concept_contains_blocked(c: &Concept) -> bool {
    match c {
        Concept::Blocked => true,
        Concept::And(cs) | Concept::Or(cs) => cs.iter().any(concept_contains_blocked),
        _ => false,
    }
}

// ── EDB scan ─────────────────────────────────────────────────────────────────────

/// One world's parsed data.
#[derive(Default)]
struct WorldData {
    /// individual → asserted class-membership nodes.
    types: BTreeMap<String, BTreeSet<String>>,
    /// class → `rdfs:subClassOf` / `owl:equivalentClass` superclass targets.
    subclass_of: BTreeMap<String, BTreeSet<String>>,
    /// symmetric `owl:equivalentClass` named-named pairs (for the reverse edge).
    equivalent_named: Vec<(String, String)>,
    /// `owl:disjointWith` pairs `(a, b)`.
    disjoint_with: Vec<(String, String)>,
    /// `owl:disjointUnionOf` `(class, list head)`.
    disjoint_union_of: Vec<(String, String)>,
    /// class-expression node definitions.
    complement_of: BTreeMap<String, String>,
    intersection_of: BTreeMap<String, String>,
    union_of: BTreeMap<String, String>,
    one_of: BTreeMap<String, String>,
    /// list head → ordered members.
    lists: BTreeMap<String, Vec<RdfTerm>>,
    /// `owl:sameAs` / `owl:differentFrom` pairs.
    same_as: Vec<(String, String)>,
    different_from: Vec<(String, String)>,
    /// `owl:AllDifferent` distinct-member list heads.
    all_different_heads: Vec<String>,
    /// every predicate present (for the consistent-fragment gate).
    predicates: BTreeSet<String>,
    /// every `rdf:type` object present (for the consistent-fragment gate).
    type_objects: BTreeSet<String>,
    /// `rdf:nil` bears an `rdf:first`/`rdf:rest` edge — a malformed list.
    malformed_list: bool,
    /// the offending `rdf:nil` edges, cited on the malformed-list clash.
    malformed_edges: BTreeSet<(String, String, String)>,
}

struct Scan {
    worlds: BTreeMap<String, WorldData>,
}

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

impl Scan {
    fn of(edb: &RdfDataset) -> Self {
        let mut worlds: BTreeMap<String, WorldData> = BTreeMap::new();
        // raw first/rest edges per world for the list walk.
        let mut firsts: BTreeMap<String, BTreeMap<String, RdfTerm>> = BTreeMap::new();
        let mut rests: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();

        for quad in edb.owned_quads() {
            let world = world_key(&quad.graph_name);
            let predicate = quad.predicate.clone();
            let Some(subject) = resource_key(&quad.subject) else {
                continue;
            };
            let w = worlds.entry(world.clone()).or_default();
            w.predicates.insert(predicate.clone());

            match predicate.as_str() {
                RDF_TYPE => {
                    if let Some(object) = resource_key(&quad.object) {
                        w.type_objects.insert(object.clone());
                        if !DECLARATION_TYPE_OBJECTS.contains(&object.as_str()) {
                            w.types.entry(subject).or_default().insert(object);
                        }
                    }
                }
                RDFS_SUBCLASSOF => {
                    if let Some(object) = resource_key(&quad.object) {
                        w.subclass_of.entry(subject).or_default().insert(object);
                    }
                }
                OWL_EQUIVALENT_CLASS => {
                    if let Some(object) = resource_key(&quad.object) {
                        w.subclass_of
                            .entry(subject.clone())
                            .or_default()
                            .insert(object.clone());
                        w.equivalent_named.push((subject, object));
                    }
                }
                OWL_DISJOINT_WITH => {
                    if let Some(object) = resource_key(&quad.object) {
                        w.disjoint_with.push((subject, object));
                    }
                }
                OWL_DISJOINT_UNION_OF => {
                    if let Some(object) = resource_key(&quad.object) {
                        w.disjoint_union_of.push((subject, object));
                    }
                }
                OWL_COMPLEMENT_OF => {
                    if let Some(object) = resource_key(&quad.object) {
                        w.complement_of.insert(subject, object);
                    }
                }
                OWL_INTERSECTION_OF => {
                    if let Some(object) = resource_key(&quad.object) {
                        w.intersection_of.insert(subject, object);
                    }
                }
                OWL_UNION_OF => {
                    if let Some(object) = resource_key(&quad.object) {
                        w.union_of.insert(subject, object);
                    }
                }
                OWL_ONE_OF => {
                    if let Some(object) = resource_key(&quad.object) {
                        w.one_of.insert(subject, object);
                    }
                }
                OWL_SAME_AS => {
                    if let Some(object) = resource_key(&quad.object) {
                        w.same_as.push((subject, object));
                    }
                }
                OWL_DIFFERENT_FROM => {
                    if let Some(object) = resource_key(&quad.object) {
                        w.different_from.push((subject, object));
                    }
                }
                OWL_DISTINCT_MEMBERS | OWL_MEMBERS => {
                    if let Some(object) = resource_key(&quad.object) {
                        w.all_different_heads.push(object);
                    }
                }
                RDF_FIRST => {
                    if subject == RDF_NIL {
                        w.malformed_list = true;
                        if let Some(object) = resource_key(&quad.object) {
                            w.malformed_edges.insert((
                                RDF_NIL.to_owned(),
                                RDF_FIRST.to_owned(),
                                object,
                            ));
                        } else {
                            w.malformed_edges.insert((
                                RDF_NIL.to_owned(),
                                RDF_FIRST.to_owned(),
                                "(literal)".to_owned(),
                            ));
                        }
                    }
                    firsts
                        .entry(world.clone())
                        .or_default()
                        .insert(subject.clone(), quad.object.clone());
                }
                RDF_REST => {
                    if subject == RDF_NIL {
                        w.malformed_list = true;
                        if let Some(object) = resource_key(&quad.object) {
                            w.malformed_edges.insert((
                                RDF_NIL.to_owned(),
                                RDF_REST.to_owned(),
                                object,
                            ));
                        }
                    }
                    if let Some(object) = resource_key(&quad.object) {
                        rests
                            .entry(world.clone())
                            .or_default()
                            .insert(subject.clone(), object);
                    }
                }
                RDFS_LABEL | RDFS_COMMENT | RDFS_SEE_ALSO | RDFS_IS_DEFINED_BY => {}
                _ => {}
            }
        }

        // Walk resource-and-literal member lists to nil per world.
        for (world, w) in &mut worlds {
            let first = firsts.remove(world).unwrap_or_default();
            let rest = rests.remove(world).unwrap_or_default();
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
                w.lists.insert(head, members);
            }
        }

        Scan { worlds }
    }

    /// True iff any case-split shape is present in any world (the engage set):
    /// `owl:complementOf` / `owl:unionOf` / `owl:oneOf` / `owl:disjointUnionOf`, or
    /// a malformed `rdf:List`. Pure `owl:intersectionOf` / `owl:disjointWith` (no
    /// disjunction) is left to the native EL/DL chase.
    fn engages(&self) -> bool {
        self.worlds.values().any(|w| {
            w.malformed_list
                || !w.complement_of.is_empty()
                || !w.union_of.is_empty()
                || !w.one_of.is_empty()
                || !w.disjoint_union_of.is_empty()
        })
    }

    fn run_world(&self, world: &str) -> WorldOutcome {
        let w = &self.worlds[world];

        // Family 6b — a malformed `rdf:List` makes the world inconsistent outright.
        if w.malformed_list {
            let mut premises: Vec<(String, String, String)> =
                w.malformed_edges.iter().cloned().collect();
            premises.sort();
            premises.dedup();
            let clash = NothingClash {
                individual: RDF_NIL.to_owned(),
                world: world.to_owned(),
                rule_name: RULE_MALFORMED_LIST.to_owned(),
                premises,
            };
            return WorldOutcome::Inconsistent([clash].into_iter().collect());
        }

        let ctx = Ctx::build(w);
        let resolver = Resolver { w };

        // Initial tableau: assert every individual's membership concepts, then the
        // sameAs merges and differentFrom constraints.
        let mut state = State::default();
        let mut all_individuals: BTreeSet<String> = BTreeSet::new();
        for (ind, classes) in &w.types {
            all_individuals.insert(ind.clone());
            let _ = classes;
        }
        for (a, b) in &w.same_as {
            all_individuals.insert(a.clone());
            all_individuals.insert(b.clone());
        }
        for (a, b) in &ctx.different {
            all_individuals.insert(a.clone());
            all_individuals.insert(b.clone());
        }
        for ind in &all_individuals {
            state.make(ind);
        }
        for (a, b) in &w.same_as {
            state.union(a, b);
        }

        let mut budget: u64 = SEARCH_BUDGET;
        // Seed the individual membership concepts.
        for (ind, classes) in &w.types {
            for class in classes {
                let concept = resolver.resolve(class, 0);
                if state.add(ind, concept, &mut budget).is_err() {
                    // An immediate clash on seeding: the world is inconsistent.
                    return WorldOutcome::Inconsistent(ctx.clashes(w, world));
                }
            }
        }

        match search(&mut state, &ctx, &mut budget) {
            SearchResult::Unsat => WorldOutcome::Inconsistent(ctx.clashes(w, world)),
            SearchResult::Bound => WorldOutcome::OutOfFragment(format!(
                "case-split search budget exceeded in world <{world}>"
            )),
            SearchResult::Sat => {
                if let Some(reason) = ctx.consistent_block_reason(w) {
                    WorldOutcome::OutOfFragment(reason)
                } else {
                    WorldOutcome::Consistent
                }
            }
        }
    }
}

enum WorldOutcome {
    Inconsistent(BTreeSet<NothingClash>),
    Consistent,
    OutOfFragment(String),
}

// ── The reasoning context (immutable per world) ─────────────────────────────────

struct Ctx {
    /// named class → told-subsumer concepts (added when `Pos(C)` is present).
    subsumers: BTreeMap<String, Vec<Concept>>,
    /// distinctness pairs (original individuals).
    different: Vec<(String, String)>,
    /// whether the fragment saw a construct that blocks a `Consistent` verdict.
    consistent_blocked: bool,
    /// the concrete reason the world blocks a `Consistent` verdict, if any.
    block_reason: Option<String>,
}

impl Ctx {
    fn build(w: &WorldData) -> Self {
        let mut subsumers: BTreeMap<String, Vec<Concept>> = BTreeMap::new();
        let resolver = Resolver { w };
        let mut consistent_blocked = false;
        let mut block_reason: Option<String> = None;

        let note_block = |reason: String, blocked: &mut bool, slot: &mut Option<String>| {
            *blocked = true;
            if slot.is_none() {
                *slot = Some(reason);
            }
        };

        // rdfs:subClassOf / owl:equivalentClass (forward) told-subsumers.
        for (class, supers) in &w.subclass_of {
            for target in supers {
                let concept = resolver.resolve(target, 0);
                if concept_contains_blocked(&concept) {
                    note_block(
                        format!("negated nominal expression in <{target}>"),
                        &mut consistent_blocked,
                        &mut block_reason,
                    );
                }
                subsumers.entry(class.clone()).or_default().push(concept);
            }
        }
        // owl:equivalentClass reverse edge for named-named pairs.
        for (a, b) in &w.equivalent_named {
            let a_is_expr = resolver.is_expression(a);
            let b_is_expr = resolver.is_expression(b);
            if a_is_expr || b_is_expr {
                note_block(
                    "owl:equivalentClass to a class expression".to_owned(),
                    &mut consistent_blocked,
                    &mut block_reason,
                );
            } else {
                subsumers
                    .entry(b.clone())
                    .or_default()
                    .push(Concept::Pos(a.clone()));
            }
        }
        // owl:disjointWith: C ⊑ ¬D and D ⊑ ¬C.
        for (a, b) in &w.disjoint_with {
            subsumers
                .entry(a.clone())
                .or_default()
                .push(negate(resolver.resolve(b, 0)));
            subsumers
                .entry(b.clone())
                .or_default()
                .push(negate(resolver.resolve(a, 0)));
        }
        // owl:disjointUnionOf(C; D₁ … Dₙ): C ⊑ (D₁ ⊔ … ⊔ Dₙ); Dᵢ ⊑ C;
        // Dᵢ ⊑ ¬Dⱼ (i ≠ j).
        for (class, head) in &w.disjoint_union_of {
            let members = resolver.list_resources(head);
            let disjuncts: Vec<Concept> = members.iter().map(|m| Concept::Pos(m.clone())).collect();
            subsumers
                .entry(class.clone())
                .or_default()
                .push(Concept::Or(disjuncts));
            for (i, di) in members.iter().enumerate() {
                subsumers
                    .entry(di.clone())
                    .or_default()
                    .push(Concept::Pos(class.clone()));
                for (j, dj) in members.iter().enumerate() {
                    if i != j {
                        subsumers
                            .entry(di.clone())
                            .or_default()
                            .push(Concept::Neg(dj.clone()));
                    }
                }
            }
        }

        // Distinctness: explicit owl:differentFrom + owl:AllDifferent expansions.
        let mut different: Vec<(String, String)> = w.different_from.clone();
        for head in &w.all_different_heads {
            let members = resolver.list_resources(head);
            for i in 0..members.len() {
                for j in (i + 1)..members.len() {
                    different.push((members[i].clone(), members[j].clone()));
                }
            }
        }

        let mut ctx = Ctx {
            subsumers,
            different,
            consistent_blocked,
            block_reason,
        };

        // Fold the standalone consistent-fragment gate (denylisted predicates /
        // type objects / literal-nominal enumerations).
        if let Some(reason) = fragment_block_reason(w) {
            ctx.consistent_blocked = true;
            if ctx.block_reason.is_none() {
                ctx.block_reason = Some(reason);
            }
        }
        ctx
    }

    /// The reason (if any) this world cannot be certified `Consistent`.
    fn consistent_block_reason(&self, w: &WorldData) -> Option<String> {
        let _ = w;
        if self.consistent_blocked {
            Some(
                self.block_reason
                    .clone()
                    .unwrap_or_else(|| "case outside the certified-complete fragment".to_owned()),
            )
        } else {
            None
        }
    }

    /// The `owl:Nothing` clashes materialized for an inconsistent world: every
    /// individual with an asserted membership is forced empty (ex falso — a sound
    /// consequence of a genuinely inconsistent world).
    fn clashes(&self, w: &WorldData, world: &str) -> BTreeSet<NothingClash> {
        let mut clashes: BTreeSet<NothingClash> = BTreeSet::new();
        for (ind, classes) in &w.types {
            let mut premises: Vec<(String, String, String)> = classes
                .iter()
                .map(|c| (ind.clone(), RDF_TYPE.to_owned(), c.clone()))
                .collect();
            premises.sort();
            premises.dedup();
            clashes.insert(NothingClash {
                individual: ind.clone(),
                world: world.to_owned(),
                rule_name: RULE_CASESPLIT.to_owned(),
                premises,
            });
        }
        // A world can be inconsistent with no typed individual only via a malformed
        // list (handled earlier). Defensive: if empty, cite rdf:nil so the verdict
        // still reads an inconsistency witness.
        if clashes.is_empty() {
            clashes.insert(NothingClash {
                individual: RDF_NIL.to_owned(),
                world: world.to_owned(),
                rule_name: RULE_CASESPLIT.to_owned(),
                premises: Vec::new(),
            });
        }
        clashes
    }
}

/// The standalone consistent-fragment gate: the denylisted predicates / type
/// objects / literal-nominal enumerations whose presence forbids a `Consistent`
/// verdict.
fn fragment_block_reason(w: &WorldData) -> Option<String> {
    for predicate in &w.predicates {
        if CONSISTENT_BLOCKING_PREDICATES.contains(&predicate.as_str()) {
            return Some(format!(
                "beyond-fragment construct <{predicate}> present — cannot certify consistent"
            ));
        }
    }
    for object in &w.type_objects {
        if CONSISTENT_BLOCKING_TYPE_OBJECTS.contains(&object.as_str()) {
            return Some(format!(
                "beyond-fragment characteristic <{object}> present — cannot certify consistent"
            ));
        }
    }
    // Any `owl:oneOf` enumeration blocks a `Consistent` verdict. The nominal
    // case-split soundly branches an individual's `oneOf` MEMBERSHIP into equality
    // alternatives (which suffices to CLOSE branches for an `Inconsistent` proof —
    // sound subset reasoning), but it does NOT model the full nominal-enumeration
    // TBox semantics a `Consistent` model requires: an enumeration's closed-world
    // upper bound, and — decisively — SET-EQUALITY across the multiple `owl:oneOf`
    // definitions of one class (`C oneOf E₁`, `C oneOf E₂` ⇒ E₁ = E₂ as sets),
    // which drives the nominal-SAT divergence cases. Rather than risk certifying a
    // false `Consistent` on that unmodeled structure, any `owl:oneOf` presence is an
    // honest boundary for the consistent side; a genuine nominal clash is still
    // decided `Inconsistent`.
    if !w.one_of.is_empty() {
        return Some(
            "owl:oneOf nominal enumeration present — the full nominal-enumeration TBox \
             (closed-world upper bound + cross-enumeration set-equality) is outside the \
             case-split consistent fragment"
                .to_owned(),
        );
    }
    None
}

/// A class-expression resolver over one world's definition maps.
struct Resolver<'a> {
    w: &'a WorldData,
}

impl Resolver<'_> {
    /// True iff `node` names a class EXPRESSION (a definition node), not a plain
    /// named class / atom.
    fn is_expression(&self, node: &str) -> bool {
        self.w.complement_of.contains_key(node)
            || self.w.intersection_of.contains_key(node)
            || self.w.union_of.contains_key(node)
            || self.w.one_of.contains_key(node)
    }

    fn list_resources(&self, head: &str) -> Vec<String> {
        self.w
            .lists
            .get(head)
            .map(|members| members.iter().filter_map(resource_key).collect())
            .unwrap_or_default()
    }

    fn resolve(&self, node: &str, depth: u32) -> Concept {
        if depth >= RESOLVE_DEPTH {
            return Concept::Pos(node.to_owned());
        }
        if node == OWL_THING {
            return Concept::Top;
        }
        if node == OWL_NOTHING {
            return Concept::Bottom;
        }
        if let Some(inner) = self.w.complement_of.get(node) {
            return negate(self.resolve(inner, depth + 1));
        }
        if let Some(head) = self.w.intersection_of.get(node) {
            let members: Vec<Concept> = self
                .list_resources(head)
                .iter()
                .map(|m| self.resolve(m, depth + 1))
                .collect();
            return Concept::And(members);
        }
        if let Some(head) = self.w.union_of.get(node) {
            let members: Vec<Concept> = self
                .list_resources(head)
                .iter()
                .map(|m| self.resolve(m, depth + 1))
                .collect();
            return Concept::Or(members);
        }
        if let Some(head) = self.w.one_of.get(node) {
            // A literal-bearing enumeration is a datatype enumeration outside the
            // nominal fragment: an opaque atom (blocks `Consistent` via the
            // standalone gate; sound to treat opaquely for refutation).
            let raw = self.w.lists.get(head).cloned().unwrap_or_default();
            if raw.iter().any(|m| matches!(m, RdfTerm::Literal(_))) {
                return Concept::Pos(node.to_owned());
            }
            let nominals: Vec<String> = raw.iter().filter_map(resource_key).collect();
            return Concept::Nominals(nominals);
        }
        Concept::Pos(node.to_owned())
    }
}

// ── The bounded tableau ──────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct State {
    parent: BTreeMap<String, String>,
    labels: BTreeMap<String, BTreeSet<Concept>>,
}

/// A branch action tried against a fresh clone of the state.
#[derive(Clone)]
enum Action {
    /// Add a disjunct concept to a root (`owl:unionOf` branch).
    Add(String, Concept),
    /// Merge a root with a nominal individual (`owl:oneOf` branch).
    Merge(String, String),
}

/// The result of saturating a state to a deterministic fixpoint.
enum Saturation {
    /// The branch closed on a clash.
    Closed,
    /// The branch saturated clash-free with no pending nondeterministic choice.
    Open,
    /// The branch reached a fixpoint with a pending nondeterministic choice.
    Branch(Vec<Action>),
}

enum SearchResult {
    Sat,
    Unsat,
    Bound,
}

impl State {
    fn make(&mut self, x: &str) {
        self.parent
            .entry(x.to_owned())
            .or_insert_with(|| x.to_owned());
        self.labels.entry(x.to_owned()).or_default();
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

    /// Merge `a` and `b`, folding the loser's labels into the winner. Returns
    /// whether the two were previously distinct.
    fn union(&mut self, a: &str, b: &str) -> bool {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == rb {
            return false;
        }
        let (root, child) = if ra <= rb { (ra, rb) } else { (rb, ra) };
        let moved = self.labels.remove(&child).unwrap_or_default();
        self.parent.insert(child, root.clone());
        let entry = self.labels.entry(root).or_default();
        for c in moved {
            entry.insert(c);
        }
        true
    }

    /// Add `concept` to `individual`'s root label set, decomposing conjunctions and
    /// detecting an immediate `Bottom`/`Pos∧Neg` clash. Returns `Err(())` on a
    /// clash. `changed` is set through the budget-charged path.
    fn add(&mut self, individual: &str, concept: Concept, budget: &mut u64) -> Result<bool, ()> {
        *budget = budget.saturating_sub(1);
        if *budget == 0 {
            // Out of budget mid-add — treat as no change; the search will surface
            // the bound. (A false "no change" cannot cause an unsound decision: the
            // caller re-checks the budget.)
            return Ok(false);
        }
        let root = self.find(individual);
        match concept {
            Concept::Top => Ok(false),
            Concept::Bottom => Err(()),
            Concept::And(cs) => {
                let mut changed = false;
                for c in cs {
                    changed |= self.add(&root, c, budget)?;
                }
                Ok(changed)
            }
            Concept::Blocked => {
                // A no-op label: dropping it only weakens the theory (sound for
                // refutation). It never contributes a clash.
                Ok(false)
            }
            other => {
                // Clash check for atoms.
                if let Concept::Pos(ref a) = other
                    && self.labels_of(&root).contains(&Concept::Neg(a.clone()))
                {
                    return Err(());
                }
                if let Concept::Neg(ref a) = other
                    && self.labels_of(&root).contains(&Concept::Pos(a.clone()))
                {
                    return Err(());
                }
                let entry = self.labels.entry(root).or_default();
                Ok(entry.insert(other))
            }
        }
    }

    fn labels_of(&mut self, individual: &str) -> BTreeSet<Concept> {
        let root = self.find(individual);
        self.labels.get(&root).cloned().unwrap_or_default()
    }
}

/// Whether a disjunct concept is already SATISFIED by a root's labels.
fn disjunct_satisfied(labels: &BTreeSet<Concept>, disjunct: &Concept) -> bool {
    match disjunct {
        Concept::Top => true,
        _ => labels.contains(disjunct),
    }
}

/// Whether a disjunct concept is REFUTED by a root's labels (its negation present),
/// so it can be soundly pruned from a disjunction.
fn disjunct_refuted(labels: &BTreeSet<Concept>, disjunct: &Concept) -> bool {
    match disjunct {
        Concept::Bottom => true,
        Concept::Pos(a) => labels.contains(&Concept::Neg(a.clone())),
        Concept::Neg(a) => labels.contains(&Concept::Pos(a.clone())),
        _ => false,
    }
}

/// The distinct current union-find roots of a state.
fn distinct_roots(state: &mut State) -> Vec<String> {
    let keys: Vec<String> = state.parent.keys().cloned().collect();
    let mut rs: BTreeSet<String> = BTreeSet::new();
    for k in keys {
        rs.insert(state.find(&k));
    }
    rs.into_iter().collect()
}

/// Saturate `state` under the deterministic rules to a fixpoint; return whether it
/// closed, saturated open, or reached a nondeterministic branch point.
///
/// A DIRTY worklist keeps saturation near-linear on the propositional fragment: a
/// root is (re)processed only when a concept is added to it. A merge (a nominal /
/// `owl:sameAs` equality — absent from the certified-consistent fragment) is rare
/// and re-seeds the whole worklist, keeping completeness for the `Inconsistent`
/// refutation while never quadratically re-scanning the common no-merge case.
fn saturate(state: &mut State, ctx: &Ctx, budget: &mut u64) -> Saturation {
    let mut dirty: BTreeSet<String> = distinct_roots(state).into_iter().collect();

    while let Some(first) = dirty.iter().next().cloned() {
        dirty.remove(&first);
        if *budget == 0 {
            return Saturation::Branch(Vec::new());
        }
        let mut root = state.find(&first);
        // Process this root to a LOCAL fixpoint.
        loop {
            if *budget == 0 {
                return Saturation::Branch(Vec::new());
            }
            let mut local_changed = false;
            let mut merged = false;
            let current = state.labels_of(&root);
            for concept in &current {
                match concept {
                    Concept::Pos(name) => {
                        if let Some(subs) = ctx.subsumers.get(name) {
                            for sub in subs {
                                match state.add(&root, sub.clone(), budget) {
                                    Ok(c) => local_changed |= c,
                                    Err(()) => return Saturation::Closed,
                                }
                            }
                        }
                    }
                    Concept::And(cs) => {
                        for c in cs {
                            match state.add(&root, c.clone(), budget) {
                                Ok(c) => local_changed |= c,
                                Err(()) => return Saturation::Closed,
                            }
                        }
                    }
                    Concept::Nominals(members) => {
                        let root_of = state.find(&root);
                        if members.iter().any(|m| state.find(m) == root_of) {
                            continue;
                        }
                        if members.is_empty() {
                            return Saturation::Closed;
                        }
                        if members.len() == 1 && state.union(&root_of, &members[0]) {
                            merged = true;
                            break;
                        }
                        // else: a pending nominal branch (handled by `pick_branch`).
                    }
                    Concept::Or(disjuncts) => {
                        let labels = state.labels_of(&root);
                        if disjuncts.iter().any(|d| disjunct_satisfied(&labels, d)) {
                            continue;
                        }
                        let live: Vec<Concept> = disjuncts
                            .iter()
                            .filter(|d| !disjunct_refuted(&labels, d))
                            .cloned()
                            .collect();
                        if live.is_empty() {
                            return Saturation::Closed;
                        }
                        if live.len() == 1 {
                            match state.add(&root, live[0].clone(), budget) {
                                Ok(c) => local_changed |= c,
                                Err(()) => return Saturation::Closed,
                            }
                        }
                        // else: a pending disjunction branch (handled by `pick_branch`).
                    }
                    Concept::Top | Concept::Bottom | Concept::Neg(_) | Concept::Blocked => {}
                }
            }

            if merged {
                // A merge may have satisfied / clashed constraints on OTHER roots —
                // re-seed the whole worklist to keep completeness.
                for (a, b) in &ctx.different {
                    if state.find(a) == state.find(b) {
                        return Saturation::Closed;
                    }
                }
                dirty = distinct_roots(state).into_iter().collect();
                root = state.find(&first);
                dirty.remove(&root);
                continue;
            }
            if !local_changed {
                break;
            }
        }
    }

    // Global fixpoint — one last distinctness clash check, then a branch point.
    for (a, b) in &ctx.different {
        if state.find(a) == state.find(b) {
            return Saturation::Closed;
        }
    }
    let roots = distinct_roots(state);
    if let Some(actions) = pick_branch(state, &roots) {
        return Saturation::Branch(actions);
    }
    Saturation::Open
}

/// Choose a deterministic nondeterministic branch point from the saturated state.
fn pick_branch(state: &mut State, roots: &[String]) -> Option<Vec<Action>> {
    // First pass: an unsatisfied disjunction with ≥2 live disjuncts.
    for root in roots {
        let labels = state.labels_of(root);
        for concept in &labels {
            if let Concept::Or(disjuncts) = concept {
                if disjuncts.iter().any(|d| disjunct_satisfied(&labels, d)) {
                    continue;
                }
                let live: Vec<Concept> = disjuncts
                    .iter()
                    .filter(|d| !disjunct_refuted(&labels, d))
                    .cloned()
                    .collect();
                if live.len() >= 2 {
                    return Some(
                        live.into_iter()
                            .map(|d| Action::Add(root.clone(), d))
                            .collect(),
                    );
                }
            }
        }
    }
    // Second pass: an unsatisfied nominal enumeration with ≥2 candidates.
    for root in roots {
        let labels = state.labels_of(root);
        let root_of = state.find(root);
        for concept in &labels {
            if let Concept::Nominals(members) = concept {
                if members.iter().any(|m| state.find(m) == root_of) {
                    continue;
                }
                if members.len() >= 2 {
                    return Some(
                        members
                            .iter()
                            .map(|m| Action::Merge(root.clone(), m.clone()))
                            .collect(),
                    );
                }
            }
        }
    }
    None
}

/// The bounded depth-first case-split search: `Sat` if any branch saturates open,
/// `Unsat` if every branch closes, `Bound` if the budget is exhausted first.
fn search(state: &mut State, ctx: &Ctx, budget: &mut u64) -> SearchResult {
    if *budget == 0 {
        return SearchResult::Bound;
    }
    match saturate(state, ctx, budget) {
        Saturation::Closed => SearchResult::Unsat,
        Saturation::Open => SearchResult::Sat,
        Saturation::Branch(actions) => {
            if actions.is_empty() {
                // The budget was hit inside saturation.
                return SearchResult::Bound;
            }
            for action in actions {
                *budget = budget.saturating_sub(1);
                if *budget == 0 {
                    return SearchResult::Bound;
                }
                let mut child = state.clone();
                let clashed = match action {
                    Action::Add(root, concept) => child.add(&root, concept, budget).is_err(),
                    Action::Merge(root, member) => {
                        child.union(&root, &member);
                        false
                    }
                };
                if clashed {
                    continue;
                }
                match search(&mut child, ctx, budget) {
                    SearchResult::Sat => return SearchResult::Sat,
                    SearchResult::Bound => return SearchResult::Bound,
                    SearchResult::Unsat => {}
                }
            }
            SearchResult::Unsat
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use purrdf::{RdfDatasetBuilder, RdfLiteral, RdfQuad};

    const W: &str = "http://ex/w";

    fn quad(s: &str, p: &str, o: &str) -> RdfQuad {
        RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
    }
    fn bnode_quad(s: &str, p: &str, o: RdfTerm) -> RdfQuad {
        RdfQuad::new(RdfTerm::iri(s), p, o).in_graph(RdfTerm::iri(W))
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
    fn malformed_list_nil_first_is_inconsistent() {
        let edb = dataset(vec![bnode_quad(
            RDF_NIL,
            RDF_FIRST,
            RdfTerm::blank_node("x"),
        )]);
        assert!(is_inconsistent(edb.as_ref()));
    }

    #[test]
    fn malformed_list_nil_rest_is_inconsistent() {
        let edb = dataset(vec![bnode_quad(
            RDF_NIL,
            RDF_REST,
            RdfTerm::blank_node("x"),
        )]);
        assert!(is_inconsistent(edb.as_ref()));
    }

    #[test]
    fn complement_membership_clash_is_inconsistent() {
        // x : C, x : ¬C  (via a complement node) ⇒ inconsistent.
        let edb = dataset(vec![
            quad("http://ex/x", RDF_TYPE, "http://ex/C"),
            quad("http://ex/x", RDF_TYPE, "http://ex/notC"),
            quad("http://ex/notC", OWL_COMPLEMENT_OF, "http://ex/C"),
        ]);
        assert!(is_inconsistent(edb.as_ref()));
    }

    /// A three-node `rdf:List` `head → [a, b] → nil` over plain IRIs, so a node's
    /// subject key and its object references always coincide.
    fn list2(head: &str, a: &str, b: &str, tail: &str) -> Vec<RdfQuad> {
        vec![
            quad(head, RDF_FIRST, a),
            quad(head, RDF_REST, tail),
            quad(tail, RDF_FIRST, b),
            quad(tail, RDF_REST, RDF_NIL),
        ]
    }

    #[test]
    fn disjoint_union_with_complement_is_consistent() {
        // Child = Boy ⊎ Girl; Stewie : Child, Stewie : ¬Girl ⇒ Stewie ∈ Boy,
        // consistent (mirrors new-feature-disjointunion-001).
        let mut quads = vec![
            quad("http://ex/Child", RDF_TYPE, OWL_CLASS),
            quad("http://ex/Child", OWL_DISJOINT_UNION_OF, "http://ex/l0"),
            quad("http://ex/Stewie", RDF_TYPE, "http://ex/Child"),
            quad("http://ex/Stewie", RDF_TYPE, "http://ex/notgirl"),
            quad("http://ex/notgirl", OWL_COMPLEMENT_OF, "http://ex/Girl"),
        ];
        quads.extend(list2(
            "http://ex/l0",
            "http://ex/Boy",
            "http://ex/Girl",
            "http://ex/l1",
        ));
        let edb = dataset(quads);
        assert!(is_consistent(edb.as_ref()));
    }

    #[test]
    fn union_disjoint_unsat_is_inconsistent() {
        // x : Test; Test ⊑ (A ⊔ B); Test ⊑ ¬A (via disjoint); Test ⊑ ¬B ⇒ every
        // branch closes ⇒ inconsistent.
        let mut quads = vec![
            quad("http://ex/x", RDF_TYPE, "http://ex/Test"),
            quad("http://ex/Test", RDFS_SUBCLASSOF, "http://ex/union"),
            quad("http://ex/union", OWL_UNION_OF, "http://ex/u0"),
            // Test disjoint with both A and B ⇒ x can be in neither.
            quad("http://ex/Test", OWL_DISJOINT_WITH, "http://ex/A"),
            quad("http://ex/Test", OWL_DISJOINT_WITH, "http://ex/B"),
        ];
        quads.extend(list2(
            "http://ex/u0",
            "http://ex/A",
            "http://ex/B",
            "http://ex/u1",
        ));
        let edb = dataset(quads);
        assert!(is_inconsistent(edb.as_ref()));
    }

    #[test]
    fn union_disjoint_sat_is_consistent() {
        // x : Test; Test ⊑ (A ⊔ B); A disjoint B (no forced clash) ⇒ consistent.
        let mut quads = vec![
            quad("http://ex/x", RDF_TYPE, "http://ex/Test"),
            quad("http://ex/Test", RDFS_SUBCLASSOF, "http://ex/union"),
            quad("http://ex/union", OWL_UNION_OF, "http://ex/u0"),
            quad("http://ex/A", OWL_DISJOINT_WITH, "http://ex/B"),
        ];
        quads.extend(list2(
            "http://ex/u0",
            "http://ex/A",
            "http://ex/B",
            "http://ex/u1",
        ));
        let edb = dataset(quads);
        assert!(is_consistent(edb.as_ref()));
    }

    #[test]
    fn nominal_equality_differentfrom_clash_is_inconsistent() {
        // x : {a}; y : {a}; x differentFrom y ⇒ x = a = y contradicts distinctness.
        let edb = dataset(vec![
            quad("http://ex/x", RDF_TYPE, "http://ex/oneA"),
            quad("http://ex/oneA", OWL_ONE_OF, "http://ex/la"),
            quad("http://ex/la", RDF_FIRST, "http://ex/a"),
            quad("http://ex/la", RDF_REST, RDF_NIL),
            quad("http://ex/y", RDF_TYPE, "http://ex/oneA2"),
            quad("http://ex/oneA2", OWL_ONE_OF, "http://ex/lb"),
            quad("http://ex/lb", RDF_FIRST, "http://ex/a"),
            quad("http://ex/lb", RDF_REST, RDF_NIL),
            quad("http://ex/x", OWL_DIFFERENT_FROM, "http://ex/y"),
        ]);
        assert!(is_inconsistent(edb.as_ref()));
    }

    #[test]
    fn existential_present_blocks_consistent_withholds() {
        // A benign complement plus a someValuesFrom restriction: the complement
        // engages the decider, but the existential blocks a `Consistent` verdict
        // (no clash) ⇒ honest withhold.
        let edb = dataset(vec![
            quad("http://ex/x", RDF_TYPE, "http://ex/C"),
            quad("http://ex/notD", OWL_COMPLEMENT_OF, "http://ex/D"),
            quad("http://ex/C", RDFS_SUBCLASSOF, "http://ex/r"),
            quad("http://ex/r", RDF_TYPE, OWL_RESTRICTION),
            quad(
                "http://ex/r",
                "http://www.w3.org/2002/07/owl#onProperty",
                "http://ex/p",
            ),
            quad(
                "http://ex/r",
                "http://www.w3.org/2002/07/owl#someValuesFrom",
                "http://ex/D",
            ),
        ]);
        assert!(withholds(edb.as_ref()));
    }

    #[test]
    fn determinism_byte_stable() {
        let edb = dataset(vec![
            quad("http://ex/x", RDF_TYPE, "http://ex/C"),
            quad("http://ex/x", RDF_TYPE, "http://ex/notC"),
            quad("http://ex/notC", OWL_COMPLEMENT_OF, "http://ex/C"),
        ]);
        let a = format!("{:?}", decide(edb.as_ref()));
        let b = format!("{:?}", decide(edb.as_ref()));
        assert_eq!(a, b);
    }

    #[test]
    fn literal_oneof_is_not_engaged_as_nominal() {
        // A pure datatype (literal) oneOf is owned by the datatype sub-decider; the
        // case-split decider must not certify it consistent as a nominal.
        let edb = dataset(vec![
            quad("http://ex/x", RDF_TYPE, "http://ex/enum"),
            quad("http://ex/enum", OWL_ONE_OF, "http://ex/ll"),
            bnode_quad(
                "http://ex/ll",
                RDF_FIRST,
                RdfTerm::Literal(RdfLiteral::typed(
                    "1",
                    "http://www.w3.org/2001/XMLSchema#integer",
                )),
            ),
            quad("http://ex/ll", RDF_REST, RDF_NIL),
        ]);
        // No clash; the literal enumeration blocks `Consistent` ⇒ withhold.
        assert!(withholds(edb.as_ref()));
    }

    // ── Corpus soundness sweep (decider-isolated) ────────────────────────────────

    /// The two sibling W3C-full corpora the soundness sweep ranges over: the
    /// still-withheld `-divergence` cases AND the relocated now-decided
    /// `-decided` cases (the case-split decider's decided cases moved into the
    /// latter, so the sweep must cover both to keep exercising the decider).
    fn full_corpus_dirs() -> [std::path::PathBuf; 2] {
        let external = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../conformance/logic/cases/external");
        [
            external.join("w3c-owl2-full-divergence"),
            external.join("w3c-owl2-full-decided"),
        ]
    }

    /// The `w3c_published_verdict` recorded in a slug's flat `profile.json`.
    fn w3c_verdict(dir: &std::path::Path, slug: &str) -> Option<String> {
        let text = std::fs::read_to_string(dir.join(slug).join("profile.json")).ok()?;
        let needle = "\"w3c_published_verdict\"";
        let after = &text[text.find(needle)? + needle.len()..];
        let after = &after[after.find(':')? + 1..];
        let start = after.find('"')? + 1;
        let end = after[start..].find('"')? + start;
        Some(after[start..end].to_owned())
    }

    fn decision_token(edb: &RdfDataset) -> Option<&'static str> {
        match decide(edb) {
            Some(RefutationCertificate::InFragment {
                decision: Decision::Inconsistent,
                ..
            }) => Some("inconsistent"),
            Some(RefutationCertificate::InFragment {
                decision: Decision::Consistent,
                ..
            }) => Some("consistent"),
            _ => None,
        }
    }

    /// SOUNDNESS SWEEP (decider-isolated) — over EVERY committed W3C-full case in
    /// BOTH sibling corpora (the still-withheld `-divergence` set AND the
    /// relocated now-decided `-decided` set), for every case the case-split
    /// decider now DECIDES (`InFragment`), the
    /// decided verdict MUST equal the W3C published verdict. A single contradiction
    /// is a hard fail: it would mean a wrong decided token could ship, breaking the
    /// `corpus_only == 0` invariant. This runs the decider DIRECTLY (never the native
    /// existential chase), so it is fast on the whole corpus and isolates the new
    /// engine's soundness.
    #[test]
    fn corpus_soundness_sweep_no_decider_contradicts_w3c() {
        use purrdf::{NativeRdfFormat, dataset_from_bytes};

        let mut decided = 0usize;
        let mut contradictions = Vec::new();
        for dir in full_corpus_dirs() {
            let mut slugs: Vec<String> = std::fs::read_dir(&dir)
                .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
                .filter_map(|e| {
                    let e = e.ok()?;
                    e.file_type()
                        .ok()?
                        .is_dir()
                        .then(|| e.file_name().to_string_lossy().into_owned())
                })
                .filter(|slug| dir.join(slug).join("input.nq").exists())
                .collect();
            slugs.sort();

            for slug in &slugs {
                let bytes = std::fs::read(dir.join(slug).join("input.nq")).expect("read input.nq");
                let dataset =
                    dataset_from_bytes(&bytes, NativeRdfFormat::NQuads).expect("parse input.nq");
                let Some(token) = decision_token(dataset.as_ref()) else {
                    continue;
                };
                decided += 1;
                match w3c_verdict(&dir, slug) {
                    Some(w3c) if w3c == token => {}
                    Some(w3c) => contradictions.push(format!(
                        "{slug}: decider says {token:?}, W3C published {w3c:?}"
                    )),
                    None => contradictions.push(format!(
                        "{slug}: decider says {token:?} but no W3C verdict recorded"
                    )),
                }
            }
        }

        assert!(
            contradictions.is_empty(),
            "SOUNDNESS SWEEP FAILURE — {} decider decision(s) contradict W3C:\n  • {}",
            contradictions.len(),
            contradictions.join("\n  • ")
        );
        assert!(
            decided >= 5,
            "expected the decider to decide several divergence cases, saw {decided}"
        );
    }
}
