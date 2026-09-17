// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Indexed native semantic probes. A probe owns at most four arrangement cursors,
//! never copied facts or an alias-expanded relation. Returned premises retain the
//! actual source predicate and terms, including source-bound variable values.

use purrdf::TermValue;

use super::{Bound, RelationStore};
use crate::facts::TermId;
use crate::physical::cursor::{LendingIterator, RowCursor};
use crate::physical::id::RowId;
use crate::rule_ir::{EvalAtom, EvalTerm, Fact, Solution};

/// A shallow borrowed term probe: constants and bindings need no lowering or text.
#[derive(Clone, Copy)]
enum TermProbe<'a> {
    Iri(&'a str),
    Value(&'a TermValue),
}

impl<'a> TermProbe<'a> {
    fn ground(term: &'a EvalTerm, solution: &'a Solution) -> Option<Self> {
        match term {
            EvalTerm::Var(name) => solution.get(name).map(Self::Value),
            EvalTerm::ConstNamed(iri) => Some(Self::Iri(iri)),
            EvalTerm::ConstLit(value) => Some(Self::Value(value)),
        }
    }

    fn id(self, store: &RelationStore) -> Option<TermId> {
        match self {
            Self::Iri(iri) => store.iri_id(iri),
            Self::Value(value) => store.term_id(value),
        }
    }

    fn iri(self) -> Option<&'a str> {
        match self {
            Self::Iri(iri) => Some(iri),
            Self::Value(TermValue::Iri(iri)) => Some(iri),
            Self::Value(_) => None,
        }
    }
}

pub(crate) struct SemanticRows<'a> {
    cursors: [Option<(&'a str, RowCursor<'a>)>; 4],
    current: usize,
}

impl<'a> SemanticRows<'a> {
    fn empty() -> Self {
        Self {
            cursors: std::array::from_fn(|_| None),
            current: 0,
        }
    }

    pub(crate) fn next(&mut self) -> Option<(TermId, TermId, RowId, &'a str)> {
        while let Some(slot) = self.cursors.get_mut(self.current) {
            if let Some((predicate, cursor)) = slot
                && let Some((subject, object, row)) = cursor.next()
            {
                return Some((subject, object, row, *predicate));
            }
            self.current += 1;
        }
        None
    }

    pub(crate) fn any_remaining(mut self) -> bool {
        self.next().is_some()
    }
}

impl RelationStore {
    /// Query an operator using its declared spellings, with exact bound terms.
    pub(crate) fn select_semantic<'a>(
        &'a self,
        predicate: &'a str,
        bound: Bound,
    ) -> SemanticRows<'a> {
        let mut rows = SemanticRows::empty();
        rows.cursors[0] = Some((predicate, self.select(predicate, bound)));
        if let Some(alternate) = self.semantics.alternate_predicate(predicate) {
            rows.cursors[1] = Some((alternate, self.select(alternate, bound)));
        }
        rows
    }

    /// A term-bound query distinguishes a constant marker from a bound variable.
    /// The latter is exact even when its value names a grounding vocabulary term.
    pub(crate) fn select_pattern<'a>(
        &'a self,
        predicate: &'a str,
        subject: Option<&TermValue>,
        object: Option<&TermValue>,
        constant_object: bool,
        constant_predicate: bool,
    ) -> SemanticRows<'a> {
        self.select_probes(
            predicate,
            subject.map(TermProbe::Value),
            object.map(TermProbe::Value),
            constant_object,
            constant_predicate,
        )
    }

    fn select_probes<'a>(
        &'a self,
        predicate: &'a str,
        subject: Option<TermProbe<'_>>,
        object: Option<TermProbe<'_>>,
        constant_object: bool,
        constant_predicate: bool,
    ) -> SemanticRows<'a> {
        let mut rows = SemanticRows::empty();
        let subject_id = match subject {
            Some(subject) => match subject.id(self) {
                Some(id) => Some(id),
                None => return rows,
            },
            None => None,
        };
        let alternate_object = if constant_object {
            object
                .and_then(TermProbe::iri)
                .and_then(|iri| self.semantics.alternate_marker(predicate, iri))
                .and_then(|iri| self.iri_id(iri))
        } else {
            None
        };
        let object_id = object.and_then(|object| object.id(self));
        if object.is_some() && object_id.is_none() && alternate_object.is_none() {
            return rows;
        }
        let alternate_predicate = if constant_predicate {
            self.semantics.alternate_predicate(predicate)
        } else {
            None
        };
        let predicates = [Some(predicate), alternate_predicate];
        let objects = if object.is_some() {
            [object_id, alternate_object]
        } else {
            [None, None]
        };
        let mut index = 0;
        for predicate in predicates.into_iter().flatten() {
            for (position, object_id) in objects.into_iter().enumerate() {
                if (object.is_some() && object_id.is_none()) || (object.is_none() && position > 0) {
                    continue;
                }
                let bound = match (subject_id, object_id) {
                    (Some(s), Some(o)) => Bound::Both(s, o),
                    (Some(s), None) => Bound::Subject(s),
                    (None, Some(o)) => Bound::Object(o),
                    (None, None) => Bound::Any,
                };
                rows.cursors[index] = Some((predicate, self.select(predicate, bound)));
                index += 1;
            }
        }
        rows
    }

    pub(crate) fn select_atom<'a>(
        &'a self,
        atom: &'a EvalAtom,
        solution: &Solution,
    ) -> SemanticRows<'a> {
        self.select_probes(
            &atom.predicate,
            TermProbe::ground(&atom.subject, solution),
            TermProbe::ground(&atom.object, solution),
            matches!(atom.object, EvalTerm::ConstNamed(_)),
            true,
        )
    }

    /// Unify the actual native terms selected by an operator probe. Constant marker
    /// equivalence is role-local; all variable equality remains exact.
    pub(crate) fn match_selected(
        &self,
        atom: &EvalAtom,
        fact: &Fact,
        base: &Solution,
    ) -> Option<Solution> {
        if self.semantics.predicate(&atom.predicate) != self.semantics.predicate(&fact.predicate) {
            return None;
        }
        let mut result = base.clone();
        for (position, (pattern, value)) in
            [(&atom.subject, &fact.subject), (&atom.object, &fact.object)]
                .into_iter()
                .enumerate()
        {
            match pattern {
                EvalTerm::Var(name) => {
                    if let Some(existing) = result.get(name) {
                        if existing != value {
                            return None;
                        }
                    } else {
                        result.bindings.push((name.clone(), value.clone()));
                    }
                }
                EvalTerm::ConstNamed(iri) => {
                    let exact = matches!(value, purrdf::TermValue::Iri(actual) if actual == iri);
                    let marker = position == 1
                        && matches!(value, purrdf::TermValue::Iri(actual)
                        if self.semantics.alternate_marker(&atom.predicate, iri) == Some(actual.as_str()));
                    if !exact && !marker {
                        return None;
                    }
                }
                EvalTerm::ConstLit(expected) => {
                    if expected != value {
                        return None;
                    }
                }
            }
        }
        Some(result)
    }
}
