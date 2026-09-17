// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native producer effects, separated by the statement patterns they can affect.
//!
//! A read waits only for potentially intersecting writes. Constants retain native
//! term identity; unknown positions conservatively overlap every value. Edges are
//! between producer firings, so an atomic multi-head firing has one schedule slot.
//! Predicate completion is a separate projection over the LAST possible writer.
//! This analysis never samples a corpus or changes execution terms or premises.

use std::collections::{BTreeMap, BTreeSet};

use purrdf::TermValue;

use super::dependency::{DependencyCycle, ReadDependency, SignedDependencies};
use crate::native_semantics::SemanticVocabulary;
use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm};

pub(super) mod value_flow;

/// Unknown slots are explicit over-approximations, never evidence of no effects.
#[derive(Debug, Clone)]
pub(crate) struct StatementPattern {
    subject: Option<TermValue>,
    predicate: Option<String>,
    object: Option<TermValue>,
    ranges: Option<[value_flow::Domain; 3]>,
}

impl StatementPattern {
    pub(crate) fn atom(atom: &EvalAtom) -> Self {
        Self {
            subject: constant(&atom.subject),
            predicate: Some(atom.predicate.clone()),
            object: constant(&atom.object),
            ranges: None,
        }
    }

    /// Schema predicates can be data-selected, unlike ordinary relational atoms.
    pub(crate) fn statement(terms: &[EvalTerm; 3]) -> Self {
        Self {
            subject: constant(&terms[0]),
            predicate: match &terms[1] {
                EvalTerm::ConstNamed(iri) | EvalTerm::ConstLit(TermValue::Iri(iri)) => {
                    Some(iri.clone())
                }
                _ => None,
            },
            object: constant(&terms[2]),
            ranges: None,
        }
    }

    pub(crate) fn relation(predicate: Option<&str>, object: Option<&str>) -> Self {
        Self {
            subject: None,
            predicate: predicate.map(str::to_owned),
            object: object.map(TermValue::iri),
            ranges: None,
        }
    }

    /// All statements about one exact source-owned subject participate in its
    /// immutable definition, including currently absent predicates.
    pub(crate) fn subject(subject: TermValue) -> Self {
        Self {
            subject: Some(subject),
            predicate: None,
            object: None,
            ranges: None,
        }
    }

    /// A sound predicate/marker completion envelope for this possible head. A
    /// literal or unknown object conservatively requires the whole predicate.
    pub(crate) fn completion_read(&self) -> crate::reason::refute::native::NativeRead {
        crate::reason::refute::native::NativeRead {
            predicate: self.predicate.clone(),
            marker: self
                .object
                .as_ref()
                .and_then(TermValue::as_iri)
                .map(str::to_owned),
            kind: crate::reason::refute::native::NativeReadKind::Completed,
        }
    }

    /// Test an asserted native statement with the same exact role semantics used
    /// by producer-effect admission. Terms are borrowed, never cloned or rewritten.
    pub(crate) fn matches_fact(
        &self,
        fact: &crate::rule_ir::Fact,
        semantics: SemanticVocabulary,
    ) -> bool {
        self.reads_terms(
            Some(&fact.subject),
            Some(fact.predicate.as_str()),
            Some(&fact.object),
            semantics,
        )
    }

    /// Read interpretation is role-sensitive. A constant marker may accept its
    /// declared alternate spelling; a subject, literal or quoted term may not.
    fn reads_write(&self, write: &Self, semantics: SemanticVocabulary) -> bool {
        if let (Some(read), Some(write)) = (&self.ranges, &write.ranges)
            && read
                .iter()
                .zip(write)
                .any(|(read, write)| !read.overlaps(write))
        {
            return false;
        }
        self.reads_terms(
            write.subject.as_ref(),
            write.predicate.as_deref(),
            write.object.as_ref(),
            semantics,
        )
    }

    fn reads_terms(
        &self,
        subject: Option<&TermValue>,
        predicate: Option<&str>,
        object: Option<&TermValue>,
        semantics: SemanticVocabulary,
    ) -> bool {
        if let (Some(read), Some(write)) = (self.predicate.as_deref(), predicate)
            && semantics.predicate(read) != semantics.predicate(write)
        {
            return false;
        }
        if let (Some(read), Some(write)) = (self.subject.as_ref(), subject)
            && read != write
        {
            return false;
        }
        match (self.object.as_ref(), object) {
            (Some(read), Some(written)) if read != written => {
                match (read, written, self.predicate.as_deref().or(predicate)) {
                    (TermValue::Iri(read), TermValue::Iri(written), Some(predicate)) => {
                        semantics.alternate_marker(predicate, read) == Some(written.as_str())
                    }
                    // Unknown operators retain both marker and ordinary roles.
                    (TermValue::Iri(_), TermValue::Iri(_), None) => true,
                    _ => false,
                }
            }
            _ => true,
        }
    }
}

fn constant(term: &EvalTerm) -> Option<TermValue> {
    match term {
        EvalTerm::Var(_) => None,
        EvalTerm::ConstNamed(iri) => Some(TermValue::iri(iri)),
        EvalTerm::ConstLit(value) => Some(value.clone()),
    }
}

/// One producer's indivisible outputs and the signed reads needed to compute them.
#[derive(Clone, Debug)]
pub(crate) struct ProducerEffect {
    name: String,
    pub(crate) writes: Vec<StatementPattern>,
    pub(crate) reads: Vec<(StatementPattern, ReadDependency)>,
    /// Non-writing admission obligations that must finish before a consumer can
    /// claim these possible head roles complete. They do not propagate terms.
    pub(crate) completion_for: Vec<StatementPattern>,
    rule_key: Option<[u8; 32]>,
    atomic_group: Option<String>,
    /// Logical completion consumers (NAF/reductions/graph builtins) require source
    /// eligibility as well as writer saturation. Preparation itself reads only
    /// actual writer completion and cannot depend on its own eligibility receipt.
    completion_obligations: bool,
}

impl ProducerEffect {
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// Source refinement erases every observable part of an unreachable
    /// producer. Execution selection must use the same proof: retaining its
    /// property or family executor after the effect disappeared can manufacture
    /// an obstruction for a firing that the selected input can never reach.
    pub(crate) fn reachable(&self) -> bool {
        !self.writes.is_empty() || !self.completion_for.is_empty() || !self.reads.is_empty()
    }
    pub(crate) fn new(
        name: String,
        writes: Vec<StatementPattern>,
        reads: Vec<(StatementPattern, ReadDependency)>,
    ) -> Self {
        Self {
            name,
            writes,
            reads,
            completion_for: Vec::new(),
            rule_key: None,
            atomic_group: None,
            completion_obligations: false,
        }
    }

    /// A logical completed-read consumer also depends on selected source-owner
    /// admission, not only on the currently executable positive writer set.
    pub(crate) fn requiring_source_admission(mut self) -> Self {
        self.completion_obligations = true;
        self
    }

    pub(crate) fn completes(mut self, patterns: Vec<StatementPattern>) -> Self {
        self.completion_for = patterns;
        self
    }

    /// Alternative guarded analyses for one indivisible native producer must
    /// execute in one SCC; taking a maximum rank after scheduling is unsound.
    pub(crate) fn in_atomic_group(mut self, group: &str) -> Self {
        self.atomic_group = Some(group.to_owned());
        self
    }

    pub(crate) fn completion_reader(&self) -> Option<&str> {
        self.reads
            .iter()
            .any(|(_, dependency)| *dependency == ReadDependency::Completed)
            .then_some(self.name.as_str())
    }

    pub(crate) fn rule(rule: &EvalRule) -> Self {
        let mut reads: Vec<_> = rule
            .body
            .iter()
            .map(|atom| {
                (
                    StatementPattern::atom(atom),
                    if atom.negated || rule.reduction.is_some() {
                        ReadDependency::Completed
                    } else {
                        ReadDependency::Positive
                    },
                )
            })
            .collect();
        reads.extend(
            rule.builtins
                .iter()
                .flat_map(super::builtin_eval::read_patterns)
                .map(|(predicate, object)| {
                    (
                        StatementPattern::relation(Some(predicate), object),
                        ReadDependency::Completed,
                    )
                }),
        );
        let mut effect = Self::new(
            rule.rule_iri.clone(),
            vec![StatementPattern::atom(&rule.head)],
            reads,
        );
        effect.completion_obligations = true;
        effect.rule_key = Some(super::plan::canonical_rule_hash(std::slice::from_ref(rule)));
        effect
    }
}

/// Find producers whose completed reads can change after an exact input delta.
/// Possible writes propagate through every positive and strict edge. This is a
/// conservative program analysis, independent of any bounded test corpus.
pub(crate) fn invalidated_completed_readers(
    effects: &[WorldProducerEffect],
    changes: &[(String, crate::rule_ir::Fact)],
    semantics: SemanticVocabulary,
) -> BTreeSet<(String, String)> {
    let mut affected = BTreeSet::new();
    loop {
        let before = affected.len();
        for (index, effect) in effects.iter().enumerate() {
            if effect
                .effect
                .reads
                .iter()
                .zip(&effect.read_worlds)
                .any(|((read, _), world)| {
                    changes
                        .iter()
                        .any(|(owner, fact)| owner == world && read.matches_fact(fact, semantics))
                        || affected.iter().any(|writer: &usize| {
                            let writer = &effects[*writer];
                            writer.owner == *world
                                && writer
                                    .effect
                                    .writes
                                    .iter()
                                    .chain(&writer.effect.completion_for)
                                    .any(|write| read.reads_write(write, semantics))
                        })
                })
            {
                affected.insert(index);
            }
        }
        if before == affected.len() {
            break;
        }
    }
    effects
        .iter()
        .filter(|effect| {
            effect
                .effect
                .reads
                .iter()
                .zip(&effect.read_worlds)
                .any(|((read, kind), world)| {
                    *kind == ReadDependency::Completed
                        && (changes.iter().any(|(owner, fact)| {
                            owner == world && read.matches_fact(fact, semantics)
                        }) || affected.iter().any(|writer| {
                            let writer = &effects[*writer];
                            writer.owner == *world
                                && writer
                                    .effect
                                    .writes
                                    .iter()
                                    .chain(&writer.effect.completion_for)
                                    .any(|write| read.reads_write(write, semantics))
                        }))
                })
        })
        .map(|effect| (effect.owner.clone(), effect.effect.name.clone()))
        .collect()
}

/// Producer ranks and relation completion are distinct: several ranks may write
/// disjoint parts of the same predicate. Unknown writers cover every predicate.
pub(crate) struct EffectSchedule {
    pub(crate) total: usize,
    pub(crate) strata: Vec<usize>,
    pub(crate) completed: BTreeMap<String, usize>,
    pub(crate) dynamic_completion: Option<usize>,
    pub(crate) read_predicates: BTreeSet<String>,
    pub(crate) observed_completion: Vec<Option<usize>>,
    rule_keys: Vec<Option<[u8; 32]>>,
}

/// Ordinary rules admitted together by the complete producer graph. Its private
/// construction prevents a second, source-insensitive stratification from replacing
/// the joint proof. Execution remains inside the source-bound JointProgram.
pub(super) struct CertifiedStratum {
    layouts: super::plan::RuleLayouts,
    indices: Vec<usize>,
}

impl CertifiedStratum {
    pub(super) fn into_parts(self) -> (super::plan::RuleLayouts, Vec<usize>) {
        (self.layouts, self.indices)
    }
}

impl EffectSchedule {
    pub(super) fn ordinary_strata(
        &self,
        layouts: &super::plan::RuleLayouts,
    ) -> Vec<CertifiedStratum> {
        let total = self.total;
        let mut selected: Vec<_> = (0..total)
            .map(|_| CertifiedStratum {
                layouts: layouts.clone(),
                indices: Vec::new(),
            })
            .collect();
        for (index, key) in layouts.keys().iter().enumerate() {
            assert_eq!(
                self.rule_keys[index],
                Some(*key),
                "joint schedule must bind the exact ordinary producer IR"
            );
            selected[self.strata[index]].indices.push(index);
        }
        selected
    }
}

/// Index writes by possible operator, then add only overlapping signed edges.
pub(crate) fn schedule(
    effects: &[ProducerEffect],
    semantics: SemanticVocabulary,
    source_predicates: &BTreeSet<String>,
) -> Result<EffectSchedule, DependencyCycle> {
    schedule_observed(effects, semantics, source_predicates, &[])
}

/// A producer's writes belong to one exact world; each read names its own world.
/// Constructors validate arity rather than silently assigning a missing read owner.
#[derive(Clone, Debug)]
pub(crate) struct WorldProducerEffect {
    pub(crate) owner: String,
    pub(crate) effect: ProducerEffect,
    pub(crate) read_worlds: Vec<String>,
}
impl WorldProducerEffect {
    pub(crate) fn local(owner: &str, effect: ProducerEffect) -> Self {
        Self {
            owner: owner.to_owned(),
            read_worlds: vec![owner.to_owned(); effect.reads.len()],
            effect,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct WorldStatementObservation {
    pub(crate) world: String,
    pub(crate) pattern: StatementPattern,
}

pub(crate) struct WorldEffectSchedule {
    pub(crate) strata: Vec<usize>,
    pub(crate) completed: BTreeMap<(String, String), usize>,
    pub(crate) dynamic_completion: BTreeMap<String, usize>,
    pub(crate) read_predicates: BTreeMap<String, BTreeSet<String>>,
    pub(crate) observed_completion: Vec<Option<usize>>,
    rule_keys: Vec<Option<[u8; 32]>>,
    effect_keys: Vec<[u8; 32]>,
}
/// Scheduler-issued exact selected-read admission. The constructor is private
/// to the physical graph; callers can only validate a particular typed effect.
pub(crate) struct CompletedWorldReads {
    effects: BTreeSet<[u8; 32]>,
}
impl CompletedWorldReads {
    pub(crate) fn covers(&self, effect: &WorldProducerEffect) -> bool {
        self.effects.contains(&crate::physical::metadata_identity(
            "gmeow-world-effect-read-v1",
            effect,
        ))
    }
}
impl WorldEffectSchedule {
    pub(in crate::physical) fn completed_reads(
        &self,
        range: std::ops::Range<usize>,
        stratum: usize,
    ) -> CompletedWorldReads {
        CompletedWorldReads {
            effects: range
                .filter(|index| self.strata[*index] == stratum)
                .map(|index| self.effect_keys[index])
                .collect(),
        }
    }

    pub(crate) fn local(
        &self,
        world: &str,
        producers: std::ops::Range<usize>,
        observations: std::ops::Range<usize>,
    ) -> EffectSchedule {
        EffectSchedule {
            total: self.strata.iter().copied().max().unwrap_or(0) + 1,
            strata: self.strata[producers.clone()].to_vec(),
            completed: self
                .completed
                .iter()
                .filter(|((owner, _), _)| owner == world)
                .map(|((_, predicate), rank)| (predicate.clone(), *rank))
                .collect(),
            dynamic_completion: self.dynamic_completion.get(world).copied(),
            read_predicates: self.read_predicates.get(world).cloned().unwrap_or_default(),
            observed_completion: self.observed_completion[observations].to_vec(),
            rule_keys: self.rule_keys[producers].to_vec(),
        }
    }
}

/// Additional native preparation observations carry the same exact writer proof
/// as executable effects, without inventing a whole-predicate completion claim.
pub(crate) fn schedule_observed(
    effects: &[ProducerEffect],
    semantics: SemanticVocabulary,
    source_predicates: &BTreeSet<String>,
    observations: &[StatementPattern],
) -> Result<EffectSchedule, DependencyCycle> {
    let effects: Vec<_> = effects
        .iter()
        .cloned()
        .map(|effect| WorldProducerEffect::local("", effect))
        .collect();
    let observations: Vec<_> = observations
        .iter()
        .cloned()
        .map(|pattern| WorldStatementObservation {
            world: String::new(),
            pattern,
        })
        .collect();
    let plan = schedule_worlds(
        &effects,
        semantics,
        &BTreeMap::from([(String::new(), source_predicates.clone())]),
        &observations,
    );
    plan.map(|plan| plan.local("", 0..effects.len(), 0..observations.len()))
}

/// The same signed native dependency analysis covers local and cross-world
/// producers. No completed local read is published ahead of a foreign writer.
pub(crate) fn schedule_worlds(
    effects: &[WorldProducerEffect],
    semantics: SemanticVocabulary,
    source_predicates: &BTreeMap<String, BTreeSet<String>>,
    observations: &[WorldStatementObservation],
) -> Result<WorldEffectSchedule, DependencyCycle> {
    let mut graph = SignedDependencies::default();
    let input = |world: &str| format!("input:{}:{world}", world.len());
    for world in source_predicates.keys() {
        graph.define(&input(world));
    }
    let names: Vec<_> = effects
        .iter()
        .enumerate()
        .map(|(index, producer)| {
            format!(
                "producer:{index:020}:{}:{}:{}",
                producer.owner.len(),
                producer.owner,
                producer.effect.name
            )
        })
        .collect();
    let mut writers = BTreeMap::<(String, String), Vec<(usize, &StatementPattern, bool)>>::new();
    let mut unknown = BTreeMap::<String, Vec<(usize, &StatementPattern, bool)>>::new();
    for (index, producer) in effects.iter().enumerate() {
        assert_eq!(
            producer.read_worlds.len(),
            producer.effect.reads.len(),
            "every native effect read has an exact world"
        );
        graph.define(&names[index]);
        graph.define(&input(&producer.owner));
        for (write, completion_only) in producer
            .effect
            .writes
            .iter()
            .map(|write| (write, false))
            .chain(
                producer
                    .effect
                    .completion_for
                    .iter()
                    .map(|write| (write, true)),
            )
        {
            if let Some(predicate) = &write.predicate {
                writers
                    .entry((
                        producer.owner.clone(),
                        semantics.predicate(predicate).to_owned(),
                    ))
                    .or_default()
                    .push((index, write, completion_only));
            } else {
                unknown.entry(producer.owner.clone()).or_default().push((
                    index,
                    write,
                    completion_only,
                ));
            }
        }
    }
    for (reader, producer) in effects.iter().enumerate() {
        let mut dependencies = BTreeMap::<usize, ReadDependency>::new();
        for ((read, kind), world) in producer.effect.reads.iter().zip(&producer.read_worlds) {
            graph.define(&input(world));
            if *kind == ReadDependency::Completed {
                graph.read(&names[reader], &input(world), *kind);
            }
            let mut visit =
                |&(writer, write, completion_only): &(usize, &StatementPattern, bool)| {
                    if (!completion_only
                        || (*kind == ReadDependency::Completed
                            && producer.effect.completion_obligations))
                        && read.reads_write(write, semantics)
                    {
                        dependencies
                            .entry(writer)
                            .and_modify(|prior| *prior = (*prior).max(*kind))
                            .or_insert(*kind);
                    }
                };
            if let Some(predicate) = &read.predicate {
                if let Some(bucket) =
                    writers.get(&(world.clone(), semantics.predicate(predicate).to_owned()))
                {
                    bucket.iter().for_each(&mut visit);
                }
            } else {
                writers
                    .iter()
                    .filter(|((owner, _), _)| owner == world)
                    .flat_map(|(_, bucket)| bucket)
                    .for_each(&mut visit);
            }
            if let Some(bucket) = unknown.get(world) {
                bucket.iter().for_each(&mut visit);
            }
        }
        for (writer, kind) in dependencies {
            graph.read(&names[reader], &names[writer], kind);
        }
    }
    let mut groups = BTreeMap::<(&str, &str), usize>::new();
    for (index, producer) in effects.iter().enumerate() {
        if let Some(group) = producer.effect.atomic_group.as_deref() {
            if let Some(first) = groups.get(&(producer.owner.as_str(), group)) {
                graph.read(&names[index], &names[*first], ReadDependency::Positive);
                graph.read(&names[*first], &names[index], ReadDependency::Positive);
            } else {
                groups.insert((&producer.owner, group), index);
            }
        }
    }
    let plan = graph.plan()?;
    let strata: Vec<_> = names
        .iter()
        .map(|name| plan.stratum(name).expect("registered native producer"))
        .collect();
    // Completion-only effects are admission receipts, not statement writers.
    // They still contribute dependency edges above for consumers that explicitly
    // require source admission, but they cannot delay the completion frontier of
    // an RDF predicate they never extend.
    let dynamic_completion: BTreeMap<_, _> = unknown
        .iter()
        .filter_map(|(world, bucket)| {
            bucket
                .iter()
                .filter(|(_, _, completion_only)| !*completion_only)
                .map(|(writer, _, _)| strata[*writer])
                .max()
                .map(|rank| (world.clone(), rank))
        })
        .collect();
    let mut completed = BTreeMap::<(String, String), usize>::new();
    let mut record = |world: &str, predicate: &str, rank: usize| {
        for spelling in std::iter::once(predicate).chain(semantics.alternate_predicate(predicate)) {
            completed
                .entry((world.to_owned(), spelling.to_owned()))
                .and_modify(|prior| *prior = (*prior).max(rank))
                .or_insert(rank);
        }
    };
    for ((world, predicate), bucket) in &writers {
        let Some(rank) = bucket
            .iter()
            .filter(|(_, _, completion_only)| !*completion_only)
            .map(|(writer, _, _)| strata[*writer])
            .chain(dynamic_completion.get(world).copied())
            .max()
        else {
            continue;
        };
        record(world, predicate, rank);
    }
    let mut read_predicates = BTreeMap::<String, BTreeSet<String>>::new();
    for producer in effects {
        for ((read, _), world) in producer.effect.reads.iter().zip(&producer.read_worlds) {
            if let Some(predicate) = read.predicate.as_deref() {
                read_predicates.entry(world.clone()).or_default().extend(
                    std::iter::once(predicate)
                        .chain(semantics.alternate_predicate(predicate))
                        .map(str::to_owned),
                );
            }
        }
    }
    for (world, rank) in &dynamic_completion {
        for predicate in source_predicates
            .get(world)
            .into_iter()
            .flatten()
            .chain(read_predicates.get(world).into_iter().flatten())
        {
            record(world, predicate, *rank);
        }
    }
    let observed_completion = observations
        .iter()
        .map(|read| {
            effects
                .iter()
                .enumerate()
                .filter(|(_, producer)| {
                    producer.owner == read.world
                        && producer
                            .effect
                            .writes
                            .iter()
                            .any(|write| read.pattern.reads_write(write, semantics))
                })
                .map(|(writer, _)| strata[writer])
                .max()
        })
        .collect();
    Ok(WorldEffectSchedule {
        strata,
        completed,
        dynamic_completion,
        read_predicates,
        observed_completion,
        rule_keys: effects
            .iter()
            .map(|producer| producer.effect.rule_key)
            .collect(),
        effect_keys: effects
            .iter()
            .map(|effect| crate::physical::metadata_identity("gmeow-world-effect-read-v1", effect))
            .collect(),
    })
}

/// Exact source-owned definition immutability uses the same role-sensitive
/// possible-write matcher as source rule admission and dependency construction.
pub(crate) fn scoped_definition_writer<'a>(
    flow: &value_flow::ValueFlow,
    effects: &'a [WorldProducerEffect],
    definition: &WorldStatementObservation,
) -> Option<&'a str> {
    effects
        .iter()
        .find(|producer| {
            producer.owner == definition.world
                && producer
                    .effect
                    .writes
                    .iter()
                    .any(|write| flow.overlaps_write(&definition.pattern, write))
        })
        .map(|producer| producer.effect.name.as_str())
}

#[cfg(test)]
mod tests;
