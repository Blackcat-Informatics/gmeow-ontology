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

use gmeow_rdf_core::{DatasetView, GraphMatch, QuadIds, RdfDataset, TermId, TermRef};
use gmeow_sparql_algebra::{NamedNodePattern, TermPattern, TriplePattern, Variable};

use crate::convert::{ground_term_pattern_to_value, named_node_to_value};
use crate::dataset_spec::GraphScope;
use crate::error::EvalError;
use crate::eval::EvalCtx;
use crate::scratch::SolutionTerm;
use crate::solution::{Solution, SolutionSeq, VarSchema};
use crate::DetHashSet;
use std::rc::Rc;

/// The `rdf:reifies` predicate IRI — the indirection edge of the RDF 1.2 reification
/// layer. A triple pattern whose predicate is bound to this IRI (and whose object is a
/// quoted-triple pattern) draws candidates from the dataset's reifier side-table via
/// [`RdfDataset::reifier_quads`], which is invisible to the `quads` table.
const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";

/// The `NUL`-prefixed marker that distinguishes a synthetic blank-node slot
/// variable from a real, projectable SPARQL variable.
const BLANK_VAR_PREFIX: char = '\u{0}';

/// A compiled triple-pattern position.
enum Pos {
    /// A variable (or blank-node) column index into the working schema.
    Slot(usize),
    /// A ground constant resolved to its dataset id.
    Bound(TermId),
    /// A nested RDF 1.2 quoted-triple pattern `<<( s p o )>>` that contains at least
    /// one variable (a fully-ground quoted triple resolves to a single [`Pos::Bound`]
    /// id instead). Binding descends into the candidate row's triple-term value,
    /// unifying the inner positions and enforcing repeated-variable consistency.
    Triple(Box<TriplePos>),
}

/// A compiled nested quoted-triple position: its three component positions, each
/// itself a [`Pos`] (so quoted triples may nest, and any component may be a variable,
/// a ground constant, or a further nested triple).
struct TriplePos {
    s: Pos,
    p: Pos,
    o: Pos,
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

    // The graph scope for this BGP (resolved once — `active_graph` is fixed across a
    // single BGP; `GRAPH` wrapping is applied by `eval_graph` before recursing in).
    let scope = ctx.active_dataset.scope_for(ctx.active_graph);

    // The interned id of `rdf:reifies`, resolved once. `None` ⇒ the dataset has no
    // reifier layer at all (the predicate was never interned), so no virtual reifier
    // candidates exist for any pattern.
    let reifies_id = ctx
        .dataset
        .term_id_by_value(&gmeow_rdf_core::TermValue::Iri(RDF_REIFIES.to_owned()));

    // Index-nested-loop evaluation. Rows start as a single all-unbound solution.
    let mut rows: Vec<Solution> = vec![vec![None; working.len()]];
    for &i in &order {
        let cp = &compiled[i];
        let mut next = Vec::new();
        for row in &rows {
            let s = query_id(&cp.s, row);
            let p = query_id(&cp.p, row);
            let o = query_id(&cp.o, row);
            match &scope {
                // Single-graph scope (store default / a named graph): the indexed
                // partition_point read, unchanged — no de-dup overhead.
                GraphScope::One(gm) => {
                    for quad in ctx.dataset.quads_for_pattern(s, p, o, *gm) {
                        if let Some(extended) = bind_row(row, cp, &quad, ctx.dataset) {
                            next.push(extended);
                        }
                    }
                    // The RDF 1.2 reification layer is a dataset-level (default-graph)
                    // side-table outside `quads`, so fold its virtual triples in here
                    // — additively (no double counting) — whenever this scope includes
                    // the default graph. A `GRAPH ?g`/named scope (`gm` matching only a
                    // named graph) never sees it, matching the store-default treatment.
                    if gm.matches(None) {
                        for quad in virtual_candidates(ctx.dataset, cp, s, p, o, reifies_id) {
                            if let Some(extended) = bind_row(row, cp, &quad, ctx.dataset) {
                                next.push(extended);
                            }
                        }
                    }
                }
                // A FROM/USING-merged default graph: union the per-graph reads, but
                // RDF-merge unions *triples*, so a triple present in two merged graphs
                // must bind once — de-dupe by (s, p, o) for this pattern+row. The
                // reification layer is store-default content (not part of an explicitly
                // FROM-named merge), so it is not folded into a merged scope.
                GraphScope::Merge(gs) => {
                    let mut seen: DetHashSet<(TermId, TermId, TermId)> = DetHashSet::default();
                    for &g in gs {
                        for quad in ctx.dataset.quads_for_pattern(s, p, o, GraphMatch::Named(g)) {
                            if !seen.insert((quad.s, quad.p, quad.o)) {
                                continue;
                            }
                            if let Some(extended) = bind_row(row, cp, &quad, ctx.dataset) {
                                next.push(extended);
                            }
                        }
                    }
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
/// dataset — it scores patterns purely by their *structure* (Principle 12: evaluation
/// order is computed in the engine, never asserted or materialised as triples).
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
/// left-to-right order on the corpus's chain/star shapes. Note that while the
/// reorder preserves the result *multiset*, it does not preserve the observable row
/// *sequence* of a `SELECT` without `ORDER BY` — which is spec-permitted (SPARQL §11
/// leaves solution order unspecified absent `ORDER BY`), so any golden harness over
/// an un-`ORDER BY`-ed query must be order-tolerant.
///
/// Returns a permutation of `0..compiled.len()`. Determinism: a dense `Vec<bool>`
/// bound-mask (indexed by the dense `0..n_cols` working columns — no hashing, so no
/// hash-iteration order can ever leak into the result) plus a strict-`>` scan in
/// original index order (lowest index wins ties) make the order identical run to run.
fn selectivity_order(compiled: &[CompiledPattern]) -> Vec<usize> {
    let n = compiled.len();
    // Working columns are dense `0..n_cols`; size the bound-mask to the highest slot.
    // A position may be a nested triple, so descend through it to find every slot.
    let mut n_cols = 0usize;
    for cp in compiled {
        for pos in [&cp.s, &cp.p, &cp.o] {
            for_each_slot(pos, &mut |c| n_cols = n_cols.max(c + 1));
        }
    }

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
/// produced so far (so joining it cannot be a Cartesian product). Descends into nested
/// quoted triples: a triple position is connected if any of its inner slots is bound.
fn pattern_connected(cp: &CompiledPattern, bound: &[bool]) -> bool {
    [&cp.s, &cp.p, &cp.o]
        .into_iter()
        .any(|pos| pos_has_bound_slot(pos, bound))
}

/// Whether a position contains an already-bound slot anywhere (recursively).
fn pos_has_bound_slot(pos: &Pos, bound: &[bool]) -> bool {
    match pos {
        Pos::Bound(_) => false,
        Pos::Slot(c) => bound[*c],
        Pos::Triple(t) => [&t.s, &t.p, &t.o]
            .into_iter()
            .any(|p| pos_has_bound_slot(p, bound)),
    }
}

/// A position is constrained iff it is a ground constant, an already-bound slot, or a
/// nested triple whose every component is itself constrained.
fn pos_is_constrained(pos: &Pos, bound: &[bool]) -> bool {
    match pos {
        Pos::Bound(_) => true,
        Pos::Slot(c) => bound[*c],
        Pos::Triple(t) => [&t.s, &t.p, &t.o]
            .into_iter()
            .all(|p| pos_is_constrained(p, bound)),
    }
}

/// Record a scheduled pattern's slot columns as now-bound (descending into nested
/// quoted triples).
fn mark_bound(cp: &CompiledPattern, bound: &mut [bool]) {
    for pos in [&cp.s, &cp.p, &cp.o] {
        for_each_slot(pos, &mut |c| bound[c] = true);
    }
}

/// Visit every slot column reachable from a position (itself, or the inner positions
/// of a nested quoted triple).
fn for_each_slot(pos: &Pos, f: &mut impl FnMut(usize)) {
    match pos {
        Pos::Bound(_) => {}
        Pos::Slot(c) => f(*c),
        Pos::Triple(t) => {
            for inner in [&t.s, &t.p, &t.o] {
                for_each_slot(inner, f);
            }
        }
    }
}

/// The slot variables a triple pattern introduces, in `(s, p, o)` order — descending
/// into any nested quoted-triple position so its inner variables become columns too. A
/// ground position yields nothing; a blank node yields a synthetic slot variable.
fn slot_keys(pattern: &TriplePattern) -> Vec<Variable> {
    let mut keys = Vec::new();
    collect_triple_slot_keys(pattern, &mut keys);
    keys
}

/// Append a triple pattern's slot variables (recursively through nested quoted
/// triples) in `(s, p, o)` order.
fn collect_triple_slot_keys(pattern: &TriplePattern, keys: &mut Vec<Variable>) {
    collect_term_slot_keys(&pattern.subject, keys);
    if let NamedNodePattern::Variable(v) = &pattern.predicate {
        keys.push(v.clone());
    }
    collect_term_slot_keys(&pattern.object, keys);
}

/// Append a term position's slot variables: a real variable, a synthetic blank-node
/// variable, or — for a quoted triple — its inner variables (recursively). Ground
/// terms yield nothing.
fn collect_term_slot_keys(term: &TermPattern, keys: &mut Vec<Variable>) {
    match term {
        TermPattern::Variable(v) => keys.push(v.clone()),
        TermPattern::BlankNode(b) => keys.push(blank_var(b.as_str())),
        TermPattern::Triple(t) => collect_triple_slot_keys(t, keys),
        TermPattern::NamedNode(_) | TermPattern::Literal(_) => {}
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

/// Compile a subject/object term position. `Ok(None)` = an absent ground constant
/// (the pattern — and hence the BGP — cannot match).
fn compile_term(
    term: &TermPattern,
    schema: &VarSchema,
    dataset: &RdfDataset,
) -> Result<Option<Pos>, EvalError> {
    match term {
        TermPattern::Variable(v) => Ok(Some(Pos::Slot(slot_col(schema, v)))),
        TermPattern::BlankNode(b) => Ok(Some(Pos::Slot(slot_col(schema, &blank_var(b.as_str()))))),
        // A quoted-triple position: if it contains a variable it is a STRUCTURAL match
        // that binds inner columns (`Pos::Triple`); a fully-ground quoted triple
        // resolves to a single interned id (`Pos::Bound`) exactly like any constant.
        TermPattern::Triple(t) => {
            if triple_has_variable(t) {
                match compile_triple_pos(t, schema, dataset)? {
                    Some(tp) => Ok(Some(Pos::Triple(Box::new(tp)))),
                    None => Ok(None),
                }
            } else {
                let value = ground_term_pattern_to_value(term)?;
                Ok(dataset.term_id_by_value(&value).map(Pos::Bound))
            }
        }
        TermPattern::NamedNode(_) | TermPattern::Literal(_) => {
            let value = ground_term_pattern_to_value(term)?;
            Ok(dataset.term_id_by_value(&value).map(Pos::Bound))
        }
    }
}

/// Compile a nested quoted-triple pattern's three positions. `Ok(None)` if any
/// ground component is absent from the dataset (so the whole pattern cannot match).
fn compile_triple_pos(
    triple: &TriplePattern,
    schema: &VarSchema,
    dataset: &RdfDataset,
) -> Result<Option<TriplePos>, EvalError> {
    let s = match compile_term(&triple.subject, schema, dataset)? {
        Some(pos) => pos,
        None => return Ok(None),
    };
    let p = match compile_predicate(&triple.predicate, schema, dataset) {
        Some(pos) => pos,
        None => return Ok(None),
    };
    let o = match compile_term(&triple.object, schema, dataset)? {
        Some(pos) => pos,
        None => return Ok(None),
    };
    Ok(Some(TriplePos { s, p, o }))
}

/// The working-schema column of a slot variable (registered in pass 1).
fn slot_col(schema: &VarSchema, var: &Variable) -> usize {
    schema
        .index_of(var)
        .expect("every slot key was registered in pass 1")
}

/// Whether a quoted-triple pattern contains at least one variable anywhere (including
/// nested quoted triples). A variable-free quoted triple is a ground constant.
fn triple_has_variable(triple: &TriplePattern) -> bool {
    term_has_variable(&triple.subject)
        || matches!(triple.predicate, NamedNodePattern::Variable(_))
        || term_has_variable(&triple.object)
}

/// Whether a term position contains a variable (recursively through quoted triples).
/// A blank node is a non-distinguished variable, so it counts.
fn term_has_variable(term: &TermPattern) -> bool {
    match term {
        TermPattern::Variable(_) | TermPattern::BlankNode(_) => true,
        TermPattern::Triple(t) => triple_has_variable(t),
        TermPattern::NamedNode(_) | TermPattern::Literal(_) => false,
    }
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
        // A structural quoted-triple position is not addressable as a single id probe
        // key for the candidate scan; it degrades to a wildcard and is unified
        // structurally in `bind_row` (which descends into the candidate's triple term).
        Pos::Triple(_) => None,
    }
}

/// Try to extend `row` by binding `cp`'s positions from `quad`. Returns `None` if a
/// repeated or previously-bound variable disagrees with the quad, if a nested
/// quoted-triple position fails to unify, or if a ground constant disagrees (the
/// virtual reification candidates are NOT pre-filtered by `quads_for_pattern`, so a
/// `Pos::Bound` mismatch must be rejected here).
fn bind_row(
    row: &Solution,
    cp: &CompiledPattern,
    quad: &QuadIds,
    dataset: &RdfDataset,
) -> Option<Solution> {
    let mut out = row.clone();
    for (pos, id) in [(&cp.s, quad.s), (&cp.p, quad.p), (&cp.o, quad.o)] {
        if !bind_pos(&mut out, pos, id, dataset) {
            return None;
        }
    }
    Some(out)
}

/// Unify one compiled position against a candidate term id, mutating `out` with any
/// newly bound slots. Returns `false` (caller rejects the row) on any disagreement:
/// - a `Pos::Bound` constant that does not equal the candidate id;
/// - a `Pos::Slot` repeated/previously-bound variable that disagrees;
/// - a `Pos::Triple` whose candidate id is not a triple term, or whose components fail
///   to unify recursively.
fn bind_pos(out: &mut Solution, pos: &Pos, id: TermId, dataset: &RdfDataset) -> bool {
    match pos {
        Pos::Bound(want) => *want == id,
        Pos::Slot(col) => {
            let value = SolutionTerm::Existing(id);
            match out[*col] {
                Some(existing) => existing == value,
                None => {
                    out[*col] = Some(value);
                    true
                }
            }
        }
        Pos::Triple(t) => match dataset.resolve(id) {
            TermRef::Triple { s, p, o } => {
                bind_pos(out, &t.s, s, dataset)
                    && bind_pos(out, &t.p, p, dataset)
                    && bind_pos(out, &t.o, o, dataset)
            }
            // The candidate term is not a quoted triple, so a structural triple pattern
            // cannot match it.
            _ => false,
        },
    }
}

/// The virtual triple candidates from the RDF 1.2 reification layer that match a
/// pattern's bound `(s, p, o)` probe, streamed lazily (reifier rows first, then
/// annotation rows — each in the side-tables' frozen sorted order). The layer is
/// NOT in `quads`, so these are strictly additive (no double counting).
///
/// Two layers contribute:
/// - **Reifier rows** `(reifier, rdf:reifies, triple-term)` — included only when the
///   pattern's predicate *can* be `rdf:reifies` (unbound, or bound exactly to it).
///   When the predicate is bound to some other IRI, no reifier row can match, so the
///   layer is skipped entirely.
/// - **Annotation rows** `(reifier, annPred, annObj)` — a reifier's statement
///   annotations look like ordinary triples whose subject is a reifier. When the
///   pattern's subject is bound, [`RdfDataset::annotations_of`] indexes straight to
///   that reifier's run; otherwise the whole annotation table is scanned.
///
/// Every candidate is residually filtered by the same id-equality the default scan
/// applies (`quads_for_pattern`), because — unlike `quads_for_pattern` — the virtual
/// iterators are not pre-narrowed by the probe. The probe ids (`s`, `p`, `o`) are
/// `Copy` and captured by value into the closures, so no per-row heap allocation is
/// needed.
fn virtual_candidates<'ds>(
    dataset: &'ds RdfDataset,
    cp: &CompiledPattern,
    s: Option<TermId>,
    p: Option<TermId>,
    o: Option<TermId>,
    reifies_id: Option<TermId>,
) -> Box<dyn Iterator<Item = QuadIds> + 'ds> {
    // Reifier layer: only when the predicate can be `rdf:reifies`. The object must also
    // be triple-term-shaped to be worth scanning — a quoted-triple pattern position
    // (`Pos::Triple`), a quoted-triple constant (`Pos::Bound` of a triple id), or a
    // free variable (`Pos::Slot`). A literal/IRI object constant can never be a triple
    // term, so the reifier scan is skipped. The residual `bind_row` enforces the exact
    // object match.
    let reifier_iter: Box<dyn Iterator<Item = QuadIds> + 'ds> = if let Some(reifies) = reifies_id {
        let predicate_can_reify = match &cp.p {
            Pos::Slot(_) => true,
            Pos::Bound(id) => *id == reifies,
            // A quoted triple is never a predicate position.
            Pos::Triple(_) => false,
        };
        if predicate_can_reify && object_can_be_triple_term(&cp.o, dataset) {
            Box::new(dataset.reifier_quads().filter(move |q| {
                s.is_none_or(|id| q.s == id)
                    && p.is_none_or(|id| q.p == id)
                    && o.is_none_or(|id| q.o == id)
            }))
        } else {
            Box::new(std::iter::empty())
        }
    } else {
        Box::new(std::iter::empty())
    };

    // Annotation layer: index by the bound reifier subject when possible, else scan.
    let annotation_iter: Box<dyn Iterator<Item = QuadIds> + 'ds> = match s {
        Some(reifier) => Box::new(
            dataset
                .annotations_of(reifier)
                .map(move |(pred, obj)| QuadIds {
                    s: reifier,
                    p: pred,
                    o: obj,
                    g: None,
                })
                .filter(move |q| {
                    s.is_none_or(|id| q.s == id)
                        && p.is_none_or(|id| q.p == id)
                        && o.is_none_or(|id| q.o == id)
                }),
        ),
        None => Box::new(dataset.annotation_quads().filter(move |q| {
            s.is_none_or(|id| q.s == id)
                && p.is_none_or(|id| q.p == id)
                && o.is_none_or(|id| q.o == id)
        })),
    };

    Box::new(reifier_iter.chain(annotation_iter))
}

/// Whether an object position could resolve to a quoted-triple term (so the reifier
/// layer — whose object is always a triple term — is worth scanning for it). An IRI or
/// literal constant never is.
fn object_can_be_triple_term(pos: &Pos, dataset: &RdfDataset) -> bool {
    match pos {
        // A free variable or a structural quoted-triple pattern can match a triple term.
        Pos::Slot(_) | Pos::Triple(_) => true,
        // A bound constant is worth scanning only if the constant is itself a triple
        // term; an IRI/literal/blank object can never match a reifier row.
        Pos::Bound(id) => matches!(dataset.resolve(*id), TermRef::Triple { .. }),
    }
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

    // ---- RDF 1.2 reification layer -----------------------------------------

    /// A dataset with one quoted statement `:alice :age 42` reified by `:r1`, which
    /// carries two annotations:
    ///   :r1 rdf:reifies <<( :alice :age 42 )>> .
    ///   :r1 :confidence "high" .
    ///   :r1 :source     :census .
    /// The reified statement itself is NOT asserted as a plain quad (the only quads
    /// table content is one unrelated `:bob :age 7` triple), proving the layer is read
    /// from the side-tables, not from `quads`.
    fn reified_graph() -> Arc<RdfDataset> {
        let mut b = RdfDatasetBuilder::new();
        let age = b.intern_iri("http://ex/age".to_owned());
        let alice = b.intern_iri("http://ex/alice".to_owned());
        let bob = b.intern_iri("http://ex/bob".to_owned());
        let forty_two = b.intern_literal(RdfLiteral::typed(
            "42",
            "http://www.w3.org/2001/XMLSchema#integer",
        ));
        let seven = b.intern_literal(RdfLiteral::typed(
            "7",
            "http://www.w3.org/2001/XMLSchema#integer",
        ));
        let statement = b.intern_triple(alice, age, forty_two);
        let r1 = b.intern_iri("http://ex/r1".to_owned());
        let reifies = b.intern_iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies".to_owned());
        let confidence = b.intern_iri("http://ex/confidence".to_owned());
        let source = b.intern_iri("http://ex/source".to_owned());
        let high = b.intern_literal(RdfLiteral::simple("high"));
        let census = b.intern_iri("http://ex/census".to_owned());

        // The interned `reifies` id is the virtual predicate; keep it referenced.
        let _ = reifies;
        // One unrelated asserted quad to prove the reified statement is NOT in `quads`.
        b.push_quad(bob, age, seven, None);
        b.push_reifier(r1, statement);
        b.push_annotation(r1, confidence, high);
        b.push_annotation(r1, source, census);
        b.freeze().expect("freeze")
    }

    fn int_val(lex: &str) -> Option<TermValue> {
        Some(TermValue::Literal {
            lexical_form: lex.to_owned(),
            datatype: "http://www.w3.org/2001/XMLSchema#integer".to_owned(),
            language: None,
            direction: None,
        })
    }

    fn str_val(lex: &str) -> Option<TermValue> {
        Some(TermValue::Literal {
            lexical_form: lex.to_owned(),
            datatype: "http://www.w3.org/2001/XMLSchema#string".to_owned(),
            language: None,
            direction: None,
        })
    }

    /// A predicate-position variable.
    fn pred_var(name: &str) -> NamedNodePattern {
        NamedNodePattern::Variable(Variable::new(name))
    }

    /// A nested quoted-triple object pattern `<<( s p o )>>`.
    fn triple_obj(s: TermPattern, p: NamedNodePattern, o: TermPattern) -> TermPattern {
        TermPattern::Triple(Box::new(TriplePattern {
            subject: s,
            predicate: p,
            object: o,
        }))
    }

    /// `?r rdf:reifies <<( ?s ?p ?o )>>` binds the reifier and the inner s/p/o from the
    /// reifier side-table — the reified statement is not in `quads`.
    #[test]
    fn reifies_pattern_binds_reifier_and_inner_variables() {
        let ds = reified_graph();
        let patterns = [triple(
            var_pos("r"),
            pred("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies"),
            triple_obj(var_pos("s"), pred_var("p"), var_pos("o")),
        )];
        let rows = run(&ds, &patterns, &["r", "s", "p", "o"]);
        assert_eq!(
            rows,
            vec![vec![
                iri_val("http://ex/r1"),
                iri_val("http://ex/alice"),
                iri_val("http://ex/age"),
                int_val("42"),
            ]]
        );
    }

    /// `?r rdf:reifies <<( :alice :age ?o )>>` — partially-ground inner pattern still
    /// binds `?r` and the free inner `?o`, and the ground inner positions filter.
    #[test]
    fn reifies_pattern_with_partly_ground_inner() {
        let ds = reified_graph();
        let patterns = [triple(
            var_pos("r"),
            pred("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies"),
            triple_obj(
                iri_pos("http://ex/alice"),
                pred("http://ex/age"),
                var_pos("o"),
            ),
        )];
        let rows = run(&ds, &patterns, &["r", "o"]);
        assert_eq!(rows, vec![vec![iri_val("http://ex/r1"), int_val("42")]]);
    }

    /// A non-matching ground inner position yields no rows (the statement is
    /// `:alice :age 42`, not `:alice :age 99`).
    #[test]
    fn reifies_pattern_inner_mismatch_is_empty() {
        let ds = reified_graph();
        let patterns = [triple(
            var_pos("r"),
            pred("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies"),
            triple_obj(
                iri_pos("http://ex/alice"),
                pred("http://ex/age"),
                TermPattern::Literal(Literal::new_typed(
                    "99",
                    NamedNode::new_unchecked("http://www.w3.org/2001/XMLSchema#integer"),
                )),
            ),
        )];
        let rows = run(&ds, &patterns, &["r"]);
        assert!(rows.is_empty());
    }

    /// A fully-open pattern `?r ?ap ?av` enumerates EVERY triple visible to the BGP:
    /// the one asserted quad, the virtual `rdf:reifies` edge, and both annotation rows.
    /// The reification layer is fully folded into ordinary BGP matching.
    #[test]
    fn open_pattern_enumerates_assertions_reifies_edge_and_annotations() {
        let ds = reified_graph();
        let patterns = [triple(var_pos("r"), pred_var("ap"), var_pos("av"))];
        let rows = run(&ds, &patterns, &["r", "ap", "av"]);
        let reifies = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
        // Rows are sorted by their Debug string for order-insensitive comparison, so
        // the expected order follows that key: bob < r1/confidence < r1/reifies <
        // r1/source.
        assert_eq!(
            rows,
            vec![
                // The asserted plain quad.
                vec![
                    iri_val("http://ex/bob"),
                    iri_val("http://ex/age"),
                    int_val("7"),
                ],
                // Annotation rows of :r1 (confidence, source sort before reifies under
                // the Debug-string key: "http://ex/…" < "http://www.…").
                vec![
                    iri_val("http://ex/r1"),
                    iri_val("http://ex/confidence"),
                    str_val("high"),
                ],
                vec![
                    iri_val("http://ex/r1"),
                    iri_val("http://ex/source"),
                    iri_val("http://ex/census"),
                ],
                // The virtual rdf:reifies edge (object is the quoted statement).
                vec![
                    iri_val("http://ex/r1"),
                    iri_val(reifies),
                    Some(TermValue::Triple {
                        s: Box::new(TermValue::Iri("http://ex/alice".to_owned())),
                        p: Box::new(TermValue::Iri("http://ex/age".to_owned())),
                        o: Box::new(TermValue::Literal {
                            lexical_form: "42".to_owned(),
                            datatype: "http://www.w3.org/2001/XMLSchema#integer".to_owned(),
                            language: None,
                            direction: None,
                        }),
                    }),
                ],
            ]
        );
    }

    /// An annotation pattern with a bound annotation predicate `?r :confidence ?v`
    /// binds only the annotation rows of that predicate (here, one).
    #[test]
    fn annotation_pattern_bound_predicate() {
        let ds = reified_graph();
        let patterns = [triple(
            var_pos("r"),
            pred("http://ex/confidence"),
            var_pos("v"),
        )];
        let rows = run(&ds, &patterns, &["r", "v"]);
        assert_eq!(rows, vec![vec![iri_val("http://ex/r1"), str_val("high")]]);
    }

    /// A bound-subject annotation pattern `:r1 :confidence ?v` indexes straight to the
    /// reifier's annotation run via `annotations_of`.
    #[test]
    fn annotation_pattern_bound_subject_indexes() {
        let ds = reified_graph();
        let patterns = [triple(
            iri_pos("http://ex/r1"),
            pred("http://ex/confidence"),
            var_pos("v"),
        )];
        let rows = run(&ds, &patterns, &["v"]);
        assert_eq!(rows, vec![vec![str_val("high")]]);
    }

    /// Joining the two layers: find the confidence of every age-statement reifier.
    /// `?r rdf:reifies <<( ?s :age ?age )>> . ?r :confidence ?c`
    #[test]
    fn join_reifier_to_its_annotation() {
        let ds = reified_graph();
        let patterns = [
            triple(
                var_pos("r"),
                pred("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies"),
                triple_obj(var_pos("s"), pred("http://ex/age"), var_pos("age")),
            ),
            triple(var_pos("r"), pred("http://ex/confidence"), var_pos("c")),
        ];
        let rows = run(&ds, &patterns, &["s", "age", "c"]);
        assert_eq!(
            rows,
            vec![vec![
                iri_val("http://ex/alice"),
                int_val("42"),
                str_val("high"),
            ]]
        );
    }

    /// A repeated inner variable `<<( ?x :age ?x )>>` enforces consistency: the only
    /// reified statement is `:alice :age 42`, where subject ≠ object, so it is rejected.
    #[test]
    fn reifies_pattern_repeated_inner_variable_enforced() {
        let ds = reified_graph();
        let patterns = [triple(
            var_pos("r"),
            pred("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies"),
            triple_obj(var_pos("x"), pred("http://ex/age"), var_pos("x")),
        )];
        let rows = run(&ds, &patterns, &["r", "x"]);
        assert!(rows.is_empty());
    }

    /// A dataset with no reifiers never interns `rdf:reifies`, so a reifies-pattern
    /// query returns empty without panicking (the `None` reifies-id branch).
    #[test]
    fn reifies_pattern_on_plain_graph_is_empty() {
        let ds = social_graph();
        let patterns = [triple(
            var_pos("r"),
            pred("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies"),
            triple_obj(var_pos("s"), pred_var("p"), var_pos("o")),
        )];
        let rows = run(&ds, &patterns, &["r"]);
        assert!(rows.is_empty());
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

    /// A ≥3-pattern STAR shape: one shared hub variable (?hub = col 0) appears in
    /// every spoke. Guards two invariants simultaneously:
    ///
    /// 1. **Connectivity**: after the seed pattern binds `?hub`, every remaining spoke
    ///    shares col 0 — so `pattern_connected` returns `true` for all of them and no
    ///    Cartesian product is ever forced.
    /// 2. **Score-then-index ordering among connected spokes**: once all spokes are
    ///    connected the algorithm still picks the highest-scoring eligible pattern,
    ///    breaking ties by lowest original index.
    ///
    /// Pattern layout (col 0 = ?hub, cols 1–4 are distinct leaf slots):
    ///   P0 (index 0): Bound Bound Slot(0)   — seed; score 2 in round 1 (two constants)
    ///   P1 (index 1): Slot(0) Bound Slot(1) — spoke A; score 2 after hub bound
    ///   P2 (index 2): Slot(0) Slot(2) Slot(3) — spoke B; score 1 after hub bound
    ///   P3 (index 3): Slot(0) Bound Slot(4) — spoke C; score 2 after hub bound
    ///
    /// Hand-simulated rounds:
    ///   Round 1: nothing bound; scores P0=2, P1=1, P2=0, P3=1 → P0 wins; binds col 0.
    ///   Round 2: col 0 bound; all of P1/P2/P3 connected; scores P1=2, P2=1, P3=2
    ///            → P1 and P3 tie at 2; lowest index → P1 wins; binds col 0,1.
    ///   Round 3: cols 0,1 bound; P2 and P3 connected; scores P2=1, P3=2 → P3 wins;
    ///            binds cols 0,1,4.
    ///   Round 4: only P2 remains, connected → P2 scheduled.
    ///   Expected order: [0, 1, 3, 2].
    #[test]
    fn star_bgp_schedules_spokes_connected_after_hub() {
        // P0: seed — two ground constants anchor ?hub (col 0) in the object position.
        let p0 = cp(Pos::Bound(tid(10)), Pos::Bound(tid(11)), Pos::Slot(0));
        // P1: spoke A — hub in subject, one ground predicate, leaf in object (col 1).
        let p1 = cp(Pos::Slot(0), Pos::Bound(tid(12)), Pos::Slot(1));
        // P2: spoke B — hub in subject, two free leaf slots (cols 2, 3); lowest score.
        let p2 = cp(Pos::Slot(0), Pos::Slot(2), Pos::Slot(3));
        // P3: spoke C — hub in subject, one ground predicate, leaf in object (col 4).
        let p3 = cp(Pos::Slot(0), Pos::Bound(tid(13)), Pos::Slot(4));

        let order = selectivity_order(&[p0, p1, p2, p3]);

        // The unique deterministic output derived by hand-simulation above.
        assert_eq!(order, vec![0, 1, 3, 2]);

        // Connectivity invariant: every pattern scheduled after the seed (position 0)
        // must share the hub column (col 0), so no Cartesian product is forced.
        let pos_of = |i: usize| order.iter().position(|&x| x == i).unwrap();
        assert!(pos_of(0) == 0, "P0 is the seed");
        // P1, P2, P3 all share col 0 with P0 — assert each follows the seed.
        assert!(pos_of(1) > pos_of(0), "P1 (spoke A) follows the seed");
        assert!(pos_of(2) > pos_of(0), "P2 (spoke B) follows the seed");
        assert!(pos_of(3) > pos_of(0), "P3 (spoke C) follows the seed");
        // Score ordering among connected spokes: P3 (score 2) before P2 (score 1).
        assert!(
            pos_of(3) < pos_of(2),
            "higher-scoring spoke P3 precedes lower-scoring P2"
        );
        // Tie-break: P1 and P3 both score 2 as connected; P1 (index 1) precedes P3 (index 3).
        assert!(
            pos_of(1) < pos_of(3),
            "lower-index P1 beats equal-scoring P3 in the tie"
        );
    }
}
