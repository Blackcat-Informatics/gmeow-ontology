// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-math-lift` — the executable `math:` ingestion front-ends.
//!
//! The mathematics grounding slice ships the ingestion-bridge **ontology**: the
//! `math:IngestRun` family, the `math:parseSource` witness, the
//! `math:ingestCorrespondence` law spine, the `math:UnliftableIngest` gate, and the
//! mathematical codomain classes. Native readers and writers implement this contract
//! over explicitly supplied bytes and reference datasets.
//!
//! This crate is those lifters. Each takes a REAL external artifact and produces `math:`
//! structures conforming to the shipped codomain:
//!
//! | module | input | reader |
//! |---|---|---|
//! | [`r`] | an R model/statistics script | hand-written recursive descent |
//! | [`onnx`] | an `.onnx` graph | hand-written protobuf wire decoder |
//! | [`proof`] | a TSTP derivation | hand-written annotated-formula parser |
//! | [`projection`] | MathML, OpenMath, Data Cube, STATO and QUDT | native XML/RDF codecs |
//!
//! # Why this is a separate crate from `gmeow-math`
//!
//! `crates/pipeline/src/stages/math_producers.rs` pins the in-bundle producers as pure,
//! no-disk-read functions: their output IS bundle content, so a producer that read the
//! filesystem would make `gmeow.gts` depend on the machine that built it. A file-reading
//! lifter therefore cannot live among them.
//!
//! The split mirrors `crates/affect-ingest` beside `crates/affect`, and it is not a
//! compromise: the same lift functions serve both callers. The shipped CLI hands them
//! bytes it read from a user's path; the in-bundle producers hand them bytes embedded at
//! compile time with `include_str!`/`include_bytes!`. One implementation, two byte
//! sources, and the producers stay pure.
//!
//! # No degraded path
//!
//! `MATHEMATICS-RUNTIME.md`'s ingestion rules are absolute: no silent fallback from a
//! structured expression to a string, no optional parser backends, no feature-gated
//! "best effort" lifter. Accordingly every entry point here returns
//! `gmeow_errors::Result` and every failure is a typed [`error`] diagnostic under
//! `math.lift.*`. There is no partial lift: an artifact this crate cannot fully structure
//! produces no triples at all. Presentation-only MathML explicitly selects notation
//! rendering: its retained source does not imply an inferred expression AST.
//!
//! # Reader tiers
//!
//! Each bridge splits into a **parse** tier (bytes → a typed Rust AST, no RDF) and a
//! **lift** tier (AST → `math:` triples, no parsing). Keeping them apart is what makes
//! the parsers testable against real artifacts without an ontology in the loop, and what
//! keeps the lift honest about which structure it is actually carrying across.

#![deny(missing_docs)]

pub mod error;
pub mod frame;
pub mod ns;
pub mod onnx;
pub mod projection;
pub mod proof;
pub mod r;
pub mod sink;

pub use error::{MATH_LIFT_DIAG_CODES, register_all};
pub use frame::{BridgeKind, Lifted, RunFrame, Rung};
pub use sink::Sink;
