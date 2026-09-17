// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Witnessed upper-bound violations over one native value relation. Qualification
//! and pairwise inequality retain source rows; the search stores compact local
//! candidates and one clique path, never combinations of materialized joins.

use super::{Delta, Fact, NativeCaches, RelationStore, Slot, datatype, seminaive_err};
use crate::facts::TermId;
use crate::physical::dependency::ReadDependency;
use crate::physical::id::RowId;
use crate::physical::store::Bound;
use crate::reason::value::NativeValues;
use crate::rule_ir::EvalTerm;
use purrdf::TermValue;

const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const DIFFERENT: &str = "http://www.w3.org/2002/07/owl#differentFrom";

/// The exact set whose maximum is constrained. Resource qualification is the
/// universal object class, with no requirement for an explicit Thing assertion.
#[derive(Clone, Debug)]
pub(crate) enum CardinalitySet {
    AllValues,
    Resources,
    Class(EvalTerm),
    Datatype(EvalTerm),
}

#[derive(Clone, Debug)]
pub(crate) struct CardinalityPattern {
    pub(crate) subject: EvalTerm,
    pub(crate) property: EvalTerm,
    pub(crate) maximum: EvalTerm,
    pub(crate) set: CardinalitySet,
}

impl CardinalityPattern {
    pub(super) fn terms(&self) -> Vec<&EvalTerm> {
        let mut terms = vec![&self.subject, &self.property, &self.maximum];
        if let CardinalitySet::Class(term) | CardinalitySet::Datatype(term) = &self.set {
            terms.push(term);
        }
        terms
    }

    pub(super) fn reads(&self) -> impl Iterator<Item = (Option<&str>, ReadDependency)> {
        [None, Some(DIFFERENT)]
            .into_iter()
            .chain(matches!(self.set, CardinalitySet::Class(_)).then_some(Some(TYPE)))
            .map(|predicate| (predicate, ReadDependency::Positive))
            .chain(
                matches!(self.set, CardinalitySet::Datatype(_))
                    .then_some(())
                    .into_iter()
                    .flat_map(|()| datatype::reads())
                    .map(|predicate| (Some(predicate), ReadDependency::Completed)),
            )
    }
}

#[derive(Debug)]
pub(super) struct PreparedCardinality {
    pub(super) slots: Vec<Slot>,
}

/// Presence probes share the complete distinct-subset search without collecting
/// a quadratic proof that a restricted witness firing would immediately discard.
pub(super) struct CountSearch {
    pub(super) minimum: u128,
    pub(super) record_evidence: bool,
}

/// A completed absence proof is distinct from an unfinished search. In
/// particular, exhaustion cannot authorize a restricted-chase witness family.
pub(super) enum CountOutcome {
    Found(Vec<Fact>, bool),
    Absent,
    Exhausted,
}

#[derive(Clone, Copy)]
struct Edge<'a> {
    subject: TermId,
    object: TermId,
    row: RowId,
    predicate: &'a str,
}

impl Edge<'_> {
    fn fact(self, rel: &RelationStore) -> Fact {
        Fact {
            subject: rel.interner().resolve(self.subject).clone(),
            predicate: self.predicate.to_owned(),
            object: rel.interner().resolve(self.object).clone(),
        }
    }

    fn fresh(self, delta: Delta) -> bool {
        (delta.lo..delta.hi).contains(&self.row.index())
    }
}

#[derive(Clone, Copy)]
struct Candidate<'a> {
    edge: Edge<'a>,
    qualification: Option<Edge<'a>>,
}

fn edge<'a>(
    rel: &'a RelationStore,
    predicate: &'a str,
    subject: TermId,
    object: TermId,
) -> Option<Edge<'a>> {
    rel.select_semantic(predicate, Bound::Both(subject, object))
        .next()
        .map(|(subject, object, row, predicate)| Edge {
            subject,
            object,
            row,
            predicate,
        })
}

/// Positive inequality evidence, open-world absence of evidence, or an
/// unavailable datatype comparison. Those three outcomes are never conflated.
enum Distinctness<'a> {
    Witness(Option<Edge<'a>>),
    Unproved,
    Undefined,
}

fn distinct<'a>(
    rel: &'a RelationStore,
    values: &mut NativeValues,
    left: TermId,
    right: TermId,
) -> Distinctness<'a> {
    if left == right {
        return Distinctness::Unproved;
    }
    let a = rel.interner().resolve(left);
    let b = rel.interner().resolve(right);
    if matches!(a, TermValue::Literal { .. }) || matches!(b, TermValue::Literal { .. }) {
        return match values.same_value(a, b) {
            Some(false) => Distinctness::Witness(None),
            Some(true) => Distinctness::Unproved,
            None => Distinctness::Undefined,
        };
    }
    match edge(rel, DIFFERENT, left, right).or_else(|| edge(rel, DIFFERENT, right, left)) {
        Some(proof) => Distinctness::Witness(Some(proof)),
        None => Distinctness::Unproved,
    }
}

impl PreparedCardinality {
    pub(super) fn evaluate(
        &self,
        source: &CardinalityPattern,
        bindings: &[Option<TermValue>],
        rel: &RelationStore,
        caches: &mut NativeCaches<'_>,
        delta: Delta,
    ) -> gmeow_errors::Result<CountOutcome> {
        let maximum = caches
            .values
            .cardinality(self.slots[2].value(bindings).expect("body-bound maximum"))
            .ok_or_else(|| {
                seminaive_err("a cardinality maximum requires a non-negative integer field")
            })?;
        self.search(
            &source.set,
            bindings,
            rel,
            caches,
            delta,
            CountSearch {
                // Every native store has fewer than u128::MAX rows. Saturation
                // therefore preserves the impossible-overflow result at that bound.
                minimum: maximum.saturating_add(1),
                record_evidence: true,
            },
        )
    }

    pub(super) fn search(
        &self,
        set: &CardinalitySet,
        bindings: &[Option<TermValue>],
        rel: &RelationStore,
        caches: &mut NativeCaches<'_>,
        delta: Delta,
        request: CountSearch,
    ) -> gmeow_errors::Result<CountOutcome> {
        let get = |index: usize| {
            self.slots[index]
                .value(bindings)
                .expect("body-bound cardinality input")
        };
        let subject = get(0);
        let TermValue::Iri(property) = get(1) else {
            return Err(seminaive_err("a cardinality property must be an IRI"));
        };
        // Structural reads are certified complete before this rule's stratum.
        // The plan can therefore survive later value rounds without stale lowering.
        let datatype = if matches!(set, CardinalitySet::Datatype(_)) {
            Some(
                caches
                    .datatypes
                    .prepare(rel, get(3), caches.lists, caches.values)?,
            )
        } else {
            None
        };
        let values = &mut *caches.values;
        if request.minimum == 0 {
            return Ok(CountOutcome::Found(Vec::new(), false));
        }
        let Some(subject) = rel.term_id(subject) else {
            return Ok(CountOutcome::Absent);
        };
        // Even counting every raw row cannot exceed this upper bound. Huge source
        // counts do not cause a huge allocation or a lossy usize conversion.
        let upper = rel.len_for(property).saturating_add(
            rel.semantics
                .alternate_predicate(property)
                .map_or(0, |alternate| rel.len_for(alternate)),
        );
        if request.minimum > upper as u128 {
            return Ok(CountOutcome::Absent);
        }
        let mut cursor = rel.select_semantic(property, Bound::Subject(subject));
        let mut candidates = Vec::new();
        let mut unknown = false;
        while let Some((subject, object, row, predicate)) = cursor.next() {
            let term = rel.interner().resolve(object);
            let qualification = match set {
                CardinalitySet::AllValues => None,
                CardinalitySet::Resources => {
                    if !matches!(term, TermValue::Iri(_) | TermValue::Blank { .. }) {
                        continue;
                    }
                    None
                }
                CardinalitySet::Class(_) => {
                    if !matches!(get(3), TermValue::Iri(_) | TermValue::Blank { .. }) {
                        return Err(seminaive_err("a cardinality class must be a resource"));
                    }
                    let Some(class) = rel.term_id(get(3)) else {
                        continue;
                    };
                    let Some(proof) = edge(rel, TYPE, object, class) else {
                        continue;
                    };
                    Some(proof)
                }
                CardinalitySet::Datatype(_) => {
                    match datatype
                        .as_ref()
                        .expect("prepared datatype")
                        .contains(term, values)
                    {
                        Some(true) => {}
                        Some(false) => continue,
                        None => {
                            unknown = true;
                            continue;
                        }
                    }
                    None
                }
            };
            // A presence-only existential needs just one admitted value. Do not
            // scan or retain the remainder of a large already-satisfied relation.
            if request.minimum == 1 && !request.record_evidence {
                return Ok(CountOutcome::Found(Vec::new(), false));
            }
            if candidates.len() == caches.witnesses.limit {
                return Ok(CountOutcome::Exhausted);
            }
            candidates.push(Candidate {
                edge: Edge {
                    subject,
                    object,
                    row,
                    predicate,
                },
                qualification,
            });
        }
        // The count is now bounded by the finite local candidate inventory.
        if request.minimum > candidates.len() as u128 {
            return if unknown {
                Err(seminaive_err(
                    "native cardinality requires an undefined datatype membership",
                ))
            } else {
                Ok(CountOutcome::Absent)
            };
        }
        let needed = usize::try_from(request.minimum).expect("bounded by candidate length");
        let mut selected = Vec::<usize>::new();
        let mut next = 0;
        let mut comparisons = 0usize;
        loop {
            if selected.len() == needed {
                if !request.record_evidence {
                    return Ok(CountOutcome::Found(Vec::new(), false));
                }
                let mut premises = datatype
                    .as_ref()
                    .map_or_else(Vec::new, |plan| plan.premises.clone());
                let mut fresh = false;
                for (position, index) in selected.iter().enumerate() {
                    let candidate = candidates[*index];
                    for proof in std::iter::once(candidate.edge).chain(candidate.qualification) {
                        fresh |= proof.fresh(delta);
                        premises.push(proof.fact(rel));
                    }
                    for previous in &selected[..position] {
                        if let Distinctness::Witness(Some(proof)) = distinct(
                            rel,
                            values,
                            candidate.edge.object,
                            candidates[*previous].edge.object,
                        ) {
                            fresh |= proof.fresh(delta);
                            premises.push(proof.fact(rel));
                        }
                    }
                }
                return Ok(CountOutcome::Found(premises, fresh));
            }
            if candidates.len() - next < needed - selected.len() {
                let Some(previous) = selected.pop() else {
                    break;
                };
                next = previous + 1;
                continue;
            }
            let candidate = next;
            next += 1;
            let mut admitted = true;
            for previous in &selected {
                if comparisons == caches.witnesses.limit {
                    return Ok(CountOutcome::Exhausted);
                }
                comparisons += 1;
                match distinct(
                    rel,
                    values,
                    candidates[*previous].edge.object,
                    candidates[candidate].edge.object,
                ) {
                    Distinctness::Witness(_) => {}
                    Distinctness::Unproved => {
                        admitted = false;
                        break;
                    }
                    Distinctness::Undefined => {
                        unknown = true;
                        admitted = false;
                        break;
                    }
                }
            }
            if admitted {
                selected.push(candidate);
            }
        }
        if unknown {
            Err(seminaive_err(
                "native cardinality requires an undefined datatype comparison or membership",
            ))
        } else {
            Ok(CountOutcome::Absent)
        }
    }
}
