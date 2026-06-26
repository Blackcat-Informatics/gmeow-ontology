// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Basic-graph-pattern (BGP) evaluation — the TermId hot path.
//!
//! A BGP is a conjunction of triple patterns. Evaluation stays entirely in
//! interned [`TermId`](gmeow_rdf_core::TermId) space:
//!
//! 1. **Compile** each triple pattern's three positions to either a [`Pos::Slot`]
//!    (a variable column) or a [`Pos::Bound`] (a ground constant resolved once via
//!    `term_id_by_value`, P4 #838). If a ground constant is absent from the dataset
//!    the whole BGP is empty — that constant cannot match.
//! 2. **Order** the patterns most-selective-first with a minimal static heuristic
//!    ([`selectivity_order`], #913): score by constrained positions, keep the join
//!    connected. Full cardinality-stats cost planning is S7b (#929).
//! 3. **Index-nested-loop join** in that order; for each partial solution, substitute
//!    its already-bound variables into the next pattern's positions and call the
//!    indexed `quads_for_pattern` (P4b #891), then extend. Repeated variables
//!    (`?x p ?x`) and previously-bound variables are enforced at bind time.
//!
//! ## Blank nodes are non-distinguished variables
//!
//! A blank node in a query BGP (`_:b`) is *not* a request to match a specific
//! dataset blank by label — it is an anonymous variable that matches any term and
//! co-refers (by label) **only within this BGP** (SPARQL §4.1.4 / §18.2.1). So a
//! blank position compiles to a synthetic slot variable whose name carries a `NUL`
//! prefix (which the SPARQL grammar can never produce, so it cannot collide with a
//! real `?var`). After the BGP is evaluated these synthetic columns are
//! **projected away**, so two independent BGPs that happen to reuse the label `_:b`
//! never accidentally share a join variable.

use gmeow_rdf_core::{DatasetView, QuadIds, RdfDataset, TermId};
use gmeow_sparql_algebra::{NamedNodePattern, TermPattern, TriplePattern, Variable};

use crate::convert::{ground_term_pattern_to_value, named_node_to_value};
use crate::error::EvalError;
use crate::eval::EvalCtx;
use crate::scratch::SolutionTerm;
use crate::solution::{Solution, SolutionSeq, VarSchema};
use std::rc::Rc;

/// The `NUL`-prefixed marker that distinguishes a synthetic blank-node slot
/// variable from a real, projectable SPARQL variable.
const BLANK_VAR_PREFIX: char = '\u{0}';

/// A compiled triple-pattern position.
enum Pos {
    /// A variable (or blank-node) column index into the working schema.
    Slot(usize),
    /// A ground constant resolved to its dataset id.
    Bound(TermId),
}

/// One compiled triple pattern: its three positions in `(s, p, o)` order.
struct CompiledPattern {
    s: Pos,
    p: Pos,
    o: Pos,
}

/// Evaluate a basic graph pattern to a multiset of solutions over its real
/// (non-blank) variables.
pub(crate) fn eval_bgp(
    patterns: &[TriplePattern],
    ctx: &mut EvalCtx<'_>,
) -> Result<SolutionSeq, EvalError> {
    // The empty BGP is the identity table Z: one solution binding nothing.
    if patterns.is_empty() {
        return Ok(SolutionSeq::unit());
    }

    // Pass 1: collect every slot variable (real + synthetic blank) in first-seen
    // (subject, predicate, object) order — the working column layout.
    let mut working = VarSchema::new();
    for pattern in patterns {
        for key in slot_keys(pattern) {
            working.push(key);
        }
    }

    // Pass 2: compile each pattern; a ground constant absent from the dataset makes
    // the whole BGP empty.
    let mut compiled = Vec::with_capacity(patterns.len());
    for pattern in patterns {
        match compile_pattern(pattern, &working, ctx.dataset)? {
            Some(cp) => compiled.push(cp),
            None => return Ok(empty_over_real_vars(&working)),
        }
    }

    // Reorder the patterns most-selective-first before the join (#913). This is a
    // pure permutation of a commutative join: `Pos::Slot` is an absolute column
    // index into `working`, so reordering cannot change which columns bind — the
    // multiset result is identical, only the join shape (and so the cost) changes.
    let order = selectivity_order(&compiled);

    // Index-nested-loop evaluation. Rows start as a single all-unbound solution.
    let mut rows: Vec<Solution> = vec![vec![None; working.len()]];
    for &i in &order {
        let cp = &compiled[i];
        let mut next = Vec::new();
        for row in &rows {
            let s = query_id(&cp.s, row);
            let p = query_id(&cp.p, row);
            let o = query_id(&cp.o, row);
            for quad in ctx.dataset.quads_for_pattern(s, p, o, ctx.active_graph) {
                if let Some(extended) = bind_row(row, cp, &quad) {
                    next.push(extended);
                }
            }
        }
        rows = next;
        if rows.is_empty() {
            break;
        }
    }

    Ok(project_out_blanks(&working, rows))
}

/// Order compiled BGP patterns most-selective-first (the minimal static heuristic,
/// #913). Full cardinality-stats cost planning is S7b (#929); this never probes the
/// dataset — it scores patterns purely by their *structure*.
///
/// A pattern's score is how many of its three positions are already *constrained*: a
/// ground constant ([`Pos::Bound`]), or a variable already bound by an
/// earlier-scheduled pattern. The order is built greedily, most-constrained-first,
/// under one hard rule:
///
/// > **never schedule a pattern disconnected from the bindings produced so far while
/// > a connected pattern still remains.**
///
/// That keeps the join left-deep and connected (no accidental Cartesian product),
/// which is what guarantees the reorder is never *slower* than the naive
/// left-to-right order on the corpus's chain/star shapes.
///
/// Returns a permutation of `0..compiled.len()`. Determinism: a dense `Vec<bool>`
/// bound-mask (indexed by the dense `0..n_cols` working columns — no hashing, so no
/// hash-iteration order can ever leak into the result) plus a strict-`>` scan in
/// original index order (lowest index wins ties) make the order identical run to run.
fn selectivity_order(compiled: &[CompiledPattern]) -> Vec<usize> {
    let n = compiled.len();
    // Working columns are dense `0..n_cols`; size the bound-mask to the highest slot.
    let n_cols = compiled
        .iter()
        .flat_map(|cp| [&cp.s, &cp.p, &cp.o])
        .filter_map(|pos| match pos {
            Pos::Slot(c) => Some(*c + 1),
            Pos::Bound(_) => None,
        })
        .max()
        .unwrap_or(0);

    let mut bound = vec![false; n_cols];
    let mut scheduled = vec![false; n];
    let mut order = Vec::with_capacity(n);

    for _ in 0..n {
        // If any remaining pattern is connected to the bindings so far, only such
        // patterns are eligible this round — never force a Cartesian product while a
        // connected join is available. (Round 1: `bound` is empty, nothing is
        // connected, so every pattern is eligible and the most-constrained — i.e.
        // the one with the most ground constants — is chosen.)
        let any_connected =
            (0..n).any(|i| !scheduled[i] && pattern_connected(&compiled[i], &bound));

        let mut best: Option<usize> = None;
        let mut best_score = 0usize;
        for i in 0..n {
            if scheduled[i] {
                continue;
            }
            if any_connected && !pattern_connected(&compiled[i], &bound) {
                continue;
            }
            let score = pattern_score(&compiled[i], &bound);
            // Strict `>` over an index-order scan ⇒ lowest original index wins ties.
            if best.is_none() || score > best_score {
                best = Some(i);
                best_score = score;
            }
        }

        let chosen = best.expect("an unscheduled pattern always remains");
        scheduled[chosen] = true;
        mark_bound(&compiled[chosen], &mut bound);
        order.push(chosen);
    }
    order
}

/// How many of a pattern's three positions are already constrained — a ground
/// constant or an already-bound slot. The structural selectivity proxy used by
/// [`selectivity_order`].
fn pattern_score(cp: &CompiledPattern, bound: &[bool]) -> usize {
    [&cp.s, &cp.p, &cp.o]
        .into_iter()
        .filter(|pos| pos_is_constrained(pos, bound))
        .count()
}

/// Whether a pattern shares at least one already-bound variable with the bindings
/// produced so far (so joining it cannot be a Cartesian product).
fn pattern_connected(cp: &CompiledPattern, bound: &[bool]) -> bool {
    [&cp.s, &cp.p, &cp.o]
        .into_iter()
        .any(|pos| matches!(pos, Pos::Slot(c) if bound[*c]))
}

/// A position is constrained iff it is a ground constant or an already-bound slot.
fn pos_is_constrained(pos: &Pos, bound: &[bool]) -> bool {
    match pos {
        Pos::Bound(_) => true,
        Pos::Slot(c) => bound[*c],
    }
}

/// Record a scheduled pattern's slot columns as now-bound.
fn mark_bound(cp: &CompiledPattern, bound: &mut [bool]) {
    for pos in [&cp.s, &cp.p, &cp.o] {
        if let Pos::Slot(c) = pos {
            bound[*c] = true;
        }
    }
}

/// The slot variables a triple pattern introduces, in `(s, p, o)` order. A ground
/// position yields nothing; a blank node yields a synthetic slot variable.
fn slot_keys(pattern: &TriplePattern) -> Vec<Variable> {
    let mut keys = Vec::new();
    if let Some(v) = term_slot_key(&pattern.subject) {
        keys.push(v);
    }
    if let NamedNodePattern::Variable(v) = &pattern.predicate {
        keys.push(v.clone());
    }
    if let Some(v) = term_slot_key(&pattern.object) {
        keys.push(v);
    }
    keys
}

/// The slot variable a term position introduces, if any: a real variable, or a
/// synthetic blank-node variable. Ground terms (incl. quoted triples) yield `None`.
fn term_slot_key(term: &TermPattern) -> Option<Variable> {
    match term {
        TermPattern::Variable(v) => Some(v.clone()),
        TermPattern::BlankNode(b) => Some(blank_var(b.as_str())),
        _ => None,
    }
}

/// The synthetic slot variable for a blank-node label (NUL-prefixed; cannot collide
/// with a parser-produced `?var`).
fn blank_var(label: &str) -> Variable {
    Variable::new(format!("{BLANK_VAR_PREFIX}bnode:{label}"))
}

/// Whether a schema variable is a synthetic blank-node slot (vs. a real variable).
fn is_blank_var(var: &Variable) -> bool {
    var.as_str().starts_with(BLANK_VAR_PREFIX)
}

/// Compile a triple pattern's positions. Returns `Ok(None)` if a ground constant is
/// absent from the dataset (the pattern — and hence the BGP — cannot match).
fn compile_pattern(
    pattern: &TriplePattern,
    schema: &VarSchema,
    dataset: &RdfDataset,
) -> Result<Option<CompiledPattern>, EvalError> {
    let s = match compile_term(&pattern.subject, schema, dataset)? {
        Some(pos) => pos,
        None => return Ok(None),
    };
    let p = match compile_predicate(&pattern.predicate, schema, dataset) {
        Some(pos) => pos,
        None => return Ok(None),
    };
    let o = match compile_term(&pattern.object, schema, dataset)? {
        Some(pos) => pos,
        None => return Ok(None),
    };
    Ok(Some(CompiledPattern { s, p, o }))
}

/// Compile a subject/object term position. `Ok(None)` = an absent ground constant.
fn compile_term(
    term: &TermPattern,
    schema: &VarSchema,
    dataset: &RdfDataset,
) -> Result<Option<Pos>, EvalError> {
    if let Some(key) = term_slot_key(term) {
        let col = schema
            .index_of(&key)
            .expect("every slot key was registered in pass 1");
        return Ok(Some(Pos::Slot(col)));
    }
    let value = ground_term_pattern_to_value(term)?;
    Ok(dataset.term_id_by_value(&value).map(Pos::Bound))
}

/// Compile a predicate position (IRI or variable). `None` = an absent ground IRI.
fn compile_predicate(
    predicate: &NamedNodePattern,
    schema: &VarSchema,
    dataset: &RdfDataset,
) -> Option<Pos> {
    match predicate {
        NamedNodePattern::Variable(v) => Some(Pos::Slot(
            schema
                .index_of(v)
                .expect("every slot key was registered in pass 1"),
        )),
        NamedNodePattern::NamedNode(n) => dataset
            .term_id_by_value(&named_node_to_value(n))
            .map(Pos::Bound),
    }
}

/// The id to query a position with, given the current partial solution: a bound
/// constant, an already-bound variable's id, or `None` (a wildcard / a variable not
/// yet bound). A `Computed` binding (never produced inside a BGP) degrades to a
/// wildcard and is rejected by [`bind_row`].
fn query_id(pos: &Pos, row: &Solution) -> Option<TermId> {
    match pos {
        Pos::Bound(id) => Some(*id),
        Pos::Slot(col) => match row[*col] {
            Some(SolutionTerm::Existing(id)) => Some(id),
            _ => None,
        },
    }
}

/// Try to extend `row` by binding `cp`'s slot positions from `quad`. Returns `None`
/// if a repeated or previously-bound variable disagrees with the quad.
fn bind_row(row: &Solution, cp: &CompiledPattern, quad: &QuadIds) -> Option<Solution> {
    let mut out = row.clone();
    for (pos, id) in [(&cp.s, quad.s), (&cp.p, quad.p), (&cp.o, quad.o)] {
        if let Pos::Slot(col) = pos {
            let value = SolutionTerm::Existing(id);
            match out[*col] {
                Some(existing) if existing != value => return None,
                Some(_) => {}
                None => out[*col] = Some(value),
            }
        }
    }
    Some(out)
}

/// An empty solution sequence over only the real (non-blank) variables of `working`.
fn empty_over_real_vars(working: &VarSchema) -> SolutionSeq {
    let real = real_var_schema(working);
    SolutionSeq::empty(Rc::new(real))
}

/// The schema of `working` restricted to its real variables, in order.
fn real_var_schema(working: &VarSchema) -> VarSchema {
    VarSchema::from_vars(working.vars().iter().filter(|v| !is_blank_var(v)).cloned())
}

/// Project the working rows onto only the real variables, dropping the synthetic
/// blank-node columns (which are scoped to this BGP and must not leak into joins).
/// Multiset cardinality is preserved (no dedup).
fn project_out_blanks(working: &VarSchema, rows: Vec<Solution>) -> SolutionSeq {
    // The working columns that survive, in order.
    let keep: Vec<usize> = working
        .vars()
        .iter()
        .enumerate()
        .filter_map(|(i, v)| (!is_blank_var(v)).then_some(i))
        .collect();

    // Fast path: no blank columns — reuse rows as-is.
    if keep.len() == working.len() {
        return SolutionSeq {
            schema: Rc::new(real_var_schema(working)),
            rows,
        };
    }

    let schema = Rc::new(real_var_schema(working));
    let projected = rows
        .into_iter()
        .map(|row| keep.iter().map(|&i| row[i]).collect())
        .collect();
    SolutionSeq {
        schema,
        rows: projected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch::ScratchInterner;
    use gmeow_rdf_core::{RdfDatasetBuilder, RdfLiteral, TermValue};
    use gmeow_sparql_algebra::{Literal, NamedNode};
    use pretty_assertions::assert_eq;
    use std::sync::Arc;

    /// A small graph:
    ///   :alice :knows :bob ; :name "Alice" .
    ///   :bob   :knows :carol .
    ///   :carol :knows :alice .
    fn social_graph() -> Arc<RdfDataset> {
        let mut b = RdfDatasetBuilder::new();
        let knows = b.intern_iri("http://ex/knows".to_owned());
        let name = b.intern_iri("http://ex/name".to_owned());
        let alice = b.intern_iri("http://ex/alice".to_owned());
        let bob = b.intern_iri("http://ex/bob".to_owned());
        let carol = b.intern_iri("http://ex/carol".to_owned());
        let alice_name = b.intern_literal(RdfLiteral::simple("Alice"));
        b.push_quad(alice, knows, bob, None);
        b.push_quad(bob, knows, carol, None);
        b.push_quad(carol, knows, alice, None);
        b.push_quad(alice, name, alice_name, None);
        b.freeze().expect("freeze")
    }

    fn var_pos(name: &str) -> TermPattern {
        TermPattern::Variable(Variable::new(name))
    }

    fn iri_pos(iri: &str) -> TermPattern {
        TermPattern::NamedNode(NamedNode::new_unchecked(iri))
    }

    fn pred(iri: &str) -> NamedNodePattern {
        NamedNodePattern::NamedNode(NamedNode::new_unchecked(iri))
    }

    fn triple(s: TermPattern, p: NamedNodePattern, o: TermPattern) -> TriplePattern {
        TriplePattern {
            subject: s,
            predicate: p,
            object: o,
        }
    }

    /// Run a BGP over `ds` and materialize each row's bindings for the given
    /// variables as `TermValue`s, sorted for order-insensitive comparison.
    fn run(
        ds: &RdfDataset,
        patterns: &[TriplePattern],
        vars: &[&str],
    ) -> Vec<Vec<Option<TermValue>>> {
        let mut ctx = EvalCtx::new(ds);
        let seq = eval_bgp(patterns, &mut ctx).expect("bgp");
        let cols: Vec<usize> = vars
            .iter()
            .map(|v| {
                seq.schema
                    .index_of(&Variable::new(*v))
                    .expect("var present")
            })
            .collect();
        let scratch = ScratchInterner::new();
        let mut out: Vec<Vec<Option<TermValue>>> = seq
            .rows
            .iter()
            .map(|row| {
                cols.iter()
                    .map(|&c| row[c].map(|t| scratch.value_of(ds, t)))
                    .collect()
            })
            .collect();
        // TermValue is not Ord; sort by a stable Debug key for order-insensitive
        // comparison of the (unordered) solution multiset.
        out.sort_by_key(|row| format!("{row:?}"));
        out
    }

    fn iri_val(iri: &str) -> Option<TermValue> {
        Some(TermValue::Iri(iri.to_owned()))
    }

    #[test]
    fn single_pattern_one_variable() {
        let ds = social_graph();
        // SELECT ?o WHERE { :alice :knows ?o }
        let patterns = [triple(
            iri_pos("http://ex/alice"),
            pred("http://ex/knows"),
            var_pos("o"),
        )];
        let rows = run(&ds, &patterns, &["o"]);
        assert_eq!(rows, vec![vec![iri_val("http://ex/bob")]]);
    }

    #[test]
    fn single_pattern_two_variables_enumerates_all_quads() {
        let ds = social_graph();
        // { ?s :knows ?o }  → all three knows-edges.
        let patterns = [triple(var_pos("s"), pred("http://ex/knows"), var_pos("o"))];
        let rows = run(&ds, &patterns, &["s", "o"]);
        assert_eq!(
            rows,
            vec![
                vec![iri_val("http://ex/alice"), iri_val("http://ex/bob")],
                vec![iri_val("http://ex/bob"), iri_val("http://ex/carol")],
                vec![iri_val("http://ex/carol"), iri_val("http://ex/alice")],
            ]
        );
    }

    #[test]
    fn two_pattern_join_on_shared_variable() {
        let ds = social_graph();
        // { ?a :knows ?b . ?b :knows ?c }  — friends-of-friends.
        let patterns = [
            triple(var_pos("a"), pred("http://ex/knows"), var_pos("b")),
            triple(var_pos("b"), pred("http://ex/knows"), var_pos("c")),
        ];
        let rows = run(&ds, &patterns, &["a", "b", "c"]);
        assert_eq!(
            rows,
            vec![
                vec![
                    iri_val("http://ex/alice"),
                    iri_val("http://ex/bob"),
                    iri_val("http://ex/carol")
                ],
                vec![
                    iri_val("http://ex/bob"),
                    iri_val("http://ex/carol"),
                    iri_val("http://ex/alice")
                ],
                vec![
                    iri_val("http://ex/carol"),
                    iri_val("http://ex/alice"),
                    iri_val("http://ex/bob")
                ],
            ]
        );
    }

    #[test]
    fn absent_constant_yields_empty() {
        let ds = social_graph();
        // :nobody is not in the graph → the constant resolves to absent → empty.
        let patterns = [triple(
            iri_pos("http://ex/nobody"),
            pred("http://ex/knows"),
            var_pos("o"),
        )];
        let rows = run(&ds, &patterns, &["o"]);
        assert!(rows.is_empty());
    }

    #[test]
    fn repeated_variable_requires_self_loop() {
        // A graph with one genuine self-loop and one non-loop edge.
        let mut b = RdfDatasetBuilder::new();
        let p = b.intern_iri("http://ex/p".to_owned());
        let x = b.intern_iri("http://ex/x".to_owned());
        let y = b.intern_iri("http://ex/y".to_owned());
        b.push_quad(x, p, x, None); // self-loop
        b.push_quad(x, p, y, None); // not a loop
        let ds = b.freeze().expect("freeze");

        // { ?v :p ?v } matches only the self-loop.
        let patterns = [triple(var_pos("v"), pred("http://ex/p"), var_pos("v"))];
        let rows = run(&ds, &patterns, &["v"]);
        assert_eq!(rows, vec![vec![iri_val("http://ex/x")]]);
    }

    #[test]
    fn literal_object_constant_matches() {
        let ds = social_graph();
        // { ?s :name "Alice" } → alice.
        let lit = TermPattern::Literal(Literal::new_simple("Alice"));
        let patterns = [triple(var_pos("s"), pred("http://ex/name"), lit)];
        let rows = run(&ds, &patterns, &["s"]);
        assert_eq!(rows, vec![vec![iri_val("http://ex/alice")]]);
    }

    #[test]
    fn blank_node_acts_as_a_variable_and_is_projected_out() {
        let ds = social_graph();
        // { _:b :knows ?o } — the blank is an anonymous variable; it matches every
        // knows-subject, and is NOT exposed as a column.
        let patterns = [triple(
            TermPattern::BlankNode(gmeow_sparql_algebra::BlankNode::new("b")),
            pred("http://ex/knows"),
            var_pos("o"),
        )];
        let mut ctx = EvalCtx::new(&ds);
        let seq = eval_bgp(&patterns, &mut ctx).expect("bgp");
        // Only ?o is a real column; the blank slot was projected away.
        assert_eq!(seq.schema.vars(), &[Variable::new("o")]);
        assert_eq!(seq.len(), 3); // three knows-edges, one row each.
    }

    // ---- selectivity_order (#913) -----------------------------------------

    /// A dataset-local id for a hand-built compiled pattern. The actual value is
    /// irrelevant to ordering — `selectivity_order` only inspects `Bound` vs `Slot`.
    fn tid(i: u32) -> TermId {
        TermId::from_index(i)
    }

    fn cp(s: Pos, p: Pos, o: Pos) -> CompiledPattern {
        CompiledPattern { s, p, o }
    }

    /// Reordering the *source* order of a BGP never changes its result multiset:
    /// `Pos::Slot` is an absolute column index, so the join is commutative.
    #[test]
    fn reordering_source_patterns_preserves_results() {
        let ds = social_graph();
        // A 3-cycle: { ?a :knows ?b . ?b :knows ?c . ?c :knows ?a } → the 3 rotations.
        let p0 = triple(var_pos("a"), pred("http://ex/knows"), var_pos("b"));
        let p1 = triple(var_pos("b"), pred("http://ex/knows"), var_pos("c"));
        let p2 = triple(var_pos("c"), pred("http://ex/knows"), var_pos("a"));

        let forward = run(&ds, &[p0.clone(), p1.clone(), p2.clone()], &["a", "b", "c"]);
        let reversed = run(&ds, &[p2, p1, p0], &["a", "b", "c"]);

        assert_eq!(forward.len(), 3);
        assert_eq!(forward, reversed);
    }

    /// A more-constrained pattern (more ground constants) is scheduled first.
    #[test]
    fn most_constrained_pattern_goes_first() {
        // index 0: all variables (score 0); index 1: two constants (score 2).
        let all_vars = cp(Pos::Slot(0), Pos::Slot(1), Pos::Slot(2));
        let two_const = cp(Pos::Bound(tid(0)), Pos::Bound(tid(1)), Pos::Slot(3));
        let order = selectivity_order(&[all_vars, two_const]);
        assert_eq!(order, vec![1, 0]);
    }

    /// The no-cross-product invariant: a disconnected pattern is never scheduled
    /// while a connected one remains, even when the disconnected one scores higher.
    #[test]
    fn connected_pattern_beats_a_higher_scoring_disconnected_one() {
        // Component A: P0 (:alice :knows ?b)  score 2, anchors the join (slots {b=0}).
        //              P1 (?b :name ?n)        score 1, connected via ?b.
        // Component B: P2 (:bob :age ?x)       score 2, disconnected from A.
        let p0 = cp(Pos::Bound(tid(10)), Pos::Bound(tid(11)), Pos::Slot(0)); // ?b = col 0
        let p1 = cp(Pos::Slot(0), Pos::Bound(tid(12)), Pos::Slot(1)); // ?n = col 1
        let p2 = cp(Pos::Bound(tid(13)), Pos::Bound(tid(14)), Pos::Slot(2)); // ?x = col 2

        let order = selectivity_order(&[p0, p1, p2]);
        // P0 (score 2) and P2 (score 2) tie for the seed → lowest index P0 wins.
        // Then P1 (connected, score 1) MUST precede P2 (disconnected, score 2).
        assert_eq!(order, vec![0, 1, 2]);
        let pos_of = |i: usize| order.iter().position(|&x| x == i).unwrap();
        assert!(
            pos_of(1) < pos_of(2),
            "connected P1 must precede disconnected P2"
        );
    }

    /// A fully disconnected BGP still yields a complete, valid permutation
    /// (most-constrained first), without panicking.
    #[test]
    fn disconnected_bgp_yields_a_valid_permutation() {
        let p0 = cp(Pos::Slot(0), Pos::Bound(tid(1)), Pos::Slot(1)); // score 1
        let p1 = cp(Pos::Slot(2), Pos::Bound(tid(2)), Pos::Bound(tid(3))); // score 2
        let order = selectivity_order(&[p0, p1]);
        assert_eq!(order, vec![1, 0]); // higher-scoring disconnected pattern first.
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec![0, 1]); // a genuine permutation of 0..n.
    }

    /// The order is identical run to run (no hash-iteration nondeterminism).
    #[test]
    fn order_is_deterministic() {
        let make = || {
            vec![
                cp(Pos::Slot(0), Pos::Bound(tid(1)), Pos::Slot(1)),
                cp(Pos::Slot(0), Pos::Bound(tid(2)), Pos::Slot(2)),
                cp(Pos::Slot(0), Pos::Bound(tid(3)), Pos::Slot(3)),
            ]
        };
        assert_eq!(selectivity_order(&make()), selectivity_order(&make()));
    }

    /// Equal-scoring patterns are broken by lowest original index (stable).
    #[test]
    fn ties_break_on_lowest_original_index() {
        // Two disconnected patterns, each score 1 → index 0 must lead.
        let p0 = cp(Pos::Slot(0), Pos::Bound(tid(9)), Pos::Slot(1));
        let p1 = cp(Pos::Slot(2), Pos::Bound(tid(9)), Pos::Slot(3));
        assert_eq!(selectivity_order(&[p0, p1]), vec![0, 1]);
    }

    /// An empty BGP produces an empty order — the `0..n` loop runs zero times so the
    /// `.expect("an unscheduled pattern always remains")` inside the loop is never
    /// reached. Guards the n == 0 boundary.
    #[test]
    fn empty_bgp_orders_to_empty() {
        assert_eq!(selectivity_order(&[]), Vec::<usize>::new());
    }

    /// All-ground patterns contain no `Pos::Slot` positions, so `n_cols == 0` and
    /// the bound-mask is zero-length. `pattern_connected` and `mark_bound` must not
    /// index the empty mask. Every position is `Pos::Bound`, so every pattern scores 3
    /// (all constrained) and none is ever "connected" (no slots). Two such patterns tie
    /// at score 3 — lowest-index-wins gives [0, 1].
    #[test]
    fn all_ground_bgp_orders_by_score() {
        // p0 and p1: all three positions Bound → score 3 each, n_cols == 0.
        let p0 = cp(Pos::Bound(tid(0)), Pos::Bound(tid(1)), Pos::Bound(tid(2)));
        let p1 = cp(Pos::Bound(tid(3)), Pos::Bound(tid(4)), Pos::Bound(tid(5)));
        let order = selectivity_order(&[p0, p1]);
        // Tie at score 3 → lowest original index wins → [0, 1].
        assert_eq!(order, vec![0, 1]);
        // Confirm it is a genuine permutation of 0..2.
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec![0, 1]);
    }
}
