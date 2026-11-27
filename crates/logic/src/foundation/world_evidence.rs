// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Admissible evidence for the cross-world judgments of the foundation evaluator.
//!
//! A cross-world rigidity judgment and an anti-rigidity witness both assert that an
//! individual `x` lost membership of a type `T` in another world `w2`.  This module
//! indexes the materialized quads once and answers the three questions such a
//! judgment needs:
//!
//! * **Accessibility.**  `w2` is judged from `w1` only when a typed accessibility
//!   record `w1 R w2` is present, with `R` one of [`ADMISSIBLE_ACCESSIBILITY`].  The
//!   bare `logic:accessibleFrom` superproperty licenses no cross-type entailment and
//!   is not a record.
//! * **Existence.**  `x` is the subject of at least one quad in `w2` other than a
//!   non-membership assertion about `x` itself.
//! * **Loss.**  `x` is not typed `T` in `w2`, and the absence is backed by evidence:
//!   an explicit `x logic:notInstanceOf T` assertion in `w2`, or a
//!   `logic:ClosureEntry` in `w2` that closes the world for `T` over the individual
//!   `x`.  Absence of a typing alone is never evidence.

use std::collections::{BTreeMap, BTreeSet};

use super::{FoundationQuad, LOGIC_NS, RDF_TYPE, strip_angle_opt};

/// Typed accessibility relations a cross-world existence judgment follows: the
/// alethic relation and temporal succession.  The doxastic, deontic, counterfactual
/// and standpoint relations relate contexts whose members need not be states of the
/// individual's existence, so they never carry one.
const ADMISSIBLE_ACCESSIBILITY: [&str; 2] = ["epistemicallyPossible", "temporallySucceeds"];

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
    /// `(world, individual, type)` triples a closed-world `logic:ClosureEntry` covers.
    closed: BTreeSet<Typing>,
    /// Typed accessibility records: source world to the worlds accessible from it.
    accessible: BTreeMap<String, BTreeSet<String>>,
}

/// The entry-local facts of one `logic:ClosureEntry` while the quads are scanned.
#[derive(Debug, Default)]
struct ClosureEntryFacts {
    is_entry: bool,
    keys: Vec<String>,
    values: Vec<String>,
    individuals: Vec<String>,
}

impl WorldEvidence {
    /// Index `quads` (the union of every materialized world).
    pub(super) fn new(quads: &[FoundationQuad]) -> Self {
        let not_instance_of = format!("{LOGIC_NS}notInstanceOf");
        let admissible: BTreeSet<String> = ADMISSIBLE_ACCESSIBILITY
            .iter()
            .map(|local| format!("{LOGIC_NS}{local}"))
            .collect();
        let closure_entry = format!("{LOGIC_NS}ClosureEntry");
        let closed_world = format!("{LOGIC_NS}ClosedWorldClosure");
        let mut evidence = Self::default();
        let mut entries: BTreeMap<(String, String), ClosureEntryFacts> = BTreeMap::new();
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
            }
            if admissible.contains(&quad.predicate) {
                evidence
                    .accessible
                    .entry(quad.subject.clone())
                    .or_default()
                    .insert(object.to_owned());
            }
            let local = quad.predicate.strip_prefix(LOGIC_NS);
            let entry_marker = quad.predicate == RDF_TYPE && object == closure_entry;
            if !entry_marker
                && !matches!(
                    local,
                    Some("closureKey" | "closureValue" | "closureIndividual")
                )
            {
                continue;
            }
            let facts = entries
                .entry((quad.graph.clone(), quad.subject.clone()))
                .or_default();
            let object = object.to_owned();
            match local {
                Some("closureKey") => facts.keys.push(object),
                Some("closureValue") => facts.values.push(object),
                Some("closureIndividual") => facts.individuals.push(object),
                _ => facts.is_entry = true,
            }
        }
        for ((world, _), facts) in entries {
            // Only an entry whose every value is the closed-world closure applies: a
            // missing, open or conflicting value leaves the key open, the default.
            if !facts.is_entry
                || facts.values.is_empty()
                || facts.values.iter().any(|value| *value != closed_world)
            {
                continue;
            }
            for type_iri in &facts.keys {
                for individual in &facts.individuals {
                    evidence
                        .closed
                        .insert((world.clone(), individual.clone(), type_iri.clone()));
                }
            }
        }
        evidence
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
        let key = (world.to_owned(), individual.to_owned(), type_iri.to_owned());
        self.existing
            .contains(&(world.to_owned(), individual.to_owned()))
            && !self.typings.contains(&key)
            && (self.non_members.contains(&key) || self.closed.contains(&key))
    }
}
