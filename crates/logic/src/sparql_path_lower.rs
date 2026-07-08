// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Lower a SPARQL property path to a `.logic` Datalog program and resolve it with
//! the native magic-sets engine (`crate::physical::resolve_native`).
//!
//! This is the **lowered** twin of the in-engine evaluator in
//! `gmeow-sparql-eval::path`. The acceptance criterion for is *parity*: the
//! two implementations must agree on every property-path query in the corpus
//! (validated by `crates/logic/tests/sparql_path_parity.rs`).
//!
//! # Engine
//!
//! The lowered programs are binary positive-Horn transitive closure — exactly the
//! fragment the native magic-sets evaluator decides completely (it tables inherently
//! and saturates the finite Herbrand base of a ground edge set, so a cyclic graph
//! terminates without any explicit tabling hint). Resolution therefore runs through
//! [`crate::physical::resolve_native`], which is **authoritative** for this fragment:
//! a lowered program that came back `Unsupported` would be a contract violation, so
//! that case is an honest hard error here — there is no silent fall-back to another
//! engine. The embedded Scryer tabling engine is retained purely as the *per-consumer
//! parity comparand* (see the `#[cfg(test)]` parity ledger below): native ≡ Scryer,
//! gap-zero, over the property-path operator corpus.
//!
//! # Coverage boundary (unchanged)
//!
//! `NegatedPropertySet` and `Wildcard` use a *variable predicate position* that the
//! fixed-predicate `.logic` grammar cannot express, and `{0,0}` has no positive
//! relation to query; those hard-fail below and must be answered by the in-engine
//! evaluator. This is an honest capability boundary, not a degraded fallback.
//!
//! # Why Datalog
//!
//! Per `slices/grounding/logic/design/LOGIC-PATHS.md`, the canonical runtime for path
//! recursion is the native least-model fixpoint engine: transitive closure
//! (`p+`/`p*`) and the stratified unroll of bounded `{n,m}` are ordinary Datalog. We
//! reuse the `ScryerForeign` seam exactly — the path's edges are loaded into a world
//! (a native named graph) and snapshotted as ground facts by the native resolver; the
//! lowered [`QProgram`] carries only the IDB rules + goal.
//!
//! # Positive relation + reflexivity
//!
//! Each operator lowers to a binary IDB predicate capturing its **positive**
//! (one-or-more, non-zero-length) relation via pure transitive-closure rules, plus
//! a `reflexive` flag tracking whether the path admits the zero-length identity
//! (`?`/`*`/`{0,…}`, composed: `Sequence` = AND, `Alternative` = OR). The
//! zero-length identity pairs are added in Rust relative to the *bound* endpoint —
//! so the Datalog program stays pure transitive closure and never needs to
//! enumerate the node universe (the both-variable case enumerates it from the
//! edge set directly).
//!
//! # Coverage
//!
//! Lowers `NamedNode`, `Reverse`, `Sequence`, `Alternative`, `ZeroOrOne`,
//! `ZeroOrMore`, `OneOrMore`, and `Range{n,m}`. `NegatedPropertySet` and `Wildcard`
//! use a *variable predicate position* that the fixed-predicate `.logic` grammar
//! cannot express; they hard-fail here (evaluate them with the in-engine
//! evaluator). This is an honest capability boundary, not a degraded fallback.

use std::collections::BTreeSet;

use purrdf::sparql::PropertyPathExpression;

use crate::physical::{NativeOutcome, resolve_native};
use crate::query_ir::{AnswerSet, Budget, QAtom, QBodyLit, QGoal, QProgram, QRule, QTerm};
use crate::seam::{BudgetStatus, WorldStoreForeign};
use crate::store::WorldStore;

/// The internal world IRI the path's edges are loaded into.
const LOWER_WORLD: &str = "urn:gmeow:sparql-path:world";
/// The (positive Horn) profile the lowered Datalog program runs under.
const LOWER_PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";
/// IRI prefix for the synthetic IDB predicates the lowering mints.
const IDB_PREFIX: &str = "urn:gmeow:sparql-path:p";

/// A path endpoint in a lowered query: a bound IRI (bare, no angle brackets) or a
/// free variable.
#[derive(Debug, Clone)]
pub enum PathEnd {
    /// A free variable endpoint.
    Variable,
    /// A ground IRI endpoint (bare IRI string, no angle brackets).
    Iri(String),
}

/// Evaluate `path` between `subject` and `object` over the ground `edges`
/// (`(subject_iri, predicate_iri, object_iri)` triples) using the native magic-sets
/// engine, returning the set of `(subject, object)` pairs in canonical
/// `term_n3` form (`<iri>`).
///
/// # Errors
///
/// - The path contains a `NegatedPropertySet`/`Wildcard` (not lowerable).
/// - The native resolver declares the lowered program `Unsupported` — a contract
///   violation for the positive-Horn transitive-closure fragment, treated as a hard
///   error rather than a silent fall-back to another engine.
/// - The native resolution does not complete within budget (`BudgetStatus` other than
///   `Ok` — a sound-but-incomplete answer set would make a parity claim against the
///   complete in-engine evaluator unsound).
pub fn evaluate_path_lowered(
    edges: &[(String, String, String)],
    path: &PropertyPathExpression,
    subject: &PathEnd,
    object: &PathEnd,
) -> Result<BTreeSet<(String, String)>, String> {
    let mut low = Lowering::new();
    let (result_pred, reflexive) = low.lower(path)?;

    let goal = QGoal {
        atoms: vec![QAtom {
            pred: result_pred,
            args: vec![end_term(subject, "S"), end_term(object, "O")],
        }],
    };
    let program = QProgram {
        rules: low.rules,
        goal,
        counterfactual: None,
        prob_facts: Vec::new(),
        prob_model: None,
        confidences: Vec::new(),
    };

    // Load the edges into a world and wrap it with the ScryerForeign seam — the EDB
    // is a world snapshot (the native resolver pulls facts from `foreign.in_world`),
    // NOT the program (which carries only the IDB rules + goal).
    let store = WorldStore::new();
    for (s, p, o) in edges {
        store.insert_quad(LOWER_WORLD, s, p, o);
    }
    let foreign = WorldStoreForeign::from_world(&store, LOWER_WORLD, LOWER_PROFILE)?;

    // Resolve on the native magic-sets engine. Native tables inherently, so the lowered
    // transitive-closure fragment needs no tabling hint. The engine is authoritative for
    // this positive-Horn fragment: an `Unsupported` outcome is a contract violation, not
    // a cue to fall back to another engine.
    let ans = match resolve_native(&foreign, LOWER_WORLD, &program, &Budget::default())? {
        NativeOutcome::Decided(ans) => ans,
        NativeOutcome::Unsupported(kind) => {
            return Err(format!(
                "lowered path resolution is not natively decided: Unsupported({kind:?})"
            ));
        }
    };
    if ans.status != BudgetStatus::Ok {
        return Err(format!(
            "lowered path resolution did not complete (status = {:?}); a partial answer set \
             cannot be compared against the complete in-engine evaluator",
            ans.status
        ));
    }

    let mut pairs = answer_to_pairs(&ans, subject, object);
    add_reflexive(&mut pairs, reflexive, subject, object, edges);
    Ok(pairs)
}

/// Map an [`AnswerSet`] to `(subject, object)` `term_n3` pairs, honouring which
/// endpoints were ground (their `<iri>` is fixed) versus variable (read from the
/// binding keyed by `"S"`/`"O"`).
fn answer_to_pairs(
    ans: &AnswerSet,
    subject: &PathEnd,
    object: &PathEnd,
) -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    for b in &ans.bindings {
        let s = match subject {
            PathEnd::Iri(iri) => n3(iri),
            PathEnd::Variable => match b.get("S") {
                Some(v) => v.clone(),
                None => continue,
            },
        };
        let o = match object {
            PathEnd::Iri(iri) => n3(iri),
            PathEnd::Variable => match b.get("O") {
                Some(v) => v.clone(),
                None => continue,
            },
        };
        out.insert((s, o));
    }
    out
}

/// Add the zero-length identity pairs when the path is reflexive, relative to the
/// bound endpoint (so no node-universe enumeration is needed unless both endpoints
/// are variable).
///
/// A ground endpoint that is **absent from the graph** (i.e. does not appear as a
/// subject or object of any edge) mirrors the in-engine evaluator's `resolve_end`
/// behaviour: an absent ground IRI makes the whole path empty (SPARQL §18.3.2.1 —
/// a term not in the active graph cannot contribute any ALP pair, including the
/// zero-length identity). We enforce this by checking node-universe membership for
/// every bound (IRI) endpoint before inserting the identity pair.
fn add_reflexive(
    out: &mut BTreeSet<(String, String)>,
    reflexive: bool,
    subject: &PathEnd,
    object: &PathEnd,
    edges: &[(String, String, String)],
) {
    if !reflexive {
        return;
    }
    let universe = node_universe(edges);
    match (subject, object) {
        (PathEnd::Iri(s), PathEnd::Variable) => {
            // Only insert the identity if the subject node exists in the graph.
            if universe.contains(s) {
                out.insert((n3(s), n3(s)));
            }
        }
        (PathEnd::Variable, PathEnd::Iri(o)) => {
            // Only insert the identity if the object node exists in the graph.
            if universe.contains(o) {
                out.insert((n3(o), n3(o)));
            }
        }
        (PathEnd::Iri(s), PathEnd::Iri(o)) => {
            // Identity holds only when s == o AND the node exists in the graph.
            if s == o && universe.contains(s) {
                out.insert((n3(s), n3(o)));
            }
        }
        (PathEnd::Variable, PathEnd::Variable) => {
            for n in &universe {
                out.insert((n3(n), n3(n)));
            }
        }
    }
}

/// The subjects and objects of `edges` — the node universe for a both-variable
/// reflexive path (matches the in-engine evaluator's node universe).
fn node_universe(edges: &[(String, String, String)]) -> BTreeSet<String> {
    let mut u = BTreeSet::new();
    for (s, _, o) in edges {
        u.insert(s.clone());
        u.insert(o.clone());
    }
    u
}

/// The canonical `term_n3` form of an IRI: `<iri>`.
fn n3(iri: &str) -> String {
    format!("<{iri}>")
}

/// The goal argument term for an endpoint: a bound `<iri>` constant or the named
/// variable.
fn end_term(end: &PathEnd, var: &str) -> QTerm {
    match end {
        PathEnd::Variable => QTerm::Var(var.to_owned()),
        PathEnd::Iri(iri) => QTerm::Const(n3(iri)),
    }
}

/// Accumulates the IDB rules while lowering a path expression.
struct Lowering {
    rules: Vec<QRule>,
    counter: usize,
}

impl Lowering {
    fn new() -> Self {
        Self {
            rules: Vec::new(),
            counter: 0,
        }
    }

    /// Mint a fresh synthetic IDB predicate IRI.
    fn fresh(&mut self) -> String {
        let n = self.counter;
        self.counter += 1;
        format!("{IDB_PREFIX}{n}")
    }

    /// A binary atom `pred(a, b)` over two variables.
    fn atom(pred: &str, a: &str, b: &str) -> QAtom {
        QAtom {
            pred: pred.to_owned(),
            args: vec![QTerm::Var(a.to_owned()), QTerm::Var(b.to_owned())],
        }
    }

    /// Push a rule `head :- body...` (all body literals are atoms).
    fn push_rule(&mut self, head: QAtom, body: Vec<QAtom>) {
        self.rules.push(QRule {
            head,
            body: body.into_iter().map(QBodyLit::Atom).collect(),
        });
    }

    /// Lower a sub-path to `(predicate, reflexive)`: the predicate names the path's
    /// **positive** (one-or-more, non-zero-length) binary relation; `reflexive` is
    /// whether the path admits the zero-length identity.
    fn lower(&mut self, path: &PropertyPathExpression) -> Result<(String, bool), String> {
        use PropertyPathExpression as P;
        match path {
            // The EDB predicate is the relation directly; not reflexive.
            P::NamedNode(p) => Ok((p.as_str().to_owned(), false)),
            P::Reverse(inner) => {
                let (pp, r) = self.lower(inner)?;
                let f = self.fresh();
                self.push_rule(Self::atom(&f, "X", "Y"), vec![Self::atom(&pp, "Y", "X")]);
                Ok((f, r))
            }
            P::Sequence(a, b) => {
                let (pa, ra) = self.lower(a)?;
                let (pb, rb) = self.lower(b)?;
                let f = self.fresh();
                self.push_rule(
                    Self::atom(&f, "X", "Z"),
                    vec![Self::atom(&pa, "X", "Y"), Self::atom(&pb, "Y", "Z")],
                );
                // If `a` can be zero-length, the sequence can skip it (`b` alone).
                if ra {
                    self.push_rule(Self::atom(&f, "X", "Z"), vec![Self::atom(&pb, "X", "Z")]);
                }
                // If `b` can be zero-length, the sequence can skip it (`a` alone).
                if rb {
                    self.push_rule(Self::atom(&f, "X", "Z"), vec![Self::atom(&pa, "X", "Z")]);
                }
                Ok((f, ra && rb))
            }
            P::Alternative(a, b) => {
                let (pa, ra) = self.lower(a)?;
                let (pb, rb) = self.lower(b)?;
                let f = self.fresh();
                self.push_rule(Self::atom(&f, "X", "Y"), vec![Self::atom(&pa, "X", "Y")]);
                self.push_rule(Self::atom(&f, "X", "Y"), vec![Self::atom(&pb, "X", "Y")]);
                Ok((f, ra || rb))
            }
            // `p?` shares `p`'s positive relation; it is reflexive.
            P::ZeroOrOne(inner) => {
                let (pp, _) = self.lower(inner)?;
                Ok((pp, true))
            }
            // `p*` = id ∪ TC(positive(p)); reflexive.
            P::ZeroOrMore(inner) => {
                let (pp, _) = self.lower(inner)?;
                let tc = self.transitive_closure(&pp);
                Ok((tc, true))
            }
            // `p+` = TC(positive(p)) ∪ (id iff p is reflexive).
            P::OneOrMore(inner) => {
                let (pp, rp) = self.lower(inner)?;
                let tc = self.transitive_closure(&pp);
                Ok((tc, rp))
            }
            P::Range { inner, min, max } => self.lower_range(inner, *min, *max),
            P::NegatedPropertySet(_) | P::Wildcard { .. } => Err(
                "negated-property-set and wildcard paths use a variable predicate position and \
                 cannot be lowered to the fixed-predicate Datalog engine; evaluate them with the \
                 in-engine path evaluator instead"
                    .to_owned(),
            ),
        }
    }

    /// Build the transitive closure `tc/2` of `pp/2`:
    /// `tc(X,Y):-pp(X,Y).  tc(X,Z):-pp(X,Y),tc(Y,Z).` The native magic-sets engine
    /// tables recursive predicates inherently and saturates the finite Herbrand base,
    /// so this recursion is cycle-safe without any explicit tabling directive.
    fn transitive_closure(&mut self, pp: &str) -> String {
        let tc = self.fresh();
        self.push_rule(Self::atom(&tc, "X", "Y"), vec![Self::atom(pp, "X", "Y")]);
        self.push_rule(
            Self::atom(&tc, "X", "Z"),
            vec![Self::atom(pp, "X", "Y"), Self::atom(&tc, "Y", "Z")],
        );
        tc
    }

    /// Lower `inner{min,max}` via **iterative O(max) rule emission** (LOGIC-PATHS
    /// canon, lines 87-97): instead of building an O(max²) AST of cloned
    /// `PropertyPathExpression` nodes, lower `inner` ONCE and emit a flat chain of
    /// per-level composition rules.
    ///
    /// The chain is:
    /// ```text
    /// step_1(X,Z) :- pp(X,Z).
    /// step_2(X,Z) :- step_1(X,Y), pp(Y,Z).
    /// …
    /// step_m(X,Z) :- step_{m-1}(X,Y), pp(Y,Z).
    /// result(X,Y) :- step_k(X,Y).   % for each k in the in-range window
    /// ```
    ///
    /// Reflexive-inner semantics: if `inner` is itself reflexive (e.g. `p?`, `p*`,
    /// or a composed reflexive path), then each application of `inner` may take the
    /// zero-length identity, so `k` applications of a reflexive `inner` reach nodes
    /// at *at most k* positive hops (any application can "idle" via the identity
    /// leg). Consequently `{min,max}` of a reflexive inner is equivalent to
    /// "at most max positive hops, always reflexive" — we union steps 1..=m and
    /// return `reflexive = true`. For a non-reflexive inner, only the steps
    /// `max(min,1)..=m` are in-range, and `reflexive = (min == 0)`.
    fn lower_range(
        &mut self,
        inner: &PropertyPathExpression,
        min: u32,
        max: Option<u32>,
    ) -> Result<(String, bool), String> {
        match max {
            // {0,0} is the zero-length-only identity — no positive relation to query.
            Some(0) => Err(
                "zero-length-only range path {0,0} has no positive relation to lower; evaluate it \
                 with the in-engine evaluator"
                    .to_owned(),
            ),
            Some(m) => {
                // Lower inner once to its positive predicate.
                let (pp, refl) = self.lower(inner)?;

                // Build the iterative step chain: step_1 = pp alias,
                // step_i(X,Z) :- step_{i-1}(X,Y), pp(Y,Z).
                let step_1 = self.fresh();
                self.push_rule(
                    Self::atom(&step_1, "X", "Z"),
                    vec![Self::atom(&pp, "X", "Z")],
                );
                let mut steps: Vec<String> = vec![step_1];
                for _i in 2..=m {
                    let prev = steps.last().unwrap().clone();
                    let next = self.fresh();
                    self.push_rule(
                        Self::atom(&next, "X", "Z"),
                        vec![Self::atom(&prev, "X", "Y"), Self::atom(&pp, "Y", "Z")],
                    );
                    steps.push(next);
                }

                // Determine the in-range window and final reflexivity.
                // If inner is reflexive, each step application can idle via the identity
                // leg, so step_k reaches nodes at *at most k* positive hops. Unioning
                // all k in 1..=m covers "at most max" and the result is always
                // reflexive (identity is reachable by idling all m steps).
                let (start_k, reflexive) = if refl {
                    (1usize, true)
                } else {
                    (min.max(1) as usize, min == 0)
                };

                // Mint the result predicate and union the in-range step levels.
                let result = self.fresh();
                for step in steps.iter().skip(start_k - 1) {
                    self.push_rule(
                        Self::atom(&result, "X", "Y"),
                        vec![Self::atom(step, "X", "Y")],
                    );
                }
                Ok((result, reflexive))
            }
            None if min == 0 => {
                // {0,} ≡ inner* : reflexive transitive closure.
                let (pp, _) = self.lower(inner)?;
                let tc = self.transitive_closure(&pp);
                Ok((tc, true))
            }
            None => {
                // {n,} ≡ at least n applications: iterative prefix of n steps,
                // then zero-or-more additional applications via transitive closure.
                let (pp, refl) = self.lower(inner)?;

                // Reflexive inner: each of the n required applications can idle via the
                // identity leg, so `inner{n,}` collapses to `inner*` (reflexive
                // transitive closure: 0-or-more positive hops). The in-engine
                // `range_reach` reaches fewer-than-n-hop nodes the same way (each
                // `reach(inner, ·)` includes identity), so parity demands the collapse —
                // the step-min prefix would otherwise miss them.
                if refl {
                    let tc = self.transitive_closure(&pp);
                    return Ok((tc, true));
                }

                // Build the iterative step chain up to step_min.
                let step_1 = self.fresh();
                self.push_rule(
                    Self::atom(&step_1, "X", "Z"),
                    vec![Self::atom(&pp, "X", "Z")],
                );
                let mut steps: Vec<String> = vec![step_1];
                for _i in 2..=min {
                    let prev = steps.last().unwrap().clone();
                    let next = self.fresh();
                    self.push_rule(
                        Self::atom(&next, "X", "Z"),
                        vec![Self::atom(&prev, "X", "Y"), Self::atom(&pp, "Y", "Z")],
                    );
                    steps.push(next);
                }
                let step_min = steps.last().unwrap().clone();

                // Transitive closure of pp for the zero-or-more tail.
                let tc = self.transitive_closure(&pp);

                // result(X,Y) :- step_min(X,Y).          [exactly min hops]
                // result(X,Z) :- step_min(X,Y), tc(Y,Z). [min + 1 or more additional hops]
                let result = self.fresh();
                self.push_rule(
                    Self::atom(&result, "X", "Y"),
                    vec![Self::atom(&step_min, "X", "Y")],
                );
                self.push_rule(
                    Self::atom(&result, "X", "Z"),
                    vec![Self::atom(&step_min, "X", "Y"), Self::atom(&tc, "Y", "Z")],
                );
                // Non-reflexive inner: `refl` is false here (the reflexive case
                // returned above), so the range is non-reflexive.
                Ok((result, false))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Paths are built from the algebra's `NamedNode`.
    use purrdf::sparql::NamedNode as AlgNamedNode;

    const EX: &str = "https://example.org/";

    /// A set of `(subject, object)` `term_n3` pairs — one engine's answer to a lowered
    /// path query.
    type PairSet = BTreeSet<(String, String)>;

    fn iri(local: &str) -> String {
        format!("{EX}{local}")
    }

    fn edge(s: &str, p: &str, o: &str) -> (String, String, String) {
        (iri(s), iri(p), iri(o))
    }

    fn named(local: &str) -> PropertyPathExpression {
        PropertyPathExpression::NamedNode(AlgNamedNode::new_unchecked(iri(local)))
    }

    /// Forward objects reachable from a bound subject, as local names, sorted.
    fn forward(
        edges: &[(String, String, String)],
        subj: &str,
        path: &PropertyPathExpression,
    ) -> Vec<String> {
        let pairs =
            evaluate_path_lowered(edges, path, &PathEnd::Iri(iri(subj)), &PathEnd::Variable)
                .expect("lowered eval");
        let mut v: Vec<String> = pairs
            .into_iter()
            .map(|(_, o)| o.trim_start_matches('<').trim_end_matches('>').to_owned())
            .map(|o| o.strip_prefix(EX).unwrap_or(&o).to_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn named_predicate_forward() {
        let edges = [edge("a", "p", "b"), edge("a", "p", "c")];
        assert_eq!(forward(&edges, "a", &named("p")), vec!["b", "c"]);
    }

    #[test]
    fn one_or_more_transitive_closure() {
        let edges = [
            edge("a", "p", "b"),
            edge("b", "p", "c"),
            edge("c", "p", "d"),
        ];
        let plus = PropertyPathExpression::OneOrMore(Box::new(named("p")));
        assert_eq!(forward(&edges, "a", &plus), vec!["b", "c", "d"]);
    }

    #[test]
    fn zero_or_more_includes_self() {
        let edges = [edge("a", "p", "b"), edge("b", "p", "c")];
        let star = PropertyPathExpression::ZeroOrMore(Box::new(named("p")));
        // a (zero-length) + b, c (transitive).
        assert_eq!(forward(&edges, "a", &star), vec!["a", "b", "c"]);
    }

    #[test]
    fn transitive_closure_terminates_on_cycle() {
        let edges = [
            edge("a", "p", "b"),
            edge("b", "p", "c"),
            edge("c", "p", "a"),
        ];
        let plus = PropertyPathExpression::OneOrMore(Box::new(named("p")));
        // Cyclic: a reaches a, b, c (a via the cycle).
        assert_eq!(forward(&edges, "a", &plus), vec!["a", "b", "c"]);
    }

    #[test]
    fn sequence_and_alternative() {
        let edges = [
            edge("a", "p", "x"),
            edge("x", "q", "b"),
            edge("a", "r", "c"),
        ];
        let seq = PropertyPathExpression::Sequence(Box::new(named("p")), Box::new(named("q")));
        assert_eq!(forward(&edges, "a", &seq), vec!["b"]);
        let alt = PropertyPathExpression::Alternative(Box::new(seq), Box::new(named("r")));
        assert_eq!(forward(&edges, "a", &alt), vec!["b", "c"]);
    }

    #[test]
    fn reverse_walks_backward() {
        let edges = [edge("a", "p", "b")];
        let rev = PropertyPathExpression::Reverse(Box::new(named("p")));
        // :b ^:p ?o → a.
        assert_eq!(forward(&edges, "b", &rev), vec!["a"]);
    }

    #[test]
    fn range_bounded_unroll() {
        let edges = [
            edge("a", "p", "b"),
            edge("b", "p", "c"),
            edge("c", "p", "d"),
            edge("d", "p", "e"),
        ];
        let rng = |min, max| PropertyPathExpression::Range {
            inner: Box::new(named("p")),
            min,
            max,
        };
        assert_eq!(forward(&edges, "a", &rng(2, Some(2))), vec!["c"]);
        assert_eq!(forward(&edges, "a", &rng(0, Some(2))), vec!["a", "b", "c"]);
        assert_eq!(forward(&edges, "a", &rng(2, None)), vec!["c", "d", "e"]);
    }

    #[test]
    fn range_unbounded_reflexive_inner_collapses_to_star() {
        // `(:p?){2,}` — a REFLEXIVE inner with an unbounded lower bound. Each of the
        // (>=2) required applications can idle via `?`'s identity leg, so the path
        // collapses to `:p*` = the reflexive transitive closure (0-or-more positive
        // hops), reaching {a, b, c} from `a` over a->b->c — NOT just the >=2-hop nodes.
        // Before the reflexive short-circuit this returned only {a, c} (step_min was
        // p^2 = {c}, plus reflexive self {a}), diverging from the in-engine
        // `range_reach` which reaches `b` via an idled leg. The short-circuit fixes it.
        let edges = [edge("a", "p", "b"), edge("b", "p", "c")];
        let inner = PropertyPathExpression::ZeroOrOne(Box::new(named("p")));
        let rng = PropertyPathExpression::Range {
            inner: Box::new(inner),
            min: 2,
            max: None,
        };
        assert_eq!(forward(&edges, "a", &rng), vec!["a", "b", "c"]);
    }

    #[test]
    fn both_ground_membership() {
        let edges = [edge("a", "p", "b"), edge("b", "p", "c")];
        let plus = PropertyPathExpression::OneOrMore(Box::new(named("p")));
        let hit = evaluate_path_lowered(
            &edges,
            &plus,
            &PathEnd::Iri(iri("a")),
            &PathEnd::Iri(iri("c")),
        )
        .expect("eval");
        assert_eq!(hit.len(), 1, "a p+ c holds");
        let miss = evaluate_path_lowered(
            &edges,
            &plus,
            &PathEnd::Iri(iri("a")),
            &PathEnd::Iri(iri("a")),
        )
        .expect("eval");
        assert!(miss.is_empty(), "a p+ a does not hold (acyclic)");
    }

    #[test]
    fn negated_and_wildcard_are_not_lowerable() {
        let edges = [edge("a", "p", "b")];
        let neg =
            PropertyPathExpression::NegatedPropertySet(vec![purrdf::sparql::NegatedPathElement {
                predicate: AlgNamedNode::new_unchecked(iri("p")),
                inverse: false,
            }]);
        assert!(
            evaluate_path_lowered(&edges, &neg, &PathEnd::Iri(iri("a")), &PathEnd::Variable)
                .is_err()
        );
        let wild = PropertyPathExpression::Wildcard { namespace: None };
        assert!(
            evaluate_path_lowered(&edges, &wild, &PathEnd::Iri(iri("a")), &PathEnd::Variable)
                .is_err()
        );
    }

    // ── Per-consumer parity ledger ───────────────────────────────────────────────
    //
    // The production `evaluate_path_lowered` now resolves the lowered transitive-closure
    // program on the NATIVE magic-sets engine (`resolve_native`). This ledger proves the
    // promotion is gap-zero: for every lowerable operator, the answer set the native
    // engine returns over the SAME lowered `QProgram` equals the answer set the retained
    // Scryer tabling engine returns. Scryer is the comparand only — it is no longer on
    // the production path. A cyclic edge graph is included so the recursive fragment is
    // exercised; the Scryer comparand is fed the program's cyclic IDB predicates as
    // `:- table` directives (via `dispatch::cyclic_predicates`) so it terminates, exactly
    // as the production dispatch fallback would have.

    /// Lower `path` between `subject`/`object` to its `QProgram`, returning the program
    /// and its reflexive flag — the raw material both engines resolve identically.
    fn lower_to_program(
        path: &PropertyPathExpression,
        subject: &PathEnd,
        object: &PathEnd,
    ) -> (QProgram, bool) {
        let mut low = Lowering::new();
        let (result_pred, reflexive) = low.lower(path).expect("path is lowerable");
        let goal = QGoal {
            atoms: vec![QAtom {
                pred: result_pred,
                args: vec![end_term(subject, "S"), end_term(object, "O")],
            }],
        };
        let program = QProgram {
            rules: low.rules,
            goal,
            counterfactual: None,
            prob_facts: Vec::new(),
            prob_model: None,
            confidences: Vec::new(),
        };
        (program, reflexive)
    }

    /// Resolve the SAME lowered program on both the native engine and Scryer over the
    /// given edge graph, returning `(native_pairs, scryer_pairs)` after applying the
    /// identical Rust-side reflexive-identity augmentation to each. Equality of the two
    /// `BTreeSet`s is the gap-zero parity claim.
    fn native_and_scryer_pairs(
        edges: &[(String, String, String)],
        path: &PropertyPathExpression,
        subject: &PathEnd,
        object: &PathEnd,
    ) -> (PairSet, PairSet) {
        let (program, reflexive) = lower_to_program(path, subject, object);

        let store = WorldStore::new();
        for (s, p, o) in edges {
            store.insert_quad(LOWER_WORLD, s, p, o);
        }
        let foreign =
            WorldStoreForeign::from_world(&store, LOWER_WORLD, LOWER_PROFILE).expect("from_world");

        // Native magic-sets — authoritative for the positive-Horn fragment.
        let native_ans = match resolve_native(&foreign, LOWER_WORLD, &program, &Budget::default())
            .expect("native resolve")
        {
            NativeOutcome::Decided(a) => a,
            NativeOutcome::Unsupported(k) => {
                panic!(
                    "native must decide the lowered positive-Horn program, got Unsupported({k:?})"
                )
            }
        };
        assert_eq!(
            native_ans.status,
            BudgetStatus::Ok,
            "native run must be complete"
        );
        let mut native_pairs = answer_to_pairs(&native_ans, subject, object);
        add_reflexive(&mut native_pairs, reflexive, subject, object, edges);

        // Scryer comparand — self-tabled over the program's cyclic IDB predicates so the
        // recursive fragment terminates, mirroring the production dispatch fallback.
        let table_preds = crate::dispatch::cyclic_predicates(&program);
        let scryer_ans = crate::scryer_engine::run_scryer(
            &foreign,
            LOWER_WORLD,
            &program,
            &table_preds,
            &Budget::default(),
        )
        .expect("scryer resolve");
        assert_eq!(
            scryer_ans.status,
            BudgetStatus::Ok,
            "scryer run must be complete"
        );
        let mut scryer_pairs = answer_to_pairs(&scryer_ans, subject, object);
        add_reflexive(&mut scryer_pairs, reflexive, subject, object, edges);

        (native_pairs, scryer_pairs)
    }

    #[test]
    fn scryer_parity_native_lowered_matches_scryer_over_operator_corpus() {
        // Acyclic chain a -> b -> c -> d and a cyclic graph a -> b -> c -> a. Every
        // lowerable operator is resolved on BOTH graphs (the cyclic one exercises the
        // recursive `+`/`*`/`{n,}` fixpoint under tabling).
        let acyclic = [
            edge("a", "p", "b"),
            edge("b", "p", "c"),
            edge("c", "p", "d"),
            edge("a", "q", "m"),
            edge("m", "r", "z"),
        ];
        // The cyclic graph carries the p-cycle plus q/r edges so every predicate the
        // corpus references exists in the snapshot: the native engine treats an absent
        // EDB predicate as the empty relation, but Scryer raises an existence error on a
        // rule body atom whose procedure has zero facts, so a well-defined comparison
        // requires each referenced predicate to be present in both graphs.
        let cyclic = [
            edge("a", "p", "b"),
            edge("b", "p", "c"),
            edge("c", "p", "a"),
            edge("a", "q", "c"),
            edge("c", "r", "b"),
        ];

        // The full lowerable-operator corpus, each labelled for a precise failure message.
        let corpus: Vec<(&str, PropertyPathExpression)> = vec![
            ("NamedNode", named("p")),
            (
                "Reverse",
                PropertyPathExpression::Reverse(Box::new(named("p"))),
            ),
            (
                "Sequence",
                PropertyPathExpression::Sequence(Box::new(named("q")), Box::new(named("r"))),
            ),
            (
                "Alternative",
                PropertyPathExpression::Alternative(Box::new(named("p")), Box::new(named("q"))),
            ),
            (
                "ZeroOrOne",
                PropertyPathExpression::ZeroOrOne(Box::new(named("p"))),
            ),
            (
                "ZeroOrMore",
                PropertyPathExpression::ZeroOrMore(Box::new(named("p"))),
            ),
            (
                "OneOrMore",
                PropertyPathExpression::OneOrMore(Box::new(named("p"))),
            ),
            (
                "Range{2,2}",
                PropertyPathExpression::Range {
                    inner: Box::new(named("p")),
                    min: 2,
                    max: Some(2),
                },
            ),
            (
                "Range{0,2}",
                PropertyPathExpression::Range {
                    inner: Box::new(named("p")),
                    min: 0,
                    max: Some(2),
                },
            ),
            (
                "Range{2,None}",
                PropertyPathExpression::Range {
                    inner: Box::new(named("p")),
                    min: 2,
                    max: None,
                },
            ),
        ];

        // For each operator, over each graph, in each of three endpoint modes
        // (subject-bound, object-bound-reverse, both-variable) the native answer set
        // must equal Scryer's.
        for (label, path) in &corpus {
            for (graph_name, edges) in [("acyclic", &acyclic[..]), ("cyclic", &cyclic[..])] {
                let modes: [(&str, PathEnd, PathEnd); 3] = [
                    ("subj-bound", PathEnd::Iri(iri("a")), PathEnd::Variable),
                    ("obj-bound", PathEnd::Variable, PathEnd::Iri(iri("a"))),
                    ("both-var", PathEnd::Variable, PathEnd::Variable),
                ];
                for (mode, subject, object) in &modes {
                    let (native, scryer) = native_and_scryer_pairs(edges, path, subject, object);
                    assert_eq!(
                        native, scryer,
                        "native-lowered ≢ Scryer for operator {label} on {graph_name} graph \
                         ({mode}): native={native:?} scryer={scryer:?}"
                    );
                }
            }
        }
    }
}
