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
const OWL_SAME_AS: &str = "http://www.w3.org/2002/07/owl#sameAs";
const OWL_PROPERTY_CHAIN_AXIOM: &str = "http://www.w3.org/2002/07/owl#propertyChainAxiom";
const OWL_INVERSE_OF: &str = "http://www.w3.org/2002/07/owl#inverseOf";
const OWL_TRANSITIVE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#TransitiveProperty";
const OWL_SYMMETRIC_PROPERTY: &str = "http://www.w3.org/2002/07/owl#SymmetricProperty";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const XSD_NON_NEGATIVE_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#nonNegativeInteger";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

const OWL_THING: &str = "http://www.w3.org/2002/07/owl#Thing";
const OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
const OWL_BOTTOM_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#bottomObjectProperty";
const OWL_BOTTOM_DATA_PROPERTY: &str = "http://www.w3.org/2002/07/owl#bottomDataProperty";
const OWL_HAS_KEY: &str = "http://www.w3.org/2002/07/owl#hasKey";
const OWL_FUNCTIONAL_PROPERTY: &str = "http://www.w3.org/2002/07/owl#FunctionalProperty";
const OWL_NEGATIVE_PROPERTY_ASSERTION: &str =
    "http://www.w3.org/2002/07/owl#NegativePropertyAssertion";
const OWL_SOURCE_INDIVIDUAL: &str = "http://www.w3.org/2002/07/owl#sourceIndividual";
const OWL_ASSERTION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#assertionProperty";
const OWL_TARGET_INDIVIDUAL: &str = "http://www.w3.org/2002/07/owl#targetIndividual";
const OWL_TARGET_VALUE: &str = "http://www.w3.org/2002/07/owl#targetValue";

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
// here.
//
// `owl:intersectionOf` is deliberately NOT listed in CONSTRUCT_COVERAGE and is
// therefore never placed in `DlCoverage::present`. This is intentional and
// honest: the EL/RL-positive path (the Nemo chase + `EL_RULES`) already
// materialises conjunction via standard subclass-propagation rules — every
// class expression `C ≡ (A ⊓ B)` in the bundle is expressed as
// `C ⊑ A`, `C ⊑ B` (plus the EL rules for the converse), so the subsumption
// closure is genuinely complete for the intersection pattern without any
// post-pass arm. Adding `owl:intersectionOf` to the inventory would force every
// intersection instance through the classifier, which would correctly return
// `decided` — but only because the *EL/RL path* already decided it. Listing it
// here would obscure WHICH path is responsible. The coverage instrument tracks
// what THIS DL post-pass decides; EL/RL coverage is the EL engine's concern.
// Concretely: `make maint-classic-cross-check` and the frozen HermiT conformance
// gold (`tests/conformance/`) both pass with this omission, confirming the
// conjunction instances in the committed bundle are fully decided by the
// EL/RL path. If a future bundle introduces an intersection pattern the EL/RL
// path cannot handle, the HermiT conformance gate will catch the regression.
const OWL_UNION_OF: &str = "http://www.w3.org/2002/07/owl#unionOf";

/// The #697 construct families this module *inventories* in the committed
/// bundle (the `(iri, qname, suffix)` triples scanned by [`scan_coverage`]).
///
/// IMPORTANT — presence in this table is **inventory, not a coverage claim.**
/// A construct's IRI appearing in the bundle places it in
/// [`DlCoverage::present`]; whether it is also [`DlCoverage::decided`] is
/// decided *empirically* by [`classify_coverage`], which inspects the actual
/// instances and the honesty of the corresponding post-pass handler. Some
/// families here (notably `owl:someValuesFrom` and the cardinality family) are
/// only *inert* in the native post-pass: they do no existential generation and
/// only fire on already-degenerate inputs, so they are reported `unsupported`
/// (Gap B) rather than silently relabelled as covered.
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
    (
        OWL_BOTTOM_OBJECT_PROPERTY,
        "owl:bottomObjectProperty",
        "bottomObjectProperty",
    ),
    (
        OWL_BOTTOM_DATA_PROPERTY,
        "owl:bottomDataProperty",
        "bottomDataProperty",
    ),
    (OWL_HAS_KEY, "owl:hasKey", "hasKey"),
    (
        OWL_NEGATIVE_PROPERTY_ASSERTION,
        "owl:NegativePropertyAssertion",
        "negativePropertyAssertion",
    ),
    (
        OWL_FUNCTIONAL_PROPERTY,
        "owl:FunctionalProperty",
        "functionalProperty",
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
/// `present` is the set of issue-#697 construct families whose IRI appears in
/// the input bundle. `decided` is the subset the native Docker-free reasoner
/// can **genuinely** decide the consistency consequences of — i.e. every
/// present instance either produced its defined consequence or is provably
/// complete by construction (see [`classify_coverage`]). `unsupported` is
/// `present \ decided`: a present construct the native path cannot honestly
/// decide. Callers surface `unsupported` through [`DlVerdict::gaps`] and gates
/// fail on it.
///
/// Honesty doctrine: a match-arm existing for a construct is **not** sufficient
/// for `decided`. An inert or incomplete handler (e.g. `owl:someValuesFrom`
/// that does no existential generation, or a cardinality clash that only fires
/// under an explicit `owl:differentFrom`/`max 0`) leaves its construct
/// `unsupported`. Coverage tells the truth; closing those gaps for real is a
/// separate step (Gap B).
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

/// A comparison key for an RDF term that distinguishes resources from literals
/// and distinguishes literals by their full value (lexical form + datatype +
/// language tag). Two terms are the SAME OWL value iff their keys are equal.
///
/// Resources use a `R\u{1f}<iri>` prefix; literals use a `L\u{1f}…` prefix so a
/// literal can never collide with a resource of the same spelling. The literal
/// key folds lexical form, datatype, and language tag together because the
/// equality / negative-assertion / key checks below are literal-aware and must
/// treat `"5"^^xsd:integer` and `"5"^^xsd:string` as distinct values.
fn term_value_key(term: &RdfTerm) -> String {
    match term {
        RdfTerm::Iri(iri) => format!("R\u{1f}{iri}"),
        RdfTerm::BlankNode(id) => format!("R\u{1f}{}", skolem_iri(id)),
        RdfTerm::Literal(lit) => format!(
            "L\u{1f}{}\u{1f}{}\u{1f}{}",
            lit.lexical_form,
            lit.datatype.as_deref().unwrap_or(""),
            lit.language.as_deref().unwrap_or("")
        ),
        RdfTerm::Triple(_) => "T\u{1f}".to_owned(),
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
    // Track which nodes appear as the `rdf:rest` target of another node — these
    // are interior nodes, not heads. Only nodes NOT in this set are heads.
    let mut rest_targets: HashSet<(String, String)> = HashSet::new();
    for (subject, predicate, object, world) in quads_by_subject(edb) {
        match predicate.as_str() {
            RDF_FIRST => {
                if let Some(value) = term_resource_key(&object) {
                    first.insert((world, subject), value);
                }
            }
            RDF_REST => {
                if let Some(value) = term_resource_key(&object) {
                    // Record this node's rest-target so we can exclude interior
                    // sublists from being mistaken for heads.
                    if value != RDF_NIL {
                        rest_targets.insert((world.clone(), value.clone()));
                    }
                    rest.insert((world, subject), value);
                }
            }
            _ => {}
        }
    }

    // Single forward pass: only walk from true list heads (nodes that have a
    // `rdf:first` but are NOT pointed to by any `rdf:rest` — i.e. are not
    // interior nodes of another list). Each node in the EDB is visited at most
    // once across the whole loop, giving O(L) total rather than O(L²).
    let mut out: HashMap<(String, String), Vec<String>> = HashMap::new();
    for (key, head_value) in &first {
        let (world, head_node) = key;
        if rest_targets.contains(&(world.clone(), head_node.clone())) {
            // This node is an interior node of a longer list; its sublist will
            // be reachable when we walk from the true head.
            continue;
        }
        // Walk the chain from this head to rdf:nil, collecting members.
        let mut node = head_node.clone();
        let mut seen: HashSet<String> = HashSet::new();
        let mut members = Vec::new();
        members.push(head_value.clone());
        seen.insert(node.clone());
        while let Some(next) = rest.get(&(world.clone(), node.clone())) {
            if next == RDF_NIL || !seen.insert(next.clone()) {
                break;
            }
            if let Some(value) = first.get(&(world.clone(), next.clone())) {
                members.push(value.clone());
            }
            node = next.clone();
        }
        out.insert((world.clone(), head_node.clone()), members);
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

/// A scoped Skolem witness IRI for an existential filler — the chase's
/// value-invention (tuple-generating dependency) discipline (SEMANTICS:27,
/// LOGIC-IR.md:66-72).
///
/// The witness identity is a *deterministic, content-addressed* function of the
/// scope `(world, property, filler_class, ordinal)` — deliberately **not** the
/// parent individual. This is the **termination guarantee** (restricted-chase
/// blocking by class-set, the canonical approach in #697): an obligation
/// `≥n p.D` in `world` always discharges to the *same* `n` witnesses
/// `w₀…wₙ₋₁` regardless of which individual raised it, so a cyclic axiom like
/// `D ⊑ ∃p.D` reuses the witness it already invented (`p(w_D, w_D')` where
/// `w_D'` already exists) instead of inventing a fresh chain — the witness pool
/// per `(world, property, filler_class)` is finite, so [`add_inferred_fact`]'s
/// `BTreeSet::insert` saturates and the fixpoint loop terminates. Distinct
/// ordinals yield distinct IRIs, giving the `n` distinct fillers a `≥n`
/// obligation needs.
///
/// We reuse the project Skolem namespace ([`crate::encode::SKOLEM_PREFIX`]) so the
/// witness is indistinguishable from a Skolemized blank node downstream.
fn witness_iri(world: &str, property: &str, filler_class: &str, ordinal: usize) -> String {
    let key = format!("dl-exists\u{1f}{world}\u{1f}{property}\u{1f}{filler_class}\u{1f}{ordinal}");
    skolem_iri(&key)
}

/// True iff `a` and `b` are provably distinct individuals under the bundle's
/// identity stance.
///
/// The native path adopts the standard well-behaved policy declared as a
/// unique-name *contract assumption* (SEMANTICS:368, "two distinct values via an
/// inequality guard", SEMANTICS:488): two **named** resources with different IRIs
/// are distinct unless explicitly merged by `owl:sameAs`. Chase witnesses carry
/// fresh content-addressed IRIs and are therefore distinct from each other and
/// from every named individual by construction — unless a `sameAs` fact merges
/// them. An explicit `owl:differentFrom` also establishes distinctness (and is
/// honored even were a same-IRI pathology to arise).
fn distinct_individuals(facts: &BTreeSet<Fact>, world: &str, a: &str, b: &str) -> bool {
    if a == b {
        return false;
    }
    // An explicit `owl:differentFrom` is a hard distinctness assertion and wins
    // even over a (contradictory) `owl:sameAs`.
    if has_fact(facts, world, a, OWL_DIFFERENT_FROM, b)
        || has_fact(facts, world, b, OWL_DIFFERENT_FROM, a)
    {
        return true;
    }
    if has_fact(facts, world, a, OWL_SAME_AS, b) || has_fact(facts, world, b, OWL_SAME_AS, a) {
        return false;
    }
    true
}

/// True iff the `fillers` are pairwise distinct under the bundle's identity
/// stance (UNA + `owl:sameAs` merges + explicit `owl:differentFrom`), i.e. they
/// genuinely witness `>n` distinct property values without needing an explicit
/// `owl:differentFrom` between every pair.
fn pairwise_distinct(facts: &BTreeSet<Fact>, world: &str, fillers: &[String]) -> bool {
    for i in 0..fillers.len() {
        for j in (i + 1)..fillers.len() {
            if !distinct_individuals(facts, world, &fillers[i], &fillers[j]) {
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

/// The existential-filler obligations a restriction imposes on each of its
/// instances: `(needed, filler_class)` where `needed` distinct property fillers
/// of type `filler_class` (or `⊤` when `None`) must exist.
///
/// `someValuesFrom D` is the qualified `≥1 p.D`; the cardinality *minima*
/// (`cardinality`, `minCardinality`, `qualifiedCardinality`,
/// `minQualifiedCardinality`) contribute their `≥n` lower bound, qualified by
/// `onClass` when present. These are the obligations the chase discharges by
/// inventing scoped Skolem witnesses ([`witness_iri`]).
fn existential_obligations(restriction: &Restriction) -> Vec<(usize, Option<&str>)> {
    let mut obligations: Vec<(usize, Option<&str>)> = Vec::new();
    if let Some(class) = restriction.some_values_from.as_deref() {
        obligations.push((1, Some(class)));
    }
    for (n, on_class) in cardinality_minima(restriction) {
        if n > 0 {
            obligations.push((n, on_class));
        }
    }
    obligations
}

/// Add DL-only finite consistency consequences to the closure.
///
/// The RL generic-triple engine owns positive entailment. This pass owns the DL
/// checks that are not natural positive RL facts: complement/disjoint-union
/// clashes, unsatisfiable restrictions, existential value-invention (the chase),
/// and cardinality clashes under the bundle's identity stance.
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
                    // Nominal closure: `C ≡ oneOf(m1..mk)` means every instance of
                    // `C` is one of the members. So an individual typed `C` that is
                    // asserted **explicitly distinct** (`owl:differentFrom`) from
                    // *every* member can be no member at all and is forced into
                    // `owl:Nothing` — an inconsistency. This is the closure half of
                    // `oneOf` (the member→type direction above is the easy half); it
                    // terminates because the enumeration is finite and no witnesses
                    // are invented. (Issue #697 Gap G: the frozen HermiT gold caught
                    // this clash; native must too — native ⊇ oracle.)
                    //
                    // Distinctness here is the **explicit** stance only (asserted
                    // `owl:differentFrom`), NOT the UNA default the cardinality
                    // anti-merge uses: standard OWL does not assume unique names, so
                    // an instance of a `oneOf` class merely *named* differently from
                    // the members can still be `owl:sameAs` one of them and is
                    // consistent. Requiring explicit `differentFrom` keeps native
                    // faithful to the oracle (no false inconsistency on a
                    // non-UNA-consistent ontology) — soundness over an over-eager
                    // superset.
                    let one_of_class = fact.subject.clone();
                    let instances: Vec<String> = facts
                        .iter()
                        .filter(|f| {
                            f.predicate == RDF_TYPE
                                && f.object == one_of_class
                                && f.world == fact.world
                        })
                        .map(|f| f.subject.clone())
                        .collect();
                    for instance in instances {
                        // Skip the members themselves: a member IS in the class.
                        if members.iter().any(|m| m == &instance) {
                            continue;
                        }
                        let distinct_from_all = members.iter().all(|m| {
                            has_fact(&facts, &fact.world, &instance, OWL_DIFFERENT_FROM, m)
                                || has_fact(&facts, &fact.world, m, OWL_DIFFERENT_FROM, &instance)
                        });
                        if !distinct_from_all {
                            continue;
                        }
                        let mut premises: Vec<(String, String, String)> = vec![
                            (
                                one_of_class.clone(),
                                OWL_ONE_OF.to_owned(),
                                fact.object.clone(),
                            ),
                            (instance.clone(), RDF_TYPE.to_owned(), one_of_class.clone()),
                        ];
                        for m in members {
                            premises.push((
                                instance.clone(),
                                OWL_DIFFERENT_FROM.to_owned(),
                                m.clone(),
                            ));
                        }
                        premises.sort();
                        add_inferred_fact(
                            inferred,
                            &mut facts,
                            Fact::new(
                                instance.clone(),
                                RDF_TYPE.to_owned(),
                                OWL_NOTHING.to_owned(),
                                fact.world.clone(),
                            ),
                            "dl:oneOf-closure-clash",
                            premises,
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
        // perf: index rebuilt each fixpoint iter; incremental update tracked under #630
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
                            super_property.clone(),
                            fact.object.clone(),
                            world.clone(),
                        ),
                        "dl:subPropertyOf-propagation",
                        vec![
                            // schema axiom: predicate ⊑ super_property
                            (
                                fact.predicate.clone(),
                                RDFS_SUBPROPERTYOF.to_owned(),
                                super_property,
                            ),
                            // source assertion: s predicate o
                            (
                                fact.subject.clone(),
                                fact.predicate.clone(),
                                fact.object.clone(),
                            ),
                        ],
                    );
                }

                for domain in objects_for(&index, world, &fact.predicate, RDFS_DOMAIN) {
                    add_inferred_fact(
                        inferred,
                        &mut facts,
                        Fact::new(
                            fact.subject.clone(),
                            RDF_TYPE.to_owned(),
                            domain.clone(),
                            world.clone(),
                        ),
                        "dl:domain",
                        vec![
                            // schema axiom: predicate rdfs:domain domain
                            (fact.predicate.clone(), RDFS_DOMAIN.to_owned(), domain),
                            // source assertion: s predicate o
                            (
                                fact.subject.clone(),
                                fact.predicate.clone(),
                                fact.object.clone(),
                            ),
                        ],
                    );
                }

                for range in objects_for(&index, world, &fact.predicate, RDFS_RANGE) {
                    add_inferred_fact(
                        inferred,
                        &mut facts,
                        Fact::new(
                            fact.object.clone(),
                            RDF_TYPE.to_owned(),
                            range.clone(),
                            world.clone(),
                        ),
                        "dl:range",
                        vec![
                            // schema axiom: predicate rdfs:range range
                            (fact.predicate.clone(), RDFS_RANGE.to_owned(), range),
                            // source assertion: s predicate o
                            (
                                fact.subject.clone(),
                                fact.predicate.clone(),
                                fact.object.clone(),
                            ),
                        ],
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
                            inverse.clone(),
                            fact.subject.clone(),
                            world.clone(),
                        ),
                        "dl:inverseOf",
                        vec![
                            // schema axiom: predicate owl:inverseOf inverse
                            (fact.predicate.clone(), OWL_INVERSE_OF.to_owned(), inverse),
                            // source assertion: s predicate o
                            (
                                fact.subject.clone(),
                                fact.predicate.clone(),
                                fact.object.clone(),
                            ),
                        ],
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

                    // ── max-cardinality / exact clash under the identity stance ──
                    // A clash needs `> max` fillers that must be *distinct* given
                    // the bundle's identity facts. Anti-merge: under the declared
                    // UNA assumption, named individuals with different IRIs are
                    // distinct unless `owl:sameAs`-merged, so no explicit
                    // `owl:differentFrom` is required (Gap B). `max == 0` clashes
                    // on a single filler regardless.
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
                            && (max == 0 || pairwise_distinct(&facts, world, &counted))
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

                    // ── existential value-invention (the chase) ─────────────────
                    // For each existential obligation `≥needed p.D` whose distinct
                    // qualifying fillers fall short, invent scoped Skolem witnesses
                    // (TGD value-invention) and assert `p(x,w)` + `type(w,D)`. The
                    // witness identity is content-addressed on
                    // (world,x,property,class,ordinal) — so re-firing re-derives
                    // the SAME witness and the fixpoint terminates (no regenerated
                    // anonymous individuals). The all-values / disjoint clash rules
                    // then saturate the witness and surface ∃p.C ⊓ ∀p.D clashes.
                    for (needed, on_class) in existential_obligations(restriction) {
                        let filler_class = on_class.unwrap_or(OWL_NOTHING);
                        // Count the distinct qualifying fillers x already has.
                        let qualifying: Vec<String> = match on_class {
                            Some(class) => fillers
                                .iter()
                                .filter(|filler| has_fact(&facts, world, filler, RDF_TYPE, class))
                                .cloned()
                                .collect(),
                            // Unqualified `≥n p.⊤`: any filler counts.
                            None => fillers.clone(),
                        };
                        if !pairwise_distinct(&facts, world, &qualifying) {
                            // Existing fillers are not provably distinct (a
                            // `sameAs` merge collapses them); the obligation may be
                            // met by overlap, so do not over-invent.
                            continue;
                        }
                        let have = qualifying.len();
                        if have >= needed {
                            continue;
                        }
                        for ordinal in have..needed {
                            let witness = witness_iri(world, property, filler_class, ordinal);
                            // p(x, witness)
                            add_inferred_fact(
                                inferred,
                                &mut facts,
                                Fact::new(
                                    subject.clone(),
                                    property.to_owned(),
                                    witness.clone(),
                                    world.clone(),
                                ),
                                "dl:exists-witness-edge",
                                vec![(
                                    restriction_key.clone(),
                                    OWL_ON_PROPERTY.to_owned(),
                                    property.to_owned(),
                                )],
                            );
                            // type(witness, D) — only for a qualified obligation;
                            // an unqualified `≥n p.⊤` invents an untyped witness.
                            if let Some(class) = on_class {
                                add_inferred_fact(
                                    inferred,
                                    &mut facts,
                                    Fact::new(
                                        witness.clone(),
                                        RDF_TYPE.to_owned(),
                                        class.to_owned(),
                                        world.clone(),
                                    ),
                                    "dl:exists-witness-type",
                                    vec![(
                                        restriction_key.clone(),
                                        OWL_SOME_VALUES_FROM.to_owned(),
                                        class.to_owned(),
                                    )],
                                );
                            }
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

    augment_with_extra_dl_clashes(inferred, &mut facts, &restrictions, edb);

    Ok(())
}

/// The literal-aware DL clashes the resource-only [`Fact`] closure cannot see.
///
/// Five direct, sound consistency contradictions are layered here, each of which
/// asserts `type(x, owl:Nothing)` (the inconsistency witness the verdict reads
/// off). All are EDB-direct (no value invention, no fixpoint dependence) and run
/// after the main closure so they observe every propagated `rdf:type` fact:
///
/// 1. **`owl:Thing` forced empty** — `owl:Thing ⊑ owl:Nothing` or
///    `owl:Thing ≡ owl:Nothing`. The extension of `owl:Thing` is never empty, so
///    forcing it into `owl:Nothing` is inconsistent. A scoped Skolem witness
///    (`owl:Thing`'s mandatory inhabitant) is forced into `owl:Nothing`.
/// 2. **Empty bottom property has a value** — any individual typed a restriction
///    `∃p.X` (or `≥1 p`) where `p` is `owl:bottomObjectProperty` /
///    `owl:bottomDataProperty`. The bottom property's extension is empty, so an
///    obligation to have a value on it is unsatisfiable.
/// 3. **Negative property assertion contradicted** —
///    `NegativePropertyAssertion(source, p, target)` co-present with the positive
///    `p(source, target)` (object via `owl:targetIndividual`, data via
///    `owl:targetValue`). A direct contradiction.
/// 4. **Functional property with two distinct values** — `p` a
///    `owl:FunctionalProperty` with `p(x, v1)` and `p(x, v2)` for provably
///    distinct `v1`, `v2` (distinct literals, or distinct individuals under the
///    identity stance). The two values are forced equal yet provably distinct.
/// 5. **Key axiom collision** — `owl:hasKey(C, [k1..kn])` with two instances of
///    `C` agreeing on every key value yet asserted `owl:differentFrom`. The key
///    forces them equal; the explicit distinctness clashes.
fn augment_with_extra_dl_clashes(
    inferred: &mut Vec<InferredAxiom>,
    facts: &mut BTreeSet<Fact>,
    restrictions: &HashMap<(String, String), Restriction>,
    edb: &RdfDataset,
) {
    // ── 1. owl:Thing forced into owl:Nothing ──────────────────────────────────
    // owl:Thing ⊑ owl:Nothing (asserted or via equivalentClass) makes the always-
    // populated top class empty — inconsistent. We materialise the mandatory
    // owl:Thing inhabitant as a scoped witness and force it into owl:Nothing.
    let thing_empty_worlds: BTreeSet<String> = facts
        .iter()
        .filter(|f| {
            f.subject == OWL_THING
                && f.object == OWL_NOTHING
                && (f.predicate == RDFS_SUBCLASSOF || f.predicate == OWL_EQUIVALENT_CLASS)
        })
        .map(|f| f.world.clone())
        .collect();
    for world in thing_empty_worlds {
        let witness = witness_iri(&world, OWL_THING, OWL_THING, 0);
        add_inferred_fact(
            inferred,
            facts,
            Fact::new(
                witness,
                RDF_TYPE.to_owned(),
                OWL_NOTHING.to_owned(),
                world.clone(),
            ),
            "dl:thing-empty-clash",
            vec![(
                OWL_THING.to_owned(),
                RDFS_SUBCLASSOF.to_owned(),
                OWL_NOTHING.to_owned(),
            )],
        );
    }

    // ── 2. Empty bottom property forced to have a value ───────────────────────
    // An individual typed a restriction on owl:bottomObjectProperty /
    // owl:bottomDataProperty that obligates ≥1 value (someValuesFrom, or a
    // cardinality / qualified minimum ≥ 1) cannot be satisfied: the bottom
    // property's extension is empty.
    for ((restriction_world, restriction_node), restriction) in restrictions {
        let Some(property) = restriction.on_property.as_deref() else {
            continue;
        };
        if property != OWL_BOTTOM_OBJECT_PROPERTY && property != OWL_BOTTOM_DATA_PROPERTY {
            continue;
        }
        let obligates_value = restriction.some_values_from.is_some()
            || restriction.has_value.is_some()
            || existential_obligations(restriction)
                .iter()
                .any(|(n, _)| *n >= 1);
        if !obligates_value {
            continue;
        }
        let instances: Vec<String> = facts
            .iter()
            .filter(|f| {
                f.world == *restriction_world
                    && f.predicate == RDF_TYPE
                    && f.object == *restriction_node
            })
            .map(|f| f.subject.clone())
            .collect();
        for instance in instances {
            add_inferred_fact(
                inferred,
                facts,
                Fact::new(
                    instance,
                    RDF_TYPE.to_owned(),
                    OWL_NOTHING.to_owned(),
                    restriction_world.clone(),
                ),
                "dl:bottom-property-clash",
                vec![(
                    restriction_node.clone(),
                    OWL_ON_PROPERTY.to_owned(),
                    property.to_owned(),
                )],
            );
        }
    }

    // ── 3. Negative property assertion contradicted by its positive ───────────
    // Reify owl:NegativePropertyAssertion(source, p, target) from its RDF shape
    // and clash it against the literal-aware positive p(source, target).
    let value_index = build_value_index(edb);
    for npa in read_negative_property_assertions(edb) {
        let positive = value_index
            .get(&(npa.world.clone(), npa.source.clone(), npa.property.clone()))
            .map(|s| s.contains_key(&npa.target))
            .unwrap_or(false);
        if positive {
            add_inferred_fact(
                inferred,
                facts,
                Fact::new(
                    npa.source.clone(),
                    RDF_TYPE.to_owned(),
                    OWL_NOTHING.to_owned(),
                    npa.world.clone(),
                ),
                "dl:negative-property-assertion-clash",
                vec![(
                    npa.source.clone(),
                    npa.property.clone(),
                    OWL_NEGATIVE_PROPERTY_ASSERTION.to_owned(),
                )],
            );
        }
    }

    // ── 4. Functional property with two provably-distinct values ──────────────
    // For every owl:FunctionalProperty p and subject x, if x has two values on p
    // that are provably distinct (distinct literals, or distinct named
    // individuals under the identity stance), x is forced into owl:Nothing.
    let functional_props: BTreeSet<(String, String)> = facts
        .iter()
        .filter(|f| f.predicate == RDF_TYPE && f.object == OWL_FUNCTIONAL_PROPERTY)
        .map(|f| (f.world.clone(), f.subject.clone()))
        .collect();
    for (world, property) in &functional_props {
        // Group this property's (subject -> distinct value-keys) in this world.
        let mut by_subject: BTreeMap<String, BTreeMap<String, RdfTerm>> = BTreeMap::new();
        for (key, terms) in &value_index {
            let (w, subject, pred) = key;
            if w != world || pred != property {
                continue;
            }
            by_subject
                .entry(subject.clone())
                .or_default()
                .extend(terms.iter().map(|(k, t)| (k.clone(), t.clone())));
        }
        for (subject, values) in by_subject {
            if values.len() < 2 {
                continue;
            }
            if functional_values_clash(facts, world, &values) {
                add_inferred_fact(
                    inferred,
                    facts,
                    Fact::new(
                        subject,
                        RDF_TYPE.to_owned(),
                        OWL_NOTHING.to_owned(),
                        world.clone(),
                    ),
                    "dl:functional-property-clash",
                    vec![(
                        property.clone(),
                        RDF_TYPE.to_owned(),
                        OWL_FUNCTIONAL_PROPERTY.to_owned(),
                    )],
                );
            }
        }
    }

    // ── 5. Key-axiom collision ────────────────────────────────────────────────
    // owl:hasKey(C, [k1..kn]); two instances of C agreeing on every key value yet
    // explicitly owl:differentFrom are forced into owl:Nothing.
    let lists = read_lists(edb);
    for (world, key_class, key_props) in read_key_axioms(edb, &lists) {
        // Members of C in this world. owl:Thing has every individual as a member,
        // so for a key on owl:Thing every individual that bears the key props
        // counts (the trivial-inconsistency case from the OWL primer).
        let instances: Vec<String> = collect_key_subjects(facts, &value_index, &world, &key_class);
        for i in 0..instances.len() {
            for j in (i + 1)..instances.len() {
                let a = &instances[i];
                let b = &instances[j];
                // Explicit distinctness is required: standard OWL does not assume
                // unique names, so two key-agreeing instances merely named
                // differently are owl:sameAs and consistent. Only an explicit
                // owl:differentFrom makes the key collision a contradiction.
                if !has_fact(facts, &world, a, OWL_DIFFERENT_FROM, b)
                    && !has_fact(facts, &world, b, OWL_DIFFERENT_FROM, a)
                {
                    continue;
                }
                if key_values_agree(&value_index, &world, a, b, &key_props) {
                    let premises = vec![
                        (key_class.clone(), OWL_HAS_KEY.to_owned(), key_class.clone()),
                        (a.clone(), OWL_DIFFERENT_FROM.to_owned(), b.clone()),
                    ];
                    add_inferred_fact(
                        inferred,
                        facts,
                        Fact::new(
                            a.clone(),
                            RDF_TYPE.to_owned(),
                            OWL_NOTHING.to_owned(),
                            world.clone(),
                        ),
                        "dl:has-key-clash",
                        premises,
                    );
                }
            }
        }
    }
}

/// A reified `owl:NegativePropertyAssertion`.
struct NegativeAssertion {
    world: String,
    source: String,
    property: String,
    /// The target's value key ([`term_value_key`]), object or literal.
    target: String,
}

/// Reify every `owl:NegativePropertyAssertion` from its RDF shape
/// (`owl:sourceIndividual` / `owl:assertionProperty` /
/// `owl:targetIndividual` | `owl:targetValue`). Incomplete assertions (missing a
/// component) are skipped — a malformed NPA asserts nothing.
fn read_negative_property_assertions(edb: &RdfDataset) -> Vec<NegativeAssertion> {
    // (world, npa_node) → partial components.
    let mut source: HashMap<(String, String), String> = HashMap::new();
    let mut property: HashMap<(String, String), String> = HashMap::new();
    let mut target: HashMap<(String, String), String> = HashMap::new();
    let mut is_npa: HashSet<(String, String)> = HashSet::new();

    for RdfQuad {
        subject,
        predicate,
        object,
        graph_name,
        ..
    } in edb.owned_quads()
    {
        let Some(subject) = term_resource_key(&subject) else {
            continue;
        };
        let world = graph_world_key(&graph_name);
        let key = (world, subject);
        match predicate.as_str() {
            RDF_TYPE => {
                if matches!(&object, RdfTerm::Iri(o) if o == OWL_NEGATIVE_PROPERTY_ASSERTION) {
                    is_npa.insert(key);
                }
            }
            OWL_SOURCE_INDIVIDUAL => {
                if let Some(v) = term_resource_key(&object) {
                    source.insert(key, v);
                }
            }
            OWL_ASSERTION_PROPERTY => {
                if let Some(v) = term_resource_key(&object) {
                    property.insert(key, v);
                }
            }
            OWL_TARGET_INDIVIDUAL | OWL_TARGET_VALUE => {
                target.insert(key, term_value_key(&object));
            }
            _ => {}
        }
    }

    let mut out = Vec::new();
    for key in is_npa {
        let (Some(s), Some(p), Some(t)) = (source.get(&key), property.get(&key), target.get(&key))
        else {
            continue;
        };
        out.push(NegativeAssertion {
            world: key.0.clone(),
            source: s.clone(),
            property: p.clone(),
            target: t.clone(),
        });
    }
    out
}

/// A literal-aware value index `(world, subject, predicate) → {value_key → term}`.
/// Unlike the resource-only [`Fact`] index, this retains literal objects so the
/// negative-assertion, functional-property, and key checks can compare data
/// values. The value key is [`term_value_key`].
#[allow(clippy::type_complexity)]
fn build_value_index(
    edb: &RdfDataset,
) -> HashMap<(String, String, String), BTreeMap<String, RdfTerm>> {
    let mut index: HashMap<(String, String, String), BTreeMap<String, RdfTerm>> = HashMap::new();
    for RdfQuad {
        subject,
        predicate,
        object,
        graph_name,
        ..
    } in edb.owned_quads()
    {
        let Some(subject) = term_resource_key(&subject) else {
            continue;
        };
        let world = graph_world_key(&graph_name);
        index
            .entry((world, subject, predicate))
            .or_default()
            .insert(term_value_key(&object), object);
    }
    index
}

/// True iff the functional-property value set contains two values that are
/// provably distinct: any two distinct literals, or two distinct named resources
/// that are not `owl:sameAs`-merged. (Distinct value keys for two literals are
/// always genuinely distinct OWL values; for resources we defer to the identity
/// stance so a `sameAs` merge does not trigger a false clash.)
fn functional_values_clash(
    facts: &BTreeSet<Fact>,
    world: &str,
    values: &BTreeMap<String, RdfTerm>,
) -> bool {
    let entries: Vec<&RdfTerm> = values.values().collect();
    for i in 0..entries.len() {
        for j in (i + 1)..entries.len() {
            let (a, b) = (entries[i], entries[j]);
            match (a, b) {
                // Two literals with different value keys are distinct values.
                (RdfTerm::Literal(_), _) | (_, RdfTerm::Literal(_)) => return true,
                _ => {
                    let (Some(ak), Some(bk)) = (term_resource_key(a), term_resource_key(b)) else {
                        continue;
                    };
                    if distinct_individuals(facts, world, &ak, &bk) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Reify every `owl:hasKey(C, list)` axiom into `(world, class, key_props)`,
/// resolving the key-property RDF list. A dangling/empty list yields no axiom.
fn read_key_axioms(
    edb: &RdfDataset,
    lists: &HashMap<(String, String), Vec<String>>,
) -> Vec<(String, String, Vec<String>)> {
    let mut out = Vec::new();
    for (subject, predicate, object, world) in quads_by_subject(edb) {
        if predicate != OWL_HAS_KEY {
            continue;
        }
        let Some(root) = term_resource_key(&object) else {
            continue;
        };
        let Some(members) = lists.get(&(world.clone(), root)) else {
            continue;
        };
        if members.is_empty() {
            continue;
        }
        out.push((world, subject, members.clone()));
    }
    out
}

/// Collect the candidate key subjects for a key on `class` in `world`: the
/// individuals asserted `rdf:type class`. `owl:Thing` keys every individual, so
/// for a key on `owl:Thing` we take every subject that appears in the value index
/// (every resource that bears at least one property) — the trivial-inconsistency
/// case from the OWL specification.
fn collect_key_subjects(
    facts: &BTreeSet<Fact>,
    value_index: &HashMap<(String, String, String), BTreeMap<String, RdfTerm>>,
    world: &str,
    class: &str,
) -> Vec<String> {
    let mut set: BTreeSet<String> = BTreeSet::new();
    if class == OWL_THING {
        for (w, subject, _pred) in value_index.keys() {
            if w == world {
                set.insert(subject.clone());
            }
        }
    } else {
        for fact in facts {
            if fact.world == *world && fact.predicate == RDF_TYPE && fact.object == *class {
                set.insert(fact.subject.clone());
            }
        }
    }
    set.into_iter().collect()
}

/// True iff `a` and `b` agree on every key property: for each key property each
/// must bear at least one value and share at least one common value. (OWL key
/// semantics: two instances with the same value for every key property are the
/// same individual; agreement requires a shared value per key property.)
fn key_values_agree(
    value_index: &HashMap<(String, String, String), BTreeMap<String, RdfTerm>>,
    world: &str,
    a: &str,
    b: &str,
    key_props: &[String],
) -> bool {
    for prop in key_props {
        let av = value_index.get(&(world.to_owned(), a.to_owned(), prop.clone()));
        let bv = value_index.get(&(world.to_owned(), b.to_owned(), prop.clone()));
        let (Some(av), Some(bv)) = (av, bv) else {
            return false;
        };
        if av.keys().all(|k| !bv.contains_key(k)) {
            return false;
        }
    }
    true
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
    // This is the verdict-only entry point. The single-chase pipeline
    // (run_reasoning → augment_inferred_with_dl → sort → verdict_from_inferred)
    // lives in [`crate::reason::reason_closure`]; we run it and keep only the
    // verdict, dropping the closure callers of this function do not need. Sharing
    // the one pipeline guarantees the verdict here is bit-for-bit the verdict the
    // typed `reason_all` result is folded from (the sort is closure-ordering only
    // and does not change which clash facts are derived).
    Ok(crate::reason::reason_closure(edb)?.1)
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
    }
    // The unsatisfiable (empty, unpopulated) classes are a separate scan, shared
    // with the typed-result fold (#768) via [`unsatisfiable_from_inferred`].
    let unsatisfiable_classes = unsatisfiable_from_inferred(inferred);

    // Only a populated clash (an individual in owl:Nothing) makes the ontology
    // inconsistent; an unsatisfiable-but-unpopulated class does not.
    let consistent = inconsistencies.is_empty();

    let coverage = scan_coverage(edb)?;
    let gaps = gaps_from_unsupported(&coverage.unsupported);

    Ok(DlVerdict {
        consistent,
        unsatisfiable_classes,
        inconsistencies,
        coverage,
        gaps,
    })
}

/// Scan a native closure for the unsatisfiable (provably-empty, unpopulated)
/// classes: every `subClassOf(?c, owl:Nothing, ?w)` with `?c` not `owl:Nothing`
/// itself. An unsatisfiable class does **not** by itself make the ontology
/// inconsistent (the module distinction).
///
/// Factored out so the typed `logic:ReasoningResult` fold (#768) can recover the
/// same DL diagnostic from the shared closure payload without re-running the
/// chase, byte-identically with [`verdict_from_inferred`].
pub fn unsatisfiable_from_inferred(inferred: &[InferredAxiom]) -> Vec<UnsatClass> {
    let mut unsatisfiable_classes: Vec<UnsatClass> = Vec::new();
    for ax in inferred {
        let object_iri = unwrap_iri(&ax.object);
        if ax.predicate == RDFS_SUBCLASSOF
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
    unsatisfiable_classes
}

/// Build the DL coverage-gap losses from the unsupported-construct names.
///
/// The single recipe `verdict_from_inferred`, the artifact ledger, and `verify`
/// all share, so the gap `code` (`reason.dl-gap.{name}`) and `message` stay
/// byte-identical whether a consumer reads `DlVerdict::gaps` directly or
/// reconstructs them from a typed result's
/// `preservation.unsupported_constructs` (#768).
pub fn gaps_from_unsupported<I, S>(unsupported: I) -> Vec<RdfLoss>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    unsupported
        .into_iter()
        .map(|name| {
            let name = name.as_ref();
            RdfLoss::new(
                format!("reason.dl-gap.{name}"),
                format!(
                    "{name} is present in the bundle but was not decided by the native DL path"
                ),
            )
        })
        .collect()
}

/// Scan the input `edb` for the #697 construct families and report **honest**
/// native coverage.
///
/// `present` lists the families whose IRI appears in the bundle (as a predicate
/// or as an IRI object — restriction fillers ride the object position).
/// `decided` is the subset [`classify_coverage`] proves the native post-pass
/// genuinely decides over the *actual* instances; `unsupported` is the residual
/// `present \ decided`. A present-but-undecided construct is **not** relabelled
/// as covered — it surfaces through [`DlVerdict::gaps`] and gates fail on it.
///
/// # Errors
///
/// Returns `Err(String)` if a quad cannot be read from the source store.
pub fn scan_coverage(edb: &RdfDataset) -> Result<DlCoverage, String> {
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
        if !present_iris.contains(iri) {
            continue;
        }
        present.push(suffix.to_owned());
    }
    present.sort();
    present.dedup();

    // Empirically classify which of the present families the native post-pass
    // can genuinely decide over the real instances.
    let decided_set = classify_coverage(edb, &present);
    let mut decided: Vec<String> = present
        .iter()
        .filter(|name| decided_set.contains(name.as_str()))
        .cloned()
        .collect();
    decided.sort();
    let mut unsupported: Vec<String> = present
        .iter()
        .filter(|name| !decided_set.contains(name.as_str()))
        .cloned()
        .collect();
    unsupported.sort();

    Ok(DlCoverage {
        present,
        decided,
        unsupported,
    })
}

/// Honestly classify which present construct families the native DL post-pass
/// genuinely decides, returning the set of decided family suffixes.
///
/// "Genuinely decided" means the native path can determine the construct's
/// consistency consequences over **every** present instance — not merely that a
/// match-arm exists. The classifier inspects the actual restrictions/lists in
/// the bundle (the same data [`augment_inferred_with_dl`] reads) and applies a
/// per-family honesty rule:
///
/// - `complementOf`, `domain`, `range`, `allValuesFrom`, `hasValue` — the
///   handler emits its defined consequence for every instance with no missing
///   sub-case (∀-restrictions and hasValue need no fresh individuals; domain/
///   range/complement are unconditional). Decided when present.
/// - `unionOf`, `disjointUnionOf`, `oneOf` — list-backed; decided iff **every**
///   instance resolves to a non-empty RDF list (a dangling/empty list is not
///   handled and so is not decided).
/// - `propertyChainAxiom` — the handler only composes chains of **exactly
///   length 2**; decided iff every present chain is a resolvable 2-list.
/// - `someValuesFrom` — the chase invents a scoped Skolem witness for `∃p.D`
///   (value invention, [`witness_iri`]), types it `D`, saturates it through the
///   EL/DL rules, and surfaces `∃p.C ⊓ ∀p.D` clashes. Decided iff every present
///   `∃` restriction has a resolvable `onProperty` and filler class so the
///   witness can be generated (Gap B).
/// - the cardinality family (`cardinality`, `min`/`maxCardinality`,
///   `qualifiedCardinality`, `min`/`maxQualifiedCardinality`) — minima generate
///   the required distinct witnesses (the same chase); maxima clash via counting
///   plus the **identity-stance anti-merge** (UNA with `owl:sameAs` merges and
///   `owl:differentFrom`), no longer requiring an explicit `owl:differentFrom`
///   between every pair. Decided iff every present cardinality instance has a
///   parseable bound and a resolvable `onProperty` (and `onClass` for the
///   qualified families).
fn classify_coverage(edb: &RdfDataset, present: &[String]) -> BTreeSet<String> {
    let present_set: BTreeSet<&str> = present.iter().map(String::as_str).collect();
    let restrictions = read_restrictions(edb);
    let lists = read_lists(edb);
    let mut decided: BTreeSet<String> = BTreeSet::new();

    // Unconditionally-complete families: the handler emits its consequence for
    // every instance and has no missing sub-case.
    for family in [
        "complementOf",
        "domain",
        "range",
        "allValuesFrom",
        "hasValue",
    ] {
        if present_set.contains(family) {
            decided.insert(family.to_owned());
        }
    }

    // List-backed enumeration/union families: decided iff every present instance
    // resolves to a non-empty RDF list the post-pass can walk.
    for (family, iri) in [
        ("unionOf", OWL_UNION_OF),
        ("disjointUnionOf", OWL_DISJOINT_UNION_OF),
        ("oneOf", OWL_ONE_OF),
    ] {
        if !present_set.contains(family) {
            continue;
        }
        if !all_list_instances_resolve(edb, iri, &lists) {
            continue;
        }
        // `owl:Thing oneOf …` constrains the (always-fixed, universe-sized) top
        // class to a finite enumeration. Deciding the resulting consistency
        // ("the extension of owl:Thing may not be a singleton") needs the
        // universe-cardinality argument, which is NOT a sound local enumeration
        // rule — the native post-pass only types the members as `owl:Thing`. We
        // therefore leave `oneOf` UNDECIDED whenever any instance targets
        // `owl:Thing`, surfacing it as an honest gap rather than a wrong
        // `consistent` (soundness over completeness).
        if family == "oneOf" && one_of_constrains_thing(edb) {
            continue;
        }
        decided.insert(family.to_owned());
    }

    // owl:bottomObjectProperty / owl:bottomDataProperty: the empty property. The
    // native post-pass forces any individual obligated to bear a value on the
    // bottom property (someValuesFrom / hasValue / ≥1 cardinality restriction)
    // into owl:Nothing. Decided unconditionally when present (the clash is a
    // direct, local consequence).
    for family in ["bottomObjectProperty", "bottomDataProperty"] {
        if present_set.contains(family) {
            decided.insert(family.to_owned());
        }
    }

    // owl:NegativePropertyAssertion: reified from its RDF shape and clashed
    // (literal-aware) against the positive assertion. Decided unconditionally
    // when present — a malformed NPA simply asserts nothing.
    if present_set.contains("negativePropertyAssertion") {
        decided.insert("negativePropertyAssertion".to_owned());
    }

    // owl:FunctionalProperty: a subject with two provably-distinct values on a
    // functional property is forced into owl:Nothing. Decided unconditionally
    // when present (literal-aware distinctness + the identity stance).
    if present_set.contains("functionalProperty") {
        decided.insert("functionalProperty".to_owned());
    }

    // owl:hasKey: two key-agreeing instances asserted owl:differentFrom clash.
    // Decided iff every key axiom resolves to a non-empty key-property list.
    if present_set.contains("hasKey") && all_list_instances_resolve(edb, OWL_HAS_KEY, &lists) {
        decided.insert("hasKey".to_owned());
    }

    // owl:propertyChainAxiom: only length-2 resolvable chains are composed.
    if present_set.contains("propertyChainAxiom") && all_property_chains_are_binary(edb, &lists) {
        decided.insert("propertyChainAxiom".to_owned());
    }

    // someValuesFrom: the chase invents a witness for every well-formed `∃p.D`.
    // Decided iff every present `∃` restriction has a resolvable onProperty and
    // filler class (Gap B value-invention).
    if present_set.contains("someValuesFrom")
        && all_some_values_from_instances_decidable(&restrictions)
    {
        decided.insert("someValuesFrom".to_owned());
    }

    // Cardinality family: minima generate distinct witnesses, maxima clash via
    // counting + identity-stance anti-merge. Decided iff every present instance
    // is well-formed for the relevant generation/clash (Gap B).
    for family in [
        "cardinality",
        "minCardinality",
        "maxCardinality",
        "qualifiedCardinality",
        "minQualifiedCardinality",
        "maxQualifiedCardinality",
    ] {
        if present_set.contains(family)
            && all_cardinality_instances_decidable(edb, &restrictions, family)
        {
            decided.insert(family.to_owned());
        }
    }

    decided
}

/// True iff every `owl:someValuesFrom` restriction is well-formed enough for the
/// chase to discharge it: it carries an `onProperty` and a resolvable
/// (IRI/bnode) filler class. A literal or absent filler/property is not a shape
/// the value-invention handler generates for, so it stays undecided.
fn all_some_values_from_instances_decidable(
    restrictions: &HashMap<(String, String), Restriction>,
) -> bool {
    let mut saw_instance = false;
    for restriction in restrictions.values() {
        if restriction.some_values_from.is_none() {
            continue;
        }
        saw_instance = true;
        if restriction.on_property.is_none() || restriction.some_values_from.is_none() {
            return false;
        }
    }
    saw_instance
}

/// True iff any `owl:oneOf` axiom has `owl:Thing` as its subject (i.e. it
/// enumerates the top class). Such an axiom needs the universe-cardinality
/// argument the native post-pass does not perform, so its presence keeps the
/// `oneOf` family honestly undecided.
fn one_of_constrains_thing(edb: &RdfDataset) -> bool {
    for (subject, predicate, _object, _world) in quads_by_subject(edb) {
        if predicate == OWL_ONE_OF && subject == OWL_THING {
            return true;
        }
    }
    false
}

/// True iff every quad whose predicate is `list_predicate` points at a resolved,
/// non-empty RDF list (so the union/oneOf/disjointUnion handler can walk it).
fn all_list_instances_resolve(
    edb: &RdfDataset,
    list_predicate: &str,
    lists: &HashMap<(String, String), Vec<String>>,
) -> bool {
    let mut saw_instance = false;
    for (_subject, predicate, object, world) in quads_by_subject(edb) {
        if predicate != list_predicate {
            continue;
        }
        let Some(root) = term_resource_key(&object) else {
            return false;
        };
        saw_instance = true;
        match lists.get(&(world, root)) {
            Some(members) if !members.is_empty() => {}
            _ => return false,
        }
    }
    saw_instance
}

/// True iff every `owl:propertyChainAxiom` instance is a resolvable list of
/// exactly two properties (the only shape the post-pass composes).
fn all_property_chains_are_binary(
    edb: &RdfDataset,
    lists: &HashMap<(String, String), Vec<String>>,
) -> bool {
    let mut saw_instance = false;
    for (_subject, predicate, object, world) in quads_by_subject(edb) {
        if predicate != OWL_PROPERTY_CHAIN_AXIOM {
            continue;
        }
        let Some(root) = term_resource_key(&object) else {
            return false;
        };
        saw_instance = true;
        match lists.get(&(world, root)) {
            Some(members) if members.len() == 2 => {}
            _ => return false,
        }
    }
    saw_instance
}

/// True iff every restriction carrying the cardinality `family` is in the
/// genuinely-decidable sub-case for the native handler (Gap B).
///
/// A cardinality instance is decidable when the chase can act on it: it has a
/// **parseable** non-negative integer bound and a resolvable `onProperty`, and —
/// for the qualified families — a resolvable `onClass`. Given that shape:
/// - a *minimum* (`min`/exact/`qualified`/`minQualified`) discharges by inventing
///   the required distinct Skolem witnesses ([`witness_iri`]);
/// - a *maximum* (`max`/exact/`qualified`/`maxQualified`) clashes by counting
///   distinct fillers under the identity-stance anti-merge ([`pairwise_distinct`]).
///
/// An unparsable bound, a missing `onProperty`, or a qualified restriction with
/// no `onClass` is a shape the handler cannot act on, so it stays undecided
/// (honesty over green) and surfaces as a gap.
fn all_cardinality_instances_decidable(
    edb: &RdfDataset,
    restrictions: &HashMap<(String, String), Restriction>,
    family: &str,
) -> bool {
    let (predicate_iri, qualified) = match family {
        "cardinality" => (OWL_CARDINALITY, false),
        "minCardinality" => (OWL_MIN_CARDINALITY, false),
        "maxCardinality" => (OWL_MAX_CARDINALITY, false),
        "qualifiedCardinality" => (OWL_QUALIFIED_CARDINALITY, true),
        "minQualifiedCardinality" => (OWL_MIN_QUALIFIED_CARDINALITY, true),
        "maxQualifiedCardinality" => (OWL_MAX_QUALIFIED_CARDINALITY, true),
        _ => return false,
    };
    // Scan the raw quads (not the parsed restrictions) so an *unparsable* bound
    // literal is caught as undecided rather than silently skipped.
    let mut saw_instance = false;
    for (subject, predicate, _object, world) in quads_by_subject(edb) {
        if predicate != predicate_iri {
            continue;
        }
        saw_instance = true;
        let Some(restriction) = restrictions.get(&(world, subject)) else {
            return false;
        };
        let bound = match family {
            "cardinality" => restriction.cardinality,
            "minCardinality" => restriction.min_cardinality,
            "maxCardinality" => restriction.max_cardinality,
            "qualifiedCardinality" => restriction.qualified_cardinality,
            "minQualifiedCardinality" => restriction.min_qualified_cardinality,
            "maxQualifiedCardinality" => restriction.max_qualified_cardinality,
            _ => None,
        };
        // Unparsable bound, missing onProperty, or (qualified) missing onClass:
        // the handler cannot generate/count, so the instance stays undecided.
        if bound.is_none() || restriction.on_property.is_none() {
            return false;
        }
        if qualified && restriction.on_class.is_none() {
            return false;
        }
    }
    saw_instance
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
    const S: &str = "http://gmeow.example/S";
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
    fn union_of_with_resolvable_list_is_decided_not_a_gap() {
        // owl:unionOf is a positive finite class-expression consequence in the
        // predicate-as-DATA path *when its list resolves*: A = unionOf (B C).
        // Its presence with a walkable list must not emit a DlGap.
        const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
        const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
        const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
        let l0 = "http://gmeow.example/l0";
        let l1 = "http://gmeow.example/l1";
        let store = dataset(vec![
            quad(A, super::OWL_UNION_OF, l0),
            quad(l0, FIRST, B),
            quad(l0, REST, l1),
            quad(l1, FIRST, C),
            quad(l1, REST, NIL),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(verdict.consistent, "bare union axiom is consistent");
        assert!(
            verdict.gaps.is_empty(),
            "owl:unionOf with a resolvable list is decided natively, not a gap: {:?}",
            verdict.gaps
        );
        assert!(
            verdict.coverage.present.contains(&"unionOf".to_owned()),
            "coverage records unionOf as present: {:?}",
            verdict.coverage
        );
        assert!(
            verdict.coverage.decided.contains(&"unionOf".to_owned()),
            "coverage records unionOf as decided: {:?}",
            verdict.coverage
        );
    }

    #[test]
    fn one_of_closure_forces_a_non_member_instance_into_nothing() {
        // Colour = oneOf (red green); x : Colour but x differentFrom both red and
        // green ⇒ x can be no member ⇒ x : owl:Nothing ⇒ INCONSISTENT. This is the
        // CLOSURE half of oneOf (the member→type direction is the easy half), the
        // beyond-EL nominal reasoning the #697 frozen HermiT gold demands native
        // catch (native ⊇ oracle).
        const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
        const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
        const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
        let colour = "http://gmeow.example/Colour";
        let red = "http://gmeow.example/red";
        let green = "http://gmeow.example/green";
        let l0 = "http://gmeow.example/l0";
        let l1 = "http://gmeow.example/l1";
        let store = dataset(vec![
            quad(colour, super::OWL_ONE_OF, l0),
            quad(l0, FIRST, red),
            quad(l0, REST, l1),
            quad(l1, FIRST, green),
            quad(l1, REST, NIL),
            quad(X, TYPE, colour),
            quad(X, OWL_DIFFERENT_FROM, red),
            quad(X, OWL_DIFFERENT_FROM, green),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            !verdict.consistent,
            "an enumeration instance distinct from every member must clash: {:?}",
            verdict.inconsistencies
        );
        assert!(
            verdict.inconsistencies.iter().any(|w| w.individual == X),
            "x must be the inconsistency witness: {:?}",
            verdict.inconsistencies
        );
        assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
    }

    #[test]
    fn one_of_with_a_member_instance_is_consistent() {
        // Same enumeration, but x is NOT asserted distinct from the members, so by
        // the open identity stance it may be one of them ⇒ NO clash ⇒ CONSISTENT.
        // Proves the closure clash is real (driven by differentFrom), not a blanket
        // "any instance of a oneOf class is unsat".
        const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
        const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
        const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
        let colour = "http://gmeow.example/Colour";
        let red = "http://gmeow.example/red";
        let green = "http://gmeow.example/green";
        let l0 = "http://gmeow.example/l0";
        let l1 = "http://gmeow.example/l1";
        let store = dataset(vec![
            quad(colour, super::OWL_ONE_OF, l0),
            quad(l0, FIRST, red),
            quad(l0, REST, l1),
            quad(l1, FIRST, green),
            quad(l1, REST, NIL),
            quad(X, TYPE, colour),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            verdict.consistent,
            "an enumeration instance not asserted distinct from the members is \
             consistent: {:?}",
            verdict.inconsistencies
        );
        assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
    }

    const SOME_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#someValuesFrom";
    const QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#qualifiedCardinality";
    const MIN_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#minQualifiedCardinality";
    const ON_CLASS: &str = "http://www.w3.org/2002/07/owl#onClass";
    const SAME_AS: &str = "http://www.w3.org/2002/07/owl#sameAs";
    const D: &str = "http://gmeow.example/D";
    const Z: &str = "http://gmeow.example/z";

    #[test]
    fn exists_p_c_and_all_p_d_with_disjoint_c_d_is_inconsistent() {
        // GAP B keystone — the case the old inert handler missed.
        // R = ∃p.C, S = ∀p.D, C disjointWith D, x : R, x : S, NO asserted filler.
        // The chase must invent a scoped witness w with p(x,w) and type(w,C);
        // ∀p.D then types w as D; C disjoint D ⇒ w : owl:Nothing ⇒ INCONSISTENT.
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            quad(R, SOME_VALUES_FROM, C),
            quad(S, ON_PROPERTY, P),
            quad(S, ALL_VALUES_FROM, D),
            quad(C, DISJOINT, D),
            quad(X, TYPE, R),
            quad(X, TYPE, S),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            !verdict.consistent,
            "∃p.C ⊓ ∀p.D with C⊓D⊑⊥ must be inconsistent via an invented witness: {:?}",
            verdict.inconsistencies
        );
        assert!(
            verdict
                .inconsistencies
                .iter()
                .any(|w| w.individual.starts_with(crate::encode::SKOLEM_PREFIX)),
            "the inconsistency witness must be the invented Skolem filler: {:?}",
            verdict.inconsistencies
        );
        assert!(
            verdict.gaps.is_empty(),
            "someValuesFrom is now genuinely decided — no gap: {:?}",
            verdict.gaps
        );
    }

    #[test]
    fn some_values_from_satisfiable_filler_is_consistent_and_terminates() {
        // R = ∃p.C, x : R, no disjointness anywhere. The chase invents w, types
        // it C, and reaches a fixed point WITHOUT regenerating witnesses
        // (content-addressed identity). If termination were broken this test
        // would hang rather than fail — its mere completion is the witness.
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            quad(R, SOME_VALUES_FROM, C),
            quad(X, TYPE, R),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(verdict.consistent, "a satisfiable ∃p.C is consistent");
        assert!(
            verdict
                .coverage
                .decided
                .contains(&"someValuesFrom".to_owned()),
            "someValuesFrom is decided: {:?}",
            verdict.coverage
        );
        assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
    }

    #[test]
    fn cyclic_some_values_from_terminates() {
        // C ⊑ ∃p.C (R = ∃p.C, C ⊑ R). x : C forces a witness w typed C, which is
        // therefore R, which needs a p-filler of type C — the SAME class-set in
        // the SAME world, so the witness pool is reused (no fresh chain). The
        // restricted-chase blocking by class-set guarantees this terminates; the
        // test completing at all is the termination proof.
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            quad(R, SOME_VALUES_FROM, C),
            quad(C, SUBCLASS, R),
            quad(X, TYPE, C),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(verdict.consistent, "cyclic but satisfiable ∃ is consistent");
        assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
    }

    #[test]
    fn max_one_with_two_distinct_fillers_clashes_under_una() {
        // R = ≤1 p (maxCardinality 1), x : R, x p y, x p z, y ≠ z by UNA (distinct
        // IRIs, no sameAs). The identity-stance anti-merge must clash WITHOUT any
        // explicit owl:differentFrom ⇒ INCONSISTENT.
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            literal_quad(R, MAX_CARDINALITY, "1", XSD_NON_NEGATIVE_INTEGER),
            quad(X, TYPE, R),
            quad(X, P, Y),
            quad(X, P, Z),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            !verdict.consistent,
            "two distinct fillers under ≤1 must clash by UNA: {:?}",
            verdict.inconsistencies
        );
        assert!(
            verdict
                .coverage
                .decided
                .contains(&"maxCardinality".to_owned()),
            "positive maxCardinality is now decided: {:?}",
            verdict.coverage
        );
        assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
    }

    #[test]
    fn max_one_with_mergeable_fillers_is_consistent() {
        // Same ≤1 p, but y owl:sameAs z merges the two fillers ⇒ NOT distinct ⇒
        // no clash ⇒ CONSISTENT. Proves the anti-merge is real, not a count of
        // raw IRIs.
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            literal_quad(R, MAX_CARDINALITY, "1", XSD_NON_NEGATIVE_INTEGER),
            quad(X, TYPE, R),
            quad(X, P, Y),
            quad(X, P, Z),
            quad(Y, SAME_AS, Z),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            verdict.consistent,
            "mergeable fillers (sameAs) must NOT clash under ≤1: {:?}",
            verdict.inconsistencies
        );
        assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
    }

    #[test]
    fn min_qualified_two_generates_two_distinct_witnesses_and_terminates() {
        // R = ≥2 p.C (minQualifiedCardinality 2, onClass C), x : R, no asserted
        // filler. The chase must invent TWO distinct witnesses w0,w1 both typed C
        // and both p-fillers of x. Consistent, terminating, and the ≥2 obligation
        // is then met (so re-running invents nothing new).
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            quad(R, ON_CLASS, C),
            literal_quad(R, MIN_QUALIFIED_CARDINALITY, "2", XSD_NON_NEGATIVE_INTEGER),
            quad(X, TYPE, R),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(verdict.consistent, "a satisfiable ≥2 p.C is consistent");
        assert!(
            verdict
                .coverage
                .decided
                .contains(&"minQualifiedCardinality".to_owned()),
            "minQualifiedCardinality is decided: {:?}",
            verdict.coverage
        );
        assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
    }

    #[test]
    fn min_two_then_max_one_qualified_clashes() {
        // R = ≥2 p.C AND ≤1 p.C on the same restriction (minQualifiedCardinality 2
        // + qualifiedCardinality... use min 2 + maxCardinality 1 unqualified for a
        // crisp clash). The min generates 2 distinct C-witnesses, the max-1 then
        // counts 2 distinct fillers ⇒ clash ⇒ INCONSISTENT. Demonstrates the
        // generation and counting interlock.
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            quad(R, ON_CLASS, C),
            literal_quad(R, MIN_QUALIFIED_CARDINALITY, "2", XSD_NON_NEGATIVE_INTEGER),
            literal_quad(R, MAX_CARDINALITY, "1", XSD_NON_NEGATIVE_INTEGER),
            quad(X, TYPE, R),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            !verdict.consistent,
            "≥2 p.C with ≤1 p must clash after witness generation: {:?}",
            verdict.inconsistencies
        );
        assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
    }

    #[test]
    fn qualified_cardinality_one_with_two_distinct_c_fillers_clashes() {
        // R = =1 p.C (qualifiedCardinality 1, onClass C), x : R, x p y, x p z,
        // y:C, z:C, y≠z by UNA ⇒ the =1 maximum clashes ⇒ INCONSISTENT.
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            quad(R, ON_CLASS, C),
            literal_quad(R, QUALIFIED_CARDINALITY, "1", XSD_NON_NEGATIVE_INTEGER),
            quad(X, TYPE, R),
            quad(X, P, Y),
            quad(X, P, Z),
            quad(Y, TYPE, C),
            quad(Z, TYPE, C),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            !verdict.consistent,
            "=1 p.C with two distinct C-fillers must clash: {:?}",
            verdict.inconsistencies
        );
        assert!(
            verdict
                .coverage
                .decided
                .contains(&"qualifiedCardinality".to_owned()),
            "qualifiedCardinality is decided: {:?}",
            verdict.coverage
        );
        assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
    }

    #[test]
    fn unparseable_cardinality_bound_stays_unsupported_so_the_gate_can_fire() {
        // The gate must still be able to fire for a genuinely-undecidable case.
        // A maxCardinality whose literal is NOT a non-negative integer cannot be
        // acted on by the handler, so it stays `unsupported` → a non-empty gaps,
        // proving the gate is not dead code.
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            literal_quad(
                R,
                MAX_CARDINALITY,
                "not-a-number",
                "http://www.w3.org/2001/XMLSchema#string",
            ),
            quad(X, TYPE, R),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            verdict
                .coverage
                .unsupported
                .contains(&"maxCardinality".to_owned()),
            "an unparsable cardinality bound is not genuinely decided: {:?}",
            verdict.coverage
        );
        assert!(
            !verdict.gaps.is_empty(),
            "an undecidable cardinality instance must yield a gap so the gate can fire: {:?}",
            verdict.gaps
        );
        assert!(
            verdict
                .gaps
                .iter()
                .any(|g| g.code == "reason.dl-gap.maxCardinality"),
            "gaps must name the undecided construct: {:?}",
            verdict.gaps
        );
    }

    #[test]
    fn some_values_from_without_on_property_stays_unsupported() {
        // A malformed ∃ restriction (someValuesFrom but no onProperty) cannot be
        // discharged by the chase, so someValuesFrom stays unsupported → gap.
        let store = dataset(vec![quad(R, SOME_VALUES_FROM, C), quad(X, TYPE, R)]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            verdict
                .coverage
                .unsupported
                .contains(&"someValuesFrom".to_owned()),
            "a someValuesFrom with no onProperty is not decidable: {:?}",
            verdict.coverage
        );
        assert!(
            !verdict.gaps.is_empty(),
            "must yield a gap: {:?}",
            verdict.gaps
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

    const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

    fn bnode_quad(s: &str, p: &str, o: &str) -> RdfQuad {
        RdfQuad::new(RdfTerm::blank_node(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
    }

    // ── owl:bottomObjectProperty / owl:bottomDataProperty ─────────────────────

    /// `i : ∃ owl:bottomObjectProperty . owl:Thing` is unsatisfiable — the bottom
    /// property is empty, so an obligation to bear a value on it forces
    /// owl:Nothing.
    #[test]
    fn bottom_object_property_some_values_from_is_inconsistent() {
        let restriction = "http://gmeow.example/r";
        let store = dataset(vec![
            quad(restriction, ON_PROPERTY, OWL_BOTTOM_OBJECT_PROPERTY),
            quad(restriction, super::OWL_SOME_VALUES_FROM, OWL_THING),
            quad(X, TYPE, restriction),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !verdict.consistent,
            "bottom-object-property obligation clashes"
        );
        assert!(
            verdict.gaps.is_empty(),
            "bottomObjectProperty is decided, not a gap: {:?}",
            verdict.gaps
        );
        assert!(verdict
            .coverage
            .decided
            .contains(&"bottomObjectProperty".to_owned()));
    }

    /// The data-property analog: `i : ∃ owl:bottomDataProperty . rdfs:Literal`.
    #[test]
    fn bottom_data_property_some_values_from_is_inconsistent() {
        let restriction = "http://gmeow.example/r";
        let literal_class = "http://www.w3.org/2000/01/rdf-schema#Literal";
        let store = dataset(vec![
            quad(restriction, ON_PROPERTY, OWL_BOTTOM_DATA_PROPERTY),
            quad(restriction, super::OWL_SOME_VALUES_FROM, literal_class),
            quad(X, TYPE, restriction),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !verdict.consistent,
            "bottom-data-property obligation clashes"
        );
        assert!(verdict.gaps.is_empty(), "bottomDataProperty is decided");
    }

    // ── owl:NegativePropertyAssertion (object + data) ─────────────────────────

    /// NPA(Peter, hasSon, Meg) co-present with hasSon(Peter, Meg) ⇒ inconsistent.
    #[test]
    fn negative_object_property_assertion_clashes_with_positive() {
        let peter = "http://gmeow.example/Peter";
        let meg = "http://gmeow.example/Meg";
        let has_son = "http://gmeow.example/hasSon";
        let npa = "npa";
        let store = dataset(vec![
            quad(peter, has_son, meg),
            bnode_quad(npa, TYPE, OWL_NEGATIVE_PROPERTY_ASSERTION),
            bnode_quad(npa, OWL_SOURCE_INDIVIDUAL, peter),
            bnode_quad(npa, OWL_ASSERTION_PROPERTY, has_son),
            bnode_quad(npa, OWL_TARGET_INDIVIDUAL, meg),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(!verdict.consistent, "NPA contradicted by its positive");
        assert!(verdict.gaps.is_empty(), "NPA is decided");
        assert!(verdict
            .coverage
            .decided
            .contains(&"negativePropertyAssertion".to_owned()));
    }

    /// The data analog: NPA(Meg, hasAge, "5") + hasAge(Meg, "5") ⇒ inconsistent,
    /// and a DIFFERENT literal value must NOT clash (literal-aware target match).
    #[test]
    fn negative_data_property_assertion_is_literal_aware() {
        let meg = "http://gmeow.example/Meg";
        let has_age = "http://gmeow.example/hasAge";
        let int_ty = "http://www.w3.org/2001/XMLSchema#integer";
        let npa = "npa";
        // Positive value "5" matches the negated target "5" — clash.
        let store = dataset(vec![
            literal_quad(meg, has_age, "5", int_ty),
            bnode_quad(npa, TYPE, OWL_NEGATIVE_PROPERTY_ASSERTION),
            bnode_quad(npa, OWL_SOURCE_INDIVIDUAL, meg),
            bnode_quad(npa, OWL_ASSERTION_PROPERTY, has_age),
            RdfQuad::new(
                RdfTerm::blank_node(npa),
                OWL_TARGET_VALUE,
                RdfTerm::Literal(RdfLiteral::typed("5", int_ty)),
            )
            .in_graph(RdfTerm::iri(W)),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !verdict.consistent,
            "matching literal value clashes the NPA"
        );

        // A DIFFERENT positive value ("6") does not match the negated "5".
        let store_ok = dataset(vec![
            literal_quad(meg, has_age, "6", int_ty),
            bnode_quad(npa, TYPE, OWL_NEGATIVE_PROPERTY_ASSERTION),
            bnode_quad(npa, OWL_SOURCE_INDIVIDUAL, meg),
            bnode_quad(npa, OWL_ASSERTION_PROPERTY, has_age),
            RdfQuad::new(
                RdfTerm::blank_node(npa),
                OWL_TARGET_VALUE,
                RdfTerm::Literal(RdfLiteral::typed("5", int_ty)),
            )
            .in_graph(RdfTerm::iri(W)),
        ]);
        let verdict_ok = dl_consistency(store_ok.as_ref()).expect("dl consistency should succeed");
        assert!(
            verdict_ok.consistent,
            "a non-matching literal value must NOT clash the NPA"
        );
    }

    // ── owl:FunctionalProperty (data values) ──────────────────────────────────

    /// A functional data property with two distinct literal values forces
    /// owl:Nothing; a single value is consistent.
    #[test]
    fn functional_data_property_two_literals_clash() {
        let peter = "http://gmeow.example/Peter";
        let has_name = "http://gmeow.example/hasName";
        let str_ty = "http://www.w3.org/2001/XMLSchema#string";
        let store = dataset(vec![
            quad(has_name, TYPE, OWL_FUNCTIONAL_PROPERTY),
            literal_quad(peter, has_name, "Peter", str_ty),
            literal_quad(peter, has_name, "Kichwa-Tembo", str_ty),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !verdict.consistent,
            "two distinct literal values on a functional property clash"
        );
        assert!(verdict
            .coverage
            .decided
            .contains(&"functionalProperty".to_owned()));

        let store_ok = dataset(vec![
            quad(has_name, TYPE, OWL_FUNCTIONAL_PROPERTY),
            literal_quad(peter, has_name, "Peter", str_ty),
        ]);
        let verdict_ok = dl_consistency(store_ok.as_ref()).expect("dl consistency should succeed");
        assert!(verdict_ok.consistent, "a single value is consistent");
    }

    // ── owl:hasKey ────────────────────────────────────────────────────────────

    /// hasKey(owl:Thing, [hasSSN]); two differentFrom individuals sharing the key
    /// literal are forced into owl:Nothing.
    #[test]
    fn has_key_collision_with_explicit_distinctness_clashes() {
        let peter = "http://gmeow.example/Peter";
        let pg = "http://gmeow.example/Peter_Griffin";
        let has_ssn = "http://gmeow.example/hasSSN";
        let str_ty = "http://www.w3.org/2001/XMLSchema#string";
        let key_list = "keylist";
        let store = dataset(vec![
            bnode_quad(key_list, FIRST, has_ssn),
            bnode_quad(key_list, REST, NIL),
            RdfQuad::new(
                RdfTerm::iri(OWL_THING),
                OWL_HAS_KEY,
                RdfTerm::blank_node(key_list),
            )
            .in_graph(RdfTerm::iri(W)),
            literal_quad(peter, has_ssn, "123-45-6789", str_ty),
            literal_quad(pg, has_ssn, "123-45-6789", str_ty),
            quad(peter, OWL_DIFFERENT_FROM, pg),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !verdict.consistent,
            "key-agreeing differentFrom individuals clash"
        );
        assert!(verdict.gaps.is_empty(), "hasKey is decided");
        assert!(verdict.coverage.decided.contains(&"hasKey".to_owned()));
    }

    /// WITHOUT explicit owl:differentFrom, two key-agreeing individuals are
    /// merely owl:sameAs (no UNA in standard OWL) — consistent, NOT a false clash.
    #[test]
    fn has_key_collision_without_distinctness_is_consistent() {
        let peter = "http://gmeow.example/Peter";
        let pg = "http://gmeow.example/Peter_Griffin";
        let has_ssn = "http://gmeow.example/hasSSN";
        let str_ty = "http://www.w3.org/2001/XMLSchema#string";
        let key_list = "keylist";
        let store = dataset(vec![
            bnode_quad(key_list, FIRST, has_ssn),
            bnode_quad(key_list, REST, NIL),
            RdfQuad::new(
                RdfTerm::iri(OWL_THING),
                OWL_HAS_KEY,
                RdfTerm::blank_node(key_list),
            )
            .in_graph(RdfTerm::iri(W)),
            literal_quad(peter, has_ssn, "123-45-6789", str_ty),
            literal_quad(pg, has_ssn, "123-45-6789", str_ty),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            verdict.consistent,
            "without differentFrom, key agreement just merges the two (sameAs) — consistent"
        );
    }

    // ── owl:Thing forced empty / constrained ──────────────────────────────────

    /// owl:Thing ≡ owl:Nothing (or ⊑) makes the always-populated top class empty
    /// — inconsistent.
    #[test]
    fn thing_equivalent_to_nothing_is_inconsistent() {
        let store = dataset(vec![quad(
            OWL_THING,
            super::OWL_EQUIVALENT_CLASS,
            OWL_NOTHING,
        )]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !verdict.consistent,
            "owl:Thing forced empty must be inconsistent"
        );
        assert!(verdict.gaps.is_empty(), "Thing≡Nothing is decided");
    }

    /// owl:Thing oneOf {s} is the DL/Full-divergent singleton-universe case. The
    /// native path does NOT perform the universe-cardinality argument, so it must
    /// stay an HONEST gap (incomplete) — NEVER a wrong `consistent` decided answer.
    #[test]
    fn one_of_on_thing_is_an_honest_gap_not_a_decided_answer() {
        let s = "http://gmeow.example/s";
        let list = "onelist";
        let store = dataset(vec![
            bnode_quad(list, FIRST, s),
            bnode_quad(list, REST, NIL),
            RdfQuad::new(
                RdfTerm::iri(OWL_THING),
                super::OWL_ONE_OF,
                RdfTerm::blank_node(list),
            )
            .in_graph(RdfTerm::iri(W)),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        // Honesty over a wrong decided answer: oneOf-on-Thing is undecided, so it
        // surfaces as a gap and the case grades DlGap, never CorpusOnly.
        assert!(
            !verdict.gaps.is_empty(),
            "oneOf-on-owl:Thing must surface as an honest gap"
        );
        assert!(
            verdict.coverage.unsupported.contains(&"oneOf".to_owned()),
            "oneOf is undecided when it constrains owl:Thing: {:?}",
            verdict.coverage
        );
    }

    /// A normal `owl:oneOf` on an ordinary class (not owl:Thing) stays DECIDED —
    /// the Thing carve-out must not regress the general enumeration handling.
    #[test]
    fn one_of_on_ordinary_class_stays_decided() {
        let list = "onelist";
        let store = dataset(vec![
            bnode_quad(list, FIRST, X),
            bnode_quad(list, REST, NIL),
            RdfQuad::new(
                RdfTerm::iri(A),
                super::OWL_ONE_OF,
                RdfTerm::blank_node(list),
            )
            .in_graph(RdfTerm::iri(W)),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            verdict.coverage.decided.contains(&"oneOf".to_owned()),
            "ordinary oneOf stays decided: {:?}",
            verdict.coverage
        );
    }
}
