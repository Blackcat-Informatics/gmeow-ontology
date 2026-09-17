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

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, OnceLock};

use gmeow_logic_compile::ir::LogicProgram;

use crate::physical::{
    ExistentialRule, JointInput, JointProgram, JointTemplate, NativeOutcome, PreparedPropertyRule,
};
use crate::result::PreservationClaim;
use crate::rule_ir::EvalRule;

/// An immutable preparation shared by predicate selection and native execution.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct PreparedProgram {
    /// Source ownership and scope captured before relational lowering.
    pub(crate) admission: crate::native_semantics::ProgramAdmission,
    /// Ordinary Horn rules, followed by the evaluable full-formula rules.
    pub(crate) rules: Vec<EvalRule>,
    /// Conjunctive existential heads, carried to the restricted chase unchanged.
    pub(crate) existential_rules: Vec<ExistentialRule>,
    /// Every formula that could not be lowered, never inferred from cache success.
    pub(crate) preservation: PreservationClaim,
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
    hasher: blake3::Hasher,
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
            admission: self.admission.clone(),
            rules: self.rules.clone(),
            existential_rules: self.existential_rules.clone(),
            preservation: self.preservation.clone(),
            joint: Mutex::new(None),
            reasoning_rules: OnceLock::new(),
            schema_plans: Mutex::new(SchemaPlans::default()),
            schema_templates: Mutex::new(SchemaTemplates::default()),
        }
    }

    fn lower(program: &LogicProgram) -> gmeow_errors::Result<Self> {
        let admission = crate::native_semantics::ProgramAdmission::capture(program);
        let lowering = crate::relational_core::lower_formulas(program);
        let mut rules = crate::lower::prepare_rule_templates(program)?;
        rules.extend(lowering.rules);
        Ok(Self {
            admission,
            rules,
            existential_rules: lowering.existential_rules,
            preservation: lowering.preservation,
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
        self.admission.admit_world_local_template()?;
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
        if let Some(template) = self.schema_templates.lock().map_err(lock_error)?.get(&key) {
            return template.input(facts, possible, contextual_effects);
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
            .with_witness_source(&self.admission),
        );
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
        self.admission.admit_world_local_template()?;
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
        self.admission.admit_world_local_template()?;
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
    static CACHE: OnceLock<Mutex<PreparationCache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(PreparationCache::default()));
    let mut digest = ProgramDigest::default();
    digest
        .hasher
        .update(b"gmeow-native-program-preparation-v1\0");
    ciborium::ser::into_writer(program, &mut digest).map_err(|error| {
        gmeow_errors::Diag::of_kind(crate::error::Lower {
            detail: format!("cannot identify the complete native program: {error}"),
        })
    })?;
    let key = *digest.hasher.finalize().as_bytes();
    if let Some(prepared) = cache
        .lock()
        .expect("native program cache poisoned")
        .get(&key)
    {
        return Ok(prepared);
    }
    let prepared = Arc::new(PreparedProgram::lower(program)?);
    if !prepared.cacheable() {
        return Ok(prepared);
    }
    Ok(cache
        .lock()
        .expect("native program cache poisoned")
        .insert(key, prepared))
}

#[path = "program_analysis.tests.rs"]
#[cfg(test)]
mod tests;
