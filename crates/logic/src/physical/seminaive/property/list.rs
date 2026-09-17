// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete, witnessed logical lists over the current native relation store.
//! This is the logical admission boundary: a partial or ambiguous list cannot
//! justify enumeration, distinctness or property composition. The round-local
//! cache holds only bounded native IDs; it never serializes a dataset or a term.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use purrdf::TermValue;

use super::{Delta, Fact, RelationStore, seminaive_err};
use crate::facts::TermId;
use crate::physical::cursor::LendingIterator;
use crate::physical::id::RowId;
use crate::physical::store::Bound;
use crate::reason::value::NativeValues;
use crate::rule_ir::EvalTerm;

pub(crate) const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
pub(crate) const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
pub(crate) const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
pub(crate) const DIFFERENT: &str = "http://www.w3.org/2002/07/owl#differentFrom";
pub(crate) const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
pub(crate) const KEY_CLASS: &str = "https://blackcatinformatics.ca/logic/keyClass";
pub(crate) const KEY_PROPERTY: &str = "https://blackcatinformatics.ca/logic/keyProperty";

/// The outputs of a complete list pattern, bound after its ordinary body.
#[derive(Debug, Clone)]
pub(crate) enum ListOperation {
    Member(EvalTerm),
    Pair(EvalTerm, EvalTerm),
    Chain(EvalTerm, EvalTerm),
    /// Bind subjects with every member type of a nonempty list. Empty intersections
    /// have separate resource-domain laws and do not use this indexed join.
    AllTypes(EvalTerm),
    /// A body-bound subject is explicitly distinct from every nominal member.
    DistinctFromAll(EvalTerm),
    /// Two body-bound subjects share a value for every property in a nonempty list.
    KeyValues(EvalTerm, EvalTerm),
    /// The same guard over a completed canonical composite-key record.
    KeyRecordValues(EvalTerm, EvalTerm),
}

impl ListOperation {
    pub(super) fn terms(&self) -> Vec<&EvalTerm> {
        match self {
            Self::Member(member) | Self::AllTypes(member) | Self::DistinctFromAll(member) => {
                vec![member]
            }
            Self::Pair(left, right)
            | Self::Chain(left, right)
            | Self::KeyValues(left, right)
            | Self::KeyRecordValues(left, right) => vec![left, right],
        }
    }
}

/// The root is bound by the ordinary body; outputs may extend that binding.
#[derive(Debug, Clone)]
pub(crate) struct ListPattern {
    pub(crate) head: EvalTerm,
    pub(crate) operation: ListOperation,
}

#[derive(Clone, Copy)]
struct Edge {
    subject: TermId,
    predicate: &'static str,
    object: TermId,
    row: RowId,
}

pub(crate) struct NativeList {
    pub(crate) members: Vec<TermId>,
    edges: Vec<Edge>,
}

impl NativeList {
    pub(super) fn fresh(&self, delta: Delta) -> bool {
        self.edges
            .iter()
            .any(|edge| (delta.lo..delta.hi).contains(&edge.row.index()))
    }

    pub(crate) fn premises(&self, rel: &RelationStore) -> Vec<Fact> {
        self.edges
            .iter()
            .map(|edge| Fact {
                subject: rel.interner().resolve(edge.subject).clone(),
                predicate: edge.predicate.to_owned(),
                object: rel.interner().resolve(edge.object).clone(),
            })
            .collect()
    }

    fn retained_bytes(&self) -> usize {
        self.members.capacity() * std::mem::size_of::<TermId>()
            + self.edges.capacity() * std::mem::size_of::<Edge>()
            + std::mem::size_of::<Self>()
    }
}

/// Exact available source support from the one logical-list traversal.
#[derive(Debug)]
pub(crate) struct LogicalListFailure {
    pub(crate) diagnostic: gmeow_errors::Diag,
    pub(crate) premises: Vec<Fact>,
}

impl LogicalListFailure {
    pub(crate) fn message(&self) -> &str {
        self.diagnostic.message()
    }
}
impl std::fmt::Display for LogicalListFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.diagnostic, f)
    }
}
impl From<LogicalListFailure> for gmeow_errors::Diag {
    fn from(failure: LogicalListFailure) -> Self {
        failure.diagnostic
    }
}

fn failure(
    rel: &RelationStore,
    edges: &[Edge],
    diagnostic: gmeow_errors::Diag,
) -> LogicalListFailure {
    LogicalListFailure {
        diagnostic,
        premises: edges
            .iter()
            .map(|edge| Fact {
                subject: rel.interner().resolve(edge.subject).clone(),
                predicate: edge.predicate.to_owned(),
                object: rel.interner().resolve(edge.object).clone(),
            })
            .collect(),
    }
}

/// Shared by every property operator in one immutable round of one world.
#[derive(Default)]
pub(crate) struct ListCache {
    entries: BTreeMap<(DefinitionKind, TermId), Arc<NativeList>>,
    bytes: usize,
    /// Exact partial fields from the last unresolved read; reset on every read.
    pending_premises: Vec<Fact>,
    /// Missing cells may be produced by later rounds. Quiescence must reject them.
    pub(in crate::physical::seminaive) pending: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum DefinitionKind {
    RdfList,
    KeyRecord,
}

impl ListCache {
    /// Borrow the exact partial support retained by the immediately preceding
    /// unresolved read. This does not retry traversal or inspect another source.
    pub(crate) fn pending_premises(&self) -> &[Fact] {
        &self.pending_premises
    }

    pub(crate) fn read(
        &mut self,
        rel: &RelationStore,
        head: &TermValue,
    ) -> Result<Option<Arc<NativeList>>, LogicalListFailure> {
        self.pending_premises.clear();
        if !matches!(head, TermValue::Iri(_) | TermValue::Blank { .. }) {
            return Err(failure(
                rel,
                &[],
                seminaive_err("a logical list root must be a resource"),
            ));
        }
        let root = rel.term_id(head);
        if let Some(cached) =
            root.and_then(|root| self.entries.get(&(DefinitionKind::RdfList, root)))
        {
            return Ok(Some(Arc::clone(cached)));
        }
        let mut node = root;
        let mut is_nil = matches!(head, TermValue::Iri(iri) if iri == NIL);
        let mut seen = BTreeSet::new();
        let mut result = NativeList {
            members: Vec::new(),
            edges: Vec::new(),
        };
        loop {
            if is_nil {
                if let Some(node) = node {
                    let first = edge(rel, node, FIRST, &mut result.edges)
                        .map_err(|error| failure(rel, &result.edges, error))?;
                    let rest = edge(rel, node, REST, &mut result.edges)
                        .map_err(|error| failure(rel, &result.edges, error))?;
                    if first.is_some() || rest.is_some() {
                        return Err(failure(
                            rel,
                            &result.edges,
                            seminaive_err("rdf:nil cannot carry logical list cells"),
                        ));
                    }
                }
                break;
            }
            let Some(current) = node else {
                self.pending = true;
                self.pending_premises = result.premises(rel);
                return Ok(None);
            };
            if !seen.insert(current) {
                return Err(failure(
                    rel,
                    &result.edges,
                    seminaive_err("a cyclic RDF list cannot certify a logical list"),
                ));
            }
            let first = edge(rel, current, FIRST, &mut result.edges)
                .map_err(|error| failure(rel, &result.edges, error))?;
            let rest = edge(rel, current, REST, &mut result.edges)
                .map_err(|error| failure(rel, &result.edges, error))?;
            let (Some(first), Some(rest)) = (first, rest) else {
                self.pending = true;
                self.pending_premises = result.premises(rel);
                return Ok(None);
            };
            result.members.push(first.object);
            let next = rel.interner().resolve(rest.object);
            if !matches!(next, TermValue::Iri(_) | TermValue::Blank { .. }) {
                return Err(failure(
                    rel,
                    &result.edges,
                    seminaive_err("a logical list tail must be a resource"),
                ));
            }
            is_nil = matches!(next, TermValue::Iri(iri) if iri == NIL);
            node = Some(rest.object);
        }
        let result = Arc::new(result);
        let bytes = result.retained_bytes();
        if let Some(root) = root
            && self.entries.len() < 64
            && self.bytes.saturating_add(bytes) <= 256 * 1024
        {
            self.entries
                .insert((DefinitionKind::RdfList, root), Arc::clone(&result));
            self.bytes += bytes;
        }
        Ok(Some(result))
    }

    /// Called only after the joint schedule completes every keyClass/keyProperty
    /// writer. A later property would strengthen this conjunction, so a round-local
    /// snapshot alone is not authority to execute a canonical key guard.
    pub(crate) fn read_key_record(
        &mut self,
        rel: &RelationStore,
        record: &TermValue,
    ) -> Result<Option<Arc<NativeList>>, LogicalListFailure> {
        self.pending_premises.clear();
        if !matches!(record, TermValue::Iri(_) | TermValue::Blank { .. }) {
            return Err(failure(
                rel,
                &[],
                seminaive_err("a canonical key record must be a resource"),
            ));
        }
        let Some(root) = rel.term_id(record) else {
            return Err(failure(
                rel,
                &[],
                seminaive_err("a canonical key record is absent"),
            ));
        };
        let key = (DefinitionKind::KeyRecord, root);
        if let Some(cached) = self.entries.get(&key) {
            return Ok(Some(Arc::clone(cached)));
        }
        let mut result = NativeList {
            members: Vec::new(),
            edges: Vec::new(),
        };
        let class = edge(rel, root, KEY_CLASS, &mut result.edges)
            .map_err(|error| failure(rel, &result.edges, error))?
            .ok_or_else(|| {
                failure(
                    rel,
                    &result.edges,
                    seminaive_err("a canonical key requires exactly one keyClass"),
                )
            })?;
        if !matches!(
            rel.interner().resolve(class.object),
            TermValue::Iri(_) | TermValue::Blank { .. }
        ) {
            return Err(failure(
                rel,
                &result.edges,
                seminaive_err("a keyClass must be a resource"),
            ));
        }
        let mut cursor = rel.select(KEY_PROPERTY, Bound::Subject(root));
        let mut invalid_property = false;
        while let Some((subject, object, row)) = cursor.next() {
            result.edges.push(Edge {
                subject,
                predicate: KEY_PROPERTY,
                object,
                row,
            });
            invalid_property |= !matches!(rel.interner().resolve(object), TermValue::Iri(_));
            result.members.push(object);
        }
        if invalid_property {
            return Err(failure(
                rel,
                &result.edges,
                seminaive_err("a keyProperty must be an IRI"),
            ));
        }
        if result.members.is_empty() {
            return Err(failure(
                rel,
                &result.edges,
                seminaive_err("a canonical key requires at least one keyProperty"),
            ));
        }
        let result = Arc::new(result);
        let bytes = result.retained_bytes();
        if self.entries.len() < 64 && self.bytes.saturating_add(bytes) <= 256 * 1024 {
            self.entries.insert(key, Arc::clone(&result));
            self.bytes += bytes;
        }
        Ok(Some(result))
    }
}

fn edge(
    rel: &RelationStore,
    subject: TermId,
    predicate: &'static str,
    evidence: &mut Vec<Edge>,
) -> gmeow_errors::Result<Option<Edge>> {
    let mut cursor = rel.select(predicate, Bound::Subject(subject));
    let mut first = None;
    let mut multiple = false;
    while let Some((subject, object, row)) = cursor.next() {
        let edge = Edge {
            subject,
            predicate,
            object,
            row,
        };
        evidence.push(edge);
        if first.is_some() {
            multiple = true;
        } else {
            first = Some(edge);
        }
    }
    if multiple {
        return Err(seminaive_err(format!(
            "a logical list cell has multiple {predicate} values"
        )));
    }
    Ok(first)
}

/// Retain one real value pair per component, never a product of intermediate
/// joins. Missing values withhold agreement; undefined comparisons cannot certify
/// a negative answer. Resource names keep term identity, without a unique-name
/// assumption or an implicit equality quotient.
pub(super) fn key_agreement(
    rel: &RelationStore,
    list: &NativeList,
    subjects: [&TermValue; 2],
    values: &mut NativeValues,
    delta: Delta,
) -> gmeow_errors::Result<Option<(Vec<Fact>, bool)>> {
    if list.members.is_empty() {
        return Err(seminaive_err("a native key requires at least one property"));
    }
    let [Some(left), Some(right)] = subjects.map(|subject| rel.term_id(subject)) else {
        return Ok(None);
    };
    let mut premises = Vec::new();
    let mut fresh = false;
    for member in &list.members {
        let TermValue::Iri(predicate) = rel.interner().resolve(*member) else {
            return Err(seminaive_err("a key property must be an IRI"));
        };
        let mut left_values = rel.select_semantic(predicate, Bound::Subject(left));
        let mut shared = None;
        let mut undefined = false;
        'pairs: while let Some(a) = left_values.next() {
            // Exact native identity has a direct index probe. Only differing
            // literal spellings need value-space comparisons; resource keys do
            // not scan a cross product of unrelated property values.
            if let Some(b) = rel
                .select_semantic(predicate, Bound::Both(right, a.1))
                .next()
            {
                shared = Some([a, b]);
                break;
            }
            if !matches!(rel.interner().resolve(a.1), TermValue::Literal { .. }) {
                continue;
            }
            let mut right_values = rel.select_semantic(predicate, Bound::Subject(right));
            while let Some(b) = right_values.next() {
                if !matches!(rel.interner().resolve(b.1), TermValue::Literal { .. }) {
                    continue;
                }
                match values.same_value(rel.interner().resolve(a.1), rel.interner().resolve(b.1)) {
                    Some(true) => {
                        shared = Some([a, b]);
                        break 'pairs;
                    }
                    Some(false) => {}
                    None => undefined = true,
                }
            }
        }
        let Some(pair) = shared else {
            if undefined {
                return Err(seminaive_err(
                    "native key agreement requires an undefined datatype value comparison",
                ));
            }
            return Ok(None);
        };
        for (subject, object, row, actual_predicate) in pair {
            fresh |= (delta.lo..delta.hi).contains(&row.index());
            premises.push(Fact {
                subject: rel.interner().resolve(subject).clone(),
                predicate: actual_predicate.to_owned(),
                object: rel.interner().resolve(object).clone(),
            });
        }
    }
    Ok(Some((premises, fresh)))
}

/// Stream arbitrary-length compositions without materializing intermediate joins.
pub(super) fn chain(
    rel: &RelationStore,
    list: &NativeList,
    start: Option<&TermValue>,
    delta: Delta,
    emit: &mut impl FnMut(TermId, TermId, &[Fact], bool) -> gmeow_errors::Result<bool>,
) -> gmeow_errors::Result<bool> {
    if list.members.len() < 2 {
        return Err(seminaive_err(
            "an OWL property chain requires at least two properties",
        ));
    }
    let predicates = list
        .members
        .iter()
        .map(|member| match rel.interner().resolve(*member) {
            TermValue::Iri(iri) => Ok(iri.as_str()),
            _ => Err(seminaive_err("a property chain member must be an IRI")),
        })
        .collect::<gmeow_errors::Result<Vec<_>>>()?;
    let bound = match start {
        Some(start) => match rel.term_id(start) {
            Some(start) => Bound::Subject(start),
            None => return Ok(true),
        },
        None => Bound::Any,
    };
    let mut cursors = vec![rel.select_semantic(predicates[0], bound)];
    let mut path = Vec::new();
    let mut rows = Vec::new();
    let mut subject = None;
    while let Some(cursor) = cursors.last_mut() {
        if let Some((s, o, row, predicate)) = cursor.next() {
            let index = cursors.len() - 1;
            if index == 0 {
                subject = Some(s);
            }
            path.truncate(index);
            rows.truncate(index);
            path.push(Fact {
                subject: rel.interner().resolve(s).clone(),
                predicate: predicate.to_owned(),
                object: rel.interner().resolve(o).clone(),
            });
            rows.push(row);
            if cursors.len() == predicates.len() {
                let fresh = rows
                    .iter()
                    .any(|row| (delta.lo..delta.hi).contains(&row.index()));
                if !emit(subject.expect("first chain edge"), o, &path, fresh)? {
                    return Ok(false);
                }
            } else {
                cursors.push(rel.select_semantic(predicates[index + 1], Bound::Subject(o)));
            }
        } else {
            cursors.pop();
        }
    }
    Ok(true)
}

/// Test a finite universal guard over an admitted complete list. Every successful
/// member supplies a real ordered premise; failure supplies no negative fact.
/// Distinctness accepts either explicit orientation and never assumes unique names.
pub(super) fn universal(
    rel: &RelationStore,
    list: &NativeList,
    subject: Option<&TermValue>,
    operation: &ListOperation,
    delta: Delta,
    emit: &mut impl FnMut(TermId, &[Fact], bool) -> gmeow_errors::Result<bool>,
) -> gmeow_errors::Result<bool> {
    let subject = match subject {
        Some(value) => match rel.term_id(value) {
            Some(subject) => Some(subject),
            None => return Ok(true),
        },
        None => None,
    };
    if matches!(operation, ListOperation::AllTypes(_)) {
        let Some(first) = list.members.first() else {
            return Ok(true);
        };
        // Each subject occurs once in this relation. Starting at an actual member
        // avoids rescanning every unrelated type and repeating the full guard for
        // each type of the same subject. No candidate table is materialized.
        let bound = subject.map_or(Bound::Object(*first), |s| Bound::Both(s, *first));
        let mut candidates = rel.select_semantic(TYPE, bound);
        while let Some((subject, _, _, _)) = candidates.next() {
            if !check_members(rel, list, subject, operation, delta, emit)? {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    check_members(
        rel,
        list,
        subject.ok_or_else(|| seminaive_err("distinctness requires a body-bound subject"))?,
        operation,
        delta,
        emit,
    )
}

fn check_members(
    rel: &RelationStore,
    list: &NativeList,
    subject: TermId,
    operation: &ListOperation,
    delta: Delta,
    emit: &mut impl FnMut(TermId, &[Fact], bool) -> gmeow_errors::Result<bool>,
) -> gmeow_errors::Result<bool> {
    let predicate = match operation {
        ListOperation::AllTypes(_) => TYPE,
        ListOperation::DistinctFromAll(_) => DIFFERENT,
        _ => return Err(seminaive_err("non-universal logical list guard")),
    };
    let symmetric = matches!(operation, ListOperation::DistinctFromAll(_));
    let mut premises = Vec::with_capacity(list.members.len());
    let mut fresh = false;
    for member in &list.members {
        let mut cursor = rel.select_semantic(predicate, Bound::Both(subject, *member));
        let matched = cursor.next();
        let matched = match matched {
            Some(edge) => Some(edge),
            None if symmetric => {
                let mut reverse = rel.select_semantic(predicate, Bound::Both(*member, subject));
                reverse.next()
            }
            None => None,
        };
        let Some((s, o, row, actual_predicate)) = matched else {
            return Ok(true);
        };
        fresh |= (delta.lo..delta.hi).contains(&row.index());
        premises.push(Fact {
            subject: rel.interner().resolve(s).clone(),
            predicate: actual_predicate.to_owned(),
            object: rel.interner().resolve(o).clone(),
        });
    }
    emit(subject, &premises, fresh)
}
