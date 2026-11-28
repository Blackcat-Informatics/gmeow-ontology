// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Property restrictions the class model decides exactly.
//!
//! The class model reasons over named classes and their Boolean combinations. A restriction
//! node enters it as one atomic class, which is exact only when the restriction cannot
//! interact with any other axiom: every Boolean combination of the atom and its complement
//! must stay realizable by an individual whose property successors are free of other
//! constraints. A restriction is therefore decided when
//!
//! * its property occurs in no other premise and is never asserted between individuals, so
//!   no other restriction, property axiom or assertion constrains the successors;
//! * its filler class or individual occurs in no other premise, so nothing can empty it;
//! * the restriction and its complement are both satisfiable: a filler of `owl:Thing` or
//!   `owl:Nothing` and a minimum of zero make one of the two empty, and stay undecided.
//!
//! A node that names a property and states no qualifier constrains nothing and is decided
//! without those conditions. Every other restriction premise stays a retained obstruction.

use std::collections::{BTreeMap, BTreeSet};

use purrdf::TermValue;

use super::{Concept, RDF_TYPE, Resolver, WorldData, semantic_predicate};

const ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
const ON_CLASS: &str = "http://www.w3.org/2002/07/owl#onClass";
const SOME_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#someValuesFrom";
const ALL_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#allValuesFrom";
const HAS_VALUE: &str = "http://www.w3.org/2002/07/owl#hasValue";
const CARDINALITY: &str = "http://www.w3.org/2002/07/owl#cardinality";
const MIN_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#minCardinality";
const MAX_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#maxCardinality";
const QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#qualifiedCardinality";
const MIN_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#minQualifiedCardinality";
const MAX_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#maxQualifiedCardinality";
const OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const NAMED_INDIVIDUAL: &str = "http://www.w3.org/2002/07/owl#NamedIndividual";

/// Declarations that add no constraint to the term they declare.
const NEUTRAL_DECLARATIONS: &[&str] = &[OBJECT_PROPERTY, CLASS, NAMED_INDIVIDUAL];

/// The predicates that make up one restriction; any other blocking predicate on the node
/// leaves it undecided.
const STRUCTURAL: &[&str] = &[
    ON_PROPERTY,
    ON_CLASS,
    SOME_VALUES_FROM,
    ALL_VALUES_FROM,
    HAS_VALUE,
    CARDINALITY,
    MIN_CARDINALITY,
    MAX_CARDINALITY,
    QUALIFIED_CARDINALITY,
    MIN_QUALIFIED_CARDINALITY,
    MAX_QUALIFIED_CARDINALITY,
];

/// True for a predicate that is part of one restriction's own definition.
pub(super) fn is_structural(predicate: &str) -> bool {
    STRUCTURAL.contains(&predicate)
}

/// The shape of one restriction node, read from its own premises.
enum Shape<'a> {
    /// A property is named and no qualifier is stated.
    Bare,
    /// `someValuesFrom`, `allValuesFrom` or `hasValue` with a resource filler.
    Filler(&'a str),
    /// An unqualified cardinality.
    Count { minimum: bool, count: u64 },
    /// A qualified cardinality with its `owl:onClass` filler.
    QualifiedCount {
        minimum: bool,
        count: u64,
        filler: &'a str,
    },
}

impl WorldData {
    /// The restriction nodes whose premises the class model decides.
    pub(super) fn decided_restrictions(&self, resolver: &Resolver<'_>) -> BTreeSet<String> {
        // How many premises mention each term, ignoring neutral declarations of it.
        let mut mentions = BTreeMap::<&str, usize>::new();
        let mut structural = BTreeMap::<&str, Vec<(&str, &TermValue)>>::new();
        let mut undecided = BTreeSet::<&str>::new();
        for (subject, predicate, object) in self.source_premises.keys() {
            let neutral = predicate == RDF_TYPE
                && object
                    .as_iri()
                    .is_some_and(|marker| NEUTRAL_DECLARATIONS.contains(&marker));
            if !neutral {
                *mentions.entry(subject).or_default() += 1;
                if let Some(object) = object.as_iri() {
                    *mentions.entry(object).or_default() += 1;
                }
            }
            if STRUCTURAL.contains(&predicate.as_str()) {
                structural
                    .entry(subject)
                    .or_default()
                    .push((predicate, object));
            } else if super::CONSISTENT_BLOCKING_PREDICATES.contains(&predicate.as_str()) {
                undecided.insert(subject);
            }
        }
        let unconstrained = |term: &str| {
            mentions.get(term).copied() == Some(1)
                && matches!(resolver.resolve_supported(term, 0).0, Concept::Pos(_))
        };
        let isolated_property = |property: &str| {
            mentions.get(property).copied() == Some(1)
                && !self.predicates.contains(property)
                && !self.predicates.contains(&semantic_predicate(property))
        };
        structural
            .into_iter()
            .filter(|(node, premises)| {
                !undecided.contains(node)
                    && !resolver.is_expression(node)
                    && Self::shape(premises).is_some_and(|(property, shape)| match shape {
                        Shape::Bare => true,
                        Shape::Filler(filler) => {
                            isolated_property(property) && unconstrained(filler)
                        }
                        Shape::Count { minimum, count } => {
                            isolated_property(property) && !(minimum && count == 0)
                        }
                        Shape::QualifiedCount {
                            minimum,
                            count,
                            filler,
                        } => {
                            isolated_property(property)
                                && !(minimum && count == 0)
                                && unconstrained(filler)
                        }
                    })
            })
            .map(|(node, _)| node.to_owned())
            .collect()
    }

    /// Read a node's single property and qualifier, or nothing when its premises are
    /// ambiguous, malformed or outside the decided shapes.
    fn shape<'a>(premises: &[(&'a str, &'a TermValue)]) -> Option<(&'a str, Shape<'a>)> {
        let one = |wanted: &str| {
            let mut found = premises
                .iter()
                .filter(|(predicate, _)| *predicate == wanted);
            let first = found.next()?;
            found.next().is_none().then_some(first.1)
        };
        let count = |wanted: &str| {
            premises
                .iter()
                .filter(|(predicate, _)| *predicate == wanted)
                .count()
        };
        let property = one(ON_PROPERTY)?.as_iri()?;
        let qualifiers: Vec<_> = premises
            .iter()
            .map(|(predicate, _)| *predicate)
            .filter(|predicate| ![ON_PROPERTY, ON_CLASS].contains(predicate))
            .collect();
        let number = |predicate: &str| match one(predicate)? {
            TermValue::Literal { lexical_form, .. } => lexical_form.parse::<u64>().ok(),
            _ => None,
        };
        let shape = match (qualifiers.as_slice(), count(ON_CLASS)) {
            ([], 0) => Shape::Bare,
            ([SOME_VALUES_FROM | ALL_VALUES_FROM | HAS_VALUE], 0) => {
                Shape::Filler(one(qualifiers[0])?.as_iri()?)
            }
            ([CARDINALITY | MIN_CARDINALITY | MAX_CARDINALITY], 0) => Shape::Count {
                minimum: qualifiers[0] == MIN_CARDINALITY,
                count: number(qualifiers[0])?,
            },
            (
                [QUALIFIED_CARDINALITY | MIN_QUALIFIED_CARDINALITY | MAX_QUALIFIED_CARDINALITY],
                1,
            ) => Shape::QualifiedCount {
                minimum: qualifiers[0] == MIN_QUALIFIED_CARDINALITY,
                count: number(qualifiers[0])?,
                filler: one(ON_CLASS)?.as_iri()?,
            },
            _ => return None,
        };
        Some((property, shape))
    }
}
