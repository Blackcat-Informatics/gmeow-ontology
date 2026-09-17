// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native inputs and evidence for the program-driven reasoning closure.
//! The selected native interpretation preserves raw operators and terms. RDF 1.2 native
//! statements remain available to the selected program in their original world.

use std::collections::BTreeMap;

#[cfg(test)]
mod test_support;
use purrdf::TermValue;
use std::sync::Arc;
#[cfg(test)]
use test_support::RdfDataset;
#[cfg(test)]
pub(super) use test_support::input_facts;

use super::{ChaseCertificate, InferredAxiom, reason_err, subject_iri};
use crate::physical::{JointMaterialization, NativeOutcome, PreparedPropertyRule};
use crate::rule_ir::{Fact, FactKey};

/// The physical closure together with the admission of its complete producer graph.
pub(super) struct ProgramClosure {
    pub(super) inferred: Vec<InferredAxiom>,
    pub(super) frontier: crate::query_ir::CompletionFrontier,
    pub(super) status: crate::seam::BudgetStatus,
    pub(super) native_status: super::refute::native::NativeClosureStatus,
    pub(super) consumed_steps: u64,
    pub(super) inference_exhausted: bool,
    pub(super) certificates: Vec<ChaseCertificate>,
    pub(super) witnesses: Vec<crate::physical::WitnessDerivation>,
    pub(super) selected_domains: crate::physical::SelectedDomains,
    pub(super) input_contract: [u8; 32],
    pub(super) graphs: WorldGraphs,
    pub(super) native_families: Vec<super::refute::native::NativeFamilyLedger>,
    pub(super) source_coverage: super::dl::SourceCoverageObservation,
    pub(super) class_admission: super::refute::ClassAdmissionObservation,
    pub(super) classes: Vec<super::refute::ClassExecutionOutcome>,
}

pub(super) type InputFacts = BTreeMap<String, Vec<Fact>>;
pub(super) type WorldGraphs = BTreeMap<String, Option<TermValue>>;

/// One native source ingestion retained for explicit world/domain selection and
/// consumed exactly once by a selected reasoning operation.
#[derive(Clone)]
pub struct PreparedReasoningInput {
    pub(super) facts: InputFacts,
    pub(super) graphs: WorldGraphs,
    pub(super) sources: super::source_existentials::Collector,
    pub(super) occurrences: BTreeMap<String, Arc<[super::refute::RefutationPremise]>>,
    /// Source-owned contextual admission and formula lowering, shared by clones
    /// until a successful original-source edit replaces the exact preparation.
    pub(super) contextual: Arc<crate::contextual::native::NativeContextualProgram>,
    ingress_contract: [u8; 32],
}

impl PreparedReasoningInput {
    /// Apply one native assertion to an explicitly named source context. Original
    /// source columns remain separate from derived rows; no RDF carrier is rebuilt.
    pub(crate) fn assert_fact(
        &mut self,
        graph: crate::physical::LogicalGraph,
        fact: Fact,
    ) -> gmeow_errors::Result<bool> {
        let world = graph.world()?;
        let graph = graph.graph().cloned();
        if self.graphs.get(&world).is_some_and(|prior| prior != &graph) {
            return Err(reason_err(
                "assertion graph aliases a different source context".to_owned(),
            ));
        }
        let occurrence = super::refute::RefutationPremise {
            subject: fact.subject,
            predicate: fact.predicate,
            object: fact.object,
            graph: graph.clone(),
        };
        let mut sources: std::collections::BTreeSet<_> = self
            .occurrences
            .get(&world)
            .into_iter()
            .flat_map(|sources| sources.iter().cloned())
            .collect();
        if !sources.insert(occurrence) {
            return Ok(false);
        }
        let mut occurrences = self.occurrences.clone();
        occurrences.insert(
            world.clone(),
            sources.into_iter().collect::<Vec<_>>().into(),
        );
        let contextual = Arc::new(crate::contextual::native::NativeContextualProgram::prepare(
            &occurrences,
        )?);
        self.graphs.insert(world.clone(), graph);
        self.occurrences = occurrences;
        self.contextual = contextual;
        self.refresh_world(&world);
        self.refresh_source_contract();
        Ok(true)
    }

    pub(crate) fn retract_axiom(
        &mut self,
        axiom: &super::LeaveOneOutAxiom,
    ) -> gmeow_errors::Result<bool> {
        let mut changed = Vec::new();
        let mut occurrences = self.occurrences.clone();
        for (world, sources) in &mut occurrences {
            let next: Vec<_> = sources
                .iter()
                .filter(|source| {
                    !(source.subject.as_iri() == Some(axiom.subject.as_str())
                        && source.predicate == axiom.predicate
                        && source.object.as_iri() == Some(axiom.object.as_str()))
                })
                .cloned()
                .collect();
            if next.len() != sources.len() {
                *sources = next.into();
                changed.push(world.clone());
            }
        }
        if changed.is_empty() {
            return Ok(false);
        }
        let contextual = Arc::new(crate::contextual::native::NativeContextualProgram::prepare(
            &occurrences,
        )?);
        self.occurrences = occurrences;
        self.contextual = contextual;
        for world in &changed {
            self.refresh_world(world);
        }
        self.refresh_source_contract();
        Ok(true)
    }

    fn refresh_world(&mut self, world: &str) {
        let rows: BTreeMap<_, _> = self.occurrences[world]
            .iter()
            .map(|source| {
                let fact = Fact {
                    subject: crate::facts::skolemize(&source.subject).into_owned(),
                    predicate: source.predicate.clone(),
                    object: crate::facts::skolemize(&source.object).into_owned(),
                };
                (fact.key(), fact)
            })
            .collect();
        self.facts
            .insert(world.to_owned(), rows.into_values().collect());
    }

    fn refresh_source_contract(&mut self) {
        self.sources = super::source_existentials::Collector::default();
        for (world, sources) in &self.occurrences {
            for source in sources.iter() {
                self.sources.observe(
                    world,
                    &source.graph,
                    &source.subject,
                    &source.predicate,
                    &source.object,
                );
            }
        }
        self.ingress_contract = crate::physical::metadata_identity(
            "gmeow-native-ingress-v1",
            &(&self.graphs, &self.occurrences),
        );
    }
    /// Exact original native context identities, including declared empty graphs.
    #[must_use]
    pub fn source_contexts(&self) -> &BTreeMap<String, Option<TermValue>> {
        &self.graphs
    }
    /// Commitment to all original native statements and source graph identities.
    #[must_use]
    pub fn ingress_contract(&self) -> &[u8; 32] {
        &self.ingress_contract
    }
    /// Original asserted occurrence count in an admitted source context.
    /// Declared empty contexts return zero; an unknown context returns `None`.
    #[must_use]
    pub fn source_assertion_count(&self, world: &str) -> Option<usize> {
        self.occurrences.get(world).map(|sources| sources.len())
    }

    /// Register only the exact caller-selected logical worlds, including empty
    /// worlds absent from the source carrier. Graph presence never selects one.
    pub(super) fn admit_domains(
        &mut self,
        domains: &crate::physical::SelectedDomains,
    ) -> gmeow_errors::Result<()> {
        domains.validate()?;
        for domain in domains.worlds() {
            let world = domain.world()?;
            let graph = domain.graph().graph().cloned();
            if self
                .graphs
                .get(&world)
                .is_some_and(|existing| existing != &graph)
            {
                return Err(gmeow_errors::Diag::of_kind(crate::error::NativeCoverage {
                    profile: format!("{:?}", domain.profile()),
                    source: domain.authority().to_owned(),
                    world,
                    detail: "selected domain world key aliases a different native graph identity"
                        .to_owned(),
                }));
            }
            self.graphs.entry(world.clone()).or_insert(graph);
            self.occurrences
                .entry(world.clone())
                .or_insert_with(|| Arc::from([]));
            self.facts.entry(world).or_default();
        }
        Ok(())
    }
}

/// Visit the native assertion tables once without parsing or flattening contexts.
pub fn prepare_reasoning_input(
    edb: &impl purrdf::DatasetView,
) -> gmeow_errors::Result<PreparedReasoningInput> {
    let mut sources = super::source_existentials::Collector::default();
    let mut partitions = BTreeMap::<String, BTreeMap<FactKey, Fact>>::new();
    let mut origins = BTreeMap::<String, Option<TermValue>>::new();
    let mut occurrences =
        BTreeMap::<String, std::collections::BTreeSet<super::refute::RefutationPremise>>::new();
    let default = super::rl::DEFAULT_WORLD.to_owned();
    partitions.insert(default.clone(), BTreeMap::new());
    origins.insert(default, None);

    let mut admit_world = |graph: Option<TermValue>| -> gmeow_errors::Result<String> {
        let key = crate::physical::LogicalGraph::from_graph(graph.clone()).world()?;
        if let Some(prior) = origins.get(&key) {
            if prior != &graph {
                return Err(reason_err(format!(
                    "distinct source graphs share reasoning world key {key:?}; a world binding must be unambiguous"
                )));
            }
        } else {
            origins.insert(key.clone(), graph);
        }
        Ok(key)
    };

    for graph in edb.named_graphs() {
        let world = admit_world(Some(crate::reason::dataset::native(edb, graph)))?;
        partitions.entry(world).or_default();
    }
    for quad in edb
        .quads()
        .chain(edb.reifier_quads())
        .chain(edb.annotation_quads())
    {
        let graph = quad
            .g
            .map(|graph| crate::reason::dataset::native(edb, graph));
        let world = admit_world(graph.clone())?;
        let TermValue::Iri(predicate) = crate::reason::dataset::native(edb, quad.p) else {
            return Err(reason_err("reasoning predicate must be an IRI".to_owned()));
        };
        let source_subject = crate::reason::dataset::native(edb, quad.s);
        let subject = crate::facts::skolemize(&source_subject);
        // Preserve selected source owners until class/source admission can issue
        // its precise typed boundary. The executable input checks row subjects
        // after source-owned grammar has been admitted, before any write.
        let source_object = crate::reason::dataset::native(edb, quad.o);
        sources.observe(&world, &graph, &source_subject, &predicate, &source_object);
        occurrences
            .entry(world.clone())
            .or_default()
            .insert(super::refute::RefutationPremise {
                subject: source_subject.clone(),
                predicate: predicate.clone(),
                object: source_object.clone(),
                graph,
            });
        let object = crate::facts::skolemize(&source_object);
        let facts = partitions.entry(world).or_default();
        let fact = Fact {
            subject: subject.as_ref().clone(),
            predicate,
            object: object.as_ref().clone(),
        };
        facts.entry(fact.key()).or_insert(fact);
    }
    let occurrences = partitions
        .keys()
        .map(|world| {
            let sources: Vec<_> = occurrences
                .remove(world)
                .unwrap_or_default()
                .into_iter()
                .collect();
            (world.clone(), Arc::from(sources))
        })
        .collect();
    let ingress_contract =
        crate::physical::metadata_identity("gmeow-native-ingress-v1", &(&origins, &occurrences));
    let contextual = Arc::new(crate::contextual::native::NativeContextualProgram::prepare(
        &occurrences,
    )?);
    Ok(PreparedReasoningInput {
        ingress_contract,
        contextual,
        occurrences,
        facts: partitions
            .into_iter()
            .map(|(world, facts)| (world, facts.into_values().collect()))
            .collect(),
        graphs: origins,
        sources,
    })
}

/// Keep only laws whose fixed vocabulary can be present in the selected run.
/// The abstract domain is the small set of constants required by the native laws,
/// not a retained corpus or a sample. Ordinary rule constants and newly enabled
/// law heads participate until this finite analysis reaches its fixed point.
pub(super) fn applicable_schema_laws(
    prepared: &crate::program_analysis::PreparedProgram,
    facts: &BTreeMap<String, Vec<Fact>>,
    sources: &super::source_existentials::PreparedSources,
    domains: &crate::physical::SelectedDomains,
    possible: &[(String, Fact)],
    contextual_effects: &[crate::physical::WorldProducerEffect],
) -> Vec<PreparedPropertyRule> {
    use crate::rule_ir::EvalTerm;
    use std::collections::BTreeSet;
    let laws = super::schema::laws();
    if !contextual_effects.is_empty() {
        // Contextual metadata has exact predicate/marker envelopes but minted
        // subject and value identities. Retain every native law here; the shared
        // abstract analysis narrows reachable writers from those typed envelopes.
        // Current source absence never authorizes omitting a future consumer.
        return laws.to_vec();
    }
    let ordinary = prepared.reasoning_rules();
    let domain_rules: Vec<_> = domains.worlds().iter().map(|world| world.rule()).collect();
    let producers: Vec<_> = prepared
        .existential_rules
        .iter()
        .chain(sources.rules.iter().map(|source| &source.rule))
        .chain(&domain_rules)
        .collect();
    let mut wanted: BTreeSet<&str> = laws
        .iter()
        .flat_map(|law| &law.source.body)
        .flat_map(|atom| &atom.0)
        .filter_map(|term| match term {
            EvalTerm::ConstNamed(iri) => Some(iri.as_str()),
            _ => None,
        })
        .collect();
    for atom in ordinary
        .iter()
        .flat_map(|rule| std::iter::once(&rule.head).chain(&rule.body))
        .chain(
            producers
                .iter()
                .flat_map(|rule| rule.head.iter().chain(&rule.body)),
        )
    {
        wanted.insert(&atom.predicate);
    }
    let semantics = crate::native_semantics::SemanticVocabulary::GroundedLogicV1;
    let observe = |iri: &str, present: &mut BTreeSet<String>| {
        for spelling in semantics.possible_spellings(iri) {
            if wanted.contains(spelling) {
                present.insert(spelling.to_owned());
            }
        }
    };
    let observe_predicate = |iri: &str, predicates: &mut BTreeSet<String>| {
        predicates.insert(iri.to_owned());
        predicates.extend(semantics.alternate_predicate(iri).map(str::to_owned));
    };
    let mut present = BTreeSet::new();
    let mut predicates = BTreeSet::new();
    for fact in facts
        .values()
        .flatten()
        .chain(possible.iter().map(|(_, fact)| fact))
    {
        observe_predicate(&fact.predicate, &mut predicates);
        observe(&fact.predicate, &mut present);
        for term in [&fact.subject, &fact.object] {
            if let TermValue::Iri(iri) = term {
                observe(iri, &mut present);
            }
        }
    }
    let head = |atom: &crate::rule_ir::EvalAtom,
                predicates: &mut BTreeSet<String>,
                present: &mut BTreeSet<String>| {
        observe_predicate(&atom.predicate, predicates);
        observe(&atom.predicate, present);
        for term in [&atom.subject, &atom.object] {
            match term {
                EvalTerm::ConstNamed(iri) | EvalTerm::ConstLit(TermValue::Iri(iri)) => {
                    observe(iri, present)
                }
                _ => {}
            }
        }
    };
    observe_predicate(
        "https://blackcatinformatics.ca/logic/instanceOf",
        &mut predicates,
    );
    observe(
        "https://blackcatinformatics.ca/logic/instanceOf",
        &mut present,
    );
    observe("https://blackcatinformatics.ca/logic/Nothing", &mut present);
    let mut selected = vec![false; laws.len()];
    loop {
        let before = (
            predicates.len(),
            present.len(),
            selected.iter().filter(|selected| **selected).count(),
        );
        for rule in ordinary {
            if rule.reduction.is_some()
                || rule
                    .body
                    .iter()
                    .filter(|atom| !atom.negated)
                    .all(|atom| predicates.contains(&atom.predicate))
            {
                head(&rule.head, &mut predicates, &mut present);
                if !rule.builtins.is_empty() || rule.reduction.is_some() {
                    // Complete reductions can fire over an empty extension. Until
                    // a producer provides a vocabulary range certificate,
                    // its output values conservatively include every tracked IRI.
                    present.extend(wanted.iter().map(|iri| (*iri).to_owned()));
                }
            }
        }
        for rule in &producers {
            if rule
                .body
                .iter()
                .all(|atom| predicates.contains(&atom.predicate))
            {
                for atom in &rule.head {
                    head(atom, &mut predicates, &mut present);
                }
            }
        }
        for (index, law) in laws.iter().enumerate() {
            let readable = law.selection_reads().all(|predicate| {
                predicate.map_or(!predicates.is_empty(), |p| predicates.contains(p))
            });
            if readable && law.constants_available(&present) {
                selected[index] = true;
                for term in law.analysis_heads.iter().flat_map(|head| &head.0) {
                    if let EvalTerm::ConstNamed(iri) = term {
                        observe(iri, &mut present);
                    }
                }
                for predicate in law.head_predicates() {
                    if let Some(predicate) = predicate {
                        observe_predicate(predicate, &mut predicates);
                    } else {
                        // A bound predicate can be any reachable tracked IRI. Invented
                        // terms require the complete conservative predicate inventory;
                        // the execution admission separately withholds an unproved
                        // existential/schema termination claim.
                        if producers.iter().any(|rule| !rule.existentials().is_empty())
                            || laws
                                .iter()
                                .zip(&selected)
                                .any(|(law, selected)| *selected && law.witness_frontier.is_some())
                        {
                            predicates.extend(wanted.iter().map(|iri| (*iri).to_owned()));
                        } else {
                            predicates.extend(present.iter().cloned());
                        }
                    }
                }
            }
        }
        let after = (
            predicates.len(),
            present.len(),
            selected.iter().filter(|selected| **selected).count(),
        );
        if before == after {
            break;
        }
    }
    laws.iter()
        .zip(selected)
        .filter(|(_, selected)| *selected)
        .map(|(law, _)| law.clone())
        .collect()
}

/// Execute ordinary, conjunctive formula and native property rules against one input.
pub(super) fn execute(
    prepared: &crate::program_analysis::PreparedProgram,
    input: PreparedReasoningInput,
    domains: &crate::physical::SelectedDomains,
    max_steps: Option<u64>,
) -> gmeow_errors::Result<ProgramClosure> {
    execute_transaction(prepared, input, domains, max_steps, &[], None)
}

pub(super) fn execute_transaction(
    prepared: &crate::program_analysis::PreparedProgram,
    mut input: PreparedReasoningInput,
    domains: &crate::physical::SelectedDomains,
    max_steps: Option<u64>,
    potential: &[(String, Fact)],
    retained: Option<&mut crate::physical::RetainedJoint>,
) -> gmeow_errors::Result<ProgramClosure> {
    prepared.admission.admit_world_local_template()?;
    input.admit_domains(domains)?;
    let modal = crate::modal::native::NativeModalProgram::prepare(&input.occurrences)?;
    for (world, graph) in modal
        .required_worlds()
        .iter()
        .chain(input.contextual.required_worlds())
    {
        let graph = graph.graph().cloned();
        if input.graphs.get(world).is_some_and(|prior| prior != &graph) {
            return Err(reason_err(
                "cross-world source requirement aliases another native graph identity".to_owned(),
            ));
        }
        input.graphs.entry(world.clone()).or_insert(graph);
        input
            .occurrences
            .entry(world.clone())
            .or_insert_with(|| Arc::from([]));
        input.facts.entry(world.clone()).or_default();
    }
    let possible = modal.possible_heads();
    let PreparedReasoningInput {
        facts,
        graphs,
        sources,
        occurrences,
        contextual,
        ..
    } = input;
    let sources = sources.prepare()?;
    let selected_potential: Vec<_> = possible.iter().chain(potential).cloned().collect();
    let properties = applicable_schema_laws(
        prepared,
        &facts,
        &sources,
        domains,
        &selected_potential,
        contextual.effects(),
    );
    let admitted_input = prepared.reasoning_input(
        &properties,
        &sources,
        domains,
        &facts,
        Arc::clone(&possible),
        contextual.effects(),
    )?;
    let binding =
        admitted_input.bind_native(&occurrences, &graphs, Some(modal), Some(contextual))?;
    if binding.source_refused {
        return Err(gmeow_errors::Diag::of_kind(
            crate::error::NativeSourceAdmission {
                input_contract: binding.input_contract,
                admission: binding.class_admission.clone(),
            },
        ));
    }
    let input_contract = binding.input_contract;
    let plan = prepared.reasoning_schema_program(&admitted_input)?;
    let program = match plan {
        NativeOutcome::Decided(program) => program,
        NativeOutcome::Unsupported(kind) => {
            return Err(reason_err(format!(
                "native program preparation refused {kind:?}"
            )));
        }
    };
    let mut governor = crate::physical::StepGovernor::new(max_steps);
    let mut registry = crate::physical::SkolemRegistry::new();
    let execution = match retained {
        Some(retained) => program.materialize_input_retained(
            &admitted_input,
            binding,
            &mut governor,
            &mut registry,
            retained,
        ),
        None => program.materialize_input_governed(
            &admitted_input,
            binding,
            &mut governor,
            &mut registry,
        ),
    }?;
    let result = match execution {
        NativeOutcome::Decided(result) => result,
        NativeOutcome::Unsupported(kind) => {
            return Err(reason_err(format!(
                "native program execution refused {kind:?}: {:?}",
                program.admission.capability_gap_rows()
            )));
        }
    };
    publish(result, input_contract, graphs, domains, &program.admission)
}

/// The class-only diagnostic selects no ordinary, schema or authored EDB
/// existential producers. Actual selected domain laws and class proofs still
/// execute in the same native store and governor.
pub(super) enum ClassDiagnosticRun {
    SourceRefused {
        admission: super::refute::ClassAdmissionObservation,
    },
    Executed {
        closure: ProgramClosure,
    },
}

pub(super) fn class_diagnostic(
    mut input: PreparedReasoningInput,
    domains: &crate::physical::SelectedDomains,
    max_steps: Option<u64>,
) -> gmeow_errors::Result<ClassDiagnosticRun> {
    input.admit_domains(domains)?;
    let selected = domains
        .worlds()
        .iter()
        .map(|world| world.world())
        .collect::<gmeow_errors::Result<std::collections::BTreeSet<_>>>()?;
    if selected != input.graphs.keys().cloned().collect() {
        return Err(reason_err("class diagnostic requires an explicit logical-domain selection for each execution context".to_owned()));
    }
    let template = Arc::new(crate::physical::JointTemplate::class_diagnostic(domains)?);
    let admitted = template.input(&input.facts, Arc::from([]), &[])?;
    let binding = admitted.bind_native(&input.occurrences, &input.graphs, None, None)?;
    if binding.source_refused {
        return Ok(ClassDiagnosticRun::SourceRefused {
            admission: binding.class_admission.clone(),
        });
    }
    let input_contract = binding.input_contract;
    let NativeOutcome::Decided(program) = admitted.prepare()? else {
        return Err(reason_err(
            "selected class diagnostic has no complete dependency admission".to_owned(),
        ));
    };
    let mut governor = crate::physical::StepGovernor::new(max_steps);
    let mut registry = crate::physical::SkolemRegistry::new();
    let NativeOutcome::Decided(result) =
        program.materialize_input_governed(&admitted, binding, &mut governor, &mut registry)?
    else {
        return Err(reason_err(
            "selected class diagnostic has no terminating or budgeted producer admission"
                .to_owned(),
        ));
    };
    Ok(ClassDiagnosticRun::Executed {
        closure: publish(
            result,
            input_contract,
            input.graphs,
            domains,
            &program.admission,
        )?,
    })
}

fn publish(
    result: JointMaterialization,
    input_contract: [u8; 32],
    graphs: WorldGraphs,
    domains: &crate::physical::SelectedDomains,
    admission: &crate::physical::ChaseAdmission,
) -> gmeow_errors::Result<ProgramClosure> {
    let JointMaterialization {
        result,
        witness_derivations,
        native_families,
        source_coverage,
        terminal,
        inference_exhausted,
        class_admission,
        classes,
    } = result;
    let class_admission = class_admission.ok_or_else(|| {
        reason_err("native execution omitted its required class source admission".to_owned())
    })?;
    let frontier = result.frontier();
    let status = result.status;
    let consumed_steps = result.consumed_steps;
    let certificates = graphs
        .keys()
        .map(|world| ChaseCertificate {
            world: world.clone(),
            input_contract,
            admission: admission.clone(),
        })
        .collect();
    let inferred: Vec<_> = result
        .rows
        .into_iter()
        .map(|row| {
            let premises = if let Some(evidence) = &row.cross_world {
                evidence.presentation_premises()
            } else {
                row.antecedents
                    .into_iter()
                    .map(|premise| {
                        Ok((
                            subject_iri(&premise.subject)?,
                            premise.predicate,
                            crate::provenance::term_display(&premise.object),
                        ))
                    })
                    .collect::<gmeow_errors::Result<Vec<_>>>()?
            };
            let is_edb = row.rule_iri == crate::provenance::ASSERT_RULE_IRI;
            Ok(InferredAxiom {
                modal_evaluation: row
                    .cross_world
                    .as_ref()
                    .and_then(|evidence| evidence.fixed_modal())
                    .map(|evidence| Box::new(evidence.evaluation.clone())),
                subject: subject_iri(&row.subject)?,
                predicate: row.predicate,
                object: row.object,
                world: row.graph,
                is_edb,
                rule_name: (!is_edb).then_some(row.rule_iri),
                premises,
            })
        })
        .collect::<gmeow_errors::Result<Vec<_>>>()?;
    Ok(ProgramClosure {
        inferred,
        frontier,
        status,
        native_status: terminal,
        class_admission,
        classes,
        consumed_steps,
        inference_exhausted,
        certificates,
        witnesses: witness_derivations,
        selected_domains: domains.clone(),
        input_contract,
        graphs,
        native_families,
        source_coverage: super::dl::SourceCoverageObservation {
            input_contract,
            worlds: source_coverage,
        },
    })
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod contextual_input_tests;

#[cfg(test)]
mod list_tests;

#[cfg(test)]
mod literal_tests;

#[cfg(test)]
mod semantic_tests;

#[cfg(test)]
mod dl_input_tests;

#[cfg(test)]
mod clash_tests;

#[cfg(test)]
mod minimum_tests;

#[cfg(test)]
mod equality_tests;

#[cfg(test)]
mod empty_class_tests;

#[cfg(test)]
mod domain_tests;

#[cfg(test)]
mod joint_family_tests;
