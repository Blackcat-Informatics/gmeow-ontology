// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Admissible evidence for the cross-world judgments of the foundation evaluator.
//!
//! A cross-world rigidity judgment and an anti-rigidity witness both assert that an
//! individual `x` lost membership of a type `T` in another world `w2`.  This module
//! indexes the materialized quads once and answers the three questions such a
//! judgment needs:
//!
//! * **Accessibility.**  `w2` is judged from `w1` only along typed accessibility
//!   records, each asserted in the graph of its own subject: `w1
//!   logic:epistemicallyPossible w2` asserted in `w1`, or a chain of `w2
//!   logic:temporallySucceeds w1` records, each asserted in its own subject's graph (the
//!   later state names its predecessor, so every later state, at any number of steps, is
//!   reached from the earlier one).  The epistemic record is not transitive.  A record
//!   asserted in any other graph, the bare
//!   `logic:accessibleFrom` superproperty and every other typed relation license
//!   nothing.
//! * **Existence.**  `x` is the subject of at least one quad in `w2` other than a
//!   non-membership assertion about `x` itself.
//! * **Loss.**  `x` is not a `T` in `w2`, where `x`'s types are its asserted `rdf:type`
//!   values closed upward under the `logic:subClassOf` triples asserted in `w2` (the
//!   subclass predicate the foundation chase reads), and the absence is backed by evidence:
//!   an explicit `x logic:notInstanceOf T` assertion in `w2`, or a closed-world
//!   closure of `x`'s `rdf:type` statements that the one `logic:ReasoningContract` asserted
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
    /// Every `(world, individual, type)` a world entails: the asserted typings closed upward
    /// under the `logic:subClassOf` triples asserted in the same world.
    entailed: BTreeSet<Typing>,
    /// `(world, individual)` pairs that exist: the individual is a subject of a quad
    /// that is not its own non-membership assertion.
    existing: BTreeSet<(String, String)>,
    /// Explicit `(world, individual, type)` non-membership assertions.
    non_members: BTreeSet<Typing>,
    /// `(world, individual)` pairs whose `rdf:type` statements the world's contract
    /// closes under the closed-world closure.
    closed: BTreeSet<(String, String)>,
    /// Direct epistemic records: judging world to the worlds it names as possible.
    epistemic: BTreeMap<String, BTreeSet<String>>,
    /// Direct temporal records, read from the named predecessor to the later state that
    /// asserts the record.
    successors: BTreeMap<String, BTreeSet<String>>,
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
    /// Refuses a world that asserts more than one `logic:ReasoningContract`, naming the
    /// contracts.  Refuses an individual-scoped closure entry of the world's contract that has
    /// several distinct keys, values or individuals, that lacks its key or value, or
    /// that assigns a value conflicting with another entry of the same world for the
    /// same individual's `rdf:type` statements.  Refuses cyclic `logic:temporallySucceeds`
    /// records.
    pub(super) fn new(quads: &[FoundationQuad]) -> gmeow_errors::Result<Self> {
        let not_instance_of = format!("{LOGIC_NS}notInstanceOf");
        let epistemic = format!("{LOGIC_NS}epistemicallyPossible");
        let temporal = format!("{LOGIC_NS}temporallySucceeds");
        let contract_class = format!("{LOGIC_NS}ReasoningContract");
        let closure_entry = format!("{LOGIC_NS}closureEntry");
        let mut evidence = Self::default();
        let mut contracts: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        let mut ownership: Vec<(&str, &str, &str)> = Vec::new();
        let sub_class_of = format!("{LOGIC_NS}subClassOf");
        let mut entries: BTreeMap<(&str, &str), ClosureEntryFacts> = BTreeMap::new();
        let mut supers: BTreeMap<(&str, &str), BTreeSet<&str>> = BTreeMap::new();
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
            if quad.predicate == sub_class_of {
                supers
                    .entry((quad.graph.as_str(), quad.subject.as_str()))
                    .or_default()
                    .insert(object);
            }
            if quad.predicate == RDF_TYPE {
                evidence.typings.insert((
                    quad.graph.clone(),
                    quad.subject.clone(),
                    object.to_owned(),
                ));
                if object == contract_class {
                    contracts
                        .entry(quad.graph.as_str())
                        .or_default()
                        .insert(quad.subject.as_str());
                }
            }
            if quad.graph == quad.subject {
                if quad.predicate == epistemic {
                    evidence
                        .epistemic
                        .entry(quad.subject.clone())
                        .or_default()
                        .insert(object.to_owned());
                } else if quad.predicate == temporal {
                    evidence
                        .successors
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
        let governing = governing_contracts(&contracts)?;
        // An entry counts only when the world's governing contract owns it.
        let contract_entries: BTreeSet<(&str, &str)> = ownership
            .into_iter()
            .filter(|(world, contract, _)| governing.get(world) == Some(contract))
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
        ensure_acyclic(&evidence.successors)?;
        evidence.entailed = entail_supertypes(&evidence.typings, &supers);
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

    /// The worlds reachable from `world` along admissible typed accessibility records: the
    /// epistemic records `world` asserts first, then the temporal successors, reached by
    /// walking succession transitively.  Each world is yielded once.  The walk is lazy, so a
    /// judgment that stops at its first witness does not expand the rest of the history.
    /// Succession is acyclic (see `ensure_acyclic`), so `world` is never yielded for itself;
    /// an epistemic record naming `world` is excluded.
    pub(super) fn accessible_from<'a>(&'a self, world: &str) -> impl Iterator<Item = &'a String> {
        let direct: Vec<&'a String> = self
            .epistemic
            .get(world)
            .into_iter()
            .flatten()
            .filter(|reached| reached.as_str() != world)
            .collect();
        let mut yielded: BTreeSet<&'a String> = direct.iter().copied().collect();
        let mut expanded: BTreeSet<&'a String> = BTreeSet::new();
        let mut pending: Vec<&'a String> =
            self.successors.get(world).into_iter().flatten().collect();
        let temporal = std::iter::from_fn(move || {
            while let Some(next) = pending.pop() {
                if expanded.insert(next) {
                    pending.extend(self.successors.get(next).into_iter().flatten());
                }
                if yielded.insert(next) {
                    return Some(next);
                }
            }
            None
        });
        direct.into_iter().chain(temporal)
    }

    /// Whether `individual` demonstrably existed in `world` without being a `type_iri`:
    /// it is present, not a member through any asserted typing or supertype entailed by the
    /// world's subclass triples, and its absence of the type is explicitly asserted or
    /// closed-world covered.
    pub(super) fn lost_in(&self, world: &str, individual: &str, type_iri: &str) -> bool {
        let present = (world.to_owned(), individual.to_owned());
        let key = (world.to_owned(), individual.to_owned(), type_iri.to_owned());
        self.existing.contains(&present)
            && !self.entailed.contains(&key)
            && (self.non_members.contains(&key) || self.closed.contains(&present))
    }
}

/// `typings` closed upward under the `logic:subClassOf` triples of the same world, as the
/// foundation chase closes `subClassOfT`: a typing of a subclass entails the typing of
/// every transitive superclass.  `supers` maps `(world, class)` to its asserted direct
/// superclasses.
fn entail_supertypes(
    typings: &BTreeSet<Typing>,
    supers: &BTreeMap<(&str, &str), BTreeSet<&str>>,
) -> BTreeSet<Typing> {
    let mut entailed = typings.clone();
    let mut pending: Vec<Typing> = typings.iter().cloned().collect();
    while let Some((world, individual, type_iri)) = pending.pop() {
        for &parent in supers
            .get(&(world.as_str(), type_iri.as_str()))
            .into_iter()
            .flatten()
        {
            let typing = (world.clone(), individual.clone(), parent.to_owned());
            if entailed.insert(typing.clone()) {
                pending.push(typing);
            }
        }
    }
    entailed
}

/// Refuse cyclic temporal succession, including a state that succeeds itself.  Succession is a
/// strict order, so a cycle would let an earlier state judge a later one in both directions.
///
/// # Errors
///
/// Names a state on a cycle of `logic:temporallySucceeds` records.
fn ensure_acyclic(successors: &BTreeMap<String, BTreeSet<String>>) -> gmeow_errors::Result<()> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mark {
        /// On the current depth-first path.
        Open,
        /// Fully explored: no cycle is reachable from it.
        Done,
    }
    /// A frame either enters a state or leaves it once all of its successors are explored.
    #[derive(Clone, Copy)]
    enum Step {
        Enter,
        Leave,
    }
    let mut marks: BTreeMap<&str, Mark> = BTreeMap::new();
    for root in successors.keys() {
        let mut stack: Vec<(&str, Step)> = vec![(root.as_str(), Step::Enter)];
        while let Some((state, step)) = stack.pop() {
            if let Step::Leave = step {
                marks.insert(state, Mark::Done);
                continue;
            }
            match marks.get(state) {
                Some(Mark::Done) => continue,
                Some(Mark::Open) => {
                    return Err(foundation_err(format!(
                        "temporal succession records form a cycle through <{state}>; \
                         logic:temporallySucceeds is a strict order"
                    )));
                }
                None => {}
            }
            marks.insert(state, Mark::Open);
            stack.push((state, Step::Leave));
            for next in successors.get(state).into_iter().flatten() {
                stack.push((next.as_str(), Step::Enter));
            }
        }
    }
    Ok(())
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

/// The one governing `logic:ReasoningContract` of each world.  A world is governed by
/// exactly one contract, so the count is the rule: two contracts are refused whether or not
/// their closure entries agree, and no closure is selected by merging them.
///
/// # Errors
///
/// Names the world and every contract asserted in it when a world has more than one.
fn governing_contracts<'a>(
    contracts: &BTreeMap<&'a str, BTreeSet<&'a str>>,
) -> gmeow_errors::Result<BTreeMap<&'a str, &'a str>> {
    let mut governing = BTreeMap::new();
    for (&world, asserted) in contracts {
        let mut names = asserted.iter();
        let (Some(&only), None) = (names.next(), names.next()) else {
            let listed = asserted
                .iter()
                .map(|contract| format!("<{contract}>"))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(foundation_err(format!(
                "world <{world}> has {} logic:ReasoningContract records ({listed}); a world is \
                 governed by exactly one contract",
                asserted.len()
            )));
        };
        governing.insert(world, only);
    }
    Ok(governing)
}
