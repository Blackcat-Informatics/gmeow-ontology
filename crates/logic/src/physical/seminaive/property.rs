// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Positive schema joins with data-selected predicates on the shared native store.
//! Predicate bindings are typed terms, never parsed RDF strings. Each recursive
//! join retains only its binding frame and ordered premises; delta partitioning
//! visits a solution once even when several body atoms consume new facts.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use purrdf::TermValue;

use super::{Delta, seminaive_err};
use crate::physical::dependency::ReadDependency;
use crate::physical::store::{RelationStore, SkolemRegistry, WitnessContract, metadata_identity};
use crate::reason::value::NativeValues;
use crate::rule_ir::{EvalTerm, Fact};

mod cardinality;
pub(crate) mod datatype;
mod datatype_constraint;
mod list;
mod minimum;
use cardinality::PreparedCardinality;
pub(crate) use cardinality::{CardinalityPattern, CardinalitySet};
pub(crate) use datatype_constraint::DatatypeConstraint;
use datatype_constraint::{DatatypeEvaluation, PreparedDatatypeConstraint};
pub(crate) use list::ListCache;
pub(crate) use list::{ListOperation, ListPattern};
pub(crate) use minimum::MinimumPattern;
use minimum::PreparedMinimum;

/// A native statement pattern, including its predicate as an ordinary typed term.
#[derive(Debug, Clone)]
pub(crate) struct PropertyAtom(pub(crate) [EvalTerm; 3]);

/// A positive native schema law. A generative operation must explicitly declare
/// its invented slots, all output effects and its complete witness frontier.
#[derive(Debug, Clone)]
pub(crate) struct PropertyRule {
    pub(crate) rule_iri: String,
    pub(crate) head: PropertyAtom,
    pub(crate) body: Vec<PropertyAtom>,
    pub(crate) operation: Option<PropertyOperation>,
    pub(crate) guards: Vec<PropertyGuard>,
}

/// Exactly one native relation operation can supplement a positive join. Each
/// variant declares its own binding, dependency and evidence contract.
#[derive(Debug, Clone)]
pub(crate) enum PropertyOperation {
    List(ListPattern),
    Cardinality(CardinalityPattern),
    Datatype(DatatypeConstraint),
    Minimum(MinimumPattern),
}

#[derive(Debug)]
enum PreparedOperation {
    List(PreparedList),
    Cardinality(PreparedCardinality),
    Datatype(PreparedDatatypeConstraint),
    Minimum(PreparedMinimum),
}

/// Pure term and datatype guards over already-bound native values. They read no
/// additional statements and therefore add no hidden completion dependencies.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ValueComparison {
    Equal,
    /// Two differently spelled resource terms eligible for an explicit equality
    /// conclusion. This is a syntactic filter, never a unique-name assumption.
    DifferentResourceTerms,
    /// Use the same admitted count syntax as the native cardinality operators.
    CardinalityEqual,
    /// Compare admitted non-negative counts without a datatype value fallback.
    CardinalityGreater,
    /// At least one operand is a literal, and their values are provably distinct.
    /// Different resource names alone never establish denotational inequality.
    DistinctLiteral,
}

#[derive(Debug, Clone)]
pub(crate) struct PropertyGuard {
    pub(crate) comparison: ValueComparison,
    pub(crate) left: EvalTerm,
    pub(crate) right: EvalTerm,
}

#[derive(Debug)]
struct PreparedGuard {
    comparison: ValueComparison,
    left: Slot,
    right: Slot,
}

#[derive(Debug, Clone)]
enum Slot {
    Variable(usize),
    Constant(TermValue),
}

impl Slot {
    fn value<'a>(&'a self, bindings: &'a [Option<TermValue>]) -> Option<&'a TermValue> {
        match self {
            Self::Variable(index) => bindings[*index].as_ref(),
            Self::Constant(value) => Some(value),
        }
    }

    fn bind(&self, value: &TermValue, bindings: &mut [Option<TermValue>]) -> bool {
        match self {
            Self::Constant(expected) => expected == value,
            Self::Variable(index) => match &bindings[*index] {
                Some(expected) => expected == value,
                None => {
                    bindings[*index] = Some(value.clone());
                    true
                }
            },
        }
    }
}

/// Immutable slot layout shared by selected templates, input shapes and rounds.
/// Cloning this handle never clones rule IR, list operators or binding layouts.
#[derive(Debug, Clone)]
pub(crate) struct PreparedPropertyRule(Arc<PropertyLayout>);

/// Read-only native law and its prepared slots. Only the validating constructor
/// can create a layout; shared handles expose no mutable access.
#[derive(Debug)]
pub(crate) struct PropertyLayout {
    pub(crate) source: PropertyRule,
    source_identity: [u8; 32],
    pub(crate) analysis_body: Vec<PropertyAtom>,
    pub(crate) analysis_heads: Vec<PropertyAtom>,
    /// Generative families depend on every body argument, including definition
    /// and count carriers that do not themselves appear in an emitted statement.
    pub(crate) witness_frontier: Option<Vec<String>>,
    head: [Slot; 3],
    body: Vec<[Slot; 3]>,
    slots: usize,
    operation: Option<PreparedOperation>,
    guards: Vec<PreparedGuard>,
    source_selections: Vec<SourceSelection>,
}

#[derive(Debug)]
struct SourceSelection {
    family: crate::reason::dl::DlConstructFamily,
    slots: [Slot; 3],
    structured: bool,
}

pub(super) struct PropertyVisit {
    pub(super) complete: bool,
    pub(super) admission_blocked: bool,
}

impl std::ops::Deref for PreparedPropertyRule {
    type Target = PropertyLayout;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone)]
struct PreparedList {
    head: Slot,
    outputs: Vec<Slot>,
}

#[derive(Clone, Copy)]
struct JoinContext<'a> {
    rel: &'a RelationStore,
    delta: Delta,
    pivot: usize,
}

/// One borrowed evaluation scope; list entries belong to the frozen round while
/// immutable datatype meanings remain reusable across rounds of this world.
#[derive(Default)]
pub(crate) struct SchemaValues {
    values: NativeValues,
    datatypes: datatype::DatatypeCache,
}

impl SchemaValues {
    /// Borrow the existing world caches; callers must complete all definition
    /// writers before retaining a datatype plan across native rounds.
    pub(crate) fn parts(&mut self) -> (&mut NativeValues, &mut datatype::DatatypeCache) {
        (&mut self.values, &mut self.datatypes)
    }

    /// Exact constructor, facet and list read inventory of the shared compiler.
    pub(crate) fn datatype_reads() -> impl Iterator<Item = &'static str> {
        datatype::reads()
    }
}

struct NativeCaches<'a> {
    admissions: &'a crate::reason::dl::SourceCoverageWorld,
    admission_blocked: bool,
    datatypes: &'a mut datatype::DatatypeCache,
    lists: &'a mut ListCache,
    values: &'a mut NativeValues,
    witnesses: NativeWitnesses<'a>,
}

/// The existing world-scoped execution and resource authority, borrowed by a
/// dynamic witness operation; ordinary property joins do not mint anything.
pub(super) struct NativeWitnesses<'a> {
    pub(super) world: &'a str,
    pub(super) contract: WitnessContract,
    pub(super) registry: &'a mut SkolemRegistry,
    pub(super) limit: usize,
}

impl PreparedPropertyRule {
    pub(crate) fn new(source: PropertyRule) -> gmeow_errors::Result<Self> {
        let mut variables = BTreeMap::new();
        for atom in &source.body {
            for term in &atom.0 {
                if let EvalTerm::Var(name) = term {
                    let next = variables.len();
                    variables.entry(name.clone()).or_insert(next);
                }
            }
        }
        if let Some(PropertyOperation::List(list)) = &source.operation {
            if let EvalTerm::Var(root) = &list.head
                && !variables.contains_key(root)
            {
                return Err(seminaive_err(
                    "a logical list root must be bound by its rule body",
                ));
            }
            if matches!(
                list.operation,
                ListOperation::DistinctFromAll(_)
                    | ListOperation::KeyValues(..)
                    | ListOperation::KeyRecordValues(..)
            ) {
                for term in list.operation.terms() {
                    if let EvalTerm::Var(name) = term
                        && !variables.contains_key(name)
                    {
                        return Err(seminaive_err(
                            "a universal value guard requires body-bound subjects",
                        ));
                    }
                }
            }
            for term in list.operation.terms() {
                if let EvalTerm::Var(name) = term {
                    let next = variables.len();
                    variables.entry(name.clone()).or_insert(next);
                }
            }
        }
        let bound_terms = match &source.operation {
            Some(PropertyOperation::Cardinality(pattern)) => pattern.terms(),
            Some(PropertyOperation::Datatype(pattern)) => pattern.terms(),
            Some(PropertyOperation::Minimum(pattern)) => pattern.terms(),
            Some(PropertyOperation::List(_)) | None => Vec::new(),
        };
        for term in bound_terms {
            if let EvalTerm::Var(name) = term
                && !variables.contains_key(name)
            {
                return Err(seminaive_err(
                    "a native value constraint requires body-bound inputs",
                ));
            }
        }
        let witness_frontier = if let Some(PropertyOperation::Minimum(pattern)) = &source.operation
        {
            if source.head.0 != pattern.head().0 || !source.guards.is_empty() {
                return Err(seminaive_err(
                    "a minimum family requires its declared head and no unrelated guard",
                ));
            }
            if variables.contains_key(&pattern.witness) {
                return Err(seminaive_err(
                    "a minimum witness must be fresh relative to its body",
                ));
            }
            let frontier: Vec<String> = variables.keys().cloned().collect();
            variables.insert(pattern.witness.clone(), variables.len());
            Some(frontier)
        } else {
            None
        };
        let lower =
            |term: &EvalTerm| -> gmeow_errors::Result<Slot> {
                match term {
                    EvalTerm::Var(name) => variables
                        .get(name)
                        .copied()
                        .map(Slot::Variable)
                        .ok_or_else(|| {
                            seminaive_err(format!(
                                "property rule {} has unbound head variable {name}",
                                source.rule_iri
                            ))
                        }),
                    EvalTerm::ConstNamed(iri) => Ok(Slot::Constant(TermValue::iri(iri.clone()))),
                    EvalTerm::ConstLit(value) => Ok(Slot::Constant(value.clone())),
                }
            };
        let atom = |atom: &PropertyAtom| -> gmeow_errors::Result<[Slot; 3]> {
            Ok([lower(&atom.0[0])?, lower(&atom.0[1])?, lower(&atom.0[2])?])
        };
        let head = atom(&source.head)?;
        let body = source
            .body
            .iter()
            .map(atom)
            .collect::<gmeow_errors::Result<Vec<_>>>()?;
        if body.is_empty() {
            return Err(seminaive_err(
                "a native property law requires a positive body",
            ));
        }
        let operation = source
            .operation
            .as_ref()
            .map(|operation| -> gmeow_errors::Result<_> {
                Ok(match operation {
                    PropertyOperation::List(list) => PreparedOperation::List(PreparedList {
                        head: lower(&list.head)?,
                        outputs: list
                            .operation
                            .terms()
                            .into_iter()
                            .map(lower)
                            .collect::<gmeow_errors::Result<_>>()?,
                    }),
                    PropertyOperation::Cardinality(pattern) => {
                        PreparedOperation::Cardinality(PreparedCardinality {
                            slots: pattern
                                .terms()
                                .into_iter()
                                .map(lower)
                                .collect::<gmeow_errors::Result<_>>()?,
                        })
                    }
                    PropertyOperation::Datatype(pattern) => {
                        PreparedOperation::Datatype(PreparedDatatypeConstraint {
                            slots: pattern
                                .terms()
                                .into_iter()
                                .map(lower)
                                .collect::<gmeow_errors::Result<_>>()?,
                        })
                    }
                    PropertyOperation::Minimum(pattern) => {
                        PreparedOperation::Minimum(PreparedMinimum {
                            selection: PreparedCardinality {
                                slots: pattern
                                    .terms()
                                    .into_iter()
                                    .map(lower)
                                    .collect::<gmeow_errors::Result<_>>()?,
                            },
                            frontier: witness_frontier
                                .as_ref()
                                .expect("generative frontier")
                                .iter()
                                .map(|name| lower(&EvalTerm::var(name)))
                                .collect::<gmeow_errors::Result<_>>()?,
                        })
                    }
                })
            })
            .transpose()?;
        let analysis_body = analysis_body(&source);
        let analysis_heads = match &source.operation {
            Some(PropertyOperation::Minimum(pattern)) => {
                pattern.analysis_heads(variables.keys().cloned().collect())
            }
            _ => vec![source.head.clone()],
        };
        let guards = source
            .guards
            .iter()
            .map(|guard| {
                Ok(PreparedGuard {
                    comparison: guard.comparison,
                    left: lower(&guard.left)?,
                    right: lower(&guard.right)?,
                })
            })
            .collect::<gmeow_errors::Result<_>>()?;
        let mut source_selections = Vec::new();
        for source_atom in &source.body {
            let EvalTerm::ConstNamed(predicate) = &source_atom.0[1] else {
                continue;
            };
            let object = match &source_atom.0[2] {
                EvalTerm::ConstNamed(iri) => Some(TermValue::iri(iri.clone())),
                EvalTerm::ConstLit(value) => Some(value.clone()),
                EvalTerm::Var(_) => None,
            };
            for family in crate::reason::dl::source_selector_families(predicate, object.as_ref()) {
                source_selections.push(SourceSelection {
                    family,
                    slots: atom(source_atom)?,
                    structured: crate::reason::dl::source_execution_requires_admission(
                        family, predicate,
                    ),
                });
            }
        }
        let source_identity = metadata_identity("gmeow-property-source-v1", &source);
        Ok(Self(Arc::new(PropertyLayout {
            source_identity,
            source,
            analysis_body,
            analysis_heads,
            witness_frontier,
            head,
            body,
            slots: variables.len(),
            operation,
            guards,
            source_selections,
        })))
    }

    /// Only selected structure-bearing owner interpretations await whole fields.
    /// Atomic relation propagation retains its own positive dependencies.
    pub(crate) fn admission_reads(&self) -> Vec<crate::reason::refute::NativeRead> {
        self.source_selections
            .iter()
            .filter(|selection| selection.structured)
            .flat_map(|selection| crate::reason::dl::source_admission_reads_for(selection.family))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// Predicate names read syntactically; None is a data-selected predicate.
    pub(crate) fn reads(&self) -> impl Iterator<Item = (Option<&str>, ReadDependency)> {
        use ReadDependency::{Completed, Positive};
        let mut reads: Vec<_> = self
            .selection_reads()
            .map(|predicate| (predicate, Positive))
            .collect();
        if let Some(PropertyOperation::List(list)) = &self.source.operation {
            if matches!(list.operation, ListOperation::KeyRecordValues(..)) {
                // An unordered composite key can grow without becoming malformed.
                // Its complete definition is a strict dependency, while new value
                // matches remain positive inputs in this stratum's fixed point.
                reads.extend([
                    (Some(list::KEY_CLASS), Completed),
                    (Some(list::KEY_PROPERTY), Completed),
                ]);
            } else {
                reads.extend([(Some(list::FIRST), Positive), (Some(list::REST), Positive)]);
            }
            match list.operation {
                ListOperation::Chain(..)
                | ListOperation::KeyValues(..)
                | ListOperation::KeyRecordValues(..) => reads.push((None, Positive)),
                ListOperation::AllTypes(_) => reads.push((Some(list::TYPE), Positive)),
                ListOperation::DistinctFromAll(_) => reads.push((Some(list::DIFFERENT), Positive)),
                ListOperation::Member(_) | ListOperation::Pair(..) => {}
            }
        }
        if let Some(PropertyOperation::Cardinality(cardinality)) = &self.source.operation {
            reads.extend(cardinality.reads());
        }
        if let Some(PropertyOperation::Datatype(datatype)) = &self.source.operation {
            reads.extend(datatype.reads());
        }
        // A restricted firing inspects existing positive witnesses. Head growth
        // can only block a trigger, so it needs no extra delta pivot or absence edge.
        if let Some(PropertyOperation::Minimum(pattern)) = &self.source.operation {
            reads.extend([(None, Positive), (Some(list::DIFFERENT), Positive)]);
            if pattern.class.is_some() {
                reads.push((Some(list::TYPE), Positive));
            }
        }
        reads.into_iter()
    }

    /// List admission remains required even when a structural cell is missing.
    pub(crate) fn selection_reads(&self) -> impl Iterator<Item = Option<&str>> {
        self.source.body.iter().map(|atom| match &atom.0[1] {
            EvalTerm::ConstNamed(iri) => Some(iri.as_str()),
            EvalTerm::Var(_) | EvalTerm::ConstLit(_) => None,
        })
    }

    /// Every possible head predicate, including dynamic family outputs.
    pub(crate) fn head_predicates(&self) -> impl Iterator<Item = Option<&str>> {
        self.analysis_heads.iter().map(|head| match &head.0[1] {
            EvalTerm::ConstNamed(iri) => Some(iri.as_str()),
            EvalTerm::Var(_) | EvalTerm::ConstLit(_) => None,
        })
    }

    /// A constant absent from the entire generatable term universe cannot match.
    /// This sufficient pruning rule never uses a bounded sample or current join result.
    pub(crate) fn constants_available(&self, iris: &BTreeSet<String>) -> bool {
        self.source
            .body
            .iter()
            .flat_map(|atom| &atom.0)
            .all(|term| !matches!(term, EvalTerm::ConstNamed(iri) if !iris.contains(iri)))
    }

    /// Stream newly enabled candidates; false means the caller's buffer bound cut.
    pub(super) fn visit(
        &self,
        rel: &RelationStore,
        delta: Delta,
        lists: &mut ListCache,
        values: &mut SchemaValues,
        witnesses: NativeWitnesses<'_>,
        admissions: &crate::reason::dl::SourceCoverageWorld,
        emit: &mut impl FnMut(&str, Fact, &[Fact]) -> gmeow_errors::Result<bool>,
    ) -> gmeow_errors::Result<PropertyVisit> {
        let mut caches = NativeCaches {
            admissions,
            admission_blocked: false,
            lists,
            values: &mut values.values,
            datatypes: &mut values.datatypes,
            witnesses,
        };
        // Datatype constraints read completed definitions only. Their bound
        // values already come from the positive body, so no extra pivot is needed.
        let relation_pivot = matches!(
            self.operation,
            Some(PreparedOperation::List(_) | PreparedOperation::Cardinality(_))
        );
        for pivot in 0..(self.body.len() + usize::from(relation_pivot)) {
            let mut bindings = vec![None; self.slots];
            let mut premises = Vec::with_capacity(self.body.len());
            if !self.join(
                JoinContext { rel, delta, pivot },
                0,
                &mut bindings,
                &mut premises,
                &mut caches,
                emit,
            )? {
                return Ok(PropertyVisit {
                    complete: false,
                    admission_blocked: caches.admission_blocked,
                });
            }
        }
        Ok(PropertyVisit {
            complete: true,
            admission_blocked: caches.admission_blocked,
        })
    }

    fn join(
        &self,
        context: JoinContext<'_>,
        position: usize,
        bindings: &mut Vec<Option<TermValue>>,
        premises: &mut Vec<Fact>,
        caches: &mut NativeCaches<'_>,
        emit: &mut impl FnMut(&str, Fact, &[Fact]) -> gmeow_errors::Result<bool>,
    ) -> gmeow_errors::Result<bool> {
        let JoinContext { rel, delta, pivot } = context;
        if position == self.body.len() {
            return self.finish(context, bindings, premises, caches, emit);
        }
        let atom = &self.body[position];
        let predicate = atom[1].value(bindings).cloned();
        let predicates: Vec<&str> = match &predicate {
            Some(TermValue::Iri(iri)) => vec![iri.as_str()],
            Some(_) => return Ok(true),
            None => rel.predicates().collect(),
        };
        let saved = bindings.clone();
        let bound_predicate = predicate.is_some();
        let constant_object = matches!(atom[2], Slot::Constant(_));
        for predicate in predicates {
            let mut cursor = rel.select_pattern(
                predicate,
                atom[0].value(&saved),
                atom[2].value(&saved),
                constant_object,
                bound_predicate,
            );
            while let Some((s, o, row, actual_predicate)) = cursor.next() {
                let fresh = (delta.lo..delta.hi).contains(&row.index());
                if (position < pivot && fresh) || (position == pivot && !fresh) {
                    continue;
                }
                bindings.clone_from(&saved);
                let subject = rel.interner().resolve(s);
                let object = rel.interner().resolve(o);
                if !atom[0].bind(subject, bindings)
                    || (!bound_predicate
                        && !atom[1].bind(&TermValue::iri(actual_predicate), bindings))
                    || (!constant_object && !atom[2].bind(object, bindings))
                {
                    continue;
                }
                premises.push(Fact {
                    subject: subject.clone(),
                    predicate: actual_predicate.to_owned(),
                    object: object.clone(),
                });
                let complete =
                    self.join(context, position + 1, bindings, premises, caches, emit)?;
                premises.pop();
                if !complete {
                    return Ok(false);
                }
            }
        }
        bindings.clone_from(&saved);
        Ok(true)
    }

    fn emit_head(
        &self,
        bindings: &[Option<TermValue>],
        premises: &[Fact],
        values: &mut NativeValues,
        admission_blocked: &mut bool,
        emit: &mut impl FnMut(&str, Fact, &[Fact]) -> gmeow_errors::Result<bool>,
    ) -> gmeow_errors::Result<bool> {
        for guard in &self.guards {
            let left = guard.left.value(bindings).expect("checked guard binding");
            let right = guard.right.value(bindings).expect("checked guard binding");
            if matches!(guard.comparison, ValueComparison::DifferentResourceTerms) {
                if left == right
                    || !matches!(left, TermValue::Iri(_) | TermValue::Blank { .. })
                    || !matches!(right, TermValue::Iri(_) | TermValue::Blank { .. })
                {
                    return Ok(true);
                }
                continue;
            }
            if matches!(
                guard.comparison,
                ValueComparison::CardinalityEqual | ValueComparison::CardinalityGreater
            ) {
                let count = values.cardinality(left).zip(values.cardinality(right));
                let Some((left, right)) = count else {
                    return Err(seminaive_err(
                        "a cardinality guard requires non-negative integer fields",
                    ));
                };
                let accepted = match guard.comparison {
                    ValueComparison::CardinalityEqual => left == right,
                    ValueComparison::CardinalityGreater => left > right,
                    _ => unreachable!("selected cardinality guard"),
                };
                if !accepted {
                    return Ok(true);
                }
                continue;
            }
            if matches!(guard.comparison, ValueComparison::DistinctLiteral)
                && !matches!(left, TermValue::Literal { .. })
                && !matches!(right, TermValue::Literal { .. })
            {
                return Ok(true);
            }
            let Some(equal) = values.same_value(left, right) else {
                *admission_blocked = true;
                return Ok(true);
            };
            let accepted = match guard.comparison {
                ValueComparison::Equal => equal,
                ValueComparison::DistinctLiteral => !equal,
                ValueComparison::DifferentResourceTerms
                | ValueComparison::CardinalityEqual
                | ValueComparison::CardinalityGreater => {
                    unreachable!("term and count guards were evaluated before datatype comparison")
                }
            };
            if !accepted {
                return Ok(true);
            }
        }
        let subject = self.head[0].value(bindings).expect("checked head binding");
        let predicate = self.head[1].value(bindings).expect("checked head binding");
        let object = self.head[2].value(bindings).expect("checked head binding");
        let TermValue::Iri(predicate) = predicate else {
            return Err(seminaive_err(format!(
                "property rule {} selected a non-IRI predicate",
                self.source.rule_iri
            )));
        };
        // Literal membership has its own typed datatype handler.
        if !matches!(subject, TermValue::Iri(_) | TermValue::Blank { .. }) {
            return Ok(true);
        }
        emit(
            &self.source.rule_iri,
            Fact {
                subject: subject.clone(),
                predicate: predicate.clone(),
                object: object.clone(),
            },
            premises,
        )
    }

    fn finish(
        &self,
        context: JoinContext<'_>,
        bindings: &mut Vec<Option<TermValue>>,
        premises: &mut Vec<Fact>,
        caches: &mut NativeCaches<'_>,
        emit: &mut impl FnMut(&str, Fact, &[Fact]) -> gmeow_errors::Result<bool>,
    ) -> gmeow_errors::Result<bool> {
        // Admission precedes every interpretation, including list traversal and
        // NPA target matching. Another owner cannot authorize this bound solution.
        for selection in &self.source_selections {
            let subject = selection.slots[0]
                .value(bindings)
                .expect("body-bound source owner");
            let predicate = selection.slots[1]
                .value(bindings)
                .and_then(TermValue::as_iri)
                .expect("fixed source predicate");
            let object = selection.slots[2]
                .value(bindings)
                .expect("body-bound source operand");
            let valid_operand = crate::reason::dl::source_operand_issue(
                selection.family,
                predicate,
                subject,
                object,
            )
            .is_none();
            let admitted = !selection.structured
                || caches
                    .admissions
                    .admission(selection.family, subject)
                    .is_some_and(|admission| {
                        admission.completion
                            == crate::reason::refute::NativeFamilyCompletion::Complete
                            && admission.obstructions.is_empty()
                    });
            if !valid_operand || !admitted {
                caches.admission_blocked = true;
                return Ok(true);
            }
        }
        if let (
            Some(PropertyOperation::Minimum(source)),
            Some(PreparedOperation::Minimum(pattern)),
        ) = (&self.source.operation, &self.operation)
        {
            return pattern.visit(
                source,
                bindings,
                context,
                caches,
                (&self.source.rule_iri, self.source_identity, premises),
                emit,
            );
        }
        if let (
            Some(PropertyOperation::Datatype(source)),
            Some(PreparedOperation::Datatype(pattern)),
        ) = (&self.source.operation, &self.operation)
        {
            let witness = match pattern.evaluate(source, bindings, context.rel, caches)? {
                DatatypeEvaluation::NoConflict => return Ok(true),
                DatatypeEvaluation::Conflict(witness) => witness,
                DatatypeEvaluation::Undecided => {
                    caches.admission_blocked = true;
                    return Ok(true);
                }
            };
            let body_premises = premises.len();
            premises.extend_from_slice(witness.premises());
            let result = self.emit_head(
                bindings,
                premises,
                caches.values,
                &mut caches.admission_blocked,
                emit,
            );
            premises.truncate(body_premises);
            return result;
        }
        if let (
            Some(PropertyOperation::Cardinality(source)),
            Some(PreparedOperation::Cardinality(cardinality)),
        ) = (&self.source.operation, &self.operation)
        {
            let (path, fresh) =
                match cardinality.evaluate(source, bindings, context.rel, caches, context.delta)? {
                    cardinality::CountOutcome::Found(path, fresh) => (path, fresh),
                    cardinality::CountOutcome::Absent => return Ok(true),
                    cardinality::CountOutcome::Exhausted => return Ok(false),
                };
            if context.pivot == self.body.len() && !fresh {
                return Ok(true);
            }
            let body_premises = premises.len();
            premises.extend(path);
            let result = self.emit_head(
                bindings,
                premises,
                caches.values,
                &mut caches.admission_blocked,
                emit,
            );
            premises.truncate(body_premises);
            return result;
        }
        let (Some(PropertyOperation::List(source)), Some(PreparedOperation::List(pattern))) =
            (&self.source.operation, &self.operation)
        else {
            assert!(
                self.source.operation.is_none() && self.operation.is_none(),
                "prepared native operation must match its immutable source"
            );
            return self.emit_head(
                bindings,
                premises,
                caches.values,
                &mut caches.admission_blocked,
                emit,
            );
        };
        let root = pattern.head.value(bindings).expect("body-bound list root");
        let operation = &source.operation;
        let list = if matches!(operation, ListOperation::KeyRecordValues(..)) {
            caches.lists.read_key_record(context.rel, root)?
        } else {
            caches.lists.read(context.rel, root)?
        };
        let Some(list) = list else {
            return Ok(true);
        };
        let body_premises = premises.len();
        for fact in list.premises(context.rel) {
            if !premises.contains(&fact) {
                premises.push(fact);
            }
        }
        let schema_premises = premises.len();
        let structure_fresh = list.fresh(context.delta);
        let saved = bindings.clone();
        let key_evidence = if matches!(
            operation,
            ListOperation::KeyValues(..) | ListOperation::KeyRecordValues(..)
        ) {
            list::key_agreement(
                context.rel,
                &list,
                [
                    pattern.outputs[0]
                        .value(&saved)
                        .expect("body-bound key subject"),
                    pattern.outputs[1]
                        .value(&saved)
                        .expect("body-bound key subject"),
                ],
                caches.values,
                context.delta,
            )?
        } else {
            None
        };
        let mut publish = |outputs: &[crate::facts::TermId], path: &[Fact], data_fresh: bool| {
            if context.pivot == self.body.len() && !structure_fresh && !data_fresh {
                return Ok(true);
            }
            bindings.clone_from(&saved);
            for (slot, value) in pattern.outputs.iter().zip(outputs) {
                if !slot.bind(context.rel.interner().resolve(*value), bindings) {
                    return Ok(true);
                }
            }
            premises.truncate(schema_premises);
            premises.extend_from_slice(path);
            self.emit_head(
                bindings,
                premises,
                caches.values,
                &mut caches.admission_blocked,
                emit,
            )
        };
        let complete = match operation {
            ListOperation::KeyValues(..) | ListOperation::KeyRecordValues(..) => match key_evidence
            {
                Some((path, fresh)) => publish(&[], &path, fresh)?,
                None => true,
            },
            ListOperation::Member(_) => {
                let mut complete = true;
                for member in &list.members {
                    if !publish(&[*member], &[], false)? {
                        complete = false;
                        break;
                    }
                }
                complete
            }
            ListOperation::Pair(..) => {
                let mut complete = true;
                'pairs: for (index, left) in list.members.iter().enumerate() {
                    for right in &list.members[index + 1..] {
                        if !publish(&[*left, *right], &[], false)? {
                            complete = false;
                            break 'pairs;
                        }
                    }
                }
                complete
            }
            ListOperation::AllTypes(_) | ListOperation::DistinctFromAll(_) => list::universal(
                context.rel,
                &list,
                pattern.outputs[0].value(&saved),
                operation,
                context.delta,
                &mut |subject, path, fresh| publish(&[subject], path, fresh),
            )?,
            ListOperation::Chain(..) => list::chain(
                context.rel,
                &list,
                pattern.outputs[0].value(&saved),
                context.delta,
                &mut |s, o, path, fresh| publish(&[s, o], path, fresh),
            )?,
        };
        bindings.clone_from(&saved);
        premises.truncate(body_premises);
        Ok(complete)
    }
}

/// Every concrete list match maps to these body facts. Connectivity between
/// cells and between first/last chain edges is relaxed only in this analysis.
fn analysis_body(source: &PropertyRule) -> Vec<PropertyAtom> {
    // Cardinality and datatype constraints bind no new terms. Omitting their filtering is a
    // conservative term-flow abstraction; their actual relation reads remain
    // explicit in the signed producer effects and execution's delta pivot.
    let mut body = source.body.clone();
    let Some(PropertyOperation::List(list)) = &source.operation else {
        return body;
    };
    let mut names: BTreeSet<_> = source
        .body
        .iter()
        .chain(std::iter::once(&source.head))
        .flat_map(|atom| &atom.0)
        .chain(list.operation.terms())
        .chain(std::iter::once(&list.head))
        .filter_map(|term| match term {
            EvalTerm::Var(name) => Some(name.clone()),
            _ => None,
        })
        .collect();
    match &list.operation {
        ListOperation::Member(value) => member_effect(&mut body, &mut names, value.clone()),
        ListOperation::Pair(left, right) => {
            member_effect(&mut body, &mut names, left.clone());
            member_effect(&mut body, &mut names, right.clone());
        }
        ListOperation::AllTypes(subject) => {
            // This operator admits only nonempty lists. Every concrete output has
            // at least one member type; relaxing the other member checks preserves
            // the complete term-flow over-approximation for joint chase admission.
            let member = fresh_analysis_var(&mut names);
            member_effect(&mut body, &mut names, member.clone());
            body.push(PropertyAtom([
                subject.clone(),
                EvalTerm::named(list::TYPE),
                member,
            ]));
        }
        // This guard binds no new value. Dropping a finite universal test is a
        // conservative over-approximation, including the empty-list case. Requiring
        // a member here would incorrectly omit empty-list producers from admission.
        ListOperation::DistinctFromAll(_)
        | ListOperation::KeyValues(..)
        | ListOperation::KeyRecordValues(..) => {}
        ListOperation::Chain(start, end) => {
            let first = fresh_analysis_var(&mut names);
            let last = fresh_analysis_var(&mut names);
            member_effect(&mut body, &mut names, first.clone());
            member_effect(&mut body, &mut names, last.clone());
            body.push(PropertyAtom([
                start.clone(),
                first,
                fresh_analysis_var(&mut names),
            ]));
            body.push(PropertyAtom([
                fresh_analysis_var(&mut names),
                last,
                end.clone(),
            ]));
        }
    }
    body
}

fn fresh_analysis_var(names: &mut BTreeSet<String>) -> EvalTerm {
    let mut index = names.len();
    loop {
        let name = format!("?native_list_analysis_{index}");
        if names.insert(name.clone()) {
            return EvalTerm::var(&name);
        }
        index += 1;
    }
}

fn member_effect(body: &mut Vec<PropertyAtom>, names: &mut BTreeSet<String>, value: EvalTerm) {
    let cell = fresh_analysis_var(names);
    body.push(PropertyAtom([
        cell.clone(),
        EvalTerm::named(list::FIRST),
        value,
    ]));
    body.push(PropertyAtom([
        cell,
        EvalTerm::named(list::REST),
        fresh_analysis_var(names),
    ]));
}
