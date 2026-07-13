// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Opaque annotation fixed point over the native physical plan.
//!
//! Fact closure and annotation closure are deliberately separate columns of the same
//! physical relation.  The ordinary semi-naive pass decides tuple membership and its
//! deterministic resource frontier; this pass then evaluates the annotation equations
//! over that admitted closure.  It enumerates real rule groundings through the same
//! planned index joins, so scores retain direct derivation lineage and are never joined
//! back onto answers post hoc.

use std::collections::{BTreeMap, BTreeSet};

use super::plan::Executable;
use super::seminaive::{Delta, join_body_indexed};
use super::store::RelationStore;
use crate::annotation::{
    AnnotationCertification, AnnotationContract, AnnotationQueryClass, TupleAnnotationAlgebra,
};
use crate::rule_ir::{EvalRule, Fact, FactKey, distinct_pairs_satisfied, ground_head};

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

/// Stable annotation values and direct derivation contributions for one world closure.
#[derive(Debug, Clone)]
pub(crate) struct AnnotationEvaluation<E> {
    pub(crate) annotations: BTreeMap<FactKey, E>,
    pub(crate) derivations: BTreeMap<FactKey, Vec<PhysicalAnnotationDerivation<E>>>,
}

/// Classify the actual positive IDB dependency graph.
///
/// Annotation semirings are monotone.  A negated body is therefore outside this seam and
/// is refused rather than assigned an invented score semantics.
pub(super) fn classify_query(rules: &[EvalRule]) -> gmeow_errors::Result<AnnotationQueryClass> {
    if rules
        .iter()
        .any(|rule| rule.body.iter().any(|atom| atom.negated))
    {
        return Err(annotation_err(
            "tuple annotations require positive Datalog; negation-as-failure has no declared annotation algebra",
        ));
    }

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
        for body in &rule.body {
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

    Ok(if visited == idb.len() {
        AnnotationQueryClass::PositiveAcyclic
    } else {
        AnnotationQueryClass::PositiveRecursive
    })
}

/// Admit an annotation contract against the inspected program class.
pub(crate) fn certify_query(
    rules: &[EvalRule],
    contract: &AnnotationContract,
) -> gmeow_errors::Result<AnnotationCertification> {
    if contract.max_fixpoint_rounds == 0 {
        return Err(annotation_err(
            "annotation max_fixpoint_rounds must be greater than zero",
        ));
    }
    let query_class = classify_query(rules)?;
    match &contract.approximation {
        None => Ok(AnnotationCertification {
            query_class,
            preservation: crate::result::PreservationClaim::exact(),
            declared_deviations: BTreeSet::new(),
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
            })
        }
    }
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

/// Evaluate opaque annotations over an already-admitted tuple closure.
///
/// `seed_annotations` contains asserted EDB values; a missing asserted value is inserted
/// by the caller as `one`.  Magic demand predicates are control tuples: they always carry
/// `one` and are omitted from body products, preventing the demand transform from
/// double-counting a prefix score.
pub(crate) fn evaluate_annotations<A: TupleAnnotationAlgebra>(
    facts: &[Fact],
    exe: &Executable,
    seed_annotations: &BTreeMap<FactKey, A::Element>,
    control_predicates: &BTreeSet<String>,
    algebra: &A,
    contract: &AnnotationContract,
) -> gmeow_errors::Result<AnnotationEvaluation<A::Element>> {
    let closure_keys: BTreeSet<FactKey> = facts.iter().map(Fact::key).collect();
    let mut rel = RelationStore::new();
    for fact in facts {
        rel.insert(&fact.predicate, &fact.subject, &fact.object);
    }
    let full = Delta::all(rel.row_count());

    let mut current: BTreeMap<FactKey, A::Element> = closure_keys
        .iter()
        .cloned()
        .map(|key| (key, algebra.zero()))
        .collect();

    for _round in 0..contract.max_fixpoint_rounds {
        let mut next: BTreeMap<FactKey, A::Element> = closure_keys
            .iter()
            .cloned()
            .map(|key| (key, algebra.zero()))
            .collect();
        let mut derivations: BTreeMap<FactKey, Vec<PhysicalAnnotationDerivation<A::Element>>> =
            BTreeMap::new();

        for (key, annotation) in seed_annotations {
            if !closure_keys.contains(key) {
                continue;
            }
            let prior = next
                .get(key)
                .expect("every admitted seed has an initialized annotation")
                .clone();
            next.insert(key.clone(), algebra.add(&prior, annotation)?);
            derivations
                .entry(key.clone())
                .or_default()
                .push(PhysicalAnnotationDerivation {
                    rule_iri: crate::provenance::ASSERT_RULE_IRI.to_owned(),
                    sources: Vec::new(),
                    annotation: annotation.clone(),
                });
        }

        // Magic demand rows are boolean control, never scored data.
        for fact in facts {
            if control_predicates.contains(&fact.predicate) {
                next.insert(fact.key(), algebra.one());
            }
        }

        let mut builtin_gap = false;
        for stratum in 0..exe.stratum_count() {
            for &index in exe.stratum_rule_indices(stratum) {
                let (rule, plan) = exe.rule_entry(index);
                let solutions = join_body_indexed(rule, plan, &rel, &rel, full, &mut builtin_gap);
                if builtin_gap {
                    return Err(annotation_err(
                        "annotation evaluation encountered an unsupported arithmetic binding",
                    ));
                }
                for solution in solutions {
                    if !distinct_pairs_satisfied(&rule.distinct_pairs, &solution)? {
                        continue;
                    }
                    let head = ground_head(&rule.head, &solution)?;
                    let key = head.key();
                    // A budget-cut fact closure is authoritative: annotations may explain
                    // admitted rows but may not smuggle a post-budget tuple into the result.
                    if !closure_keys.contains(&key) {
                        continue;
                    }
                    if control_predicates.contains(&head.predicate) {
                        next.insert(key, algebra.one());
                        continue;
                    }
                    let contribution = multiply_sources(
                        algebra,
                        &solution.source_facts,
                        &current,
                        control_predicates,
                    )?;
                    let prior = next
                        .get(&key)
                        .expect("every admitted head has an initialized annotation")
                        .clone();
                    next.insert(key.clone(), algebra.add(&prior, &contribution)?);
                    derivations
                        .entry(key)
                        .or_default()
                        .push(PhysicalAnnotationDerivation {
                            rule_iri: rule.rule_iri.clone(),
                            sources: solution.source_facts.iter().map(Fact::key).collect(),
                            annotation: contribution,
                        });
                }
            }
        }

        if next == current {
            return Ok(AnnotationEvaluation {
                annotations: next,
                derivations,
            });
        }
        current = next;
    }

    Err(annotation_err(format!(
        "annotation fixed point did not converge within {} deterministic rounds",
        contract.max_fixpoint_rounds
    )))
}
