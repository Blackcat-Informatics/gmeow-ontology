// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Projection back-end for `logic:PathShape`: a named/parametric predicate
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

use purrdf::sparql::{NamedNode as SparqlNamedNode, PropertyPathExpression};

use super::super::ir::{LegPath, LogicProgram, PathBase, PathShapeIr};
use super::{LedgerEntry, target_meta};

/// Lower a correspondence [`LegPath`] body to the SPARQL property-path algebra (the lossy
/// projection of the canonical `logic:` composite-path form). Used only to derive the
/// content-addressed key the round-trip gate compares — never to execute the leg.
pub fn lower_leg_path(path: &LegPath) -> PropertyPathExpression {
    match path {
        LegPath::Step(p) => {
            PropertyPathExpression::NamedNode(SparqlNamedNode::new_unchecked(p.clone()))
        }
        LegPath::Inverse(inner) => PropertyPathExpression::Reverse(Box::new(lower_leg_path(inner))),
        LegPath::Seq(parts) => fold_binary(parts, |a, b| {
            PropertyPathExpression::Sequence(Box::new(a), Box::new(b))
        }),
        LegPath::Alt(parts) => fold_binary(parts, |a, b| {
            PropertyPathExpression::Alternative(Box::new(a), Box::new(b))
        }),
    }
}

/// Fold a non-empty list of leg sub-paths into a right-nested binary property-path with
/// `combine`. An empty `Seq`/`Alt` is malformed upstream (the frontend rejects it); we
/// lower it to an empty negated-property-set so the canonical text is still total and
/// stable rather than panicking.
fn fold_binary(
    parts: &[LegPath],
    combine: impl Fn(PropertyPathExpression, PropertyPathExpression) -> PropertyPathExpression,
) -> PropertyPathExpression {
    let mut iter = parts.iter().rev();
    match iter.next() {
        None => PropertyPathExpression::NegatedPropertySet(Vec::new()),
        Some(last) => iter.fold(lower_leg_path(last), |acc, p| {
            combine(lower_leg_path(p), acc)
        }),
    }
}

/// The content-addressed canonical key of a leg body: its normalized form lowered to the
/// SPARQL property-path surface and serialized. Two legs are the same iff these keys are
/// equal — the decidable graph-iso identity the round-trip / mnemomorphism gates use. The
/// `Display` of [`PropertyPathExpression`] round-trips with the parser, so the key is a
/// faithful canonical form, not a lossy digest.
pub fn leg_path_canonical(path: &LegPath) -> String {
    lower_leg_path(&path.normalize()).to_string()
}

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

/// The depth-bounded Datalog projection for a path shape, using
/// `<iri>(?S, ?O, ?W)` relation syntax. The
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
            if shape.min_depth == 1 {
                // Standard transitive closure: result(X,Y) for any path length >= 1.
                lines.push(
                    "% Unbounded depth (>= 1) — recursive transitive closure, exact.".to_owned(),
                );
                lines.push(format!(
                    "<{reachable}>(?X, ?Y, ?W) :- <{edge}>(?X, ?Y, ?W) ."
                ));
                lines.push(format!(
                    "<{reachable}>(?X, ?Y, ?W) :- <{reachable}>(?X, ?Z, ?W), \
                     <{edge}>(?Z, ?Y, ?W) ."
                ));
            } else {
                // Exact unbounded closure for min_depth > 1 (CWE-400 note: the
                // closure auxiliary is recursive but unbounded in the *depth* axis,
                // not in the number of rules emitted — safe).
                //
                // Strategy:
                //   1.  Build the full transitive closure in an auxiliary relation
                //       `closure` (any length >= 1).
                //   2.  Unroll an explicit min_depth-hop chain that seeds the result
                //       predicate: result(X,Z) holds when there are min_depth edges
                //       X->N1->...->N_{min-1}->M and closure(M,Z) (or M=Z for the
                //       exact-min-depth case), guaranteeing at least min_depth hops.
                let min = shape.min_depth;
                let closure_iri = format!("{}/closure", shape.iri);
                lines.push(format!(
                    "% Unbounded depth (>= {min}) — exact: transitive closure restricted \
                     to pairs reachable in at least {min} hops."
                ));
                // Auxiliary: full closure (any depth >= 1).
                lines.push(format!(
                    "<{closure_iri}>(?X, ?Y, ?W) :- <{edge}>(?X, ?Y, ?W) ."
                ));
                lines.push(format!(
                    "<{closure_iri}>(?X, ?Y, ?W) :- <{closure_iri}>(?X, ?Z, ?W), \
                     <{edge}>(?Z, ?Y, ?W) ."
                ));
                // Build the min-depth chain: ?X -> ?N1 -> ?N2 -> ... -> ?N_{min-1}.
                // The last intermediate is ?N_{min-1}; from there the closure reaches ?Y.
                // For min=2: body is  edge(X,N1), closure(N1,Y)   [N1 reached in 1, then >=1 more]
                // For min=3: body is  edge(X,N1), edge(N1,N2), closure(N2,Y)
                // etc.
                let mut body_atoms: Vec<String> = Vec::with_capacity(min as usize);
                let mut prev_var = "?X".to_owned();
                for k in 1..min {
                    let next_var = format!("?N{k}");
                    body_atoms.push(format!("<{edge}>({prev_var}, {next_var}, ?W)"));
                    prev_var = next_var;
                }
                // The final node (?N_{min-1} or ?X for min=1) must reach ?Y via the
                // closure, which itself covers at least 1 hop — giving >= min hops total.
                body_atoms.push(format!("<{closure_iri}>({prev_var}, ?Y, ?W)"));
                lines.push(format!(
                    "<{reachable}>(?X, ?Y, ?W) :- {} .",
                    body_atoms.join(", ")
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

// The path-projection tests exercise the runtime rule engine (crate::rule_ir),
// which lives in gmeow-logic, so they are a gmeow-logic integration test
// (crates/logic/tests/logic_path_projection.rs) rather than an in-crate unit test.
