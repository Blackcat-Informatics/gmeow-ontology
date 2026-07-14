// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native logic-conformance harness (T4 of the Rust-first testing epic).
//!
//! The logic conformance corpus (`conformance/logic/cases/`) is GMEOW's
//! Principle-7 gate: the *oracle ≡ engine* equivalence check. Each case directory
//! carries an `input.logic.ttl` program, a `profile.json` declaring the semantic
//! profile (plus optional budget / foundation / counterfactual knobs), optional
//! `input.nq` world facts and `queries/*.logic` goals, and an `expected/` tree of
//! committed goldens (projections, materialized N-Quads, explanation skeletons,
//! verdicts, certification, budget, answers).
//!
//! This crate runs that corpus natively under `cargo-nextest` via
//! `datatest-stable`, one case per discovered `profile.json`. It drives the
//! [`gmeow_logic`] native engine cores **directly** — the same functions the
//! PyO3 surface (`gmeow_logic::py`) wraps — so the produced artifacts are
//! identical by construction (the Python `logic_runner.py` this replaced was
//! retired). Comparison
//! against the goldens uses the three runner-contract modes:
//!
//! * **graph-isomorphism** (RDFC-1.0) for RDF artifacts ([`compare::compare_rdf`]),
//! * **canonical JSON** (sorted keys) for verdicts / ledger / certification / budget
//!   / answers ([`compare::compare_canonical_json`]),
//! * **cited-IRI skeleton** for explanations ([`compare::compare_explanation_skeleton`]).
//!
//! The modules mirror the structure of `crates/slicetest`:
//!
//! * [`discover`] — find every case directory under `conformance/logic/cases/`.
//! * [`profile`] — parse and validate `profile.json` (hard-fail, no-optionality).
//! * [`run`] — orchestrate the native cores into typed [`run::CaseOutputs`].
//! * [`serialize`] — N-Quads / JSON serialization of the produced artifacts.
//! * [`compare`] — the three comparison modes and the per-case diff.
//! * [`paths`] — `CARGO_MANIFEST_DIR`-anchored path resolution.

/// Loader for the committed engine-benchmark corpus (`conformance/logic/cases/bench/`):
/// license-audited `corpus.json` metadata plus each case's `program.rules` / `input.nq`
/// / `expected/result.json` / `profile.json` as a typed [`bench_corpus::BenchCase`].
pub mod bench_corpus;
pub mod bless;
pub mod compare;
pub mod discover;
pub mod divergence;
pub mod error;
pub mod external;
pub mod paths;
pub mod profile;
pub mod run;
pub mod serialize;
/// The one vendored-corpus contract (`corpus.json` schema + license gate) shared by
/// both the `cases/external/` correctness family and the `cases/bench/` performance
/// family — see [`vendored`] for the single-root, single-gate statement.
pub mod vendored;
