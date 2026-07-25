// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Assemble a `gmeow:AuthoringPacket` — a self-contained authoring brief for one
//! batch of a slice's terms.
//!
//! For each covered term the packet gathers its definition and axioms, its bounded
//! graph neighbourhood (a depth-1 CBD) and definitional-dependency closure, a handful
//! of same-slice exemplar coats (their tiers are **injected** by the caller, so the
//! library never picks a scoring authority — both callers inject [`exemplar_tiers`],
//! whose eligibility is GATED by SHACL per-term conformance and ordered by coat
//! completeness), the term's cross-ontology grounding
//! (SSSOM native alignment cells), and its cross-lingual grounding
//! (`fr`/`zh` translations JOINed from the per-slice `.po` catalogs). A missing
//! translation or external mapping is a RECORDED explicit "absent" cell, never a
//! silent blank; a batch request out of range is a HARD FAIL.
//!
//! The packet projects to three surfaces, all deterministic and byte-stable across
//! identical inputs: canonical RDF turtle ([`AuthoringPacket::to_turtle`]), JSON
//! ([`AuthoringPacket::to_json`]), and a human authoring brief
//! ([`AuthoringPacket::render_text`]).
//!
//! [`assemble_packet`] is the single canonical entry point, designed for both the
//! pipeline stage and the `gmeow slice brief` CLI to call.
//!
//! ## Partition rule (deterministic)
//! The slice's defined terms (subjects with `rdfs:isDefinedBy` the slice IRI) are
//! sorted ascending by IRI string. `axis` filters to the terms whose **local name
//! starts with** the axis string. `batch N` selects the `N`-th 25-term chunk of the
//! sorted, filtered set (`N * 25 >= len` is a hard error). With neither axis nor
//! batch, the whole slice is one packet (batch 0).

pub mod assemble;
pub mod digest;
pub mod error;
pub mod model;
pub mod ns;
pub mod render;
pub mod turtle;

pub use assemble::{
    BriefInputs, CHUNK, ShapeUnion, assemble_packet, batch_count, batch_range, defined_terms,
    exemplar_tiers, load_shape_union, resolve_repo_root,
};
pub use model::{
    Annotation, AuthoringPacket, ClosureEntry, CoveredTerm, GroundingAttribute, GroundingCell,
    GroundingMargins, ObjTerm, Triple,
};
