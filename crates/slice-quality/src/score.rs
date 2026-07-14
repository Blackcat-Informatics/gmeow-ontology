// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The scoring context handed to each axis primitive, and the raw axis result.
//!
//! A primitive reads `ctx.graph` (at the breadth its `ContextScope` licenses) over
//! `ctx.terms` (the slice's own authored terms) and returns an [`AxisScore`]: a
//! normalized 0.0–1.0 score plus the advisory findings it wants surfaced. Advice
//! output is always about the one target slice, whatever the read scope.

use std::path::PathBuf;
use std::sync::Arc;

use gmeow_errors::{Finding, Severity, Standpoint};
use gmeow_lang_bridge::GmnDictionary;
use purrdf::RdfDataset;

use crate::graph;

/// Where the two repo-anchored axes (`gmn1_coverage`, `DocMaturity`) source their
/// wide-scope inputs — the shared `slices/grounding/lang/` dictionary and the
/// documentation model — from.
///
/// The repo-anchored axes read more than the one slice's own `.ttl`: they need a
/// dictionary and a documentation model that normally live in the surrounding
/// checkout. This seam lets a foreign slice pulled in on its own (with no repo
/// around it) instead carry those inputs in an embedded bundle. Every in-repo
/// caller stays [`ScoringEnv::Repo`], byte-for-byte the pre-seam behaviour.
#[derive(Clone)]
pub enum ScoringEnv {
    /// Source wide-scope inputs from the surrounding repo checkout (the verbatim
    /// pre-seam behaviour): read `slices/grounding/lang/module.ttl` off disk and
    /// build the documentation model with a repo-wide `slices/` sweep.
    Repo,
    /// Source wide-scope inputs from an embedded bundle. The carried dictionary is
    /// ALREADY loaded and validated (constructed with `?` at bundle-build time, so a
    /// corrupt wheel hard-fails there): the `gmn1_coverage` arm uses it directly with
    /// no tolerant advisory. `DocMaturity` ignores the payload and builds a fresh
    /// single-slice model from the slice's own directory.
    Bundle(Arc<GmnDictionary>),
}

/// Everything an axis primitive may read about the slice under assessment.
pub struct ScoreContext<'a> {
    /// The slice ontology IRI (`…/slices/<name>`).
    pub slice_iri: String,
    /// The slice directory on disk (for file-shaped checks — test cells, i18n).
    pub slice_dir: PathBuf,
    /// The dataset to read, already assembled at the axis's licensed scope.
    pub graph: &'a RdfDataset,
    /// The slice's own authored term IRIs (typed subjects `rdfs:isDefinedBy`
    /// the slice), sorted — the population most per-term axes score over.
    pub terms: Vec<String>,
    /// Where the two repo-anchored axes source their wide-scope inputs.
    pub env: ScoringEnv,
}

impl<'a> ScoreContext<'a> {
    /// Build a context for `slice_iri`, computing the slice's own term set from
    /// the graph (subjects whose `rdfs:isDefinedBy` is the slice IRI).
    #[must_use]
    pub fn new(
        slice_iri: String,
        slice_dir: PathBuf,
        graph: &'a RdfDataset,
        env: ScoringEnv,
    ) -> Self {
        let terms = slice_terms(graph, &slice_iri);
        Self {
            slice_iri,
            slice_dir,
            graph,
            terms,
            env,
        }
    }
}

/// The slice's own authored terms: typed IRI subjects that declare
/// `rdfs:isDefinedBy <slice_iri>`.
///
/// Namespace is deliberately irrelevant: grounding slices own `logic:`, `lang:`,
/// and `math:` terms alongside occasional `gmeow:` terms. A graph without explicit
/// ownership yields an empty population instead of silently scoring unrelated terms.
#[must_use]
pub fn slice_terms(ds: &RdfDataset, slice_iri: &str) -> Vec<String> {
    use purrdf::{DatasetView, GraphMatch, TermRef};

    let (Some(type_p), Some(defined_by_p), Some(slice_id)) = (
        graph::id(ds, graph::RDF_TYPE),
        graph::id(ds, "http://www.w3.org/2000/01/rdf-schema#isDefinedBy"),
        graph::id(ds, slice_iri),
    ) else {
        return Vec::new();
    };

    let mut out: Vec<String> = ds
        .quads_for_pattern(None, Some(defined_by_p), Some(slice_id), GraphMatch::Any)
        .filter_map(|q| match ds.resolve(q.s) {
            TermRef::Iri(iri) if graph::has_any(ds, q.s, type_p) => Some(iri.to_owned()),
            _ => None,
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The raw result of one axis primitive: a normalized score and its advisories.
pub struct AxisScore {
    /// The normalized score in 0.0–1.0 (clamped by the caller).
    pub score: f64,
    /// The advisory findings the primitive wants surfaced (never gating).
    pub findings: Vec<Finding>,
}

impl AxisScore {
    /// A clean pass with no advice.
    #[must_use]
    pub fn clean(score: f64) -> Self {
        Self {
            score,
            findings: Vec::new(),
        }
    }
}

/// Build one advisory finding on the slice-quality tool at the given code+message.
#[must_use]
pub fn advisory(code: &str, message: impl Into<String>) -> Finding {
    Finding::new(Severity::Warning, code, message)
        .with_tool("slice-quality")
        .with_standpoint(Standpoint::Advisory)
}
