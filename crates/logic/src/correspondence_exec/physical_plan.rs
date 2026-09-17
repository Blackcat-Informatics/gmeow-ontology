// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Certified physical plans for authored correspondence compositions.
//!
//! This layer compiles a declared `first ; second = composite` obligation into
//! one reusable native plan. The only fusion rule here is the exact relational
//! law for two pure [`LegPath`] reads: evaluate both paths in one join while
//! retaining the shared middle witness. It never fuses stateful `put`, derives a
//! recovery law from a bounded corpus, or treats a relation path as an atomic
//! predicate-renaming lens.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use gmeow_logic_compile::ir::{Correspondence, CorrespondenceComposition, LegPath};
use gmeow_logic_compile::projections::correspondence::CorrespondenceProgram;
use gmeow_logic_compile::projections::paths::{leg_path_canonical, lower_leg_path};
use purrdf::sparql::{
    GraphPattern, NativeSparqlEngine, PreparedQuery, Query, QueryOptions, SparqlResult,
    TermPattern, Variable,
};
use purrdf::{CanonHash, RdfDataset};
use serde::{Deserialize, Serialize};

use super::stable_digest::StableDigest;
use super::{
    EndpointRelation, endpoint_key, exec_error, execute_leg_relation, leg_relation_algebra,
};

#[cfg(test)]
mod tests;

const CERTIFICATE_VERSION: &str = "gmeow-correspondence-composition-plan-v1";
const REPORT_VERSION: &str = "gmeow-correspondence-composition-plans-v1";
const OUTPUT_CONTRACT: &str = "default-graph-composite-relation-with-middle-witness-v1";
const RULE_ID: &str = "typed-property-path-witness-join-fusion-v1";
const RULE_THEOREM: &str = "for pure set-valued property-path relations P:S→M and Q:M→T, one native join on the shared M witness equals relational composition Q∘P without publishing either endpoint relation";

/// Deterministic physical-planning bounds. A bound miss retains the original
/// two-query computation; it never truncates a path or result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionPlanLimits {
    pub max_path_nodes: usize,
}

impl Default for CompositionPlanLimits {
    fn default() -> Self {
        Self {
            max_path_nodes: 4_096,
        }
    }
}

/// Exact semantic axes that authorize reuse of one physical plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionOptimizationScope {
    pub program_digest: String,
    pub declaration_digest: String,
    pub context_digest: String,
    pub effect_digest: String,
    pub complement_digest: String,
    pub preservation_digest: String,
    pub reasoning_contract_digest: String,
}

/// One logical correspondence stage retained by a physical plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionLogicalStage {
    pub correspondence: String,
    pub get_leg: String,
    pub path: String,
    pub source_endpoint: String,
    pub target_endpoint: String,
    pub standpoint: Option<String>,
    pub checks: Vec<CompositionStageCheck>,
}

/// Checks whose identity remains attached to each authored stage after fusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CompositionStageCheck {
    EndpointType,
    Standpoint,
    PureReadEffect,
    PathAdmission,
    ComplementRetention,
    Preservation,
    LogicalGate,
}

impl CompositionStageCheck {
    fn id(self) -> &'static str {
        match self {
            Self::EndpointType => "endpoint-type-v1",
            Self::Standpoint => "standpoint-v1",
            Self::PureReadEffect => "pure-read-effect-v1",
            Self::PathAdmission => "path-admission-v1",
            Self::ComplementRetention => "complement-retention-v1",
            Self::Preservation => "preservation-v1",
            Self::LogicalGate => "logical-gate-v1",
        }
    }
}

const STAGE_CHECKS: [CompositionStageCheck; 7] = [
    CompositionStageCheck::EndpointType,
    CompositionStageCheck::Standpoint,
    CompositionStageCheck::PureReadEffect,
    CompositionStageCheck::PathAdmission,
    CompositionStageCheck::ComplementRetention,
    CompositionStageCheck::Preservation,
    CompositionStageCheck::LogicalGate,
];

/// Why a structurally valid declaration retained its original implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OriginalPlanReason {
    SearchLimit,
    NativeAdmission,
}

impl OriginalPlanReason {
    fn id(self) -> &'static str {
        match self {
            Self::SearchLimit => "search-limit-v1",
            Self::NativeAdmission => "native-admission-v1",
        }
    }
}

/// The selected physical implementation. Stateful update is deliberately not
/// represented here and remains on the complement-preserving lens executor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CompositionPhysicalPlan {
    Original { reason: OriginalPlanReason },
    FusedWitnessJoin,
}

/// Structural cost used for deterministic extraction. Wall time is absent.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CompositionPlanCost {
    pub native_queries: usize,
    pub materialized_intermediate_relations: usize,
    pub path_nodes: usize,
    pub stable_tiebreak: String,
}

/// Portable proof that one declared composite was compiled without weakening
/// context, complement, preservation, or its logical-stage contracts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionPlanCertificate {
    pub version: String,
    pub declaration: String,
    pub scope: CompositionOptimizationScope,
    pub first: CompositionLogicalStage,
    pub second: CompositionLogicalStage,
    pub composite: CompositionLogicalStage,
    pub output_contract: String,
    pub selected: CompositionPhysicalPlan,
    pub applied_rules: Vec<String>,
    pub rule_catalog_digest: String,
    pub original_cost: CompositionPlanCost,
    pub selected_cost: CompositionPlanCost,
    pub limits: CompositionPlanLimits,
    pub engine_descriptor_hash: String,
    pub certificate_digest: String,
}

/// Stable artifact written by the producer and embedded with its typed program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionPlanReport {
    pub version: String,
    pub program_digest: String,
    pub certificates: Vec<CompositionPlanCertificate>,
}

/// One witness-preserving row of the composite relation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CompositionRelationRow {
    pub source: String,
    pub middle: String,
    pub target: String,
}

/// Runtime evidence for execution on one exact canonical carrier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionExecutionReceipt {
    pub certificate_digest: String,
    pub input_digest: String,
    pub mode: CompositionPhysicalPlan,
    pub logical_stages: Vec<CompositionLogicalStage>,
    pub result_rows: usize,
    pub materialized_intermediate_rows: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionExecution {
    pub rows: Vec<CompositionRelationRow>,
    pub receipt: CompositionExecutionReceipt,
}

/// Prepared plans for every authored composition in one exact program.
pub struct PreparedCompositionProgram {
    report: CompositionPlanReport,
    plans: BTreeMap<String, PreparedCompositionPlan>,
}

struct PreparedCompositionPlan {
    engine: NativeSparqlEngine,
    first: Arc<PreparedQuery>,
    second: Arc<PreparedQuery>,
    fused: Option<Arc<PreparedQuery>>,
    certificate: CompositionPlanCertificate,
}

impl PreparedCompositionProgram {
    /// Compile every authored declaration through native algebra admission.
    /// Invalid declarations fail; deterministic resource exhaustion retains an
    /// admitted original plan.
    pub fn prepare(
        program: &CorrespondenceProgram,
        limits: CompositionPlanLimits,
    ) -> gmeow_errors::Result<Self> {
        let program_digest = program_digest(program);
        let mut plans = BTreeMap::new();
        let mut certificates = Vec::with_capacity(program.compositions.len());
        for declaration in &program.compositions {
            let plan = PreparedCompositionPlan::prepare(program, declaration, limits)?;
            if plans.insert(declaration.iri.clone(), plan).is_some() {
                return Err(exec_error(format!(
                    "duplicate correspondence composition identity <{}>",
                    declaration.iri
                )));
            }
        }
        certificates.extend(plans.values().map(|plan| plan.certificate.clone()));
        let report = CompositionPlanReport {
            version: REPORT_VERSION.to_owned(),
            program_digest,
            certificates,
        };
        verify_composition_report(&report, program)?;
        Ok(Self { report, plans })
    }

    pub fn report(&self) -> &CompositionPlanReport {
        &self.report
    }

    pub fn report_json(&self) -> gmeow_errors::Result<Vec<u8>> {
        serde_json::to_vec_pretty(&self.report)
            .map_err(|error| exec_error(format!("encode correspondence physical plans: {error}")))
    }

    /// Execute one exact declaration against caller data.
    pub fn execute(
        &self,
        declaration: &str,
        dataset: &Arc<RdfDataset>,
    ) -> gmeow_errors::Result<CompositionExecution> {
        self.plans
            .get(declaration)
            .ok_or_else(|| {
                exec_error(format!(
                    "correspondence composition <{declaration}> has no prepared plan"
                ))
            })?
            .execute(dataset)
    }
}

impl PreparedCompositionPlan {
    fn prepare(
        program: &CorrespondenceProgram,
        declaration: &CorrespondenceComposition,
        limits: CompositionPlanLimits,
    ) -> gmeow_errors::Result<Self> {
        let first = unique_correspondence(program, &declaration.first)?;
        let second = unique_correspondence(program, &declaration.second)?;
        let composite = unique_correspondence(program, &declaration.composite)?;
        verify_composition_boundary(declaration, first, second, composite)?;

        let (first_leg, first_path) = required_get(program, first)?;
        let (second_leg, second_path) = required_get(program, second)?;
        let (composite_leg, composite_path) = required_get(program, composite)?;
        let fused_path = LegPath::Seq(vec![first_path.clone(), second_path.clone()]).normalize();
        if leg_path_canonical(&fused_path) != leg_path_canonical(composite_path) {
            return Err(exec_error(format!(
                "correspondence composition <{}> declares composite <{}> whose get body is not the normalized sequential composition of <{}> then <{}>",
                declaration.iri, composite.iri, first.iri, second.iri
            )));
        }

        let engine = NativeSparqlEngine::new();
        let first_query = prepare_path(&engine, first_path)?;
        let second_query = prepare_path(&engine, second_path)?;
        let path_nodes = path_nodes(first_path)
            .saturating_add(path_nodes(second_path))
            .saturating_add(1);
        let original_cost = original_cost(path_nodes);
        let (selected, selected_cost, fused) = if path_nodes > limits.max_path_nodes {
            (
                CompositionPhysicalPlan::Original {
                    reason: OriginalPlanReason::SearchLimit,
                },
                original_cost.clone(),
                None,
            )
        } else {
            match prepare_fused(&engine, first_path, second_path) {
                Ok(query) => {
                    let candidate = fused_cost(path_nodes);
                    if candidate < original_cost {
                        (
                            CompositionPhysicalPlan::FusedWitnessJoin,
                            candidate,
                            Some(query),
                        )
                    } else {
                        (
                            CompositionPhysicalPlan::Original {
                                reason: OriginalPlanReason::NativeAdmission,
                            },
                            original_cost.clone(),
                            None,
                        )
                    }
                }
                Err(_) => (
                    CompositionPhysicalPlan::Original {
                        reason: OriginalPlanReason::NativeAdmission,
                    },
                    original_cost.clone(),
                    None,
                ),
            }
        };

        let first_stage = logical_stage(first, first_leg, first_path)?;
        let second_stage = logical_stage(second, second_leg, second_path)?;
        let composite_stage = logical_stage(composite, composite_leg, composite_path)?;
        let scope = optimization_scope(
            program,
            declaration,
            [first, second, composite],
            [first_path, second_path, composite_path],
        );
        let mut certificate = CompositionPlanCertificate {
            version: CERTIFICATE_VERSION.to_owned(),
            declaration: declaration.iri.clone(),
            scope,
            first: first_stage,
            second: second_stage,
            composite: composite_stage,
            output_contract: OUTPUT_CONTRACT.to_owned(),
            applied_rules: matches!(&selected, CompositionPhysicalPlan::FusedWitnessJoin)
                .then(|| vec![RULE_ID.to_owned()])
                .unwrap_or_default(),
            selected,
            rule_catalog_digest: rule_catalog_digest(),
            original_cost,
            selected_cost,
            limits,
            engine_descriptor_hash: crate::runtime::EngineContract::current().descriptor_hash,
            certificate_digest: String::new(),
        };
        certificate.certificate_digest = certificate_digest(&certificate);
        verify_composition_certificate(&certificate, program)?;
        Ok(Self {
            engine,
            first: first_query,
            second: second_query,
            fused,
            certificate,
        })
    }

    fn execute(&self, dataset: &Arc<RdfDataset>) -> gmeow_errors::Result<CompositionExecution> {
        let (rows, intermediate_rows) = match &self.certificate.selected {
            CompositionPhysicalPlan::Original { .. } => {
                let first = execute_leg_relation(&self.engine, dataset, &self.first)?;
                let second = execute_leg_relation(&self.engine, dataset, &self.second)?;
                let count = first.len().saturating_add(second.len());
                (join_relations(&first, &second), count)
            }
            CompositionPhysicalPlan::FusedWitnessJoin => {
                let query = self.fused.as_ref().ok_or_else(|| {
                    exec_error("fused correspondence certificate has no admitted native plan")
                })?;
                (execute_fused(&self.engine, dataset, query)?, 0)
            }
        };
        let input_digest = purrdf::try_flat_digest_view(dataset.as_ref(), CanonHash::Sha256)
            .map_err(|error| {
                exec_error(format!(
                    "canonicalize correspondence execution input: {error}"
                ))
            })?
            .to_hex();
        let rows: Vec<_> = rows.into_iter().collect();
        let receipt = CompositionExecutionReceipt {
            certificate_digest: self.certificate.certificate_digest.clone(),
            input_digest,
            mode: self.certificate.selected.clone(),
            logical_stages: vec![
                self.certificate.first.clone(),
                self.certificate.second.clone(),
                self.certificate.composite.clone(),
            ],
            result_rows: rows.len(),
            materialized_intermediate_rows: intermediate_rows,
        };
        Ok(CompositionExecution { rows, receipt })
    }
}

/// Independently check a transported report, its exact declaration coverage,
/// and every contained certificate against the complete typed program.
pub fn verify_composition_report(
    report: &CompositionPlanReport,
    program: &CorrespondenceProgram,
) -> gmeow_errors::Result<()> {
    if report.version != REPORT_VERSION || report.program_digest != program_digest(program) {
        return Err(exec_error(
            "correspondence physical-plan report is bound to a different program or version",
        ));
    }
    let expected: BTreeSet<_> = program
        .compositions
        .iter()
        .map(|declaration| declaration.iri.as_str())
        .collect();
    if expected.len() != program.compositions.len() {
        return Err(exec_error(
            "correspondence program contains duplicate composition identities",
        ));
    }
    let actual: BTreeSet<_> = report
        .certificates
        .iter()
        .map(|certificate| certificate.declaration.as_str())
        .collect();
    if actual.len() != report.certificates.len() || actual != expected {
        return Err(exec_error(
            "correspondence physical-plan report does not cover every declaration exactly once",
        ));
    }
    for certificate in &report.certificates {
        verify_composition_certificate(certificate, program)?;
    }
    Ok(())
}

/// Independently check a transported certificate against the exact typed program.
pub fn verify_composition_certificate(
    certificate: &CompositionPlanCertificate,
    program: &CorrespondenceProgram,
) -> gmeow_errors::Result<()> {
    if certificate.version != CERTIFICATE_VERSION
        || certificate.output_contract != OUTPUT_CONTRACT
        || certificate.rule_catalog_digest != rule_catalog_digest()
        || certificate.engine_descriptor_hash
            != crate::runtime::EngineContract::current().descriptor_hash
        || certificate.certificate_digest != certificate_digest(certificate)
    {
        return Err(exec_error(
            "correspondence composition certificate identity or rule catalogue mismatch",
        ));
    }
    let declaration = program
        .compositions
        .iter()
        .find(|value| value.iri == certificate.declaration)
        .ok_or_else(|| exec_error("composition certificate declaration is absent"))?;
    if program
        .compositions
        .iter()
        .filter(|value| value.iri == certificate.declaration)
        .count()
        != 1
    {
        return Err(exec_error(
            "composition certificate declaration identity is ambiguous",
        ));
    }
    let first = unique_correspondence(program, &declaration.first)?;
    let second = unique_correspondence(program, &declaration.second)?;
    let composite = unique_correspondence(program, &declaration.composite)?;
    verify_composition_boundary(declaration, first, second, composite)?;
    let (first_leg, first_path) = required_get(program, first)?;
    let (second_leg, second_path) = required_get(program, second)?;
    let (composite_leg, composite_path) = required_get(program, composite)?;
    let fused = LegPath::Seq(vec![first_path.clone(), second_path.clone()]).normalize();
    if leg_path_canonical(&fused) != leg_path_canonical(composite_path) {
        return Err(exec_error(
            "composition certificate is bound to a false executable composite",
        ));
    }
    let expected_scope = optimization_scope(
        program,
        declaration,
        [first, second, composite],
        [first_path, second_path, composite_path],
    );
    let expected_first = logical_stage(first, first_leg, first_path)?;
    let expected_second = logical_stage(second, second_leg, second_path)?;
    let expected_composite = logical_stage(composite, composite_leg, composite_path)?;
    let nodes = path_nodes(first_path)
        .saturating_add(path_nodes(second_path))
        .saturating_add(1);
    if certificate.scope != expected_scope
        || certificate.first != expected_first
        || certificate.second != expected_second
        || certificate.composite != expected_composite
        || certificate.original_cost != original_cost(nodes)
    {
        return Err(exec_error(
            "composition certificate is rebound to a different typed program",
        ));
    }
    match &certificate.selected {
        CompositionPhysicalPlan::Original {
            reason: OriginalPlanReason::SearchLimit,
        } if nodes > certificate.limits.max_path_nodes
            && certificate.applied_rules.is_empty()
            && certificate.selected_cost == certificate.original_cost => {}
        CompositionPhysicalPlan::Original {
            reason: OriginalPlanReason::NativeAdmission,
        } if certificate.applied_rules.is_empty()
            && certificate.selected_cost == certificate.original_cost => {}
        CompositionPhysicalPlan::FusedWitnessJoin
            if nodes <= certificate.limits.max_path_nodes
                && certificate.applied_rules.len() == 1
                && certificate.applied_rules[0] == RULE_ID
                && certificate.selected_cost == fused_cost(nodes)
                && certificate.selected_cost < certificate.original_cost => {}
        _ => {
            return Err(exec_error(
                "composition certificate carries an invalid deterministic extraction",
            ));
        }
    }
    Ok(())
}

fn unique_correspondence<'a>(
    program: &'a CorrespondenceProgram,
    iri: &str,
) -> gmeow_errors::Result<&'a Correspondence> {
    let mut matches = program
        .correspondences
        .iter()
        .filter(|value| value.iri == iri);
    let value = matches
        .next()
        .ok_or_else(|| exec_error(format!("correspondence <{iri}> is absent")))?;
    if matches.next().is_some() {
        return Err(exec_error(format!(
            "correspondence identity <{iri}> is ambiguous"
        )));
    }
    Ok(value)
}

fn required_get<'a>(
    program: &'a CorrespondenceProgram,
    correspondence: &'a Correspondence,
) -> gmeow_errors::Result<(&'a str, &'a LegPath)> {
    let iri = correspondence.get_leg.as_deref().ok_or_else(|| {
        exec_error(format!(
            "correspondence <{}> has no executable get leg",
            correspondence.iri
        ))
    })?;
    let mut matches = program.leg_programs.iter().filter(|value| value.iri == iri);
    let leg = matches.next().ok_or_else(|| {
        exec_error(format!(
            "correspondence <{}> get leg <{iri}> is unresolved",
            correspondence.iri
        ))
    })?;
    if matches.next().is_some() {
        return Err(exec_error(format!(
            "correspondence get leg identity <{iri}> is ambiguous"
        )));
    }
    Ok((iri, &leg.body))
}

fn verify_composition_boundary(
    declaration: &CorrespondenceComposition,
    first: &Correspondence,
    second: &Correspondence,
    composite: &Correspondence,
) -> gmeow_errors::Result<()> {
    let (source, middle) = composition_endpoints(first)?;
    let (next_source, target) = composition_endpoints(second)?;
    let (composite_source, composite_target) = composition_endpoints(composite)?;
    if middle != next_source || source != composite_source || target != composite_target {
        return Err(exec_error(format!(
            "correspondence composition <{}> endpoints do not commute in acquisition order",
            declaration.iri
        )));
    }
    if first.according_to != second.according_to || first.according_to != composite.according_to {
        return Err(exec_error(format!(
            "correspondence composition <{}> changes standpoint without an explicit context transport",
            declaration.iri
        )));
    }
    Ok(())
}

fn composition_endpoints(value: &Correspondence) -> gmeow_errors::Result<(&str, &str)> {
    value
        .source_endpoint
        .as_deref()
        .zip(value.target_endpoint.as_deref())
        .ok_or_else(|| {
            exec_error(format!(
                "correspondence <{}> requires explicit source and target endpoints for composition",
                value.iri
            ))
        })
}

fn logical_stage(
    correspondence: &Correspondence,
    leg: &str,
    path: &LegPath,
) -> gmeow_errors::Result<CompositionLogicalStage> {
    let (source_endpoint, target_endpoint) = correspondence
        .source_endpoint
        .as_ref()
        .zip(correspondence.target_endpoint.as_ref())
        .ok_or_else(|| exec_error("composition stage has incomplete endpoints"))?;
    Ok(CompositionLogicalStage {
        correspondence: correspondence.iri.clone(),
        get_leg: leg.to_owned(),
        path: leg_path_canonical(path),
        source_endpoint: source_endpoint.clone(),
        target_endpoint: target_endpoint.clone(),
        standpoint: correspondence.according_to.clone(),
        checks: STAGE_CHECKS.to_vec(),
    })
}

fn prepare_path(
    engine: &NativeSparqlEngine,
    path: &LegPath,
) -> gmeow_errors::Result<Arc<PreparedQuery>> {
    engine
        .prepare_algebra(leg_relation_algebra(path)?, QueryOptions::EMPTY)
        .map_err(|error| exec_error(format!("prepare correspondence relation path: {error}")))
}

fn prepare_fused(
    engine: &NativeSparqlEngine,
    first: &LegPath,
    second: &LegPath,
) -> gmeow_errors::Result<Arc<PreparedQuery>> {
    engine
        .prepare_algebra(fused_relation_algebra(first, second), QueryOptions::EMPTY)
        .map_err(|error| exec_error(format!("prepare fused correspondence relation: {error}")))
}

fn fused_relation_algebra(first: &LegPath, second: &LegPath) -> Query {
    let source = Variable::new("s");
    let middle = Variable::new("m");
    let target = Variable::new("o");
    let left = GraphPattern::Path {
        subject: TermPattern::Variable(source.clone()),
        path: lower_leg_path(first),
        object: TermPattern::Variable(middle.clone()),
    };
    let right = GraphPattern::Path {
        subject: TermPattern::Variable(middle.clone()),
        path: lower_leg_path(second),
        object: TermPattern::Variable(target.clone()),
    };
    Query::Select {
        pattern: GraphPattern::Project {
            inner: Box::new(GraphPattern::Join {
                left: Box::new(left),
                right: Box::new(right),
            }),
            variables: vec![source, middle, target],
        },
        dataset: Default::default(),
        base_iri: None,
        version: None,
    }
}

fn execute_fused(
    engine: &NativeSparqlEngine,
    dataset: &Arc<RdfDataset>,
    query: &PreparedQuery,
) -> gmeow_errors::Result<BTreeSet<CompositionRelationRow>> {
    let result = engine
        .query_prepared(dataset, query, &[], QueryOptions::EMPTY)
        .map_err(|error| exec_error(format!("execute fused correspondence relation: {error}")))?;
    let SparqlResult::Solutions {
        variables, rows, ..
    } = result
    else {
        return Err(exec_error(
            "fused correspondence relation did not return solutions",
        ));
    };
    let index = |name: &str| {
        variables
            .iter()
            .position(|variable| variable == name)
            .ok_or_else(|| exec_error(format!("fused correspondence omitted ?{name}")))
    };
    let source = index("s")?;
    let middle = index("m")?;
    let target = index("o")?;
    let mut out = BTreeSet::new();
    for row in rows {
        let value = |position: usize, name: &str| {
            row.get(position)
                .and_then(Option::as_ref)
                .ok_or_else(|| exec_error(format!("fused correspondence left ?{name} unbound")))
        };
        out.insert(CompositionRelationRow {
            source: endpoint_key(value(source, "s")?)?,
            middle: endpoint_key(value(middle, "m")?)?,
            target: endpoint_key(value(target, "o")?)?,
        });
    }
    Ok(out)
}

fn join_relations(
    first: &EndpointRelation,
    second: &EndpointRelation,
) -> BTreeSet<CompositionRelationRow> {
    let mut by_source: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (source, target) in second {
        by_source.entry(source).or_default().push(target);
    }
    let mut rows = BTreeSet::new();
    for (source, middle) in first {
        if let Some(targets) = by_source.get(middle.as_str()) {
            for &target in targets {
                rows.insert(CompositionRelationRow {
                    source: source.clone(),
                    middle: middle.clone(),
                    target: target.to_owned(),
                });
            }
        }
    }
    rows
}

fn path_nodes(path: &LegPath) -> usize {
    match path {
        LegPath::Step(_) => 1,
        LegPath::Inverse(inner) => 1usize.saturating_add(path_nodes(inner)),
        LegPath::Seq(parts) | LegPath::Alt(parts) => parts
            .iter()
            .fold(1usize, |sum, part| sum.saturating_add(path_nodes(part))),
    }
}

fn original_cost(path_nodes: usize) -> CompositionPlanCost {
    CompositionPlanCost {
        native_queries: 2,
        materialized_intermediate_relations: 2,
        path_nodes,
        stable_tiebreak: "original".to_owned(),
    }
}

fn fused_cost(path_nodes: usize) -> CompositionPlanCost {
    CompositionPlanCost {
        native_queries: 1,
        materialized_intermediate_relations: 0,
        path_nodes: path_nodes.saturating_add(1),
        stable_tiebreak: "fused-witness-join".to_owned(),
    }
}

fn program_digest(program: &CorrespondenceProgram) -> String {
    digest_text("gmeow-correspondence-program-v1", &program.content_key())
}

fn rule_catalog_digest() -> String {
    let mut digest = StableDigest::new("gmeow-correspondence-physical-rules-v1");
    digest.text("rule.id", RULE_ID);
    digest.text("rule.theorem", RULE_THEOREM);
    digest.finish()
}

fn optimization_scope(
    program: &CorrespondenceProgram,
    declaration: &CorrespondenceComposition,
    correspondences: [&Correspondence; 3],
    paths: [&LegPath; 3],
) -> CompositionOptimizationScope {
    let mut context = StableDigest::new("gmeow-correspondence-composition-context-v1");
    let mut effects = StableDigest::new("gmeow-correspondence-composition-effects-v1");
    let mut complements = StableDigest::new("gmeow-correspondence-composition-complements-v1");
    let mut preservation = StableDigest::new("gmeow-correspondence-composition-preservation-v1");
    effects.text("effect-contract", "pure-read-property-path-v1");
    preservation.text("program-preservation", program.preservation.as_str());
    for (index, (correspondence, path)) in correspondences.iter().zip(paths).enumerate() {
        let prefix = format!("stage.{index}");
        context.text(&format!("{prefix}.iri"), &correspondence.iri);
        context.text(
            &format!("{prefix}.source"),
            correspondence.source_endpoint.as_deref().unwrap_or(""),
        );
        context.text(
            &format!("{prefix}.target"),
            correspondence.target_endpoint.as_deref().unwrap_or(""),
        );
        context.text(
            &format!("{prefix}.standpoint"),
            correspondence.according_to.as_deref().unwrap_or(""),
        );
        context.text(
            &format!("{prefix}.determinacy"),
            correspondence
                .determinacy
                .map(|value| value.as_str())
                .unwrap_or(""),
        );
        effects.text(&format!("{prefix}.path"), &leg_path_canonical(path));

        complements.text(&format!("{prefix}.iri"), &correspondence.iri);
        complements.usize(
            &format!("{prefix}.recovery.len"),
            correspondence.recovery_cases.len(),
        );
        for case in &correspondence.recovery_cases {
            complements.text(&format!("{prefix}.recovery"), &case.content_key());
        }
        complements.usize(
            &format!("{prefix}.caveats.len"),
            correspondence.caveats.len(),
        );
        for caveat in &correspondence.caveats {
            complements.text(&format!("{prefix}.caveat.iri"), &caveat.iri);
            complements.usize(
                &format!("{prefix}.caveat.comments.len"),
                caveat.comments.len(),
            );
            for comment in &caveat.comments {
                complements.text(
                    &format!("{prefix}.caveat.comment.lexical"),
                    &comment.lexical_form,
                );
                complements.text(
                    &format!("{prefix}.caveat.comment.datatype"),
                    comment.datatype_iri(),
                );
                complements.text(
                    &format!("{prefix}.caveat.comment.language"),
                    comment.language.as_deref().unwrap_or(""),
                );
                complements.text(
                    &format!("{prefix}.caveat.comment.direction"),
                    comment.direction.map(|value| value.as_str()).unwrap_or(""),
                );
            }
        }
        complements.usize(
            &format!("{prefix}.loss.len"),
            correspondence.loss_evidence.len(),
        );
        for loss in &correspondence.loss_evidence {
            complements.text(&format!("{prefix}.loss.lexical"), &loss.lexical_form);
            complements.text(&format!("{prefix}.loss.datatype"), loss.datatype_iri());
            complements.text(
                &format!("{prefix}.loss.language"),
                loss.language.as_deref().unwrap_or(""),
            );
            complements.text(
                &format!("{prefix}.loss.direction"),
                loss.direction.map(|value| value.as_str()).unwrap_or(""),
            );
        }

        preservation.text(
            &format!("{prefix}.relation"),
            correspondence.relation.as_str(),
        );
        preservation.text(
            &format!("{prefix}.class"),
            correspondence.morphism_class.as_str(),
        );
        preservation.text(
            &format!("{prefix}.kind"),
            correspondence.morphism_kind.as_str(),
        );
        preservation.boolean(
            &format!("{prefix}.mnemomorphic"),
            correspondence.mnemomorphic,
        );
        preservation.text(
            &format!("{prefix}.preservation"),
            correspondence
                .preservation
                .map(|value| value.as_str())
                .unwrap_or(""),
        );
        preservation.usize(
            &format!("{prefix}.laws.len"),
            correspondence.law_claims.len(),
        );
        for law in &correspondence.law_claims {
            preservation.text(&format!("{prefix}.law"), &law.sort_key());
        }
    }
    let mut contract = StableDigest::new("gmeow-correspondence-reasoning-contract-v1");
    contract.text(
        "engine",
        &crate::runtime::EngineContract::current().descriptor_hash,
    );
    contract.text("rule-catalog", &rule_catalog_digest());
    contract.text("output-contract", OUTPUT_CONTRACT);
    CompositionOptimizationScope {
        program_digest: program_digest(program),
        declaration_digest: digest_text(
            "gmeow-correspondence-composition-declaration-v1",
            &declaration.content_key(),
        ),
        context_digest: context.finish(),
        effect_digest: effects.finish(),
        complement_digest: complements.finish(),
        preservation_digest: preservation.finish(),
        reasoning_contract_digest: contract.finish(),
    }
}

fn certificate_digest(certificate: &CompositionPlanCertificate) -> String {
    let mut digest = StableDigest::new("gmeow-correspondence-composition-certificate-v1");
    digest.text("version", &certificate.version);
    digest.text("declaration", &certificate.declaration);
    digest.text("scope.program", &certificate.scope.program_digest);
    digest.text("scope.declaration", &certificate.scope.declaration_digest);
    digest.text("scope.context", &certificate.scope.context_digest);
    digest.text("scope.effects", &certificate.scope.effect_digest);
    digest.text("scope.complements", &certificate.scope.complement_digest);
    digest.text("scope.preservation", &certificate.scope.preservation_digest);
    digest.text(
        "scope.reasoning-contract",
        &certificate.scope.reasoning_contract_digest,
    );
    digest_stage(&mut digest, "first", &certificate.first);
    digest_stage(&mut digest, "second", &certificate.second);
    digest_stage(&mut digest, "composite", &certificate.composite);
    digest.text("output-contract", &certificate.output_contract);
    match &certificate.selected {
        CompositionPhysicalPlan::Original { reason } => {
            digest.text("selected.kind", "original");
            digest.text("selected.reason", (*reason).id());
        }
        CompositionPhysicalPlan::FusedWitnessJoin => {
            digest.text("selected.kind", "fused-witness-join");
            digest.text("selected.reason", "");
        }
    }
    digest.usize("rules.len", certificate.applied_rules.len());
    for rule in &certificate.applied_rules {
        digest.text("rule", rule);
    }
    digest.text("rule-catalog", &certificate.rule_catalog_digest);
    digest_cost(&mut digest, "original-cost", &certificate.original_cost);
    digest_cost(&mut digest, "selected-cost", &certificate.selected_cost);
    digest.usize("limits.path-nodes", certificate.limits.max_path_nodes);
    digest.text("engine", &certificate.engine_descriptor_hash);
    digest.finish()
}

fn digest_stage(digest: &mut StableDigest, label: &str, stage: &CompositionLogicalStage) {
    digest.text(&format!("{label}.correspondence"), &stage.correspondence);
    digest.text(&format!("{label}.get-leg"), &stage.get_leg);
    digest.text(&format!("{label}.path"), &stage.path);
    digest.text(&format!("{label}.source"), &stage.source_endpoint);
    digest.text(&format!("{label}.target"), &stage.target_endpoint);
    digest.text(
        &format!("{label}.standpoint"),
        stage.standpoint.as_deref().unwrap_or(""),
    );
    digest.usize(&format!("{label}.checks.len"), stage.checks.len());
    for &check in &stage.checks {
        digest.text(&format!("{label}.check"), check.id());
    }
}

fn digest_cost(digest: &mut StableDigest, label: &str, cost: &CompositionPlanCost) {
    digest.usize(&format!("{label}.queries"), cost.native_queries);
    digest.usize(
        &format!("{label}.intermediates"),
        cost.materialized_intermediate_relations,
    );
    digest.usize(&format!("{label}.path-nodes"), cost.path_nodes);
    digest.text(&format!("{label}.tiebreak"), &cost.stable_tiebreak);
}

fn digest_text(domain: &str, value: &str) -> String {
    let mut digest = StableDigest::new(domain);
    digest.text("value", value);
    digest.finish()
}
