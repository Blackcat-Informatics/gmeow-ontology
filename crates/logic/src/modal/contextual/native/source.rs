// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! A compact native view over ingress premises. It interns each distinct term
//! once, retains only ID rows thereafter, and builds a predicate index without
//! constructing or freezing a second RDF dataset.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::reason::refute::RefutationPremise;
use foldhash::fast::FixedState;
use hashbrown::HashMap;
use purrdf::{
    DatasetView, GraphMatch, QuadIds, QuadRef, RdfStoreCapabilities, TermId, TermRef, TermValue,
};

const REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";

enum QuadCandidates<'a> {
    All(std::slice::Iter<'a, QuadIds>),
    Predicate {
        rows: &'a [QuadIds],
        indices: std::slice::Iter<'a, usize>,
    },
    Empty,
}

impl Iterator for QuadCandidates<'_> {
    type Item = QuadIds;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::All(rows) => rows.next().copied(),
            Self::Predicate { rows, indices } => indices.next().map(|index| rows[*index]),
            Self::Empty => None,
        }
    }
}

pub(super) struct SourceView {
    values: Vec<TermValue>,
    ids: HashMap<TermValue, TermId, FixedState>,
    rows: Vec<QuadIds>,
    rows_by_predicate: HashMap<TermId, Vec<usize>, FixedState>,
    reifier_rows: Vec<QuadIds>,
    annotation_rows: Vec<QuadIds>,
    named_graphs: Vec<TermId>,
}

impl SourceView {
    pub(super) fn new(
        sources: &BTreeMap<String, Arc<[RefutationPremise]>>,
    ) -> gmeow_errors::Result<Self> {
        let mut view = Self {
            values: Vec::new(),
            ids: HashMap::default(),
            rows: Vec::new(),
            rows_by_predicate: HashMap::default(),
            reifier_rows: Vec::new(),
            annotation_rows: Vec::new(),
            named_graphs: Vec::new(),
        };
        let mut rows = Vec::new();
        let mut reifiers = BTreeSet::new();
        let mut named_graphs = BTreeSet::new();
        for source in sources.values().flat_map(|sources| sources.iter()) {
            let s = view.intern(&source.subject)?;
            let p = view.intern(&TermValue::iri(&source.predicate))?;
            let o = view.intern(&source.object)?;
            let g = source
                .graph
                .as_ref()
                .map(|graph| view.intern(graph))
                .transpose()?;
            if let Some(graph) = g {
                named_graphs.insert(graph);
            }
            if source.predicate == REIFIES {
                reifiers.insert((s, g));
            }
            rows.push((QuadIds { s, p, o, g }, source.predicate == REIFIES));
        }
        view.named_graphs = named_graphs.into_iter().collect();
        for (row, is_reifier) in rows {
            if is_reifier {
                view.reifier_rows.push(row);
            } else if reifiers.contains(&(row.s, row.g)) {
                view.annotation_rows.push(row);
            } else {
                let index = view.rows.len();
                view.rows_by_predicate.entry(row.p).or_default().push(index);
                view.rows.push(row);
            }
        }
        Ok(view)
    }

    fn intern(&mut self, value: &TermValue) -> gmeow_errors::Result<TermId> {
        if let Some(id) = self.ids.get(value) {
            return Ok(*id);
        }
        match value {
            TermValue::Literal { datatype, .. } => {
                self.intern(&TermValue::iri(datatype))?;
            }
            TermValue::Triple { s, p, o } => {
                self.intern(s)?;
                self.intern(p)?;
                self.intern(o)?;
            }
            TermValue::Iri(_) | TermValue::Blank { .. } => {}
        }
        let index = u32::try_from(self.values.len())
            .ok()
            .filter(|index| *index < u32::MAX)
            .ok_or_else(|| {
                super::super::diagnostic(super::super::malformed(
                    "contextual source term inventory exceeds native IDs",
                ))
            })?;
        let id = TermId::from_index(index);
        self.ids.insert(value.clone(), id);
        self.values.push(value.clone());
        Ok(id)
    }

    fn id(&self, value: &TermValue) -> TermId {
        self.ids[value]
    }

    fn candidates(&self, predicate: Option<TermId>) -> QuadCandidates<'_> {
        match predicate {
            None => QuadCandidates::All(self.rows.iter()),
            Some(predicate) => {
                self.rows_by_predicate
                    .get(&predicate)
                    .map_or(QuadCandidates::Empty, |indices| QuadCandidates::Predicate {
                        rows: &self.rows,
                        indices: indices.iter(),
                    })
            }
        }
    }
}

impl DatasetView for SourceView {
    type Id = TermId;
    type ProbePlan = ();

    fn quads(&self) -> impl Iterator<Item = QuadIds> + '_ {
        self.rows.iter().copied()
    }
    fn quad_refs(&self) -> impl Iterator<Item = QuadRef<'_>> + '_ {
        self.quads().map(|quad| QuadRef {
            s: self.resolve(quad.s),
            p: self.resolve(quad.p),
            o: self.resolve(quad.o),
            g: quad.g.map(|graph| self.resolve(graph)),
        })
    }
    fn resolve(&self, id: TermId) -> TermRef<'_> {
        match &self.values[id.index()] {
            TermValue::Iri(iri) => TermRef::Iri(iri),
            TermValue::Blank { label, scope } => TermRef::Blank {
                label,
                scope: *scope,
            },
            TermValue::Literal {
                lexical_form,
                datatype,
                language,
                direction,
            } => TermRef::Literal {
                lexical: lexical_form,
                datatype: self.id(&TermValue::iri(datatype)),
                language: language.as_deref(),
                direction: *direction,
            },
            TermValue::Triple { s, p, o } => TermRef::Triple {
                s: self.id(s),
                p: self.id(p),
                o: self.id(o),
            },
        }
    }
    fn term_id_by_value(&self, value: &TermValue) -> Option<TermId> {
        self.ids.get(value).copied()
    }
    fn capabilities(&self) -> RdfStoreCapabilities {
        RdfStoreCapabilities {
            named_graphs: true,
            quoted_triples: true,
            reifiers: true,
            annotations: true,
            ..RdfStoreCapabilities::plain_rdf()
        }
    }
    fn probe_plan(&self, _: bool, _: bool, _: bool, _: GraphMatch) -> Self::ProbePlan {}
    fn quads_for_pattern(
        &self,
        s: Option<TermId>,
        p: Option<TermId>,
        o: Option<TermId>,
        g: GraphMatch,
    ) -> impl Iterator<Item = QuadIds> + '_ {
        self.candidates(p).filter(move |row| {
            s.is_none_or(|subject| row.s == subject)
                && o.is_none_or(|object| row.o == object)
                && g.matches(row.g)
        })
    }
    fn quads_for_pattern_with_plan(
        &self,
        _: &Self::ProbePlan,
        s: Option<TermId>,
        p: Option<TermId>,
        o: Option<TermId>,
        g: GraphMatch,
    ) -> impl Iterator<Item = QuadIds> + '_ {
        self.quads_for_pattern(s, p, o, g)
    }
    fn term_count(&self) -> usize {
        self.values.len()
    }
    fn reifier_quads(&self) -> impl Iterator<Item = QuadIds> + '_ {
        self.reifier_rows.iter().copied()
    }
    fn annotation_quads(&self) -> impl Iterator<Item = QuadIds> + '_ {
        self.annotation_rows.iter().copied()
    }
    fn annotations_of_with_graph(
        &self,
        reifier: TermId,
    ) -> impl Iterator<Item = (TermId, TermId, Option<TermId>)> + '_ {
        self.annotation_quads()
            .filter(move |row| row.s == reifier)
            .map(|row| (row.p, row.o, row.g))
    }
    fn named_graphs(&self) -> impl Iterator<Item = TermId> + '_ {
        self.named_graphs.iter().copied()
    }
}

#[cfg(test)]
#[path = "source.tests.rs"]
mod tests;
