// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Canonical-IR materialization over the native physical cores.

use purrdf::{DatasetView, FallibleDatasetView, RdfDataset, TermValue, ViewOperationStatus};

use crate::annotation::{
    AnnotatedFactKey, AnnotatedQuad, AnnotationCertification, AnnotationDerivation,
    AnnotationFactRef, AnnotationRequest, TupleAnnotationAlgebra,
};
use crate::dispatch::{QueryExecutionEvidence, QueryExecutionIdentity, ResidentViewEvidence};
use crate::provenance::ASSERT_RULE_IRI;
use crate::result::PreservationClaim;
use crate::seam::{
    BudgetStatus, DerivationId, DerivedQuad, RdfViewFactSource, WorldFactPattern, WorldFactSource,
    WorldSourceIdentity, WorldSourceMetrics,
};

pub(crate) const ASSERTED_PROFILE: &str =
    "https://blackcatinformatics.ca/logic/PositiveHornProfile";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaterializeError {
    Parse(String),
    Chase(String),
}

impl std::fmt::Display for MaterializeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(message) | Self::Chase(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for MaterializeError {}

#[derive(Debug, Clone, PartialEq)]
pub struct NonQuadRow {
    pub predicate: String,
    pub args: Vec<TermValue>,
    pub is_edb: bool,
}

pub use crate::physical::ChaseAdmission;

/// A typed conjunctive-head existential rule for the native restricted chase.
///
/// This is the public structured boundary for callers whose canonical model already
/// contains value-inventing rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StructuredTerm {
    Var(String),
    Named(String),
    Literal(TermValue),
}

impl StructuredTerm {
    #[must_use]
    pub fn var(name: impl Into<String>) -> Self {
        Self::Var(name.into())
    }

    #[must_use]
    pub fn named(iri: impl Into<String>) -> Self {
        Self::Named(iri.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuredAtom {
    pub subject: StructuredTerm,
    pub predicate: String,
    pub object: StructuredTerm,
}

impl StructuredAtom {
    #[must_use]
    pub fn new(
        subject: StructuredTerm,
        predicate: impl Into<String>,
        object: StructuredTerm,
    ) -> Self {
        Self {
            subject,
            predicate: predicate.into(),
            object,
        }
    }

    fn as_eval(&self) -> crate::rule_ir::EvalAtom {
        fn term(value: &StructuredTerm) -> crate::rule_ir::EvalTerm {
            match value {
                StructuredTerm::Var(name) => crate::rule_ir::EvalTerm::Var(name.clone()),
                StructuredTerm::Named(iri) => crate::rule_ir::EvalTerm::ConstNamed(iri.clone()),
                StructuredTerm::Literal(value) => crate::rule_ir::EvalTerm::ConstLit(value.clone()),
            }
        }
        crate::rule_ir::EvalAtom::positive(term(&self.subject), &self.predicate, term(&self.object))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuredExistentialRule {
    pub rule_iri: String,
    pub body: Vec<StructuredAtom>,
    pub head: Vec<StructuredAtom>,
    pub distinct: Vec<(String, String)>,
    pub witness_frontier: Option<Vec<String>>,
}

impl From<&StructuredExistentialRule> for crate::physical::ExistentialRule {
    fn from(rule: &StructuredExistentialRule) -> Self {
        Self {
            rule_iri: rule.rule_iri.clone(),
            body: rule.body.iter().map(StructuredAtom::as_eval).collect(),
            head: rule.head.iter().map(StructuredAtom::as_eval).collect(),
            distinct: rule.distinct.clone(),
            witness_frontier: rule.witness_frontier.clone(),
            witness_policy: crate::physical::WitnessPolicy::FrontierSkolem,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Materialization {
    pub quads: Vec<DerivedQuad>,
    pub non_quad_rows: Vec<NonQuadRow>,
    pub preservation: PreservationClaim,
    pub frontier: crate::query_ir::CompletionFrontier,
    pub chase_admission: Option<ChaseAdmission>,
    pub nonmonotone_solve_runs: Vec<WorldNonmonotoneSolveRun>,
}

/// A selected view-backed materialization certified under source and engine identities.
#[derive(Debug, Clone)]
pub struct CompleteViewMaterialization<BackendEvidence> {
    /// Program-relevant asserted rows plus their supported native closure.
    pub materialization: Materialization,
    /// Backend and source-access evidence captured after output materialization.
    pub evidence: QueryExecutionEvidence<BackendEvidence>,
    /// Source generation plus engine and materialization-contract identities.
    pub identity: QueryExecutionIdentity,
}

/// Failure of selected materialization over an operationally fallible RDF view.
#[derive(Debug)]
pub enum FallibleViewMaterializationError<OperationalError, BackendEvidence> {
    /// Program lowering or native materialization failed while the view stayed ready.
    Materialization {
        /// Ordinary native materialization failure.
        error: MaterializeError,
        /// Backend and source-access evidence at the final ready checkpoint.
        evidence: QueryExecutionEvidence<BackendEvidence>,
        /// Source and engine identities for the failed attempt.
        identity: QueryExecutionIdentity,
    },
    /// Lazy RDF access failed and invalidated every partial internal row.
    Operational {
        /// Sticky typed provider, budget, cancellation, deadline, or generation error.
        error: OperationalError,
        /// Backend and source-access evidence at the failure boundary.
        evidence: QueryExecutionEvidence<BackendEvidence>,
        /// Source and engine identities for the failed attempt.
        identity: QueryExecutionIdentity,
    },
}

/// Public return type for selected materialization over a fallible RDF view.
pub type FallibleViewMaterializationResult<OperationalError, BackendEvidence> = Result<
    CompleteViewMaterialization<BackendEvidence>,
    Box<FallibleViewMaterializationError<OperationalError, BackendEvidence>>,
>;

impl<OperationalError, BackendEvidence>
    FallibleViewMaterializationError<OperationalError, BackendEvidence>
{
    /// Borrow the evidence carried by either failure variant.
    #[must_use]
    pub const fn evidence(&self) -> &QueryExecutionEvidence<BackendEvidence> {
        match self {
            Self::Materialization { evidence, .. } | Self::Operational { evidence, .. } => evidence,
        }
    }

    /// Borrow the operational root cause, when lazy RDF access failed.
    #[must_use]
    pub const fn operational_error(&self) -> Option<&OperationalError> {
        match self {
            Self::Materialization { .. } => None,
            Self::Operational { error, .. } => Some(error),
        }
    }

    /// Borrow the native materialization error, when the RDF view remained ready.
    #[must_use]
    pub const fn materialization_error(&self) -> Option<&MaterializeError> {
        match self {
            Self::Materialization { error, .. } => Some(error),
            Self::Operational { .. } => None,
        }
    }
}

impl<OperationalError: std::fmt::Display, BackendEvidence> std::fmt::Display
    for FallibleViewMaterializationError<OperationalError, BackendEvidence>
{
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Materialization { error, .. } => error.fmt(formatter),
            Self::Operational { error, .. } => {
                write!(
                    formatter,
                    "operational RDF materialization failure: {error}"
                )
            }
        }
    }
}

/// Native materialization plus the opaque annotation carried by every admitted quad.
#[derive(Debug, Clone)]
pub struct AnnotatedMaterialization<E> {
    /// The unchanged logical/provenance materialization result.
    pub materialization: Materialization,
    /// Score-carrying quad rows, in the same deterministic order as
    /// `materialization.quads`.
    pub quads: Vec<AnnotatedQuad<E>>,
    /// Structural and algebraic admission certificate for the annotation layer.
    pub certification: AnnotationCertification,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MaterializationLimits {
    pub max_steps: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldNonmonotoneSolveRun {
    pub world: String,
    pub run: crate::reason::perf_ledger::NonmonotoneSolveRun,
}

pub(crate) fn selected_materialization_contract_hash(
    program: &gmeow_logic_compile::ir::LogicProgram,
    worlds: &[String],
    limits: MaterializationLimits,
    declared_profile: Option<gmeow_logic_compile::ir::SemanticProfileId>,
) -> String {
    fn frame(hasher: &mut blake3::Hasher, value: &[u8]) {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value);
    }

    let mut worlds = worlds.to_vec();
    worlds.sort();
    worlds.dedup();
    let mut hasher = blake3::Hasher::new();
    frame(
        &mut hasher,
        b"gmeow-selected-view-materialization-contract-v1",
    );
    frame(&mut hasher, program.canonical_key().as_bytes());
    match declared_profile {
        Some(profile) => {
            hasher.update(&[1]);
            frame(&mut hasher, profile.as_str().as_bytes());
        }
        None => {
            hasher.update(&[0]);
        }
    }
    match limits.max_steps {
        Some(steps) => {
            hasher.update(&[1]);
            hasher.update(&steps.to_le_bytes());
        }
        None => {
            hasher.update(&[0]);
        }
    }
    for world in worlds {
        frame(&mut hasher, world.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn selected_materialization_patterns(
    program: &gmeow_logic_compile::ir::LogicProgram,
) -> Result<Vec<WorldFactPattern>, MaterializeError> {
    fn source_term(term: &crate::rule_ir::EvalTerm) -> Option<TermValue> {
        match term {
            crate::rule_ir::EvalTerm::Var(_) => None,
            crate::rule_ir::EvalTerm::ConstNamed(iri) => Some(TermValue::iri(iri)),
            crate::rule_ir::EvalTerm::ConstLit(value) => Some(value.clone()),
        }
    }

    fn insert_pattern(patterns: &mut Vec<WorldFactPattern>, atom: &crate::rule_ir::EvalAtom) {
        let pattern = WorldFactPattern::new(
            source_term(&atom.subject),
            Some(atom.predicate.clone()),
            source_term(&atom.object),
        );
        if patterns.iter().any(|existing| existing.subsumes(&pattern)) {
            return;
        }
        patterns.retain(|existing| !pattern.subsumes(existing));
        patterns.push(pattern);
    }

    let lowering = crate::relational_core::lower_formulas(program);
    let rules = crate::lower::lower_eval_rules(program)
        .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;
    let mut patterns = Vec::new();
    for rule in rules.iter().chain(lowering.rules.iter()) {
        insert_pattern(&mut patterns, &rule.head);
        for atom in &rule.body {
            insert_pattern(&mut patterns, atom);
        }
    }
    for rule in &lowering.nary_head_rules {
        for atom in rule.head.iter().chain(rule.body.iter()) {
            insert_pattern(&mut patterns, atom);
        }
    }
    if patterns.is_empty() {
        return Err(MaterializeError::Chase(
            "selected view materialization has no program predicate to push into the RDF source; use the existing whole-dataset materializer when the complete input echo is the intended output"
                .to_owned(),
        ));
    }
    patterns.sort();
    Ok(patterns)
}

/// Materialize the program-relevant slice of explicit named worlds from a fact source.
///
/// Every predicate consumed or produced by the canonical program becomes a selective
/// source probe. Unrelated predicates and pages are never read or copied. The selected
/// rows form the materializer's necessary working set; the existing whole-dataset API
/// remains the explicit choice when a caller wants every unrelated asserted quad echoed.
///
/// # Errors
///
/// Returns a materialization error for source access, program lowering, or a native
/// evaluator refusal. A program with no pushable predicate is refused rather than
/// widened to an unconstrained source scan.
pub fn materialize_program_source(
    program: &gmeow_logic_compile::ir::LogicProgram,
    source: &dyn WorldFactSource,
    worlds: &[String],
    limits: MaterializationLimits,
    declared_profile: Option<gmeow_logic_compile::ir::SemanticProfileId>,
) -> Result<Materialization, MaterializeError> {
    let patterns = selected_materialization_patterns(program)?;
    let mut worlds = worlds.to_vec();
    worlds.sort();
    worlds.dedup();
    let store = crate::store::WorldStore::new();
    let mut source_provenance = std::collections::BTreeMap::new();
    for world in &worlds {
        crate::physical::visit_edb_patterns(source, world, &patterns, &mut |quad| {
            source_provenance
                .entry((
                    quad.graph.clone(),
                    quad.subject.clone(),
                    quad.predicate.clone(),
                    quad.object.clone(),
                ))
                .or_insert_with(|| quad.clone());
            store.insert_quad_terms(
                world,
                quad.subject.clone(),
                TermValue::iri(&quad.predicate),
                quad.object.clone(),
            )
        })
        .map_err(|error| MaterializeError::Parse(error.message().to_owned()))?;
    }
    let mut materialization = materialize_program_store(program, &store, limits, declared_profile)?;
    for quad in &mut materialization.quads {
        if let Some(source_quad) = source_provenance.get(&(
            quad.graph.clone(),
            quad.subject.clone(),
            quad.predicate.clone(),
            quad.object.clone(),
        )) {
            *quad = source_quad.clone();
        }
    }
    Ok(materialization)
}

/// Materialize directly from an infallible resident or succinct-pack RDF view.
///
/// Only the program-relevant rows in the explicitly supplied named worlds enter the
/// native working set; no complete-world snapshot is constructed.
pub fn materialize_program_view<V: DatasetView>(
    program: &gmeow_logic_compile::ir::LogicProgram,
    view: &V,
    source_identity: WorldSourceIdentity,
    worlds: &[String],
    limits: MaterializationLimits,
    declared_profile: Option<gmeow_logic_compile::ir::SemanticProfileId>,
) -> Result<CompleteViewMaterialization<ResidentViewEvidence>, MaterializeError> {
    let identity = QueryExecutionIdentity::for_contract(
        source_identity,
        selected_materialization_contract_hash(program, worlds, limits, declared_profile),
    );
    let source = RdfViewFactSource::new(view, ASSERTED_PROFILE, identity.source.clone());
    let materialization =
        materialize_program_source(program, &source, worlds, limits, declared_profile)?;
    Ok(CompleteViewMaterialization {
        materialization,
        evidence: QueryExecutionEvidence {
            backend: ResidentViewEvidence {
                len_hint: view.len_hint(),
                stats_fingerprint: view.stats_fingerprint(),
            },
            source: source.metrics(),
        },
        identity,
    })
}

/// Materialize directly from an operationally fallible RDF view.
///
/// Preflight and final checkpoints make provider/budget/cancellation/generation
/// failure dominant over any internal materialization result. Partial rows never
/// cross this boundary.
pub fn materialize_program_fallible_view<V: FallibleDatasetView>(
    program: &gmeow_logic_compile::ir::LogicProgram,
    view: &V,
    source_identity: WorldSourceIdentity,
    worlds: &[String],
    limits: MaterializationLimits,
    declared_profile: Option<gmeow_logic_compile::ir::SemanticProfileId>,
) -> FallibleViewMaterializationResult<V::Error, V::Evidence> {
    let identity = QueryExecutionIdentity::for_contract(
        source_identity,
        selected_materialization_contract_hash(program, worlds, limits, declared_profile),
    );
    if let ViewOperationStatus::Failed { error, evidence } = view.operation_status() {
        return Err(Box::new(FallibleViewMaterializationError::Operational {
            error,
            evidence: QueryExecutionEvidence {
                backend: evidence,
                source: WorldSourceMetrics::default(),
            },
            identity,
        }));
    }
    let source = RdfViewFactSource::new(view, ASSERTED_PROFILE, identity.source.clone());
    let evaluation = materialize_program_source(program, &source, worlds, limits, declared_profile);
    let source_metrics = source.metrics();
    match view.operation_status() {
        ViewOperationStatus::Failed { error, evidence } => {
            Err(Box::new(FallibleViewMaterializationError::Operational {
                error,
                evidence: QueryExecutionEvidence {
                    backend: evidence,
                    source: source_metrics,
                },
                identity,
            }))
        }
        ViewOperationStatus::Ready { evidence } => match evaluation {
            Ok(materialization) => Ok(CompleteViewMaterialization {
                materialization,
                evidence: QueryExecutionEvidence {
                    backend: evidence,
                    source: source_metrics,
                },
                identity,
            }),
            Err(error) => Err(Box::new(
                FallibleViewMaterializationError::Materialization {
                    error,
                    evidence: QueryExecutionEvidence {
                        backend: evidence,
                        source: source_metrics,
                    },
                    identity,
                },
            )),
        },
    }
}

fn program_profile(
    program: &gmeow_logic_compile::ir::LogicProgram,
) -> Result<gmeow_logic_compile::ir::SemanticProfileId, MaterializeError> {
    let presets = program
        .contracts
        .iter()
        .filter_map(|contract| contract.preset)
        .collect::<std::collections::BTreeSet<_>>();
    match presets.len() {
        0 => Ok(gmeow_logic_compile::ir::SemanticProfileId::PositiveHorn),
        1 => Ok(*presets.iter().next().expect("one profile")),
        _ => Err(MaterializeError::Chase(format!(
            "materialize_program requires one semantic profile, found {presets:?}"
        ))),
    }
}

fn derived_row_to_quad(row: crate::rule_ir::DerivedRow) -> DerivedQuad {
    DerivedQuad {
        graph: row.graph.clone(),
        subject: row.subject,
        predicate: row.predicate,
        object: row.object,
        graph_component: row.graph,
        derivation_id: DerivationId(row.derivation_id),
        rule_iri: row.rule_iri,
        source_quad_ids: row.source_quad_ids,
        profile: ASSERTED_PROFILE.to_owned(),
        budget_status: BudgetStatus::Ok,
    }
}

#[derive(Debug, Clone)]
enum NonmonotoneWorldSession {
    WellFounded(crate::wellfounded::IncrementalWellFoundedSession),
    StableModel(crate::stablemodel::IncrementalStableModelSession),
}

impl NonmonotoneWorldSession {
    fn prepare(
        profile: gmeow_logic_compile::ir::SemanticProfileId,
        contract_hash: &str,
        world: &str,
        edb: impl IntoIterator<Item = crate::rule_ir::Fact>,
        rules: &[crate::rule_ir::EvalRule],
    ) -> Result<Self, MaterializeError> {
        let result = match profile {
            gmeow_logic_compile::ir::SemanticProfileId::WellFounded => {
                crate::wellfounded::IncrementalWellFoundedSession::new(
                    contract_hash,
                    world,
                    edb,
                    rules,
                )
                .map(Self::WellFounded)
            }
            gmeow_logic_compile::ir::SemanticProfileId::StableModel => {
                crate::stablemodel::IncrementalStableModelSession::new(
                    contract_hash,
                    world,
                    edb,
                    rules,
                )
                .map(Self::StableModel)
            }
            other => {
                return Err(MaterializeError::Chase(format!(
                    "non-monotone materialization requires WellFoundedProfile or StableModelProfile, got {other}"
                )));
            }
        };
        result.map_err(|error| MaterializeError::Chase(error.message().to_owned()))
    }

    fn rows(&self) -> &[crate::rule_ir::DerivedRow] {
        match self {
            Self::WellFounded(session) => session.rows(),
            Self::StableModel(session) => session.rows(),
        }
    }

    fn active_ground_rule_count(&self) -> usize {
        match self {
            Self::WellFounded(session) => session.active_ground_rule_count(),
            Self::StableModel(session) => session.active_ground_rule_count(),
        }
    }
}

fn nonmonotone_solver(
    profile: gmeow_logic_compile::ir::SemanticProfileId,
) -> Result<crate::reason::perf_ledger::NonmonotoneSolver, MaterializeError> {
    match profile {
        gmeow_logic_compile::ir::SemanticProfileId::WellFounded => {
            Ok(crate::reason::perf_ledger::NonmonotoneSolver::WellFounded)
        }
        gmeow_logic_compile::ir::SemanticProfileId::StableModel => {
            Ok(crate::reason::perf_ledger::NonmonotoneSolver::StableModel)
        }
        other => Err(MaterializeError::Chase(format!(
            "non-monotone solver lookup received unsupported profile {other}"
        ))),
    }
}

fn materialize_nonmonotone(
    profile: gmeow_logic_compile::ir::SemanticProfileId,
    rules: &[crate::rule_ir::EvalRule],
    store: &crate::store::WorldStore,
    preservation: PreservationClaim,
) -> Result<Materialization, MaterializeError> {
    let rule_hash = crate::physical::canonical_rule_hash(rules)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let contract_hash = format!(
        "gmeow-native-nonmonotone-materialize-v2:{}:{rule_hash}",
        profile.as_str()
    );
    let mut rows = Vec::new();
    let mut solve_runs = Vec::new();
    for world in store.worlds() {
        let edb = crate::rule_ir::world_edb_facts(store, &world)
            .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;
        let session =
            NonmonotoneWorldSession::prepare(profile, &contract_hash, &world, edb, rules)?;
        solve_runs.push(WorldNonmonotoneSolveRun {
            world,
            run: crate::reason::perf_ledger::nonmonotone_solve_run(
                nonmonotone_solver(profile)?,
                true,
                0,
                session.active_ground_rule_count(),
            ),
        });
        rows.extend(session.rows().iter().cloned());
    }
    Ok(Materialization {
        quads: rows.into_iter().map(derived_row_to_quad).collect(),
        non_quad_rows: Vec::new(),
        preservation,
        frontier: crate::query_ir::CompletionFrontier::empty(),
        chase_admission: None,
        nonmonotone_solve_runs: solve_runs,
    })
}

pub fn materialize_program(
    program: &gmeow_logic_compile::ir::LogicProgram,
    input: &RdfDataset,
    limits: MaterializationLimits,
    declared_profile: Option<gmeow_logic_compile::ir::SemanticProfileId>,
) -> Result<Materialization, MaterializeError> {
    let store = crate::store::WorldStore::new();
    store
        .load_dataset(input)
        .map_err(|error| MaterializeError::Parse(error.message().to_owned()))?;
    materialize_program_store(program, &store, limits, declared_profile)
}

fn materialize_program_store(
    program: &gmeow_logic_compile::ir::LogicProgram,
    store: &crate::store::WorldStore,
    limits: MaterializationLimits,
    declared_profile: Option<gmeow_logic_compile::ir::SemanticProfileId>,
) -> Result<Materialization, MaterializeError> {
    let profile = match declared_profile {
        Some(profile) => profile,
        None => program_profile(program)?,
    };
    let lowering = crate::relational_core::lower_formulas(program);
    let mut rules = crate::lower::lower_eval_rules(program)
        .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;
    rules.extend(lowering.rules);
    let mut preservation = lowering.preservation;

    if matches!(
        profile,
        gmeow_logic_compile::ir::SemanticProfileId::WellFounded
            | gmeow_logic_compile::ir::SemanticProfileId::StableModel
    ) {
        if limits.max_steps.is_some() || !lowering.nary_head_rules.is_empty() {
            return Err(MaterializeError::Chase(
                "non-monotone materialization does not support step budgets or existential formula heads"
                    .to_owned(),
            ));
        }
        return materialize_nonmonotone(profile, &rules, store, preservation);
    }

    let contract_hash = format!("gmeow-materialize-structured-v2:{}", profile.as_str());
    let lookup = crate::physical::compile_cached(contract_hash, rules.clone());
    let Some(executable) = lookup.executable else {
        if profile != gmeow_logic_compile::ir::SemanticProfileId::StratifiedNaf {
            return Err(MaterializeError::Chase(
                "native structured materialization refused a non-stratifiable program".to_owned(),
            ));
        }
        preservation =
            PreservationClaim::for_unsupported(rules.iter().map(|rule| rule.rule_iri.clone()));
        let mut quads = Vec::new();
        for world in store.worlds() {
            let edb = crate::rule_ir::world_edb_facts(store, &world)
                .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;
            quads.extend(
                crate::rule_ir::echo_asserted(&world, &edb)
                    .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?
                    .into_iter()
                    .map(derived_row_to_quad),
            );
        }
        return Ok(Materialization {
            quads,
            non_quad_rows: Vec::new(),
            preservation,
            frontier: crate::query_ir::CompletionFrontier::empty(),
            chase_admission: None,
            nonmonotone_solve_runs: Vec::new(),
        });
    };

    let budgeted =
        match crate::physical::materialize_native(store, executable.as_ref(), limits.max_steps)
            .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?
        {
            crate::physical::NativeOutcome::Decided(result) => result,
            crate::physical::NativeOutcome::Unsupported(kind) => {
                return Err(MaterializeError::Chase(format!(
                    "native structured materialization refused {kind:?}"
                )));
            }
        };
    let frontier = budgeted.frontier();
    let status = budgeted.status;
    let mut quads = budgeted
        .rows
        .into_iter()
        .map(derived_row_to_quad)
        .collect::<Vec<_>>();
    for quad in &mut quads {
        quad.budget_status = match status {
            BudgetStatus::Exhausted if frontier.saturated_preds.contains(&quad.predicate) => {
                BudgetStatus::Ok
            }
            other => other,
        };
    }

    let mut chase_admission = None;
    if !lowering.nary_head_rules.is_empty() {
        if limits.max_steps.is_some() {
            return Err(MaterializeError::Chase(
                "one global step budget across ordinary and existential rules is not representable"
                    .to_owned(),
            ));
        }
        let chase_store = crate::store::WorldStore::new();
        for quad in &quads {
            chase_store
                .insert_quad_terms(
                    &quad.graph,
                    quad.subject.clone(),
                    TermValue::iri(&quad.predicate),
                    quad.object.clone(),
                )
                .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;
        }
        let (admission, outcome) =
            crate::physical::chase_materialize(&chase_store, &lowering.nary_head_rules, None)
                .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;
        chase_admission = Some(admission.clone());
        let extra = match outcome {
            crate::physical::NativeOutcome::Decided(result) => result.rows,
            crate::physical::NativeOutcome::Unsupported(kind) => {
                return Err(MaterializeError::Chase(format!(
                    "native existential materialization refused {kind:?}: {:?}",
                    admission.capability_gap_rows()
                )));
            }
        };
        quads.extend(
            extra
                .into_iter()
                .filter(|row| row.rule_iri != ASSERT_RULE_IRI)
                .map(derived_row_to_quad),
        );
    }

    quads.sort_by_cached_key(|quad| {
        (
            quad.graph.clone(),
            crate::provenance::term_display(&quad.subject),
            quad.predicate.clone(),
            crate::provenance::term_display(&quad.object),
        )
    });
    Ok(Materialization {
        quads,
        non_quad_rows: Vec::new(),
        preservation,
        frontier,
        chase_admission,
        nonmonotone_solve_runs: Vec::new(),
    })
}

fn public_annotation_derivations<E: Clone>(
    graph: &str,
    derivations: &[crate::physical::PhysicalAnnotationDerivation<E>],
) -> Vec<AnnotationDerivation<E>> {
    derivations
        .iter()
        .map(|derivation| AnnotationDerivation {
            rule_iri: derivation.rule_iri.clone(),
            sources: derivation
                .sources
                .iter()
                .map(|(subject, predicate, object)| AnnotatedFactKey {
                    graph: graph.to_owned(),
                    subject: subject.clone(),
                    predicate: predicate.clone(),
                    object: object.clone(),
                })
                .collect(),
            tuple_sources: Vec::new(),
            annotation: derivation.annotation.clone(),
        })
        .collect()
}

struct MaterializedAnnotationWorld<E> {
    facts: Vec<crate::rule_ir::Fact>,
    annotations: std::collections::BTreeMap<crate::rule_ir::FactKey, E>,
    derivations: std::collections::BTreeMap<
        crate::rule_ir::FactKey,
        Vec<crate::physical::PhysicalAnnotationDerivation<E>>,
    >,
}

/// Materialize a canonical program while carrying opaque tuple annotations.
///
/// `annotation_for` is consulted only for asserted input quads. Returning `None` gives
/// the fact `algebra.one()`, so ordinary RDF remains neutral while selected extensional
/// relations carry BM25/vector/name-similarity scores. Derived annotations are computed
/// inside the native planned join: body conjunction uses `multiply`, and alternative
/// rule groundings use `add`. Positive/stratified membership and annotation equations
/// share one physical closure pass. Non-monotone and existential solvers are also run
/// once; their already-emitted selected proof carriers are folded without replaying joins.
///
/// # Errors
///
/// Returns a typed materialization error for an unsupported program, a contract/query-
/// class mismatch, a caller algebra failure, or a non-convergent annotation fixed point.
/// No partial annotation is returned.
pub fn materialize_program_annotated<A, F>(
    program: &gmeow_logic_compile::ir::LogicProgram,
    input: &RdfDataset,
    limits: MaterializationLimits,
    declared_profile: Option<gmeow_logic_compile::ir::SemanticProfileId>,
    annotation: AnnotationRequest<'_, A, F>,
) -> Result<AnnotatedMaterialization<A::Element>, MaterializeError>
where
    A: TupleAnnotationAlgebra,
    F: for<'fact> Fn(AnnotationFactRef<'fact>) -> Option<A::Element>,
{
    let lowering = crate::relational_core::lower_formulas(program);
    let mut rules = crate::lower::lower_eval_rules(program)
        .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;
    rules.extend(lowering.rules);
    let profile = match declared_profile {
        Some(profile) => profile,
        None => program_profile(program)?,
    };
    let store = crate::store::WorldStore::new();
    store
        .load_dataset(input)
        .map_err(|error| MaterializeError::Parse(error.message().to_owned()))?;

    let seed_for = |world: &str, facts: &[crate::rule_ir::Fact]| {
        facts
            .iter()
            .map(|fact| {
                let value = (annotation.annotation_for)(AnnotationFactRef {
                    world,
                    subject: &fact.subject,
                    predicate: &fact.predicate,
                    object: &fact.object,
                })
                .unwrap_or_else(|| annotation.algebra.one());
                (fact.key(), value)
            })
            .collect::<std::collections::BTreeMap<_, _>>()
    };

    let mut evaluations = std::collections::BTreeMap::new();
    let mut physical_rows = Vec::new();
    let mut solve_runs = Vec::new();
    let mut chase_admission = None;
    let mut preservation = lowering.preservation;
    let mut frontier = crate::query_ir::CompletionFrontier::empty();
    let mut status = BudgetStatus::Ok;

    let certification = if matches!(
        profile,
        gmeow_logic_compile::ir::SemanticProfileId::WellFounded
            | gmeow_logic_compile::ir::SemanticProfileId::StableModel
    ) {
        if limits.max_steps.is_some() || !lowering.nary_head_rules.is_empty() {
            return Err(MaterializeError::Chase(
                "non-monotone materialization does not support step budgets or existential formula heads"
                    .to_owned(),
            ));
        }
        let query_class = match profile {
            gmeow_logic_compile::ir::SemanticProfileId::WellFounded => {
                crate::annotation::AnnotationQueryClass::WellFounded
            }
            _ => crate::annotation::AnnotationQueryClass::StableModel,
        };
        let certification = annotation
            .contract
            .certify_physical_class(
                query_class,
                crate::annotation::AnnotationLineageContract::SelectedPhysicalDerivation,
            )
            .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;
        let rule_hash = crate::physical::canonical_rule_hash(&rules)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let contract_hash = format!(
            "gmeow-native-nonmonotone-materialize-v2:{}:{rule_hash}",
            profile.as_str()
        );
        for world in store.worlds() {
            let edb = crate::rule_ir::world_edb_facts(&store, &world)
                .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;
            let session = NonmonotoneWorldSession::prepare(
                profile,
                &contract_hash,
                &world,
                edb.clone(),
                &rules,
            )?;
            solve_runs.push(WorldNonmonotoneSolveRun {
                world: world.clone(),
                run: crate::reason::perf_ledger::nonmonotone_solve_run(
                    nonmonotone_solver(profile)?,
                    true,
                    0,
                    session.active_ground_rule_count(),
                ),
            });
            let rows = session.rows().to_vec();
            let seeds = seed_for(&world, &edb);
            let evaluated = annotation
                .contract
                .evaluate_selected_physical_lineage(&rows, &edb, &seeds, annotation.algebra)
                .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;
            physical_rows.extend(rows);
            evaluations.insert(
                world,
                MaterializedAnnotationWorld {
                    facts: evaluated.facts,
                    annotations: evaluated.annotations,
                    derivations: evaluated.derivations,
                },
            );
        }
        certification
    } else {
        let contract_hash = format!("gmeow-materialize-annotated-v2:{}", profile.as_str());
        let lookup = crate::physical::compile_cached(contract_hash, rules.clone());
        let Some(executable) = lookup.executable else {
            if profile != gmeow_logic_compile::ir::SemanticProfileId::StratifiedNaf {
                return Err(MaterializeError::Chase(
                    "native annotated materialization refused a non-stratifiable program"
                        .to_owned(),
                ));
            }
            preservation =
                PreservationClaim::for_unsupported(rules.iter().map(|rule| rule.rule_iri.clone()));
            let certification = annotation
                .contract
                .certify_physical_class(
                    crate::annotation::AnnotationQueryClass::StratifiedNaf,
                    crate::annotation::AnnotationLineageContract::AllPhysicalDerivations,
                )
                .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;
            for world in store.worlds() {
                let edb = crate::rule_ir::world_edb_facts(&store, &world)
                    .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;
                let seeds = seed_for(&world, &edb);
                let rows = crate::rule_ir::echo_asserted(&world, &edb)
                    .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;
                let evaluated = annotation
                    .contract
                    .evaluate_selected_physical_lineage(&rows, &edb, &seeds, annotation.algebra)
                    .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;
                physical_rows.extend(rows);
                evaluations.insert(
                    world,
                    MaterializedAnnotationWorld {
                        facts: evaluated.facts,
                        annotations: evaluated.annotations,
                        derivations: evaluated.derivations,
                    },
                );
            }
            return finish_annotated_materialization(
                physical_rows,
                evaluations,
                preservation,
                frontier,
                chase_admission,
                solve_runs,
                certification,
            );
        };

        let certification = if lowering.nary_head_rules.is_empty() {
            crate::physical::certify_query(&rules, annotation.contract)
        } else {
            annotation.contract.certify_physical_class(
                crate::annotation::AnnotationQueryClass::ExistentialChase,
                crate::annotation::AnnotationLineageContract::SelectedPhysicalDerivation,
            )
        }
        .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;

        let mut remaining = limits.max_steps;
        let mut completed = executable.stratum_count();
        let mut saturated: Option<std::collections::BTreeSet<String>> = None;
        let mut consumed = 0_u64;
        for world in store.worlds() {
            let edb = crate::rule_ir::world_edb_facts(&store, &world)
                .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;
            let seeds = seed_for(&world, &edb);
            let evaluated = crate::physical::evaluate_annotations(
                &world,
                &edb,
                executable.as_ref(),
                crate::physical::AnnotationExecution::new(
                    remaining,
                    &seeds,
                    &std::collections::BTreeSet::new(),
                    annotation.algebra,
                    annotation.contract,
                ),
            )
            .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;
            if let Some(limit) = remaining.as_mut() {
                *limit = limit.saturating_sub(evaluated.frontier.consumed_steps);
            }
            consumed = consumed.saturating_add(evaluated.frontier.consumed_steps);
            completed = completed.min(evaluated.frontier.completed);
            saturated = Some(match saturated {
                None => evaluated.frontier.saturated_preds.clone(),
                Some(current) => current
                    .intersection(&evaluated.frontier.saturated_preds)
                    .cloned()
                    .collect(),
            });
            if evaluated.status == BudgetStatus::Exhausted {
                status = BudgetStatus::Exhausted;
            }
            physical_rows.extend(evaluated.rows.iter().cloned());
            evaluations.insert(
                world,
                MaterializedAnnotationWorld {
                    facts: evaluated.facts,
                    annotations: evaluated.annotations,
                    derivations: evaluated.derivations,
                },
            );
        }
        frontier = crate::query_ir::CompletionFrontier {
            completed,
            total: executable.stratum_count(),
            saturated_preds: saturated.unwrap_or_default(),
            consumed_steps: consumed,
        };

        if !lowering.nary_head_rules.is_empty() {
            if limits.max_steps.is_some() {
                return Err(MaterializeError::Chase(
                    "one global step budget across ordinary and existential rules is not representable"
                        .to_owned(),
                ));
            }
            let chase_store = crate::store::WorldStore::new();
            for row in &physical_rows {
                chase_store
                    .insert_quad_terms(
                        &row.graph,
                        row.subject.clone(),
                        TermValue::iri(&row.predicate),
                        row.object.clone(),
                    )
                    .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;
            }
            let (admission, outcome) =
                crate::physical::chase_materialize(&chase_store, &lowering.nary_head_rules, None)
                    .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;
            chase_admission = Some(admission.clone());
            let extra = match outcome {
                crate::physical::NativeOutcome::Decided(result) => result
                    .rows
                    .into_iter()
                    .filter(|row| row.rule_iri != ASSERT_RULE_IRI)
                    .collect::<Vec<_>>(),
                crate::physical::NativeOutcome::Unsupported(kind) => {
                    return Err(MaterializeError::Chase(format!(
                        "native existential materialization refused {kind:?}: {:?}",
                        admission.capability_gap_rows()
                    )));
                }
            };
            for world in store.worlds() {
                let world_extra = extra
                    .iter()
                    .filter(|row| row.graph == world)
                    .cloned()
                    .collect::<Vec<_>>();
                let (prior_facts, prior_annotations, prior_derivations) = {
                    let prior = evaluations
                        .get(&world)
                        .expect("positive world was evaluated");
                    (
                        prior.facts.clone(),
                        prior.annotations.clone(),
                        prior.derivations.clone(),
                    )
                };
                let mut folded = annotation
                    .contract
                    .evaluate_selected_physical_lineage(
                        &world_extra,
                        &prior_facts,
                        &prior_annotations,
                        annotation.algebra,
                    )
                    .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;
                // The chase fold uses the positive closure's annotations as its seed
                // column, but those rows are not newly asserted. Preserve their direct
                // physical lineage and only take new derivations from the chase rows.
                for (key, derivations) in prior_derivations {
                    folded.derivations.insert(key, derivations);
                }
                evaluations.insert(
                    world,
                    MaterializedAnnotationWorld {
                        facts: folded.facts,
                        annotations: folded.annotations,
                        derivations: folded.derivations,
                    },
                );
            }
            physical_rows.extend(extra);
        }
        certification
    };

    let result = finish_annotated_materialization(
        physical_rows,
        evaluations,
        preservation,
        frontier,
        chase_admission,
        solve_runs,
        certification,
    )?;
    if status == BudgetStatus::Exhausted {
        // Preserve the ordinary materializer's per-row budget disclosure.
        let mut result = result;
        for quad in &mut result.materialization.quads {
            quad.budget_status = if result
                .materialization
                .frontier
                .saturated_preds
                .contains(&quad.predicate)
            {
                BudgetStatus::Ok
            } else {
                BudgetStatus::Exhausted
            };
        }
        for annotated in &mut result.quads {
            if let Some(quad) = result
                .materialization
                .quads
                .iter()
                .find(|quad| quad.derivation_id == annotated.quad.derivation_id)
            {
                annotated.quad.budget_status = quad.budget_status;
            }
        }
        return Ok(result);
    }
    Ok(result)
}

fn finish_annotated_materialization<E: Clone>(
    mut rows: Vec<crate::rule_ir::DerivedRow>,
    evaluations: std::collections::BTreeMap<String, MaterializedAnnotationWorld<E>>,
    preservation: PreservationClaim,
    frontier: crate::query_ir::CompletionFrontier,
    chase_admission: Option<ChaseAdmission>,
    nonmonotone_solve_runs: Vec<WorldNonmonotoneSolveRun>,
    certification: AnnotationCertification,
) -> Result<AnnotatedMaterialization<E>, MaterializeError> {
    crate::rule_ir::sort_rows(&mut rows);
    let materialization = Materialization {
        quads: rows.into_iter().map(derived_row_to_quad).collect(),
        non_quad_rows: Vec::new(),
        preservation,
        frontier,
        chase_admission,
        nonmonotone_solve_runs,
    };
    let mut quads = Vec::with_capacity(materialization.quads.len());
    for quad in &materialization.quads {
        let key = (
            crate::provenance::term_display(&quad.subject),
            quad.predicate.clone(),
            crate::provenance::term_display(&quad.object),
        );
        let evaluation = evaluations.get(&quad.graph).ok_or_else(|| {
            MaterializeError::Chase(format!(
                "annotation evaluation is missing materialized world {}",
                quad.graph
            ))
        })?;
        let annotation = evaluation.annotations.get(&key).cloned().ok_or_else(|| {
            MaterializeError::Chase(format!(
                "annotation evaluation is missing admitted fact {key:?} in {}",
                quad.graph
            ))
        })?;
        let derivations = public_annotation_derivations(
            &quad.graph,
            evaluation
                .derivations
                .get(&key)
                .map_or(&[][..], Vec::as_slice),
        );
        quads.push(AnnotatedQuad {
            quad: quad.clone(),
            annotation,
            derivations,
        });
    }
    Ok(AnnotatedMaterialization {
        materialization,
        quads,
        certification,
    })
}

/// Materialize typed existential rules through the native restricted chase.
///
/// An uncertified unbounded program is returned as an asserted-only materialization
/// carrying its [`ChaseAdmission::Uncertified`] certificate. This preserves the
/// fail-closed capability evidence without attempting a potentially non-terminating
/// chase. A bounded call may execute such a program up to its deterministic step limit.
pub fn materialize_existential_rules(
    input: &RdfDataset,
    rules: &[StructuredExistentialRule],
    limits: MaterializationLimits,
) -> Result<Materialization, MaterializeError> {
    let store = crate::store::WorldStore::new();
    store
        .load_dataset(input)
        .map_err(|error| MaterializeError::Parse(error.message().to_owned()))?;
    let physical = rules
        .iter()
        .map(crate::physical::ExistentialRule::from)
        .collect::<Vec<_>>();
    let (admission, outcome) =
        crate::physical::chase_materialize(&store, &physical, limits.max_steps)
            .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;
    let budgeted = match outcome {
        crate::physical::NativeOutcome::Decided(result) => result,
        crate::physical::NativeOutcome::Unsupported(
            crate::physical::UnsupportedKind::NonTerminatingExistential,
        ) => {
            let mut quads = Vec::new();
            for world in store.worlds() {
                let edb = crate::rule_ir::world_edb_facts(&store, &world)
                    .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;
                quads.extend(
                    crate::rule_ir::echo_asserted(&world, &edb)
                        .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?
                        .into_iter()
                        .map(derived_row_to_quad),
                );
            }
            return Ok(Materialization {
                quads,
                non_quad_rows: Vec::new(),
                preservation: PreservationClaim::for_unsupported(
                    rules.iter().map(|rule| rule.rule_iri.clone()),
                ),
                frontier: crate::query_ir::CompletionFrontier::empty(),
                chase_admission: Some(admission),
                nonmonotone_solve_runs: Vec::new(),
            });
        }
        crate::physical::NativeOutcome::Unsupported(kind) => {
            return Err(MaterializeError::Chase(format!(
                "native typed existential materialization refused {kind:?}: {:?}",
                admission.capability_gap_rows()
            )));
        }
    };
    let frontier = budgeted.frontier();
    Ok(Materialization {
        quads: budgeted.rows.into_iter().map(derived_row_to_quad).collect(),
        non_quad_rows: Vec::new(),
        preservation: PreservationClaim::exact(),
        frontier,
        chase_admission: Some(admission),
        nonmonotone_solve_runs: Vec::new(),
    })
}

#[cfg(test)]
mod annotation_tests {
    use super::*;
    use crate::annotation::{
        AnnotationFactRef, AnnotationLineageContract, AnnotationQueryClass, AnnotationRequest,
    };
    use crate::provenance::ZWeightSemiring;
    use gmeow_logic_compile::ir::{
        ContextualScope, Formula, LogicAxiom, LogicProgram, LogicRule, SemanticProfileId, Term,
    };
    use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm};

    const WORLD: &str = "https://example.org/world";
    const EDGE: &str = "https://example.org/edge";
    const REACH: &str = "https://example.org/reach";
    const X: &str = "https://example.org/x";
    const Y: &str = "https://example.org/y";

    fn dataset() -> std::sync::Arc<RdfDataset> {
        let mut builder = RdfDatasetBuilder::new();
        builder.push_owned_quad(
            &RdfQuad::new(RdfTerm::iri(X), EDGE, RdfTerm::iri(Y)).in_graph(RdfTerm::iri(WORLD)),
        );
        builder.freeze().expect("valid annotation test dataset")
    }

    fn binary_program() -> LogicProgram {
        let head =
            LogicAxiom::new("?x", REACH, "?y", false, false, ContextualScope::default()).unwrap();
        let body =
            LogicAxiom::new("?x", EDGE, "?y", false, false, ContextualScope::default()).unwrap();
        let scope = ContextualScope {
            provenance: Some("https://example.org/rule/reach".to_owned()),
            ..ContextualScope::default()
        };
        LogicProgram::new(
            vec![],
            vec![LogicRule::new(head, vec![body], vec![], scope)],
            vec![],
            None,
        )
    }

    #[test]
    fn annotated_nonmonotone_profiles_fold_selected_lineage_without_a_second_solve() {
        let input = dataset();
        for (profile, expected) in [
            (
                SemanticProfileId::WellFounded,
                AnnotationQueryClass::WellFounded,
            ),
            (
                SemanticProfileId::StableModel,
                AnnotationQueryClass::StableModel,
            ),
        ] {
            let contract = crate::annotation::AnnotationContract::exact();
            let annotated = materialize_program_annotated(
                &binary_program(),
                input.as_ref(),
                MaterializationLimits::default(),
                Some(profile),
                AnnotationRequest::new(
                    &ZWeightSemiring,
                    &contract,
                    |fact: AnnotationFactRef<'_>| (fact.predicate == EDGE).then_some(2),
                ),
            )
            .expect("annotated non-monotone materialization");
            assert_eq!(annotated.certification.query_class, expected);
            assert_eq!(
                annotated.certification.lineage_contract,
                AnnotationLineageContract::SelectedPhysicalDerivation
            );
            assert_eq!(annotated.materialization.nonmonotone_solve_runs.len(), 1);
            let derived = annotated
                .quads
                .iter()
                .find(|row| row.quad.predicate == REACH)
                .expect("selected solver proof derives reach");
            assert_eq!(derived.annotation, 2);
            assert!(derived.derivations.iter().any(|d| d.annotation == 2));
        }
    }

    #[test]
    fn annotated_existential_head_carries_body_product_onto_every_invented_tuple_row() {
        let rel = "https://example.org/rel";
        let constant = "https://example.org/constant";
        let body = Formula::atom(
            Term::iri(REACH).unwrap(),
            vec![Term::var("x").unwrap(), Term::var("y").unwrap()],
        )
        .unwrap();
        let head = Formula::atom(
            Term::iri(rel).unwrap(),
            vec![
                Term::var("x").unwrap(),
                Term::var("y").unwrap(),
                Term::iri(constant).unwrap(),
            ],
        )
        .unwrap();
        let formula = Formula::Forall {
            vars: vec!["x".to_owned(), "y".to_owned()],
            body: Box::new(Formula::Implies(Box::new(body), Box::new(head))),
        };
        let program = binary_program().with_formulas(vec![formula]);
        let contract = crate::annotation::AnnotationContract::exact();
        let annotated = materialize_program_annotated(
            &program,
            dataset().as_ref(),
            MaterializationLimits::default(),
            Some(SemanticProfileId::PositiveHorn),
            AnnotationRequest::new(
                &ZWeightSemiring,
                &contract,
                |fact: AnnotationFactRef<'_>| (fact.predicate == EDGE).then_some(3),
            ),
        )
        .expect("annotated existential materialization");

        assert_eq!(
            annotated.certification.query_class,
            AnnotationQueryClass::ExistentialChase
        );
        assert_eq!(
            annotated.certification.lineage_contract,
            AnnotationLineageContract::SelectedPhysicalDerivation
        );
        assert!(annotated.materialization.chase_admission.is_some());
        let reach = annotated
            .quads
            .iter()
            .find(|row| row.quad.predicate == REACH)
            .expect("positive pre-chase closure derives reach");
        assert_eq!(reach.annotation, 3);
        assert!(reach.derivations.iter().any(|derivation| {
            derivation.rule_iri != ASSERT_RULE_IRI
                && derivation
                    .sources
                    .iter()
                    .any(|source| source.predicate == EDGE)
        }));
        let invented = annotated
            .quads
            .iter()
            .filter(|row| row.quad.rule_iri != ASSERT_RULE_IRI && row.quad.predicate != REACH)
            .collect::<Vec<_>>();
        assert_eq!(invented.len(), 4, "instanceOf plus three positional rows");
        assert!(invented.iter().all(|row| row.annotation == 3));
        assert!(invented.iter().all(|row| {
            row.derivations.iter().any(|derivation| {
                derivation.annotation == 3
                    && derivation
                        .sources
                        .iter()
                        .any(|source| source.predicate == REACH)
            })
        }));
    }
}
