// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared native lowering for the program-driven reasoning and materialization paths.
//!
//! A preparation contains executable rules and their complete lowering residue. Dataset
//! selection and execution borrow the same preparation; neither reparses a rule language.
//! The process-local cache retains no datasets, results, source programs or serialized
//! carriers. Its full typed-program key includes scopes, provenance and every IR family.
//! Preparation reuse is not a coverage certificate or permission to erase scope: admission
//! and execution of each selected context remain obligations of the consuming operation.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, OnceLock};

use gmeow_logic_compile::ir::LogicProgram;

use crate::native_semantics::SourceAdmissionProfile;
use crate::physical::{
    ExistentialRule, JointInput, JointProgram, JointTemplate, NativeOutcome, PreparedPropertyRule,
};
use crate::result::PreservationClaim;
use crate::rule_ir::EvalRule;

/// An immutable preparation shared by predicate selection and native execution.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct PreparedProgram {
    /// Complete typed-program identity, computed once before cache lookup.
    /// Includes source context and IR families not present in executable rules.
    pub(crate) identity: [u8; 32],
    /// Source ownership and scope captured before relational lowering.
    pub(crate) admission: crate::native_semantics::ProgramAdmission,
    /// Ordinary Horn rules, followed by the evaluable full-formula rules.
    pub(crate) rules: Vec<EvalRule>,
    /// Conjunctive existential heads, carried to the restricted chase unchanged.
    pub(crate) existential_rules: Vec<ExistentialRule>,
    /// Every formula that could not be lowered, never inferred from cache success.
    pub(crate) preservation: PreservationClaim,
    /// Each compiler-produced rule identity, with the indices into the program's
    /// formulas of the authored formulas it was clausified from.
    pub(crate) formula_rules: std::collections::BTreeMap<String, std::collections::BTreeSet<usize>>,
    /// Exact source owner of each selected denial rule, available only to live proof execution.
    #[serde(skip)]
    pub(crate) denial_owners: BTreeMap<String, String>,
    /// Formula indices whose selected lowering left a preservation boundary.
    #[serde(skip)]
    pub(crate) incomplete_formulas: BTreeSet<usize>,
    /// No datasets or run state; shared joint dependency and witness layout only.
    #[serde(skip)]
    joint: Mutex<Option<NativeOutcome<Arc<JointProgram>>>>,
    #[serde(skip)]
    reasoning_rules: std::sync::OnceLock<Vec<EvalRule>>,
    #[serde(skip)]
    schema_plans: Mutex<SchemaPlans>,
    #[serde(skip)]
    schema_templates: Mutex<SchemaTemplates>,
}

const MAX_SCHEMA_PLANS: usize = 4;
type SchemaPlans = SchemaCache<NativeOutcome<Arc<JointProgram>>>;
type SchemaTemplates = SchemaCache<Arc<JointTemplate>>;

/// Bounded immutable templates and source-bound plans. No facts or results
/// or input summaries are retained. Keys are fixed-size digests; retained source and
/// domain metadata is subject to the template's independent byte bound.
#[derive(Debug)]
struct SchemaCache<T> {
    entries: VecDeque<([u8; 32], T)>,
}

impl<T> Default for SchemaCache<T> {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
        }
    }
}

impl<T: Clone> SchemaCache<T> {
    fn get(&mut self, key: &[u8; 32]) -> Option<T> {
        let position = self
            .entries
            .iter()
            .position(|(candidate, _)| candidate == key)?;
        let entry = self.entries.remove(position)?;
        let result = entry.1.clone();
        self.entries.push_back(entry);
        Some(result)
    }

    fn insert(&mut self, key: [u8; 32], plan: T) -> T {
        if let Some(existing) = self.get(&key) {
            return existing;
        }
        if self.entries.len() == MAX_SCHEMA_PLANS {
            self.entries.pop_front();
        }
        self.entries.push_back((key, plan.clone()));
        plan
    }
}

/// Cache limits bound both the inventory and each entry's structural/payload size.
const MAX_ENTRIES: usize = 64;
const MAX_RULES: usize = 1024;
const MAX_ATOMS: usize = 16 * 1024;
const MAX_RENDERED_BYTES: usize = 1024 * 1024;

/// A digest sink: CBOR is streamed into the key, never retained or parsed again.
/// Typed framing distinguishes absent fields, strings containing separators and floats.
#[derive(Default)]
struct ProgramDigest {
    hasher: purrdf_hash::blake3::Hasher,
}

impl std::io::Write for ProgramDigest {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.hasher.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Count a bounded debug rendering without allocating its strings. Besides the atom
/// count, this bounds the retained strings, guards and residue payloads independently of
/// source size: formula expansion can be much larger than its source.
#[derive(Default)]
struct PayloadLimit(usize);

impl std::fmt::Write for PayloadLimit {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        self.0 = self.0.saturating_add(value.len());
        if self.0 > MAX_RENDERED_BYTES {
            return Err(std::fmt::Error);
        }
        Ok(())
    }
}

impl PreparedProgram {
    /// Copy only immutable native lowering into a separately owned publication.
    /// Execution caches are process-local and never serialized into a preparation.
    pub(crate) fn publication(&self) -> Self {
        Self {
            identity: self.identity,
            admission: self.admission.clone(),
            rules: self.rules.clone(),
            existential_rules: self.existential_rules.clone(),
            preservation: self.preservation.clone(),
            formula_rules: self.formula_rules.clone(),
            denial_owners: self.denial_owners.clone(),
            incomplete_formulas: self.incomplete_formulas.clone(),
            joint: Mutex::new(None),
            reasoning_rules: OnceLock::new(),
            schema_plans: Mutex::new(SchemaPlans::default()),
            schema_templates: Mutex::new(SchemaTemplates::default()),
        }
    }

    fn lower(
        program: &LogicProgram,
        identity: [u8; 32],
        profile: SourceAdmissionProfile,
        selected: Option<gmeow_logic_compile::source_inventory::SelectedProgramView<'_, '_>>,
    ) -> gmeow_errors::Result<Self> {
        let source_proof = matches!(
            profile,
            SourceAdmissionProfile::SelectedSourceProofV2 { .. }
        );
        let (admission, lowering, mut rules) = if let Some(view) = selected {
            (
                crate::native_semantics::ProgramAdmission::capture_selected(
                    view,
                    profile,
                    program.source_iri.clone(),
                ),
                crate::relational_core::lower_indexed_formulas(view.formulas()),
                view.rules()
                    .map(|(_, rule)| crate::lower::lower_rule(rule))
                    .collect::<gmeow_errors::Result<Vec<_>>>()?,
            )
        } else {
            (
                crate::native_semantics::ProgramAdmission::capture(program, profile),
                crate::relational_core::lower_formulas(program),
                crate::lower::prepare_rule_templates(program)?,
            )
        };
        rules.extend(lowering.rules);
        let mut denial_owners = BTreeMap::new();
        if let Some(view) = selected.filter(|_| source_proof)
            && let mut denials = view.forbidden_patterns().peekable()
            && denials.peek().is_some()
        {
            let mut rule_ids: BTreeSet<_> =
                rules.iter().map(|rule| rule.rule_iri.clone()).collect();
            rule_ids.extend(
                lowering
                    .existential_rules
                    .iter()
                    .map(|rule| rule.rule_iri.clone()),
            );
            rule_ids.extend(
                crate::reason::dl::structured_dl_rules()
                    .into_iter()
                    .map(|rule| rule.rule_iri),
            );
            for (_, constraint) in denials {
                // Unsupported denials remain source boundaries in NativeSourceAdmission.
                if let Ok(rule) = crate::reason::lower_forbidden_pattern(constraint) {
                    if !rule_ids.insert(rule.rule_iri.clone()) {
                        return Err(gmeow_errors::Diag::of_kind(crate::error::Lower {
                            detail: format!(
                                "forbidden pattern {} shares proof rule identity {} with a selected rule",
                                constraint.iri, rule.rule_iri
                            ),
                        }));
                    }
                    denial_owners.insert(rule.rule_iri.clone(), constraint.iri.clone());
                    rules.push(rule);
                }
            }
        }
        Ok(Self {
            identity,
            admission,
            rules,
            existential_rules: lowering.existential_rules,
            preservation: lowering.preservation,
            formula_rules: lowering.formula_rules,
            denial_owners,
            incomplete_formulas: if selected.is_some() {
                lowering.incomplete_formulas
            } else {
                BTreeSet::new()
            },
            joint: Mutex::new(None),
            reasoning_rules: std::sync::OnceLock::new(),
            schema_plans: Mutex::new(SchemaPlans::default()),
            schema_templates: Mutex::new(SchemaTemplates::default()),
        })
    }

    /// Share the ordinary/existential execution plan across selected stores.
    pub(crate) fn joint_program(&self) -> gmeow_errors::Result<NativeOutcome<Arc<JointProgram>>> {
        self.prepare_joint(
            &self.joint,
            || self.rules.clone(),
            crate::native_semantics::SemanticVocabulary::Exact,
        )
    }

    pub(crate) fn reasoning_rules(&self) -> &[EvalRule] {
        self.reasoning_rules.get_or_init(|| {
            let mut rules = crate::reason::dl::structured_dl_rules();
            rules.extend(self.rules.iter().cloned());
            rules
        })
    }

    /// Prepare the complete abstract input using a shared native producer template.
    /// The immutable borrow remains live through source-bound execution.
    pub(crate) fn reasoning_input<'a>(
        &self,
        properties: &[PreparedPropertyRule],
        sources: &crate::reason::source_existentials::PreparedSources,
        domains: &crate::physical::SelectedDomains,
        facts: &'a std::collections::BTreeMap<String, Vec<crate::rule_ir::Fact>>,
        possible: Arc<[(String, crate::rule_ir::Fact)]>,
        contextual_effects: &[crate::physical::WorldProducerEffect],
    ) -> gmeow_errors::Result<JointInput<'a>> {
        let input_facts = facts.values().map(Vec::len).sum::<usize>();
        let trace_large_input = input_facts >= 100_000;
        self.admission.admit()?;
        // RDF existential definitions arrive after compiler rule preparation.
        // Their execution identities must remain distinct from denial owners.
        if !self.denial_owners.is_empty()
            && let Some((source, owner)) = sources.rules.iter().find_map(|source| {
                self.denial_owners
                    .get(&source.rule.rule_iri)
                    .map(|owner| (source, owner))
            })
        {
            return Err(crate::reason::source_existentials::refusal(
                &crate::provenance::term_display(&source.source),
                &source.world,
                format!(
                    "rule identity {} belongs to selected forbidden pattern {owner}",
                    source.rule.rule_iri
                ),
            ));
        }
        let key = crate::physical::metadata_identity(
            "gmeow-schema-source-domain-selection-v2",
            &(
                crate::physical::schema_identity(properties),
                sources.identity(),
                domains,
            ),
        );
        let lock_error = |_| {
            gmeow_errors::Diag::of_kind(crate::error::Physical {
                detail: "schema template lock poisoned".to_owned(),
            })
        };
        // Retain the immutable template, not the cache guard, across observation
        // and input analysis. Other requests must remain able to use the cache.
        let cached = self.schema_templates.lock().map_err(lock_error)?.get(&key);
        if let Some(template) = cached {
            if trace_large_input {
                tracing::info!(
                    target: "pipeline_reasoning_detail",
                    phase = "reuse-native-reasoning-template",
                    input_facts,
                    "large native reasoning input",
                );
            }
            return template.input(facts, possible, contextual_effects);
        }
        let template_started = crate::timing::Instant::now();
        if trace_large_input {
            tracing::info!(
                target: "pipeline_reasoning_detail",
                phase = "build-native-reasoning-template",
                event = "start",
                input_facts,
                properties = properties.len(),
                source_rules = sources.rules.len(),
                "large native reasoning input",
            );
        }
        let template = Arc::new(
            JointTemplate::with_sources(
                self.reasoning_rules(),
                &self.existential_rules,
                properties,
                crate::native_semantics::SemanticVocabulary::GroundedLogicV1,
                &sources.rules,
                Some(sources.contract()),
                domains,
            )?
            .with_witness_source(&self.admission, self.identity, self.rules.len()),
        );
        if trace_large_input {
            tracing::info!(
                target: "pipeline_reasoning_detail",
                phase = "build-native-reasoning-template",
                event = "end",
                elapsed_ms = template_started.elapsed().as_millis(),
                "large native reasoning input",
            );
        }
        let cacheable = self.cacheable() && template.cacheable();
        let template = if cacheable {
            self.schema_templates
                .lock()
                .map_err(lock_error)?
                .insert(key, template)
        } else {
            template
        };
        template.input(facts, possible, contextual_effects)
    }

    /// Reuse a schedule only for the same complete input abstraction and template.
    /// Changed values can alter dependencies even when predicate names are identical.
    pub(crate) fn reasoning_schema_program(
        &self,
        input: &JointInput<'_>,
    ) -> gmeow_errors::Result<NativeOutcome<Arc<JointProgram>>> {
        self.admission.admit()?;
        let key = *input.identity();
        let lock_error = |_| {
            gmeow_errors::Diag::of_kind(crate::error::Physical {
                detail: "schema preparation lock poisoned".to_owned(),
            })
        };
        if let Some(plan) = self.schema_plans.lock().map_err(lock_error)?.get(&key) {
            return Ok(plan);
        }
        let plan = input.prepare()?;
        if !self.cacheable() || !input.cacheable() {
            return Ok(plan);
        }
        Ok(self
            .schema_plans
            .lock()
            .map_err(lock_error)?
            .insert(key, plan))
    }

    fn prepare_joint(
        &self,
        cache: &Mutex<Option<NativeOutcome<Arc<JointProgram>>>>,
        rules: impl FnOnce() -> Vec<EvalRule>,
        semantics: crate::native_semantics::SemanticVocabulary,
    ) -> gmeow_errors::Result<NativeOutcome<Arc<JointProgram>>> {
        self.admission.admit()?;
        let mut cached = cache.lock().map_err(|_| {
            gmeow_errors::Diag::of_kind(crate::error::Physical {
                detail: "joint preparation lock poisoned".to_owned(),
            })
        })?;
        if let Some(prepared) = cached.as_ref() {
            return Ok(prepared.clone());
        }
        let prepared = match JointProgram::prepare_with_semantics(
            &rules(),
            &self.existential_rules,
            &[],
            &std::collections::BTreeSet::new(),
            semantics,
        )? {
            NativeOutcome::Decided(program) => {
                NativeOutcome::Decided(Arc::new(program.with_witness_source(&self.admission)))
            }
            NativeOutcome::Unsupported(kind) => NativeOutcome::Unsupported(kind),
        };
        *cached = Some(prepared.clone());
        Ok(prepared)
    }

    fn cacheable(&self) -> bool {
        if self
            .rules
            .len()
            .saturating_add(self.existential_rules.len())
            > MAX_RULES
        {
            return false;
        }
        let atoms = self
            .rules
            .iter()
            .map(|rule| {
                rule.body
                    .len()
                    .saturating_add(1)
                    .saturating_add(rule.numeric.len())
            })
            .chain(self.existential_rules.iter().map(|rule| {
                rule.body
                    .len()
                    .saturating_add(rule.head.len())
                    .saturating_add(rule.numeric.len())
            }))
            .fold(0usize, usize::saturating_add);
        atoms <= MAX_ATOMS && write!(PayloadLimit::default(), "{self:?}").is_ok()
    }
}

/// Least-recently-used preparations. The lock protects only lookup/publication;
/// expensive misses lower outside it so independent programs do not serialize.
#[derive(Default)]
struct PreparationCache {
    entries: VecDeque<([u8; 32], Arc<PreparedProgram>)>,
}

impl PreparationCache {
    fn get(&mut self, key: &[u8; 32]) -> Option<Arc<PreparedProgram>> {
        let position = self
            .entries
            .iter()
            .position(|(candidate, _)| candidate == key)?;
        let entry = self.entries.remove(position)?;
        let prepared = Arc::clone(&entry.1);
        self.entries.push_back(entry);
        Some(prepared)
    }

    fn insert(&mut self, key: [u8; 32], prepared: Arc<PreparedProgram>) -> Arc<PreparedProgram> {
        if let Some(existing) = self.get(&key) {
            return existing;
        }
        if self.entries.len() == MAX_ENTRIES {
            self.entries.pop_front();
        }
        self.entries.push_back((key, Arc::clone(&prepared)));
        prepared
    }
}

/// Prepare once per retained program identity. An evicted or oversized entry simply
/// recomputes the same complete lowering; cache policy never selects a weaker program.
pub(crate) fn prepare_program(
    program: &LogicProgram,
) -> gmeow_errors::Result<Arc<PreparedProgram>> {
    prepare_program_under(program, SourceAdmissionProfile::WorldLocalTemplatesV1)
}

/// Prepare under an explicit source admission profile. The profile is part of the
/// preparation identity, so a program prepared for one profile is never served to a
/// request for another.
pub(crate) fn prepare_program_under(
    program: &LogicProgram,
    profile: SourceAdmissionProfile,
) -> gmeow_errors::Result<Arc<PreparedProgram>> {
    if !matches!(profile, SourceAdmissionProfile::WorldLocalTemplatesV1) {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Lower {
            detail: "resolved theory preparation requires the compiler-issued program view".into(),
        }));
    }
    prepare_program_view(program, profile, None)
}

pub(crate) fn prepare_selected_program(
    inventory: &gmeow_logic_compile::source_inventory::SourceInventory<'_>,
    selection: &gmeow_logic_compile::source_inventory::ResolvedTheorySelection,
    profile: SourceAdmissionProfile,
) -> gmeow_errors::Result<Arc<PreparedProgram>> {
    let view = selection.program_view(inventory)?;
    prepare_program_view(inventory.theory().program(), profile, Some(view))
}

pub(crate) fn selected_profile(
    selection: &gmeow_logic_compile::source_inventory::ResolvedTheorySelection,
) -> SourceAdmissionProfile {
    SourceAdmissionProfile::ResolvedTheoryV1 {
        theory: selection.request().clone(),
        execution_identity: selection.observation().identity.clone(),
    }
}

pub(crate) fn selected_source_proof_profile(
    inventory: &gmeow_logic_compile::source_inventory::SourceInventory<'_>,
    selection: &gmeow_logic_compile::source_inventory::ResolvedTheorySelection,
) -> SourceAdmissionProfile {
    SourceAdmissionProfile::SelectedSourceProofV2 {
        theory: selection.request().clone(),
        execution_identity: selection.observation().identity.clone(),
        compiler_trace: inventory.compiler_trace_digest().to_owned(),
    }
}

fn prepare_program_view(
    program: &LogicProgram,
    profile: SourceAdmissionProfile,
    selected: Option<gmeow_logic_compile::source_inventory::SelectedProgramView<'_, '_>>,
) -> gmeow_errors::Result<Arc<PreparedProgram>> {
    static CACHE: OnceLock<Mutex<PreparationCache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(PreparationCache::default()));
    let key = preparation_identity(program, &profile)?;
    if let Some(prepared) = cache
        .lock()
        .expect("native program cache poisoned")
        .get(&key)
    {
        return Ok(prepared);
    }
    let prepared = Arc::new(PreparedProgram::lower(program, key, profile, selected)?);
    if !prepared.cacheable() {
        return Ok(prepared);
    }
    Ok(cache
        .lock()
        .expect("native program cache poisoned")
        .insert(key, prepared))
}

/// The identity [`prepare_program_under`] gives the preparation of `program` under
/// `profile`: the digest of the complete typed program and the profile. A preparation
/// carries it as [`PreparedProgram::identity`], so a caller can tell whether a preparation
/// was made from a given program and profile.
pub(crate) fn preparation_identity(
    program: &LogicProgram,
    profile: &SourceAdmissionProfile,
) -> gmeow_errors::Result<[u8; 32]> {
    let unidentified = |error: &dyn std::fmt::Display| {
        gmeow_errors::Diag::of_kind(crate::error::Lower {
            detail: format!("cannot identify the complete native program: {error}"),
        })
    };
    let mut digest = ProgramDigest::default();
    digest
        .hasher
        .update(b"gmeow-native-program-preparation-v1\0");
    ciborium::ser::into_writer(program, &mut digest).map_err(|error| unidentified(&error))?;
    // The program is one self-delimiting item, so a profile written after it cannot
    // be mistaken for part of any program. The world-local profile writes nothing
    // and so keeps the identity it has always had.
    match profile {
        SourceAdmissionProfile::WorldLocalTemplatesV1 => {}
        SourceAdmissionProfile::ResolvedTheoryV1 { .. }
        | SourceAdmissionProfile::SelectedSourceProofV2 { .. } => {
            ciborium::ser::into_writer(profile, &mut digest)
                .map_err(|error| unidentified(&error))?;
        }
    }
    Ok(*digest.hasher.finalize().as_bytes())
}

#[path = "program_analysis.tests.rs"]
#[cfg(test)]
mod tests;
