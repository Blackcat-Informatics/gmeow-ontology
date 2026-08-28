// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The named-graph IRIs the READ side addresses.
//!
//! `gmeow-pipeline`'s carrier declares 45 `GRAPH_*` constants — one per named graph
//! the snapshot assembles. Five of them are named again on the *reading* end: by the
//! bundle readers in this crate, by the MCP tool surface that queries a materialized
//! `gmeow.gts`, and by `gmeow-docs-catalog`'s distribution/concept readers. Those five
//! are declared here so a reader does not have to depend on the build executor to
//! spell a graph IRI it merely selects on.
//!
//! The split is by consumer, not by importance: the other carrier graph constants
//! are addressed only while ASSEMBLING the snapshot (choosing which stage output
//! lands in which graph), so they stay with the assembler. This is not a claim that
//! each of the remaining constants was individually reviewed for read-side use — it
//! is the set the current readers reference.
//!
//! Each constant has exactly ONE definition site, here — in `gmeow-ns`, the zero-dependency
//! leaf that already owns the single declaration of every GMEOW term namespace.
//! `gmeow_bundle_view::graph_iris` re-exports this module at its original path, so a reader
//! that already links the bundle read side is unchanged, while one that only needs to spell
//! a graph IRI (the MCP reasoning segment) does not have to link it. `gmeow_pipeline`'s
//! `stages::carrier` and `stages::release` re-export them at their original
//! visibility, so the assembly side is unchanged and the two sides can never drift
//! to different IRIs.

/// The named graph carrying the documentation corpus (`gmeow_docs`'s RDF projection
/// of the typed documentation model), folded as its own queryable graph so a
/// repo-free consumer reads the docs surface straight out of `gmeow.gts`.
pub const GRAPH_DOCUMENTATION: &str = "https://blackcatinformatics.ca/gmeow/graph/documentation";

/// The per-slice authoring-packet corpus: a `gmeow:AuthoringPacket` for every in-repo
/// slice batch (definition + axioms + bounded neighbourhood + grounding cross-table),
/// assembled by `gmeow_slice_brief::assemble_packet` and attached by the dedicated
/// `stage-slice-brief` producer. Folded as its own queryable named graph so a repo-free
/// consumer reads every slice's authoring briefs straight out of `gmeow.gts` (the
/// shippable authoring deliverable). Excluded from the reasoned object-level EDB exactly
/// like `graph/quality-assessment` (it asserts a self-description corpus, not object-level
/// axioms — `gts_compose` folds only the default graph, so this named graph never pollutes
/// the composed EDB).
pub const GRAPH_AUTHORING_BRIEFS: &str =
    "https://blackcatinformatics.ca/gmeow/graph/authoring-briefs";

/// The named graph the run's `gmeow:Finding` diagnostics ride in, as emitted by
/// `gmeow_errors::render::to_gmeow_rdf_in_graph`. `diagnostics_reader` is
/// the right-inverse of that projection and scopes every SELECT to this graph.
pub const GRAPH_DIAGNOSTICS: &str = "https://blackcatinformatics.ca/gmeow/graph/diagnostics";

/// The named graph the release manifest and the per-artifact attestations ride in.
pub const GRAPH_ATTESTATIONS: &str = "https://blackcatinformatics.ca/gmeow/graph/attestations";

/// The meta-level named graph carrying the canonical distribution catalog: WHICH
/// documentation distributions exist, their family, their consumer class, and (for the
/// doc-render family) their declared capability loss — plus the formal-concept lattice
/// derived over the surface × capability incidence. Read back by
/// `gmeow_docs_catalog::read_distribution_matrix` (`gmeow docs matrix`, the MCP
/// `distribution_matrix` tool) and `gmeow_docs_catalog::read_concept_lattice`, so it is a
/// READ-side graph IRI and lives here rather than with the assembler that writes it.
///
/// NOT in `gmeow_logic::reasoning_graphs::OBJECT_LEVEL_NAMED_GRAPHS`: it is a meta-level
/// self-description corpus and stays out of the object-level reasoning EDB.
pub const GRAPH_DISTRIBUTION_CATALOG: &str =
    "https://blackcatinformatics.ca/gmeow/graph/distribution-catalog";
