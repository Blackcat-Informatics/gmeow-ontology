// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The graph-pattern evaluation recursion and its [`EvalCtx`].
//!
//! [`eval`] maps a [`GraphPattern`] to a [`SolutionSeq`] over the dataset in
//! [`EvalCtx`]. The recursion is filled in across the S6 build tasks (#912); each
//! not-yet-implemented variant hard-errors ([`EvalError::Unsupported`]) rather than
//! returning a partial bag (the `no-optionality` doctrine).
//!
//! Evaluation pins the **concrete** [`RdfDataset`] rather than a generic
//! `DatasetView`: the value→id bridge [`RdfDataset::term_id_by_value`] (P4 #838),
//! which BGP constant-resolution needs, is an inherent method on the frozen dataset
//! and is not part of the `DatasetView` trait. The dataset still exposes its
//! indexed read surface through `DatasetView` (the inherent `quads_for_pattern`
//! override, P4b #891).

use gmeow_rdf_core::{GraphMatch, RdfDataset};
use gmeow_sparql_algebra::GraphPattern;

use crate::error::EvalError;
use crate::scratch::ScratchInterner;
use crate::solution::SolutionSeq;

/// The mutable evaluation context threaded through [`eval`].
pub struct EvalCtx<'d> {
    /// The frozen dataset being queried (the concrete IR — see the module docs for
    /// why this is not a generic `DatasetView`).
    pub dataset: &'d RdfDataset,
    /// The per-query interner for terms computed during evaluation (BIND, VALUES,
    /// aggregate output, arithmetic/string-function results).
    pub scratch: ScratchInterner,
    /// The graph currently in scope (set by `GRAPH`; the default graph at the root).
    pub active_graph: GraphMatch,
    /// A monotonic counter for minting fresh blank nodes (`BNODE()` and CONSTRUCT
    /// template blanks).
    pub bnode_counter: u64,
}

impl<'d> EvalCtx<'d> {
    /// A fresh context over `dataset`, scoped to the default graph.
    pub fn new(dataset: &'d RdfDataset) -> Self {
        Self {
            dataset,
            scratch: ScratchInterner::new(),
            active_graph: GraphMatch::Default,
            bnode_counter: 0,
        }
    }
}

/// Evaluate a graph pattern to a multiset of solutions.
///
/// Implemented incrementally over the S6 build tasks; an unimplemented variant
/// returns [`EvalError::Unsupported`] naming the construct. Out-of-S6-scope nodes
/// (`Path`, `Service`, `Lateral`) remain permanent hard errors (property paths are
/// S8 #914; SERVICE is S6b #928).
pub fn eval(pattern: &GraphPattern, _ctx: &mut EvalCtx<'_>) -> Result<SolutionSeq, EvalError> {
    Err(EvalError::Unsupported(format!(
        "graph pattern `{}` is not yet implemented in sparql-eval",
        pattern_kind(pattern)
    )))
}

/// A short, stable name for a [`GraphPattern`] variant, for diagnostics.
pub(crate) fn pattern_kind(pattern: &GraphPattern) -> &'static str {
    match pattern {
        GraphPattern::Bgp { .. } => "BGP",
        GraphPattern::Path { .. } => "property path (S8 #914)",
        GraphPattern::Join { .. } => "Join",
        GraphPattern::LeftJoin { .. } => "OPTIONAL (LeftJoin)",
        GraphPattern::Lateral { .. } => "LATERAL",
        GraphPattern::Filter { .. } => "FILTER",
        GraphPattern::Union { .. } => "UNION",
        GraphPattern::Graph { .. } => "GRAPH",
        GraphPattern::Extend { .. } => "BIND (Extend)",
        GraphPattern::Minus { .. } => "MINUS",
        GraphPattern::Service { .. } => "SERVICE (S6b #928)",
        GraphPattern::Values { .. } => "VALUES",
        GraphPattern::OrderBy { .. } => "ORDER BY",
        GraphPattern::Project { .. } => "Project",
        GraphPattern::Distinct { .. } => "DISTINCT",
        GraphPattern::Reduced { .. } => "REDUCED",
        GraphPattern::Slice { .. } => "LIMIT/OFFSET (Slice)",
        GraphPattern::Group { .. } => "GROUP BY",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_rdf_core::RdfDatasetBuilder;

    #[test]
    fn stub_hard_errors_with_the_variant_name() {
        let ds = RdfDatasetBuilder::new().freeze().expect("freeze empty");
        let mut ctx = EvalCtx::new(&ds);
        let pattern = GraphPattern::Bgp { patterns: vec![] };
        let err = eval(&pattern, &mut ctx).unwrap_err();
        assert!(matches!(err, EvalError::Unsupported(_)));
        assert!(err.to_string().contains("BGP"));
    }
}
