// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-bundle-view` — the READ side of a materialized `gmeow.gts` bundle.
//!
//! GMEOW ships ONE artifact: `generated/dist/gmeow.gts`, a folded RDF dataset plus
//! the deterministic blobs (mappings, queries, cells, tests, reasoning reports,
//! shapes, axioms, schemas, models) that ride alongside it. Two very different kinds
//! of code touch that artifact:
//!
//! * `gmeow-pipeline` **writes** it. It owns the stage DAG, the carrier, the cache,
//!   the fanout, the release signer — the whole build executor.
//! * Everything else **reads** it. The consumer `gmeow` CLI resolves a term against
//!   the fold; `gmeow-dev` transpiles the folded projection sources; the MCP tool
//!   surface answers `describe` / `lookup` / `search` out of the same bytes; the
//!   diagnostics consumers rehydrate `graph/diagnostics` into a finding index.
//!
//! Before this crate existed, the read side lived *inside* the writer, so anything
//! that wanted to read a bundle inherited the entire build executor: rayon, the
//! reasoner, the signer, `ureq`, `tempfile`, the docs renderer and its embedded
//! multi-megabyte wasm. That is a hard blocker for a wasm target and a very poor
//! deal for a consumer that only wants a term card. This crate is the extraction:
//! the read side, and only the read side.
//!
//! # What it holds
//!
//! * [`bundle_blobs`] — parse-once [`Bundle`](bundle_blobs::Bundle) access to a
//!   snapshot's folded graph and each of its blob archives, from the `&[u8]`
//!   snapshot alone, with no repo checkout.
//! * [`export`] — the fold view and the flat-export renderers (CSVW, Markdown,
//!   JSONL, `llms.txt`, N-Quads, TriG, statements JSONL, SKOS, OBO Graphs, ShEx)
//!   plus the consumer resolution surface the CLI and MCP `describe` share.
//! * [`diagnostics_reader`] — the right-inverse (section) of the
//!   `graph/diagnostics` projection: quads back into findings, the antecedent DAG,
//!   the gate verdict, and the minimal fatal cut.
//! * [`native_query`] — the native SPARQL substrate the readers query through.
//! * [`graph_iris`] — the five named-graph IRIs the read side addresses.
//! * [`lpg_prefixes`] — the longest-first CURIE table the projections compact with.
//! * [`error`] — the crate's diagnostic-code catalog.
//!
//! # Boundary rules
//!
//! * **Leaf.** It depends on no first-party crate that depends on it, and in
//!   particular never on `gmeow-pipeline`. `gmeow-pipeline` depends on THIS crate
//!   and re-exports its items at the historical paths.
//! * **wasm-clean.** `cargo check --target wasm32-unknown-unknown` is part of the
//!   contract, which rules out `rayon`, process spawning, and the embedded-asset
//!   crates. Filesystem entry points exist (a caller that HAS a checkout may hand
//!   one a path), but every core view is a pure function of bytes or of an already
//!   folded [`RdfDataset`](purrdf::RdfDataset).
//! * **Read-only.** Nothing here assembles a bundle, writes a carrier graph, or
//!   runs a stage. Blob rep labels are declared here and re-exported by the
//!   producer, so reader and writer are one constant and cannot drift.

pub mod bundle_blobs;
pub mod diagnostics_reader;
pub mod error;
pub mod export;
pub mod graph_iris;
pub mod lpg_prefixes;
pub mod native_query;
