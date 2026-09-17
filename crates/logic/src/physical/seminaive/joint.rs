// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Joint ordinary, existential and schema fixed points on native indexed stores.
//!
//! Stratification and termination see every producer. Each stratum runs all
//! families against the same frozen round, then commits one ordered, budgeted
//! delta. No family is re-seeded from a materialized intermediate closure.

use std::sync::Arc;

use super::property::PreparedPropertyRule;
use super::{
    Budgeted, Delta, FactStore, FixpointState, FixpointStatus, NativeOutcome, ProvenanceMode,
    RoundCandidateBuffer, RoundExecution, RoundSnapshot, StepGovernor, StrataProgress,
    UnsupportedKind, commit_round, evaluate_round_candidates, record_candidate, seminaive_err,
};
use crate::physical::chase::{
    ChaseAdmission, ExistentialRule, PreparedChaseRule, WitnessPolicy, chase_round,
};
use crate::physical::dependency::ReadDependency;
use crate::physical::effects::{EffectSchedule, ProducerEffect, StatementPattern};
use crate::physical::plan::{Executable, RuleLayouts, compile_certified_stratum};
use crate::physical::store::{RelationStore, SkolemRegistry, WitnessContract, WitnessDerivation};
use crate::provenance::ProofHeight;
use crate::reason::refute::native::NativeClosureStatus;
use crate::rule_ir::{DerivedRow, EvalRule, Fact, echo_asserted, sort_rows, world_edb_facts};
use crate::seam::BudgetStatus;
use std::collections::BTreeSet;

mod contextual;
mod families;
mod input;
mod retained;
pub(crate) use families::NativeWorldSnapshot;
pub(crate) use retained::RetainedJoint;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JointOperation {
    Relational,
    Forward,
    ClassDiagnostic,
}
pub(crate) use input::{DefinitionContract, JointInput, JointTemplate, schema_identity};

/// Immutable native execution plans and joint dependency evidence.
struct JointLayout {
    rules: RuleLayouts,
    producers: Vec<Arc<PreparedChaseRule>>,
    properties: Vec<PreparedPropertyRule>,
    family_arms: Vec<families::Arm>,
    family_reads: Vec<crate::reason::refute::native::NativeRead>,
}

pub(crate) struct JointProgram {
    layout: Arc<JointLayout>,
    local_effects: Arc<Vec<ProducerEffect>>,
    semantics: crate::native_semantics::SemanticVocabulary,
    witness_contract: WitnessContract,
    strata: Vec<JointStratum>,
    heads: BTreeSet<String>,
    pub(crate) admission: ChaseAdmission,
    dynamic_completion: Option<usize>,
    source_contract: Option<[u8; 32]>,
    operation: JointOperation,
    read_predicates: BTreeSet<String>,
    read_completion: Vec<(crate::reason::refute::native::NativeRead, Option<usize>)>,
}

struct JointStratum {
    ordinary: Option<Arc<Executable>>,
    producers: Vec<Arc<PreparedChaseRule>>,
    properties: Vec<PreparedPropertyRule>,
    heads: BTreeSet<String>,
    families: Vec<families::Producer>,
    blocked_reads: Vec<crate::reason::refute::native::NativeRead>,
}

impl std::fmt::Debug for JointProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JointProgram")
            .field("strata", &self.strata.len())
            .field("heads", &self.heads)
            .field("admission", &self.admission)
            .finish()
    }
}

impl JointProgram {
    /// Prepare the complete dependency graph before planning either rule family.
    pub(crate) fn prepare(
        rules: &[EvalRule],
        producers: &[ExistentialRule],
    ) -> gmeow_errors::Result<NativeOutcome<Self>> {
        Self::prepare_with_properties(rules, producers, &[], &BTreeSet::new())
    }

    /// Add data-selected positive schema joins to the same dependency and execution
    /// graph. Unknown predicate reads/writes are conservative effects, never absent edges.
    pub(crate) fn prepare_with_properties(
        rules: &[EvalRule],
        producers: &[ExistentialRule],
        properties: &[PreparedPropertyRule],
        source_predicates: &BTreeSet<String>,
    ) -> gmeow_errors::Result<NativeOutcome<Self>> {
        Self::prepare_with_semantics(
            rules,
            producers,
            properties,
            source_predicates,
            crate::native_semantics::SemanticVocabulary::Exact,
        )
    }

    /// The selected operator contract governs matching, dependencies, completion,
    /// and the conservative termination abstraction together.
    pub(crate) fn prepare_with_semantics(
        rules: &[EvalRule],
        producers: &[ExistentialRule],
        properties: &[PreparedPropertyRule],
        source_predicates: &BTreeSet<String>,
        semantics: crate::native_semantics::SemanticVocabulary,
    ) -> gmeow_errors::Result<NativeOutcome<Self>> {
        // The direct relational entry has no source abstract input to narrow. Its
        // prepared effects remain conservative, including source admission barriers.
        let families = families::admission_arms(properties);
        let mut effects = producer_effects(rules, producers, properties)?;
        effects.extend(families.iter().map(|arm| arm.effect.clone()));
        let admission_reads = crate::reason::dl::source_admission_reads();
        let observations: Vec<_> = admission_reads
            .iter()
            .map(|read| {
                StatementPattern::relation(read.predicate.as_deref(), read.marker.as_deref())
            })
            .collect();
        let Ok(schedule) = crate::physical::effects::schedule_observed(
            &effects,
            semantics,
            source_predicates,
            &observations,
        ) else {
            return Ok(NativeOutcome::Unsupported(UnsupportedKind::NonStratifiable));
        };
        let admission = producer_admission(rules, producers, properties, &families, semantics);
        let layouts = RuleLayouts::new(rules)?;
        let producers = prepare_producers(producers)?;
        Ok(NativeOutcome::Decided(Self::from_schedule(
            &layouts,
            &producers,
            properties,
            &families,
            &admission_reads,
            JointOperation::Relational,
            semantics,
            schedule,
            Arc::new(effects),
            None,
            admission,
        )))
    }

    fn from_schedule(
        rules: &RuleLayouts,
        producers: &[Arc<PreparedChaseRule>],
        properties: &[PreparedPropertyRule],
        family_arms: &[families::Arm],
        family_reads: &[crate::reason::refute::native::NativeRead],
        operation: JointOperation,
        semantics: crate::native_semantics::SemanticVocabulary,
        schedule: EffectSchedule,
        local_effects: Arc<Vec<ProducerEffect>>,
        source_contract: Option<[u8; 32]>,
        admission: ChaseAdmission,
    ) -> Self {
        let layout = Arc::new(JointLayout {
            rules: rules.clone(),
            producers: producers.to_vec(),
            properties: properties.to_vec(),
            family_arms: family_arms.to_vec(),
            family_reads: family_reads.to_vec(),
        });
        Self::with_layout(
            layout,
            local_effects,
            operation,
            semantics,
            schedule,
            source_contract,
            admission,
        )
    }

    fn with_layout(
        layout: Arc<JointLayout>,
        local_effects: Arc<Vec<ProducerEffect>>,
        operation: JointOperation,
        semantics: crate::native_semantics::SemanticVocabulary,
        schedule: EffectSchedule,
        source_contract: Option<[u8; 32]>,
        admission: ChaseAdmission,
    ) -> Self {
        let JointLayout {
            rules,
            producers,
            properties,
            family_arms,
            family_reads,
        } = layout.as_ref();
        let heads = schedule.completed.keys().cloned().collect();
        let (_, remaining) = schedule.strata.split_at(rules.keys().len());
        let (producer_ranks, remaining) = remaining.split_at(producers.len());
        let (property_ranks, family_ranks) = remaining.split_at(properties.len());
        let (_, remaining_effects) = local_effects.split_at(rules.keys().len());
        let (_, remaining_effects) = remaining_effects.split_at(producers.len());
        let (property_effects, family_effects) = remaining_effects.split_at(properties.len());
        assert_eq!(property_effects.len(), properties.len());
        assert_eq!(family_effects.len(), family_arms.len());
        assert_eq!(
            family_arms.len(),
            family_ranks.len(),
            "all native producer arms participate in scheduling"
        );
        let total = schedule.total;
        let mut strata = Vec::with_capacity(total);
        for (index, ordinary) in schedule.ordinary_strata(rules).into_iter().enumerate() {
            let ordinary = compile_certified_stratum(ordinary);
            let producers = producers
                .iter()
                .zip(producer_ranks)
                .filter(|(_, rank)| **rank == index)
                .map(|(rule, _)| Arc::clone(rule))
                .collect();
            let families: BTreeSet<_> = family_arms
                .iter()
                .zip(family_ranks)
                .zip(family_effects)
                .filter(|((_, rank), effect)| **rank == index && effect.reachable())
                .map(|((arm, _), _)| arm.family)
                .collect();
            let blocked_reads = local_effects
                .iter()
                .zip(&schedule.strata)
                .filter(|(_, rank)| **rank == index)
                .flat_map(|(effect, _)| effect.writes.iter().chain(&effect.completion_for))
                .map(StatementPattern::completion_read)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            strata.push(JointStratum {
                blocked_reads,
                families: families.into_iter().collect(),
                ordinary,
                producers,
                properties: properties
                    .iter()
                    .zip(property_ranks)
                    .zip(property_effects)
                    .filter(|((_, rank), effect)| **rank == index && effect.reachable())
                    .map(|((property, _), _)| property.clone())
                    .collect(),
                heads: schedule
                    .completed
                    .iter()
                    .filter(|(_, rank)| **rank == index)
                    .map(|(predicate, _)| predicate.clone())
                    .collect(),
            });
        }
        let read_completion = family_reads
            .iter()
            .cloned()
            .zip(schedule.observed_completion)
            .collect();
        Self {
            layout,
            local_effects,
            witness_contract: WitnessContract::native(semantics),
            semantics,
            strata,
            heads,
            admission,
            dynamic_completion: schedule.dynamic_completion,
            source_contract,
            operation,
            read_predicates: schedule.read_predicates,
            read_completion,
        }
    }

    /// Bind the admitted source-program profile and original source metadata.
    pub(crate) fn with_witness_source(
        mut self,
        source: &crate::native_semantics::ProgramAdmission,
    ) -> Self {
        self.witness_contract = self.witness_contract.with_source(source);
        self
    }

    pub(crate) fn has_property_rules(&self) -> bool {
        self.strata
            .iter()
            .any(|stratum| !stratum.properties.is_empty())
    }

    /// Execute with one governor across sorted worlds; retain witness recipes and
    /// intersect the per-world completion frontiers. Unrun worlds retain their EDB.
    pub(crate) fn materialize(
        &self,
        input: &crate::store::WorldStore,
        max_steps: Option<u64>,
    ) -> gmeow_errors::Result<NativeOutcome<JointMaterialization>> {
        if self.source_contract.is_some() {
            return Err(seminaive_err(
                "a source-bound program requires its authenticated JointInput",
            ));
        }
        let mut worlds = input.worlds();
        worlds.sort();
        self.materialize_worlds(
            worlds.iter().map(|world| {
                Ok((
                    world.as_str(),
                    std::borrow::Cow::Owned(world_edb_facts(input, world)?),
                ))
            }),
            max_steps,
        )
    }

    /// Borrow native world partitions directly; no intermediate WorldStore or
    /// re-interned typed-fact carrier is needed by the reasoning driver.
    pub(crate) fn materialize_facts(
        &self,
        input: &std::collections::BTreeMap<String, Vec<Fact>>,
        max_steps: Option<u64>,
    ) -> gmeow_errors::Result<NativeOutcome<JointMaterialization>> {
        if self.source_contract.is_some() {
            return Err(seminaive_err(
                "a source-bound program requires its authenticated JointInput",
            ));
        }
        self.materialize_native_input(input, max_steps)
    }

    fn materialize_native_input(
        &self,
        input: &std::collections::BTreeMap<String, Vec<Fact>>,
        max_steps: Option<u64>,
    ) -> gmeow_errors::Result<NativeOutcome<JointMaterialization>> {
        self.materialize_worlds(
            input.iter().map(|(world, facts)| {
                Ok((world.as_str(), std::borrow::Cow::Borrowed(facts.as_slice())))
            }),
            max_steps,
        )
    }

    fn materialize_worlds<'a>(
        &self,
        worlds: impl Iterator<Item = gmeow_errors::Result<(&'a str, std::borrow::Cow<'a, [Fact]>)>>,
        max_steps: Option<u64>,
    ) -> gmeow_errors::Result<NativeOutcome<JointMaterialization>> {
        let mut governor = StepGovernor::new(max_steps);
        let mut registry = SkolemRegistry::new();
        self.materialize_worlds_governed(worlds, &mut governor, &mut registry, None, None)
    }

    fn materialize_worlds_governed<'a>(
        &self,
        worlds: impl Iterator<Item = gmeow_errors::Result<(&'a str, std::borrow::Cow<'a, [Fact]>)>>,
        governor: &mut StepGovernor,
        registry: &mut SkolemRegistry,
        mut native: Option<input::NativeInputBinding<'_>>,
        mut retention: Option<&mut RetainedJoint>,
    ) -> gmeow_errors::Result<NativeOutcome<JointMaterialization>> {
        if (self.operation != JointOperation::Relational) != native.is_some() {
            return Err(seminaive_err(
                "native family execution requires its exact original source admission",
            ));
        }
        if !self.admission.admits_native() && governor.remaining().is_none() {
            return Ok(NativeOutcome::Unsupported(
                UnsupportedKind::NonTerminatingExistential,
            ));
        }
        let initial_steps = governor.consumed;
        let modal = native.as_mut().and_then(|binding| binding.modal.take());
        let contextual = native
            .as_mut()
            .and_then(|binding| binding.contextual.take());
        let mut contextual_state = contextual.as_ref().map(|program| program.start());
        let source_refused = native
            .as_ref()
            .is_some_and(|binding| binding.source_refused);
        if source_refused {
            return Err(seminaive_err(
                "refused source cannot enter the native writer graph",
            ));
        }
        let mut runtimes = std::collections::BTreeMap::new();
        let mut effects = Vec::new();
        let mut observations = Vec::new();
        let mut source_predicates = std::collections::BTreeMap::new();
        let mut ranges = std::collections::BTreeMap::new();
        let mut asserted = Vec::new();
        let observation_flow = native.as_ref().map(|binding| binding.flow);
        for entry in worlds {
            let (world, edb) = entry?;
            if runtimes.contains_key(world) {
                return Err(seminaive_err("native world supplied twice"));
            }
            asserted.extend(echo_asserted(world, &edb)?);
            let family = if let Some(binding) = native.as_mut() {
                let class = binding
                    .classes
                    .remove(world)
                    .ok_or_else(|| seminaive_err("native world has no original class admission"))?;
                families::State::new(
                    binding,
                    world,
                    &edb,
                    governor.remaining(),
                    false,
                    class,
                    self.operation,
                )?
            } else {
                families::State::relational(
                    world,
                    &edb,
                    self.semantics,
                    governor.remaining(),
                    false,
                )?
            };
            let producer_start = effects.len();
            let observation_start = observations.len();
            let local_effects = match native.as_ref() {
                Some(binding) => binding.world_effects.get(world).ok_or_else(|| {
                    seminaive_err("native source world lacks its admitted producer effects")
                })?,
                None => &self.local_effects,
            };
            effects.extend(
                local_effects.iter().cloned().map(|effect| {
                    crate::physical::effects::WorldProducerEffect::local(world, effect)
                }),
            );
            observations.extend(self.layout.family_reads.iter().map(|read| {
                let pattern =
                    StatementPattern::relation(read.predicate.as_deref(), read.marker.as_deref());
                let pattern = if let Some(flow) = observation_flow {
                    flow.ranged_pattern(pattern)
                } else {
                    pattern
                };
                crate::physical::effects::WorldStatementObservation {
                    world: world.to_owned(),
                    pattern,
                }
            }));
            source_predicates.insert(
                world.to_owned(),
                edb.iter().map(|fact| fact.predicate.clone()).collect(),
            );
            ranges.insert(
                world.to_owned(),
                (
                    producer_start..effects.len(),
                    observation_start..observations.len(),
                ),
            );
            runtimes.insert(
                world.to_owned(),
                WorldRuntime::new(&edb, self.semantics, family)?,
            );
        }
        let modal_start = effects.len();
        if let Some(modal) = &modal {
            effects.extend(modal.effects().iter().cloned());
            for effect in modal.effects() {
                if !runtimes.contains_key(&effect.owner)
                    || effect
                        .read_worlds
                        .iter()
                        .any(|world| !runtimes.contains_key(world))
                {
                    return Err(seminaive_err(
                        "modal effect refers to an unadmitted native source world",
                    ));
                }
            }
        }
        let modal_range = modal_start..effects.len();
        let contextual_start = effects.len();
        if let Some(contextual) = &contextual {
            effects.extend(contextual.effects().iter().cloned());
            for effect in contextual.effects() {
                if !runtimes.contains_key(&effect.owner)
                    || effect
                        .read_worlds
                        .iter()
                        .any(|world| !runtimes.contains_key(world))
                {
                    return Err(seminaive_err(
                        "contextual effect refers to an unadmitted native source world",
                    ));
                }
            }
        }
        let contextual_range = contextual_start..effects.len();
        // Immutable grammar admission sees every cross-world writer, including
        // contextual publications capable of activating ordinary consumers.
        if let Some(modal) = &modal {
            for definition in modal.definition_patterns() {
                if let Some(writer) = crate::physical::effects::scoped_definition_writer(
                    native.as_ref().expect("native modal binding").flow,
                    &effects,
                    &definition,
                ) {
                    return Err(gmeow_errors::Diag::of_kind(crate::error::NativeCoverage {
                        profile: "native-immutable-modal-source-v1".to_owned(),
                        source: writer.to_owned(),
                        world: definition.world,
                        detail:
                            "reachable native producer can change selected modal definition grammar"
                                .to_owned(),
                    }));
                }
            }
        }
        if let Some(contextual) = &contextual {
            for definition in contextual.definition_patterns(runtimes.keys()) {
                if let Some(writer) = crate::physical::effects::scoped_definition_writer(
                    native.as_ref().expect("native contextual binding").flow,
                    &effects,
                    &definition,
                ) {
                    return Err(gmeow_errors::Diag::of_kind(crate::error::NativeCoverage {
                        profile: "native-immutable-contextual-source-v1".to_owned(),
                        source: writer.to_owned(), world: definition.world.clone(),
                        detail: "reachable native producer can change selected contextual definition grammar".to_owned(),
                    }));
                }
            }
        }
        let Ok(schedule) = crate::physical::effects::schedule_worlds(
            &effects,
            self.semantics,
            &source_predicates,
            &observations,
        ) else {
            return Ok(NativeOutcome::Unsupported(UnsupportedKind::NonStratifiable));
        };
        let total = schedule.strata.iter().copied().max().unwrap_or(0) + 1;
        let captured_facts = retention.as_ref().map(|_| {
            runtimes
                .iter()
                .map(|(world, runtime)| (world.clone(), runtime.store.facts().to_vec()))
                .collect()
        });
        let mut reuse = match (retention.as_deref(), native.as_ref()) {
            (Some(retained), Some(binding)) => Some(retained.admit(
                binding.template,
                binding.graphs,
                &runtimes,
                &effects,
                &schedule,
                self,
            )),
            (Some(_), None) => {
                return Err(seminaive_err(
                    "retained execution requires original native source admission",
                ));
            }
            (None, _) => None,
        };
        if let Some(reuse) = &reuse {
            reuse.install_registry(registry);
        }
        let mut plans = std::collections::BTreeMap::new();
        for (world, (producers, observations)) in ranges {
            let local_effects = match native.as_ref() {
                Some(binding) => {
                    Arc::clone(binding.world_effects.get(&world).ok_or_else(|| {
                        seminaive_err("native local schedule lost its source effects")
                    })?)
                }
                None => Arc::clone(&self.local_effects),
            };
            let mut plan = Self::with_layout(
                Arc::clone(&self.layout),
                local_effects,
                self.operation,
                self.semantics,
                schedule.local(&world, producers, observations),
                self.source_contract,
                self.admission.clone(),
            );
            plan.witness_contract = self.witness_contract;
            let runtime = runtimes.get_mut(&world).expect("registered world");
            runtime.progress = StrataProgress {
                completed: 0,
                total,
                saturated_preds: plan.seed_frontier(runtime.store.facts()),
            };
            plans.insert(world, plan);
        }
        let mut terminal = NativeClosureStatus::Completed;
        let mut inference_exhausted = false;
        'strata: for index in 0..total {
            for (world, runtime) in &mut runtimes {
                runtime.start_stratum(&plans[world], index);
            }
            // A cut stops fresh work, but already admitted proofs in the reached
            // stratum can still publish through successive frozen rounds. This
            // never advances a completed-read frontier or exposes a later stratum.
            let mut draining_retained = false;
            loop {
                let mut rounds = std::collections::BTreeMap::new();
                let mut truncated = false;
                let mut inference_cut = false;
                let mut blocked = BTreeSet::new();
                for (world, runtime) in &mut runtimes {
                    if draining_retained {
                        rounds.insert(world.clone(), RoundCandidateBuffer::new());
                        continue;
                    }
                    let round = plans[world].strata[index].gather(
                        runtime,
                        governor,
                        registry,
                        world,
                        self.witness_contract,
                    )?;
                    truncated |= round.truncated;
                    inference_cut |= round.inference_cut;
                    blocked.extend(round.blocked);
                    if !runtime.gaps.is_empty() {
                        return Err(seminaive_err(
                            crate::reason::builtin_gap::builtin_gap_refusal_detail(&runtime.gaps),
                        ));
                    }
                    rounds.insert(world.clone(), round.candidates);
                }
                let retained_heads = match &mut reuse {
                    Some(reuse) => reuse.gather(index, &runtimes, &mut rounds)?,
                    None => std::collections::BTreeMap::new(),
                };
                // Every producer reads the same cross-world frozen round. World
                // stores stay unchanged until all owner-routed candidates exist.
                if !draining_retained && let Some(modal) = &modal {
                    let selected: Vec<_> = modal_range
                        .clone()
                        .filter(|effect| schedule.strata[*effect] == index)
                        .map(|effect| effect - modal_start)
                        .collect();
                    let completion = schedule.completed_reads(modal_range.clone(), index);
                    let mut snapshots = runtimes
                        .iter_mut()
                        .map(|(world, runtime)| Ok((world.as_str(), runtime.native_snapshot()?)))
                        .collect::<gmeow_errors::Result<std::collections::BTreeMap<_, _>>>()?;
                    let candidates = modal.evaluate(&selected, &mut snapshots, &completion)?;
                    drop(snapshots);
                    for runtime in runtimes.values_mut() {
                        runtime.family.index_proofs();
                    }
                    for candidate in candidates {
                        let owner = runtimes.get(&candidate.owner).ok_or_else(|| {
                            seminaive_err("modal candidate has no admitted owner store")
                        })?;
                        if owner.store.contains_key(&candidate.head.key()) {
                            continue;
                        }
                        let world = candidate.owner.clone();
                        let candidate = record_modal_candidate(candidate, &runtimes)?;
                        rounds
                            .get_mut(&world)
                            .expect("admitted modal route")
                            .insert(candidate.head.key(), candidate, ProvenanceMode::Record)?;
                    }
                }
                let contextual_batch = if !draining_retained && let Some(contextual) = &contextual {
                    let selected: Vec<_> = contextual_range
                        .clone()
                        .filter(|effect| schedule.strata[*effect] == index)
                        .map(|effect| effect - contextual_start)
                        .collect();
                    let completion = schedule.completed_reads(contextual_range.clone(), index);
                    let mut snapshots = runtimes
                        .iter_mut()
                        .map(|(world, runtime)| Ok((world.as_str(), runtime.native_snapshot()?)))
                        .collect::<gmeow_errors::Result<std::collections::BTreeMap<_, _>>>()?;
                    let batch = contextual.evaluate(
                        contextual_state
                            .as_mut()
                            .expect("selected contextual state"),
                        &selected,
                        &mut snapshots,
                        &completion,
                        governor,
                    )?;
                    drop(snapshots);
                    for runtime in runtimes.values_mut() {
                        runtime.family.index_proofs();
                    }
                    truncated |= batch.interrupted;
                    inference_cut |= batch.interrupted;
                    Some(batch)
                } else {
                    None
                };
                let starts = runtimes
                    .iter()
                    .map(|(world, runtime)| {
                        (world.clone(), (runtime.rel.row_count(), runtime.rows.len()))
                    })
                    .collect::<std::collections::BTreeMap<_, _>>();
                let contextual_rows = match contextual_batch {
                    Some(batch) => contextual::publish(batch, &mut runtimes, registry, governor)?,
                    None => 0,
                };
                let any =
                    contextual_rows != 0 || rounds.values().any(|round| !round.entries.is_empty());
                if !any {
                    if let Some(reuse) = &mut reuse {
                        reuse.install_ready_introductions(index, &runtimes, registry)?;
                    }
                    for (world, runtime) in &mut runtimes {
                        registry.commit_heads(world, &runtime.rel);
                        runtime.family.observe_committed(
                            &runtime.store,
                            &runtime.rows,
                            registry,
                        )?;
                    }
                    if truncated || draining_retained {
                        terminal = NativeClosureStatus::Exhausted;
                        inference_exhausted |= inference_cut;
                        break 'strata;
                    }
                    if !blocked.is_empty() {
                        terminal = NativeClosureStatus::Blocked {
                            reads: blocked.into_iter().collect(),
                        };
                        break 'strata;
                    }
                    for (world, runtime) in &mut runtimes {
                        runtime.complete_stratum(&plans[world], index);
                    }
                    break;
                }
                let mut commit_cut = false;
                let empty_retained = BTreeSet::new();
                let mut reused_rows = 0;
                for (world, round) in rounds {
                    let runtime = runtimes.get_mut(&world).expect("routed native owner");
                    let (lo, committed_start) = starts[&world];
                    let carried = retained_heads.get(&world).unwrap_or(&empty_retained);
                    let outcome =
                        commit_round(round.entries, &mut runtime.fixpoint(), governor, carried)?;
                    reused_rows += runtime.rows[committed_start..]
                        .iter()
                        .filter(|row| {
                            carried.contains(&(
                                row.subject.clone(),
                                row.predicate.clone(),
                                row.object.clone(),
                            ))
                        })
                        .count();
                    registry.commit_heads(&world, &runtime.rel);
                    runtime.changed = Some(
                        runtime.rows[committed_start..]
                            .iter()
                            .map(|row| row.predicate.clone())
                            .collect(),
                    );
                    runtime.delta = Delta {
                        lo,
                        hi: runtime.rel.row_count(),
                    };
                    commit_cut |= outcome == FixpointStatus::Exhausted;
                }
                if let Some(reuse) = &mut reuse {
                    reuse.record_committed(reused_rows);
                    reuse.install_ready_introductions(index, &runtimes, registry)?;
                }
                for runtime in runtimes.values_mut() {
                    runtime
                        .family
                        .observe_committed(&runtime.store, &runtime.rows, registry)?;
                }
                if commit_cut || truncated {
                    inference_exhausted |= commit_cut || inference_cut;
                    terminal = NativeClosureStatus::Exhausted;
                    if reuse.is_none() {
                        break 'strata;
                    }
                    draining_retained = true;
                }
            }
        }
        // Final observations use the same scheduled terminal in every world.
        // A later world's analysis cannot retroactively change the completion
        // evidence supplied to an earlier or later sibling. Aggregate actual
        // analysis exhaustion only after every required observation finishes.
        for runtime in runtimes.values_mut() {
            runtime.family.finish(
                &runtime.store,
                &runtime.rel,
                &runtime.rows,
                registry,
                &mut runtime.values,
                &mut runtime.lists,
                &runtime.progress.saturated_preds,
                &terminal,
            )?;
        }
        if runtimes
            .values()
            .any(|runtime| runtime.family.analysis_exhausted())
        {
            terminal = NativeClosureStatus::Exhausted;
        }
        // Every selected request retains a complete stop judgment if an upstream
        // producer stops before its scheduled read boundary. The actual cause and
        // allowance survive; unfinished evidence is never read as semantic absence.
        use crate::contextual::native::{
            NativeContextualAnalysisStop, NativeContextualUpstreamStop,
        };
        let stop = match &terminal {
            NativeClosureStatus::Exhausted
                if inference_exhausted || governor.remaining() == Some(0) =>
            {
                Some(NativeContextualUpstreamStop::InferenceExhausted)
            }
            NativeClosureStatus::Exhausted => {
                let analyses = runtimes.values().filter_map(|runtime| {
                    let ledger = &runtime.family.ledger;
                    let class_resource_obstructions = runtime.family.classes.as_ref()
                        .filter(|class| class.completion == crate::reason::refute::native::NativeFamilyCompletion::Exhausted)
                        .into_iter()
                        .flat_map(|class| class.obstructions.iter())
                        .filter(|obstruction| obstruction.kind == crate::reason::refute::native::NativeObstructionKind::ResourceLimit)
                        .cloned()
                        .collect::<Vec<_>>();
                    (runtime.family.analysis_exhausted()
                        && (ledger.work.exhausted || ledger.work.allowance == Some(0)
                            || !class_resource_obstructions.is_empty()))
                    .then(|| NativeContextualAnalysisStop {
                        world: ledger.world.clone(),
                        graph: ledger.graph.clone(),
                        usage: ledger.work,
                        class_resource_obstructions,
                    })
                }).collect();
                Some(NativeContextualUpstreamStop::AnalysisExhausted { analyses })
            }
            NativeClosureStatus::Blocked { reads } => Some(NativeContextualUpstreamStop::Blocked {
                reads: reads.clone(),
            }),
            NativeClosureStatus::Completed if governor.remaining() == Some(0) => {
                Some(NativeContextualUpstreamStop::InferenceExhausted)
            }
            NativeClosureStatus::Completed => None,
        };
        if let Some(contextual) = &contextual
            && let Some(stop) = stop
        {
            let inference_stop = matches!(&stop, NativeContextualUpstreamStop::InferenceExhausted);
            let mut snapshots = runtimes
                .iter_mut()
                .map(|(world, runtime)| Ok((world.as_str(), runtime.native_snapshot()?)))
                .collect::<gmeow_errors::Result<std::collections::BTreeMap<_, _>>>()?;
            let batch = contextual.finalize_remaining(
                contextual_state
                    .as_mut()
                    .expect("selected contextual state"),
                &mut snapshots,
                governor,
                stop,
            )?;
            drop(snapshots);
            for runtime in runtimes.values_mut() {
                runtime.family.index_proofs();
            }
            if !batch.receipts.is_empty() && inference_stop {
                terminal = NativeClosureStatus::Exhausted;
                inference_exhausted = true;
            }
            contextual::publish(batch, &mut runtimes, registry, governor)?;
        }
        let mut native_families = Vec::new();
        let mut source_coverage = std::collections::BTreeMap::new();
        let mut classes = Vec::new();
        let mut completed = total;
        let mut saturated: Option<BTreeSet<String>> = None;
        for (world, runtime) in runtimes {
            if let Some(class) = runtime.family.classes {
                classes.push(class);
            }
            source_coverage.insert(world.clone(), runtime.family.coverage);
            native_families.push(runtime.family.ledger);
            completed = completed.min(runtime.progress.completed);
            saturated = Some(match saturated {
                None => runtime.progress.saturated_preds,
                Some(prior) => prior
                    .intersection(&runtime.progress.saturated_preds)
                    .cloned()
                    .collect(),
            });
            asserted.extend(runtime.rows.into_iter().map(|mut row| {
                row.graph = world.clone();
                row
            }));
        }
        if terminal != NativeClosureStatus::Completed {
            completed = completed.min(total.saturating_sub(1));
        }
        sort_rows(&mut asserted);
        // A cut can leave retained higher-stratum introductions unpublished.
        // Only evidence for actual committed rows may escape this transaction.
        if let Some(retained) = retention.as_deref_mut() {
            registry.retain_committed_facts(&asserted);
            let binding = native.as_ref().expect("retention admitted a native source");
            retained.capture(
                binding.template,
                binding.graphs.clone(),
                captured_facts.expect("selected retention captured its source"),
                asserted
                    .iter()
                    .filter(|row| row.rule_iri != crate::provenance::ASSERT_RULE_IRI)
                    .cloned()
                    .collect(),
                registry.clone(),
                reuse.as_ref().map_or(0, retained::Reuse::count),
            );
        }
        let witness_derivations = registry
            .witnesses()
            .map(|iri| registry.explain(iri).expect("registered witness recipe"))
            .collect();
        let status = if terminal == NativeClosureStatus::Exhausted {
            BudgetStatus::Exhausted
        } else {
            BudgetStatus::Ok
        };
        Ok(NativeOutcome::Decided(JointMaterialization {
            result: Budgeted {
                rows: asserted,
                status,
                progress: StrataProgress {
                    completed,
                    total,
                    saturated_preds: saturated.unwrap_or_default(),
                },
                consumed_steps: governor.consumed.saturating_sub(initial_steps),
            },
            witness_derivations,
            native_families,
            source_coverage,
            classes,
            class_admission: native.map(|binding| binding.class_admission),
            inference_exhausted,
            terminal,
        }))
    }

    fn seed_frontier(&self, edb: &[Fact]) -> BTreeSet<String> {
        if self.dynamic_completion.is_some() {
            return BTreeSet::new();
        }
        self.read_predicates
            .iter()
            .map(String::as_str)
            .chain(edb.iter().flat_map(|f| {
                std::iter::once(f.predicate.as_str())
                    .chain(self.semantics.alternate_predicate(&f.predicate))
            }))
            .filter(|predicate| !self.heads.contains(*predicate))
            .map(str::to_owned)
            .collect()
    }
}

/// Prepare witness layouts once, then share them across every input-value schedule.
fn prepare_producers(
    producers: &[ExistentialRule],
) -> gmeow_errors::Result<Vec<Arc<PreparedChaseRule>>> {
    producers
        .iter()
        .map(|rule| PreparedChaseRule::new(rule.clone()).map(Arc::new))
        .collect()
}

/// A source-independent termination proof shared across input-value schedules.
fn producer_admission(
    rules: &[EvalRule],
    producers: &[ExistentialRule],
    properties: &[PreparedPropertyRule],
    families: &[families::Arm],
    semantics: crate::native_semantics::SemanticVocabulary,
) -> ChaseAdmission {
    // Dropping filters/NAF is a conservative producer over-approximation for
    // termination. Arithmetic generation requires its own range certificate.
    let mut termination_rules = producers.to_vec();
    // A stratified reduction runs once on completed finite predecessors. Its
    // output is a finite seed for later strata, never a recursive value inventor.
    // Execution still requires the complete strict dependency certificate.
    termination_rules.extend(rules.iter().filter(|r| r.reduction.is_none()).map(|r| {
        ExistentialRule {
            numeric: r.numeric.clone(),
            rule_iri: r.rule_iri.clone(),
            body: r.body.iter().filter(|a| !a.negated).cloned().collect(),
            head: vec![r.head.clone()],
            distinct: Vec::new(),
            witness_frontier: None,
            witness_policy: WitnessPolicy::FrontierSkolem,
        }
    }));
    let arithmetic: Vec<_> = rules
        .iter()
        .filter(|r| !r.builtins.is_empty() && head_generates_value(r))
        .map(|r| format!("{} has no certified arithmetic producer range", r.rule_iri))
        .collect();
    for rule in &mut termination_rules {
        for atom in rule.body.iter_mut().chain(&mut rule.head) {
            semantics.abstract_atom(atom);
        }
    }
    if arithmetic.is_empty() {
        let statements: Vec<_> = termination_rules
            .iter()
            .map(crate::physical::chase::StatementRule::from_binary)
            .chain(
                properties
                    .iter()
                    .map(crate::physical::chase::StatementRule::from_property),
            )
            .chain(families.iter().map(|arm| arm.statement.clone()))
            .collect();
        ChaseAdmission::certify_statements(&statements, semantics)
    } else {
        ChaseAdmission::Uncertified {
            violations: arithmetic,
        }
    }
}

/// Declare every native producer once, retaining implicit structural reads.
fn producer_effects(
    rules: &[EvalRule],
    producers: &[ExistentialRule],
    properties: &[PreparedPropertyRule],
) -> gmeow_errors::Result<Vec<ProducerEffect>> {
    let mut effects: Vec<_> = rules.iter().map(ProducerEffect::rule).collect();
    for producer in producers {
        if producer.head.is_empty()
            || producer
                .body
                .iter()
                .chain(&producer.head)
                .any(|atom| atom.negated)
        {
            return Err(seminaive_err(format!(
                "joint existential producer {} requires a nonempty positive head and positive body",
                producer.rule_iri
            )));
        }
        // All heads publish atomically and therefore share ONE producer rank.
        effects.push(ProducerEffect::new(
            producer.rule_iri.clone(),
            producer.head.iter().map(StatementPattern::atom).collect(),
            producer
                .body
                .iter()
                .map(|atom| (StatementPattern::atom(atom), ReadDependency::Positive))
                .collect(),
        ));
    }
    for property in properties {
        let mut reads: Vec<_> = property
            .source
            .body
            .iter()
            .map(|atom| {
                (
                    StatementPattern::statement(&atom.0),
                    ReadDependency::Positive,
                )
            })
            .collect();
        // List walks have additional data-selected reads beyond the explicit
        // join body. Unknown predicates retain every potentially matching writer.
        reads.extend(property.reads().skip(property.source.body.len()).map(
            |(predicate, dependency)| (StatementPattern::relation(predicate, None), dependency),
        ));
        reads.extend(property.admission_reads().into_iter().map(|read| {
            (
                StatementPattern::relation(read.predicate.as_deref(), read.marker.as_deref()),
                ReadDependency::Completed,
            )
        }));
        effects.push(ProducerEffect::new(
            property.source.rule_iri.clone(),
            property
                .analysis_heads
                .iter()
                .map(|head| StatementPattern::statement(&head.0))
                .collect(),
            reads,
        ));
    }
    Ok(effects)
}

/// A builtin cannot expand the term domain when every variable it publishes
/// in the head is already bound by a positive body atom. Intermediate arithmetic
/// bindings used only for filters do not create producer positions.
fn head_generates_value(rule: &EvalRule) -> bool {
    use crate::rule_ir::EvalTerm;
    let bound: BTreeSet<_> = rule
        .body
        .iter()
        .filter(|atom| !atom.negated)
        .flat_map(|atom| [&atom.subject, &atom.object])
        .filter_map(|term| match term {
            EvalTerm::Var(name) => Some(name),
            _ => None,
        })
        .collect();
    [&rule.head.subject, &rule.head.object]
        .into_iter()
        .any(|term| matches!(term, EvalTerm::Var(name) if !bound.contains(name)))
}

/// Joint closure plus the registry evidence generated by this exact run.
pub(crate) struct JointMaterialization {
    pub(crate) result: Budgeted<Vec<DerivedRow>>,
    pub(crate) witness_derivations: Vec<WitnessDerivation>,
    pub(crate) native_families: Vec<crate::reason::refute::native::NativeFamilyLedger>,
    pub(crate) source_coverage:
        std::collections::BTreeMap<String, crate::reason::dl::SourceCoverageWorld>,
    pub(crate) terminal: NativeClosureStatus,
    pub(crate) inference_exhausted: bool,
    pub(crate) classes: Vec<crate::reason::refute::ClassExecutionOutcome>,
    pub(crate) class_admission: Option<crate::reason::refute::ClassAdmissionObservation>,
}

/// Persistent native state for one exact execution context. The global driver
/// owns every world together; local families borrow only their selected store.
pub(crate) struct WorldRuntime {
    store: FactStore,
    rel: RelationStore,
    depth: Vec<ProofHeight>,
    rows: Vec<DerivedRow>,
    gaps: Vec<super::BuiltinGap>,
    values: super::property::SchemaValues,
    lists: super::property::ListCache,
    family: families::State,
    progress: StrataProgress,
    delta: Delta,
    changed: Option<BTreeSet<String>>,
    completed_reads: BTreeSet<crate::reason::refute::native::NativeRead>,
}
impl WorldRuntime {
    fn new(
        edb: &[Fact],
        semantics: crate::native_semantics::SemanticVocabulary,
        mut family: families::State,
    ) -> gmeow_errors::Result<Self> {
        let mut store = FactStore::new();
        let mut rel = RelationStore::with_semantics(semantics);
        let mut depth = Vec::new();
        for fact in edb {
            if store.insert(fact.clone()).is_some() {
                rel.insert(&fact.predicate, &fact.subject, &fact.object);
                depth.push(ProofHeight::ASSERTED);
            }
        }
        family.admit(&store)?;
        Ok(Self {
            store,
            rel,
            depth,
            rows: Vec::new(),
            gaps: Vec::new(),
            values: Default::default(),
            lists: Default::default(),
            family,
            progress: StrataProgress {
                completed: 0,
                total: 0,
                saturated_preds: BTreeSet::new(),
            },
            delta: Delta::all(0),
            changed: None,
            completed_reads: BTreeSet::new(),
        })
    }
    fn fixpoint(&mut self) -> FixpointState<'_> {
        FixpointState {
            store: &mut self.store,
            rel: &mut self.rel,
            depth: &mut self.depth,
            derivations: &mut self.rows,
            builtin_gap: &mut self.gaps,
        }
    }
    fn start_stratum(&mut self, plan: &JointProgram, index: usize) {
        self.delta = Delta::all(self.rel.row_count());
        self.changed = None;
        self.completed_reads = plan
            .read_completion
            .iter()
            .filter(|(_, rank)| rank.is_none_or(|rank| rank < index))
            .map(|(read, _)| read.clone())
            .collect();
        self.completed_reads
            .extend(families::completed(&self.progress.saturated_preds, false));
    }
    fn complete_stratum(&mut self, plan: &JointProgram, index: usize) {
        self.progress.completed += 1;
        self.progress
            .saturated_preds
            .extend(plan.strata[index].heads.iter().cloned());
        if plan.dynamic_completion == Some(index) {
            self.progress.saturated_preds.extend(
                self.rel
                    .predicates()
                    .filter(|predicate| !plan.heads.contains(*predicate))
                    .flat_map(|predicate| {
                        std::iter::once(predicate)
                            .chain(plan.semantics.alternate_predicate(predicate))
                    })
                    .map(str::to_owned),
            );
        }
    }
    pub(crate) fn native_snapshot(&mut self) -> gmeow_errors::Result<NativeWorldSnapshot<'_>> {
        self.family
            .snapshot(&self.store, &self.rel, &self.rows, &self.completed_reads)
    }
}

/// A modal firing uses exact foreign proof rows for its provenance height. It
/// does not put any foreign statement into the head owner's EDB or antecedents.
fn record_modal_candidate(
    candidate: crate::modal::native::NativeModalCandidate,
    worlds: &std::collections::BTreeMap<String, WorldRuntime>,
) -> gmeow_errors::Result<crate::rule_ir::RuleRoundCandidate> {
    let crate::modal::native::NativeModalCandidate {
        owner,
        head,
        evidence,
    } = candidate;
    evidence.validate_structure(&owner, &crate::physical::WitnessStatement::from(&head))?;
    let mut max_height = ProofHeight::ASSERTED;
    let mut total_height = 0u64;
    let mut sources = Vec::with_capacity(evidence.supports.len());
    for support in &evidence.supports {
        let world = worlds
            .get(&support.world)
            .ok_or_else(|| seminaive_err("modal proof source world is absent"))?;
        let proof = world
            .family
            .recorded_proof(&support.proof)
            .ok_or_else(|| seminaive_err("modal proof support was not recorded"))?;
        let fact = Fact {
            subject: proof.statement.subject.clone(),
            predicate: proof.statement.predicate.clone(),
            object: proof.statement.object.clone(),
        };
        let row = world.store.row_index(&fact.key()).ok_or_else(|| {
            seminaive_err("modal support is not an actual committed native statement")
        })?;
        let height = *world
            .depth
            .get(row)
            .ok_or_else(|| seminaive_err("modal support lacks native proof height"))?;
        max_height = max_height.max(height);
        total_height = total_height.saturating_add(u64::from(height.get()));
        sources.push(fact.reifier()?);
    }
    if sources
        != evidence
            .evaluation
            .positive_premises()
            .iter()
            .map(crate::modal::ModalPremise::triple_id)
            .collect::<Vec<_>>()
    {
        return Err(seminaive_err(
            "modal candidate changed exact premise order or statement identity",
        ));
    }
    let mut sorted_sources = sources.clone();
    sorted_sources.sort();
    Ok(crate::rule_ir::RuleRoundCandidate {
        head,
        prov: Some(crate::rule_ir::Provenance {
            sources,
            sorted_sources,
            deriv: evidence.evaluation.derivation_id(),
            rule_iri: crate::modal::MODAL_RULE_IRI.to_owned(),
            proof_height: crate::provenance::MinProofHeightSemiring.derive([max_height])?,
            sum_src_depth: total_height,
            source_facts: Vec::new(),
            cross_world: Some(crate::modal::native::NativeCrossWorldEvidence::Modal(
                Box::new(evidence),
            )),
        }),
    })
}

struct WorldRound {
    candidates: RoundCandidateBuffer,
    truncated: bool,
    inference_cut: bool,
    blocked: Vec<crate::reason::refute::native::NativeRead>,
}
impl JointStratum {
    fn gather(
        &self,
        state: &mut WorldRuntime,
        governor: &StepGovernor,
        registry: &mut SkolemRegistry,
        world: &str,
        contract: WitnessContract,
    ) -> gmeow_errors::Result<WorldRound> {
        let snapshot = RoundSnapshot {
            store: &state.store,
            rel: &state.rel,
            depth: &state.depth,
            delta: state.delta,
            mode: ProvenanceMode::Record,
        };
        let mut round = RoundCandidateBuffer::new();
        state.family.prepare_sources(
            snapshot,
            &state.rows,
            registry,
            &mut state.values,
            &mut state.lists,
            &state.progress.saturated_preds,
            &state.completed_reads,
        )?;
        if let Some(exe) = &self.ordinary {
            for index in 0..exe.stratum_count() {
                round.merge_from(
                    evaluate_round_candidates(
                        exe,
                        index,
                        snapshot,
                        RoundExecution::Parallel,
                        None,
                    )?,
                    ProvenanceMode::Record,
                )?;
            }
        }
        let mut property_truncated = false;
        let mut property_blocked = false;
        for property in &self.properties {
            let visit = property.visit(
                &state.rel,
                state.delta,
                &mut state.lists,
                &mut state.values,
                super::property::NativeWitnesses {
                    world,
                    contract,
                    registry,
                    limit: governor.solution_cap(),
                },
                &state.family.coverage,
                &mut |rule, head, premises| {
                    if !state.store.contains_key(&head.key()) {
                        round.insert(
                            head.key(),
                            record_candidate(rule, head, premises, snapshot)?,
                            ProvenanceMode::Record,
                        )?;
                    }
                    Ok(round.entries.len() <= governor.solution_cap())
                },
            )?;
            property_blocked |= visit.admission_blocked;
            if !visit.complete {
                property_truncated = true;
                break;
            }
        }
        let chase_truncated = chase_round(
            self.producers.iter().map(Arc::as_ref),
            &state.rel,
            governor.solution_cap(),
            registry,
            (world, contract),
            state.changed.as_ref(),
            |head, premises| {
                if !state.store.contains_key(&head.key()) {
                    round.insert(
                        head.key(),
                        record_candidate(premises.rule_iri, head, premises.source_facts, snapshot)?,
                        ProvenanceMode::Record,
                    )?;
                }
                Ok(())
            },
        )?;
        state.family.round(
            &self.families,
            snapshot,
            &state.rows,
            registry,
            &mut state.values,
            &mut state.lists,
            &state.progress.saturated_preds,
            &state.completed_reads,
            &mut round,
        )?;
        state.gaps.append(&mut round.builtin_gap);
        let inference_cut = property_truncated || chase_truncated;
        let truncated = inference_cut || state.family.analysis_exhausted();
        let blocked = if property_blocked || state.family.positive_blocked(&self.families) {
            if self.blocked_reads.is_empty() {
                return Err(seminaive_err(
                    "obstructed native producer has no declared completion dependency",
                ));
            }
            self.blocked_reads.clone()
        } else {
            Vec::new()
        };
        Ok(WorldRound {
            candidates: round,
            truncated,
            inference_cut,
            blocked,
        })
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod witness_tests;
