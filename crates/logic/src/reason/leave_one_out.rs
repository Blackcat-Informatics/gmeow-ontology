// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared exact answers for batches of source-retraction probes.
//!
//! Most slice-quality probes ask whether a taxonomy edge remains reachable after
//! deleting that exact authored edge, or whether a predicate with no producer can
//! appear after deletion.  Answer those families from one immutable source index.
//! Probes outside the certified families return `None` and retain the complete
//! native-session transaction as their authority.

use std::collections::{BTreeMap, BTreeSet};

use purrdf::TermValue;

use super::{LeaveOneOutAxiom, PreparedReasoningInput, calculus_term};

const RDFS_SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDFS_SUBPROPERTY: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const OWL_NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";
const OWL_DISJOINT: &str = "http://www.w3.org/2002/07/owl#disjointWith";
const OWL_UNION: &str = "http://www.w3.org/2002/07/owl#unionOf";
const OWL_DISJOINT_UNION: &str = "http://www.w3.org/2002/07/owl#disjointUnionOf";
const OWL_INTERSECTION: &str = "http://www.w3.org/2002/07/owl#intersectionOf";
const OWL_ONE_OF: &str = "http://www.w3.org/2002/07/owl#oneOf";
const OWL_COMPLEMENT: &str = "http://www.w3.org/2002/07/owl#complementOf";
const OWL_ALL_DISJOINT_CLASSES: &str = "http://www.w3.org/2002/07/owl#AllDisjointClasses";
const OWL_MEMBERS: &str = "http://www.w3.org/2002/07/owl#members";
const OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
const OWL_EQUIVALENT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#equivalentProperty";
const OWL_ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
const OWL_INVERSE: &str = "http://www.w3.org/2002/07/owl#inverseOf";
const OWL_PROPERTY_CHAIN: &str = "http://www.w3.org/2002/07/owl#propertyChainAxiom";
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

#[derive(Clone)]
struct Edge {
    object: String,
    /// `None` is support from an equivalence axiom and cannot be removed by a
    /// subsumption probe. `Some` retains the exact authored spelling so deleting a
    /// canonical edge does not accidentally delete a separately authored RDFS twin.
    source: Option<(String, String, String)>,
}

#[derive(Default)]
struct Reachability {
    worlds: BTreeMap<String, BTreeMap<String, Vec<Edge>>>,
}

impl Reachability {
    fn insert(
        &mut self,
        world: &str,
        subject: &str,
        object: &str,
        source: Option<(&str, &str, &str)>,
    ) {
        self.worlds
            .entry(world.to_owned())
            .or_default()
            .entry(subject.to_owned())
            .or_default()
            .push(Edge {
                object: object.to_owned(),
                source: source.map(|(s, p, o)| (s.to_owned(), p.to_owned(), o.to_owned())),
            });
    }

    fn rederived_without(&self, axiom: &LeaveOneOutAxiom) -> bool {
        self.worlds.values().any(|adjacency| {
            let target_subject = calculus_term(&axiom.subject);
            let target_object = calculus_term(&axiom.object);
            let mut visited = BTreeSet::from([target_subject]);
            let mut frontier = vec![target_subject];
            while let Some(subject) = frontier.pop() {
                let Some(edges) = adjacency.get(subject) else {
                    continue;
                };
                for edge in edges {
                    if edge.source.as_ref().is_some_and(|(s, p, o)| {
                        s == &axiom.subject && p == &axiom.predicate && o == &axiom.object
                    }) {
                        continue;
                    }
                    if edge.object == target_object {
                        return true;
                    }
                    if visited.insert(edge.object.as_str()) {
                        frontier.push(edge.object.as_str());
                    }
                }
            }
            false
        })
    }

    fn has_nonself_incoming(&self, target: &str) -> bool {
        self.worlds.values().any(|adjacency| {
            adjacency.iter().any(|(subject, edges)| {
                subject != target && edges.iter().any(|edge| edge.object == target)
            })
        })
    }
}

/// Immutable source analysis shared by every probe in one batch.
pub(super) struct BatchAnalysis {
    subclass: Reachability,
    subproperty: Reachability,
    triples: Vec<SourceTriple>,
    list_triples: Vec<SourceTermTriple>,
    predicates: BTreeSet<String>,
}

struct SourceTriple {
    subject: String,
    predicate: String,
    object: String,
    raw_subject: String,
    raw_predicate: String,
    raw_object: String,
}

struct SourceTermTriple {
    world: String,
    subject: TermValue,
    predicate: String,
    object: TermValue,
}

impl BatchAnalysis {
    pub(super) fn new(input: &PreparedReasoningInput) -> Self {
        let mut out = Self {
            subclass: Reachability::default(),
            subproperty: Reachability::default(),
            triples: Vec::new(),
            list_triples: Vec::new(),
            predicates: BTreeSet::new(),
        };
        for (world, facts) in &input.facts {
            for fact in facts {
                let predicate = calculus_term(&fact.predicate);
                if matches!(
                    predicate,
                    RDF_FIRST
                        | RDF_REST
                        | RDF_TYPE
                        | OWL_MEMBERS
                        | OWL_DISJOINT_UNION
                        | OWL_PROPERTY_CHAIN
                ) {
                    out.list_triples.push(SourceTermTriple {
                        world: world.clone(),
                        subject: fact.subject.clone(),
                        predicate: predicate.to_owned(),
                        object: fact.object.clone(),
                    });
                }
                let (TermValue::Iri(subject), TermValue::Iri(object)) =
                    (&fact.subject, &fact.object)
                else {
                    continue;
                };
                let normalized_subject = calculus_term(subject);
                let normalized_object = calculus_term(object);
                out.predicates.insert(predicate.to_owned());
                out.triples.push(SourceTriple {
                    subject: normalized_subject.to_owned(),
                    predicate: predicate.to_owned(),
                    object: normalized_object.to_owned(),
                    raw_subject: subject.clone(),
                    raw_predicate: fact.predicate.clone(),
                    raw_object: object.clone(),
                });
                match predicate {
                    RDFS_SUBCLASS => out.subclass.insert(
                        world,
                        normalized_subject,
                        normalized_object,
                        Some((subject, &fact.predicate, object)),
                    ),
                    RDFS_SUBPROPERTY => out.subproperty.insert(
                        world,
                        normalized_subject,
                        normalized_object,
                        Some((subject, &fact.predicate, object)),
                    ),
                    OWL_EQUIVALENT_CLASS => {
                        out.subclass
                            .insert(world, normalized_subject, normalized_object, None);
                        out.subclass
                            .insert(world, normalized_object, normalized_subject, None);
                    }
                    OWL_EQUIVALENT_PROPERTY => {
                        out.subproperty
                            .insert(world, normalized_subject, normalized_object, None);
                        out.subproperty
                            .insert(world, normalized_object, normalized_subject, None);
                    }
                    _ => {}
                }
            }
        }
        out
    }

    fn has_predicate(&self, predicate: &str) -> bool {
        self.predicates.contains(predicate)
    }

    fn has_pattern(&self, subject: Option<&str>, predicate: &str, object: Option<&str>) -> bool {
        self.triples.iter().any(|triple| {
            subject.is_none_or(|wanted| triple.subject == wanted)
                && triple.predicate == predicate
                && object.is_none_or(|wanted| triple.object == wanted)
        })
    }

    /// Conservative open-head test matching the schema routes that can mint a
    /// predicate not already present as that exact authored relation.
    fn may_generate(&self, target: &str) -> bool {
        self.triples.iter().any(|triple| {
            (triple.predicate == OWL_ON_PROPERTY && triple.object == target)
                || (triple.predicate == RDFS_SUBPROPERTY
                    && (triple.subject == target || triple.object == target))
                || ((triple.predicate == OWL_INVERSE
                    || triple.predicate == OWL_EQUIVALENT_PROPERTY)
                    && (triple.subject == target || triple.object == target))
                || (triple.predicate == OWL_PROPERTY_CHAIN && triple.subject == target)
        }) || self.list_triples.iter().any(|triple| {
            triple.predicate == OWL_PROPERTY_CHAIN
                && matches!(&triple.subject, TermValue::Iri(subject) if calculus_term(subject) == target)
        })
    }

    fn subclass_exact(&self, axiom: &LeaveOneOutAxiom) -> bool {
        let subject = calculus_term(&axiom.subject);
        let object = calculus_term(&axiom.object);
        // The two fixed list laws that mint subClassOf are local: a union produces
        // `member subClassOf union-class`, while an intersection produces
        // `intersection-class subClassOf member`. An unrelated constructor elsewhere
        // in a large vocabulary cannot possibly rederive this probe. The former global
        // predicate test routed every taxonomy probe in the logic slice through a full
        // native transaction because that slice also happens to define one union.
        !self.has_pattern(Some(object), OWL_UNION, None)
            && !self.has_pattern(Some(subject), OWL_INTERSECTION, None)
            && !self.has_predicate(OWL_DISJOINT_UNION)
            && !self.may_generate(OWL_UNION)
            && !self.may_generate(OWL_DISJOINT_UNION)
            && !self.may_generate(RDFS_SUBCLASS)
            && !self.may_generate(OWL_EQUIVALENT_CLASS)
    }

    fn subproperty_exact(&self) -> bool {
        !self.may_generate(RDFS_SUBPROPERTY) && !self.may_generate(OWL_EQUIVALENT_PROPERTY)
    }

    fn direct_alternate_support(&self, axiom: &LeaveOneOutAxiom, predicate: &str) -> bool {
        let subject = calculus_term(&axiom.subject);
        let object = calculus_term(&axiom.object);
        self.triples.iter().any(|triple| {
            triple.subject == subject
                && triple.predicate == predicate
                && triple.object == object
                && (triple.raw_subject != axiom.subject
                    || triple.raw_predicate != axiom.predicate
                    || triple.raw_object != axiom.object)
        })
    }

    fn term_object(&self, world: &str, subject: &TermValue, predicate: &str) -> Option<&TermValue> {
        self.list_triples.iter().find_map(|triple| {
            (triple.world == world && &triple.subject == subject && triple.predicate == predicate)
                .then_some(&triple.object)
        })
    }

    fn list_contains_pair(&self, world: &str, head: &TermValue, left: &str, right: &str) -> bool {
        let mut node = head.clone();
        let mut seen = Vec::new();
        let mut has_left = false;
        let mut has_right = false;
        while !matches!(&node, TermValue::Iri(iri) if iri == RDF_NIL) && !seen.contains(&node) {
            seen.push(node.clone());
            let Some(member) = self.term_object(world, &node, RDF_FIRST) else {
                return false;
            };
            has_left |= matches!(member, TermValue::Iri(iri) if calculus_term(iri) == left);
            has_right |= matches!(member, TermValue::Iri(iri) if calculus_term(iri) == right);
            let Some(rest) = self.term_object(world, &node, RDF_REST) else {
                return false;
            };
            node = rest.clone();
        }
        matches!(&node, TermValue::Iri(iri) if iri == RDF_NIL) && has_left && has_right
    }

    fn disjoint_rederived_without(&self, axiom: &LeaveOneOutAxiom) -> bool {
        let left = calculus_term(&axiom.subject);
        let right = calculus_term(&axiom.object);
        if self.direct_alternate_support(axiom, OWL_DISJOINT) {
            return true;
        }
        if self.triples.iter().any(|triple| {
            if triple.predicate == OWL_COMPLEMENT
                && ((triple.subject == left && triple.object == right)
                    || (triple.subject == right && triple.object == left))
            {
                return true;
            }
            false
        }) {
            return true;
        }
        self.list_triples.iter().any(|triple| {
            if triple.predicate == OWL_DISJOINT_UNION {
                return self.list_contains_pair(&triple.world, &triple.object, left, right);
            }
            if triple.predicate != RDF_TYPE
                || !matches!(&triple.object, TermValue::Iri(iri) if calculus_term(iri) == OWL_ALL_DISJOINT_CLASSES)
            {
                return false;
            }
            self.term_object(&triple.world, &triple.subject, OWL_MEMBERS)
                .is_some_and(|head| self.list_contains_pair(&triple.world, head, left, right))
        })
    }

    fn characteristic_has_no_alternative_producer(&self, axiom: &LeaveOneOutAxiom) -> bool {
        let characteristic = calculus_term(&axiom.object);
        if calculus_term(&axiom.predicate) != RDF_TYPE
            || !is_property_characteristic(characteristic)
        {
            return false;
        }
        if self.subclass.has_nonself_incoming(characteristic)
            || self.has_pattern(None, RDFS_DOMAIN, Some(characteristic))
            || self.has_pattern(None, RDFS_RANGE, Some(characteristic))
        {
            return false;
        }
        for predicate in [
            OWL_ON_PROPERTY,
            OWL_UNION,
            OWL_DISJOINT_UNION,
            OWL_INTERSECTION,
            OWL_ONE_OF,
        ] {
            if self.has_pattern(Some(characteristic), predicate, None) {
                return false;
            }
        }
        [
            RDF_TYPE,
            RDFS_DOMAIN,
            RDFS_RANGE,
            OWL_ONE_OF,
            OWL_INTERSECTION,
        ]
        .into_iter()
        .all(|predicate| !self.may_generate(predicate))
    }

    /// Return an exact batch answer, or `None` when the complete native transaction
    /// remains necessary.
    pub(super) fn answer(&self, axiom: &LeaveOneOutAxiom) -> Option<bool> {
        let predicate = calculus_term(&axiom.predicate);
        if predicate == RDFS_SUBCLASS && axiom.object != OWL_NOTHING && self.subclass_exact(axiom) {
            return Some(self.subclass.rederived_without(axiom));
        }
        if predicate == RDFS_SUBPROPERTY && self.subproperty_exact() {
            return Some(self.subproperty.rederived_without(axiom));
        }

        // These schema relations are not heads of the fixed calculus. If no
        // open-headed producer can mint the relation, deletion cannot rederive it.
        if matches!(
            predicate,
            RDFS_DOMAIN | RDFS_RANGE | OWL_EQUIVALENT_CLASS | OWL_EQUIVALENT_PROPERTY | OWL_INVERSE
        ) && !self.may_generate(predicate)
        {
            return Some(self.direct_alternate_support(axiom, predicate));
        }

        // The fixed calculus mints class disjointness only from complement pairs,
        // disjoint-union lists, and owl:AllDisjointClasses lists. Inspect those exact
        // witnesses in their own logical world; unrelated constructors cannot force a
        // complete source-retraction transaction.
        if predicate == OWL_DISJOINT && !self.may_generate(predicate) {
            return Some(self.disjoint_rederived_without(axiom));
        }

        if self.characteristic_has_no_alternative_producer(axiom) {
            return Some(self.direct_alternate_support(axiom, predicate));
        }

        // Keep rdf:type and every unclassified family on the native path. Their
        // producer set includes class propagation, restrictions, and datatype laws.
        None
    }
}

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

fn is_property_characteristic(iri: &str) -> bool {
    matches!(
        iri,
        "http://www.w3.org/2002/07/owl#TransitiveProperty"
            | "http://www.w3.org/2002/07/owl#SymmetricProperty"
            | "http://www.w3.org/2002/07/owl#AsymmetricProperty"
            | "http://www.w3.org/2002/07/owl#ReflexiveProperty"
            | "http://www.w3.org/2002/07/owl#IrreflexiveProperty"
            | "http://www.w3.org/2002/07/owl#FunctionalProperty"
            | "http://www.w3.org/2002/07/owl#InverseFunctionalProperty"
    )
}
