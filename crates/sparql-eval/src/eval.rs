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

use std::sync::Arc;

use gmeow_rdf_core::{GraphMatch, RdfDataset};
use gmeow_sparql_algebra::{GraphPattern, Query};

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
    /// The evaluation-time value of NOW() — an xsd:dateTime, captured once at
    /// context construction so all NOW() calls in a query return the same instant.
    /// On wasm32 this is always the Unix epoch (1970-01-01T00:00:00Z) because
    /// `std::time::SystemTime` is not available there without a WASI environment.
    pub now: gmeow_xsd::XsdValue,
    /// Splitmix64 PRNG state for RAND()/UUID()/STRUUID().
    /// Seeded from the current time on native targets; fixed to 0 on wasm32.
    pub rng_state: u64,
}

impl<'d> EvalCtx<'d> {
    /// A fresh context over `dataset`, scoped to the default graph.
    pub fn new(dataset: &'d RdfDataset) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let now_val = {
            use std::time::SystemTime;
            let secs = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            gmeow_xsd::XsdValue::DateTime(gmeow_xsd::datetime_from_unix_seconds(secs))
        };
        #[cfg(target_arch = "wasm32")]
        let now_val = gmeow_xsd::XsdValue::DateTime(gmeow_xsd::datetime_epoch());

        #[cfg(not(target_arch = "wasm32"))]
        let rng_seed = {
            use std::time::SystemTime;
            let d = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default();
            d.as_secs() ^ (u64::from(d.subsec_nanos()))
        };
        #[cfg(target_arch = "wasm32")]
        let rng_seed: u64 = 0;

        Self {
            dataset,
            scratch: ScratchInterner::new(),
            active_graph: GraphMatch::Default,
            bnode_counter: 0,
            now: now_val,
            rng_state: rng_seed,
        }
    }
}

/// Evaluate a graph pattern to a multiset of solutions.
///
/// Implemented incrementally over the S6 build tasks; an unimplemented variant
/// returns [`EvalError::Unsupported`] naming the construct. Property paths are
/// evaluated in-engine (S8 #914, the `path` module); the remaining out-of-scope
/// nodes (`Service`, `Lateral`) stay permanent hard errors (SERVICE is S6b #928).
pub fn eval(pattern: &GraphPattern, ctx: &mut EvalCtx<'_>) -> Result<SolutionSeq, EvalError> {
    match pattern {
        GraphPattern::Bgp { patterns } => crate::bgp::eval_bgp(patterns, ctx),
        GraphPattern::Path {
            subject,
            path,
            object,
        } => crate::path::eval_path(subject, path, object, ctx),
        GraphPattern::Join { left, right } => crate::binop::eval_join(left, right, ctx),
        GraphPattern::Union { left, right } => crate::binop::eval_union(left, right, ctx),
        GraphPattern::LeftJoin {
            left,
            right,
            expression,
        } => crate::binop::eval_left_join(left, right, expression, ctx),
        GraphPattern::Minus { left, right } => crate::binop::eval_minus(left, right, ctx),
        GraphPattern::Filter { expr, inner } => crate::expr::eval_filter(expr, inner, ctx),
        GraphPattern::Extend {
            inner,
            variable,
            expression,
        } => crate::expr::eval_extend(inner, variable, expression, ctx),
        GraphPattern::Values {
            variables,
            bindings,
        } => crate::modifier::eval_values(variables, bindings, ctx),
        GraphPattern::Project { inner, variables } => {
            crate::modifier::eval_project(inner, variables, ctx)
        }
        GraphPattern::Distinct { inner } => crate::modifier::eval_distinct(inner, ctx),
        GraphPattern::Reduced { inner } => crate::modifier::eval_reduced(inner, ctx),
        GraphPattern::Slice {
            inner,
            start,
            length,
        } => crate::modifier::eval_slice(inner, *start, *length, ctx),
        GraphPattern::OrderBy { inner, expression } => {
            crate::modifier::eval_order_by(inner, expression, ctx)
        }
        GraphPattern::Graph { name, inner } => crate::modifier::eval_graph(name, inner, ctx),
        GraphPattern::Group {
            inner,
            variables,
            aggregates,
        } => crate::modifier::eval_group(inner, variables, aggregates, ctx),
        // Implemented incrementally over the remaining S6 build tasks; until then
        // (and permanently, for out-of-scope nodes) a hard error names the construct.
        other => Err(EvalError::Unsupported(format!(
            "graph pattern `{}` is not yet implemented in sparql-eval",
            pattern_kind(other)
        ))),
    }
}

/// The result of evaluating a top-level query form — the internal counterpart of
/// the `SparqlResult` egress model (materialized by the engine, S6 Task 9).
#[derive(Debug)]
pub enum Outcome {
    /// `SELECT` solutions (a multiset over the projected schema).
    Solutions(SolutionSeq),
    /// `CONSTRUCT`/`DESCRIBE` graph result.
    Graph(Arc<RdfDataset>),
    /// `ASK` boolean.
    Boolean(bool),
}

/// Evaluate a top-level [`Query`] form over `ctx`'s dataset.
///
/// `SELECT`/`ASK` walk the modifier-wrapped pattern; `CONSTRUCT` emits the IR
/// dataset directly. `DESCRIBE` is out of S6 scope (a hard error).
pub fn evaluate_query(query: &Query, ctx: &mut EvalCtx<'_>) -> Result<Outcome, EvalError> {
    match query {
        Query::Select { pattern, .. } => Ok(Outcome::Solutions(eval(pattern, ctx)?)),
        Query::Ask { pattern, .. } => Ok(Outcome::Boolean(!eval(pattern, ctx)?.is_empty())),
        Query::Construct {
            template, pattern, ..
        } => Ok(Outcome::Graph(crate::construct::eval_construct(
            template, pattern, ctx,
        )?)),
        Query::Describe { .. } => Err(EvalError::unsupported(
            "DESCRIBE query form (out of S6 scope)",
        )),
    }
}

/// A short, stable name for a [`GraphPattern`] variant, for diagnostics.
pub(crate) fn pattern_kind(pattern: &GraphPattern) -> &'static str {
    match pattern {
        GraphPattern::Bgp { .. } => "BGP",
        GraphPattern::Path { .. } => "property path",
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
    fn empty_bgp_is_the_unit_sequence() {
        let ds = RdfDatasetBuilder::new().freeze().expect("freeze empty");
        let mut ctx = EvalCtx::new(&ds);
        let seq = eval(&GraphPattern::Bgp { patterns: vec![] }, &mut ctx).expect("empty BGP");
        // The identity table Z: exactly one solution that binds nothing.
        assert_eq!(seq.len(), 1);
        assert!(seq.schema.is_empty());
    }

    #[test]
    fn unimplemented_variant_hard_errors_with_its_name() {
        let ds = RdfDatasetBuilder::new().freeze().expect("freeze empty");
        let mut ctx = EvalCtx::new(&ds);
        let inner = Box::new(GraphPattern::Bgp { patterns: vec![] });
        // SERVICE (federation) is permanently out of S6 scope (→ S6b #928).
        let pattern = GraphPattern::Service {
            name: gmeow_sparql_algebra::NamedNodePattern::NamedNode(
                gmeow_sparql_algebra::NamedNode::new_unchecked("http://ex/endpoint"),
            ),
            inner,
            silent: false,
        };
        let err = eval(&pattern, &mut ctx).unwrap_err();
        assert!(matches!(err, EvalError::Unsupported(_)));
        assert!(err.to_string().contains("SERVICE"));
    }
}
