// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The gts-`Graph` arena read shape, materialized over the native carrier
//! [`RdfDataset`].
//!
//! The export leaf `yaml_ld` was written against the gts
//! in-memory `Graph` model: a flat term arena indexed by `usize` term id, `(s, p, o,
//! g)` quad tuples, and the RDF 1.2 reifier / annotation side-tables. The carrier is
//! the native [`RdfDataset`]; this ONE adapter materializes that shape from it so the
//! deterministic projection bodies stay unchanged — rather than the leaf carrying a
//! near-identical private shim. GTS is exit-only; the leaf reads the carrier here.
//!
//! Blobs are deliberately absent: the carrier holds RDF only (blob payloads live in
//! the gts archive by reference, never in the in-memory transport), so a leaf that
//! projects a blob table reads an empty set off the carrier.

use purrdf::{RdfDataset, TermId, TermRef};

/// The kind of an RDF term (mirror of the former `purrdf::gts::model::TermKind`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum TermKind {
    #[default]
    Iri,
    Bnode,
    Literal,
    Triple,
}

/// One materialized RDF term (mirror of the former `purrdf::gts::model::Term`).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub(crate) struct Term {
    pub(crate) kind: TermKind,
    /// IRI string / blank-node label / literal lexical form (`None` for triple terms).
    pub(crate) value: Option<String>,
    pub(crate) lang: Option<String>,
    pub(crate) direction: Option<String>,
    /// Datatype term id for literals (resolved to its IRI by [`Graph::datatype_iri`]).
    pub(crate) datatype: Option<usize>,
    /// The reifier term id binding this (triple) term, when one annotates it — the
    /// gts arena's `reifier` column, reconstructed from the dataset's reifier table.
    pub(crate) reifier: Option<usize>,
    /// The component term ids `(s, p, o)` of a quoted-triple term.
    pub(crate) triple: Option<(usize, usize, usize)>,
}

/// The folded graph view (mirror of the former `purrdf::gts::model::Graph` read API).
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub(crate) struct Graph {
    pub(crate) terms: Vec<Term>,
    pub(crate) quads: Vec<(usize, usize, usize, Option<usize>)>,
    pub(crate) reifiers: Vec<(usize, (usize, usize, usize))>,
    pub(crate) annotations: Vec<(usize, usize, usize)>,
}

impl Graph {
    /// Materialize the gts-`Graph` arena shape from the native carrier dataset.
    pub(crate) fn from_dataset(ds: &RdfDataset) -> Self {
        // The reifier side-table as `(reifier_tid, (s, p, o))`, and the reverse map
        // `reified-triple-term -> reifier` used to reconstruct each triple term's
        // arena `reifier` column.
        let mut reifiers: Vec<(usize, (usize, usize, usize))> = Vec::new();
        let mut reifier_of_triple: std::collections::BTreeMap<usize, usize> =
            std::collections::BTreeMap::new();
        for (rid, triple) in ds.reifiers() {
            if let TermRef::Triple { s, p, o } = ds.resolve(triple) {
                reifiers.push((rid.index(), (s.index(), p.index(), o.index())));
                reifier_of_triple
                    .entry(triple.index())
                    .or_insert(rid.index());
            }
        }

        let terms: Vec<Term> = (0..ds.term_count())
            .map(|tid| {
                let mut term = Term::from_ref(ds.resolve(TermId::from_index(tid as u32)));
                if term.kind == TermKind::Triple {
                    term.reifier = reifier_of_triple.get(&tid).copied();
                }
                term
            })
            .collect();
        let quads = ds
            .quads()
            .map(|q| {
                (
                    q.s.index(),
                    q.p.index(),
                    q.o.index(),
                    q.g.map(|g| g.index()),
                )
            })
            .collect();
        let annotations = ds
            .annotations()
            .map(|(r, p, v)| (r.index(), p.index(), v.index()))
            .collect();
        Graph {
            terms,
            quads,
            reifiers,
            annotations,
        }
    }
}

impl Term {
    fn from_ref(tref: TermRef<'_>) -> Self {
        match tref {
            TermRef::Iri(s) => Term {
                kind: TermKind::Iri,
                value: Some(s.to_string()),
                lang: None,
                direction: None,
                datatype: None,
                reifier: None,
                triple: None,
            },
            TermRef::Blank { label, .. } => Term {
                kind: TermKind::Bnode,
                value: Some(label.to_string()),
                lang: None,
                direction: None,
                datatype: None,
                reifier: None,
                triple: None,
            },
            TermRef::Literal {
                lexical,
                datatype,
                language,
                direction,
            } => Term {
                kind: TermKind::Literal,
                value: Some(lexical.to_string()),
                lang: language.map(str::to_string),
                direction: direction.map(|d| d.as_str().to_string()),
                datatype: Some(datatype.index()),
                reifier: None,
                triple: None,
            },
            TermRef::Triple { s, p, o } => Term {
                kind: TermKind::Triple,
                value: None,
                lang: None,
                direction: None,
                datatype: None,
                reifier: None,
                triple: Some((s.index(), p.index(), o.index())),
            },
        }
    }
}
