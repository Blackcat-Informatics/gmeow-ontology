// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Admissible evidence for the cross-world judgments of the foundation evaluator.
//!
//! A cross-world rigidity judgment and an anti-rigidity witness both assert that an
//! individual `x` lost membership of a type `T` in another world `w2`.  This module
//! indexes the materialized quads once and answers the three questions such a
//! judgment needs:
//!
//! * **Accessibility.**  `w2` is judged from `w1` only along a typed accessibility
//!   record asserted in the graph of the record's subject: `w1
//!   logic:epistemicallyPossible w2` asserted in `w1`, or `w2 logic:temporallySucceeds
//!   w1` asserted in `w2` (the later state names its predecessor, so a later state is
//!   reached from the earlier one).  A record asserted in any other graph, the bare
//!   `logic:accessibleFrom` superproperty and every other typed relation license
//!   nothing.
//! * **Existence.**  `x` is the subject of at least one quad in `w2` other than a
//!   non-membership assertion about `x` itself.
//! * **Loss.**  `x` is not typed `T` in `w2`, and the absence is backed by evidence:
//!   an explicit `x logic:notInstanceOf T` assertion in `w2`, or a closed-world
//!   closure of `x`'s `rdf:type` statements that a `logic:ReasoningContract` asserted
//!   in `w2` declares through `logic:closureEntry`.  Absence of a typing alone is
//!   never evidence.

use std::collections::{BTreeMap, BTreeSet};

use super::{FoundationQuad, LOGIC_NS, RDF_TYPE, foundation_err, strip_angle_opt};

/// One typing: `(world, individual, type)`.
pub(super) type Typing = (String, String, String);

/// Indexed evidence over one materialized quad set.
#[derive(Debug, Default)]
pub(super) struct WorldEvidence {
    /// Every `(world, individual, type)` with an IRI-typed `rdf:type` quad.
    typings: BTreeSet<Typing>,
    /// `(world, individual)` pairs that exist: the individual is a subject of a quad
    /// that is not its own non-membership assertion.
    existing: BTreeSet<(String, String)>,
    /// Explicit `(world, individual, type)` non-membership assertions.
    non_members: BTreeSet<Typing>,
    /// `(world, individual)` pairs whose `rdf:type` statements the world's contract
    /// closes under the closed-world closure.
    closed: BTreeSet<(String, String)>,
    /// Typed accessibility records: judging world to the worlds accessible from it.
    accessible: BTreeMap<String, BTreeSet<String>>,
}

/// The fields of one closure entry, keyed by `(world, entry)` while the quads are
/// scanned.  Sets keep identical repetitions harmless; distinct values are ambiguous.
#[derive(Debug, Default)]
struct ClosureEntryFacts {
    keys: BTreeSet<String>,
    values: BTreeSet<String>,
    individuals: BTreeSet<String>,
}

impl ClosureEntryFacts {
    /// The `logic:` local names of the fields an entry carries.
    const FIELDS: [&'static str; 3] = ["closureKey", "closureValue", "closureIndividual"];

    fn field_mut(&mut self, local: &str) -> Option<&mut BTreeSet<String>> {
        match local {
            "closureKey" => Some(&mut self.keys),
            "closureValue" => Some(&mut self.values),
            "closureIndividual" => Some(&mut self.individuals),
            _ => None,
        }
    }
}

impl WorldEvidence {
    /// Index `quads` (the union of every materialized world).
    ///
    /// # Errors
    ///
    /// Refuses an individual-scoped closure entry of a world's contract that has
    /// several distinct keys, values or individuals, that lacks its key or value, or
    /// that assigns a value conflicting with another entry of the same world for the
    /// same individual's `rdf:type` statements.
    pub(super) fn new(quads: &[FoundationQuad]) -> gmeow_errors::Result<Self> {
        let not_instance_of = format!("{LOGIC_NS}notInstanceOf");
        let epistemic = format!("{LOGIC_NS}epistemicallyPossible");
        let temporal = format!("{LOGIC_NS}temporallySucceeds");
        let contract_class = format!("{LOGIC_NS}ReasoningContract");
        let closure_entry = format!("{LOGIC_NS}closureEntry");
        let mut evidence = Self::default();
        let mut contracts: BTreeSet<(&str, &str)> = BTreeSet::new();
        let mut ownership: Vec<(&str, &str, &str)> = Vec::new();
        let mut entries: BTreeMap<(&str, &str), ClosureEntryFacts> = BTreeMap::new();
        for quad in quads {
            let object = strip_angle_opt(&quad.object);
            if quad.predicate == not_instance_of {
                if let Some(type_iri) = object {
                    evidence.non_members.insert((
                        quad.graph.clone(),
                        quad.subject.clone(),
                        type_iri.to_owned(),
                    ));
                }
            } else {
                evidence
                    .existing
                    .insert((quad.graph.clone(), quad.subject.clone()));
            }
            let Some(object) = object else { continue };
            if quad.predicate == RDF_TYPE {
                evidence.typings.insert((
                    quad.graph.clone(),
                    quad.subject.clone(),
                    object.to_owned(),
                ));
                if object == contract_class {
                    contracts.insert((quad.graph.as_str(), quad.subject.as_str()));
                }
            }
            if quad.graph == quad.subject {
                if quad.predicate == epistemic {
                    evidence
                        .accessible
                        .entry(quad.subject.clone())
                        .or_default()
                        .insert(object.to_owned());
                } else if quad.predicate == temporal {
                    evidence
                        .accessible
                        .entry(object.to_owned())
                        .or_default()
                        .insert(quad.subject.clone());
                }
            }
            if quad.predicate == closure_entry {
                ownership.push((quad.graph.as_str(), quad.subject.as_str(), object));
                continue;
            }
            let Some(local) = quad.predicate.strip_prefix(LOGIC_NS) else {
                continue;
            };
            if ClosureEntryFacts::FIELDS.contains(&local)
                && let Some(field) = entries
                    .entry((quad.graph.as_str(), quad.subject.as_str()))
                    .or_default()
                    .field_mut(local)
            {
                field.insert(object.to_owned());
            }
        }
        // An entry counts only when a contract asserted in the same world owns it.
        let contract_entries: BTreeSet<(&str, &str)> = ownership
            .into_iter()
            .filter(|(world, contract, _)| contracts.contains(&(*world, *contract)))
            .map(|(world, _, entry)| (world, entry))
            .collect();
        let mut membership_closure: BTreeMap<(&str, &str), &str> = BTreeMap::new();
        for (&(world, entry), facts) in &entries {
            if facts.individuals.is_empty() || !contract_entries.contains(&(world, entry)) {
                continue;
            }
            let (Some(key), Some(value), Some(individual)) = (
                single(&facts.keys, "closureKey", world, entry)?,
                single(&facts.values, "closureValue", world, entry)?,
                single(&facts.individuals, "closureIndividual", world, entry)?,
            ) else {
                return Err(foundation_err(format!(
                    "closure entry <{entry}> in world <{world}> is scoped to an individual but \
                     lacks logic:closureKey or logic:closureValue"
                )));
            };
            if key != RDF_TYPE {
                continue;
            }
            if let Some(previous) = membership_closure.insert((world, individual), value)
                && previous != value
            {
                return Err(foundation_err(format!(
                    "world <{world}> assigns conflicting closure values <{previous}> and \
                     <{value}> to the rdf:type statements of <{individual}>"
                )));
            }
        }
        let closed_world = format!("{LOGIC_NS}ClosedWorldClosure");
        evidence.closed = membership_closure
            .into_iter()
            .filter(|(_, value)| *value == closed_world)
            .map(|((world, individual), _)| (world.to_owned(), individual.to_owned()))
            .collect();
        Ok(evidence)
    }

    /// Every `(world, individual, type)` typing, in canonical order.
    pub(super) fn typings(&self) -> impl Iterator<Item = &Typing> {
        self.typings.iter()
    }

    /// The worlds reachable from `world` along an admissible typed accessibility record.
    pub(super) fn accessible_from<'a>(&'a self, world: &str) -> impl Iterator<Item = &'a String> {
        self.accessible.get(world).into_iter().flatten()
    }

    /// Whether `individual` demonstrably existed in `world` without being a `type_iri`:
    /// it is present, not typed, and its absence of the type is explicitly asserted or
    /// closed-world covered.
    pub(super) fn lost_in(&self, world: &str, individual: &str, type_iri: &str) -> bool {
        let present = (world.to_owned(), individual.to_owned());
        let key = (world.to_owned(), individual.to_owned(), type_iri.to_owned());
        self.existing.contains(&present)
            && !self.typings.contains(&key)
            && (self.non_members.contains(&key) || self.closed.contains(&present))
    }
}

/// The one value of a closure-entry field; several distinct values are ambiguous, the
/// same refusal the contract frontend applies.
fn single<'a>(
    values: &'a BTreeSet<String>,
    field: &str,
    world: &str,
    entry: &str,
) -> gmeow_errors::Result<Option<&'a str>> {
    if values.len() > 1 {
        return Err(foundation_err(format!(
            "closure entry <{entry}> in world <{world}> has multiple distinct logic:{field} values"
        )));
    }
    Ok(values.first().map(String::as_str))
}
