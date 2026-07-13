// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Canonical-IR materialization over the native physical cores.

use purrdf::{RdfDataset, TermValue};

use crate::annotation::{
    AnnotatedFactKey, AnnotatedQuad, AnnotationCertification, AnnotationDerivation,
    AnnotationFactRef, AnnotationRequest, TupleAnnotationAlgebra,
};
use crate::provenance::ASSERT_RULE_IRI;
use crate::result::PreservationClaim;
use crate::seam::{BudgetStatus, DerivationId, DerivedQuad};

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
/// contains value-inventing rules. Compact textual fixtures are confined to
/// [`materialize_benchmark_existential`].
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
    let profile = match declared_profile {
        Some(profile) => profile,
        None => program_profile(program)?,
    };
    let lowering = crate::relational_core::lower_formulas(program);
    let mut rules = crate::lower::lower_eval_rules(program)
        .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;
    rules.extend(lowering.rules);
    let mut preservation = lowering.preservation;
    let store = crate::store::WorldStore::new();
    store
        .load_dataset(input)
        .map_err(|error| MaterializeError::Parse(error.message().to_owned()))?;

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
        return materialize_nonmonotone(profile, &rules, &store, preservation);
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
            preservation,
            frontier: crate::query_ir::CompletionFrontier::empty(),
            chase_admission: None,
            nonmonotone_solve_runs: Vec::new(),
        });
    };

    let budgeted =
        match crate::physical::materialize_native(&store, executable.as_ref(), limits.max_steps)
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
            annotation: derivation.annotation.clone(),
        })
        .collect()
}

/// Materialize a positive canonical program while carrying opaque tuple annotations.
///
/// `annotation_for` is consulted only for asserted input quads. Returning `None` gives
/// the fact `algebra.one()`, so ordinary RDF remains neutral while selected extensional
/// relations carry BM25/vector/name-similarity scores. Derived annotations are computed
/// inside the native planned join: body conjunction uses `multiply`, and alternative
/// rule groundings use `add`.
///
/// # Errors
///
/// Returns a typed materialization error for an unsupported non-positive/existential
/// program, a contract/query-class mismatch, a caller algebra failure, or a non-convergent
/// annotation fixed point. No partial annotation is returned.
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
    if !lowering.nary_head_rules.is_empty() {
        return Err(MaterializeError::Chase(
            "tuple annotations do not yet admit value-inventing existential heads".to_owned(),
        ));
    }
    let mut rules = crate::lower::lower_eval_rules(program)
        .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;
    rules.extend(lowering.rules);
    let certification = crate::physical::certify_query(&rules, annotation.contract)
        .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;

    let materialization = materialize_program(program, input, limits, declared_profile)?;
    if materialization.chase_admission.is_some()
        || !materialization.nonmonotone_solve_runs.is_empty()
    {
        return Err(MaterializeError::Chase(
            "tuple annotations require the positive semi-naive materialization path".to_owned(),
        ));
    }

    let profile = match declared_profile {
        Some(profile) => profile,
        None => program_profile(program)?,
    };
    let contract_hash = format!("gmeow-materialize-annotated-v1:{}", profile.as_str());
    let lookup = crate::physical::compile_cached(contract_hash, rules);
    let executable = lookup.executable.ok_or_else(|| {
        MaterializeError::Chase(
            "tuple annotations require a stratifiable native executable".to_owned(),
        )
    })?;

    let mut facts_by_world: std::collections::BTreeMap<String, Vec<crate::rule_ir::Fact>> =
        std::collections::BTreeMap::new();
    let mut seeds_by_world: std::collections::BTreeMap<
        String,
        std::collections::BTreeMap<crate::rule_ir::FactKey, A::Element>,
    > = std::collections::BTreeMap::new();
    for quad in &materialization.quads {
        let fact = crate::rule_ir::Fact {
            subject: quad.subject.clone(),
            predicate: quad.predicate.clone(),
            object: quad.object.clone(),
        };
        if quad.rule_iri == ASSERT_RULE_IRI {
            let fact_annotation = (annotation.annotation_for)(AnnotationFactRef {
                world: &quad.graph,
                subject: &quad.subject,
                predicate: &quad.predicate,
                object: &quad.object,
            })
            .unwrap_or_else(|| annotation.algebra.one());
            seeds_by_world
                .entry(quad.graph.clone())
                .or_default()
                .insert(fact.key(), fact_annotation);
        }
        facts_by_world
            .entry(quad.graph.clone())
            .or_default()
            .push(fact);
    }

    let mut evaluations = std::collections::BTreeMap::new();
    for (world, facts) in &facts_by_world {
        let seeds = seeds_by_world.get(world).cloned().unwrap_or_default();
        let evaluation = crate::physical::evaluate_annotations(
            facts,
            executable.as_ref(),
            &seeds,
            &std::collections::BTreeSet::new(),
            annotation.algebra,
            annotation.contract,
        )
        .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;
        evaluations.insert(world.clone(), evaluation);
    }

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

/// Run the repo-owned existential benchmark fixture language through the native chase.
/// Production callers use [`materialize_program`]; this adapter exists only so the
/// committed performance corpus can retain its compact TGD fixtures.
pub fn materialize_benchmark_existential(
    source: &str,
    input: &RdfDataset,
) -> Result<Materialization, MaterializeError> {
    let normalized = source.replace('!', "?");
    let rules = crate::rule_ir::parse_benchmark_rules(&normalized)
        .map_err(|error| MaterializeError::Parse(error.message().to_owned()))?
        .into_iter()
        .map(|rule| crate::physical::ExistentialRule {
            rule_iri: rule.rule_iri,
            body: rule.body,
            head: vec![rule.head],
            distinct: rule.distinct_pairs,
            witness_frontier: None,
        })
        .collect::<Vec<_>>();
    let store = crate::store::WorldStore::new();
    store
        .load_dataset(input)
        .map_err(|error| MaterializeError::Parse(error.message().to_owned()))?;
    let (admission, outcome) = crate::physical::chase_materialize(&store, &rules, None)
        .map_err(|error| MaterializeError::Chase(error.message().to_owned()))?;
    let budgeted = match outcome {
        crate::physical::NativeOutcome::Decided(result) => result,
        crate::physical::NativeOutcome::Unsupported(kind) => {
            return Err(MaterializeError::Chase(format!(
                "native existential benchmark refused {kind:?}: {:?}",
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
