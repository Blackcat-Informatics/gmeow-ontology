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

/// One reasoner-derived "why" fact about a documented term: the rule that fired, a
/// display of the axiom it concluded, and the premises it fired from. Parsed by the
/// pipeline (`build_executable_docs_data`) from `stage-reason`'s already-materialized
/// `reasoning-explanations` proof skeletons (reason-once — the reasoner itself never
/// runs in the docs crate; see [`ExecutableDocsData::term_entailments`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entailment {
    /// The firing rule: a compact display form (a CURIE where the canonical prefix
    /// registry knows one, else the bare IRI in `<...>` form) — never a fabricated
    /// human label, since `viaRule` names a rule IRI, not prose.
    pub rule: String,
    /// A compact, deterministic display of the concluded triple (`s p o`,
    /// CURIE-compacted), in the same style as the "try it" asserted/inferred lines.
    pub conclusion: String,
    /// Compact displays of the derivation's premises (zero or more, sorted), in the
    /// same `s p o` style as [`conclusion`](Self::conclusion).
    pub premises: Vec<String>,
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
    /// Term/slice export both flow through the playground's `DESCRIBE` (client-side,
    /// all RDF formats via purrdf), so this asset is the single export substrate too.
    pub playground_trig: Vec<u8>,
    /// The **core browser bundle** — the object-level ontology (the bundle's default
    /// graph) as N-Quads text, sized for the browser (~24 MB vs the full bundle's
    /// ~948 MB extraction). The bundle-explorer surface parses this client-side (via
    /// the vendored purrdf RDF engine) to answer `info`/`describe` over the SAME
    /// authored ontology the pipeline shipped. Empty ⇒ no explorer bundle. Built by
    /// [`gmeow_validate::store::core_browser_bundle_nquads`].
    pub core_bundle_nquads: Vec<u8>,
    /// The full `gmeow.gts` bundle bytes — the browser Tier-1 validate surface reads
    /// its `shapes-archive` (a small container read; it does NOT extract the whole
    /// dataset). Empty ⇒ no in-browser validation. Shipped as an EXTERNAL site asset,
    /// never re-embedded into `gmeow.gts`.
    pub full_bundle_gts: Vec<u8>,
    /// Per-term reasoner "why" panels (B3): every derivation whose `gmeow:concludes`
    /// or `gmeow:hasPremise` quoted triple mentions the term's IRI (in subject,
    /// predicate, or object position), keyed by the exact term IRI. Parsed by the
    /// pipeline from `stage-reason`'s materialized `reasoning-explanations` artifact
    /// (reason-once — never a second reasoning pass). Empty in the model-only render
    /// (`ExecutableDocsData::default`): the "Inferred facts" panel and the
    /// "unsatisfiable because" derivation lines render only when this map carries an
    /// entry for the term, never a fabricated "no entailments" claim.
    pub term_entailments: BTreeMap<String, Vec<Entailment>>,
}

impl ExecutableDocsData {
    /// Whether the offline SPARQL playground (and thus the export surface) can be
    /// rendered — i.e. the pipeline supplied the bundled query asset.
    #[must_use]
    pub fn has_playground(&self) -> bool {
        !self.playground_trig.is_empty()
    }

    /// Whether the browser bundle assets (the core N-Quads for the explorer and the
    /// full `gmeow.gts` for in-browser validation) were supplied — i.e. the
    /// interactive validate/explore surfaces can be rendered.
    #[must_use]
    pub fn has_bundle(&self) -> bool {
        !self.core_bundle_nquads.is_empty() && !self.full_bundle_gts.is_empty()
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
