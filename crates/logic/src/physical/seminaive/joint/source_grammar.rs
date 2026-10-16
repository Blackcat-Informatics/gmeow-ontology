// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Inductive grammar preservation by constructor-owned, closed RDF lists.
//!
//! This proves syntax eligibility, never completion of a relation or world. All
//! possible grammar writers are checked, including external output envelopes.
//! Existing source terms cannot alias the constructors; each cell's first/rest
//! values are functions of its witness address, and every rest path ends at nil.
//! Runtime admission still checks every committed selector and list premise, so
//! a budget cut through a firing never authorizes a partial list.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use purrdf::TermValue;

use super::PreparedPropertyRule;
use super::families::{Arm, Producer};
use crate::native_semantics::SemanticVocabulary;
use crate::physical::chase::{ExistentialRule, PreparedChaseRule, WitnessPolicy};
use crate::physical::dependency::ReadDependency;
use crate::physical::effects::value_flow::ValueFlow;
use crate::physical::effects::{ProducerEffect, StatementPattern, WorldProducerEffect};
use crate::reason::dl::{DlConstructFamily, NativeSourceGrammarBasis};
use crate::rule_ir::{EvalTerm, Fact};

const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
const ONE_OF: &str = "http://www.w3.org/2002/07/owl#oneOf";

pub(super) struct GrammarInput<'a> {
    pub world: &'a str,
    pub facts: &'a [Fact],
    pub possible: &'a [(String, Fact)],
    pub external: &'a [WorldProducerEffect],
    pub ordinary: usize,
    pub producers: &'a [Arc<PreparedChaseRule>],
    pub properties: &'a [PreparedPropertyRule],
    pub families: &'a [Arm],
    pub effects: &'a mut [ProducerEffect],
    pub flow: &'a ValueFlow,
}

#[path = "source_grammar.tests.rs"]
#[cfg(test)]
mod tests;

/// A live checked capability, not deserializable observation data. Its source,
/// writer inventory and world participate in the native input commitment.
#[derive(Debug, Clone)]
pub(crate) struct ConstructorClosedLists {
    world: String,
    commitment: [u8; 32],
    producers: Vec<String>,
    consumers: BTreeSet<usize>,
}

fn constructor_iri(iri: &str) -> bool {
    iri.starts_with(crate::facts::SKOLEM_PREFIX)
        || iri.starts_with(crate::provenance::NARY_REIFIER_PREFIX)
}

fn contains_constructor(value: &TermValue) -> bool {
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        match value {
            TermValue::Iri(iri) if constructor_iri(iri) => return true,
            TermValue::Triple { s, p, o } => {
                pending.extend([s.as_ref(), p.as_ref(), o.as_ref()]);
            }
            TermValue::Literal { datatype, .. } if constructor_iri(datatype) => return true,
            _ => {}
        }
    }
    false
}

fn grammar(predicate: &str, semantics: SemanticVocabulary) -> bool {
    [FIRST, REST, ONE_OF]
        .into_iter()
        .any(|selected| semantics.predicate(predicate) == semantics.predicate(selected))
}

/// All grammar heads in one firing must describe constructor-owned finite lists.
/// An explicit reduced frontier must not erase an operand that selects a cell's
/// first value. Ancestor blocking cannot establish fresh ownership.
fn closed_head(rule: &ExistentialRule, semantics: SemanticVocabulary) -> bool {
    if rule.witness_policy != WitnessPolicy::FrontierSkolem {
        return false;
    }
    let fresh: BTreeSet<_> = rule.existentials().into_iter().collect();
    let mut cells = BTreeMap::<&str, (Option<&EvalTerm>, Option<&EvalTerm>)>::new();
    let mut roots = Vec::new();
    for atom in &rule.head {
        if atom.negated {
            return false;
        }
        let predicate = semantics.predicate(&atom.predicate);
        if predicate == semantics.predicate(ONE_OF) {
            if !matches!(&atom.subject, EvalTerm::ConstNamed(_))
                && !matches!(&atom.subject, EvalTerm::Var(name) if fresh.contains(name))
            {
                return false;
            }
            roots.push(&atom.object);
        } else if predicate == semantics.predicate(FIRST) || predicate == semantics.predicate(REST)
        {
            let EvalTerm::Var(cell) = &atom.subject else {
                return false;
            };
            if !fresh.contains(cell) {
                return false;
            }
            let pair = cells.entry(cell).or_default();
            let slot = if predicate == semantics.predicate(FIRST) {
                &mut pair.0
            } else {
                &mut pair.1
            };
            if slot.is_some_and(|previous| previous != &atom.object) {
                return false;
            }
            *slot = Some(&atom.object);
            if let EvalTerm::Var(value) = &atom.object
                && !fresh.contains(value)
                && rule
                    .witness_frontier
                    .as_ref()
                    .is_some_and(|frontier| !frontier.contains(value))
            {
                return false;
            }
        }
    }
    if cells.is_empty() && roots.is_empty() {
        return false;
    }
    let mut starts: Vec<_> = cells
        .keys()
        .map(|cell| EvalTerm::Var((*cell).to_owned()))
        .collect();
    starts.extend(roots.into_iter().cloned());
    for start in &starts {
        let mut cursor = start;
        let mut seen = BTreeSet::new();
        loop {
            match cursor {
                EvalTerm::ConstNamed(iri) if iri == NIL => break,
                EvalTerm::Var(cell) if seen.insert(cell) => {
                    let Some((Some(_), Some(rest))) = cells.get(cell.as_str()) else {
                        return false;
                    };
                    cursor = rest;
                }
                _ => return false,
            }
        }
    }
    true
}

impl ConstructorClosedLists {
    pub(super) fn certify(input: GrammarInput<'_>) -> Option<Self> {
        let semantics = input.flow.semantics();
        let protected: Vec<_> = [FIRST, REST, ONE_OF]
            .into_iter()
            .map(|predicate| StatementPattern::relation(Some(predicate), None))
            .collect();
        let overlaps = |effect: &ProducerEffect| {
            effect.writes.iter().any(|write| {
                protected
                    .iter()
                    .any(|read| input.flow.overlaps_write(read, write))
            })
        };
        let mut producers = Vec::new();
        for (index, _) in input
            .effects
            .iter()
            .enumerate()
            .filter(|(_, effect)| overlaps(effect))
        {
            let producer = index
                .checked_sub(input.ordinary)
                .and_then(|index| input.producers.get(index))?;
            if !producer.uses_only_frontier_skolems() || !closed_head(&producer.rule, semantics) {
                return None;
            }
            producers.push(producer.rule.rule_iri.clone());
        }
        if producers.is_empty() {
            return None;
        }
        producers.sort();
        producers.dedup();
        if input
            .external
            .iter()
            .any(|effect| effect.owner == input.world && overlaps(&effect.effect))
        {
            return None;
        }
        if input
            .possible
            .iter()
            .any(|(world, fact)| world == input.world && grammar(&fact.predicate, semantics))
        {
            return None;
        }
        // Inspect the complete selected source, not an abstract sample: known
        // constructor-looking IRIs explicitly overlap native output addresses.
        if input
            .facts
            .iter()
            .chain(
                input
                    .possible
                    .iter()
                    .filter(|(world, _)| world == input.world)
                    .map(|(_, fact)| fact),
            )
            .any(|fact| {
                constructor_iri(&fact.predicate)
                    || contains_constructor(&fact.subject)
                    || contains_constructor(&fact.object)
            })
        {
            return None;
        }
        let mut consumers = BTreeSet::new();
        let property_start = input.ordinary + input.producers.len();
        for (index, property) in input.properties.iter().enumerate() {
            let mut selected = BTreeSet::new();
            for atom in &property.source.body {
                let EvalTerm::ConstNamed(predicate) = &atom.0[1] else {
                    continue;
                };
                let object = match &atom.0[2] {
                    EvalTerm::ConstNamed(iri) => Some(TermValue::iri(iri.clone())),
                    EvalTerm::ConstLit(value) => Some(value.clone()),
                    EvalTerm::Var(_) => None,
                };
                selected.extend(
                    crate::reason::dl::source_selector_families(predicate, object.as_ref())
                        .into_iter()
                        .filter(|family| {
                            crate::reason::dl::source_execution_requires_admission(
                                *family, predicate,
                            )
                        }),
                );
            }
            if selected == BTreeSet::from([DlConstructFamily::OneOf]) {
                consumers.insert(property_start + index);
            }
        }
        for (index, arm) in input.families.iter().enumerate() {
            if arm.family == Producer::Admission(DlConstructFamily::OneOf) {
                consumers.insert(property_start + input.properties.len() + index);
            }
        }
        let commitment = crate::physical::metadata_identity(
            "gmeow-constructor-closed-list-grammar-v1",
            &(
                input.world,
                input.facts,
                input.possible,
                input.external,
                input
                    .producers
                    .iter()
                    .map(|producer| &producer.rule)
                    .collect::<Vec<_>>(),
                &consumers,
                semantics,
            ),
        );
        let proof = Self {
            world: input.world.to_owned(),
            commitment,
            producers,
            consumers,
        };
        proof.schedule(input.effects, semantics);
        Some(proof)
    }

    fn schedule(&self, effects: &mut [ProducerEffect], semantics: SemanticVocabulary) {
        for &index in &self.consumers {
            for (read, dependency) in &mut effects[index].reads {
                if read
                    .predicate()
                    .is_some_and(|predicate| grammar(predicate, semantics))
                    && *dependency == ReadDependency::Completed
                {
                    *dependency = ReadDependency::Positive;
                }
            }
        }
    }

    pub(crate) fn basis(
        &self,
        world: &str,
        family: DlConstructFamily,
    ) -> Option<NativeSourceGrammarBasis> {
        (self.world == world && family == DlConstructFamily::OneOf).then(|| {
            NativeSourceGrammarBasis::ConstructorClosedLists {
                commitment: self.commitment,
                world: self.world.clone(),
                producers: self.producers.clone(),
            }
        })
    }
}
