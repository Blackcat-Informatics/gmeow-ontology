// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Binary graph-pattern operators: `Join` and `Union` (multiset semantics).
//!
//! Both produce a result over the **ordered union** of the operand schemas (left
//! columns first), and both preserve multiset cardinality — duplicate solutions
//! are kept (no implicit `DISTINCT`).
//!
//! `Join` is a hash join on the shared variables. The wrinkle is **unbound shared
//! columns**: a solution may leave a shared variable unbound (`None`), which is
//! compatible with any value (SPARQL §17.5 / §18.2.2). A pure hash-on-key join is
//! correct only when every shared column is bound, so the build side is split into
//! a key-indexed set (all shared columns bound) and a `wild` list (≥1 shared column
//! unbound), and a probe row that itself has an unbound shared column falls back to
//! a compatibility scan over all build rows. The common case — two fully-bound BGPs
//! — stays an O(n+m) hash join.

use std::rc::Rc;

use gmeow_sparql_algebra::GraphPattern;

use crate::error::EvalError;
use crate::eval::{eval, EvalCtx};
use crate::scratch::SolutionTerm;
use crate::solution::{compatible, Solution, SolutionSeq, VarSchema};
use crate::DetHashMap;

/// Evaluate `left . right` (algebra `Join`) as a hash join on shared variables.
pub(crate) fn eval_join(
    left: &GraphPattern,
    right: &GraphPattern,
    ctx: &mut EvalCtx<'_>,
) -> Result<SolutionSeq, EvalError> {
    let l = eval(left, ctx)?;
    let r = eval(right, ctx)?;
    Ok(hash_join(&l, &r))
}

/// Evaluate `left UNION right` as a multiset concatenation over the union schema.
pub(crate) fn eval_union(
    left: &GraphPattern,
    right: &GraphPattern,
    ctx: &mut EvalCtx<'_>,
) -> Result<SolutionSeq, EvalError> {
    let l = eval(left, ctx)?;
    let r = eval(right, ctx)?;

    let out = l.schema.union(&r.schema);
    let out_len = out.len();
    let left_len = l.schema.len();
    let right_to_out = right_to_out_map(&r.schema, &out);

    let mut rows = Vec::with_capacity(l.rows.len() + r.rows.len());
    for lrow in &l.rows {
        // Left columns are out[0..left_len] in order; pad the rest with None.
        let mut row = vec![None; out_len];
        row[..left_len].copy_from_slice(lrow);
        rows.push(row);
    }
    for rrow in &r.rows {
        let mut row = vec![None; out_len];
        for (j, &cell) in rrow.iter().enumerate() {
            row[right_to_out[j]] = cell;
        }
        rows.push(row);
    }

    Ok(SolutionSeq {
        schema: Rc::new(out),
        rows,
    })
}

/// The mapping from a right operand's column ordinal to its ordinal in `out`.
fn right_to_out_map(right: &VarSchema, out: &VarSchema) -> Vec<usize> {
    right
        .vars()
        .iter()
        .map(|v| {
            out.index_of(v)
                .expect("union schema contains every right variable")
        })
        .collect()
}

/// Hash-join two solution sequences on their shared variables.
fn hash_join(l: &SolutionSeq, r: &SolutionSeq) -> SolutionSeq {
    let out = l.schema.union(&r.schema);
    let out_len = out.len();
    let left_len = l.schema.len();
    let right_to_out = right_to_out_map(&r.schema, &out);
    // Shared columns as (left_ordinal, right_ordinal) pairs, in left order.
    let shared = l.schema.shared_columns(&r.schema);

    // Build side = right. Index rows whose shared columns are all bound; keep the
    // rest (with an unbound shared column) as `wild`.
    let mut keyed: DetHashMap<Vec<SolutionTerm>, Vec<usize>> = DetHashMap::default();
    let mut wild: Vec<usize> = Vec::new();
    for (idx, rrow) in r.rows.iter().enumerate() {
        match bound_key(rrow, &shared, KeySide::Right) {
            Some(key) => keyed.entry(key).or_default().push(idx),
            None => wild.push(idx),
        }
    }

    let mut rows = Vec::new();
    for lrow in &l.rows {
        match bound_key(lrow, &shared, KeySide::Left) {
            // Probe is fully bound on shared columns: hit the matching bucket
            // (exact key ⇒ compatible) plus any wild build rows it is compatible
            // with (a wild row's None shared column matches anything).
            Some(key) => {
                if let Some(idxs) = keyed.get(&key) {
                    for &idx in idxs {
                        rows.push(merge(lrow, &r.rows[idx], left_len, &right_to_out, out_len));
                    }
                }
                for &idx in &wild {
                    if compatible(lrow, &r.rows[idx], &shared) {
                        rows.push(merge(lrow, &r.rows[idx], left_len, &right_to_out, out_len));
                    }
                }
            }
            // Probe has an unbound shared column: it can match any build row, so
            // fall back to a compatibility scan over all of them.
            None => {
                for rrow in &r.rows {
                    if compatible(lrow, rrow, &shared) {
                        rows.push(merge(lrow, rrow, left_len, &right_to_out, out_len));
                    }
                }
            }
        }
    }

    SolutionSeq {
        schema: Rc::new(out),
        rows,
    }
}

/// Which side's ordinal a shared-column pair addresses.
#[derive(Clone, Copy)]
enum KeySide {
    Left,
    Right,
}

/// The shared-column key of `row`, or `None` if any shared column is unbound.
///
/// Both sides build the key in the same `shared` order, so a left key equals a
/// right key iff the two rows agree on every (bound) shared column.
fn bound_key(
    row: &Solution,
    shared: &[(usize, usize)],
    side: KeySide,
) -> Option<Vec<SolutionTerm>> {
    let mut key = Vec::with_capacity(shared.len());
    for &(ia, ib) in shared {
        let col = match side {
            KeySide::Left => ia,
            KeySide::Right => ib,
        };
        key.push(row[col]?);
    }
    Some(key)
}

/// Merge a compatible `(left_row, right_row)` pair into one solution over the output
/// layout. Left columns occupy `out[0..left_len]`; each right column fills its
/// output slot only if still unbound, so a shared column unbound on the left is
/// filled from the right (and an already-bound shared column — equal by
/// compatibility — is left intact).
fn merge(
    left_row: &Solution,
    right_row: &Solution,
    left_len: usize,
    right_to_out: &[usize],
    out_len: usize,
) -> Solution {
    let mut merged = vec![None; out_len];
    merged[..left_len].copy_from_slice(left_row);
    for (j, &cell) in right_row.iter().enumerate() {
        let p = right_to_out[j];
        if merged[p].is_none() {
            merged[p] = cell;
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalCtx;
    use gmeow_rdf_core::{RdfDataset, RdfDatasetBuilder};
    use gmeow_sparql_algebra::{NamedNode, NamedNodePattern, TermPattern, TriplePattern, Variable};
    use pretty_assertions::assert_eq;
    use std::sync::Arc;

    fn graph() -> Arc<RdfDataset> {
        // :a :knows :b ; :likes :cake .
        // :b :likes :tea .
        let mut b = RdfDatasetBuilder::new();
        let knows = b.intern_iri("http://ex/knows".to_owned());
        let likes = b.intern_iri("http://ex/likes".to_owned());
        let a = b.intern_iri("http://ex/a".to_owned());
        let bb = b.intern_iri("http://ex/b".to_owned());
        let cake = b.intern_iri("http://ex/cake".to_owned());
        let tea = b.intern_iri("http://ex/tea".to_owned());
        b.push_quad(a, knows, bb, None);
        b.push_quad(a, likes, cake, None);
        b.push_quad(bb, likes, tea, None);
        b.freeze().expect("freeze")
    }

    fn vp(n: &str) -> TermPattern {
        TermPattern::Variable(Variable::new(n))
    }
    fn pred(iri: &str) -> NamedNodePattern {
        NamedNodePattern::NamedNode(NamedNode::new_unchecked(iri))
    }
    fn bgp(s: TermPattern, p: NamedNodePattern, o: TermPattern) -> GraphPattern {
        GraphPattern::Bgp {
            patterns: vec![TriplePattern {
                subject: s,
                predicate: p,
                object: o,
            }],
        }
    }

    fn render(ds: &RdfDataset, seq: &SolutionSeq, vars: &[&str]) -> Vec<Vec<Option<String>>> {
        let scratch = crate::scratch::ScratchInterner::new();
        let cols: Vec<usize> = vars
            .iter()
            .map(|v| seq.schema.index_of(&Variable::new(*v)).expect("var"))
            .collect();
        let mut out: Vec<Vec<Option<String>>> = seq
            .rows
            .iter()
            .map(|row| {
                cols.iter()
                    .map(|&c| {
                        row[c].map(|t| match scratch.value_of(ds, t) {
                            gmeow_rdf_core::TermValue::Iri(s) => s,
                            other => format!("{other:?}"),
                        })
                    })
                    .collect()
            })
            .collect();
        out.sort();
        out
    }

    #[test]
    fn join_on_shared_variable() {
        let ds = graph();
        let mut ctx = EvalCtx::new(&ds);
        // { ?x :knows ?y } JOIN { ?y :likes ?z }
        let left = bgp(vp("x"), pred("http://ex/knows"), vp("y"));
        let right = bgp(vp("y"), pred("http://ex/likes"), vp("z"));
        let seq = eval_join(&left, &right, &mut ctx).expect("join");
        // a knows b; b likes tea → (x=a, y=b, z=tea).
        assert_eq!(
            render(&ds, &seq, &["x", "y", "z"]),
            vec![vec![
                Some("http://ex/a".to_owned()),
                Some("http://ex/b".to_owned()),
                Some("http://ex/tea".to_owned()),
            ]]
        );
    }

    #[test]
    fn join_with_no_shared_vars_is_cross_product() {
        let ds = graph();
        let mut ctx = EvalCtx::new(&ds);
        // { ?x :knows ?y } JOIN { ?p :likes ?q } — disjoint vars → cross product.
        let left = bgp(vp("x"), pred("http://ex/knows"), vp("y")); // 1 row
        let right = bgp(vp("p"), pred("http://ex/likes"), vp("q")); // 2 rows
        let seq = eval_join(&left, &right, &mut ctx).expect("join");
        assert_eq!(seq.len(), 2); // 1 × 2.
    }

    #[test]
    fn join_with_no_overlap_is_empty() {
        let ds = graph();
        let mut ctx = EvalCtx::new(&ds);
        // { ?y :likes ?z } JOIN { ?y :knows ?w } — y=b likes tea, but b knows
        // nothing; y=a likes cake, a knows b. Shared y: a(likes cake)+a(knows b).
        let left = bgp(vp("y"), pred("http://ex/likes"), vp("z")); // y∈{a,b}
        let right = bgp(vp("y"), pred("http://ex/knows"), vp("w")); // y∈{a}
        let seq = eval_join(&left, &right, &mut ctx).expect("join");
        // Only y=a survives: (y=a, z=cake, w=b).
        assert_eq!(
            render(&ds, &seq, &["y", "z", "w"]),
            vec![vec![
                Some("http://ex/a".to_owned()),
                Some("http://ex/cake".to_owned()),
                Some("http://ex/b".to_owned()),
            ]]
        );
    }

    #[test]
    fn union_concatenates_preserving_multiset() {
        let ds = graph();
        let mut ctx = EvalCtx::new(&ds);
        // { ?s :knows ?o } UNION { ?s :likes ?o }  → 1 + 2 = 3 rows.
        let left = bgp(vp("s"), pred("http://ex/knows"), vp("o"));
        let right = bgp(vp("s"), pred("http://ex/likes"), vp("o"));
        let seq = eval_union(&left, &right, &mut ctx).expect("union");
        assert_eq!(seq.len(), 3);
        // Same var names on both sides → schema is exactly [s, o].
        assert_eq!(seq.schema.vars(), &[Variable::new("s"), Variable::new("o")]);
    }

    #[test]
    fn union_of_disjoint_schemas_widens_and_pads() {
        let ds = graph();
        let mut ctx = EvalCtx::new(&ds);
        // { ?a :knows ?b } UNION { ?c :likes ?d } → schema [a,b,c,d]; each row binds
        // only its own side's two columns, the other two are None.
        let left = bgp(vp("a"), pred("http://ex/knows"), vp("b")); // 1
        let right = bgp(vp("c"), pred("http://ex/likes"), vp("d")); // 2
        let seq = eval_union(&left, &right, &mut ctx).expect("union");
        assert_eq!(seq.len(), 3);
        assert_eq!(
            seq.schema.vars(),
            &[
                Variable::new("a"),
                Variable::new("b"),
                Variable::new("c"),
                Variable::new("d"),
            ]
        );
        // The left row has c,d unbound; a right row has a,b unbound.
        let left_rows = seq.rows.iter().filter(|r| r[0].is_some()).count();
        let right_rows = seq.rows.iter().filter(|r| r[2].is_some()).count();
        assert_eq!((left_rows, right_rows), (1, 2));
    }
}
