// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The static, allocation-free **read view** over an RDF dataset (purrdf P2,
//! #836). See [`docs/design/purrdf-backend-contract.md`](../../../docs/design/purrdf-backend-contract.md).
//!
//! [`DatasetView`] generalizes the owned-quad [`RdfStore`](crate::RdfStore) into an
//! id-based, borrowed read interface: it yields `Copy` [`QuadIds`] and borrowed
//! [`QuadRef`]s (no per-quad allocation, no term-string clones), and offers
//! [`DatasetView::quads_for_pattern`] keyed on dataset-local [`TermId`]s plus a
//! [`GraphMatch`]. The default `quads_for_pattern` is a linear scan; backends with
//! access-pattern indexes (P4, #838) override it.
//!
//! This is the **static** trait layer (generic `impl DatasetView`, RPITIT — not
//! object-safe). Per the backend contract (C1), backend selection is compile-time
//! and single, so the erased `&mut dyn` layer is deferred; this trait carries no
//! object-safety obligation. `RdfStore` survives alongside `DatasetView` until the
//! consumer migration (P2c) retires it.

use crate::ir::{QuadIds, QuadRef, RdfDataset, TermId, TermRef};
use crate::RdfStoreCapabilities;

/// How a pattern query matches the graph slot of a quad.
///
/// Storage keeps `g: Option<TermId>` where `None` is the default graph, so
/// `Option<TermId>` alone cannot distinguish *any graph* from *the default graph* —
/// hence this dedicated three-way match. Deliberately exhaustive (NOT
/// `#[non_exhaustive]`): a quad's graph is either the default or exactly one named
/// graph, so the three cases are closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphMatch {
    /// Match quads in any graph (default or named).
    Any,
    /// Match only quads in the default graph (`g == None`).
    Default,
    /// Match only quads in the named graph identified by this id.
    Named(TermId),
}

impl GraphMatch {
    /// Whether a quad's stored graph slot (`None` = default graph) matches.
    #[inline]
    #[must_use]
    pub fn matches(self, graph: Option<TermId>) -> bool {
        match self {
            GraphMatch::Any => true,
            GraphMatch::Default => graph.is_none(),
            GraphMatch::Named(id) => graph == Some(id),
        }
    }
}

/// A static, allocation-free read view over an RDF dataset (purrdf backend
/// contract, C2/C3/C6). All methods are infallible for a frozen, validated dataset.
pub trait DatasetView {
    /// Iterate every quad as `Copy` [`QuadIds`] (dataset-local term ids).
    fn quads(&self) -> impl Iterator<Item = QuadIds> + '_;

    /// Iterate every quad as a borrowed, resolved [`QuadRef`] (no allocation).
    fn quad_refs(&self) -> impl Iterator<Item = QuadRef<'_>> + '_;

    /// Resolve a dataset-local [`TermId`] to its borrowed [`TermRef`].
    fn resolve(&self, id: TermId) -> TermRef<'_>;

    /// Quads matching an optional `(s, p, o)` id pattern and a [`GraphMatch`].
    ///
    /// The default is an id-equality linear scan (no string resolution); backends
    /// with access-pattern indexes (P4, #838) override this with an indexed lookup.
    /// Callers resolve term *values* to ids first (`term_id_by_value`, P4).
    fn quads_for_pattern(
        &self,
        s: Option<TermId>,
        p: Option<TermId>,
        o: Option<TermId>,
        g: GraphMatch,
    ) -> impl Iterator<Item = QuadIds> + '_ {
        self.quads().filter(move |q| {
            // Closure params named `id` (not s/p/o) to avoid shadowing the outer
            // `Option<TermId>` filters with the unwrapped `TermId`.
            s.is_none_or(|id| q.s == id)
                && p.is_none_or(|id| q.p == id)
                && o.is_none_or(|id| q.o == id)
                && g.matches(q.g)
        })
    }

    /// The capabilities this view's backing data exposes (C7).
    fn capabilities(&self) -> RdfStoreCapabilities;

    /// A size hint for the number of quads, if known.
    fn len_hint(&self) -> Option<usize> {
        None
    }
}

/// The production read view: the immutable value-interned [`RdfDataset`] (#819 C1).
impl DatasetView for RdfDataset {
    #[inline]
    fn quads(&self) -> impl Iterator<Item = QuadIds> + '_ {
        // Inherent methods take method-resolution priority over trait methods, so
        // these delegate to `RdfDataset`'s own impls (no recursion).
        RdfDataset::quads(self)
    }

    #[inline]
    fn quad_refs(&self) -> impl Iterator<Item = QuadRef<'_>> + '_ {
        RdfDataset::quad_refs(self)
    }

    #[inline]
    fn resolve(&self, id: TermId) -> TermRef<'_> {
        RdfDataset::resolve(self, id)
    }

    #[inline]
    fn capabilities(&self) -> RdfStoreCapabilities {
        RdfDataset::capabilities(self)
    }

    #[inline]
    fn len_hint(&self) -> Option<usize> {
        Some(RdfDataset::quad_count(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::RdfDatasetBuilder;

    fn iri(b: &mut RdfDatasetBuilder, n: &str) -> TermId {
        b.intern_iri(format!("http://example.org/{n}"))
    }

    #[test]
    fn graph_match_three_way() {
        let mut b = RdfDatasetBuilder::new();
        let g = iri(&mut b, "g");
        assert!(GraphMatch::Any.matches(None) && GraphMatch::Any.matches(Some(g)));
        assert!(GraphMatch::Default.matches(None) && !GraphMatch::Default.matches(Some(g)));
        assert!(GraphMatch::Named(g).matches(Some(g)) && !GraphMatch::Named(g).matches(None));
    }

    #[test]
    fn quads_for_pattern_filters_by_id_and_graph() {
        let mut b = RdfDatasetBuilder::new();
        let s = iri(&mut b, "s");
        let p = iri(&mut b, "p");
        let o1 = iri(&mut b, "o1");
        let o2 = iri(&mut b, "o2");
        let g = iri(&mut b, "g");
        b.push_quad(s, p, o1, None); // default graph
        b.push_quad(s, p, o2, Some(g)); // named graph g
        let ds = b.freeze().expect("freeze");

        // Whole-dataset (Any matches everything).
        assert_eq!(
            ds.quads_for_pattern(None, None, None, GraphMatch::Any)
                .count(),
            2
        );
        assert_eq!(ds.len_hint(), Some(2));
        // Object filter.
        assert_eq!(
            ds.quads_for_pattern(None, None, Some(o1), GraphMatch::Any)
                .count(),
            1
        );
        // Default graph only.
        assert_eq!(
            ds.quads_for_pattern(None, None, None, GraphMatch::Default)
                .count(),
            1
        );
        // Named graph only.
        assert_eq!(
            ds.quads_for_pattern(None, None, None, GraphMatch::Named(g))
                .count(),
            1
        );
        // s+p match both quads.
        assert_eq!(
            ds.quads_for_pattern(Some(s), Some(p), None, GraphMatch::Any)
                .count(),
            2
        );
        // A non-matching subject yields nothing.
        assert_eq!(
            ds.quads_for_pattern(Some(o1), None, None, GraphMatch::Any)
                .count(),
            0
        );
        // The trait read view agrees with the inherent iterators.
        assert_eq!(DatasetView::quads(&*ds).count(), 2);
        assert_eq!(DatasetView::quad_refs(&*ds).count(), 2);
    }
}
