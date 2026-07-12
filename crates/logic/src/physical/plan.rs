// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The consuming type-state plan pipeline `Parsed → Stratified → Planned → Executable`.
//!
//! # Why a type-state pipeline
//!
//! The semi-naive executor ([`super::seminaive::eval_stratum_fixpoint`]) may run ONLY a
//! program that has been (a) proven stratifiable and (b) join-planned.  Encoding that
//! as a doc contract ("call `stratify` first") is fragile: a caller can forget, or
//! re-stratify per call.  This module makes an unstratified/unplanned program
//! **unrepresentable at the executor boundary** — the executor's only input is an
//! [`Executable`], whose sole constructor chain is `Parsed::new(..).stratify()?.plan()
//! .into_executable()`.  There is no other way to obtain one, so the compiler — not a
//! comment — enforces "stratify then plan then execute".
//!
//! # Consuming transitions (not marker generics)
//!
//! Each stage is a DISTINCT type; each transition method takes `self` by value and
//! returns the next stage, mirroring `purrdf`'s `ValidatedRdfDatasetBuilder`.  A
//! `PhantomData<State>` marker bolted onto one shared struct would let a caller name
//! the wrong state or transmute between them; distinct types cannot be confused, and a
//! consumed stage is moved-from and unusable, so a stale earlier-stage value can never
//! be fed to a later step.
//!
//! # What each stage MEMOIZES (computed once, not per round / per call)
//!
//! - [`Stratified`] owns the [`stratify`](super::seminaive::stratify) result lowered
//!   into the per-stratum rule grouping (`strata[k]` = the program-order rule indices
//!   of stratum `k`) — exactly the `rules_by_stratum` the forward/backward evaluators
//!   used to rebuild inside every `materialize_native`/`evaluate` call.
//! - [`Planned`] owns a [`RulePlan`] per rule: the positive/negated body-atom partition
//!   the per-round join
//!   ([`join_body_indexed`](super::seminaive::join_body_indexed)) used to re-`filter`
//!   and re-allocate on every semi-naive round.  This is the genuinely
//!   per-round-redundant work; the partition is a static function of the rule, so it is
//!   hoisted here once.
//! - [`Executable`] additionally memoizes the head-predicate set (the IDB-derivable
//!   predicates) the completion frontier reads.
//!
//! Predicate→`PredId` resolution is intentionally NOT hoisted here: a
//! [`PredId`](crate::facts::PredId) is a per-`RelationStore` handle minted at
//! EDB-load/derivation time in insertion order, so it is meaningless against a store
//! that does not yet exist at plan time.  The plan is store-independent; resolving ids
//! here would be unsound, not an optimization.

use std::collections::{BTreeMap, BTreeSet};

use petgraph::algo::bridges;
use petgraph::graph::{EdgeIndex, UnGraph};
use petgraph::unionfind::UnionFind;
use petgraph::visit::EdgeRef;

use crate::rule_ir::EvalRule;

use super::seminaive::stratify;

/// A per-rule precomputed join plan.
///
/// Today this is the positive/negated body-atom partition — the ONLY genuinely
/// per-round-redundant, statically-determinable work in the join.  The atom ORDER is
/// already static in `rule.body`, and the actual `Bound` VALUES depend on the runtime
/// partial solution (so they are not precomputable); what the per-round loop needlessly
/// recomputed was the `rule.body.iter().filter(..).collect()` partition, re-allocated
/// every round.  That is hoisted here once.
pub(crate) struct RulePlan {
    /// Body indices of the POSITIVE atoms, in body order (the join drivers).
    positive: Box<[usize]>,
    /// Body indices of the NEGATED atoms, in body order (the NAF filters).
    negated: Box<[usize]>,
    /// Present only for a structurally-certified cyclic rule. Acyclic rules retain
    /// exactly two immutable slices and allocate no physical-group sidecar.
    hybrid: Option<Box<HybridPlan>>,
}

/// The cyclic-only physical sidecar. Boxing it keeps the common acyclic `RulePlan`
/// smaller than the former two-`Vec` representation (`Box<[usize]>` is two words), so
/// selective WCOJ does not tax the binary majority's resident plan footprint.
struct HybridPlan {
    join_groups: Box<[JoinGroup]>,
    source_order_swaps: Box<[(usize, usize)]>,
}

/// One positive atom together with both of its stable coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlannedAtom {
    /// Index into [`EvalRule::body`].
    body_index: usize,
    /// Index within [`RulePlan::positive`].
    positive_position: usize,
}

impl PlannedAtom {
    pub(crate) fn body_index(self) -> usize {
        self.body_index
    }

    pub(crate) fn positive_position(self) -> usize {
        self.positive_position
    }
}

/// A planner-certified cyclic component lowered to the multiway kernel.
#[derive(Debug)]
pub(crate) struct CyclicPlan {
    /// Component atoms in authored positive-body order.
    atoms: Vec<PlannedAtom>,
    /// Deterministic LFTJ variable order: structural degree descending, then first
    /// authored occurrence, then lexical name.
    variables: Vec<String>,
}

impl CyclicPlan {
    pub(crate) fn atoms(&self) -> &[PlannedAtom] {
        &self.atoms
    }

    pub(crate) fn variables(&self) -> &[String] {
        &self.variables
    }
}

/// One physical group in a rule's positive join.
#[derive(Debug)]
pub(crate) enum JoinGroup {
    /// The existing indexed binary operator for one atom.
    Binary(PlannedAtom),
    /// One certified cyclic component evaluated as a multiway leapfrog triejoin.
    Leapfrog(CyclicPlan),
}

impl RulePlan {
    /// The static partition of `rule`'s body into positive (join) and negated (NAF)
    /// atoms, preserving body order — byte-identical to the per-round
    /// `filter(|a| !a.negated)` / `filter(|a| a.negated)` it replaces.
    pub(super) fn for_rule(rule: &EvalRule) -> Self {
        let mut positive = Vec::new();
        let mut negated = Vec::new();
        for (i, atom) in rule.body.iter().enumerate() {
            if atom.negated {
                negated.push(i);
            } else {
                positive.push(i);
            }
        }
        // A simple undirected cycle requires at least three positive edges. Avoid all
        // graph/planned-atom scratch for the overwhelmingly-common 0/1/2-atom rules.
        let cyclic = if positive.len() < 3 {
            Vec::new()
        } else {
            certified_cyclic_components(rule, &positive)
        };

        if cyclic.is_empty() {
            return Self {
                positive: positive.into_boxed_slice(),
                negated: negated.into_boxed_slice(),
                hybrid: None,
            };
        }

        // Map every promoted atom to its owning cycle component. Components are
        // edge-disjoint after bridge removal; one atom can therefore belong to at most
        // one component.
        let mut component_of: Vec<Option<usize>> = vec![None; rule.body.len()];
        for (component, plan) in cyclic.iter().enumerate() {
            for atom in &plan.atoms {
                component_of[atom.body_index] = Some(component);
            }
        }

        // Emit a cycle component at its first authored atom and skip its later atoms.
        // Any non-cycle atom remains a binary group at its own authored position.
        let mut cyclic: Vec<Option<CyclicPlan>> = cyclic.into_iter().map(Some).collect();
        let mut join_groups = Vec::new();
        let mut execution_source_order = Vec::with_capacity(positive.len());
        for (positive_position, &body_index) in positive.iter().enumerate() {
            let atom = PlannedAtom {
                body_index,
                positive_position,
            };
            match component_of[atom.body_index] {
                Some(component) => {
                    if let Some(plan) = cyclic[component].take() {
                        execution_source_order
                            .extend(plan.atoms.iter().map(|a| a.positive_position));
                        join_groups.push(JoinGroup::Leapfrog(plan));
                    }
                }
                None => {
                    execution_source_order.push(atom.positive_position);
                    join_groups.push(JoinGroup::Binary(atom));
                }
            }
        }

        let source_order_swaps = restore_body_order_swaps(&execution_source_order);
        Self {
            positive: positive.into_boxed_slice(),
            negated: negated.into_boxed_slice(),
            hybrid: Some(Box::new(HybridPlan {
                join_groups: join_groups.into_boxed_slice(),
                source_order_swaps: source_order_swaps.into_boxed_slice(),
            })),
        }
    }

    /// The positive body-atom indices, in body order.
    pub(crate) fn positive(&self) -> &[usize] {
        &self.positive
    }

    /// The negated body-atom indices, in body order.
    pub(crate) fn negated(&self) -> &[usize] {
        &self.negated
    }

    /// Whether this rule has a planner-certified cyclic positive subplan.
    pub(crate) fn has_cyclic_subplan(&self) -> bool {
        self.hybrid.is_some()
    }

    /// Physical positive-join groups in deterministic execution order.
    pub(crate) fn join_groups(&self) -> &[JoinGroup] {
        &self
            .hybrid
            .as_ref()
            .expect("join groups exist only for a certified cyclic plan")
            .join_groups
    }

    /// In-place swaps restoring physical source order to authored body order.
    pub(crate) fn source_order_swaps(&self) -> &[(usize, usize)] {
        &self
            .hybrid
            .as_ref()
            .expect("source swaps exist only for a certified cyclic plan")
            .source_order_swaps
    }
}

/// Certify the positive-body cycle components eligible for WCOJ.
///
/// Each atom with two DISTINCT variable positions contributes one undirected variable
/// edge. Duplicate pairs are collapsed before graph analysis: two relations over the
/// same `(X,Y)` edge are an acyclic intersection, not a two-edge cycle. Removing every
/// graph bridge leaves exactly the edges participating in a simple cycle; their
/// connected components are the subplans safe to promote. Constants, repeated
/// variables, unary atoms, trees, and bridge atoms consequently remain binary.
fn certified_cyclic_components(rule: &EvalRule, positive: &[usize]) -> Vec<CyclicPlan> {
    use crate::rule_ir::EvalTerm;

    let mut edge_atoms: BTreeMap<(String, String), Vec<PlannedAtom>> = BTreeMap::new();
    let mut first_occurrence: BTreeMap<String, usize> = BTreeMap::new();
    let mut occurrence = 0usize;

    for (positive_position, &body_index) in positive.iter().enumerate() {
        let planned = PlannedAtom {
            body_index,
            positive_position,
        };
        let atom = &rule.body[body_index];
        for term in [&atom.subject, &atom.object] {
            if let EvalTerm::Var(var) = term {
                first_occurrence.entry(var.clone()).or_insert(occurrence);
                occurrence += 1;
            }
        }
        let (EvalTerm::Var(left), EvalTerm::Var(right)) = (&atom.subject, &atom.object) else {
            continue;
        };
        if left == right {
            continue;
        }
        let edge = if left < right {
            (left.clone(), right.clone())
        } else {
            (right.clone(), left.clone())
        };
        edge_atoms.entry(edge).or_default().push(planned);
    }

    if edge_atoms.len() < 3 {
        return Vec::new();
    }

    let variable_names: Vec<String> = edge_atoms
        .keys()
        .flat_map(|(left, right)| [left.clone(), right.clone()])
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let variable_id: BTreeMap<&str, usize> = variable_names
        .iter()
        .enumerate()
        .map(|(id, name)| (name.as_str(), id))
        .collect();

    let mut graph: UnGraph<(), ()> = UnGraph::default();
    let nodes: Vec<_> = (0..variable_names.len())
        .map(|_| graph.add_node(()))
        .collect();
    let mut edge_keys = Vec::with_capacity(edge_atoms.len());
    for edge @ (left, right) in edge_atoms.keys() {
        graph.add_edge(
            nodes[variable_id[left.as_str()]],
            nodes[variable_id[right.as_str()]],
            (),
        );
        edge_keys.push(edge.clone());
    }

    let bridge_ids: BTreeSet<EdgeIndex> = bridges(&graph).map(|edge| edge.id()).collect();
    let mut union = UnionFind::new(variable_names.len());
    for edge in graph.edge_indices() {
        if bridge_ids.contains(&edge) {
            continue;
        }
        let (left, right) = graph
            .edge_endpoints(edge)
            .expect("an indexed undirected edge has endpoints");
        union.union(left.index(), right.index());
    }

    let mut component_edges: BTreeMap<usize, Vec<EdgeIndex>> = BTreeMap::new();
    for edge in graph.edge_indices() {
        if bridge_ids.contains(&edge) {
            continue;
        }
        let (left, _) = graph
            .edge_endpoints(edge)
            .expect("an indexed undirected edge has endpoints");
        component_edges
            .entry(union.find(left.index()))
            .or_default()
            .push(edge);
    }

    let mut plans = Vec::new();
    for edges in component_edges.into_values() {
        // A simple undirected cycle has at least three unique edges. This also makes
        // the duplicate-pair non-cycle exclusion explicit at the promotion boundary.
        if edges.len() < 3 {
            continue;
        }
        let mut atoms = Vec::new();
        let mut degree: BTreeMap<String, usize> = BTreeMap::new();
        for edge in edges {
            let key = &edge_keys[edge.index()];
            atoms.extend(edge_atoms[key].iter().copied());
            *degree.entry(key.0.clone()).or_default() += 1;
            *degree.entry(key.1.clone()).or_default() += 1;
        }
        atoms.sort_by_key(|atom| atom.positive_position);
        let mut variables: Vec<String> = degree.keys().cloned().collect();
        variables.sort_by(|left, right| {
            degree[right]
                .cmp(&degree[left])
                .then_with(|| first_occurrence[left].cmp(&first_occurrence[right]))
                .then_with(|| left.cmp(right))
        });
        plans.push(CyclicPlan { atoms, variables });
    }
    plans
}

/// Precompute a minimal deterministic swap program from physical source order to
/// authored positive-body order. The executor applies these swaps directly to each
/// completed hybrid solution, with no per-solution permutation allocation.
fn restore_body_order_swaps(execution_order: &[usize]) -> Vec<(usize, usize)> {
    let mut current = execution_order.to_vec();
    let mut swaps = Vec::new();
    for wanted in 0..current.len() {
        let position = current
            .iter()
            .position(|&value| value == wanted)
            .expect("physical groups cover every positive atom exactly once");
        if position != wanted {
            current.swap(wanted, position);
            swaps.push((wanted, position));
        }
    }
    debug_assert!(current.iter().copied().eq(0..current.len()));
    swaps
}

/// Stage 1: a parsed rule program, not yet stratified.
///
/// Borrows the caller-owned rules (the pipeline threads references, never a clone, so
/// the memoized plan sits next to the rules the caller already holds).
pub(crate) struct Parsed<'r> {
    rules: &'r [EvalRule],
}

impl<'r> Parsed<'r> {
    /// Enter the pipeline with a parsed rule program.
    pub(crate) fn new(rules: &'r [EvalRule]) -> Self {
        Self { rules }
    }

    /// Compute the stratification ONCE and lower it into the per-stratum rule grouping.
    ///
    /// `None` ⇒ the program is non-stratifiable (a negative dependency-graph edge inside
    /// a cycle): a declared gap the caller routes to its oracle / base-fallback, exactly
    /// where the evaluators used to return `Unsupported(NonStratifiable)`.
    pub(crate) fn stratify(self) -> Option<Stratified<'r>> {
        let stratum_of = stratify(self.rules)?;

        // Order the rules into strata.  A rule belongs to the stratum of its HEAD
        // predicate; within a stratum the original program order is preserved (rules
        // fire in parse order, matching the reference engine).  This is byte-identical
        // to the `rules_by_stratum` the evaluators previously rebuilt per call — the
        // grouping is by program-order rule index.
        let max_stratum = self
            .rules
            .iter()
            .map(|r| stratum_of[r.head.predicate.as_str()])
            .max()
            .unwrap_or(0);
        let mut strata: Vec<Vec<usize>> = vec![Vec::new(); max_stratum + 1];
        for (i, rule) in self.rules.iter().enumerate() {
            let s = stratum_of[rule.head.predicate.as_str()];
            strata[s].push(i);
        }

        Some(Stratified {
            rules: self.rules,
            strata,
        })
    }
}

/// Stage 2: a stratifiable program with its per-stratum rule grouping memoized.
pub(crate) struct Stratified<'r> {
    rules: &'r [EvalRule],
    /// `strata[k]` = the program-order indices (into `rules`) of stratum `k`'s rules.
    strata: Vec<Vec<usize>>,
}

impl<'r> Stratified<'r> {
    /// Precompute a [`RulePlan`] per rule (the positive/negated partition hoisted out of
    /// the per-round join), yielding the [`Planned`] stage.
    pub(crate) fn plan(self) -> Planned<'r> {
        let plans: Vec<RulePlan> = self.rules.iter().map(RulePlan::for_rule).collect();
        Planned {
            rules: self.rules,
            strata: self.strata,
            plans,
        }
    }
}

/// Stage 3: a stratified program with its per-rule join plans memoized.
pub(crate) struct Planned<'r> {
    rules: &'r [EvalRule],
    strata: Vec<Vec<usize>>,
    /// One entry per rule, parallel to `rules` by index.
    plans: Vec<RulePlan>,
}

impl<'r> Planned<'r> {
    /// Seal the plan into the terminal [`Executable`] — the ONLY type the semi-naive
    /// executor accepts.  Memoizes the head-predicate set the completion frontier reads.
    pub(crate) fn into_executable(self) -> Executable<'r> {
        let head_predicates: BTreeSet<String> = self
            .rules
            .iter()
            .map(|r| r.head.predicate.as_str().to_owned())
            .collect();
        Executable {
            rules: self.rules,
            strata: self.strata,
            plans: self.plans,
            head_predicates,
        }
    }
}

/// Stage 4 (terminal): a fully stratified, join-planned program.
///
/// The SOLE input type of the semi-naive executor
/// ([`super::seminaive::eval_stratum_fixpoint`]).  Its fields are private and its only
/// constructor is [`Planned::into_executable`], so a value of this type is a proof that
/// the program was stratified (stage 1→2) and planned (stage 2→3) — the type gate.
pub(crate) struct Executable<'r> {
    rules: &'r [EvalRule],
    strata: Vec<Vec<usize>>,
    plans: Vec<RulePlan>,
    head_predicates: BTreeSet<String>,
}

impl Executable<'_> {
    /// The number of strata (the completion frontier's `total`).
    pub(crate) fn stratum_count(&self) -> usize {
        self.strata.len()
    }

    /// Whether stratum `k` has no rules (a trivially-saturated empty stratum).
    pub(crate) fn stratum_is_empty(&self, k: usize) -> bool {
        self.strata[k].is_empty()
    }

    /// The IDB-derivable (rule-head) predicates — the ones settled only when their
    /// stratum completes, excluded from the pure-EDB seed frontier.
    pub(crate) fn head_predicates(&self) -> &BTreeSet<String> {
        &self.head_predicates
    }

    /// The `(rule, plan)` entries of stratum `k`, in program order — the executor's
    /// per-round rule iteration, paired with each rule's precomputed join plan.
    pub(crate) fn stratum_entries(&self, k: usize) -> impl Iterator<Item = (&EvalRule, &RulePlan)> {
        self.strata[k]
            .iter()
            .map(move |&i| (&self.rules[i], &self.plans[i]))
    }

    /// The head predicates of stratum `k`'s rules — recorded into the settled frontier
    /// when the stratum reaches its natural fixpoint.
    pub(crate) fn stratum_head_predicates(&self, k: usize) -> impl Iterator<Item = &str> {
        self.strata[k]
            .iter()
            .map(move |&i| self.rules[i].head.predicate.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physical::store::RelationStore;
    use crate::rule_ir::parse_eval_rules;
    use crate::store::WorldStore;

    const NS: &str = "https://example.org/plan/";

    /// A one-stratum transitive-closure program in the ternary `pred(s, o, world)`
    /// encoding — stratifiable, so it reaches an `Executable`.
    fn tc_rules() -> Vec<EvalRule> {
        let rls = format!(
            "#[name(\"{NS}base\")]\n\
             <{NS}path>(?X, ?Z, ?W) :- <{NS}edge>(?X, ?Z, ?W) .\n\
             #[name(\"{NS}step\")]\n\
             <{NS}path>(?X, ?Z, ?W) :- <{NS}edge>(?X, ?Y, ?W), <{NS}path>(?Y, ?Z, ?W) .\n"
        );
        parse_eval_rules(&rls).expect("parse TC rules")
    }

    /// The consuming type-state pipeline is the SOLE path to an `Executable`, and the
    /// semi-naive executor entries accept ONLY an `Executable` — enforced by the compiler,
    /// not a doc comment.
    ///
    /// Two independent compile-time facts establish the gate:
    ///
    /// 1. **Distinct stage types.** Each `let` below binds an EXPLICIT stage type; the
    ///    program only type-checks because `stratify`/`plan`/`into_executable` are
    ///    consuming transitions between four SEPARATE types (a `PhantomData<State>` marker
    ///    on one shared struct would collapse them and defeat the gate). A consumed stage
    ///    is moved-from, so a stale `Parsed`/`Stratified`/`Planned` cannot be reused.
    ///
    /// 2. **Executor input is `&Executable`.** The `fn`-pointer coercions pin the
    ///    rule-input parameter of BOTH executor entries to `&Executable<'_>`. Were either
    ///    changed to accept `&[EvalRule]` (the old ad-hoc-stratify signature), a `Parsed`,
    ///    a `Stratified`, or a `Planned`, the coercion would fail to compile. Combined with
    ///    `Executable`'s private fields and single constructor (`Planned::into_executable`),
    ///    an unstratified/unplanned program is unrepresentable at the executor boundary.
    #[test]
    fn typestate_pipeline_is_the_sole_path_to_the_executor() {
        let rules = tc_rules();

        // (1) Walk the pipeline; the explicit stage annotations are the distinct-types
        // proof. There is no shortcut: `Executable` has no other constructor.
        let parsed: Parsed<'_> = Parsed::new(&rules);
        let stratified: Stratified<'_> = parsed.stratify().expect("stratifiable");
        let planned: Planned<'_> = stratified.plan();
        let executable: Executable<'_> = planned.into_executable();

        // (2) The forward/backward executor entries accept ONLY `&Executable`. These
        // coercions are the compile-time gate — `_` infers the parameters we do not pin.
        let _forward_gate: fn(&WorldStore, &Executable<'_>, Option<u64>) -> _ =
            crate::physical::materialize_native;
        let _backward_gate: fn(RelationStore, &Executable<'_>, Option<u64>) -> _ =
            crate::physical::evaluate;

        // The terminal stage genuinely drives the executor end-to-end (not just a type
        // check): seed `edge(a,b),(b,c)` in one world and confirm the closure runs.
        let store = WorldStore::new();
        store.insert_quad(
            "https://example.org/plan/w",
            &format!("{NS}a"),
            &format!("{NS}edge"),
            &format!("{NS}b"),
        );
        store.insert_quad(
            "https://example.org/plan/w",
            &format!("{NS}b"),
            &format!("{NS}edge"),
            &format!("{NS}c"),
        );
        let outcome = crate::physical::materialize_native(&store, &executable, None)
            .expect("materialize_native runs on the sealed Executable");
        let crate::physical::NativeOutcome::Decided(budgeted) = outcome else {
            panic!("a stratifiable program must be Decided");
        };
        // path = {a→b, b→c, a→c} (three derived pairs) proves the sealed plan executed.
        let path_pred = format!("{NS}path");
        let derived_paths = budgeted
            .rows
            .iter()
            .filter(|r| r.predicate == path_pred)
            .count();
        assert_eq!(
            derived_paths, 3,
            "transitive closure over a→b→c derives 3 paths"
        );
    }

    /// The plan memoizes the positive/negated body-atom partition ONCE (the per-round
    /// redundant work that is genuinely hoistable): every positive body index points to a
    /// non-negated atom and every negated index to a negated one, in ascending body order —
    /// byte-identical to the `filter(|a| !a.negated)` / `filter(|a| a.negated)` it replaces.
    #[test]
    fn rule_plan_partitions_body_by_negation_flag() {
        // A rule mixing positive and negated body atoms.
        let rls = format!(
            "#[name(\"{NS}mixed\")]\n\
             <{NS}h>(?X, ?Z, ?W) :- <{NS}p>(?X, ?Z, ?W), ~<{NS}q>(?X, ?Z, ?W), \
             <{NS}r>(?X, ?Z, ?W) .\n"
        );
        let rules = parse_eval_rules(&rls).expect("parse mixed rule");
        let rule = &rules[0];
        let plan = RulePlan::for_rule(rule);

        // The partition is exactly the parser's body order split on the `negated` flag —
        // the same split the per-round join used to recompute, now hoisted once.
        let want_positive: Vec<usize> = (0..rule.body.len())
            .filter(|&i| !rule.body[i].negated)
            .collect();
        let want_negated: Vec<usize> = (0..rule.body.len())
            .filter(|&i| rule.body[i].negated)
            .collect();
        assert_eq!(plan.positive(), want_positive.as_slice());
        assert_eq!(plan.negated(), want_negated.as_slice());
        // Every positive index is a non-negated atom; every negated index is negated.
        assert!(plan.positive().iter().all(|&i| !rule.body[i].negated));
        assert!(plan.negated().iter().all(|&i| rule.body[i].negated));
        // Ascending body order (insertion-order preservation).
        assert!(plan.positive().windows(2).all(|w| w[0] < w[1]));
        assert!(plan.negated().windows(2).all(|w| w[0] < w[1]));
    }

    fn one_rule(body: &str) -> Vec<EvalRule> {
        let rls = format!(
            "#[name(\"{NS}cycle-test\")]\n\
             <{NS}h>(?X, ?Z, ?W) :- {body} .\n"
        );
        parse_eval_rules(&rls).expect("parse cycle-plan rule")
    }

    fn cyclic_atoms(plan: &RulePlan) -> Vec<Vec<usize>> {
        if !plan.has_cyclic_subplan() {
            return Vec::new();
        }
        plan.join_groups()
            .iter()
            .filter_map(|group| match group {
                JoinGroup::Binary(_) => None,
                JoinGroup::Leapfrog(cycle) => {
                    Some(cycle.atoms().iter().map(|atom| atom.body_index()).collect())
                }
            })
            .collect()
    }

    /// Triangle and clique bodies are certified structurally, with deterministic
    /// variable and atom order independent of runtime relation cardinalities.
    #[test]
    fn planner_certifies_triangle_and_clique() {
        let triangle = one_rule(&format!(
            "<{NS}r>(?X, ?Y, ?W), <{NS}s>(?Y, ?Z, ?W), <{NS}t>(?Z, ?X, ?W)"
        ));
        let plan = RulePlan::for_rule(&triangle[0]);
        assert_eq!(cyclic_atoms(&plan), vec![vec![0, 1, 2]]);
        let JoinGroup::Leapfrog(cycle) = &plan.join_groups()[0] else {
            panic!("a triangle must lower to one leapfrog group");
        };
        assert_eq!(cycle.variables(), &["?X", "?Y", "?Z"]);

        let clique = one_rule(&format!(
            "<{NS}ab>(?A, ?B, ?W), <{NS}ac>(?A, ?C, ?W), \
             <{NS}ad>(?A, ?D, ?W), <{NS}bc>(?B, ?C, ?W), \
             <{NS}bd>(?B, ?D, ?W), <{NS}cd>(?C, ?D, ?W)"
        ));
        let clique_plan = RulePlan::for_rule(&clique[0]);
        assert_eq!(cyclic_atoms(&clique_plan), vec![vec![0, 1, 2, 3, 4, 5]]);
        let JoinGroup::Leapfrog(cycle) = &clique_plan.join_groups()[0] else {
            panic!("a K4 body must lower to one leapfrog group");
        };
        assert_eq!(cycle.variables(), &["?A", "?B", "?C", "?D"]);
    }

    /// Bridge removal promotes only the cycle, even when an acyclic atom is authored
    /// between cycle atoms. The precomputed swaps restore provenance to body order.
    #[test]
    fn planner_extracts_cycle_subplan_and_keeps_bridge_binary() {
        let rules = one_rule(&format!(
            "<{NS}r>(?X, ?Y, ?W), <{NS}leaf>(?X, ?Q, ?W), \
             <{NS}s>(?Y, ?Z, ?W), <{NS}t>(?Z, ?X, ?W)"
        ));
        let plan = RulePlan::for_rule(&rules[0]);
        assert_eq!(cyclic_atoms(&plan), vec![vec![0, 2, 3]]);
        assert_eq!(plan.join_groups().len(), 2);
        let JoinGroup::Binary(atom) = plan.join_groups()[1] else {
            panic!("the leaf bridge must remain binary");
        };
        assert_eq!(atom.body_index(), 1);
        assert!(
            !plan.source_order_swaps().is_empty(),
            "interleaved physical groups need a body-order restoration program"
        );
    }

    /// The conservative certificate rejects every non-cycle shape, including the
    /// duplicate-edge false positive that a multigraph cycle detector would promote.
    #[test]
    fn planner_keeps_non_cycles_binary() {
        let bodies = [
            format!("<{NS}a>(?X, ?Y, ?W), <{NS}b>(?Y, ?Z, ?W), <{NS}c>(?Z, ?Q, ?W)"),
            format!("<{NS}a>(?X, ?Y, ?W), <{NS}b>(?X, ?Z, ?W), <{NS}c>(?X, ?Q, ?W)"),
            format!("<{NS}a>(?X, ?Y, ?W), <{NS}b>(?X, ?Y, ?W), <{NS}c>(?X, ?Y, ?W)"),
            format!("<{NS}a>(?X, ?X, ?W), <{NS}b>(?X, ?Y, ?W), <{NS}c>(?Y, ?Z, ?W)"),
            format!(
                "<{NS}a>(?X, ?Y, ?W), <{NS}b>(?Y, ?Z, ?W), \
                 <{NS}c>(?Z, <{NS}constant>, ?W)"
            ),
        ];
        for body in bodies {
            let rules = one_rule(&body);
            let plan = RulePlan::for_rule(&rules[0]);
            assert!(
                !plan.has_cyclic_subplan(),
                "non-cycle body was falsely promoted: {body}"
            );
        }
    }
}
