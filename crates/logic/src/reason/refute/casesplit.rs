// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Retained class-expression source admission and native contextual case analysis.
//!
//! Complement, finite union/disjoint union and nominal membership use exhaustive
//! branch proofs. Every proof retains the original world, source leaves and actual
//! native committed support. Only subjects contradicted in every branch receive a
//! local empty-class head from the shared governor.
//!
//! Source grammar refusal precedes all writers. Runtime model obstructions and
//! resource bounds remain visible alongside positive conflicts. Class diagnostics
//! make no whole-program consistency claim. Typed production execution consumes
//! one retained source Scan and the shared native store/proof DAG; source-only
//! assertion helpers below exist exclusively for tiny synthetic tests.

use std::collections::{BTreeMap, BTreeSet};

use purrdf::{DatasetView, RdfTerm, TermValue};

mod proof;
use proof::Support;
pub(crate) mod execution;
pub use execution::ClassExecutionOutcome;
mod admission;
pub use admission::{
    ClassAdmissionObservation, ClassAdmissionSourceWorld, ClassAdmissionWorld, ClassSourceRefusal,
    PreparedClassAnalysis,
};

#[cfg(test)]
use super::{
    ContextualConflict, Decision, FragmentFamily, NothingClash, RefutationCertificate, Witness,
    WitnessEvidence, certify_membership,
};
use super::{
    FragmentBoundary, RefutationAssumption, RefutationBranch, RefutationClash,
    RefutationExpression as Concept, RefutationPremise, RefutationProof, RefutationSourceIssue,
    resource_key, world_key,
};

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

/// Each selected definition requires a resource-valued class or finite-list head.
const EXPRESSION_DEFINITION_PREDICATES: &[&str] = &[
    OWL_COMPLEMENT_OF,
    OWL_INTERSECTION_OF,
    OWL_UNION_OF,
    OWL_ONE_OF,
    OWL_DISJOINT_UNION_OF,
];

/// Operators requiring another model family when connected to a selected class
/// obligation. Independent components keep their own family ownership. These
/// obstructions remain visible alongside every positive closing argument.
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

/// Declaration markers requiring another model family for a connected class
/// obligation. AllDifferent is handled through its admitted member-list expansion.
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

/// Maximum nested class-expression resolution; exceeding it is an explicit
/// unsupported model construct, never an ordinary named-class replacement.
const RESOLVE_DEPTH: u32 = 64;

/// The maximum case-split search RECURSION depth (nested nondeterministic branch
/// points along one DFS path). The selected step allowance alone is NOT a recursion
/// guard: a cyclic class expression (e.g. `_:B = B ⊓ (_:B ⊔ C)`) makes `pick_branch`
/// re-offer a non-progressing disjunct branch, so the DFS recurses one native stack
/// frame per step and SIGABRTs on stack exhaustion (~6500 frames) long before the
/// 400 000-step budget. This explicit bound keeps recursion FINITE and, like the
/// budget, WITHHOLDS ([`SearchResult::Bound`]) rather than crash or guess. Chosen an
/// order of magnitude above the deepest branch nesting any decided corpus case needs
/// (measured tens) and an order of magnitude below the native stack limit.
const SEARCH_DEPTH: u32 = 1024;

// ── Synthetic source-level assertions ──────────────────────────────────────────

// ── Concepts ─────────────────────────────────────────────────────────────────────

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
    all_different_heads: Vec<(String, String)>,
    /// every predicate present (for the consistent-fragment gate).
    predicates: BTreeSet<String>,
    /// every `rdf:type` object present (for the consistent-fragment gate).
    type_objects: BTreeSet<String>,
    /// Reachable list nodes are owned by selected definitions, never bare list fields.
    list_nodes: BTreeMap<String, BTreeSet<String>>,
    /// Admission is prepared once, after every selected source operand is known.
    source_boundary: Option<FragmentBoundary>,
    admission_issues: BTreeMap<FragmentBoundary, Support>,
    source_premises: BTreeMap<(String, String, TermValue), Support>,
    list_premises: BTreeMap<String, Support>,
    incomplete_lists: BTreeSet<String>,
    cyclic_lists: BTreeSet<String>,
    nonresource_list_members: BTreeSet<String>,
    field_values: BTreeMap<(String, String), TermValue>,
    list_fields: BTreeMap<String, Support>,
    ambiguous_fields: BTreeSet<(String, String)>,
    /// Unsupported selected operands remain visible before resource-only lowering.
    unsupported_expression_operands: BTreeSet<(String, String, TermValue)>,
    unsupported_expression_owners: BTreeMap<(String, String), TermValue>,
}

fn semantic_predicate(predicate: &str) -> String {
    let semantics = crate::native_semantics::SemanticVocabulary::GroundedLogicV1;
    let native = semantics.predicate(predicate);
    semantics
        .alternate_predicate(native)
        .unwrap_or(native)
        .to_owned()
}

/// Only class-owner positions admit the two built-in class markers as subjects.
/// A property, nominal member or ordinary data resource with the same IRI stays exact.
fn semantic_class_owner(predicate: &str, subject: String) -> String {
    if EXPRESSION_DEFINITION_PREDICATES.contains(&predicate)
        || matches!(
            predicate,
            RDFS_SUBCLASSOF | OWL_EQUIVALENT_CLASS | OWL_DISJOINT_WITH
        )
    {
        let canonical = match subject.as_str() {
            "https://blackcatinformatics.ca/logic/Thing" => Some(OWL_THING),
            "https://blackcatinformatics.ca/logic/Nothing" => Some(OWL_NOTHING),
            _ => None,
        };
        if let Some(marker) = canonical {
            return marker.to_owned();
        }
    }
    subject
}

fn semantic_value(predicate: &str, term: TermValue) -> TermValue {
    let term = crate::facts::skolemize(&term).into_owned();
    if let TermValue::Iri(iri) = &term {
        let semantics = crate::native_semantics::SemanticVocabulary::GroundedLogicV1;
        if semantics.alternate_marker(predicate, iri).is_some()
            && let Some(projected) = gmeow_ns::owl_view_of_type_marker(iri)
        {
            return TermValue::iri(projected);
        }
    }
    term
}

impl WorldData {
    /// Reachability through actual logical roles bounds the selected class model.
    /// Unrelated property axioms remain owned by their native family. Shared
    /// individuals, class axioms and property restrictions retain their interactions.
    fn model_scope(&self) -> BTreeSet<String> {
        let mut scope: BTreeSet<_> = self
            .source_premises
            .keys()
            .filter(|(owner, predicate, _)| self.selects_definition(owner, predicate))
            .map(|(owner, _, _)| owner.clone())
            .collect();
        let owned_list_nodes: BTreeSet<_> = self
            .list_nodes
            .values()
            .flat_map(|nodes| nodes.iter().map(String::as_str))
            .collect();
        let mut edges = BTreeMap::<&str, BTreeSet<&str>>::new();
        for (subject, predicate, object) in self.source_premises.keys() {
            let Some(object) = object.as_iri() else {
                continue;
            };
            if object == RDF_NIL || subject == RDF_NIL {
                continue;
            }
            let membership = predicate == RDF_TYPE
                && !DECLARATION_TYPE_OBJECTS.contains(&object)
                && !CONSISTENT_BLOCKING_TYPE_OBJECTS.contains(&object);
            let list_field = matches!(predicate.as_str(), RDF_FIRST | RDF_REST)
                && owned_list_nodes.contains(subject.as_str());
            let edge = membership
                || list_field
                || EXPRESSION_DEFINITION_PREDICATES.contains(&predicate.as_str())
                || CONSISTENT_BLOCKING_PREDICATES.contains(&predicate.as_str())
                || matches!(
                    predicate.as_str(),
                    RDFS_SUBCLASSOF
                        | OWL_EQUIVALENT_CLASS
                        | OWL_DISJOINT_WITH
                        | OWL_SAME_AS
                        | OWL_DIFFERENT_FROM
                )
                || (matches!(predicate.as_str(), OWL_MEMBERS | OWL_DISTINCT_MEMBERS)
                    && self.selects_definition(subject, predicate));
            if edge {
                edges.entry(subject).or_default().insert(object);
                edges.entry(object).or_default().insert(subject);
            }
        }
        let mut pending: Vec<_> = scope.iter().cloned().collect();
        while let Some(owner) = pending.pop() {
            if let Some(neighbors) = edges.get(owner.as_str()) {
                for next in neighbors {
                    if scope.insert((*next).to_owned()) {
                        pending.push((*next).to_owned());
                    }
                }
            }
        }
        scope
    }

    /// Defining class operators select their operands directly; a member-list
    /// property selects a list only with its actual AllDifferent declaration.
    fn selects_definition(&self, owner: &str, predicate: &str) -> bool {
        EXPRESSION_DEFINITION_PREDICATES.contains(&predicate)
            || (matches!(predicate, OWL_MEMBERS | OWL_DISTINCT_MEMBERS)
                && !self.support(owner, RDF_TYPE, OWL_ALL_DIFFERENT).is_empty())
    }

    /// Each selected list retains its defining owner and source declaration.
    /// Shared heads are walked once; their independent owners remain visible.
    fn selected_lists(&self) -> Vec<(&str, &str, Support)> {
        let mut selected = Vec::new();
        for ((owner, predicate, operand), source) in &self.source_premises {
            if !matches!(
                predicate.as_str(),
                OWL_UNION_OF
                    | OWL_INTERSECTION_OF
                    | OWL_ONE_OF
                    | OWL_DISJOINT_UNION_OF
                    | OWL_MEMBERS
                    | OWL_DISTINCT_MEMBERS
            ) || !self.selects_definition(owner, predicate)
            {
                continue;
            }
            let Some(head) = operand.as_iri() else {
                continue;
            };
            let mut definition = source.clone();
            if matches!(predicate.as_str(), OWL_MEMBERS | OWL_DISTINCT_MEMBERS) {
                definition.extend(&self.support(owner, RDF_TYPE, OWL_ALL_DIFFERENT));
            }
            selected.push((owner.as_str(), head, definition));
        }
        selected.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
        selected
    }

    /// Refuse only the selected source grammar, with its owner and reached path.
    fn admission_boundaries(&self, world: &str) -> BTreeMap<FragmentBoundary, Support> {
        let boundary = |support: Support, issue| {
            (
                FragmentBoundary::SourceAdmission {
                    world: world.to_owned(),
                    premises: support.rows(),
                    issue,
                },
                support,
            )
        };
        let mut boundaries = Vec::new();
        for ((owner_key, predicate), owner) in &self.unsupported_expression_owners {
            if !self.selects_definition(owner_key, predicate) {
                continue;
            }
            let mut support = Support::default();
            for ((subject, field, _), source) in &self.source_premises {
                if subject == owner_key && field == predicate {
                    support.extend(source);
                }
            }
            if matches!(predicate.as_str(), OWL_MEMBERS | OWL_DISTINCT_MEMBERS) {
                support.extend(&self.support(owner_key, RDF_TYPE, OWL_ALL_DIFFERENT));
            }
            for (_, head, definition) in self
                .selected_lists()
                .into_iter()
                .filter(|(owner, _, _)| *owner == owner_key)
            {
                support.extend(&definition);
                if let Some(path) = self.list_premises.get(head) {
                    support.extend(path);
                }
            }
            boundaries.push(boundary(
                support,
                RefutationSourceIssue::UnsupportedExpressionOwner {
                    owner: owner.clone(),
                    predicate: predicate.clone(),
                },
            ));
        }
        for (owner, predicate, operand) in &self.unsupported_expression_operands {
            if !self.selects_definition(owner, predicate) {
                continue;
            }
            let mut support = self
                .source_premises
                .get(&(owner.clone(), predicate.clone(), operand.clone()))
                .expect("an unsupported selected operand retains its original source row")
                .clone();
            if matches!(predicate.as_str(), OWL_MEMBERS | OWL_DISTINCT_MEMBERS) {
                support.extend(&self.support(owner, RDF_TYPE, OWL_ALL_DIFFERENT));
            }
            boundaries.push(boundary(
                support,
                RefutationSourceIssue::UnsupportedExpressionOperand {
                    owner: owner.clone(),
                    predicate: predicate.clone(),
                    operand: operand.clone(),
                },
            ));
        }
        for (subject, predicate) in &self.ambiguous_fields {
            if !self.selects_definition(subject, predicate) {
                continue;
            }
            let mut support = Support::default();
            for ((s, p, _), source) in &self.source_premises {
                if s == subject && p == predicate {
                    support.extend(source);
                }
            }
            boundaries.push(boundary(
                support,
                RefutationSourceIssue::ExpressionMultiplicity {
                    subject: subject.clone(),
                    predicate: predicate.clone(),
                },
            ));
        }
        for (owner, head, definition) in self.selected_lists() {
            let nodes = self
                .list_nodes
                .get(head)
                .expect("selected list paths are prepared");
            let support = definition.merge(
                self.list_premises
                    .get(head)
                    .expect("selected list source evidence is retained"),
            );
            if nodes.contains(RDF_NIL) && self.list_fields.contains_key(RDF_NIL) {
                boundaries.push(boundary(
                    support.clone(),
                    RefutationSourceIssue::MalformedNil,
                ));
            }
            for (node, predicate) in self.ambiguous_fields.iter().filter(|(node, predicate)| {
                node.as_str() != RDF_NIL
                    && matches!(predicate.as_str(), RDF_FIRST | RDF_REST)
                    && nodes.contains(node)
            }) {
                boundaries.push(boundary(
                    support.clone(),
                    RefutationSourceIssue::ConflictingListField {
                        subject: node.clone(),
                        predicate: predicate.clone(),
                    },
                ));
            }
            if self.incomplete_lists.contains(head) {
                boundaries.push(boundary(
                    support.clone(),
                    RefutationSourceIssue::IncompleteList {
                        owner: owner.to_owned(),
                        head: head.to_owned(),
                    },
                ));
            }
            if self.cyclic_lists.contains(head) {
                boundaries.push(boundary(
                    support.clone(),
                    RefutationSourceIssue::CyclicList {
                        owner: owner.to_owned(),
                        head: head.to_owned(),
                    },
                ));
            }
            if nodes.iter().any(|node| {
                node.as_str() != RDF_NIL && self.nonresource_list_members.contains(node)
            }) {
                boundaries.push(boundary(
                    support,
                    RefutationSourceIssue::UnsupportedListMember {
                        owner: owner.to_owned(),
                        head: head.to_owned(),
                    },
                ));
            }
        }
        boundaries.into_iter().collect()
    }

    fn support_term(&self, subject: &str, predicate: &str, object: TermValue) -> Support {
        self.source_premises
            .get(&(
                subject.to_owned(),
                predicate.to_owned(),
                semantic_value(predicate, object),
            ))
            .cloned()
            .unwrap_or_default()
    }
    fn support(&self, subject: &str, predicate: &str, object: &str) -> Support {
        self.support_term(subject, predicate, TermValue::iri(object))
    }
    fn note(&mut self, subject: &str, predicate: &str, quad: &purrdf::RdfQuad, support: &Support) {
        let value = semantic_value(predicate, crate::reason::dataset::value(&quad.object));
        if (EXPRESSION_DEFINITION_PREDICATES.contains(&predicate)
            || matches!(predicate, OWL_MEMBERS | OWL_DISTINCT_MEMBERS))
            && !matches!(&value, TermValue::Iri(_))
        {
            self.unsupported_expression_operands.insert((
                subject.to_owned(),
                predicate.to_owned(),
                value.clone(),
            ));
        }
        if matches!(
            predicate,
            RDF_FIRST
                | RDF_REST
                | OWL_COMPLEMENT_OF
                | OWL_INTERSECTION_OF
                | OWL_UNION_OF
                | OWL_ONE_OF
        ) {
            let field = (subject.to_owned(), predicate.to_owned());
            if let Some(prior) = self.field_values.insert(field.clone(), value.clone())
                && prior != value
            {
                self.ambiguous_fields.insert(field);
            }
        }
        if predicate == RDF_FIRST && !matches!(&value, TermValue::Iri(_)) {
            self.nonresource_list_members.insert(subject.to_owned());
        }
        if matches!(predicate, RDF_FIRST | RDF_REST) {
            self.list_fields
                .entry(subject.to_owned())
                .or_default()
                .extend(support);
        }
        let key = (subject.to_owned(), predicate.to_owned(), value);
        self.source_premises.entry(key).or_default().extend(support);
    }
}

/// Adapt one native statement to the tableau's owned term surface, never RDF text.
fn supported_quad(
    subject: &TermValue,
    predicate: &str,
    object: &TermValue,
    graph: Option<&TermValue>,
) -> gmeow_errors::Result<purrdf::RdfQuad> {
    let mut quad = purrdf::RdfQuad::new(
        crate::reason::term_value_to_rdf_term(subject)?,
        predicate,
        crate::reason::term_value_to_rdf_term(object)?,
    );
    quad.graph_name = graph
        .map(crate::reason::term_value_to_rdf_term)
        .transpose()?;
    Ok(quad)
}

struct Scan {
    source_worlds: BTreeMap<String, ClassAdmissionSourceWorld>,
    source_alias: bool,
    firsts: BTreeMap<String, BTreeMap<String, RdfTerm>>,
    rests: BTreeMap<String, BTreeMap<String, BTreeSet<String>>>,
    worlds: BTreeMap<String, WorldData>,
}

impl Scan {
    fn of(edb: &impl DatasetView) -> Self {
        let graphs = edb
            .named_graphs()
            .map(|id| crate::reason::dataset::native(edb, id));
        let rows = crate::reason::dataset::owned_quads(edb).map(|quad| {
            let support = Support::one(RefutationPremise {
                subject: crate::reason::dataset::value(&quad.subject),
                predicate: quad.predicate.clone(),
                object: crate::reason::dataset::value(&quad.object),
                graph: quad.graph_name.as_ref().map(crate::reason::dataset::value),
            });
            Ok((quad, support))
        });
        Self::from_parts(graphs, rows)
            .expect("admitted native RDF rows do not require a fallible conversion")
    }

    /// One typed ingress over retained native occurrences, without any dataset rebuild.
    fn from_parts(
        graphs: impl IntoIterator<Item = TermValue>,
        rows: impl IntoIterator<Item = gmeow_errors::Result<(purrdf::RdfQuad, Support)>>,
    ) -> gmeow_errors::Result<Self> {
        let worlds: BTreeMap<String, WorldData> = BTreeMap::new();
        let mut source_worlds: BTreeMap<String, ClassAdmissionSourceWorld> = BTreeMap::new();
        let mut source_alias = false;
        for graph in graphs {
            if let Ok(world) = admission::graph_world(Some(&graph)) {
                if let Some(prior) = source_worlds.insert(
                    world,
                    ClassAdmissionSourceWorld {
                        graph: Some(graph.clone()),
                        assertions: 0,
                    },
                ) {
                    source_alias |= prior.graph.as_ref() != Some(&graph);
                }
            } else {
                source_alias = true;
            }
        }
        // raw first/rest edges per world for the list walk.
        let firsts: BTreeMap<String, BTreeMap<String, RdfTerm>> = BTreeMap::new();
        let rests: BTreeMap<String, BTreeMap<String, BTreeSet<String>>> = BTreeMap::new();

        let mut scan = Scan {
            worlds,
            source_worlds,
            source_alias,
            firsts,
            rests,
        };
        for row in rows {
            let (quad, support) = row?;
            scan.ingest(quad, support);
        }
        scan.prepare_schema();
        Ok(scan)
    }

    /// Append one committed statement with its real native origin and source leaves.
    fn ingest(&mut self, quad: purrdf::RdfQuad, support: Support) {
        let world = world_key(&quad.graph_name);
        let graph = quad.graph_name.as_ref().map(crate::reason::dataset::value);
        let source =
            self.source_worlds
                .entry(world.clone())
                .or_insert_with(|| ClassAdmissionSourceWorld {
                    graph: graph.clone(),
                    assertions: 0,
                });
        self.source_alias |= source.graph != graph;
        source.assertions += 1;
        // The shared native resolver admits predicate roles before the selected
        // case-split operators inspect them. Marker interpretation is separately
        // role-checked; nominal members and other data IRIs remain exact. The
        // evidence table retains each original statement before interpretation.
        let predicate = semantic_predicate(&quad.predicate);
        let Some(subject) = resource_key(&quad.subject) else {
            // A quoted owner does not become its embedded subject. Retain only
            // actual source grammar fields so selected operators cannot vanish.
            if EXPRESSION_DEFINITION_PREDICATES.contains(&predicate.as_str())
                || matches!(
                    predicate.as_str(),
                    OWL_MEMBERS | OWL_DISTINCT_MEMBERS | RDF_TYPE
                )
            {
                let owner = crate::reason::dataset::value(&quad.subject);
                let owner_key = gmeow_term_arena::engine::native_term_key(&owner);
                let w = self.worlds.entry(world).or_default();
                w.note(&owner_key, &predicate, &quad, &support);
                if predicate != RDF_TYPE {
                    w.unsupported_expression_owners
                        .insert((owner_key, predicate.clone()), owner);
                }
            }
            return;
        };
        let subject = semantic_class_owner(&predicate, subject);
        let w = self.worlds.entry(world.clone()).or_default();
        w.predicates.insert(predicate.clone());
        if CONSISTENT_BLOCKING_PREDICATES.contains(&predicate.as_str())
            || matches!(
                predicate.as_str(),
                RDF_TYPE
                    | RDFS_SUBCLASSOF
                    | OWL_EQUIVALENT_CLASS
                    | OWL_DISJOINT_WITH
                    | OWL_DISJOINT_UNION_OF
                    | OWL_COMPLEMENT_OF
                    | OWL_INTERSECTION_OF
                    | OWL_UNION_OF
                    | OWL_ONE_OF
                    | OWL_SAME_AS
                    | OWL_DIFFERENT_FROM
                    | OWL_MEMBERS
                    | OWL_DISTINCT_MEMBERS
                    | RDF_FIRST
                    | RDF_REST
            )
        {
            w.note(&subject, &predicate, &quad, &support);
        }

        match predicate.as_str() {
            RDF_TYPE => {
                if let Some(object) = resource_key(&quad.object) {
                    let object = semantic_value(&predicate, TermValue::iri(object))
                        .as_iri()
                        .expect("resource marker")
                        .to_owned();
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
                    w.all_different_heads.push((subject, object));
                }
            }
            RDF_FIRST => {
                self.firsts
                    .entry(world.clone())
                    .or_default()
                    .insert(subject.clone(), quad.object.clone());
            }
            RDF_REST => {
                if let Some(object) = resource_key(&quad.object) {
                    self.rests
                        .entry(world.clone())
                        .or_default()
                        .entry(subject.clone())
                        .or_default()
                        .insert(object);
                }
            }
            RDFS_LABEL | RDFS_COMMENT | RDFS_SEE_ALSO | RDFS_IS_DEFINED_BY => {}
            _ => {}
        }
    }

    /// Resolve selected definitions and reachable list paths once for this schema.
    fn prepare_schema(&mut self) {
        // Only complete source-owned lists can justify exhaustive alternatives.
        for (world, w) in &mut self.worlds {
            w.lists.clear();
            w.list_nodes.clear();
            w.list_premises.clear();
            w.incomplete_lists.clear();
            w.cyclic_lists.clear();
            w.same_as.sort();
            w.same_as.dedup();
            w.different_from.sort();
            w.different_from.dedup();
            w.equivalent_named.sort();
            w.equivalent_named.dedup();
            w.disjoint_with.sort();
            w.disjoint_with.dedup();
            w.disjoint_union_of.sort();
            w.disjoint_union_of.dedup();
            w.all_different_heads.sort();
            w.all_different_heads.dedup();
            let first = self.firsts.get(world);
            let rest = self.rests.get(world);
            let selected_heads: BTreeSet<String> = w
                .selected_lists()
                .into_iter()
                .map(|(_, head, _)| head.to_owned())
                .collect();
            for head in selected_heads {
                // Enter/leave frames retain the active path without recursion;
                // each reachable source node is read once even after ambiguous tails.
                let mut pending = vec![(head.clone(), false)];
                let mut active = BTreeSet::new();
                let mut seen = BTreeSet::new();
                let mut members = Vec::new();
                let mut support = Support::default();
                let mut complete = true;
                while let Some((node, leaving)) = pending.pop() {
                    if leaving {
                        active.remove(&node);
                        continue;
                    }
                    if active.contains(&node) {
                        complete = false;
                        w.cyclic_lists.insert(head.clone());
                        continue;
                    }
                    if !seen.insert(node.clone()) {
                        continue;
                    }
                    active.insert(node.clone());
                    pending.push((node.clone(), true));
                    if let Some(fields) = w.list_fields.get(&node) {
                        support.extend(fields);
                    }
                    if node == RDF_NIL {
                        complete &= !w.list_fields.contains_key(RDF_NIL);
                        continue;
                    }
                    if w.ambiguous_fields
                        .contains(&(node.clone(), RDF_FIRST.to_owned()))
                        || w.ambiguous_fields
                            .contains(&(node.clone(), RDF_REST.to_owned()))
                        || w.nonresource_list_members.contains(&node)
                    {
                        complete = false;
                    }
                    if let Some(value) = first.and_then(|fields| fields.get(&node)) {
                        members.push(value.clone());
                    } else {
                        complete = false;
                        w.incomplete_lists.insert(head.clone());
                    }
                    if let Some(next) = rest.and_then(|fields| fields.get(&node)) {
                        pending.extend(next.iter().rev().map(|tail| (tail.clone(), false)));
                    } else {
                        complete = false;
                        w.incomplete_lists.insert(head.clone());
                    }
                }
                if complete {
                    w.lists.insert(head.clone(), members);
                }
                w.list_premises.insert(head.clone(), support);
                w.list_nodes.insert(head, seen);
            }
            w.admission_issues = w.admission_boundaries(world);
            w.source_boundary = match w.admission_issues.len() {
                0 => None,
                1 => w.admission_issues.keys().next().cloned(),
                _ => Some(FragmentBoundary::Combined(
                    w.admission_issues.keys().cloned().collect(),
                )),
            };
        }
    }

    /// True iff any case-split shape is present in any world (the engage set):
    /// `owl:complementOf` / `owl:unionOf` / `owl:oneOf` / `owl:disjointUnionOf`, or
    /// an unadmitted selected expression/list/declaration operand. Unowned list
    /// fields never select this procedure. Pure admitted `owl:intersectionOf` / `owl:disjointWith` (no
    /// disjunction) is left to the native EL/DL chase.
    fn engages(&self) -> bool {
        self.worlds.values().any(|w| {
            w.source_boundary.is_some()
                || !w.complement_of.is_empty()
                || !w.union_of.is_empty()
                || !w.one_of.is_empty()
                || !w.disjoint_union_of.is_empty()
        })
    }

    fn run_world_budget(
        &self,
        world: &str,
        budget: &mut u64,
    ) -> (WorldOutcome, BTreeMap<String, Support>) {
        let w = &self.worlds[world];

        if let Some(boundary) = &w.source_boundary {
            return (
                WorldOutcome::SourceBoundary(boundary.clone()),
                BTreeMap::new(),
            );
        }

        let ctx = Ctx::build(w);
        let resolver = Resolver { w };
        let outcome = (|| {
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
            for (a, b, _) in &ctx.different {
                all_individuals.insert(a.clone());
                all_individuals.insert(b.clone());
            }
            for ind in &all_individuals {
                state.make(ind);
            }
            for (a, b) in &w.same_as {
                state.union(a, b, w.support(a, OWL_SAME_AS, b));
            }

            for individual in &all_individuals {
                let mut existence = Support::default();
                if let Some(classes) = w.types.get(individual) {
                    for class in classes {
                        existence.extend(&w.support(individual, RDF_TYPE, class));
                    }
                }
                for (left, right) in &w.same_as {
                    if left == individual || right == individual {
                        existence.extend(&w.support(left, OWL_SAME_AS, right));
                    }
                }
                for (left, right, support) in &ctx.different {
                    if left == individual || right == individual {
                        existence.extend(support);
                    }
                }
                for (obligation, definition) in &ctx.universal {
                    if let Err(proof) = state.add(
                        individual,
                        obligation.clone(),
                        existence.merge(definition),
                        budget,
                    ) {
                        return WorldOutcome::Inconsistent(proof);
                    }
                }
            }

            // Seed the individual membership concepts.
            for (ind, classes) in &w.types {
                for class in classes {
                    let (concept, definition) = resolver.resolve_supported(class, 0);
                    let support = w.support(ind, RDF_TYPE, class).merge(&definition);
                    if let Err(proof) = state.add(ind, concept, support, budget) {
                        return WorldOutcome::Inconsistent(proof);
                    }
                }
            }

            match search(&mut state, &ctx, budget, 0) {
                SearchResult::Unsat(proof) => WorldOutcome::Inconsistent(proof),
                SearchResult::Bound(bound) => WorldOutcome::SearchBoundary(bound),
                SearchResult::Sat => {
                    if !ctx.obstructions.is_empty() {
                        WorldOutcome::OutOfFragment(
                            ctx.obstructions
                                .keys()
                                .cloned()
                                .collect::<Vec<_>>()
                                .join("; "),
                        )
                    } else {
                        WorldOutcome::Consistent
                    }
                }
            }
        })();
        (outcome, ctx.obstructions)
    }
}

enum WorldOutcome {
    SourceBoundary(FragmentBoundary),
    Inconsistent(RefutationProof),
    Consistent,
    OutOfFragment(String),
    SearchBoundary(SearchBound),
}

// ── The reasoning context (immutable per world) ─────────────────────────────────

struct Ctx {
    /// Obligations on every actual inhabitant, with source-owned class axioms.
    universal: Vec<(Concept, Support)>,
    /// named class → told-subsumer concepts (added when `Pos(C)` is present).
    subsumers: BTreeMap<String, Vec<(Concept, Support)>>,
    /// distinctness pairs (original individuals).
    different: Vec<(String, String, Support)>,
    /// Every selected model obstruction with exact source/native support, retained
    /// even when a different obligation already closes the current branches.
    obstructions: BTreeMap<String, Support>,
}

impl Ctx {
    fn build(w: &WorldData) -> Self {
        let mut subsumers: BTreeMap<String, Vec<(Concept, Support)>> = BTreeMap::new();
        let resolver = Resolver { w };
        let scope = w.model_scope();
        let mut obstructions = BTreeMap::<String, Support>::new();
        let owners: BTreeSet<_> = w
            .source_premises
            .keys()
            .filter(|(_, predicate, _)| {
                EXPRESSION_DEFINITION_PREDICATES.contains(&predicate.as_str())
            })
            .map(|(owner, _, _)| owner.as_str())
            .collect();
        for owner in owners {
            let (concept, support) = resolver.resolve_node(owner, 0, false);
            if concept_contains_blocked(&concept) {
                obstructions
                    .entry(format!("unsupported selected definition on <{owner}>"))
                    .or_default()
                    .extend(&support);
            }
        }
        for (class, supers) in &w.subclass_of {
            for target in supers {
                let (concept, definitions) = resolver.resolve_supported(target, 0);
                let support = definitions
                    .merge(&w.support(class, RDFS_SUBCLASSOF, target))
                    .merge(&w.support(class, OWL_EQUIVALENT_CLASS, target));
                if concept_contains_blocked(&concept) && scope.contains(class) {
                    obstructions
                        .entry(format!("unsupported subsumer <{target}> of <{class}>"))
                        .or_default()
                        .extend(&support);
                }
                subsumers
                    .entry(class.clone())
                    .or_default()
                    .push((concept, support));
            }
        }
        for (a, b) in &w.equivalent_named {
            if resolver.is_expression(a) || resolver.is_expression(b) {
                if scope.contains(a) || scope.contains(b) {
                    obstructions
                        .entry(format!("class expression equivalence <{a}> to <{b}>"))
                        .or_default()
                        .extend(&w.support(a, OWL_EQUIVALENT_CLASS, b));
                }
            } else {
                subsumers.entry(b.clone()).or_default().push((
                    Concept::Pos(a.clone()),
                    w.support(a, OWL_EQUIVALENT_CLASS, b),
                ));
            }
        }
        for (a, b) in &w.disjoint_with {
            for (subject, target) in [(a, b), (b, a)] {
                let (concept, definitions) = resolver.resolve_supported(target, 0);
                let concept = negate(concept);
                let support = definitions.merge(&w.support(a, OWL_DISJOINT_WITH, b));
                if scope.contains(subject) && concept_contains_blocked(&concept) {
                    obstructions
                        .entry(format!(
                            "unsupported disjoint expression <{target}> against <{subject}>"
                        ))
                        .or_default()
                        .extend(&support);
                }
                subsumers
                    .entry(subject.clone())
                    .or_default()
                    .push((concept, support));
            }
        }
        for (class, head) in &w.disjoint_union_of {
            let Some(list_support) = w.list_premises.get(head) else {
                obstructions
                    .entry(format!(
                        "incomplete disjoint union list <{head}> for <{class}>"
                    ))
                    .or_default()
                    .extend(&w.support(class, OWL_DISJOINT_UNION_OF, head));
                continue;
            };
            let members = resolver.list_resources(head);
            let support = w
                .support(class, OWL_DISJOINT_UNION_OF, head)
                .merge(list_support);
            subsumers.entry(class.clone()).or_default().push((
                Concept::Or(members.iter().cloned().map(Concept::Pos).collect()),
                support.clone(),
            ));
            for (index, member) in members.iter().enumerate() {
                subsumers
                    .entry(member.clone())
                    .or_default()
                    .push((Concept::Pos(class.clone()), support.clone()));
                for (other_index, other) in members.iter().enumerate() {
                    if index != other_index {
                        subsumers
                            .entry(member.clone())
                            .or_default()
                            .push((Concept::Neg(other.clone()), support.clone()));
                    }
                }
            }
        }
        let mut different: Vec<_> = w
            .different_from
            .iter()
            .map(|(a, b)| (a.clone(), b.clone(), w.support(a, OWL_DIFFERENT_FROM, b)))
            .collect();
        for (node, head) in &w.all_different_heads {
            let declaration = w.support(node, RDF_TYPE, OWL_ALL_DIFFERENT);
            if declaration.is_empty() {
                continue;
            }
            let Some(list_support) = w.list_premises.get(head) else {
                obstructions
                    .entry(format!(
                        "incomplete AllDifferent list <{head}> for <{node}>"
                    ))
                    .or_default()
                    .extend(
                        &declaration
                            .merge(&w.support(node, OWL_MEMBERS, head))
                            .merge(&w.support(node, OWL_DISTINCT_MEMBERS, head)),
                    );
                continue;
            };
            let support = declaration
                .merge(list_support)
                .merge(&w.support(node, OWL_MEMBERS, head))
                .merge(&w.support(node, OWL_DISTINCT_MEMBERS, head));
            let members = resolver.list_resources(head);
            for (index, a) in members.iter().enumerate() {
                for b in &members[index + 1..] {
                    different.push((a.clone(), b.clone(), support.clone()));
                }
            }
        }
        for ((subject, predicate, object), support) in &w.source_premises {
            if !scope.contains(subject) {
                continue;
            }
            if CONSISTENT_BLOCKING_PREDICATES.contains(&predicate.as_str()) {
                obstructions.entry(format!("selected class model does not complete <{subject}> <{predicate}> {object:?}"))
                    .or_default().extend(support);
            }
            if predicate == RDF_TYPE
                && object
                    .as_iri()
                    .is_some_and(|marker| CONSISTENT_BLOCKING_TYPE_OBJECTS.contains(&marker))
            {
                obstructions
                    .entry(format!(
                        "selected class model does not complete declaration <{subject}> {object:?}"
                    ))
                    .or_default()
                    .extend(support);
            }
        }
        for (owner, head) in &w.one_of {
            let source = w.support(owner, OWL_ONE_OF, head);
            let support = w
                .list_premises
                .get(head)
                .map_or(source.clone(), |path| source.merge(path));
            obstructions
                .entry(format!(
                    "nominal class <{owner}> requires complete set-equality model admission"
                ))
                .or_default()
                .extend(&support);
        }
        for (individual, classes) in &w.types {
            for class in classes {
                if !scope.contains(individual) && !scope.contains(class) {
                    continue;
                }
                let (concept, definition) = resolver.resolve_supported(class, 0);
                if concept_contains_blocked(&concept) {
                    obstructions
                        .entry(format!(
                            "unsupported membership expression <{class}> on <{individual}>"
                        ))
                        .or_default()
                        .extend(&w.support(individual, RDF_TYPE, class).merge(&definition));
                }
            }
        }
        let mut universal = subsumers.remove(OWL_THING).unwrap_or_default();
        for (owner, is_empty) in [(OWL_THING, false), (OWL_NOTHING, true)] {
            if resolver.is_expression(owner) {
                let (definition, support) = resolver.resolve_node(owner, 0, false);
                let concept = if is_empty {
                    negate(definition)
                } else {
                    definition
                };
                if concept_contains_blocked(&concept) {
                    obstructions
                        .entry(format!("unsupported universal definition on <{owner}>"))
                        .or_default()
                        .extend(&support);
                }
                universal.push((concept, support));
            }
        }
        Self {
            universal,
            subsumers,
            different,
            obstructions,
        }
    }
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

    fn resolve_supported(&self, node: &str, depth: u32) -> (Concept, Support) {
        self.resolve_node(node, depth, true)
    }

    fn resolve_node(&self, node: &str, depth: u32, intrinsic: bool) -> (Concept, Support) {
        if depth >= RESOLVE_DEPTH {
            return (Concept::Blocked, Support::default());
        }
        let marker = semantic_value(RDF_TYPE, TermValue::iri(node));
        if intrinsic && marker.as_iri() == Some(OWL_THING) {
            return (Concept::Top, Support::default());
        }
        if intrinsic && marker.as_iri() == Some(OWL_NOTHING) {
            return (Concept::Bottom, Support::default());
        }
        if let Some(inner) = self.w.complement_of.get(node) {
            let (concept, support) = self.resolve_supported(inner, depth + 1);
            return (
                negate(concept),
                support.merge(&self.w.support(node, OWL_COMPLEMENT_OF, inner)),
            );
        }
        for (predicate, head) in [
            (OWL_INTERSECTION_OF, self.w.intersection_of.get(node)),
            (OWL_UNION_OF, self.w.union_of.get(node)),
            (OWL_ONE_OF, self.w.one_of.get(node)),
        ] {
            let Some(head) = head else { continue };
            let source = self.w.support(node, predicate, head);
            let (Some(raw), Some(list)) = (self.w.lists.get(head), self.w.list_premises.get(head))
            else {
                return (Concept::Blocked, source);
            };
            if raw.iter().any(|term| resource_key(term).is_none()) {
                return (Concept::Blocked, source.merge(list));
            }
            let members = self.list_resources(head);
            let mut support = source.merge(list);
            if predicate == OWL_ONE_OF {
                return (Concept::Nominals(members), support);
            }
            let concepts = members
                .iter()
                .map(|member| {
                    let (concept, proof) = self.resolve_supported(member, depth + 1);
                    support.extend(&proof);
                    concept
                })
                .collect();
            return (
                if predicate == OWL_UNION_OF {
                    Concept::Or(concepts)
                } else {
                    Concept::And(concepts)
                },
                support,
            );
        }
        (Concept::Pos(node.to_owned()), Support::default())
    }
}

// ── The bounded tableau ──────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct State {
    parent: BTreeMap<String, String>,
    parent_support: BTreeMap<String, Support>,
    labels: BTreeMap<String, BTreeMap<Concept, Support>>,
}

#[derive(Clone)]
enum Action {
    Add(String, Concept, Support),
    Merge(String, String, Support),
}

impl Action {
    fn assumption(&self) -> RefutationAssumption {
        match self {
            Self::Add(subject, expression, _) => RefutationAssumption::Membership {
                subject: subject.clone(),
                expression: expression.clone(),
            },
            Self::Merge(subject, member, _) => RefutationAssumption::Equality {
                subject: subject.clone(),
                member: member.clone(),
            },
        }
    }
}

struct Choice {
    support: Support,
    alternatives: Vec<Action>,
}

enum Saturation {
    Closed(RefutationProof),
    Open,
    Branch(Choice),
    Bound,
}

enum SearchResult {
    Sat,
    Unsat(RefutationProof),
    Bound(SearchBound),
}

#[derive(Clone, Copy)]
enum SearchBound {
    Steps,
    Depth,
    NonProgress,
}
impl SearchBound {
    fn detail(self) -> String {
        match self {
            Self::Steps => "class search exhausted its shared analysis allowance".to_owned(),
            Self::Depth => format!("class search reached its recursion ceiling of {SEARCH_DEPTH}"),
            Self::NonProgress => {
                "class source choice requires an unsupported recursive model completion".to_owned()
            }
        }
    }
}

impl State {
    fn make(&mut self, individual: &str) {
        self.parent
            .entry(individual.to_owned())
            .or_insert_with(|| individual.to_owned());
        self.labels.entry(individual.to_owned()).or_default();
    }

    // Keep the union forest's supporting edges. Path compression without carrying
    // their proof would destroy the source of a nominal/equality contradiction.
    fn find(&mut self, individual: &str) -> String {
        self.make(individual);
        let mut root = individual.to_owned();
        while let Some(parent) = self.parent.get(&root) {
            if parent == &root {
                break;
            }
            root = parent.clone();
        }
        root
    }

    fn equality_support(&self, individual: &str) -> Support {
        let mut support = Support::default();
        let mut node = individual;
        while let Some(parent) = self.parent.get(node) {
            if parent == node {
                break;
            }
            support.extend(
                self.parent_support
                    .get(node)
                    .expect("every merge retains its support"),
            );
            node = parent;
        }
        support
    }

    fn union(&mut self, left: &str, right: &str, support: Support) -> bool {
        let a = self.find(left);
        let b = self.find(right);
        if a == b {
            return false;
        }
        let support = support
            .merge(&self.equality_support(left))
            .merge(&self.equality_support(right));
        let (root, child) = if a <= b { (a, b) } else { (b, a) };
        let moved = self.labels.remove(&child).unwrap_or_default();
        self.parent.insert(child.clone(), root.clone());
        self.parent_support.insert(child, support.clone());
        let labels = self.labels.entry(root).or_default();
        for (concept, prior) in moved {
            let proof = prior.merge(&support);
            labels
                .entry(concept)
                .and_modify(|current| {
                    if proof < *current {
                        *current = proof.clone();
                    }
                })
                .or_insert(proof);
        }
        true
    }

    fn conflict(
        &mut self,
        individual: &str,
        kind: RefutationClash,
        mut support: Support,
    ) -> RefutationProof {
        let root = self.find(individual);
        let names: Vec<_> = self.parent.keys().cloned().collect();
        let subjects = names
            .into_iter()
            .filter(|name| self.find(name) == root)
            .collect::<BTreeSet<_>>();
        for subject in &subjects {
            support.extend(&self.equality_support(subject));
        }
        RefutationProof::Conflict {
            kind,
            subjects,
            premises: support.rows(),
            support: support.native(),
        }
    }

    fn current_conflict(&mut self, individual: &str) -> Option<RefutationProof> {
        let labels = self.labels_of(individual);
        for (concept, support) in &labels {
            if let Concept::Pos(class) = concept
                && let Some(opposed) = labels.get(&Concept::Neg(class.clone()))
            {
                return Some(self.conflict(
                    individual,
                    RefutationClash::OpposedClass(class.clone()),
                    support.merge(opposed),
                ));
            }
        }
        None
    }

    fn add(
        &mut self,
        individual: &str,
        concept: Concept,
        support: Support,
        budget: &mut u64,
    ) -> Result<bool, RefutationProof> {
        *budget = budget.saturating_sub(1);
        if *budget == 0 {
            return Ok(false);
        }
        let root = self.find(individual);
        let support = support.merge(&self.equality_support(individual));
        match concept {
            Concept::Top | Concept::Blocked => Ok(false),
            Concept::Bottom => Err(self.conflict(&root, RefutationClash::Bottom, support)),
            Concept::And(concepts) => {
                let mut changed = false;
                for concept in concepts {
                    changed |= self.add(&root, concept, support.clone(), budget)?;
                }
                Ok(changed)
            }
            other => {
                let opposite = match &other {
                    Concept::Pos(class) => Some((class, Concept::Neg(class.clone()))),
                    Concept::Neg(class) => Some((class, Concept::Pos(class.clone()))),
                    _ => None,
                };
                if let Some((class, opposite)) = opposite
                    && let Some(prior) = self.labels_of(&root).get(&opposite)
                {
                    return Err(self.conflict(
                        &root,
                        RefutationClash::OpposedClass(class.clone()),
                        support.merge(prior),
                    ));
                }
                let labels = self.labels.entry(root).or_default();
                match labels.entry(other) {
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(support);
                        Ok(true)
                    }
                    std::collections::btree_map::Entry::Occupied(mut entry) => {
                        if support < *entry.get() {
                            entry.insert(support);
                        }
                        Ok(false)
                    }
                }
            }
        }
    }

    fn labels_of(&mut self, individual: &str) -> BTreeMap<Concept, Support> {
        let root = self.find(individual);
        self.labels.get(&root).cloned().unwrap_or_default()
    }
}

fn disjunct_satisfied(labels: &BTreeMap<Concept, Support>, disjunct: &Concept) -> bool {
    matches!(disjunct, Concept::Top) || labels.contains_key(disjunct)
}

fn disjunct_refutation(labels: &BTreeMap<Concept, Support>, disjunct: &Concept) -> Option<Support> {
    match disjunct {
        Concept::Bottom => Some(Support::default()),
        Concept::Pos(class) => labels.get(&Concept::Neg(class.clone())).cloned(),
        Concept::Neg(class) => labels.get(&Concept::Pos(class.clone())).cloned(),
        _ => None,
    }
}

fn live_disjuncts(
    labels: &BTreeMap<Concept, Support>,
    disjuncts: &[Concept],
    source: &Support,
) -> (Vec<Concept>, Support) {
    let mut support = source.clone();
    let live = disjuncts
        .iter()
        .filter_map(|disjunct| {
            if let Some(proof) = disjunct_refutation(labels, disjunct) {
                support.extend(&proof);
                None
            } else {
                Some(disjunct.clone())
            }
        })
        .collect();
    (live, support)
}

fn distinct_roots(state: &mut State) -> Vec<String> {
    let keys = state.parent.keys().cloned().collect::<Vec<_>>();
    keys.iter()
        .map(|key| state.find(key))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn identity_conflict(state: &mut State, ctx: &Ctx) -> Option<RefutationProof> {
    for (a, b, support) in &ctx.different {
        if state.find(a) == state.find(b) {
            let support = support
                .merge(&state.equality_support(a))
                .merge(&state.equality_support(b));
            return Some(state.conflict(a, RefutationClash::EqualityDistinctness, support));
        }
    }
    None
}

fn saturate(state: &mut State, ctx: &Ctx, budget: &mut u64) -> Saturation {
    let mut dirty: BTreeSet<_> = distinct_roots(state).into_iter().collect();
    while let Some(first) = dirty.pop_first() {
        if *budget == 0 {
            return Saturation::Bound;
        }
        let mut root = state.find(&first);
        loop {
            if *budget == 0 {
                return Saturation::Bound;
            }
            if let Some(proof) = state.current_conflict(&root) {
                return Saturation::Closed(proof);
            }
            let mut local_changed = false;
            let mut merged = false;
            let current = state.labels_of(&root);
            for (concept, support) in &current {
                match concept {
                    Concept::Pos(name) => {
                        if let Some(subsumers) = ctx.subsumers.get(name) {
                            for (sub, definition) in subsumers {
                                match state.add(
                                    &root,
                                    sub.clone(),
                                    support.merge(definition),
                                    budget,
                                ) {
                                    Ok(changed) => local_changed |= changed,
                                    Err(proof) => return Saturation::Closed(proof),
                                }
                            }
                        }
                    }
                    Concept::And(concepts) => {
                        for concept in concepts {
                            match state.add(&root, concept.clone(), support.clone(), budget) {
                                Ok(changed) => local_changed |= changed,
                                Err(proof) => return Saturation::Closed(proof),
                            }
                        }
                    }
                    Concept::Nominals(members) => {
                        let root_of = state.find(&root);
                        if members.iter().any(|member| state.find(member) == root_of) {
                            continue;
                        }
                        if members.is_empty() {
                            return Saturation::Closed(state.conflict(
                                &root,
                                RefutationClash::EmptyEnumeration,
                                support.clone(),
                            ));
                        }
                        if members.len() == 1 && state.union(&root_of, &members[0], support.clone())
                        {
                            merged = true;
                            break;
                        }
                    }
                    Concept::Or(disjuncts) => {
                        let labels = state.labels_of(&root);
                        if disjuncts
                            .iter()
                            .any(|disjunct| disjunct_satisfied(&labels, disjunct))
                        {
                            continue;
                        }
                        let (live, proof) = live_disjuncts(&labels, disjuncts, support);
                        if live.is_empty() {
                            return Saturation::Closed(state.conflict(
                                &root,
                                RefutationClash::ExhaustedDisjunction,
                                proof,
                            ));
                        }
                        if live.len() == 1 {
                            match state.add(&root, live[0].clone(), proof, budget) {
                                Ok(changed) => local_changed |= changed,
                                Err(proof) => return Saturation::Closed(proof),
                            }
                        }
                    }
                    Concept::Top | Concept::Bottom | Concept::Neg(_) | Concept::Blocked => {}
                }
            }
            if merged {
                if let Some(proof) = identity_conflict(state, ctx) {
                    return Saturation::Closed(proof);
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
    if let Some(proof) = identity_conflict(state, ctx) {
        return Saturation::Closed(proof);
    }
    let roots = distinct_roots(state);
    pick_branch(state, &roots).map_or(Saturation::Open, Saturation::Branch)
}

fn pick_branch(state: &mut State, roots: &[String]) -> Option<Choice> {
    for root in roots {
        let labels = state.labels_of(root);
        for (concept, support) in &labels {
            if let Concept::Or(disjuncts) = concept {
                if disjuncts
                    .iter()
                    .any(|disjunct| disjunct_satisfied(&labels, disjunct))
                {
                    continue;
                }
                let (live, proof) = live_disjuncts(&labels, disjuncts, support);
                if live.len() >= 2 {
                    return Some(Choice {
                        support: proof.clone(),
                        alternatives: live
                            .into_iter()
                            .map(|disjunct| Action::Add(root.clone(), disjunct, proof.clone()))
                            .collect(),
                    });
                }
            }
        }
    }
    for root in roots {
        let labels = state.labels_of(root);
        let root_of = state.find(root);
        for (concept, support) in &labels {
            if let Concept::Nominals(members) = concept {
                if members.iter().any(|member| state.find(member) == root_of) {
                    continue;
                }
                if members.len() >= 2 {
                    return Some(Choice {
                        support: support.clone(),
                        alternatives: members
                            .iter()
                            .map(|member| {
                                Action::Merge(root.clone(), member.clone(), support.clone())
                            })
                            .collect(),
                    });
                }
            }
        }
    }
    None
}

fn search(state: &mut State, ctx: &Ctx, budget: &mut u64, depth: u32) -> SearchResult {
    if *budget == 0 {
        return SearchResult::Bound(SearchBound::Steps);
    }
    if depth >= SEARCH_DEPTH {
        return SearchResult::Bound(SearchBound::Depth);
    }
    match saturate(state, ctx, budget) {
        Saturation::Closed(proof) => SearchResult::Unsat(proof),
        Saturation::Open => SearchResult::Sat,
        Saturation::Bound => SearchResult::Bound(SearchBound::Steps),
        Saturation::Branch(choice) => {
            let alternatives = choice.alternatives.iter().map(Action::assumption).collect();
            let mut branches = Vec::with_capacity(choice.alternatives.len());
            for action in choice.alternatives {
                *budget = budget.saturating_sub(1);
                if *budget == 0 {
                    return SearchResult::Bound(SearchBound::Steps);
                }
                let mut child = state.clone();
                let assumption = action.assumption();
                match action {
                    Action::Add(root, concept, support) => {
                        match child.add(&root, concept, support, budget) {
                            Err(proof) => {
                                branches.push(RefutationBranch { assumption, proof });
                                continue;
                            }
                            // A non-progressing recursive source choice cannot certify either result.
                            Ok(false) => {
                                return SearchResult::Bound(if *budget == 0 {
                                    SearchBound::Steps
                                } else {
                                    SearchBound::NonProgress
                                });
                            }
                            Ok(true) => {}
                        }
                    }
                    Action::Merge(root, member, support) => {
                        child.union(&root, &member, support);
                    }
                }
                match search(&mut child, ctx, budget, depth + 1) {
                    SearchResult::Sat => return SearchResult::Sat,
                    SearchResult::Bound(bound) => return SearchResult::Bound(bound),
                    SearchResult::Unsat(proof) => {
                        branches.push(RefutationBranch { assumption, proof })
                    }
                }
            }
            SearchResult::Unsat(RefutationProof::Cases {
                choice: choice.support.rows(),
                support: choice.support.native(),
                alternatives,
                branches,
            })
        }
    }
}

#[cfg(test)]
use purrdf::RdfDataset;

#[cfg(test)]
mod scope_tests;

#[path = "casesplit.tests.rs"]
#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "casesplit_test_support.rs"]
mod test_support;
#[cfg(test)]
pub(crate) use test_support::{decide, decides};
