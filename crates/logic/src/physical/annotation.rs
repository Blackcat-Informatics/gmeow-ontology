// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Opaque annotations carried by the native physical closure.
//!
//! The positive/stratified path below owns tuple membership and annotation equations in
//! one fixpoint.  It does not materialize a fact closure and then replay every join.  For
//! physical classes whose solver already returns a selected proof carrier (well-founded,
//! cautious stable-model, and restricted existential chase), [`evaluate_selected_lineage`]
//! folds that carrier without invoking a second reasoner pass.

use std::collections::{BTreeMap, BTreeSet};

use super::plan::Executable;
use super::seminaive::{Delta, StepGovernor, join_body_indexed};
use super::store::RelationStore;
use crate::annotation::{
    AnnotationCertification, AnnotationContract, AnnotationLineageContract, AnnotationQueryClass,
    TupleAnnotationAlgebra,
};
use crate::provenance::{MinProofHeightSemiring, ProofHeight, mint_derivation_id};
use crate::query_ir::CompletionFrontier;
use crate::rule_ir::{
    DerivedRow, EvalRule, Fact, FactKey, distinct_pairs_satisfied, echo_asserted, ground_head,
    sort_rows,
};
use crate::seam::BudgetStatus;

fn annotation_err(detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Physical {
        detail: detail.into(),
    })
}

/// One direct physical rule contribution before a world is attached at the public seam.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PhysicalAnnotationDerivation<E> {
    pub(crate) rule_iri: String,
    pub(crate) sources: Vec<FactKey>,
    pub(crate) annotation: E,
}

/// One closure pass's facts, proof rows, annotations, and resource frontier.
#[derive(Debug, Clone)]
pub(crate) struct AnnotationEvaluation<E> {
    pub(crate) rows: Vec<DerivedRow>,
    pub(crate) facts: Vec<Fact>,
    pub(crate) annotations: BTreeMap<FactKey, E>,
    pub(crate) derivations: BTreeMap<FactKey, Vec<PhysicalAnnotationDerivation<E>>>,
    pub(crate) status: BudgetStatus,
    pub(crate) frontier: CompletionFrontier,
}

/// Borrowed inputs that configure one world's combined membership/annotation pass.
pub(crate) struct AnnotationExecution<'a, A: TupleAnnotationAlgebra> {
    max_steps: Option<u64>,
    seed_annotations: &'a BTreeMap<FactKey, A::Element>,
    control_predicates: &'a BTreeSet<String>,
    algebra: &'a A,
    contract: &'a AnnotationContract,
}

impl<'a, A: TupleAnnotationAlgebra> AnnotationExecution<'a, A> {
    pub(crate) fn new(
        max_steps: Option<u64>,
        seed_annotations: &'a BTreeMap<FactKey, A::Element>,
        control_predicates: &'a BTreeSet<String>,
        algebra: &'a A,
        contract: &'a AnnotationContract,
    ) -> Self {
        Self {
            max_steps,
            seed_annotations,
            control_predicates,
            algebra,
            contract,
        }
    }
}

fn positive_shape(rules: &[EvalRule], nary: bool) -> AnnotationQueryClass {
    let idb: BTreeSet<String> = rules
        .iter()
        .map(|rule| rule.head.predicate.clone())
        .collect();
    let mut edges: BTreeMap<String, BTreeSet<String>> = idb
        .iter()
        .map(|pred| (pred.clone(), BTreeSet::new()))
        .collect();
    let mut indegree: BTreeMap<String, usize> = idb.iter().map(|pred| (pred.clone(), 0)).collect();

    for rule in rules {
        let head = rule.head.predicate.as_str();
        for body in rule.body.iter().filter(|atom| !atom.negated) {
            if idb.contains(&body.predicate)
                && edges
                    .get_mut(head)
                    .expect("every rule head is an IDB node")
                    .insert(body.predicate.clone())
            {
                *indegree
                    .get_mut(&body.predicate)
                    .expect("every IDB body predicate has an indegree slot") += 1;
            }
        }
    }

    let mut ready: BTreeSet<String> = indegree
        .iter()
        .filter_map(|(pred, &degree)| (degree == 0).then_some(pred.clone()))
        .collect();
    let mut visited = 0usize;
    while let Some(pred) = ready.pop_first() {
        visited += 1;
        for next in &edges[&pred] {
            let degree = indegree
                .get_mut(next)
                .expect("every dependency target has an indegree slot");
            *degree -= 1;
            if *degree == 0 {
                ready.insert(next.clone());
            }
        }
    }

    match (nary, visited == idb.len()) {
        (false, true) => AnnotationQueryClass::PositiveAcyclic,
        (false, false) => AnnotationQueryClass::PositiveRecursive,
        (true, true) => AnnotationQueryClass::PositiveNaryAcyclic,
        (true, false) => AnnotationQueryClass::PositiveNaryRecursive,
    }
}

/// Classify the actual binary program, including the explicit stratified-NAF contract.
pub(super) fn classify_query(rules: &[EvalRule]) -> gmeow_errors::Result<AnnotationQueryClass> {
    if rules
        .iter()
        .any(|rule| rule.body.iter().any(|atom| atom.negated))
    {
        return Ok(AnnotationQueryClass::StratifiedNaf);
    }
    Ok(positive_shape(rules, false))
}

/// Admit a caller algebra for an explicitly inspected physical query class.
pub(crate) fn certify_class(
    query_class: AnnotationQueryClass,
    lineage_contract: AnnotationLineageContract,
    contract: &AnnotationContract,
) -> gmeow_errors::Result<AnnotationCertification> {
    if contract.max_fixpoint_rounds == 0 {
        return Err(annotation_err(
            "annotation max_fixpoint_rounds must be greater than zero",
        ));
    }
    match &contract.approximation {
        None => Ok(AnnotationCertification {
            query_class,
            preservation: crate::result::PreservationClaim::exact(),
            declared_deviations: BTreeSet::new(),
            lineage_contract,
        }),
        Some(declaration) => {
            if declaration.deviates_from.is_empty() {
                return Err(annotation_err(
                    "a declared annotation approximation must name at least one violated semiring law",
                ));
            }
            if !declaration.certified_for.contains(&query_class) {
                return Err(annotation_err(format!(
                    "annotation algebra deviation is not certified for actual query class {query_class:?}"
                )));
            }
            let mut preservation = crate::result::PreservationClaim::default();
            preservation.insert(gmeow_logic_compile::ir::PreservationKind::CompleteOver)?;
            Ok(AnnotationCertification {
                query_class,
                preservation,
                declared_deviations: declaration.deviates_from.clone(),
                lineage_contract,
            })
        }
    }
}

impl AnnotationContract {
    /// Certify a physical class that is selected outside the binary rule classifier.
    pub(crate) fn certify_physical_class(
        &self,
        query_class: AnnotationQueryClass,
        lineage_contract: AnnotationLineageContract,
    ) -> gmeow_errors::Result<AnnotationCertification> {
        certify_class(query_class, lineage_contract, self)
    }
}

/// Admit an annotation contract against the inspected binary program.
pub(crate) fn certify_query(
    rules: &[EvalRule],
    contract: &AnnotationContract,
) -> gmeow_errors::Result<AnnotationCertification> {
    certify_class(
        classify_query(rules)?,
        AnnotationLineageContract::AllPhysicalDerivations,
        contract,
    )
}

fn multiply_sources<A: TupleAnnotationAlgebra>(
    algebra: &A,
    sources: &[Fact],
    current: &BTreeMap<FactKey, A::Element>,
    control_predicates: &BTreeSet<String>,
) -> gmeow_errors::Result<A::Element> {
    let mut product = algebra.one();
    for source in sources {
        let factor = if control_predicates.contains(&source.predicate) {
            algebra.one()
        } else {
            current
                .get(&source.key())
                .cloned()
                .unwrap_or_else(|| algebra.zero())
        };
        product = algebra.multiply(&product, &factor)?;
    }
    Ok(product)
}

#[derive(Clone)]
struct MembershipCandidate {
    head: Fact,
    rule_iri: String,
    sources: Vec<Fact>,
    source_ids: Vec<String>,
    sorted_source_ids: Vec<String>,
    derivation_id: String,
    proof_height: ProofHeight,
    source_height_sum: u64,
}

type AnnotationContribution<E> = (MembershipCandidate, E, PhysicalAnnotationDerivation<E>);

impl MembershipCandidate {
    fn key(&self) -> (u32, u64, &[String], &str, &[String]) {
        (
            self.proof_height.get(),
            self.source_height_sum,
            &self.sorted_source_ids,
            &self.rule_iri,
            &self.source_ids,
        )
    }
}

fn membership_candidate(
    head: Fact,
    rule_iri: &str,
    sources: &[Fact],
    heights: &BTreeMap<FactKey, ProofHeight>,
) -> gmeow_errors::Result<MembershipCandidate> {
    let mut source_ids = Vec::with_capacity(sources.len());
    let mut max_height = ProofHeight::ASSERTED;
    let mut source_height_sum = 0_u64;
    for source in sources {
        source_ids.push(source.reifier()?);
        let height = heights
            .get(&source.key())
            .copied()
            .unwrap_or(ProofHeight::ASSERTED);
        max_height = max_height.max(height);
        source_height_sum = source_height_sum.saturating_add(u64::from(height.get()));
    }
    let proof_height = MinProofHeightSemiring.derive([max_height])?;
    let refs = source_ids.iter().map(String::as_str).collect::<Vec<_>>();
    let derivation_id = mint_derivation_id(rule_iri, &refs);
    let mut sorted_source_ids = source_ids.clone();
    sorted_source_ids.sort();
    Ok(MembershipCandidate {
        head,
        rule_iri: rule_iri.to_owned(),
        sources: sources.to_vec(),
        source_ids,
        sorted_source_ids,
        derivation_id,
        proof_height,
        source_height_sum,
    })
}

/// Materialize one world's binary positive/stratified closure and its annotation column
/// in one physical fixpoint.
pub(crate) fn evaluate_annotations<A: TupleAnnotationAlgebra>(
    world: &str,
    edb: &[Fact],
    exe: &Executable,
    execution: AnnotationExecution<'_, A>,
) -> gmeow_errors::Result<AnnotationEvaluation<A::Element>> {
    let AnnotationExecution {
        max_steps,
        seed_annotations,
        control_predicates,
        algebra,
        contract,
    } = execution;
    let mut rel = RelationStore::new();
    let mut facts = BTreeMap::<FactKey, Fact>::new();
    let mut heights = BTreeMap::<FactKey, ProofHeight>::new();
    for fact in edb {
        rel.insert(&fact.predicate, &fact.subject, &fact.object);
        facts.insert(fact.key(), fact.clone());
        heights.insert(fact.key(), ProofHeight::ASSERTED);
    }

    let mut rows = echo_asserted(world, edb)?;
    let mut current = BTreeMap::new();
    let mut derivations = BTreeMap::new();
    for fact in edb {
        let key = fact.key();
        let value = if control_predicates.contains(&fact.predicate) {
            algebra.one()
        } else {
            seed_annotations
                .get(&key)
                .cloned()
                .unwrap_or_else(|| algebra.one())
        };
        current.insert(key.clone(), value.clone());
        derivations.insert(
            key,
            vec![PhysicalAnnotationDerivation {
                rule_iri: crate::provenance::ASSERT_RULE_IRI.to_owned(),
                sources: Vec::new(),
                annotation: value,
            }],
        );
    }

    let mut governor = StepGovernor::new(max_steps);
    let mut saturated_preds = edb
        .iter()
        .map(|fact| fact.predicate.clone())
        .collect::<BTreeSet<_>>();
    let total = exe.stratum_count();
    let mut completed = 0usize;
    let mut status = BudgetStatus::Ok;

    'strata: for stratum in 0..total {
        let rule_indices = exe.stratum_rule_indices(stratum);
        let head_predicates = rule_indices
            .iter()
            .map(|&index| exe.rule_entry(index).0.head.predicate.clone())
            .collect::<BTreeSet<_>>();

        let mut reached_annotation_fixpoint = false;
        for _round in 0..contract.max_fixpoint_rounds {
            let full = Delta::all(rel.row_count());
            let mut contributions: BTreeMap<FactKey, Vec<AnnotationContribution<A::Element>>> =
                BTreeMap::new();
            let mut builtin_gap: Vec<super::builtin_eval::BuiltinGap> = Vec::new();
            for &index in rule_indices {
                let (rule, plan) = exe.rule_entry(index);
                for solution in join_body_indexed(rule, plan, &rel, &rel, full, &mut builtin_gap) {
                    if !distinct_pairs_satisfied(&rule.distinct_pairs, &solution)? {
                        continue;
                    }
                    let head = ground_head(&rule.head, &solution)?;
                    let contribution = if control_predicates.contains(&head.predicate) {
                        algebra.one()
                    } else {
                        multiply_sources(
                            algebra,
                            &solution.source_facts,
                            &current,
                            control_predicates,
                        )?
                    };
                    let candidate = membership_candidate(
                        head.clone(),
                        &rule.rule_iri,
                        &solution.source_facts,
                        &heights,
                    )?;
                    let direct = PhysicalAnnotationDerivation {
                        rule_iri: rule.rule_iri.clone(),
                        sources: solution.source_facts.iter().map(Fact::key).collect(),
                        annotation: contribution.clone(),
                    };
                    contributions.entry(head.key()).or_default().push((
                        candidate,
                        contribution,
                        direct,
                    ));
                }
            }
            if !builtin_gap.is_empty() {
                // Name each gap's `math:` class + operation rather than an anonymous
                // "unsupported arithmetic binding".
                return Err(annotation_err(
                    crate::reason::builtin_gap::builtin_gap_refusal_detail(&builtin_gap),
                ));
            }

            let mut inserted = false;
            for (key, candidates) in &contributions {
                if facts.contains_key(key) {
                    continue;
                }
                if governor.spent() {
                    status = BudgetStatus::Exhausted;
                    break;
                }
                let winner = candidates
                    .iter()
                    .map(|(candidate, _, _)| candidate)
                    .min_by(|left, right| left.key().cmp(&right.key()))
                    .expect("a contribution group is non-empty")
                    .clone();
                rel.insert(
                    &winner.head.predicate,
                    &winner.head.subject,
                    &winner.head.object,
                );
                facts.insert(key.clone(), winner.head.clone());
                heights.insert(key.clone(), winner.proof_height);
                rows.push(DerivedRow {
                    graph: world.to_owned(),
                    subject: winner.head.subject,
                    predicate: winner.head.predicate,
                    object: winner.head.object,
                    rule_iri: winner.rule_iri,
                    source_quad_ids: winner.source_ids,
                    derivation_id: winner.derivation_id,
                    proof_height: winner.proof_height,
                    antecedents: winner.sources,
                });
                governor.charge();
                inserted = true;
            }

            let mut next = current.clone();
            let mut next_derivations = derivations.clone();
            for (key, fact) in &facts {
                if head_predicates.contains(&fact.predicate) {
                    next.insert(key.clone(), algebra.zero());
                    next_derivations.remove(key);
                }
            }
            for (key, seed) in seed_annotations {
                let Some(fact) = facts.get(key) else { continue };
                if !head_predicates.contains(&fact.predicate) {
                    continue;
                }
                let prior = next.get(key).cloned().unwrap_or_else(|| algebra.zero());
                next.insert(key.clone(), algebra.add(&prior, seed)?);
                next_derivations.entry(key.clone()).or_default().push(
                    PhysicalAnnotationDerivation {
                        rule_iri: crate::provenance::ASSERT_RULE_IRI.to_owned(),
                        sources: Vec::new(),
                        annotation: seed.clone(),
                    },
                );
            }
            for (key, candidates) in contributions {
                let Some(fact) = facts.get(&key) else {
                    continue;
                };
                if control_predicates.contains(&fact.predicate) {
                    next.insert(key, algebra.one());
                    continue;
                }
                for (_, contribution, direct) in candidates {
                    let prior = next.get(&key).cloned().unwrap_or_else(|| algebra.zero());
                    next.insert(key.clone(), algebra.add(&prior, &contribution)?);
                    next_derivations
                        .entry(key.clone())
                        .or_default()
                        .push(direct);
                }
            }
            for (key, fact) in &facts {
                if control_predicates.contains(&fact.predicate) {
                    next.insert(key.clone(), algebra.one());
                }
            }

            let annotation_stable = facts.iter().all(|(key, fact)| {
                !head_predicates.contains(&fact.predicate) || next.get(key) == current.get(key)
            });
            current = next;
            derivations = next_derivations;
            if status == BudgetStatus::Exhausted {
                break 'strata;
            }
            if !inserted && annotation_stable {
                reached_annotation_fixpoint = true;
                break;
            }
        }
        if !reached_annotation_fixpoint {
            return Err(annotation_err(format!(
                "annotation fixed point did not converge within {} deterministic rounds",
                contract.max_fixpoint_rounds
            )));
        }
        completed += 1;
        saturated_preds.extend(head_predicates);
    }

    sort_rows(&mut rows);
    Ok(AnnotationEvaluation {
        rows,
        facts: facts.into_values().collect(),
        annotations: current,
        derivations,
        status,
        frontier: CompletionFrontier {
            completed,
            total,
            saturated_preds,
            consumed_steps: governor.consumed,
        },
    })
}

/// Fold annotations through proof rows selected by a non-monotone solver or existential
/// chase. This traverses the emitted lineage only; it never re-runs physical joins.
pub(crate) fn evaluate_selected_lineage<A: TupleAnnotationAlgebra>(
    rows: &[DerivedRow],
    seed_facts: &[Fact],
    seed_annotations: &BTreeMap<FactKey, A::Element>,
    algebra: &A,
    contract: &AnnotationContract,
) -> gmeow_errors::Result<AnnotationEvaluation<A::Element>> {
    let mut facts = seed_facts
        .iter()
        .cloned()
        .map(|fact| (fact.key(), fact))
        .collect::<BTreeMap<_, _>>();
    for row in rows {
        let fact = Fact {
            subject: row.subject.clone(),
            predicate: row.predicate.clone(),
            object: row.object.clone(),
        };
        facts.insert(fact.key(), fact);
    }
    let mut current = facts
        .keys()
        .cloned()
        .map(|key| (key, algebra.zero()))
        .collect::<BTreeMap<_, _>>();

    for _round in 0..contract.max_fixpoint_rounds {
        let mut next = facts
            .keys()
            .cloned()
            .map(|key| (key, algebra.zero()))
            .collect::<BTreeMap<_, _>>();
        let mut derivations = BTreeMap::new();
        for (key, seed) in seed_annotations {
            if !facts.contains_key(key) {
                continue;
            }
            let prior = next.get(key).cloned().unwrap_or_else(|| algebra.zero());
            next.insert(key.clone(), algebra.add(&prior, seed)?);
            derivations
                .entry(key.clone())
                .or_insert_with(Vec::new)
                .push(PhysicalAnnotationDerivation {
                    rule_iri: crate::provenance::ASSERT_RULE_IRI.to_owned(),
                    sources: Vec::new(),
                    annotation: seed.clone(),
                });
        }
        for row in rows
            .iter()
            .filter(|row| row.rule_iri != crate::provenance::ASSERT_RULE_IRI)
        {
            let key = Fact {
                subject: row.subject.clone(),
                predicate: row.predicate.clone(),
                object: row.object.clone(),
            }
            .key();
            let contribution =
                multiply_sources(algebra, &row.antecedents, &current, &BTreeSet::new())?;
            let prior = next.get(&key).cloned().unwrap_or_else(|| algebra.zero());
            next.insert(key.clone(), algebra.add(&prior, &contribution)?);
            derivations
                .entry(key)
                .or_insert_with(Vec::new)
                .push(PhysicalAnnotationDerivation {
                    rule_iri: row.rule_iri.clone(),
                    sources: row.antecedents.iter().map(Fact::key).collect(),
                    annotation: contribution,
                });
        }
        if next == current {
            return Ok(AnnotationEvaluation {
                rows: rows.to_vec(),
                facts: facts.into_values().collect(),
                annotations: next,
                derivations,
                status: BudgetStatus::Ok,
                frontier: CompletionFrontier::empty(),
            });
        }
        current = next;
    }
    Err(annotation_err(format!(
        "annotation fixed point did not converge within {} deterministic rounds",
        contract.max_fixpoint_rounds
    )))
}

impl AnnotationContract {
    /// Fold annotations through a selected native proof carrier without replaying joins.
    pub(crate) fn evaluate_selected_physical_lineage<A: TupleAnnotationAlgebra>(
        &self,
        rows: &[DerivedRow],
        seed_facts: &[Fact],
        seed_annotations: &BTreeMap<FactKey, A::Element>,
        algebra: &A,
    ) -> gmeow_errors::Result<AnnotationEvaluation<A::Element>> {
        evaluate_selected_lineage(rows, seed_facts, seed_annotations, algebra, self)
    }
}
