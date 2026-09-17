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
const OWL_UNION: &str = "http://www.w3.org/2002/07/owl#unionOf";
const OWL_DISJOINT_UNION: &str = "http://www.w3.org/2002/07/owl#disjointUnionOf";
const OWL_INTERSECTION: &str = "http://www.w3.org/2002/07/owl#intersectionOf";
const OWL_ONE_OF: &str = "http://www.w3.org/2002/07/owl#oneOf";
const OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
const OWL_EQUIVALENT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#equivalentProperty";
const OWL_ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
const OWL_INVERSE: &str = "http://www.w3.org/2002/07/owl#inverseOf";
const OWL_PROPERTY_CHAIN: &str = "http://www.w3.org/2002/07/owl#propertyChainAxiom";

#[derive(Clone)]
struct Edge {
    object: String,
    /// `None` is support from an equivalence axiom and cannot be removed by a
    /// subsumption probe. `Some` retains the exact authored spelling so deleting a
    /// canonical edge does not accidentally delete a separately authored RDFS twin.
    source_predicate: Option<String>,
}

#[derive(Default)]
struct Reachability {
    worlds: BTreeMap<String, BTreeMap<String, Vec<Edge>>>,
}

impl Reachability {
    fn insert(&mut self, world: &str, subject: &str, object: &str, source_predicate: Option<&str>) {
        self.worlds
            .entry(world.to_owned())
            .or_default()
            .entry(subject.to_owned())
            .or_default()
            .push(Edge {
                object: object.to_owned(),
                source_predicate: source_predicate.map(str::to_owned),
            });
    }

    fn rederived_without(&self, axiom: &LeaveOneOutAxiom) -> bool {
        self.worlds.values().any(|adjacency| {
            let mut visited = BTreeSet::from([axiom.subject.as_str()]);
            let mut frontier = vec![axiom.subject.as_str()];
            while let Some(subject) = frontier.pop() {
                let Some(edges) = adjacency.get(subject) else {
                    continue;
                };
                for edge in edges {
                    if subject == axiom.subject
                        && edge.object == axiom.object
                        && edge.source_predicate.as_deref() == Some(axiom.predicate.as_str())
                    {
                        continue;
                    }
                    if edge.object == axiom.object {
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
    triples: Vec<(String, String, String, String)>,
    predicates: BTreeSet<String>,
}

impl BatchAnalysis {
    pub(super) fn new(input: &PreparedReasoningInput) -> Self {
        let mut out = Self {
            subclass: Reachability::default(),
            subproperty: Reachability::default(),
            triples: Vec::new(),
            predicates: BTreeSet::new(),
        };
        for (world, facts) in &input.facts {
            for fact in facts {
                let (TermValue::Iri(subject), TermValue::Iri(object)) =
                    (&fact.subject, &fact.object)
                else {
                    continue;
                };
                let predicate = calculus_term(&fact.predicate);
                out.predicates.insert(predicate.to_owned());
                out.triples.push((
                    subject.clone(),
                    predicate.to_owned(),
                    object.clone(),
                    fact.predicate.clone(),
                ));
                match predicate {
                    RDFS_SUBCLASS => {
                        out.subclass
                            .insert(world, subject, object, Some(&fact.predicate))
                    }
                    RDFS_SUBPROPERTY => {
                        out.subproperty
                            .insert(world, subject, object, Some(&fact.predicate))
                    }
                    OWL_EQUIVALENT_CLASS => {
                        out.subclass.insert(world, subject, object, None);
                        out.subclass.insert(world, object, subject, None);
                    }
                    OWL_EQUIVALENT_PROPERTY => {
                        out.subproperty.insert(world, subject, object, None);
                        out.subproperty.insert(world, object, subject, None);
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
        self.triples.iter().any(|(s, p, o, _)| {
            subject.is_none_or(|wanted| s == wanted)
                && p == predicate
                && object.is_none_or(|wanted| o == wanted)
        })
    }

    /// Conservative open-head test matching the schema routes that can mint a
    /// predicate not already present as that exact authored relation.
    fn may_generate(&self, target: &str) -> bool {
        self.triples.iter().any(|(subject, predicate, object, _)| {
            (*predicate == OWL_ON_PROPERTY && object == target)
                || (*predicate == RDFS_SUBPROPERTY && (subject == target || object == target))
                || ((*predicate == OWL_INVERSE || *predicate == OWL_EQUIVALENT_PROPERTY)
                    && (subject == target || object == target))
                || (*predicate == OWL_PROPERTY_CHAIN && subject == target)
        })
    }

    fn subclass_exact(&self) -> bool {
        !self.has_predicate(OWL_UNION)
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
        self.triples
            .iter()
            .any(|(subject, existing, object, source_predicate)| {
                subject == &axiom.subject
                    && existing == predicate
                    && object == &axiom.object
                    && source_predicate != &axiom.predicate
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
        if predicate == RDFS_SUBCLASS && axiom.object != OWL_NOTHING && self.subclass_exact() {
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

        if self.characteristic_has_no_alternative_producer(axiom) {
            return Some(false);
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
