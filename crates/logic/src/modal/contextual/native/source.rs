// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! A borrowed view of the native ingress columns. Only term lookup is indexed;
//! statements stay in their existing source owner and are never copied or frozen.

use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;
use std::sync::Arc;

use crate::reason::refute::RefutationPremise;
use purrdf::{DatasetView, GraphMatch, QuadIds, RdfStoreCapabilities, TermId, TermRef, TermValue};

const REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";

pub(super) struct SourceView<'a> {
    sources: &'a BTreeMap<String, Arc<[RefutationPremise]>>,
    values: Vec<TermValue>,
    ids: BTreeMap<TermValue, TermId>,
    reifiers: BTreeSet<(TermId, Option<TermId>)>,
}

impl<'a> SourceView<'a> {
    pub(super) fn new(
        sources: &'a BTreeMap<String, Arc<[RefutationPremise]>>,
    ) -> gmeow_errors::Result<Self> {
        let mut view = Self {
            sources,
            values: Vec::new(),
            ids: BTreeMap::new(),
            reifiers: BTreeSet::new(),
        };
        for source in sources.values().flat_map(|sources| sources.iter()) {
            view.intern(&source.subject)?;
            view.intern(&TermValue::iri(&source.predicate))?;
            view.intern(&source.object)?;
            if let Some(graph) = &source.graph {
                view.intern(graph)?;
            }
        }
        view.reifiers = sources
            .values()
            .flat_map(|sources| sources.iter())
            .filter(|source| source.predicate == REIFIES)
            .map(|source| {
                (
                    view.id(&source.subject),
                    source.graph.as_ref().map(|graph| view.id(graph)),
                )
            })
            .collect();
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

    fn row(&self, source: &RefutationPremise) -> QuadIds {
        QuadIds {
            s: self.id(&source.subject),
            p: self.id(&TermValue::iri(&source.predicate)),
            o: self.id(&source.object),
            g: source.graph.as_ref().map(|graph| self.id(graph)),
        }
    }

    fn is_annotation(&self, source: &RefutationPremise) -> bool {
        source.predicate != REIFIES
            && self.reifiers.contains(&(
                self.id(&source.subject),
                source.graph.as_ref().map(|graph| self.id(graph)),
            ))
    }
}

impl DatasetView for SourceView<'_> {
    type Id = TermId;
    // Every term and row is resident in this view; a read cannot fail.
    type ReadError = Infallible;
    type TermGuard<'a>
        = TermRef<'a>
    where
        Self: 'a;
    type ProbePlan = ();

    fn quads(&self) -> impl Iterator<Item = QuadIds> + '_ {
        self.sources
            .values()
            .flat_map(|sources| sources.iter())
            .filter(|source| source.predicate != REIFIES && !self.is_annotation(source))
            .map(|source| self.row(source))
    }
    fn resolve(&self, id: TermId) -> Result<TermRef<'_>, Infallible> {
        Ok(match &self.values[id.index()] {
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
        })
    }
    fn term_id_by_value(&self, value: &TermValue) -> Result<Option<TermId>, Infallible> {
        Ok(self.ids.get(value).copied())
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
    fn term_count(&self) -> u64 {
        self.values.len() as u64
    }
    fn reifier_quads(&self) -> impl Iterator<Item = QuadIds> + '_ {
        self.sources
            .values()
            .flat_map(|sources| sources.iter())
            .filter(|source| source.predicate == REIFIES)
            .map(|source| self.row(source))
    }
    fn annotation_quads(&self) -> impl Iterator<Item = QuadIds> + '_ {
        self.sources
            .values()
            .flat_map(|sources| sources.iter())
            .filter(|source| self.is_annotation(source))
            .map(|source| self.row(source))
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
        self.sources
            .values()
            .flat_map(|sources| sources.iter())
            .filter_map(|source| source.graph.as_ref().map(|graph| self.id(graph)))
            .collect::<BTreeSet<_>>()
            .into_iter()
    }
}
