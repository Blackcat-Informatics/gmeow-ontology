// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Certified projection and witnessed recovery of canonical `logic:Plan` skeletons.
//!
//! A projection contains planned steps and control edges, never execution events.
//! Fixed authored loops may be explicitly unrolled within a caller bound. An
//! observed record is a separate input and contributes only occurrences that
//! carry both `logic:instantiatesPlan` and `logic:instantiatesSchema` witnesses.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use purrdf::{CanonHash, RdfDataset, RdfLiteral, RdfQuad, RdfTerm};
use serde::{Deserialize, Serialize};

use super::stable_digest::StableDigest;
use super::{exec_error, term_key};

#[cfg(test)]
mod tests;

const VERSION: &str = "gmeow-plan-projection-v1";
const OUTPUT_CONTRACT: &str = "planned-skeleton-not-observed-execution-v1";
const RECOVERY_VERSION: &str = "gmeow-plan-recovery-v1";
const RECOVERY_OUTPUT_CONTRACT: &str = "witnessed-planned-skeleton-only-v1";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

fn logic(local: &str) -> String {
    format!("{LOGIC}{local}")
}

fn gmeow(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// Deterministic admission limits. A bound miss refuses projection; no loop,
/// list or branch is silently truncated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanProjectionLimits {
    pub max_unroll: usize,
    pub max_list_items: usize,
    pub max_reachable_guard_nodes: usize,
    pub max_source_facts: usize,
}

impl Default for PlanProjectionLimits {
    fn default() -> Self {
        Self {
            max_unroll: 1_024,
            max_list_items: 16_384,
            max_reachable_guard_nodes: 16_384,
            max_source_facts: 1_048_576,
        }
    }
}

/// A projected node remains prescriptive. It is not an event or an assertion
/// that the action occurred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlannedStepKind {
    Action,
    Guard,
}

impl PlannedStepKind {
    fn id(self) -> &'static str {
        match self {
            Self::Action => "action",
            Self::Guard => "guard",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedStep {
    pub id: String,
    pub cycle: usize,
    pub position: usize,
    pub source_node: String,
    pub kind: PlannedStepKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlannedEdgeKind {
    Serial,
    GuardThen,
    GuardElse,
    NextIteration,
}

impl PlannedEdgeKind {
    fn id(self) -> &'static str {
        match self {
            Self::Serial => "serial",
            Self::GuardThen => "guard-then",
            Self::GuardElse => "guard-else",
            Self::NextIteration => "next-iteration",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedEdge {
    pub from: String,
    pub to: String,
    pub kind: PlannedEdgeKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectedOutcome {
    pub source_node: String,
    pub case: String,
    pub continuation: Option<String>,
    pub compensation: Option<String>,
}

/// One normalized fact in a condition or effect closure. The lexical object is
/// the complete RDF term rendering used by the native carrier, not an IRI-only
/// approximation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProjectedExpressionFact {
    pub subject: String,
    pub predicate: String,
    pub object: String,
}

/// A condition/effect root with the exact bounded fact closure that gives it
/// meaning in the selected source graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectedExpression {
    pub root: String,
    pub digest: String,
    pub facts: Vec<ProjectedExpressionFact>,
    pub tracked_states: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectedConditionalStep {
    pub source_node: String,
    pub guard: ProjectedExpression,
    pub then_steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectedAction {
    pub schema: String,
    pub preconditions: Vec<ProjectedExpression>,
    pub effects: Vec<ProjectedExpression>,
    pub resources: Vec<String>,
    pub capabilities: Vec<String>,
    pub observations: Vec<String>,
    pub outcomes: Vec<ProjectedOutcome>,
    pub conditional_steps: Vec<ProjectedConditionalStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectedGuard {
    pub guard: String,
    pub condition: ProjectedExpression,
    pub then_steps: Vec<String>,
    pub else_steps: Vec<String>,
}

/// Complete RDF literal identity on a portable serde surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanLiteral {
    pub lexical_form: String,
    pub datatype: String,
    pub language: Option<String>,
    pub direction: Option<String>,
}

impl From<RdfLiteral> for PlanLiteral {
    fn from(value: RdfLiteral) -> Self {
        let datatype = value.datatype_iri().to_owned();
        let direction = value
            .direction
            .map(|direction| direction.as_str().to_owned());
        Self {
            lexical_form: value.lexical_form,
            datatype,
            language: value.language,
            direction,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FreshnessRequirement {
    pub tracked_state: String,
    pub currency: PlanLiteral,
}

/// One exact normalized fact in the admitted source carrier. Graph placement is
/// explicit, and `subject`/`object` retain RDF-star quoted terms.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProjectedCarrierFact {
    pub graph: Option<String>,
    pub subject: String,
    pub predicate: String,
    pub object: String,
}

/// The complement needed to interpret and recover the planned skeleton.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanProjectionComplement {
    pub source_digest: String,
    pub source_graph: Option<String>,
    pub plan: String,
    pub standpoint: Option<String>,
    pub loop_body: Vec<String>,
    pub expression_digests: Vec<String>,
    pub source_facts: Vec<ProjectedCarrierFact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanProjectionCertificate {
    pub version: String,
    pub output_contract: String,
    pub plan: String,
    pub source_digest: String,
    pub projection_digest: String,
    pub certificate_digest: String,
}

/// Certified prescriptive projection. `steps` are projected plan positions, not
/// observed occurrences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanProjection {
    pub plan: String,
    pub source_graph: Option<String>,
    pub standpoint: Option<String>,
    pub preconditions: Vec<ProjectedExpression>,
    pub loop_count: usize,
    pub loop_variable: Option<PlanLiteral>,
    pub loop_period: Option<PlanLiteral>,
    pub loop_body: Vec<String>,
    pub steps: Vec<PlannedStep>,
    pub edges: Vec<PlannedEdge>,
    pub actions: Vec<ProjectedAction>,
    pub guards: Vec<ProjectedGuard>,
    pub freshness: Vec<FreshnessRequirement>,
    pub loss_evidence: Vec<String>,
    pub complement: PlanProjectionComplement,
    pub certificate: PlanProjectionCertificate,
}

/// Recoverable evidence is an observed resource already present in caller data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedPlanOccurrence {
    pub occurrence: String,
    pub graph: Option<String>,
    pub schema: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlanRecoveryVerdict {
    PlannedSkeletonRecovered,
}

impl PlanRecoveryVerdict {
    fn id(self) -> &'static str {
        match self {
            Self::PlannedSkeletonRecovered => "planned-skeleton-recovered",
        }
    }
}

/// Recovery of the planned schema skeleton. It never claims to reconstruct the
/// real execution history or occurrence order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanRecovery {
    pub version: String,
    pub output_contract: String,
    pub plan: String,
    pub projection_certificate_digest: String,
    pub observed_input_digest: String,
    pub verdict: PlanRecoveryVerdict,
    pub occurrences: Vec<ObservedPlanOccurrence>,
    pub recovered_schemas: Vec<String>,
    pub unobserved_planned_schemas: Vec<String>,
    pub off_plan_occurrences: Vec<String>,
    pub loss_evidence: Vec<String>,
    pub recovery_digest: String,
}

#[derive(Clone)]
struct Fact {
    subject: String,
    predicate: String,
    object: RdfTerm,
    graph: Option<String>,
}

impl Fact {
    fn from_quad(quad: RdfQuad) -> Self {
        Self {
            subject: term_key(&quad.subject),
            predicate: quad.predicate,
            object: quad.object,
            graph: quad.graph_name.as_ref().map(term_key),
        }
    }
}

struct Facts {
    graph: Option<String>,
    by_sp: BTreeMap<(String, String), Vec<RdfTerm>>,
}

impl Facts {
    fn select(all: &[Fact], plan: &str, graph: Option<&str>) -> gmeow_errors::Result<Self> {
        let plan_class = logic("Plan");
        let mut candidates: BTreeSet<Option<String>> = all
            .iter()
            .filter(|fact| {
                fact.subject == plan
                    && fact.predicate == RDF_TYPE
                    && iri_value(&fact.object) == Some(plan_class.as_str())
            })
            .map(|fact| fact.graph.clone())
            .collect();
        if let Some(graph) = graph {
            candidates.retain(|candidate| candidate.as_deref() == Some(graph));
        }
        let selected = match candidates.len() {
            1 => candidates.into_iter().next().expect("one graph"),
            0 => {
                return Err(exec_error(format!(
                    "selected logic:Plan <{plan}> is absent from the requested graph"
                )));
            }
            count => {
                return Err(exec_error(format!(
                    "selected logic:Plan <{plan}> occurs in {count} graphs; select one exact graph"
                )));
            }
        };
        let mut by_sp: BTreeMap<(String, String), Vec<RdfTerm>> = BTreeMap::new();
        for fact in all.iter().filter(|fact| fact.graph == selected) {
            by_sp
                .entry((fact.subject.clone(), fact.predicate.clone()))
                .or_default()
                .push(fact.object.clone());
        }
        for values in by_sp.values_mut() {
            values.sort_by_key(term_key);
            values.dedup();
        }
        Ok(Self {
            graph: selected,
            by_sp,
        })
    }

    fn values(&self, subject: &str, predicate: &str) -> &[RdfTerm] {
        self.by_sp
            .get(&(subject.to_owned(), predicate.to_owned()))
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    fn resources(&self, subject: &str, predicate: &str) -> Vec<String> {
        self.values(subject, predicate)
            .iter()
            .filter_map(resource_key)
            .collect()
    }

    fn one_resource(&self, subject: &str, predicate: &str) -> gmeow_errors::Result<String> {
        let values = self.resources(subject, predicate);
        match values.as_slice() {
            [value] => Ok(value.clone()),
            _ => Err(exec_error(format!(
                "<{subject}> requires exactly one resource <{predicate}>; found {}",
                values.len()
            ))),
        }
    }

    fn optional_resource(
        &self,
        subject: &str,
        predicate: &str,
    ) -> gmeow_errors::Result<Option<String>> {
        let values = self.resources(subject, predicate);
        match values.as_slice() {
            [] => Ok(None),
            [value] => Ok(Some(value.clone())),
            _ => Err(exec_error(format!(
                "<{subject}> has ambiguous <{predicate}> resources"
            ))),
        }
    }

    fn one_literal(&self, subject: &str, predicate: &str) -> gmeow_errors::Result<RdfLiteral> {
        let values: Vec<_> = self
            .values(subject, predicate)
            .iter()
            .filter_map(|value| match value {
                RdfTerm::Literal(literal) => Some(literal.clone()),
                _ => None,
            })
            .collect();
        match values.as_slice() {
            [value] => Ok(value.clone()),
            _ => Err(exec_error(format!(
                "<{subject}> requires exactly one literal <{predicate}>; found {}",
                values.len()
            ))),
        }
    }

    fn optional_literal(
        &self,
        subject: &str,
        predicate: &str,
    ) -> gmeow_errors::Result<Option<RdfLiteral>> {
        let values: Vec<_> = self
            .values(subject, predicate)
            .iter()
            .filter_map(|value| match value {
                RdfTerm::Literal(literal) => Some(literal.clone()),
                _ => None,
            })
            .collect();
        match values.as_slice() {
            [] => Ok(None),
            [value] => Ok(Some(value.clone())),
            _ => Err(exec_error(format!(
                "<{subject}> has ambiguous <{predicate}> literals"
            ))),
        }
    }

    fn has_type(&self, subject: &str, class: &str) -> bool {
        self.values(subject, RDF_TYPE)
            .iter()
            .any(|value| iri_value(value) == Some(class))
    }

    fn list(&self, head: &str, limit: usize) -> gmeow_errors::Result<Vec<String>> {
        let mut node = head.to_owned();
        let mut seen = BTreeSet::new();
        let mut values = Vec::new();
        while node != RDF_NIL {
            if !seen.insert(node.clone()) {
                return Err(exec_error("RDF list contains a cycle"));
            }
            if values.len() >= limit {
                return Err(exec_error("RDF list admission limit exceeded"));
            }
            values.push(self.one_resource(&node, RDF_FIRST)?);
            node = self.one_resource(&node, RDF_REST)?;
        }
        Ok(values)
    }

    fn branch_items(&self, node: &str, limit: usize) -> gmeow_errors::Result<Vec<String>> {
        if !self.values(node, RDF_FIRST).is_empty() {
            self.list(node, limit)
        } else {
            Ok(vec![node.to_owned()])
        }
    }
}

fn resource_key(value: &RdfTerm) -> Option<String> {
    match value {
        RdfTerm::Iri(_) | RdfTerm::BlankNode(_) => Some(term_key(value)),
        RdfTerm::Literal(_) | RdfTerm::Triple(_) => None,
    }
}

fn iri_value(value: &RdfTerm) -> Option<&str> {
    match value {
        RdfTerm::Iri(iri) => Some(iri),
        RdfTerm::BlankNode(_) | RdfTerm::Literal(_) | RdfTerm::Triple(_) => None,
    }
}

fn input_digest(dataset: &RdfDataset) -> gmeow_errors::Result<String> {
    Ok(purrdf::try_flat_digest_view(dataset, CanonHash::Sha256)
        .map_err(|error| exec_error(format!("canonicalize plan input: {error}")))?
        .to_hex())
}

/// Project one canonical plan. A fixed `logic:loopCount` is an explicit bounded
/// unrolling contract; an absent/unbounded loop or a bound miss hard-fails.
pub fn project_plan(
    dataset: &RdfDataset,
    plan: &str,
    graph: Option<&str>,
    limits: PlanProjectionLimits,
) -> gmeow_errors::Result<PlanProjection> {
    let projection = build_plan_projection(dataset, plan, graph, limits)?;
    verify_plan_projection(&projection, dataset, graph, limits)?;
    Ok(projection)
}

fn build_plan_projection(
    dataset: &RdfDataset,
    plan: &str,
    graph: Option<&str>,
    limits: PlanProjectionLimits,
) -> gmeow_errors::Result<PlanProjection> {
    let all: Vec<_> = purrdf::native_quads::flat_rdf_quads(dataset)
        .map(Fact::from_quad)
        .collect();
    if all.len() > limits.max_source_facts {
        return Err(exec_error(format!(
            "plan source carrier has {} facts, exceeding the explicit {}-fact admission contract",
            all.len(),
            limits.max_source_facts
        )));
    }
    let mut source_facts: Vec<_> = all
        .iter()
        .map(|fact| ProjectedCarrierFact {
            graph: fact.graph.clone(),
            subject: fact.subject.clone(),
            predicate: fact.predicate.clone(),
            object: term_key(&fact.object),
        })
        .collect();
    source_facts.sort();
    source_facts.dedup();
    let facts = Facts::select(&all, plan, graph)?;
    let source_digest = input_digest(dataset)?;
    let standpoint = facts.optional_resource(plan, &gmeow("accordingTo"))?;
    let precondition_roots = facts.resources(plan, &logic("precondition"));
    let body = facts.one_resource(plan, &logic("body"))?;
    if !facts.has_type(&body, &logic("Loop")) {
        return Err(exec_error(format!(
            "logic:Plan <{plan}> body is not an explicitly bounded logic:Loop"
        )));
    }
    let loop_count_literal = facts.one_literal(&body, &logic("loopCount"))?;
    let loop_count: usize = loop_count_literal.lexical_form.parse().map_err(|error| {
        exec_error(format!(
            "logic:Loop <{body}> has invalid loopCount {:?}: {error}",
            loop_count_literal.lexical_form
        ))
    })?;
    if loop_count == 0 || loop_count > limits.max_unroll {
        return Err(exec_error(format!(
            "logic:Loop <{body}> count {loop_count} is outside the explicit 1..={} unrolling contract",
            limits.max_unroll
        )));
    }
    let loop_variable = facts
        .optional_literal(&body, &logic("loopVar"))?
        .map(PlanLiteral::from);
    let loop_period = facts
        .optional_literal(&body, &logic("loopPeriod"))?
        .map(PlanLiteral::from);
    let body_list = facts.one_resource(&body, &logic("loopBody"))?;
    let loop_body = facts.list(&body_list, limits.max_list_items)?;
    if loop_body.is_empty() {
        return Err(exec_error("bounded plan loop has an empty body"));
    }

    let freshness = freshness_requirements(&facts)?;
    let tracked: BTreeSet<_> = freshness
        .iter()
        .map(|requirement| requirement.tracked_state.clone())
        .collect();
    let preconditions = project_expressions(
        &facts,
        &precondition_roots,
        &tracked,
        limits.max_reachable_guard_nodes,
    )?;
    let mut builder = ProjectionBuilder::new(plan, &facts, &tracked, limits);
    let mut previous_exits: Vec<String> = Vec::new();
    for cycle in 0..loop_count {
        let (starts, exits) = builder.expand_plan_list(&loop_body, cycle)?;
        if cycle > 0 {
            for from in &previous_exits {
                for to in &starts {
                    builder.edge(from, to, PlannedEdgeKind::NextIteration);
                }
            }
        }
        previous_exits = exits;
    }
    builder.finish_edges();
    let mut actions: Vec<_> = builder.actions.into_values().collect();
    actions.sort_by(|left, right| left.schema.cmp(&right.schema));
    let mut guards: Vec<_> = builder.guards.into_values().collect();
    guards.sort_by(|left, right| left.guard.cmp(&right.guard));
    let loss_evidence = vec![format!(
        "The authored loop is projected by the selected fixed-count bounded-unrolling contract ({loop_count} iterations); the result is a prescriptive skeleton and does not assert that any step occurred."
    )];
    let mut expression_digests: Vec<_> = preconditions
        .iter()
        .map(|expression| expression.digest.clone())
        .collect();
    for action in &actions {
        expression_digests.extend(
            action
                .preconditions
                .iter()
                .chain(&action.effects)
                .map(|expression| expression.digest.clone()),
        );
        expression_digests.extend(
            action
                .conditional_steps
                .iter()
                .map(|step| step.guard.digest.clone()),
        );
    }
    expression_digests.extend(guards.iter().map(|guard| guard.condition.digest.clone()));
    expression_digests.sort();
    expression_digests.dedup();
    let complement = PlanProjectionComplement {
        source_digest: source_digest.clone(),
        source_graph: facts.graph.clone(),
        plan: plan.to_owned(),
        standpoint: standpoint.clone(),
        loop_body: loop_body.clone(),
        expression_digests,
        source_facts,
    };
    let mut projection = PlanProjection {
        plan: plan.to_owned(),
        source_graph: facts.graph.clone(),
        standpoint,
        preconditions,
        loop_count,
        loop_variable,
        loop_period,
        loop_body,
        steps: builder.steps,
        edges: builder.edges,
        actions,
        guards,
        freshness,
        loss_evidence,
        complement,
        certificate: PlanProjectionCertificate {
            version: VERSION.to_owned(),
            output_contract: OUTPUT_CONTRACT.to_owned(),
            plan: plan.to_owned(),
            source_digest,
            projection_digest: String::new(),
            certificate_digest: String::new(),
        },
    };
    projection.certificate.projection_digest = projection_digest(&projection);
    projection.certificate.certificate_digest = certificate_digest(&projection.certificate);
    Ok(projection)
}

/// Re-project from the exact source and compare the complete typed result.
pub fn verify_plan_projection(
    projection: &PlanProjection,
    dataset: &RdfDataset,
    graph: Option<&str>,
    limits: PlanProjectionLimits,
) -> gmeow_errors::Result<()> {
    verify_plan_projection_certificate(projection)?;
    if projection.certificate.source_digest != input_digest(dataset)? {
        return Err(exec_error(
            "plan projection certificate is bound to a different canonical source",
        ));
    }
    let rebuilt = build_plan_projection(dataset, &projection.plan, graph, limits)?;
    if rebuilt != *projection {
        return Err(exec_error(
            "plan projection differs from the exact canonical source",
        ));
    }
    Ok(())
}

/// Verify every self-contained binding in a portable projection certificate.
/// Source authenticity still requires [`verify_plan_projection`] with the
/// referenced canonical dataset.
pub fn verify_plan_projection_certificate(projection: &PlanProjection) -> gmeow_errors::Result<()> {
    if projection.certificate.version != VERSION
        || projection.certificate.output_contract != OUTPUT_CONTRACT
        || projection.certificate.plan != projection.plan
        || projection.complement.plan != projection.plan
        || projection.certificate.source_digest != projection.complement.source_digest
        || projection.source_graph != projection.complement.source_graph
        || projection.standpoint != projection.complement.standpoint
        || projection.certificate.projection_digest != projection_digest(projection)
        || projection.certificate.certificate_digest != certificate_digest(&projection.certificate)
    {
        return Err(exec_error(
            "plan projection certificate is incomplete, corrupt or rebound",
        ));
    }
    Ok(())
}

struct ProjectionBuilder<'a> {
    plan: &'a str,
    facts: &'a Facts,
    tracked: &'a BTreeSet<String>,
    limits: PlanProjectionLimits,
    position: usize,
    steps: Vec<PlannedStep>,
    edges: Vec<PlannedEdge>,
    edge_set: BTreeSet<(String, String, PlannedEdgeKind)>,
    actions: BTreeMap<String, ProjectedAction>,
    guards: BTreeMap<String, ProjectedGuard>,
    active_plans: BTreeSet<String>,
}

impl<'a> ProjectionBuilder<'a> {
    fn new(
        plan: &'a str,
        facts: &'a Facts,
        tracked: &'a BTreeSet<String>,
        limits: PlanProjectionLimits,
    ) -> Self {
        Self {
            plan,
            facts,
            tracked,
            limits,
            position: 0,
            steps: Vec::new(),
            edges: Vec::new(),
            edge_set: BTreeSet::new(),
            actions: BTreeMap::new(),
            guards: BTreeMap::new(),
            active_plans: BTreeSet::new(),
        }
    }

    fn step(&mut self, source: &str, kind: PlannedStepKind, cycle: usize) -> String {
        let position = self.position;
        self.position += 1;
        let id = format!("{}/projection/cycle/{cycle}/step/{position}", self.plan);
        self.steps.push(PlannedStep {
            id: id.clone(),
            cycle,
            position,
            source_node: source.to_owned(),
            kind,
        });
        id
    }

    fn edge(&mut self, from: &str, to: &str, kind: PlannedEdgeKind) {
        self.edge_set.insert((from.to_owned(), to.to_owned(), kind));
    }

    fn finish_edges(&mut self) {
        self.edges = std::mem::take(&mut self.edge_set)
            .into_iter()
            .map(|(from, to, kind)| PlannedEdge { from, to, kind })
            .collect();
    }

    fn expand_plan_list(
        &mut self,
        plans: &[String],
        cycle: usize,
    ) -> gmeow_errors::Result<(Vec<String>, Vec<String>)> {
        let mut starts: Vec<String> = Vec::new();
        let mut exits: Vec<String> = Vec::new();
        for plan in plans {
            if !self.active_plans.insert(plan.clone()) {
                return Err(exec_error(format!(
                    "recursive plan reference <{plan}> requires an explicit bounded-unrolling contract"
                )));
            }
            let serial_head = self.facts.one_resource(plan, &logic("serial"))?;
            let members = self.facts.list(&serial_head, self.limits.max_list_items)?;
            let sequence = self.expand_sequence(&members, cycle)?;
            self.active_plans.remove(plan);
            if starts.is_empty() {
                starts = sequence.0.clone();
            }
            for from in &exits {
                for to in &sequence.0 {
                    self.edge(from, to, PlannedEdgeKind::Serial);
                }
            }
            exits = sequence.1;
        }
        Ok((starts, exits))
    }

    fn expand_sequence(
        &mut self,
        members: &[String],
        cycle: usize,
    ) -> gmeow_errors::Result<(Vec<String>, Vec<String>)> {
        if members.is_empty() {
            return Err(exec_error("logic:serial list is empty"));
        }
        let mut starts: Vec<String> = Vec::new();
        let mut exits: Vec<String> = Vec::new();
        for member in members {
            let (member_starts, member_exits) =
                if self.facts.has_type(member, &logic("GuardedBranch")) {
                    self.expand_guard(member, cycle)?
                } else if self.facts.has_type(member, &logic("Plan")) {
                    self.expand_plan_list(std::slice::from_ref(member), cycle)?
                } else {
                    self.expand_action(member, cycle)?
                };
            if starts.is_empty() {
                starts = member_starts.clone();
            }
            for from in &exits {
                for to in &member_starts {
                    self.edge(from, to, PlannedEdgeKind::Serial);
                }
            }
            exits = member_exits;
        }
        Ok((starts, exits))
    }

    fn expand_action(
        &mut self,
        schema: &str,
        cycle: usize,
    ) -> gmeow_errors::Result<(Vec<String>, Vec<String>)> {
        self.admit_action(schema)?;
        let id = self.step(schema, PlannedStepKind::Action, cycle);
        Ok((vec![id.clone()], vec![id]))
    }

    fn admit_action(&mut self, schema: &str) -> gmeow_errors::Result<()> {
        if self.actions.contains_key(schema) {
            return Ok(());
        }
        let action = read_action(self.facts, schema, self.tracked, self.limits)?;
        let mut referenced = Vec::new();
        for outcome in &action.outcomes {
            referenced.extend(outcome.continuation.iter().cloned());
            referenced.extend(outcome.compensation.iter().cloned());
        }
        for conditional in &action.conditional_steps {
            referenced.extend(conditional.then_steps.iter().cloned());
        }
        self.actions.insert(schema.to_owned(), action);
        for referenced in referenced {
            if self.facts.has_type(&referenced, &logic("ActionSchema")) {
                self.admit_action(&referenced)?;
            }
        }
        Ok(())
    }

    fn expand_guard(
        &mut self,
        guard: &str,
        cycle: usize,
    ) -> gmeow_errors::Result<(Vec<String>, Vec<String>)> {
        let condition = self.facts.one_resource(guard, &logic("guard"))?;
        let then_root = self.facts.one_resource(guard, &logic("then"))?;
        let else_root = self.facts.one_resource(guard, &logic("else"))?;
        let then_steps = self
            .facts
            .branch_items(&then_root, self.limits.max_list_items)?;
        let else_steps = self
            .facts
            .branch_items(&else_root, self.limits.max_list_items)?;
        let condition = project_expression(
            self.facts,
            &condition,
            self.tracked,
            self.limits.max_reachable_guard_nodes,
        )?;
        let value = ProjectedGuard {
            guard: guard.to_owned(),
            condition,
            then_steps: then_steps.clone(),
            else_steps: else_steps.clone(),
        };
        if let Some(previous) = self.guards.insert(guard.to_owned(), value.clone())
            && previous != value
        {
            return Err(exec_error(format!(
                "guard <{guard}> changed between projected iterations"
            )));
        }
        let guard_id = self.step(guard, PlannedStepKind::Guard, cycle);
        let (then_starts, then_exits) = self.expand_sequence(&then_steps, cycle)?;
        let (else_starts, else_exits) = self.expand_sequence(&else_steps, cycle)?;
        for to in &then_starts {
            self.edge(&guard_id, to, PlannedEdgeKind::GuardThen);
        }
        for to in &else_starts {
            self.edge(&guard_id, to, PlannedEdgeKind::GuardElse);
        }
        let mut exits = then_exits;
        exits.extend(else_exits);
        exits.sort();
        exits.dedup();
        Ok((vec![guard_id], exits))
    }
}

fn read_action(
    facts: &Facts,
    schema: &str,
    tracked: &BTreeSet<String>,
    limits: PlanProjectionLimits,
) -> gmeow_errors::Result<ProjectedAction> {
    if !facts.has_type(schema, &logic("ActionSchema")) {
        return Err(exec_error(format!(
            "planned action <{schema}> is not typed logic:ActionSchema"
        )));
    }
    let preconditions = project_expressions(
        facts,
        &facts.resources(schema, &logic("precondition")),
        tracked,
        limits.max_reachable_guard_nodes,
    )?;
    let effects = project_expressions(
        facts,
        &facts.resources(schema, &logic("effect")),
        tracked,
        limits.max_reachable_guard_nodes,
    )?;
    let mut outcomes = Vec::new();
    for outcome in facts.resources(schema, &logic("outcome")) {
        outcomes.push(ProjectedOutcome {
            source_node: outcome.clone(),
            case: facts.one_resource(&outcome, &logic("outcomeCase"))?,
            continuation: facts.optional_resource(&outcome, &logic("then"))?,
            compensation: facts.optional_resource(&outcome, &logic("compensation"))?,
        });
    }
    outcomes.sort_by(|left, right| {
        (
            &left.source_node,
            &left.case,
            &left.continuation,
            &left.compensation,
        )
            .cmp(&(
                &right.source_node,
                &right.case,
                &right.continuation,
                &right.compensation,
            ))
    });
    let mut conditional_steps = Vec::new();
    for conditional in facts.resources(schema, &logic("conditionalStep")) {
        let guard = facts.one_resource(&conditional, &logic("guard"))?;
        let then = facts.one_resource(&conditional, &logic("then"))?;
        conditional_steps.push(ProjectedConditionalStep {
            source_node: conditional,
            guard: project_expression(facts, &guard, tracked, limits.max_reachable_guard_nodes)?,
            then_steps: facts.branch_items(&then, limits.max_list_items)?,
        });
    }
    conditional_steps.sort_by(|left, right| left.source_node.cmp(&right.source_node));
    conditional_steps.dedup();
    Ok(ProjectedAction {
        schema: schema.to_owned(),
        preconditions,
        effects,
        resources: facts.resources(schema, &logic("resource")),
        capabilities: facts.resources(schema, &logic("capability")),
        observations: facts.resources(schema, &logic("observation")),
        outcomes,
        conditional_steps,
    })
}

fn freshness_requirements(facts: &Facts) -> gmeow_errors::Result<Vec<FreshnessRequirement>> {
    let tracked_class = gmeow("TrackedState");
    let mut subjects: Vec<_> = facts
        .by_sp
        .keys()
        .map(|(subject, _)| subject)
        .filter(|subject| facts.has_type(subject, &tracked_class))
        .cloned()
        .collect();
    subjects.sort();
    subjects.dedup();
    subjects
        .into_iter()
        .map(|tracked_state| {
            Ok(FreshnessRequirement {
                currency: PlanLiteral::from(facts.one_literal(&tracked_state, &logic("currency"))?),
                tracked_state,
            })
        })
        .collect()
}

fn project_expressions(
    facts: &Facts,
    roots: &[String],
    tracked: &BTreeSet<String>,
    limit: usize,
) -> gmeow_errors::Result<Vec<ProjectedExpression>> {
    roots
        .iter()
        .map(|root| project_expression(facts, root, tracked, limit))
        .collect()
}

fn project_expression(
    facts: &Facts,
    root: &str,
    tracked: &BTreeSet<String>,
    limit: usize,
) -> gmeow_errors::Result<ProjectedExpression> {
    let mut queue = VecDeque::from([root.to_owned()]);
    let mut seen = BTreeSet::new();
    let mut rows: Vec<ProjectedExpressionFact> = Vec::new();
    let mut states = BTreeSet::new();
    while let Some(subject) = queue.pop_front() {
        if !seen.insert(subject.clone()) {
            continue;
        }
        if seen.len() > limit {
            return Err(exec_error("guard closure admission limit exceeded"));
        }
        for ((row_subject, predicate), values) in &facts.by_sp {
            if row_subject != &subject {
                continue;
            }
            for value in values {
                let object = term_key(value);
                rows.push(ProjectedExpressionFact {
                    subject: row_subject.clone(),
                    predicate: predicate.clone(),
                    object: object.clone(),
                });
                if tracked.contains(&object) {
                    states.insert(object.clone());
                }
                if !tracked.contains(&object)
                    && (matches!(value, RdfTerm::BlankNode(_))
                        || resource_key(value).is_some_and(|resource| {
                            facts.has_type(&resource, &logic("Formula"))
                                || !facts.values(&resource, RDF_FIRST).is_empty()
                        }))
                {
                    queue.push_back(object);
                }
            }
        }
    }
    if rows.is_empty() {
        return Err(exec_error(format!(
            "expression root <{root}> has no meaning in the selected source graph"
        )));
    }
    rows.sort();
    rows.dedup();
    let mut digest = StableDigest::new("gmeow-plan-guard-v1");
    digest.usize("fact.count", rows.len());
    for (index, fact) in rows.iter().enumerate() {
        let prefix = format!("row.{index}");
        digest.text(&format!("{prefix}.subject"), &fact.subject);
        digest.text(&format!("{prefix}.predicate"), &fact.predicate);
        digest.text(&format!("{prefix}.object"), &fact.object);
    }
    Ok(ProjectedExpression {
        root: root.to_owned(),
        digest: digest.finish(),
        facts: rows,
        tracked_states: states.into_iter().collect(),
    })
}

/// Recover only the schema skeleton witnessed by an observed record. Missing or
/// ambiguous in-band witnesses hard-fail. Off-plan occurrences are retained as
/// explicit loss and never rewritten into planned steps.
pub fn recover_planned_schema_skeleton(
    projection: &PlanProjection,
    observed: &RdfDataset,
) -> gmeow_errors::Result<PlanRecovery> {
    let recovery = build_plan_recovery(projection, observed)?;
    verify_plan_recovery(&recovery, projection, observed)?;
    Ok(recovery)
}

fn build_plan_recovery(
    projection: &PlanProjection,
    observed: &RdfDataset,
) -> gmeow_errors::Result<PlanRecovery> {
    verify_plan_projection_certificate(projection)?;
    let facts: Vec<_> = purrdf::native_quads::flat_rdf_quads(observed)
        .map(Fact::from_quad)
        .collect();
    let plan_predicate = logic("instantiatesPlan");
    let schema_predicate = logic("instantiatesSchema");
    let mut selected: BTreeMap<(Option<String>, String), Vec<&Fact>> = BTreeMap::new();
    for fact in &facts {
        if fact.predicate == plan_predicate
            && iri_value(&fact.object) == Some(projection.plan.as_str())
        {
            selected
                .entry((fact.graph.clone(), fact.subject.clone()))
                .or_default()
                .push(fact);
        }
    }
    if selected.is_empty() {
        return Err(exec_error(format!(
            "observed record carries no instantiatesPlan witness for <{}>",
            projection.plan
        )));
    }
    let expected = planned_action_schemas(projection);
    let mut occurrences = Vec::new();
    let mut recovered = BTreeSet::new();
    let mut off_plan = Vec::new();
    for ((graph, occurrence), plan_links) in selected {
        if plan_links.len() != 1 {
            return Err(exec_error(format!(
                "observed occurrence <{occurrence}> has ambiguous instantiatesPlan witnesses"
            )));
        }
        let mut schemas: BTreeSet<_> = facts
            .iter()
            .filter(|fact| {
                fact.graph == graph
                    && fact.subject == occurrence
                    && fact.predicate == schema_predicate
            })
            .filter_map(|fact| iri_value(&fact.object).map(str::to_owned))
            .collect();
        if schemas.len() != 1 {
            return Err(exec_error(format!(
                "observed occurrence <{occurrence}> requires exactly one instantiatesSchema witness"
            )));
        }
        let schema = schemas.pop_first().expect("one schema");
        if expected.contains(&schema) {
            recovered.insert(schema.clone());
        } else {
            off_plan.push(occurrence.clone());
        }
        occurrences.push(ObservedPlanOccurrence {
            occurrence,
            graph,
            schema,
        });
    }
    occurrences.sort_by(|left, right| {
        (&left.graph, &left.occurrence, &left.schema).cmp(&(
            &right.graph,
            &right.occurrence,
            &right.schema,
        ))
    });
    off_plan.sort();
    let unobserved: Vec<_> = expected.difference(&recovered).cloned().collect();
    let mut loss_evidence = Vec::new();
    if !unobserved.is_empty() {
        loss_evidence.push(format!(
            "These planned branch, loop, continuation or compensation schemas were not witnessed by this record; absence is not evidence that an event did not occur: {}.",
            unobserved.join(", ")
        ));
    }
    if !off_plan.is_empty() {
        loss_evidence.push(format!(
            "Observed off-plan occurrences are retained as loss evidence: {}.",
            off_plan.join(", ")
        ));
    }
    loss_evidence.push(
        "Recovery establishes only the planned schema skeleton; it does not reconstruct or fabricate execution order, timing, outcomes, or unreported events."
            .to_owned(),
    );
    let mut recovery = PlanRecovery {
        version: RECOVERY_VERSION.to_owned(),
        output_contract: RECOVERY_OUTPUT_CONTRACT.to_owned(),
        plan: projection.plan.clone(),
        projection_certificate_digest: projection.certificate.certificate_digest.clone(),
        observed_input_digest: input_digest(observed)?,
        verdict: PlanRecoveryVerdict::PlannedSkeletonRecovered,
        occurrences,
        recovered_schemas: recovered.into_iter().collect(),
        unobserved_planned_schemas: unobserved,
        off_plan_occurrences: off_plan,
        loss_evidence,
        recovery_digest: String::new(),
    };
    recovery.recovery_digest = plan_recovery_digest(&recovery);
    Ok(recovery)
}

/// Re-run witnessed recovery over the exact observed carrier and compare the
/// complete result. This verifies the projection binding, observation digest,
/// loss evidence and every occurrence/schema witness.
pub fn verify_plan_recovery(
    recovery: &PlanRecovery,
    projection: &PlanProjection,
    observed: &RdfDataset,
) -> gmeow_errors::Result<()> {
    verify_plan_recovery_receipt(recovery, projection)?;
    if recovery.observed_input_digest != input_digest(observed)? {
        return Err(exec_error(
            "plan recovery receipt is bound to a different observed carrier",
        ));
    }
    let rebuilt = build_plan_recovery(projection, observed)?;
    if rebuilt != *recovery {
        return Err(exec_error(
            "plan recovery differs from the exact witnessed observation",
        ));
    }
    Ok(())
}

/// Verify the portable recovery receipt and its projection binding without
/// claiming that the observed carrier itself was supplied.
pub fn verify_plan_recovery_receipt(
    recovery: &PlanRecovery,
    projection: &PlanProjection,
) -> gmeow_errors::Result<()> {
    verify_plan_projection_certificate(projection)?;
    if recovery.version != RECOVERY_VERSION
        || recovery.output_contract != RECOVERY_OUTPUT_CONTRACT
        || recovery.plan != projection.plan
        || recovery.projection_certificate_digest != projection.certificate.certificate_digest
        || recovery.recovery_digest != plan_recovery_digest(recovery)
    {
        return Err(exec_error(
            "plan recovery receipt is incomplete, corrupt or rebound",
        ));
    }
    Ok(())
}

fn planned_action_schemas(projection: &PlanProjection) -> BTreeSet<String> {
    projection
        .actions
        .iter()
        .map(|action| action.schema.clone())
        .collect()
}

fn plan_recovery_digest(recovery: &PlanRecovery) -> String {
    let mut digest = StableDigest::new("gmeow-plan-recovery-content-v1");
    digest.text("version", &recovery.version);
    digest.text("output-contract", &recovery.output_contract);
    digest.text("plan", &recovery.plan);
    digest.text(
        "projection-certificate",
        &recovery.projection_certificate_digest,
    );
    digest.text("observed-input", &recovery.observed_input_digest);
    digest.text("verdict", recovery.verdict.id());
    digest.usize("occurrence.count", recovery.occurrences.len());
    for (index, occurrence) in recovery.occurrences.iter().enumerate() {
        let prefix = format!("occurrence.{index}");
        digest.text(&format!("{prefix}.occurrence"), &occurrence.occurrence);
        optional_text_digest(
            &mut digest,
            &format!("{prefix}.graph"),
            occurrence.graph.as_deref(),
        );
        digest.text(&format!("{prefix}.schema"), &occurrence.schema);
    }
    string_list_digest(&mut digest, "recovered-schema", &recovery.recovered_schemas);
    string_list_digest(
        &mut digest,
        "unobserved-planned-schema",
        &recovery.unobserved_planned_schemas,
    );
    string_list_digest(
        &mut digest,
        "off-plan-occurrence",
        &recovery.off_plan_occurrences,
    );
    string_list_digest(&mut digest, "loss-evidence", &recovery.loss_evidence);
    digest.finish()
}

fn projection_digest(projection: &PlanProjection) -> String {
    let mut digest = StableDigest::new("gmeow-plan-projection-content-v1");
    digest.text("plan", &projection.plan);
    optional_text_digest(&mut digest, "graph", projection.source_graph.as_deref());
    optional_text_digest(&mut digest, "standpoint", projection.standpoint.as_deref());
    expression_list_digest(&mut digest, "precondition", &projection.preconditions);
    digest.usize("loop-count", projection.loop_count);
    digest.boolean("loop-variable.present", projection.loop_variable.is_some());
    if let Some(variable) = &projection.loop_variable {
        literal_digest(&mut digest, "loop-variable", variable);
    }
    digest.boolean("loop-period.present", projection.loop_period.is_some());
    if let Some(period) = &projection.loop_period {
        literal_digest(&mut digest, "loop-period", period);
    }
    string_list_digest(&mut digest, "loop-body", &projection.loop_body);
    digest.usize("step.count", projection.steps.len());
    for (index, step) in projection.steps.iter().enumerate() {
        let prefix = format!("step.{index}");
        digest.text(&format!("{prefix}.id"), &step.id);
        digest.usize(&format!("{prefix}.cycle"), step.cycle);
        digest.usize(&format!("{prefix}.position"), step.position);
        digest.text(&format!("{prefix}.source"), &step.source_node);
        digest.text(&format!("{prefix}.kind"), step.kind.id());
    }
    digest.usize("edge.count", projection.edges.len());
    for (index, edge) in projection.edges.iter().enumerate() {
        let prefix = format!("edge.{index}");
        digest.text(&format!("{prefix}.from"), &edge.from);
        digest.text(&format!("{prefix}.to"), &edge.to);
        digest.text(&format!("{prefix}.kind"), edge.kind.id());
    }
    digest.usize("action.count", projection.actions.len());
    for (index, action) in projection.actions.iter().enumerate() {
        let prefix = format!("action.{index}");
        digest.text(&format!("{prefix}.schema"), &action.schema);
        expression_list_digest(
            &mut digest,
            &format!("{prefix}.precondition"),
            &action.preconditions,
        );
        expression_list_digest(&mut digest, &format!("{prefix}.effect"), &action.effects);
        string_list_digest(
            &mut digest,
            &format!("{prefix}.resource"),
            &action.resources,
        );
        string_list_digest(
            &mut digest,
            &format!("{prefix}.capability"),
            &action.capabilities,
        );
        string_list_digest(
            &mut digest,
            &format!("{prefix}.observation"),
            &action.observations,
        );
        digest.usize(&format!("{prefix}.outcome.count"), action.outcomes.len());
        for (outcome_index, outcome) in action.outcomes.iter().enumerate() {
            let outcome_prefix = format!("{prefix}.outcome.{outcome_index}");
            digest.text(
                &format!("{outcome_prefix}.source-node"),
                &outcome.source_node,
            );
            digest.text(&format!("{outcome_prefix}.case"), &outcome.case);
            optional_text_digest(
                &mut digest,
                &format!("{outcome_prefix}.continuation"),
                outcome.continuation.as_deref(),
            );
            optional_text_digest(
                &mut digest,
                &format!("{outcome_prefix}.compensation"),
                outcome.compensation.as_deref(),
            );
        }
        digest.usize(
            &format!("{prefix}.conditional-step.count"),
            action.conditional_steps.len(),
        );
        for (step_index, step) in action.conditional_steps.iter().enumerate() {
            let step_prefix = format!("{prefix}.conditional-step.{step_index}");
            digest.text(&format!("{step_prefix}.source-node"), &step.source_node);
            expression_digest(&mut digest, &format!("{step_prefix}.guard"), &step.guard);
            string_list_digest(
                &mut digest,
                &format!("{step_prefix}.then-step"),
                &step.then_steps,
            );
        }
    }
    digest.usize("guard.count", projection.guards.len());
    for (index, guard) in projection.guards.iter().enumerate() {
        let prefix = format!("guard.{index}");
        digest.text(&format!("{prefix}.guard"), &guard.guard);
        expression_digest(
            &mut digest,
            &format!("{prefix}.condition"),
            &guard.condition,
        );
        string_list_digest(
            &mut digest,
            &format!("{prefix}.then-step"),
            &guard.then_steps,
        );
        string_list_digest(
            &mut digest,
            &format!("{prefix}.else-step"),
            &guard.else_steps,
        );
    }
    digest.usize("freshness.count", projection.freshness.len());
    for (index, freshness) in projection.freshness.iter().enumerate() {
        let prefix = format!("freshness.{index}");
        digest.text(&format!("{prefix}.tracked-state"), &freshness.tracked_state);
        literal_digest(
            &mut digest,
            &format!("{prefix}.currency"),
            &freshness.currency,
        );
    }
    string_list_digest(&mut digest, "loss-evidence", &projection.loss_evidence);
    digest.text(
        "complement.source-digest",
        &projection.complement.source_digest,
    );
    optional_text_digest(
        &mut digest,
        "complement.source-graph",
        projection.complement.source_graph.as_deref(),
    );
    digest.text("complement.plan", &projection.complement.plan);
    optional_text_digest(
        &mut digest,
        "complement.standpoint",
        projection.complement.standpoint.as_deref(),
    );
    string_list_digest(
        &mut digest,
        "complement.loop-body",
        &projection.complement.loop_body,
    );
    string_list_digest(
        &mut digest,
        "complement.expression-digest",
        &projection.complement.expression_digests,
    );
    digest.usize(
        "complement.source-fact.count",
        projection.complement.source_facts.len(),
    );
    for (index, fact) in projection.complement.source_facts.iter().enumerate() {
        let prefix = format!("complement.source-fact.{index}");
        optional_text_digest(
            &mut digest,
            &format!("{prefix}.graph"),
            fact.graph.as_deref(),
        );
        digest.text(&format!("{prefix}.subject"), &fact.subject);
        digest.text(&format!("{prefix}.predicate"), &fact.predicate);
        digest.text(&format!("{prefix}.object"), &fact.object);
    }
    digest.finish()
}

fn certificate_digest(certificate: &PlanProjectionCertificate) -> String {
    let mut digest = StableDigest::new("gmeow-plan-projection-certificate-v1");
    digest.text("version", &certificate.version);
    digest.text("output-contract", &certificate.output_contract);
    digest.text("plan", &certificate.plan);
    digest.text("source", &certificate.source_digest);
    digest.text("projection", &certificate.projection_digest);
    digest.finish()
}

fn literal_digest(digest: &mut StableDigest, prefix: &str, literal: &PlanLiteral) {
    digest.text(&format!("{prefix}.lexical"), &literal.lexical_form);
    digest.text(&format!("{prefix}.datatype"), &literal.datatype);
    optional_text_digest(
        digest,
        &format!("{prefix}.language"),
        literal.language.as_deref(),
    );
    optional_text_digest(
        digest,
        &format!("{prefix}.direction"),
        literal.direction.as_deref(),
    );
}

fn optional_text_digest(digest: &mut StableDigest, prefix: &str, value: Option<&str>) {
    digest.boolean(&format!("{prefix}.present"), value.is_some());
    if let Some(value) = value {
        digest.text(&format!("{prefix}.value"), value);
    }
}

fn string_list_digest(digest: &mut StableDigest, prefix: &str, values: &[String]) {
    digest.usize(&format!("{prefix}.count"), values.len());
    for (index, value) in values.iter().enumerate() {
        digest.text(&format!("{prefix}.{index}"), value);
    }
}

fn expression_list_digest(digest: &mut StableDigest, prefix: &str, values: &[ProjectedExpression]) {
    digest.usize(&format!("{prefix}.count"), values.len());
    for (index, value) in values.iter().enumerate() {
        expression_digest(digest, &format!("{prefix}.{index}"), value);
    }
}

fn expression_digest(digest: &mut StableDigest, prefix: &str, value: &ProjectedExpression) {
    digest.text(&format!("{prefix}.root"), &value.root);
    digest.text(&format!("{prefix}.digest"), &value.digest);
    digest.usize(&format!("{prefix}.fact.count"), value.facts.len());
    for (index, fact) in value.facts.iter().enumerate() {
        let fact_prefix = format!("{prefix}.fact.{index}");
        digest.text(&format!("{fact_prefix}.subject"), &fact.subject);
        digest.text(&format!("{fact_prefix}.predicate"), &fact.predicate);
        digest.text(&format!("{fact_prefix}.object"), &fact.object);
    }
    string_list_digest(
        digest,
        &format!("{prefix}.tracked-state"),
        &value.tracked_states,
    );
}
