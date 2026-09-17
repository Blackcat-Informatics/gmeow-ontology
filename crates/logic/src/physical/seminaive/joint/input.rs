// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared producer analysis and source-bound execution admission.
//!
//! The borrowed input cannot change between abstract admission and execution. A
//! cached schedule is reusable only for the same complete abstract input shape;
//! neither a predicate-only key nor a caller-supplied digest can authorize it.

use std::collections::{BTreeMap, VecDeque};
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

use super::{
    JointMaterialization, JointProgram, NativeOutcome, PreparedPropertyRule, producer_effects,
    seminaive_err,
};
use crate::native_semantics::SemanticVocabulary;
use crate::physical::chase::{ChaseAdmission, ExistentialRule, PreparedChaseRule};
use crate::physical::effects::value_flow::{FlowRule, FlowSummary, ValueFlow};
use crate::physical::effects::{ProducerEffect, StatementPattern, WorldProducerEffect};
use crate::physical::plan::RuleLayouts;
use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm, Fact};

mod admission;

/// A digest-only formatter for immutable native rule metadata. No serialized RDF
/// or temporary text buffer is produced; the executable source remains native IR.
struct MetadataDigest(blake3::Hasher, usize);

impl std::fmt::Write for MetadataDigest {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        self.0.update(text.as_bytes());
        self.1 = self.1.saturating_add(text.len());
        Ok(())
    }
}

/// Selection identity inside one immutable PreparedProgram. All property law
/// fields, including list operators and source names, participate in this key.
pub(crate) fn schema_identity(properties: &[PreparedPropertyRule]) -> [u8; 32] {
    let mut digest = MetadataDigest(blake3::Hasher::new(), 0);
    digest.0.update(b"gmeow-native-schema-template-v1\0");
    digest.0.update(&(properties.len() as u64).to_le_bytes());
    for property in properties {
        write!(digest, "{:?}", property.source).expect("digest-only formatter");
    }
    *digest.0.finalize().as_bytes()
}

/// Source-independent layouts and full native producer effects, shared by every
/// input shape of the selected program. Bounded source/domain ownership is retained;
/// input facts and results remain run-local.
pub(crate) struct JointTemplate {
    identity: [u8; 32],
    witness_contract: crate::physical::store::WitnessContract,
    metadata_bytes: usize,
    rules: RuleLayouts,
    producers: Vec<Arc<PreparedChaseRule>>,
    properties: Vec<PreparedPropertyRule>,
    effects: Vec<ProducerEffect>,
    flow: Arc<ValueFlow>,
    semantics: SemanticVocabulary,
    admission: ChaseAdmission,
    termination: Option<admission::Template>,
    analyses: Mutex<VecDeque<CachedEffects>>,
    definitions: Option<DefinitionContract>,
    families: Vec<super::families::Arm>,
    family_reads: Vec<crate::reason::refute::native::NativeRead>,
    domains: crate::physical::SelectedDomains,
    operation: super::JointOperation,
}

/// Explicit immutable-definition admission; absence is valid only for operations
/// that do not select source grammar execution at all.
#[derive(Debug, Clone)]
pub(crate) struct DefinitionContract {
    pub(crate) profile: String,
    pub(crate) patterns: Vec<StatementPattern>,
    pub(crate) owners: Vec<(String, String)>,
}

struct CachedEffects {
    identity: [u8; 32],
    effects: Arc<Vec<ProducerEffect>>,
}

impl std::fmt::Debug for JointTemplate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JointTemplate")
            .field("identity", &self.identity)
            .field("producers", &self.effects.len())
            .finish()
    }
}

fn statement(atom: &EvalAtom) -> [EvalTerm; 3] {
    [
        atom.subject.clone(),
        EvalTerm::named(&atom.predicate),
        atom.object.clone(),
    ]
}

impl JointTemplate {
    pub(crate) fn with_witness_source(
        mut self,
        source: &crate::native_semantics::ProgramAdmission,
    ) -> Self {
        self.witness_contract = self.witness_contract.with_source(source);
        self.identity = crate::physical::store::metadata_identity(
            "gmeow-joint-source-contract-v1",
            &(self.identity, self.witness_contract),
        );
        self
    }

    pub(crate) fn new(
        rules: &[EvalRule],
        producers: &[ExistentialRule],
        properties: &[PreparedPropertyRule],
        semantics: SemanticVocabulary,
    ) -> gmeow_errors::Result<Self> {
        Self::build(
            rules,
            producers,
            properties,
            semantics,
            &[],
            None,
            &crate::physical::SelectedDomains::new([])?,
            super::JointOperation::Relational,
        )
    }

    pub(crate) fn with_sources(
        rules: &[EvalRule],
        templates: &[ExistentialRule],
        properties: &[PreparedPropertyRule],
        semantics: SemanticVocabulary,
        sources: &[crate::physical::chase::SourceExistentialRule],
        definitions: Option<DefinitionContract>,
        domains: &crate::physical::SelectedDomains,
    ) -> gmeow_errors::Result<Self> {
        Self::build(
            rules,
            templates,
            properties,
            semantics,
            sources,
            definitions,
            domains,
            super::JointOperation::Forward,
        )
    }

    pub(crate) fn class_diagnostic(
        domains: &crate::physical::SelectedDomains,
    ) -> gmeow_errors::Result<Self> {
        Self::build(
            &[],
            &[],
            &[],
            SemanticVocabulary::GroundedLogicV1,
            &[],
            None,
            domains,
            super::JointOperation::ClassDiagnostic,
        )
    }

    fn build(
        rules: &[EvalRule],
        templates: &[ExistentialRule],
        properties: &[PreparedPropertyRule],
        semantics: SemanticVocabulary,
        sources: &[crate::physical::chase::SourceExistentialRule],
        definitions: Option<DefinitionContract>,
        domains: &crate::physical::SelectedDomains,
        operation: super::JointOperation,
    ) -> gmeow_errors::Result<Self> {
        if !sources.is_empty() && definitions.is_none() {
            return Err(seminaive_err(
                "source-owned producers require their immutable-definition contract",
            ));
        }
        domains.validate()?;
        let producers: Vec<_> = templates
            .iter()
            .cloned()
            .chain(sources.iter().map(|source| source.rule.clone()))
            .chain(domains.worlds().iter().map(|world| world.rule()))
            .collect();
        let producers = producers.as_slice();
        let mut families = match operation {
            super::JointOperation::Relational => Vec::new(),
            super::JointOperation::Forward => super::families::arms(),
            super::JointOperation::ClassDiagnostic => super::families::arms()
                .into_iter()
                .filter(|arm| arm.family == super::families::Producer::Class)
                .collect(),
        };
        match operation {
            super::JointOperation::Forward => {
                families.extend(super::families::admission_arms(crate::reason::schema_laws()))
            }
            super::JointOperation::Relational => {
                families.extend(super::families::admission_arms(properties))
            }
            super::JointOperation::ClassDiagnostic => {}
        }
        let family_reads: std::collections::BTreeSet<_> = families
            .iter()
            .flat_map(|arm| &arm.preparation)
            .cloned()
            .chain(crate::reason::dl::source_admission_reads())
            .collect();
        let family_reads = family_reads.into_iter().collect::<Vec<_>>();
        let mut effects = producer_effects(rules, producers, properties)?;
        effects.extend(families.iter().map(|arm| arm.effect.clone()));
        let mut flows: Vec<_> = rules
            .iter()
            .map(|rule| {
                let mut reads: Vec<_> =
                    rule.body.iter().map(|atom| Some(statement(atom))).collect();
                reads.extend(
                    rule.builtins
                        .iter()
                        .flat_map(crate::physical::builtin_eval::read_patterns)
                        .map(|_| None),
                );
                FlowRule {
                    // Reductions can invent values and a keyless empty group.
                    // Ordinary positive reachability cannot narrow these effects;
                    // retain conservative reads/writes for their completion proof.
                    body: if rule.reduction.is_some() {
                        Vec::new()
                    } else {
                        rule.body
                            .iter()
                            .filter(|atom| !atom.negated)
                            .map(statement)
                            .collect()
                    },
                    heads: vec![statement(&rule.head)],
                    native_witnesses: Vec::new(),
                    reads,
                }
            })
            .collect();
        flows.extend(producers.iter().map(|rule| FlowRule {
            body: rule.body.iter().map(statement).collect(),
            heads: rule.head.iter().map(statement).collect(),
            native_witnesses: rule.existentials(),
            reads: rule.body.iter().map(|atom| Some(statement(atom))).collect(),
        }));
        flows.extend(properties.iter().map(|rule| {
            let mut reads: Vec<_> = rule
                .source
                .body
                .iter()
                .map(|atom| Some(atom.0.clone()))
                .collect();
            reads.extend(rule.reads().skip(rule.source.body.len()).map(|_| None));
            reads.extend(rule.admission_reads().iter().map(|_| None));
            FlowRule {
                body: rule
                    .analysis_body
                    .iter()
                    .map(|atom| atom.0.clone())
                    .collect(),
                heads: rule
                    .analysis_heads
                    .iter()
                    .map(|head| head.0.clone())
                    .collect(),
                native_witnesses: Vec::new(),
                reads,
            }
        }));
        flows.extend(families.iter().map(|arm| arm.flow.clone()));
        let protected_definitions = definitions
            .as_ref()
            .map_or_else(Vec::new, |contract| contract.patterns.clone());
        let mut observed_definitions = protected_definitions.clone();
        observed_definitions.extend(crate::modal::native::definition_vocabulary());
        observed_definitions.extend(crate::contextual::native::definition_vocabulary());
        let flow = Arc::new(ValueFlow::with_conditioned_observations(
            &flows,
            &effects,
            semantics,
            &observed_definitions,
            &protected_definitions,
        ));
        let admission =
            super::producer_admission(rules, producers, properties, &families, semantics);
        let termination = if admission.admits_native() {
            None
        } else {
            admission::Template::new(rules, producers, properties, &families)
        };
        let mut digest = MetadataDigest(blake3::Hasher::new(), 0);
        digest.0.update(b"gmeow-native-joint-value-flow-v5\0");
        write!(digest, "{rules:?}").expect("digest-only formatter");
        for property in properties {
            write!(digest, "{:?}", property.source).expect("digest-only formatter");
        }
        write!(
            digest,
            "{semantics:?}:{producers:?}:{sources:?}:{definitions:?}:{domains:?}:{operation:?}"
        )
        .expect("digest-only formatter");
        // All retained analyses participate in the same payload bound. The fixed
        // native source admission arms can outnumber the selected property joins.
        write!(digest, "{effects:?}:{flows:?}:{family_reads:?}").expect("digest-only formatter");
        for arm in &families {
            write!(digest, "{:?}:{:?}", arm.family, arm.statement).expect("digest-only formatter");
        }
        let mut prepared_producers = super::prepare_producers(templates)?;
        prepared_producers.extend(
            sources
                .iter()
                .cloned()
                .map(PreparedChaseRule::from_source)
                .map(|result| result.map(Arc::new))
                .collect::<gmeow_errors::Result<Vec<_>>>()?,
        );
        prepared_producers.extend(
            domains
                .worlds()
                .iter()
                .cloned()
                .map(PreparedChaseRule::from_domain)
                .map(|result| result.map(Arc::new))
                .collect::<gmeow_errors::Result<Vec<_>>>()?,
        );
        Ok(Self {
            witness_contract: crate::physical::store::WitnessContract::native(semantics),
            identity: *digest.0.finalize().as_bytes(),
            metadata_bytes: digest.1,
            rules: RuleLayouts::new(rules)?,
            producers: prepared_producers,
            properties: properties.to_vec(),
            effects,
            flow,
            semantics,
            admission,
            termination,
            analyses: Mutex::new(VecDeque::new()),
            definitions,
            families,
            family_reads,
            domains: domains.clone(),
            operation,
        })
    }

    pub(crate) fn cacheable(&self) -> bool {
        self.metadata_bytes <= 1024 * 1024
    }

    /// This constructor observes EVERY selected fact while holding the immutable
    /// input borrow used by execution. Callers cannot manufacture summary identity.
    pub(crate) fn input<'a>(
        self: &Arc<Self>,
        facts: &'a BTreeMap<String, Vec<Fact>>,
        possible: Arc<[(String, Fact)]>,
        contextual_effects: &[WorldProducerEffect],
    ) -> gmeow_errors::Result<JointInput<'a>> {
        for domain in self.domains.worlds() {
            if !facts.contains_key(&domain.world()?) {
                return Err(seminaive_err(
                    "selected logical world has no admitted native input partition",
                ));
            }
        }
        if contextual_effects
            .iter()
            .any(|effect| !facts.contains_key(&effect.owner))
        {
            return Err(seminaive_err(
                "contextual output envelope has no admitted owner world",
            ));
        }
        let flow = if contextual_effects.is_empty() {
            self.flow.as_ref().clone()
        } else {
            self.flow.with_additional_observations(
                contextual_effects
                    .iter()
                    .flat_map(|effect| effect.effect.writes.iter()),
            )
        };
        let certificate_flow = Arc::new(flow.clone());
        let flow = flow.with_source_operators(
            facts
                .values()
                .flatten()
                .chain(possible.iter().map(|(_, fact)| fact)),
        );
        let flow = Arc::new(flow);
        let flow_contract = crate::physical::metadata_identity(
            "gmeow-native-flow-observations-v1",
            &(
                self.identity,
                contextual_effects,
                flow.vocabulary_identity(),
            ),
        );
        let mut summary = flow.summarize(
            facts
                .values()
                .flatten()
                .chain(possible.iter().map(|(_, fact)| fact)),
        );
        flow.seed_patterns(
            &mut summary,
            contextual_effects
                .iter()
                .flat_map(|effect| &effect.effect.writes),
        );
        let shape = summary.identity(&flow_contract);
        let effects = self.effects(&flow, &summary, shape)?;
        let mut world_effects = BTreeMap::new();
        let mut world_shapes = Vec::new();
        for (world, local) in facts {
            let mut summary = flow.summarize(
                local.iter().chain(
                    possible
                        .iter()
                        .filter(|(owner, _)| owner == world)
                        .map(|(_, fact)| fact),
                ),
            );
            flow.seed_patterns(
                &mut summary,
                contextual_effects
                    .iter()
                    .filter(|effect| effect.owner == *world)
                    .flat_map(|effect| &effect.effect.writes),
            );
            let mut enabled = vec![true; self.effects.len()];
            for (index, producer) in self.producers.iter().enumerate() {
                enabled[self.rules.keys().len() + index] = producer.owns_world(world)?;
            }
            let scoped = flow.refine_enabled(&self.effects, &summary, &enabled);
            if let Some(contract) = &self.definitions {
                for pattern in &contract.patterns {
                    if let Some(writer) = flow.overlapping_writer(pattern, &scoped) {
                        let source = contract
                            .owners
                            .iter()
                            .find(|(_, owner)| owner == world)
                            .map_or("<selected source grammar>", |(source, _)| source);
                        return Err(gmeow_errors::Diag::of_kind(crate::error::NativeCoverage {
                            profile: contract.profile.clone(),
                            source: source.to_owned(),
                            world: world.clone(),
                            detail: format!(
                                "immutable definitions cannot admit reachable writer {writer:?} of {pattern:?}"
                            ),
                        }));
                    }
                }
            }
            world_shapes.push((summary.identity(&flow_contract), enabled));
            world_effects.insert(world.clone(), Arc::new(scoped));
        }
        let evidence = self
            .termination
            .as_ref()
            // Selected contextual producers enter the same abstract summary and
            // immutable-predicate check. Source absence alone proves nothing;
            // an external envelope either participates in the finite bound or
            // prevents the source-concrete certificate.
            .filter(|_| self.cacheable())
            .and_then(|termination| {
                termination.observe(
                    facts,
                    &possible,
                    &flow,
                    &effects,
                    contextual_effects,
                    self.semantics,
                )
            });
        world_shapes.sort();
        let mut identity = crate::physical::metadata_identity(
            "gmeow-native-world-input-shape-v1",
            &(shape, world_shapes, contextual_effects),
        );
        if let Some(evidence) = &evidence {
            let mut digest = blake3::Hasher::new();
            digest.update(b"gmeow-native-source-admission-v1\0");
            digest.update(&identity);
            digest.update(&evidence.identity);
            identity = *digest.finalize().as_bytes();
        }
        Ok(JointInput {
            template: Arc::clone(self),
            facts,
            possible,
            contextual_effects: contextual_effects.to_vec().into(),
            certificate_flow,
            flow,
            effects,
            world_effects,
            evidence,
            identity,
        })
    }

    /// At most eight complete abstract analyses, each with at most one MiB of
    /// formatted metadata. Source facts never enter this cache. Distinct exact
    /// static bindings can share the expensive full-reachability analysis while
    /// retaining independent termination admissions and executable schedules.
    fn effects(
        &self,
        flow: &ValueFlow,
        summary: &FlowSummary,
        identity: [u8; 32],
    ) -> gmeow_errors::Result<Arc<Vec<ProducerEffect>>> {
        let lock_error = |_| seminaive_err("native value-flow cache lock poisoned");
        if let Some(entry) = self
            .analyses
            .lock()
            .map_err(lock_error)?
            .iter()
            .find(|entry| entry.identity == identity)
        {
            return Ok(Arc::clone(&entry.effects));
        }
        let effects = Arc::new(flow.refine(&self.effects, summary));
        let mut digest = MetadataDigest(blake3::Hasher::new(), 0);
        write!(digest, "{effects:?}").expect("digest-only formatter");
        if self.cacheable() && digest.1 <= 1024 * 1024 {
            let mut cache = self.analyses.lock().map_err(lock_error)?;
            if let Some(entry) = cache.iter().find(|entry| entry.identity == identity) {
                return Ok(Arc::clone(&entry.effects));
            }
            if cache.len() == 8 {
                cache.pop_front();
            }
            cache.push_back(CachedEffects {
                identity,
                effects: Arc::clone(&effects),
            });
        }
        Ok(effects)
    }
}

/// Native input plus its complete abstract shape, authenticated by construction.
/// The facts remain borrowed from the caller; no corpus clone or cache is created.
pub(crate) struct JointInput<'a> {
    template: Arc<JointTemplate>,
    facts: &'a BTreeMap<String, Vec<Fact>>,
    possible: Arc<[(String, Fact)]>,
    contextual_effects: Arc<[WorldProducerEffect]>,
    certificate_flow: Arc<ValueFlow>,
    flow: Arc<ValueFlow>,
    effects: Arc<Vec<ProducerEffect>>,
    world_effects: BTreeMap<String, Arc<Vec<ProducerEffect>>>,
    evidence: Option<admission::Evidence<'a>>,
    identity: [u8; 32],
}

/// Exact original native occurrences for one selected execution. Source statements
/// are borrowed from ingress, never reconstructed from already-derived rows.
pub(crate) struct NativeInputBinding<'a> {
    pub(super) template: [u8; 32],
    pub(super) world_effects: &'a BTreeMap<String, Arc<Vec<ProducerEffect>>>,
    pub(super) flow: &'a ValueFlow,
    pub(super) sources: &'a BTreeMap<String, Arc<[crate::reason::refute::RefutationPremise]>>,
    pub(super) graphs: &'a BTreeMap<String, Option<purrdf::TermValue>>,
    pub(super) domains: &'a crate::physical::SelectedDomains,
    pub(crate) input_contract: [u8; 32],
    shape: [u8; 32],
    pub(super) classes: BTreeMap<String, crate::reason::refute::PreparedClassAnalysis>,
    pub(crate) class_admission: crate::reason::refute::ClassAdmissionObservation,
    pub(crate) source_refused: bool,
    pub(super) modal: Option<crate::modal::native::NativeModalProgram>,
    pub(super) contextual: Option<Arc<crate::contextual::native::NativeContextualProgram>>,
}

impl JointInput<'_> {
    pub(crate) fn bind_native<'a>(
        &'a self,
        sources: &'a BTreeMap<String, Arc<[crate::reason::refute::RefutationPremise]>>,
        graphs: &'a BTreeMap<String, Option<purrdf::TermValue>>,
        modal: Option<crate::modal::native::NativeModalProgram>,
        contextual: Option<Arc<crate::contextual::native::NativeContextualProgram>>,
    ) -> gmeow_errors::Result<NativeInputBinding<'a>> {
        if (self.template.operation == super::JointOperation::Forward) != modal.is_some() {
            return Err(seminaive_err(
                "native operation has no matching mandatory modal source admission",
            ));
        }
        if (self.template.operation == super::JointOperation::Forward) != contextual.is_some() {
            return Err(seminaive_err(
                "native operation has no matching mandatory contextual source admission",
            ));
        }
        let actual_effects = contextual
            .as_ref()
            .map_or(&[][..], |program| program.effects());
        if crate::physical::metadata_identity(
            "gmeow-contextual-output-envelope-v1",
            &actual_effects,
        ) != crate::physical::metadata_identity(
            "gmeow-contextual-output-envelope-v1",
            &self.contextual_effects.as_ref(),
        ) {
            return Err(seminaive_err(
                "contextual output envelope differs from selected source preparation",
            ));
        }
        if modal.as_ref().map_or(!self.possible.is_empty(), |modal| {
            modal.possible_heads().as_ref() != self.possible.as_ref()
        }) {
            return Err(seminaive_err(
                "native abstract output admission differs from selected modal source",
            ));
        }
        if self.template.families.is_empty()
            || sources.keys().ne(self.facts.keys())
            || graphs.keys().ne(self.facts.keys())
        {
            return Err(seminaive_err(
                "native source admission must cover exactly every input world",
            ));
        }
        for (world, graph) in graphs {
            if crate::physical::LogicalGraph::from_graph(graph.clone()).world()? != *world
                || sources[world].iter().any(|source| source.graph != *graph)
            {
                return Err(seminaive_err(
                    "native source occurrence has inconsistent original graph ownership",
                ));
            }
        }
        let mut classes = BTreeMap::new();
        let mut class_admission = crate::reason::refute::ClassAdmissionObservation {
            contract: crate::reason::refute::CLASS_EXPRESSION_SOURCE_ADMISSION_ID.to_owned(),
            source_worlds: BTreeMap::new(),
            selected_worlds: BTreeMap::new(),
        };
        for (world, graph) in graphs {
            let class = crate::reason::refute::PreparedClassAnalysis::from_native_sources(
                world,
                graph.as_ref(),
                &sources[world],
            )?;
            class_admission.source_worlds.extend(
                class
                    .admission()
                    .source_worlds
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone())),
            );
            class_admission.selected_worlds.extend(
                class
                    .admission()
                    .selected_worlds
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone())),
            );
            classes.insert(world.clone(), class);
        }
        class_admission.validate()?;
        fn hard_source_refusal(
            boundary: &crate::reason::refute::FragmentBoundary,
            has_list_writer: bool,
        ) -> bool {
            match boundary {
                crate::reason::refute::FragmentBoundary::SourceAdmission { issue, .. } => {
                    match issue {
                        crate::reason::refute::RefutationSourceIssue::IncompleteList { .. } => {
                            !has_list_writer
                        }
                        _ => {
                            issue.refusal_class()
                                == crate::reason::refute::ClassSourceRefusal::Invalid
                        }
                    }
                }
                crate::reason::refute::FragmentBoundary::Combined(boundaries) => boundaries
                    .iter()
                    .any(|boundary| hard_source_refusal(boundary, has_list_writer)),
                _ => true,
            }
        }
        let source_refused = class_admission
            .selected_worlds
            .iter()
            .any(|(world, selected)| {
                let Some(refusal) = &selected.refusal else {
                    return false;
                };
                let effects = &self.world_effects[world];
                let has_list_writer = [
                    "http://www.w3.org/1999/02/22-rdf-syntax-ns#first",
                    "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest",
                ]
                .into_iter()
                .any(|predicate| {
                    self.flow
                        .overlapping_writer(
                            &StatementPattern::relation(Some(predicate), None),
                            effects,
                        )
                        .is_some()
                });
                hard_source_refusal(refusal, has_list_writer)
            });
        if !source_refused {
            for (world, facts) in self.facts {
                for fact in facts {
                    if !matches!(
                        fact.subject,
                        purrdf::TermValue::Iri(_) | purrdf::TermValue::Blank { .. }
                    ) {
                        return Err(gmeow_errors::Diag::of_kind(crate::error::NativeCoverage {
                            profile: "native-resource-subject-v1".to_owned(), source: crate::provenance::term_display(&fact.subject), world: world.clone(),
                            detail: "selected forward row has a non-resource subject outside this result contract".to_owned(),
                        }));
                    }
                }
            }
        }
        let input_contract = crate::physical::metadata_identity(
            "gmeow-native-exact-input-v1",
            &(
                self.template.identity,
                &self.template.domains,
                graphs,
                sources,
                crate::reason::refute::native::NativeSourceTerms::RdfSkolem,
            ),
        );
        Ok(NativeInputBinding {
            template: self.template.identity,
            world_effects: &self.world_effects,
            flow: &self.flow,
            sources,
            graphs,
            domains: &self.template.domains,
            input_contract,
            shape: self.identity,
            classes,
            class_admission,
            source_refused,
            modal,
            contextual,
        })
    }

    pub(crate) fn template(&self) -> Arc<JointTemplate> {
        Arc::clone(&self.template)
    }

    pub(crate) fn identity(&self) -> &[u8; 32] {
        &self.identity
    }
    pub(crate) fn cacheable(&self) -> bool {
        self.template.cacheable()
    }

    pub(crate) fn prepare(&self) -> gmeow_errors::Result<NativeOutcome<Arc<JointProgram>>> {
        let template = &self.template;
        let observations: Vec<_> = self
            .facts
            .keys()
            .flat_map(|world| {
                template.family_reads.iter().map(move |read| {
                    crate::physical::effects::WorldStatementObservation {
                        world: world.clone(),
                        pattern: self.flow.ranged_pattern(StatementPattern::relation(
                            read.predicate.as_deref(),
                            read.marker.as_deref(),
                        )),
                    }
                })
            })
            .collect();
        let scoped: Vec<_> = self
            .world_effects
            .iter()
            .flat_map(|(world, effects)| {
                effects.iter().cloned().map(move |effect| {
                    crate::physical::effects::WorldProducerEffect::local(world, effect)
                })
            })
            .collect();
        let predicates = self
            .facts
            .iter()
            .map(|(world, facts)| {
                (
                    world.clone(),
                    facts.iter().map(|fact| fact.predicate.clone()).collect(),
                )
            })
            .collect();
        let Ok(world_schedule) = crate::physical::effects::schedule_worlds(
            &scoped,
            template.semantics,
            &predicates,
            &observations,
        ) else {
            return Ok(NativeOutcome::Unsupported(
                super::UnsupportedKind::NonStratifiable,
            ));
        };
        let schedule = if let Some(world) = self.facts.keys().next() {
            world_schedule.local(world, 0..self.effects.len(), 0..template.family_reads.len())
        } else {
            crate::physical::effects::schedule_observed(
                &self.effects,
                template.semantics,
                &Default::default(),
                &[],
            )
            .map_err(|_| seminaive_err("empty relational operation has cyclic native effects"))?
        };
        let mut admission = template.admission.clone();
        if let (Some(termination), Some(evidence)) = (&template.termination, &self.evidence)
            && let Some(refined) = termination.certify(
                evidence,
                &self.certificate_flow,
                self.facts,
                &self.possible,
                &self.contextual_effects,
                template.semantics,
            )?
            && refined.admits_native()
        {
            admission = refined;
        }
        let mut program = JointProgram::from_schedule(
            &template.rules,
            &template.producers,
            &template.properties,
            &template.families,
            &template.family_reads,
            template.operation,
            template.semantics,
            schedule,
            Arc::clone(&self.effects),
            Some(self.identity),
            admission,
        );
        program.witness_contract = template.witness_contract;
        Ok(NativeOutcome::Decided(Arc::new(program)))
    }
}

impl JointProgram {
    /// Execute one selected native run with original sources and one governor/registry.
    pub(crate) fn materialize_input_governed(
        &self,
        input: &JointInput<'_>,
        binding: NativeInputBinding<'_>,
        governor: &mut super::StepGovernor,
        registry: &mut crate::physical::store::SkolemRegistry,
    ) -> gmeow_errors::Result<NativeOutcome<JointMaterialization>> {
        if self.source_contract != Some(input.identity)
            || binding.shape != input.identity
            || self.operation == super::JointOperation::Relational
        {
            return Err(seminaive_err(
                "joint schedule does not admit this native input summary",
            ));
        }
        self.materialize_worlds_governed(
            input.facts.iter().map(|(world, facts)| {
                Ok((world.as_str(), std::borrow::Cow::Borrowed(facts.as_slice())))
            }),
            governor,
            registry,
            Some(binding),
            None,
        )
    }

    /// Execute an explicit retained transaction through the same admitted writer graph.
    pub(crate) fn materialize_input_retained(
        &self,
        input: &JointInput<'_>,
        binding: NativeInputBinding<'_>,
        governor: &mut super::StepGovernor,
        registry: &mut crate::physical::store::SkolemRegistry,
        retained: &mut super::RetainedJoint,
    ) -> gmeow_errors::Result<NativeOutcome<JointMaterialization>> {
        if self.source_contract != Some(input.identity)
            || binding.shape != input.identity
            || self.operation == super::JointOperation::Relational
        {
            return Err(seminaive_err(
                "retained schedule does not admit this exact native input",
            ));
        }
        self.materialize_worlds_governed(
            input.facts.iter().map(|(world, facts)| {
                Ok((world.as_str(), std::borrow::Cow::Borrowed(facts.as_slice())))
            }),
            governor,
            registry,
            Some(binding),
            Some(retained),
        )
    }

    /// The sole execution entry for a source-bound schedule. Its input remains
    /// borrowed from the exact summary constructor throughout admission and use.
    pub(crate) fn materialize_input(
        &self,
        input: &JointInput<'_>,
        max_steps: Option<u64>,
    ) -> gmeow_errors::Result<NativeOutcome<JointMaterialization>> {
        if self.source_contract != Some(input.identity)
            || self.operation != super::JointOperation::Relational
        {
            return Err(seminaive_err(
                "joint schedule does not admit this native input summary",
            ));
        }
        self.materialize_native_input(input.facts, max_steps)
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod contextual_tests;
