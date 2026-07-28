// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The build-time **executable-docs** data: the reasoned + serialized inputs the
//! "live" documentation surfaces need, computed by the pipeline from the carrier and
//! handed to [`render_site_lang_exec`](crate::slug::render_site_lang_exec).
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
    /// The full `gmeow.gts` bundle bytes — the ONE queryable asset every interactive
    /// surface reads. The browser MCP engine boots over exactly these bytes and then
    /// answers the playground, the explorer, Tier-1 validation, reasoning and the codec
    /// tools from them. Empty ⇒ no interactive surface at all. Shipped as an EXTERNAL
    /// site asset, never re-embedded into `gmeow.gts`.
    ///
    /// This field used to have two siblings — a `playground_trig` TriG projection and a
    /// `core_bundle_nquads` object-level N-Quads projection — each re-serializing a slice
    /// of these same bytes for a separate client-side parser. Both are retired: they were
    /// 311 MB of duplicate substrate for questions the engine already answers from the
    /// bundle it is booted over, and the TriG one was worse than redundant. It routed every
    /// statement into a NAMED graph, so the playground's own default query
    /// (`SELECT ?s ?p ?o WHERE { ?s ?p ?o }`) and every term page's `?q=` prefill matched
    /// the default graph and returned nothing. One asset, one engine, one answer.
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
    /// The **conjecture demo library** — the curated `logic:Conjecture` corpus
    /// (`slices/grounding/logic/examples/conjectures.ttl`) shipped verbatim as a site
    /// sub-asset. The W4 conjecture playground fetches + byte-verifies it, presents its
    /// six curated conjectures (every Belnap-to-lifecycle branch, with witnesses and
    /// anti-legs), and runs the live wasm symmetric conjecture engine over the built-in
    /// runnable demos. Empty ⇒ no conjecture playground. Read deterministically from the
    /// committed slice example by the pipeline (hard-fail if absent).
    pub conjectures_ttl: Vec<u8>,
}

impl ExecutableDocsData {
    /// Whether the queryable bundle was supplied — i.e. EVERY interactive surface (the
    /// SPARQL playground, the bundle explorer, the Tier-1 validate controls, live
    /// reasoning, the GMN transcode and the export affordances) can be rendered.
    ///
    /// There is exactly ONE such predicate because there is exactly one queryable asset.
    /// It replaced a `has_playground()` / `has_bundle()` pair that gated on two different
    /// projections of these same bytes: a page could then be emitted under one gate while
    /// the surface behind it was driven by the other, which is precisely how the playground
    /// page came to ship a query asset it could not query.
    #[must_use]
    pub fn has_bundle(&self) -> bool {
        !self.full_bundle_gts.is_empty()
    }

    /// Whether the conjecture demo library was supplied — i.e. the W4 conjecture
    /// playground surface can be rendered. Requires the bundle too: the playground runs
    /// the live engine, which boots over it.
    #[must_use]
    pub fn has_conjectures(&self) -> bool {
        !self.conjectures_ttl.is_empty() && self.has_bundle()
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
