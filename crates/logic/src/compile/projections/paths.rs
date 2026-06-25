// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Projection back-end for `logic:PathShape` (#1010): a named/parametric predicate
//! path → an **extended SPARQL property path** + a **depth-bounded Datalog** rule
//! scheme.
//!
//! Unlike the seven whole-program projections in [`super`], a path shape is a
//! per-shape concern, so this back-end is invoked over `program.path_shapes`
//! ([`project_path_shapes`]) and is *not* part of the byte-pinned conformance
//! sweep.  Each projection carries the `"property-path"` loss-ledger row.
//!
//! # The two projections
//!
//! * **Property path** ([`lower_to_property_path`]): the canonical shape lowers to
//!   the extended [`PropertyPathExpression`] algebra — a named step uses the
//!   standard operators where it can (`p`, `p+`) and the GMEOW `Range`/`Wildcard`
//!   extensions only when it must.  Serializing it is the SPARQL surface.
//! * **Datalog** ([`datalog_text`]): the bounded `{min,max}` path unrolls to a
//!   stratified, terminating rule scheme (`reach1`, `reach2`, … `reachable`) — no
//!   recursion, exact for bounded depth; unbounded uses the recursive closure.
//!   The rules run on the existing native least-model engine (no new engine) — see
//!   the `nearby_orgs_*` runtime tests.
//!
//! A predicate **wildcard** has no SPARQL §9 operator and the native join engine
//! cannot bind a predicate *variable*, so the wildcard's `edge` relation is
//! materialized by a namespace-scoped pre-pass (iterate the graph, keep `(X,p,Y)`
//! with `p` in the namespace) before the depth closure runs.  That step is declared
//! in the ledger and in the emitted Datalog header, never hidden.

use gmeow_sparql_algebra::{NamedNode as SparqlNamedNode, PropertyPathExpression};

use super::super::ir::{LogicProgram, PathBase, PathShapeIr};
use super::{target_meta, LedgerEntry};

/// Lower a [`PathShapeIr`] to the extended SPARQL property-path algebra.
///
/// The base step becomes a `NamedNode` (named) or `Wildcard` (any-predicate,
/// optionally namespace-scoped).  The depth bound reuses the standard operators
/// losslessly where possible — exactly-one (`{1,1}`) is the bare step and
/// at-least-one (`{1,}`) is `+` — and only falls back to the GMEOW `Range`
/// extension for a genuinely bounded `{min,max}`.
pub fn lower_to_property_path(shape: &PathShapeIr) -> PropertyPathExpression {
    let inner = match &shape.base {
        PathBase::NamedPredicate(p) => {
            PropertyPathExpression::NamedNode(SparqlNamedNode::new_unchecked(p.clone()))
        }
        PathBase::Wildcard => PropertyPathExpression::Wildcard {
            namespace: shape
                .namespace_scope
                .as_ref()
                .map(|ns| SparqlNamedNode::new_unchecked(ns.clone())),
        },
    };
    match (shape.min_depth, shape.max_depth) {
        (1, Some(1)) => inner,
        (1, None) => PropertyPathExpression::OneOrMore(Box::new(inner)),
        (min, max) => PropertyPathExpression::Range {
            inner: Box::new(inner),
            min,
            max,
        },
    }
}

/// The serialized extended SPARQL property path — the property-path projection
/// surface.
pub fn property_path_text(shape: &PathShapeIr) -> String {
    lower_to_property_path(shape).to_string()
}

/// IRI of the `edge` relation the depth closure walks: the named step predicate
/// itself, or (for a wildcard) a minted relation materialized by the pre-pass.
pub fn edge_predicate_iri(shape: &PathShapeIr) -> String {
    match &shape.base {
        PathBase::NamedPredicate(p) => p.clone(),
        PathBase::Wildcard => format!("{}/edge", shape.iri),
    }
}

fn reach_iri(shape: &PathShapeIr, k: u32) -> String {
    format!("{}/reach/{k}", shape.iri)
}

fn reachable_iri(shape: &PathShapeIr) -> String {
    format!("{}/reachable", shape.iri)
}

/// The depth-bounded Datalog rule scheme for a path shape, in the native engine's
/// `<iri>(?S, ?O, ?W)` syntax (directly runnable by `parse_eval_rules`).  The
/// `reachable` relation holds every `(start, node)` pair within the shape's depth
/// bound; the `edge` relation is the named step (or the wildcard pre-pass output).
pub fn datalog_text(shape: &PathShapeIr) -> String {
    let edge = edge_predicate_iri(shape);
    let reachable = reachable_iri(shape);
    let mut lines = vec![format!(
        "% GENERATED — Datalog projection of logic:PathShape <{}>.",
        shape.iri
    )];

    if let PathBase::Wildcard = &shape.base {
        match shape.namespace_scope.as_deref() {
            Some(ns) => lines.push(format!(
                "% <{edge}> is materialized by a namespace-scoped pre-pass: \
                 edge(X,Y) for every (X,p,Y) with p in <{ns}>."
            )),
            None => lines.push(format!(
                "% <{edge}> is materialized by a pre-pass: edge(X,Y) for every \
                 (X,p,Y) (any predicate)."
            )),
        }
    }

    match shape.max_depth {
        Some(max) => {
            lines.push(format!(
                "% Bounded depth {{{},{max}}} — exact, terminating (unrolled, no recursion).",
                shape.min_depth
            ));
            lines.push(format!(
                "<{}>(?X, ?Y, ?W) :- <{edge}>(?X, ?Y, ?W) .",
                reach_iri(shape, 1)
            ));
            for k in 2..=max {
                lines.push(format!(
                    "<{}>(?X, ?Y, ?W) :- <{}>(?X, ?Z, ?W), <{edge}>(?Z, ?Y, ?W) .",
                    reach_iri(shape, k),
                    reach_iri(shape, k - 1)
                ));
            }
            for k in shape.min_depth..=max {
                lines.push(format!(
                    "<{reachable}>(?X, ?Y, ?W) :- <{}>(?X, ?Y, ?W) .",
                    reach_iri(shape, k)
                ));
            }
        }
        None => {
            lines.push(format!(
                "% Unbounded depth (>= {}) — recursive transitive closure.",
                shape.min_depth
            ));
            lines.push(format!(
                "<{reachable}>(?X, ?Y, ?W) :- <{edge}>(?X, ?Y, ?W) ."
            ));
            lines.push(format!(
                "<{reachable}>(?X, ?Y, ?W) :- <{reachable}>(?X, ?Z, ?W), <{edge}>(?Z, ?Y, ?W) ."
            ));
            if shape.min_depth > 1 {
                lines.push(format!(
                    "% NOTE: minDepth {} > 1 with an unbounded path is not separately \
                     lower-bounded in this closure (declared approximation).",
                    shape.min_depth
                ));
            }
        }
    }

    format!("{}\n", lines.join("\n"))
}

/// One path shape's projections plus its loss-ledger row.
#[derive(Debug, Clone)]
pub struct PathProjection {
    /// IRI of the projected `logic:PathShape`.
    pub shape_iri: String,
    /// The serialized extended SPARQL property path.
    pub property_path: String,
    /// The depth-bounded Datalog rule scheme (native-engine syntax).
    pub datalog: String,
    /// The `"property-path"` preservation-ledger row.
    pub ledger: LedgerEntry,
}

/// Project a single path shape to both surfaces, carrying the loss ledger.
pub fn project_path_shape(shape: &PathShapeIr) -> PathProjection {
    let (kind, complexity, drops) = target_meta("property-path");
    PathProjection {
        shape_iri: shape.iri.clone(),
        property_path: property_path_text(shape),
        datalog: datalog_text(shape),
        ledger: LedgerEntry {
            target: "property-path".to_owned(),
            preservation: kind.as_str().to_owned(),
            complexity: complexity.to_owned(),
            lossy_drops: drops.into_iter().map(str::to_owned).collect(),
        },
    }
}

/// Project every `logic:PathShape` carried by a program — the invoked entry point
/// (so the `logic:PropertyPathProjection` target is genuinely exercised, never
/// inert vocabulary).
pub fn project_path_shapes(program: &LogicProgram) -> Vec<PathProjection> {
    program.path_shapes.iter().map(project_path_shape).collect()
}

#[cfg(test)]
mod tests;
