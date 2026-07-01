// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The build-time **executable-docs** data: the reasoned + serialized inputs the
//! "live" documentation surfaces need, computed by the pipeline from the carrier and
//! handed to [`render_site_lang_exec`](crate::render::render_site_lang_exec).
//!
//! The docs crate cannot produce these on its own — they need the native reasoner and
//! the assembled ontology carrier, both pipeline-only. So the crate defines the shape
//! here and renders the executable surfaces *only when the data is present*. A
//! model-only render (unit tests, the PyO3 preview surface) passes
//! [`ExecutableDocsData::default`] and produces the complete base site **without** the
//! executable surfaces — a genuine layering seam, not a degraded fallback.

use std::collections::BTreeMap;
use std::sync::Arc;

use gmeow_rdf::RdfDataset;

/// The asserted-vs-inferred view of one worked example, computed at build time by
/// running the native reasoner over `(ontology TBox ∪ example ABoxes)` and slicing the
/// closure by the example's own subjects.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InferenceDiff {
    /// The example's asserted triples (its ABox), as display lines (`s p o`).
    pub asserted: Vec<String>,
    /// The triples the reasoner **derived** about the example's subjects that the
    /// example did not assert — the "try it" payload.
    pub inferred: Vec<String>,
}

impl InferenceDiff {
    /// Whether there is anything to show (either column non-empty).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.asserted.is_empty() && self.inferred.is_empty()
    }
}

/// Everything the executable docs surfaces need. Empty ⇒ render the base site only.
#[derive(Debug, Clone, Default)]
pub struct ExecutableDocsData {
    /// Per-example asserted-vs-inferred, keyed by [`example_key`]. The "try it"
    /// section on an example/term page reads this.
    pub example_inferences: BTreeMap<String, InferenceDiff>,
    /// Inferences the union-reasoning pass produced that are **not** attributable to
    /// any single example (shared / blank-Skolem witnesses). Surfaced on the
    /// playground page so no derived triple is silently dropped.
    pub cross_example: Vec<String>,
    /// The RDF asset the offline SPARQL playground queries: the documentation graph
    /// plus the reasoned ontology closure, serialized to TriG. Empty ⇒ no playground.
    pub playground_trig: Vec<u8>,
    /// The reasoned ontology dataset, for on-demand per-term / per-slice multi-format
    /// export (`gmeow_rdf::describe`). `None` ⇒ no export surface.
    pub ontology: Option<Arc<RdfDataset>>,
}

impl ExecutableDocsData {
    /// Whether the offline SPARQL playground can be rendered (its asset is present).
    #[must_use]
    pub fn has_playground(&self) -> bool {
        !self.playground_trig.is_empty()
    }

    /// Whether the per-term / per-slice export surface can be rendered.
    #[must_use]
    pub fn has_export(&self) -> bool {
        self.ontology.is_some()
    }

    /// The inference diff for one example, if any was computed.
    #[must_use]
    pub fn inference_for(&self, slice: &str, logical_path: &str) -> Option<&InferenceDiff> {
        self.example_inferences
            .get(&example_key(slice, logical_path))
    }
}

/// The stable key for an example's inference diff: `<slice-iri>\u{0}<logical-path>`.
/// The pipeline (producer) and the renderer (consumer) must agree on this.
#[must_use]
pub fn example_key(slice: &str, logical_path: &str) -> String {
    format!("{slice}\u{0}{logical_path}")
}
