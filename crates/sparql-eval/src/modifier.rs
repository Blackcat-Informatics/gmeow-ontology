// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Solution modifiers and the `VALUES` / `GRAPH` graph-pattern nodes:
//! `Project`, `Distinct`, `Reduced`, `OrderBy`, `Slice`, plus inline `VALUES` data
//! and named-graph scoping.

use std::cmp::Ordering;
use std::rc::Rc;

use gmeow_rdf_core::{GraphMatch, TermId, TermValue};
use gmeow_sparql_algebra::{Expression, GraphPattern, NamedNodePattern, OrderExpression, Variable};
use gmeow_xsd::{parse_by_iri, value_cmp};

use crate::convert::{ground_term_to_value, named_node_to_value};
use crate::error::EvalError;
use crate::eval::{eval, EvalCtx};
use crate::expr::eval_expr;
use crate::scratch::SolutionTerm;
use crate::solution::{Solution, SolutionSeq, VarSchema};
use crate::DetHashSet;

/// Inline `VALUES`: one solution per binding row, each cell an interned ground term
/// (or unbound for `UNDEF`).
pub(crate) fn eval_values(
    variables: &[Variable],
    bindings: &[Vec<Option<gmeow_sparql_algebra::GroundTerm>>],
    ctx: &mut EvalCtx<'_>,
) -> Result<SolutionSeq, EvalError> {
    let schema = Rc::new(VarSchema::from_vars(variables.iter().cloned()));
    let width = schema.len();
    let mut rows = Vec::with_capacity(bindings.len());
    for binding in bindings {
        let mut row = vec![None; width];
        for (i, cell) in binding.iter().enumerate() {
            if let Some(ground) = cell {
                row[i] = Some(
                    ctx.scratch
                        .intern(ctx.dataset, ground_term_to_value(ground)),
                );
            }
        }
        rows.push(row);
    }
    Ok(SolutionSeq { schema, rows })
}

/// `SELECT`-list projection: restrict to `variables` in order. A projected variable
/// absent from the inner solution yields an all-unbound column.
pub(crate) fn eval_project(
    inner: &GraphPattern,
    variables: &[Variable],
    ctx: &mut EvalCtx<'_>,
) -> Result<SolutionSeq, EvalError> {
    let seq = eval(inner, ctx)?;
    let out = Rc::new(VarSchema::from_vars(variables.iter().cloned()));
    // For each projected column, the source column in the inner schema (if any).
    let src: Vec<Option<usize>> = out.vars().iter().map(|v| seq.schema.index_of(v)).collect();
    let rows = seq
        .rows
        .iter()
        .map(|row| src.iter().map(|s| s.and_then(|c| row[c])).collect())
        .collect();
    Ok(SolutionSeq { schema: out, rows })
}

/// `DISTINCT`: drop duplicate whole-solution rows, preserving first-seen order.
pub(crate) fn eval_distinct(
    inner: &GraphPattern,
    ctx: &mut EvalCtx<'_>,
) -> Result<SolutionSeq, EvalError> {
    Ok(dedup(eval(inner, ctx)?))
}

/// `REDUCED`: permitted to drop duplicates; we apply the same dedup as `DISTINCT`
/// (a stronger-but-permitted reduction than the spec's minimum).
pub(crate) fn eval_reduced(
    inner: &GraphPattern,
    ctx: &mut EvalCtx<'_>,
) -> Result<SolutionSeq, EvalError> {
    Ok(dedup(eval(inner, ctx)?))
}

/// Drop duplicate rows, preserving first-seen order (SolutionTerm equality is exact
/// RDF-term identity — see the scratch-interner promotion rule).
fn dedup(seq: SolutionSeq) -> SolutionSeq {
    let mut seen: DetHashSet<Solution> = DetHashSet::default();
    let mut rows = Vec::new();
    for row in seq.rows {
        if seen.insert(row.clone()) {
            rows.push(row);
        }
    }
    SolutionSeq {
        schema: seq.schema,
        rows,
    }
}

/// `LIMIT`/`OFFSET`: skip `start` solutions then keep at most `length`.
pub(crate) fn eval_slice(
    inner: &GraphPattern,
    start: usize,
    length: Option<usize>,
    ctx: &mut EvalCtx<'_>,
) -> Result<SolutionSeq, EvalError> {
    let seq = eval(inner, ctx)?;
    let rows = seq
        .rows
        .into_iter()
        .skip(start)
        .take(length.unwrap_or(usize::MAX))
        .collect();
    Ok(SolutionSeq {
        schema: seq.schema,
        rows,
    })
}

/// `ORDER BY`: stable-sort by the sort keys under SPARQL ordering (§15.1).
pub(crate) fn eval_order_by(
    inner: &GraphPattern,
    exprs: &[OrderExpression],
    ctx: &mut EvalCtx<'_>,
) -> Result<SolutionSeq, EvalError> {
    let seq = eval(inner, ctx)?;
    let schema = seq.schema.clone();

    // Precompute each row's sort keys as owned values, so the sort comparator is a
    // pure function (no `ctx` borrow during the sort).
    let mut keyed: Vec<(Vec<Option<TermValue>>, Solution)> = Vec::with_capacity(seq.rows.len());
    for row in seq.rows {
        let mut keys = Vec::with_capacity(exprs.len());
        for oe in exprs {
            let term = eval_expr(order_expr(oe), &row, &schema, ctx)?;
            keys.push(term.map(|t| ctx.scratch.value_of(ctx.dataset, t)));
        }
        keyed.push((keys, row));
    }

    keyed.sort_by(|(ka, _), (kb, _)| compare_keys(ka, kb, exprs));
    let rows = keyed.into_iter().map(|(_, row)| row).collect();
    Ok(SolutionSeq { schema, rows })
}

/// `GRAPH name { ... }`: scope the inner pattern to a named graph (or, for a
/// variable, every named graph in turn, binding the variable to each).
pub(crate) fn eval_graph(
    name: &NamedNodePattern,
    inner: &GraphPattern,
    ctx: &mut EvalCtx<'_>,
) -> Result<SolutionSeq, EvalError> {
    match name {
        NamedNodePattern::NamedNode(n) => {
            match ctx.dataset.term_id_by_value(&named_node_to_value(n)) {
                Some(id) => {
                    let saved = ctx.active_graph;
                    ctx.active_graph = GraphMatch::Named(id);
                    let result = eval(inner, ctx);
                    ctx.active_graph = saved;
                    result
                }
                // The named graph IRI is not even a term → it has no quads → empty.
                None => {
                    let seq = eval(inner, ctx)?;
                    Ok(SolutionSeq::empty(seq.schema))
                }
            }
        }
        NamedNodePattern::Variable(v) => eval_graph_var(v, inner, ctx),
    }
}

/// `GRAPH ?g { ... }`: evaluate the inner pattern once per named graph, binding `?g`
/// to the graph IRI, and union the results.
fn eval_graph_var(
    var: &Variable,
    inner: &GraphPattern,
    ctx: &mut EvalCtx<'_>,
) -> Result<SolutionSeq, EvalError> {
    let mut graphs: Vec<TermId> = ctx.dataset.quads().filter_map(|q| q.g).collect();
    graphs.sort();
    graphs.dedup();

    let saved = ctx.active_graph;
    let mut out_schema: Option<Rc<VarSchema>> = None;
    let mut rows = Vec::new();
    for g in graphs {
        ctx.active_graph = GraphMatch::Named(g);
        let inner_seq = eval(inner, ctx)?;
        let mut sch = (*inner_seq.schema).clone();
        let gcol = sch.push(var.clone());
        let width = sch.len();
        for mut row in inner_seq.rows {
            row.resize(width, None);
            row[gcol] = Some(SolutionTerm::Existing(g));
            rows.push(row);
        }
        out_schema = Some(Rc::new(sch));
    }
    ctx.active_graph = saved;

    // No named graphs (or none matched): still produce the right schema with no rows.
    let schema = match out_schema {
        Some(s) => s,
        None => {
            let seq = eval(inner, ctx)?;
            let mut sch = (*seq.schema).clone();
            sch.push(var.clone());
            Rc::new(sch)
        }
    };
    Ok(SolutionSeq { schema, rows })
}

// ---------------------------------------------------------------------------
// ordering
// ---------------------------------------------------------------------------

fn order_expr(oe: &OrderExpression) -> &Expression {
    match oe {
        OrderExpression::Asc(e) | OrderExpression::Desc(e) => e,
    }
}

fn is_descending(oe: &OrderExpression) -> bool {
    matches!(oe, OrderExpression::Desc(_))
}

/// Compare two rows' precomputed sort keys, applying each key's `ASC`/`DESC`.
fn compare_keys(
    a: &[Option<TermValue>],
    b: &[Option<TermValue>],
    exprs: &[OrderExpression],
) -> Ordering {
    for (i, oe) in exprs.iter().enumerate() {
        let mut ord = sparql_order(&a[i], &b[i]);
        if is_descending(oe) {
            ord = ord.reverse();
        }
        if ord != Ordering::Equal {
            return ord;
        }
    }
    Ordering::Equal
}

/// SPARQL ORDER BY total order: unbound sorts before any bound term; otherwise by
/// term kind (blank < IRI < literal < triple) and then within the kind.
fn sparql_order(a: &Option<TermValue>, b: &Option<TermValue>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(x), Some(y)) => term_value_order(x, y),
    }
}

fn kind_rank(v: &TermValue) -> u8 {
    match v {
        TermValue::Blank { .. } => 0,
        TermValue::Iri(_) => 1,
        TermValue::Literal { .. } => 2,
        TermValue::Triple { .. } => 3,
    }
}

fn term_value_order(a: &TermValue, b: &TermValue) -> Ordering {
    match (a, b) {
        (
            TermValue::Blank {
                label: la,
                scope: sa,
            },
            TermValue::Blank {
                label: lb,
                scope: sb,
            },
        ) => (sa.ordinal(), la).cmp(&(sb.ordinal(), lb)),
        (TermValue::Iri(x), TermValue::Iri(y)) => x.cmp(y),
        (
            TermValue::Literal {
                lexical_form: lx,
                datatype: dx,
                language: gx,
                ..
            },
            TermValue::Literal {
                lexical_form: ly,
                datatype: dy,
                language: gy,
                ..
            },
        ) => literal_order((lx, dx, gx), (ly, dy, gy)),
        (
            TermValue::Triple {
                s: sa,
                p: pa,
                o: oa,
            },
            TermValue::Triple {
                s: sb,
                p: pb,
                o: ob,
            },
        ) => term_value_order(sa, sb)
            .then_with(|| term_value_order(pa, pb))
            .then_with(|| term_value_order(oa, ob)),
        _ => kind_rank(a).cmp(&kind_rank(b)),
    }
}

/// Order two literals: by XSD value where both are value-comparable, otherwise a
/// deterministic fall-back by (datatype, language, lexical form).
fn literal_order(a: (&str, &str, &Option<String>), b: (&str, &str, &Option<String>)) -> Ordering {
    let (lx, dx, gx) = a;
    let (ly, dy, gy) = b;
    if let (Ok(Some(ax)), Ok(Some(bx))) = (parse_by_iri(lx, dx), parse_by_iri(ly, dy)) {
        if let Some(ord) = value_cmp(&ax, &bx) {
            return ord;
        }
    }
    (dx, gx, lx).cmp(&(dy, gy, ly))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_rdf_core::{RdfDataset, RdfDatasetBuilder, RdfLiteral};
    use gmeow_sparql_algebra::{NamedNode, NamedNodePattern, TermPattern, TriplePattern};
    use std::sync::Arc;

    const XINT: &str = "http://www.w3.org/2001/XMLSchema#integer";

    fn ages() -> Arc<RdfDataset> {
        // :a :age 30 ; :b :age 17 ; :c :age 30  (duplicate age 30)
        let mut b = RdfDatasetBuilder::new();
        let age = b.intern_iri("http://ex/age".to_owned());
        for (s, n) in [("a", "30"), ("b", "17"), ("c", "30")] {
            let subj = b.intern_iri(format!("http://ex/{s}"));
            let val = b.intern_literal(RdfLiteral {
                lexical_form: n.to_owned(),
                datatype: Some(XINT.to_owned()),
                language: None,
                direction: None,
            });
            b.push_quad(subj, age, val, None);
        }
        b.freeze().expect("freeze")
    }

    fn age_bgp() -> GraphPattern {
        GraphPattern::Bgp {
            patterns: vec![TriplePattern {
                subject: TermPattern::Variable(Variable::new("s")),
                predicate: NamedNodePattern::NamedNode(NamedNode::new_unchecked("http://ex/age")),
                object: TermPattern::Variable(Variable::new("n")),
            }],
        }
    }

    fn ints(ds: &RdfDataset, seq: &SolutionSeq, var: &str) -> Vec<String> {
        let scratch = crate::scratch::ScratchInterner::new();
        let col = seq.schema.index_of(&Variable::new(var)).unwrap();
        seq.rows
            .iter()
            .filter_map(|r| r[col])
            .map(|t| match scratch.value_of(ds, t) {
                TermValue::Literal { lexical_form, .. } => lexical_form,
                other => format!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn order_by_ascending_value_space() {
        let ds = ages();
        let mut ctx = EvalCtx::new(&ds);
        let seq = eval_order_by(
            &age_bgp(),
            &[OrderExpression::Asc(Expression::Variable(Variable::new(
                "n",
            )))],
            &mut ctx,
        )
        .expect("order");
        // 17, 30, 30 — numeric (value-space) ascending.
        assert_eq!(ints(&ds, &seq, "n"), vec!["17", "30", "30"]);
    }

    #[test]
    fn order_by_descending() {
        let ds = ages();
        let mut ctx = EvalCtx::new(&ds);
        let seq = eval_order_by(
            &age_bgp(),
            &[OrderExpression::Desc(Expression::Variable(Variable::new(
                "n",
            )))],
            &mut ctx,
        )
        .expect("order");
        assert_eq!(ints(&ds, &seq, "n"), vec!["30", "30", "17"]);
    }

    #[test]
    fn distinct_drops_duplicate_rows() {
        let ds = ages();
        let mut ctx = EvalCtx::new(&ds);
        // Project to ?n only → {30, 17, 30}; DISTINCT → {30, 17}.
        let project = GraphPattern::Project {
            inner: Box::new(age_bgp()),
            variables: vec![Variable::new("n")],
        };
        let seq = eval_distinct(&project, &mut ctx).expect("distinct");
        assert_eq!(seq.len(), 2);
    }

    #[test]
    fn slice_offset_and_limit() {
        let ds = ages();
        let mut ctx = EvalCtx::new(&ds);
        let ordered = GraphPattern::OrderBy {
            inner: Box::new(age_bgp()),
            expression: vec![OrderExpression::Asc(Expression::Variable(Variable::new(
                "n",
            )))],
        };
        // OFFSET 1 LIMIT 1 over [17,30,30] → [30].
        let seq = eval_slice(&ordered, 1, Some(1), &mut ctx).expect("slice");
        assert_eq!(ints(&ds, &seq, "n"), vec!["30"]);
    }

    #[test]
    fn project_keeps_only_listed_vars_in_order() {
        let ds = ages();
        let mut ctx = EvalCtx::new(&ds);
        let seq = eval_project(&age_bgp(), &[Variable::new("n")], &mut ctx).expect("project");
        assert_eq!(seq.schema.vars(), &[Variable::new("n")]);
        assert_eq!(seq.len(), 3);
    }

    #[test]
    fn values_seeds_solutions() {
        let ds = ages();
        let mut ctx = EvalCtx::new(&ds);
        use gmeow_sparql_algebra::GroundTerm;
        // VALUES ?x { :a UNDEF }
        let vars = vec![Variable::new("x")];
        let bindings = vec![
            vec![Some(GroundTerm::NamedNode(NamedNode::new_unchecked(
                "http://ex/a",
            )))],
            vec![None],
        ];
        let seq = eval_values(&vars, &bindings, &mut ctx).expect("values");
        assert_eq!(seq.len(), 2);
        let x = seq.schema.index_of(&Variable::new("x")).unwrap();
        assert!(seq.rows[0][x].is_some());
        assert!(seq.rows[1][x].is_none()); // UNDEF.
    }
}
