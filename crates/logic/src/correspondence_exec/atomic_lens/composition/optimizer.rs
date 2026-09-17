// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Certified physical planning for native atomic correspondence compositions.
//!
//! The optimizer is deliberately narrower than the correspondence calculus. It
//! recognizes the closed atomic-property fragment, saturates one typed root
//! equivalence class under a fixed rule catalogue, and extracts a deterministic
//! physical plan by structural cost. The certificate is checked again without
//! trusting the search. Bounded recovery cases and sampled law verdicts are not
//! inputs, so they cannot authorize a rewrite.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use purrdf::{CompositeDatasetView, DatasetView, RdfDataset, ViewLimits};
use serde::{Deserialize, Serialize};

use super::{AtomicComposition, AtomicCompositionState};
use crate::correspondence_exec::atomic_lens::{AtomicLensState, AtomicPropertyLens};
use crate::correspondence_exec::exec_error;
use crate::correspondence_exec::stable_digest::StableDigest;

#[cfg(test)]
mod tests;

const CERTIFICATE_VERSION: &str = "gmeow-atomic-correspondence-plan-v1";

/// Exact semantic identities outside the physical syntax of an atomic plan.
/// Every value is a lower-case BLAKE3 digest produced by the owning compiler or
/// analysis. They remain separate so a context or effect change cannot hide in
/// a single opaque cache key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtomicOptimizationScope {
    pub program_digest: String,
    pub source_theory_digest: String,
    pub view_theory_digest: String,
    pub context_digest: String,
    pub effect_digest: String,
    pub complement_digest: String,
    pub preservation_digest: String,
    pub reasoning_contract_digest: String,
}

impl AtomicOptimizationScope {
    /// Admit the eight independent certificate axes.
    ///
    /// # Errors
    /// Refuses anything other than a canonical 32-byte hexadecimal digest.
    pub fn new(values: [&str; 8]) -> gmeow_errors::Result<Self> {
        for value in values {
            if value.len() != 64
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(exec_error(
                    "atomic optimization scope requires eight lower-case 64-digit digests",
                ));
            }
        }
        Ok(Self {
            program_digest: values[0].to_owned(),
            source_theory_digest: values[1].to_owned(),
            view_theory_digest: values[2].to_owned(),
            context_digest: values[3].to_owned(),
            effect_digest: values[4].to_owned(),
            complement_digest: values[5].to_owned(),
            preservation_digest: values[6].to_owned(),
            reasoning_contract_digest: values[7].to_owned(),
        })
    }
}

/// Deterministic optimizer bounds. Exhaustion selects the original plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtomicOptimizerLimits {
    pub max_eclasses: usize,
    pub max_enodes: usize,
    pub max_rule_applications: usize,
}

impl Default for AtomicOptimizerLimits {
    fn default() -> Self {
        Self {
            max_eclasses: 4_096,
            max_enodes: 8_192,
            max_rule_applications: 16_384,
        }
    }
}

/// Stable copy of PurRDF's five independent view-retention ceilings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AtomicViewLimits {
    pub sources: usize,
    pub terms: usize,
    pub rows: usize,
    pub payload_bytes: usize,
    pub auxiliary_bytes: usize,
}

impl From<ViewLimits> for AtomicViewLimits {
    fn from(value: ViewLimits) -> Self {
        Self {
            sources: value.max_sources,
            terms: value.max_terms,
            rows: value.max_rows,
            payload_bytes: value.max_payload_bytes,
            auxiliary_bytes: value.max_auxiliary_bytes,
        }
    }
}

impl From<AtomicViewLimits> for ViewLimits {
    fn from(value: AtomicViewLimits) -> Self {
        Self {
            max_sources: value.sources,
            max_terms: value.terms,
            max_rows: value.rows,
            max_payload_bytes: value.payload_bytes,
            max_auxiliary_bytes: value.auxiliary_bytes,
        }
    }
}

/// Complete executable identity of one original atomic stage.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AtomicStageDescriptor {
    pub source_predicate: String,
    pub view_predicate: String,
    pub inverse: bool,
    pub limits: AtomicViewLimits,
}

impl AtomicStageDescriptor {
    fn from_lens(lens: &AtomicPropertyLens) -> Self {
        Self {
            source_predicate: lens.source_predicate.clone(),
            view_predicate: lens.view_predicate.clone(),
            inverse: lens.inverse,
            limits: lens.limits.into(),
        }
    }

    fn key(&self) -> String {
        format!(
            "{}>{};inverse={};limits={},{},{},{},{}",
            self.source_predicate,
            self.view_predicate,
            self.inverse,
            self.limits.sources,
            self.limits.terms,
            self.limits.rows,
            self.limits.payload_bytes,
            self.limits.auxiliary_bytes,
        )
    }

    fn digest(&self) -> String {
        digest("gmeow-atomic-stage-v1", [self.key().as_str()])
    }

    fn lens(&self) -> gmeow_errors::Result<AtomicPropertyLens> {
        AtomicPropertyLens::new(
            &self.source_predicate,
            &self.view_predicate,
            self.inverse,
            self.limits.into(),
        )
    }
}

/// The physical term extracted from the saturated root class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AtomicPhysicalPlan {
    /// Execute the authored sequence without a physical rewrite.
    Original,
    /// One native focus operation with all original admissions retained by the
    /// execution preflight and logical-stage receipts.
    Fused { stage: AtomicStageDescriptor },
}

impl AtomicPhysicalPlan {
    fn key(&self) -> String {
        match self {
            Self::Original => "original".to_owned(),
            Self::Fused { stage } => format!("fused:{}", stage.key()),
        }
    }
}

/// Fixed native-checkable rewrite catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AtomicRewriteRule {
    IdentityElimination,
    Reassociation,
    CompiledCommonSubexpression,
    PredicateProjectionPushdown,
    CompositionFusion,
    WitnessedGetPutCancellation,
}

impl AtomicRewriteRule {
    pub fn id(self) -> &'static str {
        match self {
            Self::IdentityElimination => "atomic-identity-elimination-v1",
            Self::Reassociation => "atomic-reassociation-v1",
            Self::CompiledCommonSubexpression => "atomic-compiled-cse-v1",
            Self::PredicateProjectionPushdown => "atomic-focus-pushdown-v1",
            Self::CompositionFusion => "atomic-composition-fusion-v1",
            Self::WitnessedGetPutCancellation => "atomic-get-put-cancellation-v1",
        }
    }

    pub fn theorem(self) -> &'static str {
        match self {
            Self::IdentityElimination => {
                "an orientation-preserving p-to-p focus is the identity while its admission receipt remains required"
            }
            Self::Reassociation => {
                "typed atomic composition is associative when every adjacent predicate boundary matches"
            }
            Self::CompiledCommonSubexpression => {
                "equal immutable stage descriptors may share compiled representation, never result state"
            }
            Self::PredicateProjectionPushdown => {
                "an atomic focus owns one ordinary predicate and an immutable complete residual"
            }
            Self::CompositionFusion => {
                "a matched atomic rename chain equals one endpoint rename with XOR orientation under complete runtime preflight"
            }
            Self::WitnessedGetPutCancellation => {
                "an unchanged augmented atomic view restores its exact retained residual and focus"
            }
        }
    }
}

const RULES: [AtomicRewriteRule; 6] = [
    AtomicRewriteRule::IdentityElimination,
    AtomicRewriteRule::Reassociation,
    AtomicRewriteRule::CompiledCommonSubexpression,
    AtomicRewriteRule::PredicateProjectionPushdown,
    AtomicRewriteRule::CompositionFusion,
    AtomicRewriteRule::WitnessedGetPutCancellation,
];

fn rule_catalog_digest() -> String {
    digest(
        "gmeow-atomic-rule-catalog-v1",
        RULES.iter().flat_map(|rule| [rule.id(), rule.theorem()]),
    )
}

/// One checked rule application in deterministic catalogue order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtomicRuleApplication {
    pub rule: AtomicRewriteRule,
    pub theorem: String,
    pub stages: Vec<usize>,
}

/// Checks retained for every authored logical stage even when its physical
/// focus publication is fused away.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AtomicStageCheck {
    BoundaryType,
    PredicateAdmission,
    InversionSafety,
    GraphCatalogue,
    RetentionLimits,
    ComplementIdentity,
}

impl AtomicStageCheck {
    fn id(self) -> &'static str {
        match self {
            Self::BoundaryType => "boundary-type-v1",
            Self::PredicateAdmission => "predicate-admission-v1",
            Self::InversionSafety => "inversion-safety-v1",
            Self::GraphCatalogue => "graph-catalogue-v1",
            Self::RetentionLimits => "retention-limits-v1",
            Self::ComplementIdentity => "complement-identity-v1",
        }
    }
}

const STAGE_CHECKS: [AtomicStageCheck; 6] = [
    AtomicStageCheck::BoundaryType,
    AtomicStageCheck::PredicateAdmission,
    AtomicStageCheck::InversionSafety,
    AtomicStageCheck::GraphCatalogue,
    AtomicStageCheck::RetentionLimits,
    AtomicStageCheck::ComplementIdentity,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtomicLogicalStageReceipt {
    pub stage: usize,
    pub descriptor_digest: String,
    pub checks: Vec<AtomicStageCheck>,
}

/// Structural extraction cost. Wall time is intentionally absent.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AtomicPlanCost {
    pub carrier_scans: usize,
    pub focus_publications: usize,
    pub intermediate_publications: usize,
    pub compiled_operations: usize,
    pub stable_tiebreak: String,
}

/// Deterministic saturation accounting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtomicSearchStats {
    pub eclasses: usize,
    pub enodes: usize,
    pub rule_applications: usize,
    pub exhausted: bool,
}

/// Portable certificate for one extracted physical plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtomicPlanCertificate {
    pub version: String,
    pub scope: AtomicOptimizationScope,
    pub original_stages: Vec<AtomicStageDescriptor>,
    pub selected: AtomicPhysicalPlan,
    pub rule_catalog_digest: String,
    pub applications: Vec<AtomicRuleApplication>,
    pub logical_stages: Vec<AtomicLogicalStageReceipt>,
    pub original_cost: AtomicPlanCost,
    pub selected_cost: AtomicPlanCost,
    pub limits: AtomicOptimizerLimits,
    pub search: AtomicSearchStats,
    pub engine_descriptor_hash: String,
    pub certificate_digest: String,
}

/// Executable plan whose certificate was independently rechecked at creation.
#[derive(Debug, Clone)]
pub struct CertifiedAtomicPlan {
    original: Arc<AtomicComposition>,
    certificate: AtomicPlanCertificate,
}

/// Which physical implementation actually published a state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AtomicExecutionMode {
    Original,
    Fused,
}

/// Runtime receipt binds physical selection back to every logical stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AtomicExecutionReceipt {
    pub certificate_digest: String,
    pub mode: AtomicExecutionMode,
    pub fallback: Option<String>,
    pub applied_runtime_rule: Option<AtomicRewriteRule>,
    pub logical_stages: Vec<AtomicLogicalStageReceipt>,
}

#[derive(Debug, Clone)]
enum AtomicPhysicalState {
    Original(AtomicCompositionState),
    Fused(AtomicLensState),
}

/// A state published by a certified plan. The receipt makes a runtime fallback
/// visible and keeps all original checks addressable.
#[derive(Debug, Clone)]
pub struct CertifiedAtomicState {
    plan: Arc<CertifiedAtomicPlan>,
    state: AtomicPhysicalState,
    receipt: AtomicExecutionReceipt,
}

impl AtomicComposition {
    /// Saturate and extract a certified physical plan for this exact sequence.
    /// Search exhaustion is a successful original-plan selection.
    pub fn optimize(
        &self,
        scope: AtomicOptimizationScope,
        limits: AtomicOptimizerLimits,
    ) -> gmeow_errors::Result<CertifiedAtomicPlan> {
        let stages: Vec<_> = self
            .stages
            .iter()
            .map(AtomicStageDescriptor::from_lens)
            .collect();
        let original_cost = original_cost(&stages);
        let mut applications = applications(&stages);
        let unique_stages = stages.iter().collect::<BTreeSet<_>>().len();
        let required_eclasses = unique_stages.saturating_add(1);
        let required_enodes = stages.len().saturating_add(1);
        let mut search = AtomicSearchStats {
            eclasses: required_eclasses.min(limits.max_eclasses),
            enodes: required_enodes.min(limits.max_enodes),
            rule_applications: applications.len(),
            exhausted: false,
        };
        if required_eclasses > limits.max_eclasses
            || required_enodes > limits.max_enodes
            || search.rule_applications > limits.max_rule_applications
        {
            search.exhausted = true;
            applications.truncate(limits.max_rule_applications);
            search.rule_applications = applications.len();
        }
        let fused = fused_descriptor(&stages);
        if fused.is_some() && !search.exhausted {
            if search.enodes == limits.max_enodes {
                search.exhausted = true;
            } else {
                search.enodes += 1;
            }
        }
        let (selected, selected_cost) = if let Some(fused) = fused
            && !search.exhausted
        {
            let plan = AtomicPhysicalPlan::Fused { stage: fused };
            let cost = fused_cost(&plan, unique_stages);
            if cost < original_cost {
                (plan, cost)
            } else {
                (AtomicPhysicalPlan::Original, original_cost.clone())
            }
        } else {
            (AtomicPhysicalPlan::Original, original_cost.clone())
        };
        let logical_stages = stage_receipts(&stages);
        let mut certificate = AtomicPlanCertificate {
            version: CERTIFICATE_VERSION.to_owned(),
            scope,
            original_stages: stages,
            selected,
            rule_catalog_digest: rule_catalog_digest(),
            applications,
            logical_stages,
            original_cost,
            selected_cost,
            limits,
            search,
            engine_descriptor_hash: crate::runtime::EngineContract::current().descriptor_hash,
            certificate_digest: String::new(),
        };
        certificate.certificate_digest = certificate_digest(&certificate);
        verify_atomic_certificate(&certificate, self, &certificate.scope)?;
        Ok(CertifiedAtomicPlan {
            original: Arc::new(self.clone()),
            certificate,
        })
    }
}

impl CertifiedAtomicPlan {
    pub fn certificate(&self) -> &AtomicPlanCertificate {
        &self.certificate
    }

    /// Execute the selected plan. A data-dependent conservative preflight miss
    /// executes the original plan and records the fallback; it never weakens an
    /// admission or publishes a partial fused state.
    pub fn acquire(&self, source: Arc<RdfDataset>) -> gmeow_errors::Result<CertifiedAtomicState> {
        let plan = Arc::new(self.clone());
        match &self.certificate.selected {
            AtomicPhysicalPlan::Original => {
                let state = self.original.acquire(source)?;
                Ok(CertifiedAtomicState::new(
                    plan,
                    AtomicPhysicalState::Original(state),
                    None,
                ))
            }
            AtomicPhysicalPlan::Fused { stage } => {
                let fused = stage.lens()?;
                let attempted = preflight_forward(&self.original, &source)
                    .and_then(|()| fused.acquire(Arc::clone(&source)))
                    .and_then(|state| {
                        preflight_limits(&self.original, state.get().dataset(), false)?;
                        Ok(state)
                    });
                match attempted {
                    Ok(state) => Ok(CertifiedAtomicState::new(
                        plan,
                        AtomicPhysicalState::Fused(state),
                        None,
                    )),
                    Err(reason) => {
                        let detail = format!("fused acquisition retained original plan: {reason}");
                        let state = self.original.acquire(source)?;
                        Ok(CertifiedAtomicState::new(
                            plan,
                            AtomicPhysicalState::Original(state),
                            Some(detail),
                        ))
                    }
                }
            }
        }
    }
}

impl CertifiedAtomicState {
    fn new(
        plan: Arc<CertifiedAtomicPlan>,
        state: AtomicPhysicalState,
        fallback: Option<String>,
    ) -> Self {
        let mode = match state {
            AtomicPhysicalState::Original(_) => AtomicExecutionMode::Original,
            AtomicPhysicalState::Fused(_) => AtomicExecutionMode::Fused,
        };
        let receipt = AtomicExecutionReceipt {
            certificate_digest: plan.certificate.certificate_digest.clone(),
            mode,
            fallback,
            applied_runtime_rule: None,
            logical_stages: plan.certificate.logical_stages.clone(),
        };
        Self {
            plan,
            state,
            receipt,
        }
    }

    pub fn receipt(&self) -> &AtomicExecutionReceipt {
        &self.receipt
    }

    pub fn view(&self) -> Arc<RdfDataset> {
        match &self.state {
            AtomicPhysicalState::Original(state) => Arc::clone(state.get().dataset()),
            AtomicPhysicalState::Fused(state) => Arc::clone(state.get().dataset()),
        }
    }

    pub fn carrier(&self) -> &CompositeDatasetView {
        match &self.state {
            AtomicPhysicalState::Original(state) => state.carrier(),
            AtomicPhysicalState::Fused(state) => state.carrier(),
        }
    }

    /// Update through the same certified plan. If a conservative fused preflight
    /// cannot establish every original stage's limits, the exact current carrier
    /// is materialized once and the original sequence executes.
    pub fn put_shared_scopes(&self, view: Arc<RdfDataset>) -> gmeow_errors::Result<Self> {
        match &self.state {
            AtomicPhysicalState::Original(state) => Ok(Self::new(
                Arc::clone(&self.plan),
                AtomicPhysicalState::Original(state.put_shared_scopes(view)?),
                self.receipt.fallback.clone(),
            )),
            AtomicPhysicalState::Fused(state) => {
                let attempted = preflight_reverse(&self.plan.original, &view)
                    .and_then(|()| preflight_limits(&self.plan.original, &view, true))
                    .and_then(|()| state.put_shared_scopes(Arc::clone(&view)));
                match attempted {
                    Ok(updated) => Ok(Self::new(
                        Arc::clone(&self.plan),
                        AtomicPhysicalState::Fused(updated),
                        None,
                    )),
                    Err(reason) => {
                        let source = self
                            .carrier()
                            .materialize()
                            .map_err(|error| exec_error(error.to_string()))?;
                        let updated = self
                            .plan
                            .original
                            .acquire(source)?
                            .put_shared_scopes(view)?;
                        Ok(Self::new(
                            Arc::clone(&self.plan),
                            AtomicPhysicalState::Original(updated),
                            Some(format!("fused update retained original plan: {reason}")),
                        ))
                    }
                }
            }
        }
    }

    /// Certified cancellation of `put(get(s), s)` for the unchanged augmented
    /// view. Runtime preflight still executes every original stage's admission.
    /// The existing complete carrier is returned without a second publication.
    pub fn cancel_unchanged_get_put(
        &self,
    ) -> gmeow_errors::Result<(Arc<CompositeDatasetView>, AtomicExecutionReceipt)> {
        let view = self.view();
        preflight_reverse(&self.plan.original, &view)?;
        preflight_limits(&self.plan.original, &view, true)?;
        let carrier = match &self.state {
            AtomicPhysicalState::Original(state) => state.carrier_handle(),
            AtomicPhysicalState::Fused(state) => Arc::clone(&state.carrier),
        };
        let mut receipt = self.receipt.clone();
        receipt.applied_runtime_rule = Some(AtomicRewriteRule::WitnessedGetPutCancellation);
        Ok((carrier, receipt))
    }
}

/// Check a transported certificate against the expected logical plan and all
/// eight expected semantic identities.
pub fn verify_atomic_certificate(
    certificate: &AtomicPlanCertificate,
    composition: &AtomicComposition,
    expected_scope: &AtomicOptimizationScope,
) -> gmeow_errors::Result<()> {
    if certificate.version != CERTIFICATE_VERSION
        || &certificate.scope != expected_scope
        || certificate.rule_catalog_digest != rule_catalog_digest()
        || certificate.engine_descriptor_hash
            != crate::runtime::EngineContract::current().descriptor_hash
        || certificate.certificate_digest != certificate_digest(certificate)
    {
        return Err(exec_error(
            "atomic optimization certificate identity or rule catalogue mismatch",
        ));
    }
    let expected: Vec<_> = composition
        .stages
        .iter()
        .map(AtomicStageDescriptor::from_lens)
        .collect();
    if certificate.original_stages != expected
        || certificate.logical_stages != stage_receipts(&expected)
        || certificate.original_cost != original_cost(&expected)
    {
        return Err(exec_error(
            "atomic optimization certificate is rebound to a different logical plan",
        ));
    }
    verify_boundaries(&expected)?;
    for application in &certificate.applications {
        if application.theorem != application.rule.theorem()
            || !application_holds(application, &expected)
        {
            return Err(exec_error(
                "atomic optimization certificate carries an invalid rule application",
            ));
        }
    }
    let unique = expected.iter().collect::<BTreeSet<_>>().len();
    match &certificate.selected {
        AtomicPhysicalPlan::Original => {
            if certificate.selected_cost != certificate.original_cost {
                return Err(exec_error("original atomic plan has a rewritten cost"));
            }
        }
        AtomicPhysicalPlan::Fused { stage } => {
            if certificate.search.exhausted
                || Some(stage.clone()) != fused_descriptor(&expected)
                || !certificate
                    .applications
                    .iter()
                    .any(|application| application.rule == AtomicRewriteRule::CompositionFusion)
                || !certificate.applications.iter().any(|application| {
                    application.rule == AtomicRewriteRule::PredicateProjectionPushdown
                })
                || certificate.selected_cost != fused_cost(&certificate.selected, unique)
                || certificate.selected_cost >= certificate.original_cost
            {
                return Err(exec_error(
                    "fused atomic plan lacks an applicable theorem or deterministic cost improvement",
                ));
            }
        }
    }
    if certificate.search.eclasses > certificate.limits.max_eclasses
        || certificate.search.enodes > certificate.limits.max_enodes
        || certificate.search.rule_applications > certificate.limits.max_rule_applications
        || certificate.search.rule_applications != certificate.applications.len()
    {
        return Err(exec_error(
            "atomic optimization certificate exceeds its deterministic search limits",
        ));
    }
    Ok(())
}

fn applications(stages: &[AtomicStageDescriptor]) -> Vec<AtomicRuleApplication> {
    let mut applications = Vec::new();
    let identities: Vec<_> = stages
        .iter()
        .enumerate()
        .filter(|(_, stage)| !stage.inverse && stage.source_predicate == stage.view_predicate)
        .map(|(index, _)| index)
        .collect();
    if !identities.is_empty() {
        applications.push(application(
            AtomicRewriteRule::IdentityElimination,
            identities,
        ));
    }
    if stages.len() >= 3 {
        applications.push(application(
            AtomicRewriteRule::Reassociation,
            (0..stages.len()).collect(),
        ));
    }
    let mut positions: BTreeMap<&AtomicStageDescriptor, Vec<usize>> = BTreeMap::new();
    for (index, stage) in stages.iter().enumerate() {
        positions.entry(stage).or_default().push(index);
    }
    let duplicates: Vec<_> = positions
        .values()
        .filter(|indices| indices.len() > 1)
        .flatten()
        .copied()
        .collect();
    if !duplicates.is_empty() {
        applications.push(application(
            AtomicRewriteRule::CompiledCommonSubexpression,
            duplicates,
        ));
    }
    if stages.len() >= 2 {
        let all = (0..stages.len()).collect();
        applications.push(application(
            AtomicRewriteRule::PredicateProjectionPushdown,
            all,
        ));
        applications.push(application(
            AtomicRewriteRule::CompositionFusion,
            (0..stages.len()).collect(),
        ));
    }
    applications.push(application(
        AtomicRewriteRule::WitnessedGetPutCancellation,
        (0..stages.len()).collect(),
    ));
    applications
}

fn application(rule: AtomicRewriteRule, stages: Vec<usize>) -> AtomicRuleApplication {
    AtomicRuleApplication {
        rule,
        theorem: rule.theorem().to_owned(),
        stages,
    }
}

fn application_holds(
    application: &AtomicRuleApplication,
    stages: &[AtomicStageDescriptor],
) -> bool {
    let valid = application.stages.iter().all(|&index| index < stages.len());
    if !valid {
        return false;
    }
    match application.rule {
        AtomicRewriteRule::IdentityElimination => {
            !application.stages.is_empty()
                && application.stages.iter().all(|&index| {
                    let stage = &stages[index];
                    !stage.inverse && stage.source_predicate == stage.view_predicate
                })
        }
        AtomicRewriteRule::Reassociation => {
            stages.len() >= 3 && application.stages == (0..stages.len()).collect::<Vec<_>>()
        }
        AtomicRewriteRule::CompiledCommonSubexpression => {
            application.stages.len() >= 2
                && application
                    .stages
                    .iter()
                    .map(|&index| &stages[index])
                    .collect::<BTreeSet<_>>()
                    .len()
                    < application.stages.len()
        }
        AtomicRewriteRule::PredicateProjectionPushdown | AtomicRewriteRule::CompositionFusion => {
            stages.len() >= 2
                && application.stages == (0..stages.len()).collect::<Vec<_>>()
                && verify_boundaries(stages).is_ok()
        }
        AtomicRewriteRule::WitnessedGetPutCancellation => {
            !stages.is_empty()
                && application.stages == (0..stages.len()).collect::<Vec<_>>()
                && verify_boundaries(stages).is_ok()
        }
    }
}

fn verify_boundaries(stages: &[AtomicStageDescriptor]) -> gmeow_errors::Result<()> {
    if stages.is_empty()
        || stages
            .windows(2)
            .any(|pair| pair[0].view_predicate != pair[1].source_predicate)
    {
        return Err(exec_error(
            "atomic optimizer received an ill-typed sequence",
        ));
    }
    Ok(())
}

fn fused_descriptor(stages: &[AtomicStageDescriptor]) -> Option<AtomicStageDescriptor> {
    (stages.len() >= 2 && verify_boundaries(stages).is_ok()).then(|| AtomicStageDescriptor {
        source_predicate: stages[0].source_predicate.clone(),
        view_predicate: stages.last().expect("nonempty").view_predicate.clone(),
        inverse: stages
            .iter()
            .fold(false, |parity, stage| parity ^ stage.inverse),
        // The rich source and residual belong to the first stage. Later limits
        // are checked separately against conservative focus/catalogue witnesses.
        limits: stages[0].limits,
    })
}

fn stage_receipts(stages: &[AtomicStageDescriptor]) -> Vec<AtomicLogicalStageReceipt> {
    stages
        .iter()
        .enumerate()
        .map(|(stage, descriptor)| AtomicLogicalStageReceipt {
            stage,
            descriptor_digest: descriptor.digest(),
            checks: STAGE_CHECKS.to_vec(),
        })
        .collect()
}

fn original_cost(stages: &[AtomicStageDescriptor]) -> AtomicPlanCost {
    AtomicPlanCost {
        carrier_scans: stages.len(),
        focus_publications: stages.len(),
        intermediate_publications: stages.len().saturating_sub(1),
        compiled_operations: stages.iter().collect::<BTreeSet<_>>().len(),
        stable_tiebreak: "original".to_owned(),
    }
}

fn fused_cost(plan: &AtomicPhysicalPlan, unique_stages: usize) -> AtomicPlanCost {
    AtomicPlanCost {
        // One conservative preflight scan plus one native focus publication.
        carrier_scans: 2,
        focus_publications: 1,
        intermediate_publications: 0,
        compiled_operations: unique_stages.min(1),
        stable_tiebreak: plan.key(),
    }
}

fn preflight_forward(
    composition: &AtomicComposition,
    source: &RdfDataset,
) -> gmeow_errors::Result<()> {
    let Some(predicate) = source.term_id_by_iri(&composition.stages[0].source_predicate) else {
        return Ok(());
    };
    let rows: Vec<_> = source
        .quads_for_pattern(None, Some(predicate), None, purrdf::GraphMatch::Any)
        .collect();
    let mut parity = false;
    for (index, stage) in composition.stages.iter().enumerate() {
        if stage.inverse {
            for row in &rows {
                let candidate = if parity { row.s } else { row.o };
                if matches!(source.resolve(candidate), purrdf::TermRef::Literal { .. }) {
                    return Err(exec_error(format!(
                        "atomic composition stage {index}: inversion would place a literal in subject position"
                    )));
                }
            }
            parity = !parity;
        }
    }
    Ok(())
}

fn preflight_reverse(
    composition: &AtomicComposition,
    view: &RdfDataset,
) -> gmeow_errors::Result<()> {
    let Some(predicate) = view.term_id_by_iri(
        &composition
            .stages
            .last()
            .expect("nonempty checked composition")
            .view_predicate,
    ) else {
        return Ok(());
    };
    let rows: Vec<_> = view
        .quads_for_pattern(None, Some(predicate), None, purrdf::GraphMatch::Any)
        .collect();
    let mut parity = false;
    for (index, stage) in composition.stages.iter().enumerate().rev() {
        if stage.inverse {
            for row in &rows {
                let candidate = if parity { row.s } else { row.o };
                if matches!(view.resolve(candidate), purrdf::TermRef::Literal { .. }) {
                    return Err(exec_error(format!(
                        "atomic composition stage {index}: inverse update would place a literal in subject position"
                    )));
                }
            }
            parity = !parity;
        }
    }
    Ok(())
}

fn preflight_limits(
    composition: &AtomicComposition,
    terminal: &Arc<RdfDataset>,
    restoring: bool,
) -> gmeow_errors::Result<()> {
    if composition.stages.len() < 2 {
        return Ok(());
    }
    let last = composition.stages.last().expect("nonempty");
    let longest = composition
        .stages
        .iter()
        .flat_map(|stage| [&stage.source_predicate, &stage.view_predicate])
        .map(String::len)
        .max()
        .unwrap_or(0);
    let mut witness_iri = format!(
        "urn:gmeow:atomic-optimizer:limit-witness:{}:",
        digest(
            "gmeow-atomic-limit-witness-v1",
            composition
                .stages
                .iter()
                .map(|s| s.source_predicate.as_str())
        )
    );
    while witness_iri.len() <= longest || terminal.term_id_by_iri(&witness_iri).is_some() {
        witness_iri.push('x');
    }
    let witness = super::super::rename_focus(terminal, &last.view_predicate, &witness_iri, false)?;
    let graphs = super::super::graph_catalogue_dataset(terminal)?;
    for (index, stage) in composition.stages.iter().enumerate().skip(1) {
        let residual = purrdf::ir::MutableDataset::new(Arc::clone(&graphs))
            .snapshot_view_with_limits(stage.limits)
            .map_err(|error| {
                exec_error(format!(
                    "atomic composition stage {index}: retain graph catalogue: {error}"
                ))
            })?;
        if restoring {
            CompositeDatasetView::from_shared_sources(
                vec![
                    purrdf::CompositeSource::from_delta(Arc::new(residual)),
                    purrdf::CompositeSource::new(Arc::clone(&witness)),
                ],
                stage.limits,
            )
            .map_err(|error| {
                exec_error(format!(
                    "atomic composition stage {index}: admit restored focus: {error}"
                ))
            })?;
        } else {
            CompositeDatasetView::with_shared_scopes(vec![Arc::clone(&witness)], stage.limits)
                .map_err(|error| {
                    exec_error(format!(
                        "atomic composition stage {index}: admit projected focus: {error}"
                    ))
                })?;
        }
    }
    Ok(())
}

fn certificate_digest(certificate: &AtomicPlanCertificate) -> String {
    let mut digest = StableDigest::new("gmeow-atomic-plan-certificate-v1");
    digest.text("version", &certificate.version);
    digest.text("scope.program", &certificate.scope.program_digest);
    digest.text(
        "scope.source-theory",
        &certificate.scope.source_theory_digest,
    );
    digest.text("scope.view-theory", &certificate.scope.view_theory_digest);
    digest.text("scope.context", &certificate.scope.context_digest);
    digest.text("scope.effects", &certificate.scope.effect_digest);
    digest.text("scope.complement", &certificate.scope.complement_digest);
    digest.text("scope.preservation", &certificate.scope.preservation_digest);
    digest.text(
        "scope.reasoning-contract",
        &certificate.scope.reasoning_contract_digest,
    );
    digest.text("rule-catalog", &certificate.rule_catalog_digest);
    digest.text("engine", &certificate.engine_descriptor_hash);

    digest.usize("original-stages.len", certificate.original_stages.len());
    for stage in &certificate.original_stages {
        digest_stage(&mut digest, "original-stage", stage);
    }
    match &certificate.selected {
        AtomicPhysicalPlan::Original => digest.text("selected.kind", "original"),
        AtomicPhysicalPlan::Fused { stage } => {
            digest.text("selected.kind", "fused");
            digest_stage(&mut digest, "selected.stage", stage);
        }
    }

    digest.usize("applications.len", certificate.applications.len());
    for application in &certificate.applications {
        digest.text("application.rule", application.rule.id());
        digest.text("application.theorem", &application.theorem);
        digest.usize("application.stages.len", application.stages.len());
        for &stage in &application.stages {
            digest.usize("application.stage", stage);
        }
    }

    digest.usize("logical-stages.len", certificate.logical_stages.len());
    for stage in &certificate.logical_stages {
        digest.usize("logical-stage.index", stage.stage);
        digest.text("logical-stage.descriptor", &stage.descriptor_digest);
        digest.usize("logical-stage.checks.len", stage.checks.len());
        for &check in &stage.checks {
            digest.text("logical-stage.check", check.id());
        }
    }

    digest_cost(&mut digest, "original-cost", &certificate.original_cost);
    digest_cost(&mut digest, "selected-cost", &certificate.selected_cost);
    digest.usize("limits.eclasses", certificate.limits.max_eclasses);
    digest.usize("limits.enodes", certificate.limits.max_enodes);
    digest.usize(
        "limits.rule-applications",
        certificate.limits.max_rule_applications,
    );
    digest.usize("search.eclasses", certificate.search.eclasses);
    digest.usize("search.enodes", certificate.search.enodes);
    digest.usize(
        "search.rule-applications",
        certificate.search.rule_applications,
    );
    digest.boolean("search.exhausted", certificate.search.exhausted);
    digest.finish()
}

fn digest_stage(digest: &mut StableDigest, label: &str, stage: &AtomicStageDescriptor) {
    digest.text(&format!("{label}.source"), &stage.source_predicate);
    digest.text(&format!("{label}.view"), &stage.view_predicate);
    digest.boolean(&format!("{label}.inverse"), stage.inverse);
    digest.usize(&format!("{label}.limits.sources"), stage.limits.sources);
    digest.usize(&format!("{label}.limits.terms"), stage.limits.terms);
    digest.usize(&format!("{label}.limits.rows"), stage.limits.rows);
    digest.usize(
        &format!("{label}.limits.payload-bytes"),
        stage.limits.payload_bytes,
    );
    digest.usize(
        &format!("{label}.limits.auxiliary-bytes"),
        stage.limits.auxiliary_bytes,
    );
}

fn digest_cost(digest: &mut StableDigest, label: &str, cost: &AtomicPlanCost) {
    digest.usize(&format!("{label}.carrier-scans"), cost.carrier_scans);
    digest.usize(
        &format!("{label}.focus-publications"),
        cost.focus_publications,
    );
    digest.usize(
        &format!("{label}.intermediate-publications"),
        cost.intermediate_publications,
    );
    digest.usize(
        &format!("{label}.compiled-operations"),
        cost.compiled_operations,
    );
    digest.text(&format!("{label}.tiebreak"), &cost.stable_tiebreak);
}

fn digest<'a>(domain: &'a str, values: impl IntoIterator<Item = &'a str>) -> String {
    let mut hasher = blake3::Hasher::new();
    for value in std::iter::once(domain).chain(values) {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}
