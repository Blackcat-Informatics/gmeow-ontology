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
//! [`Executable`], whose sole constructor chain is `Parsed::uncached(..).stratify()?.plan()
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
//! - [`Planned`] owns a [`RulePlan`] per rule: the positive/negated body partition,
//!   flat variable slots, binding-aware SIPS order, guaranteed index shape,
//!   variable/constant kernel shape, cyclic subplans, and source-order restoration.
//!   All are static functions of the rule and are hoisted out of every semi-naive round.
//! - [`Executable`] additionally memoizes the head-predicate set (the IDB-derivable
//!   predicates) the completion frontier reads.
//!
//! Predicate→`PredId` resolution is intentionally NOT hoisted here: a
//! [`PredId`](crate::facts::PredId) is a per-`RelationStore` handle minted at
//! EDB-load/derivation time in insertion order, so it is meaningless against a store
//! that does not yet exist at plan time.  The plan is store-independent; resolving ids
//! here would be unsound, not an optimization.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};

use petgraph::algo::bridges;
use petgraph::graph::{EdgeIndex, UnGraph};
use petgraph::unionfind::UnionFind;
use petgraph::visit::EdgeRef;

use crate::provenance::term_display;
use crate::query_ir::{QBuiltin, QTerm};
use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm};

use super::seminaive::stratify;

/// Version of the physical planner and its executable-kernel ABI.
///
/// This is deliberately independent of the rule and reasoning-contract digests: a
/// planner/kernel change invalidates every cached plan even when its logical input is
/// unchanged.
pub(crate) const PLAN_SOLVER_VERSION: &str = "gmeow-native-plan-v1";

/// Maximum number of compiled programs retained by the process-wide plan cache.
///
/// The cache is intentionally small and bounded. Plans are immutable [`Arc`] values;
/// eviction drops only the cache's reference and cannot invalidate an in-flight run.
const PLAN_CACHE_CAPACITY: usize = 64;

/// Content-addressed identity of one immutable executable plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlanIdentity {
    contract_hash: String,
    rule_hash: [u8; 32],
    solver_version: &'static str,
}

impl PlanIdentity {
    fn new(contract_hash: impl Into<String>, rules: &[EvalRule]) -> Self {
        Self {
            contract_hash: contract_hash.into(),
            rule_hash: canonical_rule_hash(rules),
            solver_version: PLAN_SOLVER_VERSION,
        }
    }

    pub(crate) fn contract_hash(&self) -> &str {
        &self.contract_hash
    }

    pub(crate) fn rule_hash(&self) -> &[u8; 32] {
        &self.rule_hash
    }

    pub(crate) fn solver_version(&self) -> &'static str {
        self.solver_version
    }
}

/// Result of looking up or compiling a physical plan.
///
/// `plan_builds` and `planning_units` are deterministic cost-vector coordinates:
/// a cold lookup reports one build and the exact number of static rule/atom/builtin
/// nodes inspected; a warm lookup reports zero for both. Neither relies on wall time.
pub(crate) struct PlanLookup {
    pub(crate) executable: Option<Arc<Executable>>,
    pub(crate) cache_hit: bool,
    pub(crate) plan_builds: u64,
    pub(crate) planning_units: u64,
}

struct CachedPlan {
    identity: PlanIdentity,
    executable: Option<Arc<Executable>>,
}

/// Explicit bounded LRU cache used both by the process-wide planning seam and focused
/// deterministic tests. Execution never takes this lock: the lookup returns an immutable
/// [`Arc<Executable>`], then releases the cache before touching data.
pub(crate) struct PlanCache {
    capacity: usize,
    entries: VecDeque<CachedPlan>,
}

impl PlanCache {
    pub(crate) fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "plan cache capacity must be non-zero");
        Self {
            capacity,
            entries: VecDeque::with_capacity(capacity),
        }
    }

    /// Look up `rules`, compiling and inserting them on a miss.
    ///
    /// Non-stratifiable programs are cached as `None`, so repeated declared gaps also
    /// skip stratification. The cache key contains the full canonical rule digest; a
    /// contract hash alone can never alias two programs.
    pub(crate) fn get_or_compile(
        &mut self,
        contract_hash: impl Into<String>,
        rules: Vec<EvalRule>,
    ) -> PlanLookup {
        let identity = PlanIdentity::new(contract_hash, &rules);
        if let Some(executable) = self.lookup(&identity) {
            return PlanLookup {
                executable,
                cache_hit: true,
                plan_builds: 0,
                planning_units: 0,
            };
        }

        let planning_units = static_planning_units(&rules);
        let executable = compile_executable(identity.clone(), rules);
        self.insert(identity, executable.clone());
        PlanLookup {
            executable,
            cache_hit: false,
            plan_builds: 1,
            planning_units,
        }
    }

    /// LRU lookup. The outer `Option` distinguishes a miss from a cached negative
    /// result (`Some(None)`).
    fn lookup(&mut self, identity: &PlanIdentity) -> Option<Option<Arc<Executable>>> {
        let position = self
            .entries
            .iter()
            .position(|entry| &entry.identity == identity)?;
        let entry = self
            .entries
            .remove(position)
            .expect("located cache entry still exists");
        let executable = entry.executable.clone();
        self.entries.push_back(entry);
        Some(executable)
    }

    fn insert(&mut self, identity: PlanIdentity, executable: Option<Arc<Executable>>) {
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(CachedPlan {
            identity,
            executable,
        });
    }
}

static PLAN_CACHE: OnceLock<Mutex<PlanCache>> = OnceLock::new();

/// Compile through the process-wide bounded plan cache.
pub(crate) fn compile_cached(contract_hash: impl Into<String>, rules: Vec<EvalRule>) -> PlanLookup {
    let cache = PLAN_CACHE.get_or_init(|| Mutex::new(PlanCache::new(PLAN_CACHE_CAPACITY)));
    let identity = PlanIdentity::new(contract_hash, &rules);
    let planning_units = static_planning_units(&rules);
    let compile_identity = identity.clone();
    compile_with_cache(cache, identity, planning_units, || {
        compile_executable(compile_identity, rules)
    })
}

fn compile_executable(identity: PlanIdentity, rules: Vec<EvalRule>) -> Option<Arc<Executable>> {
    Parsed::from_owned(identity, rules)
        .stratify()
        .map(|stratified| Arc::new(stratified.plan().into_executable()))
}

/// Process-cache protocol: lookup under the mutex, compile without it, then
/// re-lock and reuse a racing insertion or install this result.
fn compile_with_cache(
    cache: &Mutex<PlanCache>,
    identity: PlanIdentity,
    planning_units: u64,
    compile: impl FnOnce() -> Option<Arc<Executable>>,
) -> PlanLookup {
    if let Some(executable) = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .lookup(&identity)
    {
        return PlanLookup {
            executable,
            cache_hit: true,
            plan_builds: 0,
            planning_units: 0,
        };
    }

    let compiled = compile();
    let executable = {
        let mut cache = cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(existing) = cache.lookup(&identity) {
            existing
        } else {
            cache.insert(identity, compiled.clone());
            compiled
        }
    };
    PlanLookup {
        executable,
        cache_hit: false,
        plan_builds: 1,
        planning_units,
    }
}

fn static_planning_units(rules: &[EvalRule]) -> u64 {
    rules.iter().fold(0_u64, |units, rule| {
        units
            .saturating_add(1)
            .saturating_add(rule.body.len() as u64)
            .saturating_add(rule.distinct_pairs.len() as u64)
            .saturating_add(rule.builtins.len() as u64)
    })
}

fn frame(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn frame_str(hasher: &mut blake3::Hasher, value: &str) {
    frame(hasher, value.as_bytes());
}

fn hash_eval_term(hasher: &mut blake3::Hasher, term: &EvalTerm) {
    match term {
        EvalTerm::Var(name) => {
            hasher.update(&[0]);
            frame_str(hasher, name);
        }
        EvalTerm::ConstNamed(iri) => {
            hasher.update(&[1]);
            frame_str(hasher, iri);
        }
        EvalTerm::ConstLit(value) => {
            hasher.update(&[2]);
            frame_str(hasher, &term_display(value));
        }
    }
}

fn hash_eval_atom(hasher: &mut blake3::Hasher, atom: &EvalAtom) {
    hash_eval_term(hasher, &atom.subject);
    frame_str(hasher, &atom.predicate);
    hash_eval_term(hasher, &atom.object);
    hasher.update(&[u8::from(atom.negated)]);
}

fn hash_qterm(hasher: &mut blake3::Hasher, term: &QTerm) {
    match term {
        QTerm::Const(value) => {
            hasher.update(&[0]);
            frame_str(hasher, value);
        }
        QTerm::Var(name) => {
            hasher.update(&[1]);
            frame_str(hasher, name);
        }
        QTerm::Num(value) => {
            hasher.update(&[2]);
            hasher.update(&value.to_le_bytes());
        }
        QTerm::Struct(_) => {
            // G13: hashing an arena-local `NodeId::index()` into the compiled-plan cache
            // key would risk cross-arena collisions (two DIFFERENT structured terms from
            // unrelated `TermDag` arenas can share the same index). This arm is only
            // reachable through a `QBuiltin` operand ([`hash_builtin`]'s `target`/`lhs`/
            // `rhs`), and a `QBuiltin` operand is documented (`query_ir::QBuiltin`) to be
            // exclusively `Var`/`Num` — arithmetic never carries a compound
            // (function-symbol) term. No `TermDag` is threaded through
            // `canonical_rule_hash`/`EvalRule`/`EvalTerm` at all (that pipeline is the
            // flat rule-IR, distinct from the structured-term `physical::term_dag`
            // arena), so there is no content key (`TermDag::key`) available here to hash
            // instead — a genuinely-reachable `Struct` would need the caller to plumb a
            // `dag` through this entire module. Provably dead under every current
            // producer; a `Struct` reaching here would be an EvalRule construction bug
            // upstream, not a hashing decision this function should paper over.
            unreachable!(
                "canonical_rule_hash: QTerm::Struct in a QBuiltin operand — arithmetic \
                 builtin operands are Var/Num only (query_ir::QBuiltin); a Struct operand \
                 here is an upstream EvalRule construction bug, not a hashable term"
            );
        }
    }
}

fn hash_builtin(hasher: &mut blake3::Hasher, builtin: &QBuiltin) {
    match builtin {
        QBuiltin::Is {
            target,
            lhs,
            op,
            rhs,
        } => {
            hasher.update(&[0]);
            hash_qterm(hasher, target);
            hash_qterm(hasher, lhs);
            frame_str(hasher, op.token());
            hash_qterm(hasher, rhs);
        }
        QBuiltin::Compare { lhs, op, rhs } => {
            hasher.update(&[1]);
            hash_qterm(hasher, lhs);
            frame_str(hasher, op.token());
            hash_qterm(hasher, rhs);
        }
        QBuiltin::BilinearSqDist {
            target,
            gram,
            x,
            y,
        } => {
            hasher.update(&[2]);
            frame_str(hasher, "bilinearSqDist");
            hash_qterm(hasher, target);
            hash_qterm(hasher, gram);
            hash_qterm(hasher, x);
            hash_qterm(hasher, y);
        }
    }
}

/// Canonical, order-sensitive digest of every execution-relevant [`EvalRule`] field.
///
/// Every variable-length field is length-prefixed and every enum has an explicit tag;
/// this is not a display/debug hash. Rule, body, distinct-guard, and builtin order are
/// retained because all four are observable by deterministic execution/provenance.
pub(crate) fn canonical_rule_hash(rules: &[EvalRule]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    frame_str(&mut hasher, "gmeow-eval-rule-ir-v1");
    hasher.update(&(rules.len() as u64).to_le_bytes());
    for rule in rules {
        hash_eval_atom(&mut hasher, &rule.head);
        hasher.update(&(rule.body.len() as u64).to_le_bytes());
        for atom in &rule.body {
            hash_eval_atom(&mut hasher, atom);
        }
        frame_str(&mut hasher, &rule.rule_iri);
        hasher.update(&(rule.distinct_pairs.len() as u64).to_le_bytes());
        for (left, right) in &rule.distinct_pairs {
            frame_str(&mut hasher, left);
            frame_str(&mut hasher, right);
        }
        hasher.update(&(rule.builtins.len() as u64).to_le_bytes());
        for builtin in &rule.builtins {
            hash_builtin(&mut hasher, builtin);
        }
    }
    *hasher.finalize().as_bytes()
}

/// A per-rule precomputed join plan.
///
/// This is the store-independent relational-algebra plan: body partition, flat binding
/// frame, SIPS order, index and term-shape kernels, selective cyclic groups, and the swap
/// programs that restore authored provenance order. Store-local term IDs remain runtime
/// values, but which columns are bound and which concrete kernel consumes them are
/// decided here once.
pub(crate) struct RulePlan {
    /// Body indices of the POSITIVE atoms, in body order (the join drivers).
    positive: Box<[usize]>,
    /// Body indices of the NEGATED atoms, in body order (the NAF filters).
    negated: Box<[usize]>,
    /// Stable authored-first-occurrence variable names. The acyclic executor stores
    /// bindings in a flat `Vec<Option<String>>` indexed by these slots.
    variables: Box<[String]>,
    /// One statically-shaped atom operator per positive body atom, in physical SIPS
    /// order. Each retains its authored positive position for provenance restoration.
    /// Term kind dispatch, index shape, and constant rendering happen here, once.
    operators: Box<[AtomOperator]>,
    /// In-place swaps restoring positive-source provenance from physical execution
    /// order to authored body order.
    operator_source_order_swaps: Box<[(usize, usize)]>,
    /// Present only for a structurally-certified cyclic rule. Acyclic rules retain
    /// exactly two immutable slices and allocate no physical-group sidecar.
    hybrid: Option<Box<HybridPlan>>,
}

/// A positive atom lowered to a body coordinate plus one monomorphic term-shape
/// kernel. Runtime binding presence still selects an index (`Any`/subject/object/both),
/// but variable-name and term-enum interpretation is absent from the tuple loop.
#[derive(Debug)]
pub(crate) struct AtomOperator {
    body_index: usize,
    positive_position: usize,
    index: IndexChoice,
    kernel: AtomKernel,
}

impl AtomOperator {
    pub(crate) fn body_index(&self) -> usize {
        self.body_index
    }

    pub(crate) fn positive_position(&self) -> usize {
        self.positive_position
    }

    pub(crate) fn index(&self) -> IndexChoice {
        self.index
    }

    pub(crate) fn kernel(&self) -> &AtomKernel {
        &self.kernel
    }
}

/// Guaranteed index shape at this point in the planned SIPS order. Runtime values are
/// resolved to store-local term IDs, but the bound columns are a plan-time property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IndexChoice {
    Any,
    Subject,
    Object,
    Both,
}

/// Statically selected subject/object term shape for one binary atom.
#[derive(Debug)]
pub(crate) enum AtomKernel {
    Vars {
        subject_slot: usize,
        object_slot: usize,
    },
    VarConst {
        subject_slot: usize,
        object: String,
    },
    ConstVar {
        subject: String,
        object_slot: usize,
    },
    Consts {
        subject: String,
        object: String,
    },
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
    /// The same deterministic order lowered to the rule's flat binding slots.
    variable_slots: Vec<usize>,
}

impl CyclicPlan {
    pub(crate) fn atoms(&self) -> &[PlannedAtom] {
        &self.atoms
    }

    pub(crate) fn variables(&self) -> &[String] {
        &self.variables
    }

    pub(crate) fn variable_slots(&self) -> &[usize] {
        &self.variable_slots
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

        // Assign flat slots in authored first-occurrence order across the body, then
        // the head. Body-first preserves the join's natural binding order; including
        // the head gives diagnostics/tests a complete rule layout without changing the
        // positive operators. Builtin-generated variables remain a post-join concern.
        let mut variables = Vec::new();
        let mut slots = BTreeMap::new();
        for atom in rule.body.iter().chain(std::iter::once(&rule.head)) {
            for term in [&atom.subject, &atom.object] {
                if let EvalTerm::Var(name) = term
                    && !slots.contains_key(name)
                {
                    let slot = variables.len();
                    variables.push(name.clone());
                    slots.insert(name.clone(), slot);
                }
            }
        }
        // A simple undirected cycle requires at least three positive edges. Avoid all
        // graph/planned-atom scratch for the overwhelmingly-common 0/1/2-atom rules.
        let cyclic = if positive.len() < 3 {
            Vec::new()
        } else {
            certified_cyclic_components(rule, &positive, &slots)
        };

        if cyclic.is_empty() {
            let execution_order = sips_order(rule, &positive);
            let operators = lower_operators(rule, &positive, &execution_order, &slots);
            let operator_source_order_swaps = restore_body_order_swaps(&execution_order);
            return Self {
                positive: positive.into_boxed_slice(),
                negated: negated.into_boxed_slice(),
                variables: variables.into_boxed_slice(),
                operators: operators.into_boxed_slice(),
                operator_source_order_swaps: operator_source_order_swaps.into_boxed_slice(),
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
        let operators = lower_operators(rule, &positive, &execution_source_order, &slots);
        let operator_source_order_swaps = restore_body_order_swaps(&execution_source_order);
        Self {
            positive: positive.into_boxed_slice(),
            negated: negated.into_boxed_slice(),
            variables: variables.into_boxed_slice(),
            operators: operators.into_boxed_slice(),
            operator_source_order_swaps: operator_source_order_swaps.into_boxed_slice(),
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

    /// Stable slot-to-variable table for the rule's physical binding frame.
    pub(crate) fn variables(&self) -> &[String] {
        &self.variables
    }

    /// Prelowered positive atom operators, in physical SIPS/group order.
    pub(crate) fn operators(&self) -> &[AtomOperator] {
        &self.operators
    }

    /// The operator for one authored positive-body coordinate.
    pub(crate) fn operator_at(&self, positive_position: usize) -> &AtomOperator {
        self.operators
            .iter()
            .find(|operator| operator.positive_position == positive_position)
            .expect("every positive atom has exactly one physical operator")
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
    pub(crate) fn operator_source_order_swaps(&self) -> &[(usize, usize)] {
        &self.operator_source_order_swaps
    }

    /// Source restoration for the cyclic group's physical execution order.
    pub(crate) fn hybrid_source_order_swaps(&self) -> &[(usize, usize)] {
        &self
            .hybrid
            .as_ref()
            .expect("hybrid source swaps exist only for a cyclic plan")
            .source_order_swaps
    }
}

fn term_is_known(term: &EvalTerm, bound: &BTreeSet<String>) -> bool {
    match term {
        EvalTerm::Var(variable) => bound.contains(variable),
        EvalTerm::ConstNamed(_) | EvalTerm::ConstLit(_) => true,
    }
}

fn bind_atom_variables(atom: &EvalAtom, bound: &mut BTreeSet<String>) {
    for term in [&atom.subject, &atom.object] {
        if let EvalTerm::Var(variable) = term {
            bound.insert(variable.clone());
        }
    }
}

/// Deterministic sideways-information-passing order for an acyclic positive body.
///
/// With no store yet, cardinalities cannot be consulted soundly. The static information
/// available at plan time is still valuable: prefer atoms with more constant/already-
/// bound columns, then constants, then repeated-variable equality, and finally authored
/// position. After choosing an atom, all of its variables become bound for subsequent
/// choices. This is store-independent and byte-stable.
fn sips_order(rule: &EvalRule, positive: &[usize]) -> Vec<usize> {
    let mut remaining: Vec<usize> = (0..positive.len()).collect();
    let mut bound = BTreeSet::new();
    let mut order = Vec::with_capacity(positive.len());
    while !remaining.is_empty() {
        let (slot, &positive_position) = remaining
            .iter()
            .enumerate()
            .max_by_key(|&(_, &positive_position)| {
                let atom = &rule.body[positive[positive_position]];
                let known = usize::from(term_is_known(&atom.subject, &bound))
                    + usize::from(term_is_known(&atom.object, &bound));
                let constants = usize::from(!matches!(atom.subject, EvalTerm::Var(_)))
                    + usize::from(!matches!(atom.object, EvalTerm::Var(_)));
                let repeated = usize::from(matches!(
                    (&atom.subject, &atom.object),
                    (EvalTerm::Var(left), EvalTerm::Var(right)) if left == right
                ));
                (known, constants, repeated, usize::MAX - positive_position)
            })
            .expect("a non-empty remaining set has a best SIPS atom");
        remaining.remove(slot);
        order.push(positive_position);
        bind_atom_variables(&rule.body[positive[positive_position]], &mut bound);
    }
    order
}

fn index_choice(atom: &EvalAtom, bound: &BTreeSet<String>) -> IndexChoice {
    match (
        term_is_known(&atom.subject, bound),
        term_is_known(&atom.object, bound),
    ) {
        (false, false) => IndexChoice::Any,
        (true, false) => IndexChoice::Subject,
        (false, true) => IndexChoice::Object,
        (true, true) => IndexChoice::Both,
    }
}

fn lower_operators(
    rule: &EvalRule,
    positive: &[usize],
    execution_order: &[usize],
    slots: &BTreeMap<String, usize>,
) -> Vec<AtomOperator> {
    let mut bound = BTreeSet::new();
    let mut operators = Vec::with_capacity(execution_order.len());
    for &positive_position in execution_order {
        let body_index = positive[positive_position];
        let atom = &rule.body[body_index];
        operators.push(AtomOperator {
            body_index,
            positive_position,
            index: index_choice(atom, &bound),
            kernel: atom_kernel(atom, slots),
        });
        bind_atom_variables(atom, &mut bound);
    }
    operators
}

fn constant_surface(term: &EvalTerm) -> String {
    match term {
        EvalTerm::ConstNamed(iri) => format!("<{iri}>"),
        EvalTerm::ConstLit(value) => term_display(value),
        EvalTerm::Var(_) => unreachable!("constant_surface called only for a constant term"),
    }
}

fn atom_kernel(atom: &EvalAtom, slots: &BTreeMap<String, usize>) -> AtomKernel {
    match (&atom.subject, &atom.object) {
        (EvalTerm::Var(subject), EvalTerm::Var(object)) => AtomKernel::Vars {
            subject_slot: slots[subject],
            object_slot: slots[object],
        },
        (EvalTerm::Var(subject), object) => AtomKernel::VarConst {
            subject_slot: slots[subject],
            object: constant_surface(object),
        },
        (subject, EvalTerm::Var(object)) => AtomKernel::ConstVar {
            subject: constant_surface(subject),
            object_slot: slots[object],
        },
        (subject, object) => AtomKernel::Consts {
            subject: constant_surface(subject),
            object: constant_surface(object),
        },
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
fn certified_cyclic_components(
    rule: &EvalRule,
    positive: &[usize],
    slots: &BTreeMap<String, usize>,
) -> Vec<CyclicPlan> {
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
        let variable_slots = variables.iter().map(|variable| slots[variable]).collect();
        plans.push(CyclicPlan {
            atoms,
            variables,
            variable_slots,
        });
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
/// Owns the rules behind an [`Arc`], so the terminal executable can be cached and shared
/// across calls without borrowing a parser/query scratch buffer.
pub(crate) struct Parsed {
    rules: Arc<[EvalRule]>,
    identity: PlanIdentity,
}

impl Parsed {
    /// Enter the uncached pipeline with a parsed rule program.
    ///
    /// This explicit one-shot constructor is for focused tests and genuinely uncached
    /// internal evaluations. Production repeated-evaluation boundaries use
    /// [`compile_cached`] with their real reasoning/query contract hash.
    pub(crate) fn uncached(rules: &[EvalRule]) -> Self {
        let identity = PlanIdentity::new("gmeow-uncached-plan", rules);
        Self::from_owned(identity, rules.to_vec())
    }

    fn from_owned(identity: PlanIdentity, rules: Vec<EvalRule>) -> Self {
        Self {
            rules: Arc::from(rules),
            identity,
        }
    }

    /// Compute the stratification ONCE and lower it into the per-stratum rule grouping.
    ///
    /// `None` ⇒ the program is non-stratifiable (a negative dependency-graph edge inside
    /// a cycle): a declared gap. The caller may use the sound native untransformed-base
    /// path where defined, or surface `Unsupported(NonStratifiable)`; no external engine
    /// fallback exists.
    pub(crate) fn stratify(self) -> Option<Stratified> {
        let stratum_of = stratify(&self.rules)?;

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
            identity: self.identity,
            strata,
        })
    }
}

/// Stage 2: a stratifiable program with its per-stratum rule grouping memoized.
pub(crate) struct Stratified {
    rules: Arc<[EvalRule]>,
    identity: PlanIdentity,
    /// `strata[k]` = the program-order indices (into `rules`) of stratum `k`'s rules.
    strata: Vec<Vec<usize>>,
}

impl Stratified {
    /// Precompute one complete store-independent [`RulePlan`] per rule, yielding the
    /// [`Planned`] stage.
    pub(crate) fn plan(self) -> Planned {
        let plans: Vec<RulePlan> = self.rules.iter().map(RulePlan::for_rule).collect();
        Planned {
            rules: self.rules,
            identity: self.identity,
            strata: self.strata,
            plans,
        }
    }
}

/// Stage 3: a stratified program with its per-rule join plans memoized.
pub(crate) struct Planned {
    rules: Arc<[EvalRule]>,
    identity: PlanIdentity,
    strata: Vec<Vec<usize>>,
    /// One entry per rule, parallel to `rules` by index.
    plans: Vec<RulePlan>,
}

impl Planned {
    /// Seal the plan into the terminal [`Executable`] — the ONLY type the semi-naive
    /// executor accepts.  Memoizes the head-predicate set the completion frontier reads.
    pub(crate) fn into_executable(self) -> Executable {
        let head_predicates: BTreeSet<String> = self
            .rules
            .iter()
            .map(|r| r.head.predicate.as_str().to_owned())
            .collect();
        Executable {
            rules: self.rules,
            identity: self.identity,
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
pub(crate) struct Executable {
    rules: Arc<[EvalRule]>,
    identity: PlanIdentity,
    strata: Vec<Vec<usize>>,
    plans: Vec<RulePlan>,
    head_predicates: BTreeSet<String>,
}

impl Executable {
    /// The immutable content identity under which this plan was compiled.
    pub(crate) fn identity(&self) -> &PlanIdentity {
        &self.identity
    }

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

    /// The program-order rule indices assigned to stratum `k`.
    ///
    /// Exposing the immutable index slice lets the executor use Rayon's indexed
    /// parallel iterator while preserving program order at the deterministic merge
    /// boundary. The indices remain an implementation detail of this executable;
    /// callers resolve them through [`rule_entry`](Self::rule_entry).
    pub(crate) fn stratum_rule_indices(&self, k: usize) -> &[usize] {
        &self.strata[k]
    }

    /// Resolve one executable rule index to its immutable rule and precomputed plan.
    pub(crate) fn rule_entry(&self, index: usize) -> (&EvalRule, &RulePlan) {
        (&self.rules[index], &self.plans[index])
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
    use crate::physical::term_dag::TermDag;
    use crate::query_ir::StructNode;

    /// A minimal single-fact `EvalRule` (`rule_iri` "r") carrying one `QBuiltin::Compare`
    /// whose `lhs` operand is `term` — the shape `canonical_rule_hash`'s `hash_qterm`
    /// dispatches on.
    fn rule_with_builtin_operand(term: QTerm) -> EvalRule {
        let mut rule = EvalRule::positive(
            "https://example.org/r",
            EvalAtom::positive(
                EvalTerm::var("?X"),
                "https://example.org/p",
                EvalTerm::var("?X"),
            ),
            Vec::new(),
        );
        rule.builtins.push(QBuiltin::Compare {
            lhs: term,
            op: crate::query_ir::CmpOp::Eq,
            rhs: QTerm::Num(0),
        });
        rule
    }

    /// G13 lock: `canonical_rule_hash`'s flat rule-IR pipeline never threads a `TermDag` (it
    /// is a distinct, arena-free pipeline from the structured-term `physical::term_dag`
    /// world), so a `QTerm::Struct` reaching `hash_qterm` cannot be content-hashed by
    /// `TermDag::key` here — hashing its arena-local `NodeId::index()` instead would risk a
    /// false collision between two DIFFERENT structured terms from unrelated arenas that
    /// happen to share a raw index. `hash_qterm`'s `QTerm::Struct` arm is therefore a hard
    /// `unreachable!` (never a silent index hash), justified by `QBuiltin`'s own contract
    /// (`query_ir::QBuiltin` operands are documented `Var`/`Num` only — arithmetic never
    /// carries a compound term) rather than papered over.
    ///
    /// This test proves the arm's chosen failure mode DIRECTLY: two independently-built
    /// `TermDag`s each intern one leaf node first, so their first `NodeId`s share
    /// `index() == 0` by construction — exactly the collision a naive `NodeId::index()`
    /// hash would silently forge into an equal digest for two DIFFERENT structured terms.
    /// Wrapping each into a `QBuiltin` operand and hashing the enclosing rule must instead
    /// PANIC (the documented `unreachable!` firing), never silently succeed with a
    /// forged-equal (or any) hash.
    #[test]
    fn canonical_rule_hash_hard_fails_on_a_struct_builtin_operand_rather_than_hashing_arena_index()
    {
        let mut dag_a = TermDag::new();
        let leaf_a = dag_a.intern_leaf(purrdf::TermValue::iri("https://example.org/a"));
        let mut dag_b = TermDag::new();
        let leaf_b = dag_b.intern_leaf(purrdf::TermValue::iri("https://example.org/b"));
        assert_eq!(
            leaf_a.index(),
            leaf_b.index(),
            "two independently-built arenas' first interned node share the same raw index \
             — exactly the collision NodeId::index() hashing would silently forge"
        );

        let struct_a = QTerm::Struct(StructNode::new(leaf_a, dag_a.arena()));
        let struct_b = QTerm::Struct(StructNode::new(leaf_b, dag_b.arena()));

        let rule_a = rule_with_builtin_operand(struct_a);
        let rule_b = rule_with_builtin_operand(struct_b);

        let result_a = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            canonical_rule_hash(std::slice::from_ref(&rule_a))
        }));
        assert!(
            result_a.is_err(),
            "a Struct QBuiltin operand must hard-panic canonical_rule_hash, never silently \
             hash a raw NodeId::index()"
        );
        let result_b = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            canonical_rule_hash(std::slice::from_ref(&rule_b))
        }));
        assert!(
            result_b.is_err(),
            "the SAME guard must fire for the second (index-colliding, different-arena) \
             Struct rule — never silently forging an equal hash for two distinct terms"
        );
    }
}
