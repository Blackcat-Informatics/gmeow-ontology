// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native physical execution core.
//!
//! This module hosts the native engine's working representation — starting with
//! the columnar [`RelationStore`] and the single oxigraph → columnar bridge
//! [`extract_edb`].
//!
//! # Phase dead code
//!
//! Like [`crate::rule_ir`], the early rungs of this engine land before the
//! forward/backward evaluators that consume them, so the not-yet-wired surface
//! allows `dead_code` module-internally rather than scattering per-item attributes
//! that would be unwound the next rung.
#![allow(dead_code)]

// The phase-scoped row/tuple bump arena: a genuinely resettable
// per-round argument-tuple buffer, distinct from the persistent term arena
// (`facts::TermInterner`). Consumed by the semi-naive fixpoint (`seminaive`).
mod annotation;
mod arena;
mod binding_pattern;
// The dense `u64`-word delta bitset: row-id membership over the
// phase-scoped value column. It is the delta probe of the semi-naive fixpoint
// (`seminaive`) and the ternary reduct engine (`crate::rule_ir`) — one word test per
// selected row, no hashing. `pub(crate)` (like `id`) so the sibling `rule_ir` module
// reuses the SAME `DenseBitset` for its own row-index delta (one definition, greenfield).
pub(crate) mod bitset;
mod builtin_eval;
mod chase;
// The arrangement's native galloping lending cursor: a sealed GAT `LendingIterator`
// that concatenates each sorted batch's galloped bound-run with a tail scan, replacing
// the materialized per-stage `Vec<(TermId, TermId, RowId)>` on the semi-naive join hot
// path (`seminaive`, `chase`).
mod cursor;
mod generic;
// The signed-batch / nested-iteration incremental circuit for the finite positive
// binary Datalog fragment. It owns recursive insert/retract maintenance and is the
// stateful sibling of the scratch `seminaive` evaluator.
mod incremental;
// Incremental grounding for the non-monotone binary fragment.  It reuses the
// signed positive-Datalog session for the candidate universe, then differentiates
// each grounding join while deliberately leaving WFS / stable-model solving on
// its named from-scratch boundary.
mod incremental_grounding;
// Branded niche IDs for every engine entity class (`TermId`/`PredId`/`RuleId`/
// `RowId`). `pub(crate)` so `crate::facts` can re-express its `TermId` as this
// module's `Id<Term>` alias (one definition, not two — greenfield).
pub(crate) mod id;
mod magic;
// The persistent hash-consed structured-term DAG: content-addressed, binder-aware
// (locally-nameless de-Bruijn) function-symbol / proof-object nodes. It grows the
// `facts::TermInterner` seam that `id::TermRef` documents; distinct from the per-round
// `arena::RowArena`. The DAG (`term_dag`) and its content-key fold (`term_key`) land
// ahead of the unification / proof-object consumers on the next rungs.
mod magic_generic;
pub(crate) mod term_dag;
pub(crate) mod term_key;
// The three-consumer lowering into the shared `term_dag::TermDag`: `logic:`
// (`gmeow_logic_compile::ir::Formula`/`Term`), `math:` (the RDF-authored
// application/binding expression vocab), and `lang:` (a form + its one-way
// `lang:`→`logic:` denotation) all lower into ONE arena, so alpha-equivalent
// inputs authored in any surface intern to the SAME `NodeId` and content key.
// Consumed by the unification / proof-object rungs to come.
pub(crate) mod lower;
// Robinson unification with occurs-check over the `term_dag::TermDag`: a union-find
// `Subst` over `MetaId`, the single `resolve` identity primitive, and capture-avoiding
// `apply`/`shift` (locally-nameless de-Bruijn, so the shift IS the capture-avoidance).
// Consumed by the proof-object / backward-FOL rungs to come.
pub(crate) mod unify;
// First-class CHECKABLE proof objects: a proof IS a `term_dag::TermDag` node
// (`by_rule`/`assert` constructors), and `check` re-derives it bottom-up via `unify`/`apply`
// (the de-Bruijn/Curry-Howard criterion), rejecting any proof that does not prove its stated
// goal. `derivation_iri`/`reify` project a proof/term node to the SAME content-addressed
// provenance IRI a `RuleApplication` mints. Consumed by the backward-FOL rung to come.
pub(crate) mod proof;
// The structured (full-FOL) backward resolver: SLG tabling over compound (function-symbol)
// terms with three-valued SLG-WFS well-founded negation. It stands on `term_dag` (the
// hash-consed arena), `unify` (order-sorted occurs-checked unification), and `proof`
// (checkable proof objects), and every answer it yields is proof-carrying. `dispatch::
// dispatch_query`'s physical entry (`magic::resolve_native_under`) routes a program carrying
// any `QTerm::Struct` argument here; flat programs stay on the byte-identical binary path.
pub(crate) mod resolve_fol;
// The consuming type-state plan pipeline: `Parsed → Stratified →
// Planned → Executable`. Makes an unstratified/unplanned program unrepresentable at the
// semi-naive executor boundary and memoizes the content-addressed owned RA plan: strata,
// flat slots, SIPS/index/kernel choices, and selective cyclic groups.
mod plan;
mod seminaive;
mod store;

// The arity-generic binding-pattern adornment lattice: the shared demand/query-plan
// adornment consumed by both the backward magic-sets keying (`magic`) and the forward
// generic evaluator's index selection (`generic`).
#[allow(unused_imports)]
pub(crate) use binding_pattern::BindingPattern;

// The arity-generic positive-Datalog forward evaluator: the predicate-as-data n-ary
// core the OWL 2 RL/RDF meta-rules need (variable property position). Consumed by
// `crate::oracle::NativeForwardOracle` for the generic-triple encoding; the binary
// EL/DL path stays on `seminaive`. `#[cfg(test)]`-gated coverage lives in the module.
#[allow(unused_imports)]
pub(crate) use generic::{GenericAtom, GenericRule, materialize_generic};

#[allow(unused_imports)]
pub(crate) use incremental::{
    BudgetedIncrementalDelta, IncrementalDelta, IncrementalDerivation, IncrementalIdentity,
    IncrementalSession, SignedFact,
};
#[allow(unused_imports)]
pub(crate) use incremental_grounding::{
    GroundProgramSnapshot, GroundRuleChange, GroundingUpdate, IncrementalGroundProgram,
};

// Phase-A: these are the engine's public-to-crate surface, consumed by the
// forward/backward evaluators landing on the next rung. Until then the re-export is
// unused crate-wide, so allow it here rather than dropping the intended API.
#[allow(unused_imports)]
pub(crate) use store::{Bound, RelationStore, SkolemRegistry, extract_edb};

// The decomposable derivation of one chase-invented null (firing rule, existential
// ordinal, frontier binding). Public so the pipeline can project it into the shipped
// diagnostics graph and the CLI/playground can explain an invented individual.
pub use store::WitnessDerivation;

// The arrangement's native lending cursor + its sealed GAT trait: the zero-alloc row
// scan consumed by the semi-naive join (`seminaive`) and the chase (`chase`).
#[allow(unused_imports)]
pub(crate) use cursor::{LendingIterator, RowCursor};

// The forward native evaluator: the stratified semi-naive core, its
// `RelationStore`-seeded backward entry, and the declared-gap outcome. `materialize_native`
// + `NativeOutcome` are the primary forward path consumed by `materialize::materialize_routed`;
// `evaluate`/`UnsupportedKind` are consumed by the backward `magic` leg.
#[allow(unused_imports)]
pub(crate) use seminaive::{
    Budgeted, NativeOutcome, RuleParallelProbe, UnsupportedKind, evaluate, materialize_native,
    rule_parallel_probe,
};

pub(crate) use annotation::{
    AnnotationExecution, PhysicalAnnotationDerivation, certify_query, evaluate_annotations,
};

// The type-state plan pipeline: the executor's entry-gate types. `Parsed` is the sole
// entry; `Executable` is the sole type the forward/backward evaluators accept. The
// intermediate `Stratified`/`Planned` are re-exported so a caller can name a stage if it
// chooses, though the fluent `Parsed::uncached(..).stratify()?.plan().into_executable()` chain
// never needs to.
#[allow(unused_imports)]
pub(crate) use plan::{
    Executable, Parsed, PlanCache, Planned, Stratified, canonical_rule_hash, compile_cached,
};

// The native restricted (standard) existential-rule chase: value invention for the
// existential fragment, admitted by the `ChaseAdmission` termination certificate and
// consumed by `materialize::materialize_routed`.
#[allow(unused_imports)]
pub(crate) use chase::{
    ExistentialRule, WitnessPolicy, chase_materialize, chase_world, route_chase,
    route_chase_with_registry,
};
// The termination certificate is surfaced PUBLICLY (re-exported through the public
// `materialize` module below) so callers can read the chase's weak-acyclicity certificate
// and its `to_finding()` gmeow:Finding off a `materialize_routed` result.
pub use chase::ChaseAdmission;

// The backward native evaluator: magic-sets demand transformation +
// `resolve_native`, the oracle-parity sibling of `reference_resolver::resolve`. The primary
// backward path consumed by `dispatch::dispatch_query`.
#[cfg(test)]
pub(crate) use magic::resolve_native;
pub(crate) use magic::resolve_native_annotated_under;
pub(crate) use magic::{IncrementalQuerySession, prepare_incremental_query, resolve_native_under};

// The shared moded builtin evaluator: one arithmetic/comparison semantics called
// by every native engine. `emit_integer_surface` is the single canonical
// computed-value surface, reused by dispatch and reference emitters so
// byte-identity is by construction. `eval_builtin`/`BuiltinOutcome`/`BuiltinError`
// are consumed by the forward/backward evaluators wired on the next rungs.
#[allow(unused_imports)]
pub(crate) use builtin_eval::{
    BuiltinError, BuiltinOutcome, XSD_INTEGER, emit_integer_surface, eval as eval_builtin,
};
