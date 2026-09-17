// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Compact object-witness families on the native restricted chase. Counts never
//! expand the execution template. Required inequality facts are streamed from one
//! linear witness inventory after a complete existing-witness probe.

use super::cardinality::{CountOutcome, CountSearch, PreparedCardinality};
use super::{CardinalitySet, Fact, JoinContext, NativeCaches, PropertyAtom, Slot, seminaive_err};
use crate::physical::store::SkolemTerm;
use crate::rule_ir::EvalTerm;
use purrdf::TermValue;
use std::collections::BTreeSet;

const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const THING: &str = "http://www.w3.org/2002/07/owl#Thing";
const ON_CLASS: &str = "http://www.w3.org/2002/07/owl#onClass";
const DIFFERENT: &str = "http://www.w3.org/2002/07/owl#differentFrom";

/// An admitted object-property obligation. Datatype witnesses require their own
/// value-space constructor and cannot enter this resource-valued operation.
#[derive(Clone, Debug)]
pub(crate) struct MinimumPattern {
    pub(crate) subject: EvalTerm,
    pub(crate) property: EvalTerm,
    pub(crate) minimum: EvalTerm,
    pub(crate) class: Option<EvalTerm>,
    pub(crate) witness: String,
}

impl MinimumPattern {
    pub(super) fn terms(&self) -> Vec<&EvalTerm> {
        let mut terms = vec![&self.subject, &self.property, &self.minimum];
        terms.extend(self.class.as_ref());
        terms
    }

    pub(super) fn head(&self) -> PropertyAtom {
        PropertyAtom([
            self.subject.clone(),
            self.property.clone(),
            EvalTerm::var(&self.witness),
        ])
    }

    /// Two symbolic ordinals cover every position of a finite witness family.
    /// Multiplicity changes no position or frontier dependency. Both ordinals
    /// carry every property/type head, and their inequality adds both directions
    /// of value flow without materializing an input-sized rule.
    pub(super) fn analysis_heads(&self, mut names: BTreeSet<String>) -> Vec<PropertyAtom> {
        let other = super::fresh_analysis_var(&mut names);
        let first = EvalTerm::var(&self.witness);
        let mut heads = Vec::new();
        for witness in [&first, &other] {
            heads.push(PropertyAtom([
                self.subject.clone(),
                self.property.clone(),
                witness.clone(),
            ]));
            if let Some(class) = &self.class {
                heads.push(PropertyAtom([
                    witness.clone(),
                    EvalTerm::named(TYPE),
                    class.clone(),
                ]));
            }
        }
        heads.push(PropertyAtom([first, EvalTerm::named(DIFFERENT), other]));
        heads
    }
}

#[derive(Debug)]
pub(super) struct PreparedMinimum {
    pub(super) selection: PreparedCardinality,
    /// All body-bound arguments, including the count and definition carriers.
    /// Termination admission uses this same complete frontier.
    pub(super) frontier: Vec<Slot>,
}

impl PreparedMinimum {
    pub(super) fn visit(
        &self,
        source: &MinimumPattern,
        bindings: &[Option<TermValue>],
        context: JoinContext<'_>,
        caches: &mut NativeCaches<'_>,
        firing: (&str, [u8; 32], &[Fact]),
        emit: &mut impl FnMut(&str, Fact, &[Fact]) -> gmeow_errors::Result<bool>,
    ) -> gmeow_errors::Result<bool> {
        let get = |index: usize| {
            self.selection.slots[index]
                .value(bindings)
                .expect("body-bound witness input")
        };
        let subject = get(0);
        if !matches!(subject, TermValue::Iri(_) | TermValue::Blank { .. }) {
            return Err(seminaive_err(
                "an object witness subject must be a resource",
            ));
        }
        let TermValue::Iri(property) = get(1) else {
            return Err(seminaive_err("an object witness property must be an IRI"));
        };
        let class = source.class.as_ref().map(|_| get(3));
        if class.is_some_and(|class| !matches!(class, TermValue::Iri(_) | TermValue::Blank { .. }))
        {
            return Err(seminaive_err("an object witness class must be a resource"));
        }
        // The admitted onClass operand denotes the universal resource class in
        // either declared spelling. Interpret this class role through the selected
        // semantics; preserve the original bound term in source/witness evidence.
        let class = class.filter(|class| {
            !matches!(class, TermValue::Iri(iri) if iri == THING
                || context.rel.semantics.alternate_marker(ON_CLASS, iri) == Some(THING))
        });
        let minimum = caches.values.cardinality(get(2)).ok_or_else(|| {
            seminaive_err("a cardinality minimum requires a non-negative integer field")
        })?;
        if minimum == 0 {
            return Ok(true);
        }
        let set = if class.is_some() {
            CardinalitySet::Class(source.class.clone().expect("selected class"))
        } else {
            CardinalitySet::Resources
        };
        match self.selection.search(
            &set,
            bindings,
            context.rel,
            caches,
            context.delta,
            CountSearch {
                minimum,
                record_evidence: false,
            },
        )? {
            CountOutcome::Found(..) => return Ok(true),
            CountOutcome::Exhausted => return Ok(false),
            CountOutcome::Absent => {}
        }
        // Check the whole conjunctive firing before allocating any witness. The
        // existing DL materialization ceiling also bounds an unbudgeted run;
        // exceeding it withholds completion, never claims a negative answer.
        let limit = caches.witnesses.limit.min(
            usize::try_from(crate::physical::DL_CHASE_STEP_BACKSTOP)
                .expect("DL working-set ceiling fits every supported host"),
        );
        let row_count = minimum
            .checked_mul(minimum - 1)
            .map(|pairs| pairs / 2)
            .and_then(|pairs| {
                minimum
                    .checked_mul(1 + u128::from(class.is_some()))
                    .and_then(|rows| rows.checked_add(pairs))
            });
        if row_count.is_none_or(|rows| rows > limit as u128) {
            return Ok(false);
        }
        let count = usize::try_from(minimum).expect("bounded by firing row ceiling");
        let (rule_iri, source_identity, premises) = firing;
        let scope = caches
            .witnesses
            .contract
            .scope(caches.witnesses.world, source_identity);
        let frontier: Vec<_> = self
            .frontier
            .iter()
            .map(|slot| {
                slot.value(bindings)
                    .expect("body-bound witness frontier")
                    .clone()
            })
            .collect();
        let witnesses: Vec<_> = (0..count)
            .map(|ordinal| {
                caches.witnesses.registry.mint(SkolemTerm {
                    scope: scope.clone(),
                    rule_iri: rule_iri.to_owned(),
                    ordinal,
                    frontier: frontier.clone(),
                })
            })
            .collect();
        let mut emit_head = |introduced: &[TermValue], fact: Fact| {
            caches
                .witnesses
                .registry
                .record_head(&scope, introduced, &fact, premises)?;
            emit(rule_iri, fact, premises)
        };
        let mut complete = true;
        for (index, witness) in witnesses.iter().enumerate() {
            complete &= emit_head(
                std::slice::from_ref(witness),
                Fact {
                    subject: subject.clone(),
                    predicate: property.clone(),
                    object: witness.clone(),
                },
            )?;
            if let Some(class) = class {
                complete &= emit_head(
                    std::slice::from_ref(witness),
                    Fact {
                        subject: witness.clone(),
                        predicate: TYPE.to_owned(),
                        object: class.clone(),
                    },
                )?;
            }
            for other in &witnesses[..index] {
                complete &= emit_head(
                    &[other.clone(), witness.clone()],
                    Fact {
                        subject: other.clone(),
                        predicate: DIFFERENT.to_owned(),
                        object: witness.clone(),
                    },
                )?;
            }
        }
        Ok(complete)
    }
}
