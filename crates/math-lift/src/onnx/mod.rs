// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The ONNX ingestion bridge — the AI-self-structure flagship.
//!
//! `MATHEMATICS-BRIDGES.md`: the bridge lifts a model graph into `math:` so that "an AI can
//! describe its own architecture by lifting its own ONNX export". ONNX is the interchange
//! anchor; the *meaning* of the architecture is authored here, not in ONNX.
//!
//! Three tiers, split exactly as the R bridge is:
//!
//! | module | in | out |
//! |---|---|---|
//! | [`wire`] | `.onnx` bytes | protobuf fields, or a byte-offset failure |
//! | [`model`] | protobuf fields | the typed `onnx.proto` subset — **never a tensor payload** |
//! | [`mod@lift`] | that subset | `math:` triples |
//!
//! Keeping the tiers apart is what lets the decoder be tested against real bytes with no
//! ontology in the loop, and what keeps the lift honest about which structure it is actually
//! carrying across.
//!
//! The `encode` module is test-only: it is the inverse of [`wire`], written independently
//! against `onnx.proto`, and it is what the committed fixtures under
//! `crates/math-lift/fixtures/` are byte-pinned against.

#[cfg(test)]
pub(crate) mod encode;
pub mod lift;
pub mod model;
pub mod wire;

pub use lift::lift;
