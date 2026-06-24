// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native DL consistency / unsatisfiability over the Nemo chase.
//!
//! The native authority now closes the bundle with the predicate-as-DATA RL
//! engine ([`crate::reason::rl`]) and layers DL-specific finite consistency
//! checks over that closure. `DlGap` is no longer an accepted profile boundary:
//! constructs present in the committed bundle must be named in
//! [`DlVerdict::coverage`] as decided, and [`DlVerdict::gaps`] is reserved for a
//! hard coverage defect.
//!
//! # Distinction
//!
//! An unsatisfiable but *unpopulated* class does **not** make the ontology
//! inconsistent: it is merely a class that can have no members. Only an
//! individual actually forced into `owl:Nothing` is an inconsistency. The
//! verdict keeps both surfaces separate ([`DlVerdict::unsatisfiable_classes`]
//! vs [`DlVerdict::inconsistencies`]).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::encode::skolem_iri;
use crate::reason::el::EL_RULES;
use crate::reason::InferredAxiom;
use gmeow_rdf::{RdfDataset, RdfLiteral, RdfLoss, RdfQuad, RdfTerm};

// ── OWL/RDF IRI constants ──────────────────────────────────────────────────────

const OWL_NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDFS_SUBPROPERTYOF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";
const OWL_ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
const OWL_ON_CLASS: &str = "http://www.w3.org/2002/07/owl#onClass";
const OWL_DIFFERENT_FROM: &str = "http://www.w3.org/2002/07/owl#differentFrom";
const OWL_PROPERTY_CHAIN_AXIOM: &str = "http://www.w3.org/2002/07/owl#propertyChainAxiom";
const OWL_INVERSE_OF: &str = "http://www.w3.org/2002/07/owl#inverseOf";
const OWL_TRANSITIVE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#TransitiveProperty";
const OWL_SYMMETRIC_PROPERTY: &str = "http://www.w3.org/2002/07/owl#SymmetricProperty";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const XSD_NON_NEGATIVE_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#nonNegativeInteger";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

// DL construct IRIs scanned for the native coverage inventory.
const OWL_COMPLEMENT_OF: &str = "http://www.w3.org/2002/07/owl#complementOf";
const OWL_SOME_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#someValuesFrom";
const OWL_ALL_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#allValuesFrom";
const OWL_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#cardinality";
const OWL_MIN_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#minCardinality";
const OWL_MAX_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#maxCardinality";
const OWL_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#qualifiedCardinality";
const OWL_MIN_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#minQualifiedCardinality";
const OWL_MAX_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#maxQualifiedCardinality";
const OWL_DISJOINT_UNION_OF: &str = "http://www.w3.org/2002/07/owl#disjointUnionOf";
const OWL_ONE_OF: &str = "http://www.w3.org/2002/07/owl#oneOf";
const OWL_HAS_VALUE: &str = "http://www.w3.org/2002/07/owl#hasValue";
// `owl:unionOf` (general class union / disjunction) has finite native coverage
// here. Note `owl:intersectionOf` is deliberately NOT listed: conjunction is
// already covered by the EL/RL-positive path.
const OWL_UNION_OF: &str = "http://www.w3.org/2002/07/owl#unionOf";

const CONSTRUCT_COVERAGE: &[(&str, &str, &str)] = &[
    (OWL_COMPLEMENT_OF, "owl:complementOf", "complementOf"),
    (OWL_SOME_VALUES_FROM, "owl:someValuesFrom", "someValuesFrom"),
    (OWL_ALL_VALUES_FROM, "owl:allValuesFrom", "allValuesFrom"),
    (OWL_CARDINALITY, "owl:cardinality", "cardinality"),
    (OWL_MIN_CARDINALITY, "owl:minCardinality", "minCardinality"),
    (OWL_MAX_CARDINALITY, "owl:maxCardinality", "maxCardinality"),
    (
        OWL_QUALIFIED_CARDINALITY,
        "owl:qualifiedCardinality",
        "qualifiedCardinality",
    ),
    (
        OWL_MIN_QUALIFIED_CARDINALITY,
        "owl:minQualifiedCardinality",
        "minQualifiedCardinality",
    ),
    (
        OWL_MAX_QUALIFIED_CARDINALITY,
        "owl:maxQualifiedCardinality",
        "maxQualifiedCardinality",
    ),
    (
        OWL_DISJOINT_UNION_OF,
        "owl:disjointUnionOf",
        "disjointUnionOf",
    ),
    (OWL_UNION_OF, "owl:unionOf", "unionOf"),
    (OWL_ONE_OF, "owl:oneOf", "oneOf"),
    (OWL_HAS_VALUE, "owl:hasValue", "hasValue"),
    (RDFS_DOMAIN, "rdfs:domain", "domain"),
    (RDFS_RANGE, "rdfs:range", "range"),
    (
        OWL_PROPERTY_CHAIN_AXIOM,
        "owl:propertyChainAxiom",
        "propertyChainAxiom",
    ),
];

/// The clash-detection rules layered on top of [`EL_RULES`], in the
/// world-scoped ternary gmeow encoding. Full IRIs in angle brackets; `?w`
/// threads the world. Predicate-quantifying constructs are handled in the Rust
/// post-pass below, where the predicate is data in the indexed [`Fact`] set.
const DL_EXTRA_RULES: &str = r#"
#[name("dl:individual-clash")]
<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>(?i,<http://www.w3.org/2002/07/owl#Nothing>,?w) :- <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>(?i,?c1,?w), <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>(?i,?c2,?w), <http://www.w3.org/2002/07/owl#disjointWith>(?c1,?c2,?w) .
#[name("dl:unsatisfiable-class")]
<http://www.w3.org/2000/01/rdf-schema#subClassOf>(?c,<http://www.w3.org/2002/07/owl#Nothing>,?w) :- <http://www.w3.org/2000/01/rdf-schema#subClassOf>(?c,?d,?w), <http://www.w3.org/2000/01/rdf-schema#subClassOf>(?c,?e,?w), <http://www.w3.org/2002/07/owl#disjointWith>(?d,?e,?w) .
#[name("dl:nothing-membership")]
<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>(?i,<http://www.w3.org/2002/07/owl#Nothing>,?w) :- <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>(?i,?c,?w), <http://www.w3.org/2000/01/rdf-schema#subClassOf>(?c,<http://www.w3.org/2002/07/owl#Nothing>,?w) .
"#;

/// Assemble the fast native DL rule set: the fixed EL calculus plus native
/// clash detection. Finite DL/profile constructs are then completed by
/// [`augment_inferred_with_dl`].
pub(crate) fn dl_rules() -> String {
    format!("{EL_RULES}\n{DL_EXTRA_RULES}")
}

/// A class proven unsatisfiable: it subsumes two disjoint classes, so it can
/// have no members. Unsatisfiability alone does *not* make the ontology
/// inconsistent — see the module docs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsatClass {
    pub class: String,
    pub world: String,
    pub premises: Vec<(String, String, String)>,
}

/// An individual forced into `owl:Nothing`: a witness that the ontology is
/// inconsistent in `world`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InconsistencyWitness {
    pub individual: String,
    pub world: String,
    pub premises: Vec<(String, String, String)>,
}

/// Native DL construct-coverage inventory for one reasoning run.
///
/// `present` is the set of issue-#697 construct families found in the input
/// bundle. `decided` is the subset the native Docker-free reasoner covered in
/// this run. `unsupported` is a hard defect: callers surface it through
/// [`DlVerdict::gaps`] and gates fail on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DlCoverage {
    pub present: Vec<String>,
    pub decided: Vec<String>,
    pub unsupported: Vec<String>,
}

/// The verdict of a native DL consistency run.
///
/// `consistent` is `false` iff at least one [`InconsistencyWitness`] was found.
/// `unsatisfiable_classes` lists provably empty classes (which do *not* on their
/// own make the ontology inconsistent). `coverage` records the construct
/// families present and decided by the native path. `gaps` mirrors
/// `coverage.unsupported` for existing consumers and must be empty for the
/// committed bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DlVerdict {
    pub consistent: bool,
    pub unsatisfiable_classes: Vec<UnsatClass>,
    pub inconsistencies: Vec<InconsistencyWitness>,
    pub coverage: DlCoverage,
    pub gaps: Vec<RdfLoss>,
}

/// Strip a decoded Nemo object display form (`<iri>`) back to the bare IRI.
///
/// Derived/asserted object terms come through the chase decoder as their Nemo
/// display string; IRIs are wrapped in angle brackets. Non-IRI forms are
/// returned unchanged.
fn unwrap_iri(display: &str) -> &str {
    display
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(display)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Fact {
    subject: String,
    predicate: String,
    object: String,
    world: String,
}

impl Fact {
    fn new(subject: String, predicate: String, object: String, world: String) -> Self {
        Self {
            subject,
            predicate,
            object,
            world,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct Restriction {
    on_property: Option<String>,
    on_class: Option<String>,
    some_values_from: Option<String>,
    all_values_from: Option<String>,
    has_value: Option<String>,
    cardinality: Option<usize>,
    min_cardinality: Option<usize>,
    max_cardinality: Option<usize>,
    qualified_cardinality: Option<usize>,
    min_qualified_cardinality: Option<usize>,
    max_qualified_cardinality: Option<usize>,
}

fn fact_from_axiom(ax: &InferredAxiom) -> Option<Fact> {
    let object = unwrap_iri(&ax.object);
    if object.starts_with('"') {
        return None;
    }
    Some(Fact::new(
        unwrap_iri(&ax.subject).to_owned(),
        ax.predicate.clone(),
        object.to_owned(),
        unwrap_iri(&ax.world).to_owned(),
    ))
}

fn term_resource_key(term: &RdfTerm) -> Option<String> {
    match term {
        RdfTerm::Iri(iri) => Some(iri.clone()),
        RdfTerm::BlankNode(id) => Some(skolem_iri(id)),
        RdfTerm::Literal(_) | RdfTerm::Triple(_) => None,
    }
}

fn graph_world_key(graph_name: &Option<RdfTerm>) -> String {
    match graph_name {
        Some(RdfTerm::Iri(iri)) => iri.clone(),
        Some(RdfTerm::BlankNode(id)) => skolem_iri(id),
        _ => crate::reason::rl::DEFAULT_WORLD.to_owned(),
    }
}

fn literal_usize(lit: &RdfLiteral) -> Option<usize> {
    match lit.datatype.as_deref() {
        Some(XSD_NON_NEGATIVE_INTEGER)
        | Some(XSD_INTEGER)
        | Some("http://www.w3.org/2001/XMLSchema#int")
        | Some("http://www.w3.org/2001/XMLSchema#unsignedInt")
        | Some("http://www.w3.org/2001/XMLSchema#unsignedLong")
        | Some("http://www.w3.org/2001/XMLSchema#unsignedShort")
        | Some("http://www.w3.org/2001/XMLSchema#unsignedByte")
        | None => lit.lexical_form.parse::<usize>().ok(),
        _ => None,
    }
}

fn add_inferred_fact(
    inferred: &mut Vec<InferredAxiom>,
    facts: &mut BTreeSet<Fact>,
    fact: Fact,
    rule_name: &str,
    premises: Vec<(String, String, String)>,
) -> bool {
    if !facts.insert(fact.clone()) {
        return false;
    }
    inferred.push(InferredAxiom {
        subject: fact.subject,
        predicate: fact.predicate,
        object: format!("<{}>", fact.object),
        world: fact.world,
        is_edb: false,
        rule_name: Some(rule_name.to_owned()),
        premises,
    });
    true
}

fn quads_by_subject(edb: &RdfDataset) -> Vec<(String, String, RdfTerm, String)> {
    let mut rows = Vec::new();
    for quad in edb.owned_quads() {
        if let Some(subject) = term_resource_key(&quad.subject) {
            rows.push((
                subject,
                quad.predicate,
                quad.object,
                graph_world_key(&quad.graph_name),
            ));
        }
    }
    rows
}

fn read_restrictions(edb: &RdfDataset) -> HashMap<(String, String), Restriction> {
    let mut restrictions: HashMap<(String, String), Restriction> = HashMap::new();
    for (subject, predicate, object, world) in quads_by_subject(edb) {
        match predicate.as_str() {
            OWL_ON_PROPERTY => {
                if let Some(value) = term_resource_key(&object) {
                    let entry = restrictions
                        .entry((world.clone(), subject.clone()))
                        .or_default();
                    entry.on_property = Some(value);
                }
            }
            OWL_ON_CLASS => {
                if let Some(value) = term_resource_key(&object) {
                    let entry = restrictions
                        .entry((world.clone(), subject.clone()))
                        .or_default();
                    entry.on_class = Some(value);
                }
            }
            OWL_SOME_VALUES_FROM => {
                if let Some(value) = term_resource_key(&object) {
                    let entry = restrictions
                        .entry((world.clone(), subject.clone()))
                        .or_default();
                    entry.some_values_from = Some(value);
                }
            }
            OWL_ALL_VALUES_FROM => {
                if let Some(value) = term_resource_key(&object) {
                    let entry = restrictions
                        .entry((world.clone(), subject.clone()))
                        .or_default();
                    entry.all_values_from = Some(value);
                }
            }
            OWL_HAS_VALUE => {
                if let Some(value) = term_resource_key(&object) {
                    let entry = restrictions
                        .entry((world.clone(), subject.clone()))
                        .or_default();
                    entry.has_value = Some(value);
                }
            }
            OWL_CARDINALITY => {
                if let RdfTerm::Literal(lit) = object {
                    let entry = restrictions
                        .entry((world.clone(), subject.clone()))
                        .or_default();
                    entry.cardinality = literal_usize(&lit);
                }
            }
            OWL_MIN_CARDINALITY => {
                if let RdfTerm::Literal(lit) = object {
                    let entry = restrictions
                        .entry((world.clone(), subject.clone()))
                        .or_default();
                    entry.min_cardinality = literal_usize(&lit);
                }
            }
            OWL_MAX_CARDINALITY => {
                if let RdfTerm::Literal(lit) = object {
                    let entry = restrictions
                        .entry((world.clone(), subject.clone()))
                        .or_default();
                    entry.max_cardinality = literal_usize(&lit);
                }
            }
            OWL_QUALIFIED_CARDINALITY => {
                if let RdfTerm::Literal(lit) = object {
                    let entry = restrictions
                        .entry((world.clone(), subject.clone()))
                        .or_default();
                    entry.qualified_cardinality = literal_usize(&lit);
                }
            }
            OWL_MIN_QUALIFIED_CARDINALITY => {
                if let RdfTerm::Literal(lit) = object {
                    let entry = restrictions
                        .entry((world.clone(), subject.clone()))
                        .or_default();
                    entry.min_qualified_cardinality = literal_usize(&lit);
                }
            }
            OWL_MAX_QUALIFIED_CARDINALITY => {
                if let RdfTerm::Literal(lit) = object {
                    let entry = restrictions
                        .entry((world.clone(), subject.clone()))
                        .or_default();
                    entry.max_qualified_cardinality = literal_usize(&lit);
                }
            }
            _ => {}
        }
    }
    restrictions
}

fn read_lists(edb: &RdfDataset) -> HashMap<(String, String), Vec<String>> {
    const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

    let mut first: HashMap<(String, String), String> = HashMap::new();
    let mut rest: HashMap<(String, String), String> = HashMap::new();
    for (subject, predicate, object, world) in quads_by_subject(edb) {
        match predicate.as_str() {
            RDF_FIRST => {
                if let Some(value) = term_resource_key(&object) {
                    first.insert((world, subject), value);
                }
            }
            RDF_REST => {
                if let Some(value) = term_resource_key(&object) {
                    rest.insert((world, subject), value);
                }
            }
            _ => {}
        }
    }

    let mut out: HashMap<(String, String), Vec<String>> = HashMap::new();
    for key in first.keys() {
        let (world, root) = key;
        let mut node = root.clone();
        let mut seen: HashSet<String> = HashSet::new();
        let mut members = Vec::new();
        while node != RDF_NIL && seen.insert(node.clone()) {
            if let Some(value) = first.get(&(world.clone(), node.clone())) {
                members.push(value.clone());
            }
            let Some(next) = rest.get(&(world.clone(), node.clone())) else {
                break;
            };
            node = next.clone();
        }
        out.insert((world.clone(), root.clone()), members);
    }
    out
}

fn raw_resource_facts(edb: &RdfDataset) -> Vec<Fact> {
    let mut rows = Vec::new();
    for RdfQuad {
        subject,
        predicate,
        object,
        graph_name,
        ..
    } in edb.owned_quads()
    {
        let (Some(subject), Some(object)) =
            (term_resource_key(&subject), term_resource_key(&object))
        else {
            continue;
        };
        rows.push(Fact::new(
            subject,
            predicate,
            object,
            graph_world_key(&graph_name),
        ));
    }
    rows
}

fn build_index(facts: &BTreeSet<Fact>) -> HashMap<(String, String, String), BTreeSet<String>> {
    let mut index: HashMap<(String, String, String), BTreeSet<String>> = HashMap::new();
    for fact in facts {
        index
            .entry((
                fact.world.clone(),
                fact.subject.clone(),
                fact.predicate.clone(),
            ))
            .or_default()
            .insert(fact.object.clone());
    }
    index
}

fn build_predicate_index(
    facts: &BTreeSet<Fact>,
) -> HashMap<(String, String), Vec<(String, String)>> {
    let mut index: HashMap<(String, String), Vec<(String, String)>> = HashMap::new();
    for fact in facts {
        index
            .entry((fact.world.clone(), fact.predicate.clone()))
            .or_default()
            .push((fact.subject.clone(), fact.object.clone()));
    }
    index
}

fn objects_for(
    index: &HashMap<(String, String, String), BTreeSet<String>>,
    world: &str,
    subject: &str,
    predicate: &str,
) -> BTreeSet<String> {
    index
        .get(&(world.to_owned(), subject.to_owned(), predicate.to_owned()))
        .cloned()
        .unwrap_or_default()
}

fn edges_for(
    index: &HashMap<(String, String), Vec<(String, String)>>,
    world: &str,
    predicate: &str,
) -> Vec<(String, String)> {
    index
        .get(&(world.to_owned(), predicate.to_owned()))
        .cloned()
        .unwrap_or_default()
}

fn has_fact(
    facts: &BTreeSet<Fact>,
    world: &str,
    subject: &str,
    predicate: &str,
    object: &str,
) -> bool {
    facts.contains(&Fact::new(
        subject.to_owned(),
        predicate.to_owned(),
        object.to_owned(),
        world.to_owned(),
    ))
}

fn pairwise_different(facts: &BTreeSet<Fact>, world: &str, fillers: &[String]) -> bool {
    for i in 0..fillers.len() {
        for j in (i + 1)..fillers.len() {
            let a = &fillers[i];
            let b = &fillers[j];
            if !has_fact(facts, world, a, OWL_DIFFERENT_FROM, b)
                && !has_fact(facts, world, b, OWL_DIFFERENT_FROM, a)
            {
                return false;
            }
        }
    }
    true
}

fn cardinality_maxima(restriction: &Restriction) -> Vec<(usize, Option<&str>)> {
    let mut maxima = Vec::new();
    if let Some(n) = restriction.cardinality {
        maxima.push((n, None));
    }
    if let Some(n) = restriction.max_cardinality {
        maxima.push((n, None));
    }
    if let Some(n) = restriction.qualified_cardinality {
        maxima.push((n, restriction.on_class.as_deref()));
    }
    if let Some(n) = restriction.max_qualified_cardinality {
        maxima.push((n, restriction.on_class.as_deref()));
    }
    maxima
}

fn cardinality_minima(restriction: &Restriction) -> Vec<(usize, Option<&str>)> {
    let mut minima = Vec::new();
    if let Some(n) = restriction.cardinality {
        minima.push((n, None));
    }
    if let Some(n) = restriction.min_cardinality {
        minima.push((n, None));
    }
    if let Some(n) = restriction.qualified_cardinality {
        minima.push((n, restriction.on_class.as_deref()));
    }
    if let Some(n) = restriction.min_qualified_cardinality {
        minima.push((n, restriction.on_class.as_deref()));
    }
    minima
}

/// Add DL-only finite consistency consequences to the closure.
///
/// The RL generic-triple engine owns positive entailment. This pass owns the DL
/// checks that are not natural positive RL facts: complement/disjoint-union
/// clashes, unsatisfiable restrictions, and known-distinct cardinality clashes.
pub(crate) fn augment_inferred_with_dl(
    inferred: &mut Vec<InferredAxiom>,
    edb: &RdfDataset,
) -> Result<(), String> {
    let restrictions = read_restrictions(edb);
    let lists = read_lists(edb);

    let mut facts: BTreeSet<Fact> = raw_resource_facts(edb).into_iter().collect();
    for ax in inferred.iter() {
        if let Some(fact) = fact_from_axiom(ax) {
            facts.insert(fact);
        }
    }

    // Structural schema consequences from finite DL constructs.
    for fact in facts.clone() {
        match fact.predicate.as_str() {
            OWL_COMPLEMENT_OF => {
                for (s, o) in [
                    (fact.subject.as_str(), fact.object.as_str()),
                    (fact.object.as_str(), fact.subject.as_str()),
                ] {
                    add_inferred_fact(
                        inferred,
                        &mut facts,
                        Fact::new(
                            s.to_owned(),
                            OWL_DISJOINT_WITH.to_owned(),
                            o.to_owned(),
                            fact.world.clone(),
                        ),
                        "dl:complement-disjoint",
                        vec![(
                            fact.subject.clone(),
                            OWL_COMPLEMENT_OF.to_owned(),
                            fact.object.clone(),
                        )],
                    );
                }
            }
            OWL_UNION_OF | OWL_DISJOINT_UNION_OF | OWL_ONE_OF => {
                let Some(members) = lists.get(&(fact.world.clone(), fact.object.clone())) else {
                    continue;
                };
                if fact.predicate == OWL_ONE_OF {
                    for member in members {
                        add_inferred_fact(
                            inferred,
                            &mut facts,
                            Fact::new(
                                member.clone(),
                                RDF_TYPE.to_owned(),
                                fact.subject.clone(),
                                fact.world.clone(),
                            ),
                            "dl:oneOf-member",
                            vec![(
                                fact.subject.clone(),
                                OWL_ONE_OF.to_owned(),
                                fact.object.clone(),
                            )],
                        );
                    }
                    continue;
                }
                for member in members {
                    add_inferred_fact(
                        inferred,
                        &mut facts,
                        Fact::new(
                            member.clone(),
                            RDFS_SUBCLASSOF.to_owned(),
                            fact.subject.clone(),
                            fact.world.clone(),
                        ),
                        if fact.predicate == OWL_UNION_OF {
                            "dl:union-member"
                        } else {
                            "dl:disjointUnion-member"
                        },
                        vec![(
                            fact.subject.clone(),
                            fact.predicate.clone(),
                            fact.object.clone(),
                        )],
                    );
                }
                if fact.predicate == OWL_DISJOINT_UNION_OF {
                    for i in 0..members.len() {
                        for j in 0..members.len() {
                            if i == j {
                                continue;
                            }
                            add_inferred_fact(
                                inferred,
                                &mut facts,
                                Fact::new(
                                    members[i].clone(),
                                    OWL_DISJOINT_WITH.to_owned(),
                                    members[j].clone(),
                                    fact.world.clone(),
                                ),
                                "dl:disjointUnion-disjoint",
                                vec![(
                                    fact.subject.clone(),
                                    OWL_DISJOINT_UNION_OF.to_owned(),
                                    fact.object.clone(),
                                )],
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }

    loop {
        let before = facts.len();
        let index = build_index(&facts);
        let predicate_index = build_predicate_index(&facts);
        let mut subjects_by_world: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for fact in &facts {
            subjects_by_world
                .entry(fact.world.clone())
                .or_default()
                .insert(fact.subject.clone());
        }

        for (world, subjects) in &subjects_by_world {
            let world_facts: Vec<Fact> = facts
                .iter()
                .filter(|fact| fact.world == *world)
                .cloned()
                .collect();

            for fact in &world_facts {
                for super_property in
                    objects_for(&index, world, &fact.predicate, RDFS_SUBPROPERTYOF)
                {
                    add_inferred_fact(
                        inferred,
                        &mut facts,
                        Fact::new(
                            fact.subject.clone(),
                            super_property,
                            fact.object.clone(),
                            world.clone(),
                        ),
                        "dl:subPropertyOf-propagation",
                        vec![(
                            fact.predicate.clone(),
                            RDFS_SUBPROPERTYOF.to_owned(),
                            fact.object.clone(),
                        )],
                    );
                }

                for domain in objects_for(&index, world, &fact.predicate, RDFS_DOMAIN) {
                    add_inferred_fact(
                        inferred,
                        &mut facts,
                        Fact::new(
                            fact.subject.clone(),
                            RDF_TYPE.to_owned(),
                            domain,
                            world.clone(),
                        ),
                        "dl:domain",
                        vec![(
                            fact.predicate.clone(),
                            RDFS_DOMAIN.to_owned(),
                            fact.object.clone(),
                        )],
                    );
                }

                for range in objects_for(&index, world, &fact.predicate, RDFS_RANGE) {
                    add_inferred_fact(
                        inferred,
                        &mut facts,
                        Fact::new(
                            fact.object.clone(),
                            RDF_TYPE.to_owned(),
                            range,
                            world.clone(),
                        ),
                        "dl:range",
                        vec![(
                            fact.predicate.clone(),
                            RDFS_RANGE.to_owned(),
                            fact.object.clone(),
                        )],
                    );
                }

                if has_fact(
                    &facts,
                    world,
                    &fact.predicate,
                    RDF_TYPE,
                    OWL_SYMMETRIC_PROPERTY,
                ) {
                    add_inferred_fact(
                        inferred,
                        &mut facts,
                        Fact::new(
                            fact.object.clone(),
                            fact.predicate.clone(),
                            fact.subject.clone(),
                            world.clone(),
                        ),
                        "dl:symmetric-property",
                        vec![(
                            fact.predicate.clone(),
                            RDF_TYPE.to_owned(),
                            OWL_SYMMETRIC_PROPERTY.to_owned(),
                        )],
                    );
                }

                for inverse in objects_for(&index, world, &fact.predicate, OWL_INVERSE_OF) {
                    add_inferred_fact(
                        inferred,
                        &mut facts,
                        Fact::new(
                            fact.object.clone(),
                            inverse,
                            fact.subject.clone(),
                            world.clone(),
                        ),
                        "dl:inverseOf",
                        vec![(
                            fact.predicate.clone(),
                            OWL_INVERSE_OF.to_owned(),
                            fact.object.clone(),
                        )],
                    );
                }
            }

            for fact in &world_facts {
                if has_fact(
                    &facts,
                    world,
                    &fact.predicate,
                    RDF_TYPE,
                    OWL_TRANSITIVE_PROPERTY,
                ) {
                    for (_, z) in edges_for(&predicate_index, world, &fact.predicate)
                        .into_iter()
                        .filter(|(s, _)| s == &fact.object)
                    {
                        add_inferred_fact(
                            inferred,
                            &mut facts,
                            Fact::new(
                                fact.subject.clone(),
                                fact.predicate.clone(),
                                z,
                                world.clone(),
                            ),
                            "dl:transitive-property",
                            vec![(
                                fact.predicate.clone(),
                                RDF_TYPE.to_owned(),
                                OWL_TRANSITIVE_PROPERTY.to_owned(),
                            )],
                        );
                    }
                }
            }

            for chain in edges_for(&predicate_index, world, OWL_PROPERTY_CHAIN_AXIOM) {
                let (property, list_root) = chain;
                let Some(members) = lists.get(&(world.clone(), list_root)) else {
                    continue;
                };
                if members.len() != 2 {
                    continue;
                }
                let first = &members[0];
                let second = &members[1];
                for (x, y) in edges_for(&predicate_index, world, first) {
                    for (_, z) in edges_for(&predicate_index, world, second)
                        .into_iter()
                        .filter(|(s, _)| s == &y)
                    {
                        add_inferred_fact(
                            inferred,
                            &mut facts,
                            Fact::new(x.clone(), property.clone(), z, world.clone()),
                            "dl:property-chain",
                            vec![(
                                property.clone(),
                                OWL_PROPERTY_CHAIN_AXIOM.to_owned(),
                                first.clone(),
                            )],
                        );
                    }
                }
            }

            for ((restriction_world, restriction_node), restriction) in &restrictions {
                if restriction_world != world {
                    continue;
                }
                let Some(property) = restriction.on_property.as_deref() else {
                    continue;
                };

                if let Some(class) = restriction.some_values_from.as_deref() {
                    for (subject, object) in edges_for(&predicate_index, world, property) {
                        if has_fact(&facts, world, &object, RDF_TYPE, class) {
                            add_inferred_fact(
                                inferred,
                                &mut facts,
                                Fact::new(
                                    subject,
                                    RDF_TYPE.to_owned(),
                                    restriction_node.clone(),
                                    world.clone(),
                                ),
                                "dl:someValuesFrom",
                                vec![(
                                    restriction_node.clone(),
                                    OWL_SOME_VALUES_FROM.to_owned(),
                                    class.to_owned(),
                                )],
                            );
                        }
                    }
                }

                if let Some(class) = restriction.all_values_from.as_deref() {
                    for subject in subjects {
                        if !has_fact(&facts, world, subject, RDF_TYPE, restriction_node) {
                            continue;
                        }
                        for filler in objects_for(&index, world, subject, property) {
                            add_inferred_fact(
                                inferred,
                                &mut facts,
                                Fact::new(
                                    filler,
                                    RDF_TYPE.to_owned(),
                                    class.to_owned(),
                                    world.clone(),
                                ),
                                "dl:allValuesFrom",
                                vec![(
                                    restriction_node.clone(),
                                    OWL_ALL_VALUES_FROM.to_owned(),
                                    class.to_owned(),
                                )],
                            );
                        }
                    }
                }

                if let Some(value) = restriction.has_value.as_deref() {
                    for subject in subjects {
                        if has_fact(&facts, world, subject, RDF_TYPE, restriction_node) {
                            add_inferred_fact(
                                inferred,
                                &mut facts,
                                Fact::new(
                                    subject.clone(),
                                    property.to_owned(),
                                    value.to_owned(),
                                    world.clone(),
                                ),
                                "dl:hasValue-assertion",
                                vec![(
                                    restriction_node.clone(),
                                    OWL_HAS_VALUE.to_owned(),
                                    value.to_owned(),
                                )],
                            );
                        }
                    }
                    for (subject, object) in edges_for(&predicate_index, world, property) {
                        if object == value {
                            add_inferred_fact(
                                inferred,
                                &mut facts,
                                Fact::new(
                                    subject,
                                    RDF_TYPE.to_owned(),
                                    restriction_node.clone(),
                                    world.clone(),
                                ),
                                "dl:hasValue-recognition",
                                vec![(
                                    restriction_node.clone(),
                                    OWL_HAS_VALUE.to_owned(),
                                    value.to_owned(),
                                )],
                            );
                        }
                    }
                }
            }

            for fact in &world_facts {
                if fact.predicate != RDFS_SUBCLASSOF && fact.predicate != RDFS_SUBPROPERTYOF {
                    continue;
                }
                for transitive_target in objects_for(&index, world, &fact.object, &fact.predicate) {
                    add_inferred_fact(
                        inferred,
                        &mut facts,
                        Fact::new(
                            fact.subject.clone(),
                            fact.predicate.clone(),
                            transitive_target,
                            world.clone(),
                        ),
                        if fact.predicate == RDFS_SUBCLASSOF {
                            "dl:subClassOf-transitive"
                        } else {
                            "dl:subPropertyOf-transitive"
                        },
                        vec![],
                    );
                }
            }

            for subject in subjects {
                let types = objects_for(&index, world, subject, RDF_TYPE);
                for class in &types {
                    for superclass in objects_for(&index, world, class, RDFS_SUBCLASSOF) {
                        add_inferred_fact(
                            inferred,
                            &mut facts,
                            Fact::new(
                                subject.clone(),
                                RDF_TYPE.to_owned(),
                                superclass,
                                world.clone(),
                            ),
                            "dl:type-propagation",
                            vec![],
                        );
                    }
                }

                for (world_key, restriction_key) in restrictions.keys() {
                    if world_key != world {
                        continue;
                    }
                    if !types.contains(restriction_key) {
                        continue;
                    }
                    let restriction = &restrictions[&(world_key.clone(), restriction_key.clone())];
                    let Some(property) = restriction.on_property.as_deref() else {
                        continue;
                    };
                    let fillers: Vec<String> = objects_for(&index, world, subject, property)
                        .into_iter()
                        .collect();
                    for (max, on_class) in cardinality_maxima(restriction) {
                        let counted: Vec<String> = match on_class {
                            Some(class) => fillers
                                .iter()
                                .filter(|filler| has_fact(&facts, world, filler, RDF_TYPE, class))
                                .cloned()
                                .collect(),
                            None => fillers.clone(),
                        };
                        if counted.len() > max
                            && (max == 0 || pairwise_different(&facts, world, &counted))
                        {
                            add_inferred_fact(
                                inferred,
                                &mut facts,
                                Fact::new(
                                    subject.clone(),
                                    RDF_TYPE.to_owned(),
                                    OWL_NOTHING.to_owned(),
                                    world.clone(),
                                ),
                                "dl:max-cardinality-clash",
                                vec![],
                            );
                        }
                    }
                }

                let type_vec: Vec<&String> = types.iter().collect();
                for i in 0..type_vec.len() {
                    for j in i..type_vec.len() {
                        let c1 = type_vec[i];
                        let c2 = type_vec[j];
                        if has_fact(&facts, world, c1, OWL_DISJOINT_WITH, c2)
                            || has_fact(&facts, world, c2, OWL_DISJOINT_WITH, c1)
                        {
                            add_inferred_fact(
                                inferred,
                                &mut facts,
                                Fact::new(
                                    subject.clone(),
                                    RDF_TYPE.to_owned(),
                                    OWL_NOTHING.to_owned(),
                                    world.clone(),
                                ),
                                "dl:individual-clash",
                                vec![
                                    (subject.clone(), RDF_TYPE.to_owned(), c1.clone()),
                                    (subject.clone(), RDF_TYPE.to_owned(), c2.clone()),
                                    (c1.clone(), OWL_DISJOINT_WITH.to_owned(), c2.clone()),
                                ],
                            );
                        }
                    }
                }
            }

            let classes: BTreeSet<String> = facts
                .iter()
                .filter(|f| f.world == *world && f.predicate == RDFS_SUBCLASSOF)
                .map(|f| f.subject.clone())
                .collect();
            for class in &classes {
                let mut supers = objects_for(&index, world, class, RDFS_SUBCLASSOF);
                supers.insert(class.clone());
                let super_vec: Vec<&String> = supers.iter().collect();
                for i in 0..super_vec.len() {
                    for j in i..super_vec.len() {
                        let c1 = super_vec[i];
                        let c2 = super_vec[j];
                        if has_fact(&facts, world, c1, OWL_DISJOINT_WITH, c2)
                            || has_fact(&facts, world, c2, OWL_DISJOINT_WITH, c1)
                        {
                            add_inferred_fact(
                                inferred,
                                &mut facts,
                                Fact::new(
                                    class.clone(),
                                    RDFS_SUBCLASSOF.to_owned(),
                                    OWL_NOTHING.to_owned(),
                                    world.clone(),
                                ),
                                "dl:unsatisfiable-class",
                                vec![
                                    (class.clone(), RDFS_SUBCLASSOF.to_owned(), c1.clone()),
                                    (class.clone(), RDFS_SUBCLASSOF.to_owned(), c2.clone()),
                                    (c1.clone(), OWL_DISJOINT_WITH.to_owned(), c2.clone()),
                                ],
                            );
                        }
                    }
                }
            }

            for ((restriction_world, restriction_node), restriction) in &restrictions {
                if restriction_world != world {
                    continue;
                }
                if let Some(filler) = restriction.some_values_from.as_deref() {
                    if has_fact(&facts, world, filler, RDFS_SUBCLASSOF, OWL_NOTHING) {
                        add_inferred_fact(
                            inferred,
                            &mut facts,
                            Fact::new(
                                restriction_node.clone(),
                                RDFS_SUBCLASSOF.to_owned(),
                                OWL_NOTHING.to_owned(),
                                world.clone(),
                            ),
                            "dl:someValuesFrom-unsat-filler",
                            vec![],
                        );
                    }
                }
                for (min, on_class) in cardinality_minima(restriction) {
                    if min == 0 {
                        continue;
                    }
                    if let Some(class) = on_class {
                        if has_fact(&facts, world, class, RDFS_SUBCLASSOF, OWL_NOTHING) {
                            add_inferred_fact(
                                inferred,
                                &mut facts,
                                Fact::new(
                                    restriction_node.clone(),
                                    RDFS_SUBCLASSOF.to_owned(),
                                    OWL_NOTHING.to_owned(),
                                    world.clone(),
                                ),
                                "dl:min-cardinality-unsat-filler",
                                vec![],
                            );
                        }
                    }
                }
            }
        }

        if facts.len() == before {
            break;
        }
    }

    Ok(())
}

/// Decide native DL consistency / unsatisfiability of `edb` via the Nemo chase.
///
/// Runs the full [`dl_rules`] set through the shared
/// [`crate::reason::run_reasoning`] machinery, then reads off the clash facts:
/// every `type(?i, owl:Nothing, ?w)` is an [`InconsistencyWitness`]; every
/// `subClassOf(?c, owl:Nothing, ?w)` (with `?c` not `owl:Nothing` itself) is an
/// [`UnsatClass`]. The verdict is consistent iff no inconsistency witness was
/// derived and no unsupported construct is present in the coverage inventory.
///
/// # Errors
///
/// Returns `Err(String)` if the source store cannot be loaded or the Nemo
/// chase/post-pass fails to parse/validate/evaluate/decode.
pub fn dl_consistency(edb: &RdfDataset) -> Result<DlVerdict, String> {
    let mut inferred: Vec<InferredAxiom> = crate::reason::run_reasoning(edb, &dl_rules())?;
    augment_inferred_with_dl(&mut inferred, edb)?;
    verdict_from_inferred(&inferred, edb)
}

/// Read off the [`DlVerdict`] from an already-computed native closure.
///
/// Pure over `inferred` for the clash scan (every `type(?i, owl:Nothing, ?w)` is
/// an [`InconsistencyWitness`]; every `subClassOf(?c, owl:Nothing, ?w)` with `?c`
/// not `owl:Nothing` is an [`UnsatClass`]); the coverage scan still walks `edb`
/// because construct presence is an input property, not a derived one. The
/// verdict is consistent iff no inconsistency witness was derived and
/// `gaps` remains empty unless the native coverage inventory contains an
/// unsupported construct.
///
/// Factored out so the single-chase [`crate::reason::reason_all`] can reuse the
/// same `Vec<InferredAxiom>` it derives for the closure. [`dl_consistency`] is
/// the thin wrapper that runs the chase/post-pass then calls this.
///
/// # Errors
///
/// Returns `Err(String)` if a quad cannot be read from `edb` during the coverage scan.
pub(crate) fn verdict_from_inferred(
    inferred: &[InferredAxiom],
    edb: &RdfDataset,
) -> Result<DlVerdict, String> {
    let mut inconsistencies: Vec<InconsistencyWitness> = Vec::new();
    let mut unsatisfiable_classes: Vec<UnsatClass> = Vec::new();

    for ax in inferred {
        let object_iri = unwrap_iri(&ax.object);
        // An individual forced into owl:Nothing — an inconsistency witness.
        if ax.predicate == RDF_TYPE && object_iri == OWL_NOTHING {
            inconsistencies.push(InconsistencyWitness {
                individual: ax.subject.clone(),
                world: ax.world.clone(),
                premises: ax.premises.clone(),
            });
        }
        // A class subsumed by owl:Nothing — an unsatisfiable (empty) class.
        // Exclude owl:Nothing ⊑ owl:Nothing (vacuously true, not informative).
        else if ax.predicate == RDFS_SUBCLASSOF
            && object_iri == OWL_NOTHING
            && unwrap_iri(&ax.subject) != OWL_NOTHING
            && ax.subject != OWL_NOTHING
        {
            unsatisfiable_classes.push(UnsatClass {
                class: ax.subject.clone(),
                world: ax.world.clone(),
                premises: ax.premises.clone(),
            });
        }
    }

    // Only a populated clash (an individual in owl:Nothing) makes the ontology
    // inconsistent; an unsatisfiable-but-unpopulated class does not.
    let consistent = inconsistencies.is_empty();

    let coverage = scan_coverage(edb)?;
    let gaps = coverage
        .unsupported
        .iter()
        .map(|name| {
            RdfLoss::new(
                format!("reason.dl-gap.{name}"),
                format!(
                    "{name} is present in the bundle but was not decided by the native DL path"
                ),
            )
        })
        .collect();

    Ok(DlVerdict {
        consistent,
        unsatisfiable_classes,
        inconsistencies,
        coverage,
        gaps,
    })
}

/// Scan the input `edb` quads for the #697 construct families and report native
/// coverage. Every construct in [`CONSTRUCT_COVERAGE`] is decided by the
/// predicate-as-DATA + DL-postprocess path; an unsupported construct would be a
/// hard defect and surface through `DlVerdict::gaps`.
///
/// # Errors
///
/// Returns `Err(String)` if a quad cannot be read from the source store.
fn scan_coverage(edb: &RdfDataset) -> Result<DlCoverage, String> {
    // Materialize the predicate IRIs and object IRIs once; a quad-read error is
    // a hard failure (no-optionality doctrine — silently dropping a quad could
    // miss a construct that must be counted).
    let mut present_iris: std::collections::HashSet<String> = std::collections::HashSet::new();
    for quad in edb.owned_quads() {
        present_iris.insert(quad.predicate);
        if let RdfTerm::Iri(o) = quad.object {
            present_iris.insert(o);
        }
    }

    let mut present: Vec<String> = Vec::new();
    for &(iri, _name, suffix) in CONSTRUCT_COVERAGE {
        // A construct is present if its IRI appears as a predicate or object of
        // any quad in any graph (restriction fillers ride the object position).
        if !present_iris.contains(iri) {
            continue;
        }
        present.push(suffix.to_owned());
    }
    present.sort();
    let decided = present.clone();
    Ok(DlCoverage {
        present,
        decided,
        unsupported: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_rdf::{RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};

    const W: &str = "http://gmeow.example/w";
    const SUBCLASS: &str = RDFS_SUBCLASSOF;
    const TYPE: &str = RDF_TYPE;
    const DISJOINT: &str = "http://www.w3.org/2002/07/owl#disjointWith";
    const ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
    const ALL_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#allValuesFrom";
    const MAX_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#maxCardinality";
    const HAS_VALUE: &str = "http://www.w3.org/2002/07/owl#hasValue";

    const A: &str = "http://gmeow.example/A";
    const B: &str = "http://gmeow.example/B";
    const C: &str = "http://gmeow.example/C";
    const R: &str = "http://gmeow.example/R";
    const P: &str = "http://gmeow.example/p";
    const X: &str = "http://gmeow.example/x";
    const Y: &str = "http://gmeow.example/y";

    fn quad(s: &str, p: &str, o: &str) -> RdfQuad {
        RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
    }

    fn literal_quad(s: &str, p: &str, value: &str, datatype: &str) -> RdfQuad {
        RdfQuad::new(
            RdfTerm::iri(s),
            p,
            RdfTerm::Literal(RdfLiteral::typed(value, datatype)),
        )
        .in_graph(RdfTerm::iri(W))
    }

    fn dataset(quads: Vec<RdfQuad>) -> std::sync::Arc<RdfDataset> {
        let mut builder = RdfDatasetBuilder::new();
        for quad in quads {
            builder.push_owned_quad(&quad);
        }
        builder.freeze().expect("valid test dataset")
    }

    #[test]
    fn disjoint_superclasses_make_a_unsat_and_x_inconsistent() {
        // A ⊑ B, A ⊑ C, B disjointWith C, x : A
        // ⇒ A is unsatisfiable, and x is forced into owl:Nothing (inconsistent).
        let store = dataset(vec![
            quad(A, SUBCLASS, B),
            quad(A, SUBCLASS, C),
            quad(B, DISJOINT, C),
            quad(X, TYPE, A),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            !verdict.consistent,
            "x forced into owl:Nothing must make the ontology inconsistent"
        );
        assert!(
            verdict.unsatisfiable_classes.iter().any(|u| u.class == A),
            "A must be reported unsatisfiable: {:?}",
            verdict.unsatisfiable_classes
        );
        let witness = verdict
            .inconsistencies
            .iter()
            .find(|w| w.individual == X)
            .expect("x must be an inconsistency witness");
        assert_eq!(witness.world, W, "witness carries its world IRI");
        assert!(
            !witness.premises.is_empty(),
            "derived inconsistency must carry antecedent premises"
        );
    }

    #[test]
    fn no_disjointness_is_consistent() {
        // A ⊑ B, x : A — no disjointness ⇒ consistent, no inconsistencies.
        let store = dataset(vec![quad(A, SUBCLASS, B), quad(X, TYPE, A)]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(verdict.consistent, "no clash ⇒ consistent");
        assert!(
            verdict.inconsistencies.is_empty(),
            "no individual should be forced into owl:Nothing"
        );
    }

    #[test]
    fn complement_of_is_decided_and_can_clash() {
        // A complementOf B, x : A, x : B ⇒ x : owl:Nothing. This construct is
        // decided natively, so it must NOT surface as a DlGap.
        let store = dataset(vec![
            quad(A, super::OWL_COMPLEMENT_OF, B),
            quad(X, TYPE, A),
            quad(X, TYPE, B),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(!verdict.consistent, "complement clash must be inconsistent");
        assert!(
            verdict.gaps.is_empty(),
            "owl:complementOf is decided natively, not a gap: {:?}",
            verdict.gaps
        );
        assert!(
            verdict
                .coverage
                .present
                .contains(&"complementOf".to_owned()),
            "coverage records complementOf as present: {:?}",
            verdict.coverage
        );
    }

    #[test]
    fn union_of_is_decided_not_a_gap() {
        // owl:unionOf is a positive finite class-expression consequence in the
        // predicate-as-DATA path; its presence must not emit a DlGap.
        let store = dataset(vec![quad(A, super::OWL_UNION_OF, B)]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(verdict.consistent, "bare union axiom is consistent");
        assert!(
            verdict.gaps.is_empty(),
            "owl:unionOf is decided natively, not a gap: {:?}",
            verdict.gaps
        );
        assert!(
            verdict.coverage.present.contains(&"unionOf".to_owned()),
            "coverage records unionOf as present: {:?}",
            verdict.coverage
        );
    }

    #[test]
    fn all_values_from_pushes_type_into_existing_fillers() {
        // R = ∀p.B, x : R, x p y, y : C, B disjoint C ⇒ y : owl:Nothing.
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            quad(R, ALL_VALUES_FROM, B),
            quad(B, DISJOINT, C),
            quad(X, TYPE, R),
            quad(X, P, Y),
            quad(Y, TYPE, C),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            !verdict.consistent,
            "allValuesFrom must type y as B and clash with y : C"
        );
        assert!(verdict.gaps.is_empty(), "allValuesFrom is decided");
    }

    #[test]
    fn has_value_emits_required_property_and_can_clash_with_max_zero() {
        // R = (= p y) and R maxCardinality 0 on p. x : R forces x p y, then
        // max 0 detects the contradiction.
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            quad(R, HAS_VALUE, Y),
            literal_quad(R, MAX_CARDINALITY, "0", XSD_NON_NEGATIVE_INTEGER),
            quad(X, TYPE, R),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            !verdict.consistent,
            "hasValue plus maxCardinality 0 must be inconsistent"
        );
        assert!(verdict.gaps.is_empty(), "hasValue/cardinality are decided");
        assert!(
            verdict.coverage.present.contains(&"hasValue".to_owned())
                && verdict
                    .coverage
                    .present
                    .contains(&"maxCardinality".to_owned()),
            "coverage records hasValue and maxCardinality: {:?}",
            verdict.coverage
        );
    }
}
