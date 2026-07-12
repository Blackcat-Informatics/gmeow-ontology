// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native physical execution core.
//!
//! The destination is one native Rust engine. Nemo remains a temporary forward
//! comparison oracle; oxigraph remains a storage compatibility layer. This module hosts the engine's working
//! representation — starting with the columnar [`RelationStore`] and the single
//! oxigraph → columnar bridge [`extract_edb`].
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
// The arrangement's native galloping lending cursor: a sealed
// GAT `LendingIterator` over row-id-ordered runs, replacing the materialized per-stage
// `Vec<(TermId, TermId, RowId)>` on the semi-naive join hot path (`seminaive`, `chase`).
mod cursor;
mod generic;
// Branded niche IDs for every engine entity class (`TermId`/`PredId`/`RuleId`/
// `RowId`). `pub(crate)` so `crate::facts` can re-express its `TermId` as this
// module's `Id<Term>` alias (one definition, not two — greenfield).
pub(crate) mod id;
mod magic;
mod magic_generic;
mod parity;
// The consuming type-state plan pipeline: `Parsed → Stratified →
// Planned → Executable`. Makes an unstratified/unplanned program unrepresentable at the
// semi-naive executor boundary and memoizes the stratification + per-rule join partition.
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
pub(crate) use generic::{
    GenericRule, lower_program_generic_rules, materialize_generic, parse_generic_rules,
};

// Phase-A: these are the engine's public-to-crate surface, consumed by the
// forward/backward evaluators landing on the next rung. Until then the re-export is
// unused crate-wide, so allow it here rather than dropping the intended API.
#[allow(unused_imports)]
pub(crate) use store::{Bound, RelationStore, extract_edb};

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
    Budgeted, NativeOutcome, UnsupportedKind, evaluate, materialize_native,
};

// The type-state plan pipeline: the executor's entry-gate types. `Parsed` is the sole
// entry; `Executable` is the sole type the forward/backward evaluators accept. The
// intermediate `Stratified`/`Planned` are re-exported so a caller can name a stage if it
// chooses, though the fluent `Parsed::new(..).stratify()?.plan().into_executable()` chain
// never needs to.
#[allow(unused_imports)]
pub(crate) use plan::{Executable, Parsed, Planned, Stratified};

// The native restricted (standard) existential-rule chase: value invention for the
// existential fragment, admitted by the `ChaseAdmission` termination certificate and
// consumed by `materialize::materialize_routed`.
#[allow(unused_imports)]
pub(crate) use chase::{
    ExistentialRule, chase_materialize, chase_world, parse_existential_rules, route_chase,
};
// The termination certificate is surfaced PUBLICLY (re-exported through the public
// `materialize` module below) so callers can read the chase's weak-acyclicity certificate
// and its `to_finding()` gmeow:Finding off a `materialize_routed` result.
pub use chase::ChaseAdmission;

// The backward native evaluator: magic-sets demand transformation +
// `resolve_native`, the oracle-parity sibling of `reference_resolver::resolve`. The primary
// backward path consumed by `dispatch::dispatch_query`.
pub(crate) use magic::resolve_native;

// The shared moded builtin evaluator: one arithmetic/comparison semantics called
// by every native engine. `emit_integer_surface` is the single canonical
// computed-value surface, reused by dispatch and reference emitters so
// byte-identity is by construction. `eval_builtin`/`BuiltinOutcome`/`BuiltinError`
// are consumed by the forward/backward evaluators wired on the next rungs.
#[allow(unused_imports)]
pub(crate) use builtin_eval::{
    BuiltinError, BuiltinOutcome, XSD_INTEGER, emit_integer_surface, eval as eval_builtin,
};
