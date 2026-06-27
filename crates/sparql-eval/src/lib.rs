// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native, RDF-1.2-first **multiset SPARQL evaluator** (purrdf S6, EPIC #906).
//!
//! This crate is the evaluation runtime that consumes the
//! [`gmeow_sparql_algebra`] front-end (S5, #911) and evaluates it over the
//! [`gmeow_rdf_core`] IR's [`DatasetView`](gmeow_rdf_core::DatasetView) read trait
//! **entirely in interned [`TermId`](gmeow_rdf_core::TermId) space**. It is the
//! native replacement for the oxigraph-family `spareval` on the query path and
//! the single required impl of the
//! [`SparqlEngine`](gmeow_rdf_core::SparqlEngine) seam (#887).
//!
//! ## Design pillars
//!
//! - **TermId hot path.** Basic-graph-pattern matching and joins never leave
//!   interned-id space: constants resolve to a dataset [`TermId`] once (via
//!   `term_id_by_value`, P4 #838) and solutions carry [`SolutionTerm`]s that are a
//!   single integer compare apart. Computed terms (FILTER/BIND results not already
//!   in the dataset) are interned in a per-query scratch table — but a computed
//!   value that *does* exist in the dataset is **promoted** to
//!   [`SolutionTerm::Existing`] at mint time, so cross-case join keys are unequal
//!   purely by construction (no structural fallback at join time). See
//!   [`scratch`].
//! - **Multiset (bag) semantics.** Solutions are a bag, preserved until
//!   `DISTINCT`/`REDUCED`. See [`solution`].
//! - **Property paths in-engine (S8 #914).** The `Path` graph pattern is evaluated
//!   over the same indexed surface, wasm-safe, covering the full algebra
//!   (`* + ? / | ^ !()` and the GMEOW `{n,m}` / `<any>` extensions) — see the
//!   `path` module.
//! - **Hard-fail, no degraded fallback.** A well-formed but out-of-scope algebra
//!   node (`SERVICE`, `LATERAL`, `DESCRIBE`, SPARQL `UPDATE`) or an unimplemented
//!   builtin is a typed [`EvalError::Unsupported`] — never a partial or wrong
//!   answer (the project `no-optionality` doctrine).
//!
//! The crate carries **zero oxigraph-family dependencies** and builds for
//! `wasm32-unknown-unknown` (the wasm query path of EPIC #906); both invariants are
//! gated by `make rdf-core-hygiene`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod bgp;
mod binop;
mod construct;
mod convert;
mod dataset_spec;
pub mod engine;
pub mod error;
pub mod eval;
mod expr;
mod modifier;
mod path;
pub mod scratch;
pub mod solution;
mod template;
pub mod update;

pub use engine::{NativeSparqlEngine, PlanCache, PreparedQuery};
pub use error::EvalError;
pub use eval::{eval, evaluate_query, EvalCtx, Outcome};
pub use scratch::{ScratchId, ScratchInterner, SolutionTerm};
pub use solution::{compatible, Solution, SolutionSeq, VarSchema};
pub use update::GraphResolver;

/// A deterministic, seed-free hasher builder (`SipHash` with fixed keys).
///
/// Used for every internal map/set whose construction order or membership could
/// otherwise depend on a per-process random seed. Two reasons:
///
/// 1. **Determinism.** SPARQL multiset output must be reproducible; a randomly
///    seeded hasher could reorder hash-iteration-driven steps and leak into the
///    result. We always drive *output* order from `Vec`s, but a fixed seed removes
///    the hazard entirely (cf. the repo `mappings-determinism` lesson).
/// 2. **wasm-cleanliness.** `std`'s default `RandomState` would pull a random
///    source; the fixed-seed `DefaultHasher` needs none, keeping the crate clean
///    on `wasm32-unknown-unknown`.
///
/// This mirrors `gmeow-rdf-core`'s own use of `DefaultHasher` for its value index.
pub(crate) type DetHasher =
    std::hash::BuildHasherDefault<std::collections::hash_map::DefaultHasher>;

/// A deterministic, seed-free [`HashMap`](std::collections::HashMap). See [`DetHasher`].
pub(crate) type DetHashMap<K, V> = std::collections::HashMap<K, V, DetHasher>;

/// A deterministic, seed-free [`HashSet`](std::collections::HashSet). See [`DetHasher`].
pub(crate) type DetHashSet<K> = std::collections::HashSet<K, DetHasher>;
