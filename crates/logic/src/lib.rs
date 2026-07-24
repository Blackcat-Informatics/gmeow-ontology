// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
#![feature(portable_simd)]
#![doc = include_str!("../README.md")]
//!
//! This crate is single-target native only.

pub mod annotation;
pub mod certificate;
/// Conjecture-and-refutation runtime: [`conjecture::conjecture_test`] tests a candidate
/// first-order formula against a KB in an isolated, standpoint-scoped scenario world.
pub mod conjecture;
/// Executed lens-law discharge for a `logic:Correspondence`'s realized `LegPath` legs —
/// the per-correspondence section-law verdict the (execution-free) correspondence gates read.
pub mod correspondence_exec;
pub mod cost;
/// Deterministic native engine-benchmark seams and the decomposable
/// [`cost::CostVector`] — the
/// scalar-projection carrier of the cost semiring (`LOGIC-PERFORMANCE.md
/// §Measurement doctrine`) keyed by `(rule, predicate, stratum)`.
pub mod counterfactual;
/// The DAG-workflow profile certifier (`logic:DagWorkflowResource`): the single
/// shared acyclicity check the canonical process model and the build pipeline run.
pub mod dag_profile;
/// Dense-id graph primitives (interner + bitset) for the hot graph algorithms.
pub(crate) mod dense;
pub mod derivation_graph;
pub mod dispatch;
/// Native entailment by refutation (`A ⊨ C` iff `A ∪ ¬C` inconsistent): a thin
/// composition over [`reason::dl_consistency`] plus the shared conclusion-shape
/// negation calculus with sound reserved-namespace minting. Lives OUTSIDE `reason`
/// so it is not folded into `reason::native_contract_hash` (it adds no rule).
pub mod entail;
pub mod entrenchment;
/// Reasoning-core diagnostic-kind catalog: the typed [`gmeow_errors::DiagKind`]
/// set the core raises on the shared diagnostic substrate, one per subsystem.
pub mod error;
pub mod explain;
/// Query-scoped annotated external relations and their deterministic receipts.
pub mod external_relation;
// The typed-fact bridge: dictionary-interned facts (TermInterner / TypedFactSet)
// exchanged between the store sweep and the reasoning adapters. Crate-internal.
pub(crate) mod facts;
pub mod foundation;
/// The goal-directed (backward) demonstrator façade — the single thin `pub` surface over
/// the proof-carrying full-FOL backward engine (`crate::physical::resolve_fol` +
/// `crate::physical::proof::check`), evaluating shipped structured demonstrators into
/// proof-checked answers the pipeline folds into `graph/goal-directed` of `gmeow.gts`.
pub mod goal_directed;
// Runtime-side projection of compiler parse diagnostics into the PyO3-tainted
// gmeow-errors Report — kept out of the wasm-able compiler crate.
pub mod logic_diagnostics;
// Compiler-IR → runtime EvalRule bridge: depends on crate::rule_ir,
// so it stays in the runtime crate, not the wasm-able gmeow-logic-compile crate.
pub mod lower;
pub mod materialize;
// The math: measure-and-dimension reasoned-graph gate — dimensional homogeneity,
// integral composition, math:dimensionVector drift, and Gram positive-definiteness,
// all computed through the exact-rational (ℚ⁷) gmeow_math source at reason-verify
// speed. Runs alongside the obligation checks in `verify`.
pub mod math_dimension;
// Fixed-arity n-ary predication → reified-binary lowering + the native n-ary
// forward-chase ingestion entry. The reified encoding (`logic:instanceOf` /
// `logic:naryArg{i}` over a content-addressed reifier) keeps `EvalAtom` binary,
// per LOGIC-IR.md §RelationalCore.
pub mod nary;
pub mod obligations;
// Native physical execution core: columnar RelationStore + the semi-naive / magic-sets
// engine that the materialize and dispatch routers invoke native-first. Crate-internal.
mod physical;
/// The native bilinear-form distance authority: the exact-ℚ squared-distance builtin
/// `(x−y)ᵀG(x−y)` and its overflow-safe ordering, exposed so external crates
/// (gmeow-affect's nearest-prototype classifier) compute Q9 metric
/// distances THROUGH the governed moded-builtin family rather than a private path.
pub use physical::{BilinearFormError, bilinear_sqdist, compare_sqdist};
pub mod probabilistic;
pub mod profile_gate;
pub mod provenance;
/// Verified PURREMB external-relation provider: a query-scoped nearest-neighbour relation
/// over a fully verified embedding artifact, exposing retrieved RDF 1.2 identities to the
/// native annotated relational evaluator as derived query inputs.
pub mod purremb_relation;
pub mod query_ir;
pub mod reason;
/// The shared named-graph boundary of the object-level reasoning EDB.
pub mod reasoning_graphs;
pub mod reference_resolver;
pub mod relational_core;
pub mod result;
pub mod result_rdf;
/// The typed `logic:ResultShape` lives in the runtime-free `gmeow-logic-compile`
/// crate (alongside `LOGIC_NAMESPACE`/`PreservationKind`) so pure-data consumers
/// — notably the slice-test harness — can use it without pulling in the reasoner;
/// re-exported here as `gmeow_logic::result_shape` for the result family.
pub use gmeow_logic_compile::result_shape;
pub mod rule_ir;
/// The supported, pin-able runtime query surface for an external runtime consumer:
/// a curated projection of the store → snapshot → dispatch → result chain plus the
/// self-describing [`runtime::EngineContract`] runtime pin. Stability is delivered
/// consumer-side (git-tag/vendor + the content-addressed contract), never as a
/// backwards-compat freeze of the churning core.
pub mod runtime;
pub mod seam;
pub mod slme;
pub mod stablemodel;
pub mod store;
/// Synthetic relational-core Datalog generators (transitive closure, SCC, same
/// generation, reachability) for the engine benchmark harness: each returns
/// `(rules, edb, expected_rows)` with an analytically-known golden. Shared by the
/// in-crate benches and the `gmeow-conformance` bench-corpus loader.
pub mod synth_corpus;
pub mod teleology;
mod term_codec;
/// Termination-class ladder demonstrators shipped into `gmeow.gts` (one general
/// existential program per broader chase-termination class, each in its own world).
pub mod termination_demonstrators;
pub mod transaction;
pub mod transition;
pub mod verify;
pub mod versioning;
pub mod wellfounded;
// The intra-engine phase descriptor of the well-founded materializer — the
// runtime twin the dogfood parity gate checks the authored
// `logic:wellFoundedMaterializerPlan` against (Principle 12).
pub use wellfounded::{WELL_FOUNDED_ITERATED_PHASE, WELL_FOUNDED_PHASES};

// The reasoning-oracle boundary: Forward/Backward oracle traits + engine adapters.
pub(crate) mod oracle;

// Static profile / decidability certifier.
pub mod certify;
