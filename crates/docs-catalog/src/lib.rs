// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-docs-catalog` — the READ side of the meta-level documentation-distribution
//! catalog.
//!
//! GMEOW's shipped `gmeow.gts` carries a meta-level named graph,
//! `gmeow:graph/distribution-catalog`, declaring WHICH documentation distributions exist,
//! their family, their consumer class, and (for the doc-render family) their declared
//! capability loss. Two very different kinds of code touch it:
//!
//! * `gmeow-pipeline`'s `stages::distribution_catalog` **writes** it, as part of the
//!   carrier assembly.
//! * Consumers **read** it: `gmeow docs matrix` prints the per-format consumer-need
//!   matrix, and the MCP `distribution_matrix` tool serves the same rows to an agent out
//!   of the same bytes.
//!
//! The reader used to live inside the writer, so anything that wanted a distribution row
//! inherited the entire build executor — its stage DAG, scheduler, cache, release signer,
//! network client, and the docs renderer's embedded multi-megabyte wasm. That is a hard
//! blocker for a wasm target and a very poor deal for a consumer that wants eight rows of
//! a table. This crate is the extraction: the catalog read side, and only the read side.
//!
//! # What it holds
//!
//! * [`distribution_matrix`] — [`read_distribution_matrix`] and [`DistributionRow`]: the
//!   per-format consumer-need matrix, resolved by QUERYING the shipped catalog rather
//!   than re-authoring a static table.
//! * [`concept_lattice`] — [`read_concept_lattice`] and [`ConceptRow`]: the formal-concept
//!   lattice over the catalog's surface × capability incidence. A separate reader because
//!   a concept has an extent and an intent and a distribution has neither.
//! * [`catalog_graph`] — the single structural read of the catalog named graph both
//!   readers share.
//! * [`identity`] — the catalog's subject-IRI and N-Triples formatting helpers, shared
//!   with the emitter.
//! * [`error`] — the crate's diagnostic-code catalog (`docs-catalog.*`).
//!
//! # Boundary rules
//!
//! * **Leaf.** It depends on no first-party crate that depends on it, and in particular
//!   never on `gmeow-pipeline` (which depends on THIS crate and re-exports its items at
//!   the historical paths) and never on `gmeow-docs`.
//! * **wasm-clean.** `cargo check --target wasm32-unknown-unknown` is part of the
//!   contract, which rules out `rayon`, process spawning, and the embedded-asset crates.
//!   Every entry point is a pure function of snapshot bytes.
//! * **Read-only.** Nothing here assembles a catalog, writes a carrier graph, or runs a
//!   stage.
//!
//! # How the four couplings to `gmeow-pipeline` were resolved
//!
//! `read_distribution_matrix` used four items from inside the build executor. Each is
//! resolved here explicitly, and none of them re-opens an edge to the pipeline:
//!
//! 1. **`crate::error::DocsDistribution`** → this crate mints its OWN diagnostic kinds
//!    under the `docs-catalog.*` code namespace ([`error`]), following the
//!    `gmeow-bundle-view` shape: a `DOCS_CATALOG_DIAG_CODES` catalog, a `register_all()`,
//!    and a self-consistency test proving the two are in bijection. There is no central
//!    aggregator in this workspace, so that per-crate catalog IS the complete declaration.
//!    The hyphenated crate name keeps its hyphen in the code prefix, as `slice-quality.io`
//!    already does.
//! 2. **`crate::projections::{TagMap, project_graph}`** → NOT moved, NOT hoisted, NOT
//!    re-exported: it turned out not to be a coupling of the moved code at all. Those two
//!    items are used by `build_docs_distribution_manifest`, which projects the release
//!    DCAT instance through the bundle's compiled `dcat.rq` CONSTRUCT — a WRITE-side,
//!    release-time concern that stays in `gmeow-pipeline` with the rest of the manifest
//!    builder. `read_distribution_matrix` never touches either one. Hoisting
//!    `project_graph` into `gmeow-bundle-view` on its account would have moved a SPARQL
//!    CONSTRUCT driver (and its up-projection prefix corpus) across a seam to satisfy a
//!    dependency that does not exist; the honest resolution is that the module-level
//!    `use` was broader than the function, and the split retires the false edge.
//! 3. **`crate::stages::carrier::GRAPH_DISTRIBUTION_CATALOG`** → hoisted into
//!    [`gmeow_bundle_view::graph_iris`] as its fifth constant, joining the four read-side
//!    graph IRIs already there; `gmeow_pipeline::stages::carrier` re-exports it back at
//!    its original `pub(crate)` visibility, exactly as it already does for
//!    `GRAPH_DOCUMENTATION` / `GRAPH_DIAGNOSTICS` / `GRAPH_AUTHORING_BRIEFS`. One
//!    definition site; the assembler and the readers cannot drift to different IRIs.
//! 4. **`crate::stages::distribution_catalog::{dist_iri, iri, site_sub_asset_iri, triple,
//!    triple_lit}`** → [`identity`] takes over `dist_iri`, `iri`, `triple`, `triple_lit`
//!    (plus the `DISTRIBUTION_BASE` they hang off and the N-Triples literal escaper they
//!    share), and the pipeline module re-exports each at its original `pub(crate)`
//!    visibility. `site_sub_asset_iri` deliberately did NOT move: it is defined over
//!    `gmeow_docs::formats::DocFormat::Site`, so hoisting it would drag `gmeow-docs` into
//!    a wasm-clean leaf. It stays with the emitter, defined in terms of the moved
//!    [`identity::dist_iri`] — one definition site either way, never a copy.

pub mod catalog_graph;
pub mod concept_lattice;
pub mod distribution_matrix;
pub mod error;
pub mod identity;

pub use catalog_graph::GRAPH_DISTRIBUTION_CATALOG;
pub use concept_lattice::{ConceptRow, read_concept_lattice};
pub use distribution_matrix::{DistributionRow, read_distribution_matrix};
