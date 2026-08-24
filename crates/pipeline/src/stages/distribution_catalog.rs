// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The canonical **distribution catalog** — a meta-level named graph declaring WHICH
//! documentation distributions exist, their FAMILY, their CONSUMER class, and (for a
//! distribution that IS a capability-bearing projection surface) its declared capability
//! LOSS.
//!
//! This is a SCHEMA, not a release record: it declares the nine distributions gmeow
//! ships and their static properties. It carries NO per-release digests — a shipped
//! release's concrete `gmeow:contentDigest` is a later release-time instance, never
//! folded into this carrier-time graph (a render-derived digest here would be a
//! non-converging fixpoint: the digest of THIS bundle would have to be known before
//! THIS bundle is serialized).
//!
//! ## One table, nine rows
//!
//! [`DISTRIBUTIONS`] is the SINGLE declaration of what a shipped distribution is. Every
//! other per-distribution answer in this repo is a fold over it and nothing else:
//! [`declared_distribution_slugs`], [`media_type_for_slug`], the emitted catalog rows,
//! and (across the crate seam) `gmeow-dev sync`'s rendered destinations and
//! content-addressed release entries. There is deliberately no second array anywhere that
//! restates a slug, a family, a media type, or a consumer: the previous shape carried the
//! same nine facts in five separate places (two enums here, two array literals in
//! `dev_project.rs`, and a `CANONICAL_SLUGS` literal in the contract gate), so a new
//! surface could be declared in one and silently absent from the rest.
//!
//! ## The three families
//!
//! * **doc-render** — `site`, `mdbook`, `pdf`, `snippets`. These are EXACTLY the
//!   [`gmeow_docs::formats::DocFormat`] variants.
//! * **serialization** — `okf`, `jsonld`, `yamlld`, `pydantic`. These are structured
//!   re-serializations of the ontology, not lossy prose renderings; they carry no
//!   capability partition at all (`surface: None`) and so declare no
//!   `gmeow:declaredLoss` here.
//! * **interactive-runtime** — `console`. The standalone in-browser console: a live
//!   runtime over the shipped bundle rather than a rendered document, and — like the four
//!   doc-render surfaces — a [`DistributionSurface`] with a capability partition.
//!
//! A row whose `surface` is `Some` gets its BOTH-SIDED capability ledger from the SINGLE
//! authority ([`gmeow_docs::formats::surface_capabilities`]) — the same table
//! [`crate::stages::docs_format_rendering`] reads — so the catalog's loss claims can never
//! drift from the renderer's, and the console's loss is DERIVED from the lattice exactly
//! the way `site`'s and `pdf`'s are, never authored here.
//!
//! Every subject this module emits (distribution nodes, family nodes, loss nodes)
//! carries the proven generated-aBox skeleton (mirroring the carrier's
//! `build_fanout_opaque_manifest`): `rdf:type` +
//! `rdfs:isDefinedBy <graph/distribution-catalog>` + `gmeow:graphBoxRole
//! gmeow:boxABox` + `rdfs:label`, which the whole-bundle structural lint accepts as an
//! assertional individual without a `skos:definition`.
//!
//! The N-Triples are sorted + deduped and no clock/randomness rides here, so the
//! catalog is byte-reproducible.

use gmeow_docs::formats::{Capability, DistributionSurface, DocFormat, surface_capabilities};
use gmeow_docs::surface_lattice::{
    AUTHORED_INCIDENCE, CapMask, Implication, SurfaceConcept, authored_concepts, authored_dg_basis,
};

use crate::stages::carrier::{GRAPH_DISTRIBUTION_CATALOG, parse_into_graph};

use gmeow_ns::{GMEOW_NS, LOGIC_NS};
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";

/// The instance subject base every distribution/family/loss IRI this module mints
/// lives under. Defined ONCE in [`gmeow_docs_catalog::identity`], alongside the
/// [`dist_iri`] built on it, and re-exported here so the emitter and the reader can never
/// disagree about where the catalog's subjects live.
use gmeow_docs_catalog::identity::DISTRIBUTION_BASE;

/// A distribution family — the coarse KIND of artifact a distribution is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Family {
    /// A lossy prose/document rendering of the documentation model.
    DocRender,
    /// A structured re-serialization of the ontology: no capability partition at all.
    Serialization,
    /// A live in-browser runtime over the shipped bundle.
    InteractiveRuntime,
}

impl Family {
    /// Every family, in emission order.
    ///
    /// [`emit_ntriples`] iterates THIS rather than an inline array literal. Before this
    /// constant existed the emitter walked a hand-written `[Family::DocRender,
    /// Family::Serialization]`, so a family variant added for a new distribution row would
    /// have emitted NO family node at all and every row referencing it would have pointed
    /// at a dangling IRI — a silent capability degradation with no gate on it.
    pub const ALL: [Family; 3] = [
        Family::DocRender,
        Family::Serialization,
        Family::InteractiveRuntime,
    ];

    /// The stable machine slug (the `…/distribution/family/<slug>` tail, and the `family`
    /// column `gmeow docs matrix` prints).
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Family::DocRender => "doc-render",
            Family::Serialization => "serialization",
            Family::InteractiveRuntime => "interactive-runtime",
        }
    }
}

/// One row of [`DISTRIBUTIONS`]: everything the repo knows, statically, about one shipped
/// documentation distribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DistRow {
    /// The stable machine slug. When [`DistRow::surface`] is `Some`, this MUST equal that
    /// surface's own slug — gated by `the_table_agrees_with_the_surface_vocabulary`.
    pub slug: &'static str,
    /// The family this distribution belongs to.
    pub family: Family,
    /// The declared `gmeow:artifactMediaType`.
    pub media_type: &'static str,
    /// The `gmeow:ProjectionContext` consumer local name (`gmeow:eligibleForConsumer`).
    pub consumer: &'static str,
    /// The capability-bearing projection surface this distribution IS, when it is one.
    ///
    /// `Some` ⟹ the row's BOTH-SIDED capability ledger (`gmeow:representableParameter` +
    /// `gmeow:declaredLoss`) is DERIVED from [`surface_capabilities`], never authored
    /// here. `None` ⟹ a structured re-serialization, which carries no capability partition
    /// and therefore declares no loss.
    pub surface: Option<DistributionSurface>,
    /// The release-relative path the rendered tree ships at — the destination
    /// `gmeow-dev sync`'s docs fanout reconciles to and the `gmeow:sourceLocation` the
    /// release manifest records.
    pub rel_path: &'static str,
    /// Whether this distribution ships the shared interactive-engine `gmeow:SubAsset`s.
    ///
    /// The sub-assets are ONE set of shared subjects; every owner links to them with
    /// `gmeow:hasSubAsset` and prices them out of its OWN rendered tree, so the
    /// byte-identity of the engine across owners is a checkable release-time fact rather
    /// than an assumption.
    pub sub_assets: bool,
}

/// **The** distribution table: the single declaration of the nine shipped documentation
/// distributions. See the module docs — everything per-distribution folds over this and
/// nothing restates it.
pub const DISTRIBUTIONS: [DistRow; 9] = [
    DistRow {
        slug: "site",
        family: Family::DocRender,
        media_type: "text/html",
        consumer: "consumerPublicSite",
        surface: Some(DistributionSurface::Format(DocFormat::Site)),
        rel_path: "dist/gmeow-docs/site",
        sub_assets: true,
    },
    DistRow {
        slug: "mdbook",
        family: Family::DocRender,
        media_type: "text/markdown",
        consumer: "consumerOfflineBook",
        surface: Some(DistributionSurface::Format(DocFormat::Mdbook)),
        rel_path: "dist/gmeow-docs/mdbook",
        sub_assets: false,
    },
    DistRow {
        slug: "pdf",
        family: Family::DocRender,
        media_type: "application/pdf",
        consumer: "consumerPrintArchive",
        surface: Some(DistributionSurface::Format(DocFormat::Pdf)),
        rel_path: "dist/gmeow-docs/pdf",
        sub_assets: false,
    },
    DistRow {
        slug: "snippets",
        family: Family::DocRender,
        media_type: "text/markdown",
        consumer: "consumerAgentMemory",
        surface: Some(DistributionSurface::Format(DocFormat::Snippets)),
        rel_path: "dist/gmeow-docs/snippets",
        sub_assets: false,
    },
    // The standalone interactive console. Its capability ledger — including its declared
    // loss of `search-index` and `cross-link-fidelity` — is DERIVED from the surface
    // lattice below exactly the way every other surface row's is; nothing about the
    // console's loss is authored in this table.
    DistRow {
        slug: "console",
        family: Family::InteractiveRuntime,
        media_type: "text/html",
        consumer: "consumerInteractiveConsole",
        surface: Some(DistributionSurface::Console),
        rel_path: "dist/gmeow-docs/console",
        sub_assets: true,
    },
    DistRow {
        slug: "okf",
        family: Family::Serialization,
        media_type: "application/json",
        consumer: "consumerKnowledgeFederation",
        surface: None,
        rel_path: "dist/gmeow-docs/okf",
        sub_assets: false,
    },
    DistRow {
        slug: "jsonld",
        family: Family::Serialization,
        media_type: "application/ld+json",
        consumer: "consumerLinkedDataTooling",
        surface: None,
        rel_path: "dist/gmeow-docs/jsonld",
        sub_assets: false,
    },
    DistRow {
        slug: "yamlld",
        family: Family::Serialization,
        media_type: "application/yaml",
        consumer: "consumerLinkedDataTooling",
        surface: None,
        rel_path: "dist/gmeow-docs/yamlld",
        sub_assets: false,
    },
    DistRow {
        slug: "pydantic",
        family: Family::Serialization,
        media_type: "text/x-python",
        consumer: "consumerTypedModelClient",
        surface: None,
        rel_path: "dist/gmeow-docs/pydantic",
        sub_assets: false,
    },
];

/// A shared sub-asset: one of the vendored MCP engine segments, the queryable `gmeow.gts`
/// bundle, or the conjecture demo library that ships INSIDE an interactive distribution
/// rather than as one of its own, content-addressed at a known path in the rendered tree.
///
/// This set used to carry two more rows: a vendored purrdf SPARQL engine and an
/// object-level `gmeow-core.nq` re-serialization for it to parse. Both are retired with the
/// engine that needed them — the MCP segments answer from the bundle row below, which is
/// the same bytes the catalog already had to price.
///
/// These are SUB-ASSETS — never top-level distributions — so they are NOT in
/// [`declared_distribution_slugs`] and the nine-slug bijection is preserved. Their schema
/// rows (family / consumer / media-type) ride here DIGEST-FREE; the per-release content
/// digests live only in the `dist/` instance manifest ([`crate::docs_distribution`]).
///
/// The subjects are SHARED across owners: `site` and `console` ship the byte-identical
/// engine set (`gmeow_docs::console_files` folds `interactive_asset_files` in), so they
/// name the same [`sub_asset_iri`] rather than each minting a private copy of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SerializationDist {
    Okf,
    Jsonld,
    Yamlld,
    Pydantic,
}

impl SerializationDist {
    const ALL: [SerializationDist; 4] = [
        SerializationDist::Okf,
        SerializationDist::Jsonld,
        SerializationDist::Yamlld,
        SerializationDist::Pydantic,
    ];

    fn slug(&self) -> &'static str {
        match self {
            SerializationDist::Okf => "okf",
            SerializationDist::Jsonld => "jsonld",
            SerializationDist::Yamlld => "yamlld",
            SerializationDist::Pydantic => "pydantic",
        }
    }

    fn media_type(&self) -> &'static str {
        match self {
            SerializationDist::Okf => "application/json",
            SerializationDist::Jsonld => "application/ld+json",
            SerializationDist::Yamlld => "application/yaml",
            SerializationDist::Pydantic => "text/x-python",
        }
    }

    fn consumer(&self) -> &'static str {
        match self {
            SerializationDist::Okf => "consumerKnowledgeFederation",
            SerializationDist::Jsonld => "consumerLinkedDataTooling",
            SerializationDist::Yamlld => "consumerLinkedDataTooling",
            SerializationDist::Pydantic => "consumerTypedModelClient",
        }
    }
}

/// A `site` sub-asset: one of the vendored interactive engines or the conjecture
/// demo library the site (and the packed mdbook) ships EXTERNALLY, content-addressed.
/// These are
/// SUB-ASSETS of the `site` distribution — never new top-level distributions — so they
/// are NOT in [`declared_distribution_slugs`] and the eight-slug bijection is preserved.
/// Their schema rows (family / consumer / media-type) ride here DIGEST-FREE; the
/// per-release content digests live only in the `dist/` instance manifest
/// ([`crate::docs_distribution`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SiteSubAsset {
    QueryWasm,
    ValidateWasm,
    ReasonWasm,
    GmnWasm,
    ConjectureDemo,
    /// The standalone console's eagerly-loaded lean MCP engine.
    McpCoreWasm,
    /// The standalone console's demand-loaded MCP reasoning segment.
    McpWasm,
}

impl SiteSubAsset {
    const ALL: [SiteSubAsset; 7] = [
        SiteSubAsset::QueryWasm,
        SiteSubAsset::ValidateWasm,
        SiteSubAsset::ReasonWasm,
        SiteSubAsset::GmnWasm,
        SiteSubAsset::ConjectureDemo,
        SiteSubAsset::McpCoreWasm,
        SiteSubAsset::McpWasm,
    ];

    fn slug(self) -> &'static str {
        match self {
            SiteSubAsset::QueryWasm => "query-wasm",
            SiteSubAsset::ValidateWasm => "validate-wasm",
            SiteSubAsset::ReasonWasm => "reason-wasm",
            SiteSubAsset::GmnWasm => "gmn-wasm",
            SiteSubAsset::ConjectureDemo => "conjectures",
            SiteSubAsset::McpCoreWasm => "mcp-core-wasm",
            SiteSubAsset::McpWasm => "mcp-wasm",
        }
    }

    /// The wasm engines are `application/wasm`; the conjecture demo library is Turtle.
    fn media_type(&self) -> &'static str {
        match self {
            SiteSubAsset::ConjectureDemo => "text/turtle",
            _ => "application/wasm",
        }
    }

    /// The OWNER-tree-relative path (or directory prefix) the sub-asset's bytes ship at,
    /// so the release-time digest producer content-addresses exactly what the catalog
    /// prices. The prefix is the same in every owner's tree, which is what makes the
    /// cross-owner byte-identity check meaningful.
    fn tree_path_prefix(self) -> &'static str {
        match self {
            SiteSubAsset::QueryWasm => "assets/query/",
            SiteSubAsset::ValidateWasm => "assets/validate/",
            SiteSubAsset::ReasonWasm => "assets/reason/",
            SiteSubAsset::GmnWasm => "assets/gmn/",
            SiteSubAsset::ConjectureDemo => "assets/conjectures.ttl",
            SiteSubAsset::McpCoreWasm => "assets/mcp-core/",
            SiteSubAsset::McpWasm => "assets/mcp/",
        }
    }

    /// A human label for the schema row.
    fn label(self) -> &'static str {
        match self {
            SiteSubAsset::QueryWasm => "RDF 1.2 / SPARQL query wasm engine",
            SiteSubAsset::ValidateWasm => "Tier-1 validator wasm engine",
            SiteSubAsset::ReasonWasm => "structured-DL reasoner wasm engine",
            SiteSubAsset::GmnWasm => "GMN-0/GMN-1 codec wasm engine",
            SiteSubAsset::ConjectureDemo => "curated conjecture playground demo library (Turtle)",
            SiteSubAsset::McpCoreWasm => "MCP core segment (the console's first-load engine)",
            SiteSubAsset::McpWasm => "MCP reasoning segment (demand-loaded)",
        }
    }
}

/// Build the distribution catalog graph: the nine distributions, their three families,
/// the shared sub-assets, and (for every surface-bearing row) its declared capability
/// loss, folded into [`GRAPH_DISTRIBUTION_CATALOG`].
pub fn build_distribution_catalog() -> Result<std::sync::Arc<purrdf::RdfDataset>, gmeow_errors::Diag>
{
    let nt = emit_ntriples()?;
    parse_into_graph(&nt, "application/n-triples", GRAPH_DISTRIBUTION_CATALOG)
}

/// The catalog's serialized N-Triples, exactly as [`build_distribution_catalog`] parses
/// them. Exposed so a cross-crate contract gate can read the emitted rows without folding a
/// dataset and without a second, test-local emitter that could drift from this one.
///
/// # Errors
///
/// Propagates a refusal from [`emit_capability_laws`].
pub fn distribution_catalog_ntriples() -> Result<Vec<u8>, gmeow_errors::Diag> {
    emit_ntriples()
}

/// The [`DISTRIBUTIONS`] row for `slug`, or `None` for a slug the catalog does not
/// declare. The one lookup every other by-slug answer folds through.
#[must_use]
pub fn distribution_row(slug: &str) -> Option<&'static DistRow> {
    DISTRIBUTIONS.iter().find(|row| row.slug == slug)
}

/// The declared `gmeow:artifactMediaType` for a distribution slug — read straight off
/// [`DISTRIBUTIONS`], the SAME row this module folds onto the carrier-time catalog
/// subject, so a release-time producer (the external docs fanout, `gmeow-dev sync`) can
/// never consult a second media-type table that drifts from the schema. Returns `None`
/// for an unrecognized slug.
#[must_use]
pub fn media_type_for_slug(slug: &str) -> Option<&'static str> {
    distribution_row(slug).map(|row| row.media_type)
}

/// Every distribution slug the catalog declares — the keys of [`DISTRIBUTIONS`], the ONE
/// authority [`emit_ntriples`] folds. Exposed so a bijection gate can compare against the
/// WHOLE declared set: an added or dropped distribution surfaces as a set-size mismatch
/// rather than passing a subset-presence check.
#[must_use]
pub fn declared_distribution_slugs() -> std::collections::BTreeSet<&'static str> {
    DISTRIBUTIONS.iter().map(|row| row.slug).collect()
}

// ── identity helpers ────────────────────────────────────────────────────────────────

/// The canonical distribution-catalog subject IRI for a distribution slug
/// (`https://blackcatinformatics.ca/gmeow/distribution/dist/<slug>`).
///
/// Defined ONCE in [`gmeow_docs_catalog::identity`] — the catalog READER needs the same
/// subject namespace and may not depend on this build executor to spell it — and
/// re-exported here at its original `pub(crate)` visibility so a release-time instance
/// producer ([`crate::docs_distribution`]) mints the SAME subject its members hang off,
/// never a re-derived string literal.
pub(crate) use gmeow_docs_catalog::identity::dist_iri;

fn family_iri(family: Family) -> String {
    format!("{DISTRIBUTION_BASE}family/{}", family.slug())
}

fn loss_iri(slug: &str, cap_slug: &str) -> String {
    format!("{DISTRIBUTION_BASE}loss/{slug}/{cap_slug}")
}

/// The canonical catalog subject for a shared sub-asset
/// (`…/distribution/sub-asset/<slug>`). `pub(crate)` so the release-time instance producer
/// ([`crate::docs_distribution`]) hangs each sub-asset's `gmeow:contentDigest` off the SAME
/// subject, never a re-derived string.
///
/// The subject deliberately sits OUTSIDE the `…/distribution/dist/` namespace, for two
/// reasons. It is now shared by every owning distribution (`site` and `console` both link
/// to it with `gmeow:hasSubAsset`), so nesting it under one owner would have been a false
/// identity claim. And the previous `…/dist/site/sub-asset/<slug>` shape was actively
/// broken on the consumer side: [`crate::docs_distribution::verify_docs_distribution`]
/// recovers a distribution slug by stripping `…/distribution/dist/` off every manifest
/// subject, so a real release manifest handed it the pseudo-slug
/// `site/sub-asset/mcp-core-wasm` and it hard-failed looking for a `site/sub-asset/…`
/// directory that never existed. Out of the `dist/` namespace, the strip correctly
/// declines and sub-assets no longer masquerade as distributions.
///
/// This is the ONE identity helper that stayed on the writer's side of the
/// `gmeow-docs-catalog` split, alongside the sub-asset vocabulary it is defined over. It
/// is still a single definition site, built on the moved [`DISTRIBUTION_BASE`].
pub(crate) fn sub_asset_iri(slug: &str) -> String {
    format!("{DISTRIBUTION_BASE}sub-asset/{slug}")
}

/// Every `site` sub-asset slug the catalog declares (the vendored interactive engines +
/// the conjecture demo library). Exposed so the release-time instance producer prices the SAME
/// set, and a contract gate can assert these are sub-assets of `site` — NOT members of
/// the eight-slug distribution bijection.
pub fn declared_site_sub_asset_slugs() -> std::collections::BTreeSet<&'static str> {
    SiteSubAsset::ALL.into_iter().map(|s| s.slug()).collect()
}

/// Every [`DISTRIBUTIONS`] row that ships the shared sub-assets, in table order.
fn sub_asset_owners() -> impl Iterator<Item = &'static DistRow> {
    DISTRIBUTIONS.iter().filter(|row| row.sub_assets)
}

/// The distribution slugs that own the shared sub-assets (`site`, `console`), derived
/// from [`DISTRIBUTIONS`]. Exposed so a contract gate can check ownership without
/// restating it.
#[must_use]
pub fn sub_asset_owner_slugs() -> std::collections::BTreeSet<&'static str> {
    sub_asset_owners().map(|row| row.slug).collect()
}

/// The DISTRIBUTION-PARAMETERIZED `(owner slug, sub-asset slug, owner-tree path prefix,
/// media type)` pricing tuples: one per (owner, sub-asset) pair, in a fixed order.
///
/// The release-time digest producer (`gmeow-dev sync`) iterates this to content-address
/// each sub-asset OUT OF ITS OWNER'S OWN rendered tree and hang its `gmeow:contentDigest`
/// off the SAME shared [`sub_asset_iri`] subject the carrier catalog prices digest-free.
/// Because the subject is shared, two owners pricing the same sub-asset to two different
/// digests is a contradiction the producer must refuse rather than silently publish — see
/// `gmeow-dev-cli`'s `price_sub_assets`.
///
/// This replaced a site-only `site_sub_asset_pricing()`: with the console a shipped
/// distribution that ships the identical engine set, a site-only pricing would have left
/// the console's copy of a 7 MB wasm image with no release digest at all.
#[must_use]
pub fn sub_asset_pricing() -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    sub_asset_owners()
        .flat_map(|owner| {
            SiteSubAsset::ALL.into_iter().map(move |sub| {
                (
                    owner.slug,
                    sub.slug(),
                    sub.tree_path_prefix(),
                    sub.media_type(),
                )
            })
        })
        .collect()
}

/// The DECLARED `gmeow:ProjectionCapability` individual a capability is. The kernel owns
/// this value vocabulary, exactly as it owns the `gmeow:ProjectionContext` consumers this
/// module already references by name through [`consumer_iri`] — so a loss node's accounted
/// parameter and a concept's intent member are the SAME six declared individuals, never a
/// second, catalog-local capability namespace.
///
/// `pub(crate)` because it is the ONE spelling authority for these six local names: the
/// docs-distribution release producer's matrix gate reads the expected local name back
/// through [`gmeow_docs_catalog::identity::local_name`] over this function, rather than
/// keeping a second copy of the table that a rename would silently fork.
pub(crate) fn capability_iri(cap: Capability) -> String {
    let local = match cap {
        Capability::SearchIndex => "capabilitySearchIndex",
        Capability::LiveSparql => "capabilityLiveSparql",
        Capability::Interactivity => "capabilityInteractivity",
        Capability::LiveReasoning => "capabilityLiveReasoning",
        Capability::Diagrams => "capabilityDiagrams",
        Capability::CrossLinkFidelity => "capabilityCrossLinkFidelity",
    };
    iri(GMEOW_NS, local)
}

/// The catalog subject for a capability-bearing projection SURFACE — the object column of
/// the formal context.
///
/// Every surface IS a shipped distribution now (the console became the ninth), so this is
/// exactly [`dist_iri`] over the surface's slug — one subject per surface, not a parallel
/// `…/surface/<slug>` namespace shadowing the distribution one.
/// `every_surface_is_a_declared_distribution` gates the correspondence.
fn surface_iri(surface: DistributionSurface) -> String {
    dist_iri(surface.slug())
}

/// The subject of one derived formal concept, named by its extent so the identity is
/// content-addressed: adding a surface renames only the concepts it actually joins, and no
/// concept carries a positional index that a later edit would silently renumber.
fn concept_iri(concept: SurfaceConcept) -> String {
    let extent: Vec<&str> = concept
        .extent
        .members()
        .iter()
        .map(|surface| surface.slug())
        .collect();
    let name = if extent.is_empty() {
        "empty".to_owned()
    } else {
        extent.join("+")
    };
    format!("{DISTRIBUTION_BASE}concept/{name}")
}

/// The subject of one Duquenne–Guigues law, named by its premise (pseudo-intents are
/// pairwise distinct, so the name is a key).
fn law_iri(implication: Implication) -> String {
    let premise: Vec<&str> = implication
        .premise
        .members()
        .iter()
        .map(|cap| cap.slug())
        .collect();
    format!("{DISTRIBUTION_BASE}law/{}", premise.join("+"))
}

/// A term carrier shared by every law atom: the quantified surface variable, and one
/// carrier per capability. Sharing them keeps the emitted AST linear in the basis size
/// instead of re-minting two carriers per atom.
fn law_term_iri(name: &str) -> String {
    format!("{DISTRIBUTION_BASE}law/term/{name}")
}

fn consumer_iri(name: &str) -> String {
    format!("{GMEOW_NS}{name}")
}

/// The `gmeow:` predicate/class IRI builder, and the N-Triples subject/predicate/object
/// formatters this module emits every catalog row through.
///
/// All three are defined ONCE in [`gmeow_docs_catalog::identity`] and re-exported here at
/// their original `pub(crate)` visibility. The READ side needs `iri` to spell the very
/// predicates this module writes, and it may not depend on this build executor to do it —
/// so the shared strings live in the leaf and the emitter borrows them, never the reverse
/// and never a second copy. That is also what keeps the N-Triples escaping/quoting
/// convention from forking between the emitter here and
/// [`crate::docs_distribution`]'s release-instance emitter.
pub(crate) use gmeow_docs_catalog::identity::{XSD_INTEGER, iri, triple, triple_lit, triple_typed};

/// The four a-box skeleton triples every subject this module emits carries:
/// `rdf:type`, `rdfs:isDefinedBy <graph/distribution-catalog>`, `gmeow:graphBoxRole
/// gmeow:boxABox`, and `rdfs:label`.
fn skeleton(lines: &mut Vec<String>, subject: &str, rdf_type: &str, label: &str) {
    lines.push(triple(subject, RDF_TYPE, rdf_type));
    lines.push(triple(
        subject,
        RDFS_IS_DEFINED_BY,
        GRAPH_DISTRIBUTION_CATALOG,
    ));
    lines.push(triple(
        subject,
        &iri(GMEOW_NS, "graphBoxRole"),
        &iri(GMEOW_NS, "boxABox"),
    ));
    lines.push(triple_lit(subject, RDFS_LABEL, label));
}

/// Emit one surface's BOTH-SIDED capability ledger: what it carries
/// (`gmeow:representableParameter`) and what it drops (`gmeow:declaredLoss` →
/// `gmeow:ProjectionLoss` → `gmeow:accountsForParameter`).
///
/// Both halves are read from the single authority
/// ([`gmeow_docs::formats::surface_capabilities`]), which is why the catalog's ledger and
/// the renderer's loss appendix cannot drift, and why the derived Duquenne–Guigues laws
/// below are checkable against this very graph rather than floating above it. The subject
/// must be a `gmeow:LossBearingProfile` — the class both predicates take as their domain.
fn emit_capability_ledger(lines: &mut Vec<String>, subject: &str, surface: DistributionSurface) {
    let slug = surface.slug();
    let caps = surface_capabilities(surface);
    for cap in &caps.representable {
        lines.push(triple(
            subject,
            &iri(GMEOW_NS, "representableParameter"),
            &capability_iri(*cap),
        ));
    }
    for cap in &caps.dropped {
        let cap_slug = cap.slug();
        let loss_node = loss_iri(slug, cap_slug);
        lines.push(triple(subject, &iri(GMEOW_NS, "declaredLoss"), &loss_node));
        skeleton(
            lines,
            &loss_node,
            &iri(GMEOW_NS, "ProjectionLoss"),
            &format!("projection surface {slug} declared loss {cap_slug}"),
        );
        lines.push(triple(
            &loss_node,
            &iri(GMEOW_NS, "accountsForParameter"),
            &capability_iri(*cap),
        ));
    }
}

/// Emit the shared sub-assets: one digest-free schema row per [`SubAsset`], linked from
/// EVERY owning distribution ([`sub_asset_owners`]) with `gmeow:hasSubAsset`.
///
/// One subject per sub-asset, not one per (owner, sub-asset) pair: `site` and `console`
/// ship the byte-identical engine set, so two subjects would have asserted that the two
/// copies are different components and would have doubled the release-manifest rows for a
/// single shipped artifact. Each owner's consumer rides onto the shared node, so the
/// sub-asset's audience is the union of its owners' audiences rather than a hard-coded one.
fn emit_sub_assets(lines: &mut Vec<String>) {
    for sub in SiteSubAsset::ALL {
        let node = sub_asset_iri(sub.slug());
        skeleton(lines, &node, &iri(GMEOW_NS, "SiteSubAsset"), sub.label());
        lines.push(triple_lit(
            &node,
            &iri(GMEOW_NS, "distributionFormat"),
            sub.slug(),
        ));
        // The engines, the browser bundle, and the demo library are emitted ONLY by an
        // interactive render — they are what makes a surface a live runtime — so the
        // interactive-runtime family is their honest classification.
        lines.push(triple(
            &node,
            &iri(GMEOW_NS, "distributionFamily"),
            &family_iri(Family::InteractiveRuntime),
        ));
        lines.push(triple_lit(
            &node,
            &iri(GMEOW_NS, "artifactMediaType"),
            sub.media_type(),
        ));
        for owner in sub_asset_owners() {
            lines.push(triple(
                &dist_iri(owner.slug),
                &iri(GMEOW_NS, "hasSubAsset"),
                &node,
            ));
            lines.push(triple(
                &node,
                &iri(GMEOW_NS, "eligibleForConsumer"),
                &consumer_iri(owner.consumer),
            ));
        }
    }
}

/// Emit the COMPLETE concept-lattice element set: every formal concept of the
/// surface × capability incidence, with its extent and its intent.
///
/// Digest-free, like the rest of this carrier-time catalog. The order between concepts is
/// deliberately NOT stored as edges: extent inclusion recovers it exactly, so an emitted
/// edge would be a second, drift-prone encoding of the same fact.
fn emit_concept_lattice(lines: &mut Vec<String>) {
    for concept in authored_concepts() {
        let node = concept_iri(concept);
        let extent: Vec<&str> = concept
            .extent
            .members()
            .iter()
            .map(|surface| surface.slug())
            .collect();
        let intent: Vec<&str> = concept
            .intent
            .members()
            .iter()
            .map(|cap| cap.slug())
            .collect();
        skeleton(
            lines,
            &node,
            &iri(GMEOW_NS, "FormalConcept"),
            &format!(
                "formal concept: surfaces [{}] share capabilities [{}]",
                extent.join(", "),
                intent.join(", ")
            ),
        );
        for surface in concept.extent.members() {
            lines.push(triple(
                &node,
                &iri(GMEOW_NS, "conceptExtent"),
                &surface_iri(surface),
            ));
        }
        for cap in concept.intent.members() {
            lines.push(triple(
                &node,
                &iri(GMEOW_NS, "conceptIntent"),
                &capability_iri(cap),
            ));
        }
    }
}

/// Emit one side of a law as a `logic:` formula node, returning its subject: a bare atom
/// when the side is a single capability, an explicit `logic:and` conjunction otherwise.
fn emit_law_side(lines: &mut Vec<String>, law: &str, side: &str, mask: CapMask) -> Option<String> {
    let members = mask.members();
    if members.is_empty() {
        return None;
    }
    let atoms: Vec<String> = members
        .iter()
        .map(|cap| {
            let atom = format!("{law}/{side}/{}", cap.slug());
            skeleton(
                lines,
                &atom,
                &iri(LOGIC_NS, "Formula"),
                &format!("law atom: the surface represents {}", cap.slug()),
            );
            lines.push(triple(
                &atom,
                &iri(LOGIC_NS, "relation"),
                &iri(GMEOW_NS, "representableParameter"),
            ));
            lines.push(triple(
                &atom,
                &iri(LOGIC_NS, "argument"),
                &law_term_iri("surface"),
            ));
            lines.push(triple(
                &atom,
                &iri(LOGIC_NS, "argument"),
                &law_term_iri(cap.slug()),
            ));
            atom
        })
        .collect();
    if let [only] = atoms.as_slice() {
        return Some(only.clone());
    }
    let conjunction = format!("{law}/{side}");
    skeleton(
        lines,
        &conjunction,
        &iri(LOGIC_NS, "Formula"),
        &format!("law {side}: a conjunction of {} atoms", atoms.len()),
    );
    for atom in &atoms {
        lines.push(triple(&conjunction, &iri(LOGIC_NS, "and"), atom));
    }
    Some(conjunction)
}

/// Emit the Duquenne–Guigues implication basis of the incidence as derived catalog LAWS —
/// `logic:Formula` ASTs with `logic:antecedent` / `logic:consequent`, the repo's one
/// representation for a law. No implication vocabulary is minted here.
///
/// A law whose premise no surface exhibits holds only VACUOUSLY over the authored context;
/// such a law additionally carries `logic:expressivenessBoundary logic:FirstOrder`, marking
/// it as an honest gap rather than a witnessed catalog fact.
///
/// # Errors
///
/// Refuses when a basis premise is EMPTY. An `∅ → C` law is not representable in the
/// implication AST (an antecedent-free implication is not an implication), and emitting the
/// law without its antecedent would silently publish a strictly stronger claim than the
/// context supports. It cannot arise while some surface has an empty intent — which makes
/// `∅` closed — so this is a fail-closed guard, not a live branch.
fn emit_capability_laws(lines: &mut Vec<String>) -> Result<(), gmeow_errors::Diag> {
    let basis = authored_dg_basis();

    // One shared carrier for the quantified variable, and one per capability. Every atom
    // below argues over exactly these, so the AST is linear in the basis size.
    let variable = law_term_iri("surface");
    skeleton(
        lines,
        &variable,
        &iri(LOGIC_NS, "TermCarrier"),
        "law term carrier: the quantified projection surface",
    );
    lines.push(triple_typed(
        &variable,
        &iri(LOGIC_NS, "termIndex"),
        "0",
        XSD_INTEGER,
    ));
    lines.push(triple_lit(
        &variable,
        &iri(LOGIC_NS, "termVariable"),
        "surface",
    ));
    for cap in Capability::ALL {
        let carrier = law_term_iri(cap.slug());
        skeleton(
            lines,
            &carrier,
            &iri(LOGIC_NS, "TermCarrier"),
            &format!("law term carrier: capability {}", cap.slug()),
        );
        lines.push(triple_typed(
            &carrier,
            &iri(LOGIC_NS, "termIndex"),
            "1",
            XSD_INTEGER,
        ));
        lines.push(triple(
            &carrier,
            &iri(LOGIC_NS, "termIri"),
            &capability_iri(cap),
        ));
    }

    for implication in basis {
        let law = law_iri(implication);
        let premise: Vec<&str> = implication
            .premise
            .members()
            .iter()
            .map(|cap| cap.slug())
            .collect();
        let conclusion: Vec<&str> = implication
            .conclusion
            .members()
            .iter()
            .map(|cap| cap.slug())
            .collect();

        skeleton(
            lines,
            &law,
            &iri(LOGIC_NS, "Formula"),
            &format!(
                "derived catalog law: a surface representing [{}] represents [{}]",
                premise.join(", "),
                conclusion.join(", ")
            ),
        );
        lines.push(triple(
            &law,
            &iri(LOGIC_NS, "quantifiedVariable"),
            &variable,
        ));

        let body = format!("{law}/implication");
        lines.push(triple(&law, &iri(LOGIC_NS, "forall"), &body));
        skeleton(
            lines,
            &body,
            &iri(LOGIC_NS, "Formula"),
            "derived catalog law: the quantified implication body",
        );

        let antecedent =
            emit_law_side(lines, &law, "premise", implication.premise).ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::Projection {
                    message: format!(
                        "the Duquenne-Guigues basis yielded a law with an EMPTY premise \
                     (conclusion [{}]); an antecedent-free implication cannot be represented \
                     as a logic:Formula AST without publishing a stronger claim than the \
                     incidence supports",
                        conclusion.join(", ")
                    ),
                })
            })?;
        // A basis implication's conclusion is `premise″ ∖ premise`, which is non-empty by
        // construction (a pseudo-closed premise is never closed), so this cannot be `None`.
        let consequent = emit_law_side(lines, &law, "conclusion", implication.conclusion)
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::Projection {
                    message: format!(
                        "the Duquenne-Guigues law with premise [{}] has an EMPTY conclusion, which \
                     contradicts its premise being pseudo-closed",
                        premise.join(", ")
                    ),
                })
            })?;
        lines.push(triple(&body, &iri(LOGIC_NS, "antecedent"), &antecedent));
        lines.push(triple(&body, &iri(LOGIC_NS, "consequent"), &consequent));

        if implication.is_unrealized(&AUTHORED_INCIDENCE) {
            lines.push(triple(
                &law,
                &iri(LOGIC_NS, "expressivenessBoundary"),
                &iri(LOGIC_NS, "FirstOrder"),
            ));
        }
    }
    Ok(())
}

/// Emit the sorted, deduped, byte-stable N-Triples for the whole catalog.
///
/// # Errors
///
/// Propagates a refusal from [`emit_capability_laws`].
fn emit_ntriples() -> Result<Vec<u8>, gmeow_errors::Diag> {
    let mut lines: Vec<String> = Vec::new();

    // ── family nodes: Family::ALL, never an inline literal ──
    for family in Family::ALL {
        let fam = family_iri(family);
        skeleton(
            &mut lines,
            &fam,
            &iri(GMEOW_NS, "DistributionFamily"),
            &format!("documentation distribution family {}", family.slug()),
        );
    }

    // ── the distributions: one loop over the ONE table ──
    for row in DISTRIBUTIONS {
        let dist = dist_iri(row.slug);
        skeleton(
            &mut lines,
            &dist,
            &iri(GMEOW_NS, "DocumentationDistribution"),
            &format!("documentation distribution {}", row.slug),
        );
        lines.push(triple_lit(
            &dist,
            &iri(GMEOW_NS, "distributionFormat"),
            row.slug,
        ));
        lines.push(triple(
            &dist,
            &iri(GMEOW_NS, "distributionFamily"),
            &family_iri(row.family),
        ));
        lines.push(triple_lit(
            &dist,
            &iri(GMEOW_NS, "artifactMediaType"),
            row.media_type,
        ));
        lines.push(triple(
            &dist,
            &iri(GMEOW_NS, "eligibleForConsumer"),
            &consumer_iri(row.consumer),
        ));

        // The capability ledger, SOURCED FROM the single authority — never re-authored
        // here, and identical in kind for the console and for `site`.
        // `gmeow:DocumentationDistribution` is a `gmeow:LossBearingProfile`, which is what
        // makes it a legal subject of `gmeow:declaredLoss`. A serialization row has no
        // surface and therefore no ledger at all.
        if let Some(surface) = row.surface {
            emit_capability_ledger(&mut lines, &dist, surface);
        }
    }

    // ── serialization distributions: no declaredLoss ──
    for dist_enum in SerializationDist::ALL {
        let slug = dist_enum.slug();
        let dist = dist_iri(slug);
        skeleton(
            &mut lines,
            &dist,
            &iri(GMEOW_NS, "DocumentationDistribution"),
            &format!("documentation distribution {slug}"),
        );
        lines.push(triple_lit(
            &dist,
            &iri(GMEOW_NS, "distributionFormat"),
            slug,
        ));
        lines.push(triple(
            &dist,
            &iri(GMEOW_NS, "distributionFamily"),
            &family_iri(Family::Serialization),
        ));
        lines.push(triple_lit(
            &dist,
            &iri(GMEOW_NS, "artifactMediaType"),
            dist_enum.media_type(),
        ));
        lines.push(triple(
            &dist,
            &iri(GMEOW_NS, "eligibleForConsumer"),
            &consumer_iri(dist_enum.consumer()),
        ));
    }

    // ── site sub-assets: the vendored interactive engines + the conjecture demo library ──
    // First-class schema rows, DIGEST-FREE (the per-release content digests ride only in
    // the `dist/` instance manifest). Hung off EVERY owning distribution via
    // gmeow:hasSubAsset, so they are sub-assets — NOT top-level distributions — and the
    // nine-slug bijection is untouched.
    emit_sub_assets(&mut lines);

    // ── the DERIVED half: the complete concept lattice and its implication basis ──
    // Both are computed from the surface × capability incidence, never authored here.
    emit_concept_lattice(&mut lines);
    emit_capability_laws(&mut lines)?;

    lines.sort();
    lines.dedup();
    let mut out = lines.join("\n");
    out.push('\n');
    Ok(out.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ntriples_text() -> String {
        // Building via the real entry point exercises parse_into_graph end-to-end (a
        // parse failure here would fail the test), then the string-content assertions
        // below grep the exact bytes the emitter produced (what parse_into_graph parsed).
        let _ds = build_distribution_catalog().expect("build distribution catalog");
        String::from_utf8(emit_ntriples().expect("emit catalog")).expect("utf8 n-triples")
    }

    #[test]
    fn catalog_is_byte_reproducible() {
        let a = emit_ntriples().expect("emit catalog");
        let b = emit_ntriples().expect("emit catalog");
        assert_eq!(a, b, "distribution catalog N-Triples must be deterministic");
    }

    #[test]
    fn every_slug_appears_as_a_distribution_format() {
        let nt = ntriples_text();
        let pred = iri(GMEOW_NS, "distributionFormat");
        for row in DISTRIBUTIONS {
            let needle = format!("<{}> \"{}\" .", pred, row.slug);
            assert!(
                nt.lines().any(|l| l.contains(&needle)),
                "missing gmeow:distributionFormat for slug {:?}",
                row.slug
            );
        }
    }

    /// The table's own internal consistency: slugs are unique, a row that claims a surface
    /// agrees with that surface's slug, and EVERY declared surface is a row. Without this,
    /// `surface_iri` could mint a subject no distribution row backs (or two rows could
    /// claim the same one) and the concept lattice's extents would point at nothing.
    #[test]
    fn the_table_agrees_with_the_surface_vocabulary() {
        let slugs = declared_distribution_slugs();
        assert_eq!(
            slugs.len(),
            DISTRIBUTIONS.len(),
            "DISTRIBUTIONS carries a duplicate slug: {slugs:?}"
        );
        for row in DISTRIBUTIONS {
            if let Some(surface) = row.surface {
                assert_eq!(
                    row.slug,
                    surface.slug(),
                    "row {:?} claims surface {surface:?}, whose slug disagrees",
                    row.slug
                );
            }
            assert_eq!(
                row.rel_path,
                format!("dist/gmeow-docs/{}", row.slug),
                "row {:?} must ship under the shared docs-distribution base",
                row.slug
            );
        }
        // Every capability-bearing surface is a shipped distribution — which is what makes
        // `surface_iri` = `dist_iri` sound.
        for surface in DistributionSurface::ALL {
            assert!(
                slugs.contains(surface.slug()),
                "surface {surface:?} is not a declared distribution; surface_iri would mint \
                 a subject no row backs"
            );
        }
        // …and only a serialization row lacks one.
        for row in DISTRIBUTIONS {
            assert_eq!(
                row.surface.is_none(),
                row.family == Family::Serialization,
                "row {:?}: a surface-free row must be exactly a serialization row",
                row.slug
            );
        }
    }

    /// `Family::ALL` really is every family the table uses — the D-j guard. A row naming a
    /// family absent from `ALL` would emit a dangling `gmeow:distributionFamily` IRI.
    #[test]
    fn every_row_family_has_an_emitted_family_node() {
        let nt = ntriples_text();
        for row in DISTRIBUTIONS {
            assert!(
                Family::ALL.contains(&row.family),
                "row {:?} names family {:?}, which is absent from Family::ALL — its family \
                 node would never be emitted",
                row.slug,
                row.family
            );
            assert!(
                nt.contains(&triple(
                    &family_iri(row.family),
                    RDF_TYPE,
                    &iri(GMEOW_NS, "DistributionFamily")
                )),
                "family {:?} has no emitted gmeow:DistributionFamily node",
                row.family.slug()
            );
        }
        for family in Family::ALL {
            assert!(
                nt.contains(&triple(
                    &family_iri(family),
                    RDF_TYPE,
                    &iri(GMEOW_NS, "DistributionFamily")
                )),
                "Family::ALL member {:?} emits no family node",
                family.slug()
            );
        }
    }

    /// `media_type_for_slug` and `declared_distribution_slugs` are FOLDS over the table,
    /// not parallel copies: every row answers with its own media type, and nothing else
    /// answers at all.
    #[test]
    fn the_by_slug_lookups_fold_over_the_table() {
        for row in DISTRIBUTIONS {
            assert_eq!(media_type_for_slug(row.slug), Some(row.media_type));
            assert_eq!(distribution_row(row.slug), Some(&row));
        }
        assert_eq!(media_type_for_slug("not-a-distribution"), None);
        assert_eq!(distribution_row("not-a-distribution"), None);
        for slug in declared_site_sub_asset_slugs() {
            assert_eq!(
                media_type_for_slug(slug),
                None,
                "sub-asset {slug:?} must not answer as a distribution"
            );
        }
    }

    #[test]
    fn every_subject_carries_the_four_skeleton_triples() {
        let nt = ntriples_text();
        // Collect every distinct subject IRI under DISTRIBUTION_BASE.
        let mut subjects: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for line in nt.lines() {
            if let Some(rest) = line.strip_prefix('<')
                && let Some(end) = rest.find('>')
            {
                let subject = &rest[..end];
                if subject.starts_with(DISTRIBUTION_BASE) {
                    subjects.insert(subject.to_string());
                }
            }
        }
        assert!(
            subjects.len() >= DISTRIBUTIONS.len() + Family::ALL.len(),
            "expected at least {} distributions + {} families, got {}",
            DISTRIBUTIONS.len(),
            Family::ALL.len(),
            subjects.len()
        );
        for subject in &subjects {
            assert!(
                nt.contains(&format!("<{subject}> <{RDF_TYPE}>")),
                "{subject} missing rdf:type"
            );
            assert!(
                nt.contains(&triple(
                    subject,
                    RDFS_IS_DEFINED_BY,
                    GRAPH_DISTRIBUTION_CATALOG
                )),
                "{subject} missing rdfs:isDefinedBy <{GRAPH_DISTRIBUTION_CATALOG}>"
            );
            assert!(
                nt.contains(&triple(
                    subject,
                    &iri(GMEOW_NS, "graphBoxRole"),
                    &iri(GMEOW_NS, "boxABox")
                )),
                "{subject} missing gmeow:graphBoxRole gmeow:boxABox"
            );
            assert!(
                nt.contains(&format!("<{subject}> <{RDFS_LABEL}>")),
                "{subject} missing rdfs:label"
            );
        }
    }

    #[test]
    fn sub_assets_are_priced_digest_free_and_outside_the_bijection() {
        let nt = ntriples_text();
        let bijection = declared_distribution_slugs();
        assert_eq!(
            bijection.len(),
            9,
            "the nine-slug bijection must hold: {bijection:?}"
        );

        // Ownership is SHARED: both `site` and `console` ship the identical engine set, so
        // both must link to the same subjects. A single owner here would leave the other
        // distribution's copy of a 7 MB wasm image unpriced on the release path.
        let owners = sub_asset_owner_slugs();
        assert_eq!(
            owners,
            std::collections::BTreeSet::from(["site", "console"]),
            "the shared sub-assets must be owned by exactly the two interactive surfaces"
        );

        for slug in declared_site_sub_asset_slugs() {
            // NOT a top-level distribution — the bijection is preserved.
            assert!(
                !bijection.contains(slug),
                "sub-asset {slug:?} must NOT be a top-level distribution slug"
            );
            let node = sub_asset_iri(slug);
            // Typed as a SiteSubAsset and hung off EVERY owning distribution.
            assert!(
                nt.contains(&triple(&node, RDF_TYPE, &iri(GMEOW_NS, "SiteSubAsset"))),
                "sub-asset {slug:?} missing rdf:type gmeow:SiteSubAsset"
            );
            for owner in &owners {
                assert!(
                    nt.contains(&triple(
                        &dist_iri(owner),
                        &iri(GMEOW_NS, "hasSubAsset"),
                        &node
                    )),
                    "{owner} distribution must declare gmeow:hasSubAsset {slug:?}"
                );
            }
            // The subject sits OUTSIDE the `dist/` namespace, so the consumer-side
            // `verify_docs_distribution` slug strip cannot mistake it for a distribution
            // directory (it used to, and hard-failed on every real release manifest).
            assert!(
                !node.starts_with(&format!("{DISTRIBUTION_BASE}dist/")),
                "sub-asset subject {node} must not live under the distribution namespace"
            );
            // Schema row present, DIGEST-FREE (no contentDigest in the carrier catalog).
            assert!(
                nt.contains(&format!(
                    "<{node}> <{}>",
                    iri(GMEOW_NS, "artifactMediaType")
                )),
                "sub-asset {slug:?} missing artifactMediaType"
            );
            // DIGEST-FREE: no line about this sub-asset mentions a content digest (the
            // per-release digest rides only in the dist/ instance manifest).
            assert!(
                !nt.lines()
                    .any(|l| l.starts_with(&format!("<{node}>")) && l.contains("Digest")),
                "sub-asset {slug:?} must be digest-free in the carrier catalog (digests \
                 live only in the dist/ instance manifest)"
            );
        }
    }

    #[test]
    fn doc_render_declared_loss_matches_format_capabilities_exactly() {
        let nt = ntriples_text();
        for fmt in DocFormat::ALL {
            let slug = fmt.slug();
            let dist = dist_iri(slug);
            let caps = surface_capabilities(DistributionSurface::Format(fmt));
            for cap in Capability::ALL {
                let loss_node = loss_iri(slug, cap.slug());
                let declares =
                    nt.contains(&triple(&dist, &iri(GMEOW_NS, "declaredLoss"), &loss_node));
                let is_dropped = caps.dropped.contains(&cap);
                assert_eq!(
                    declares, is_dropped,
                    "{slug}/{:?}: catalog declaredLoss ({declares}) disagrees with \
                     format_capabilities().dropped ({is_dropped}) — single-authority drift",
                    cap
                );
                if is_dropped {
                    assert!(
                        nt.contains(&triple(
                            &loss_node,
                            &iri(GMEOW_NS, "accountsForParameter"),
                            &capability_iri(cap)
                        )),
                        "{slug}/{:?} loss node missing accountsForParameter",
                        cap
                    );
                }
            }
        }
    }

    /// The console is the NINTH shipped distribution: a full catalog row (family, media
    /// type, audience, format slug) whose capability ledger is DERIVED from the surface
    /// lattice, exactly like `site`'s — not authored anywhere in this module.
    #[test]
    fn the_console_is_the_ninth_distribution_with_a_derived_ledger() {
        let nt = ntriples_text();
        let console = dist_iri("console");
        let row = distribution_row("console").expect("the console is a declared distribution");
        assert_eq!(row.family, Family::InteractiveRuntime);
        assert_eq!(row.media_type, "text/html");
        assert_eq!(row.consumer, "consumerInteractiveConsole");
        assert_eq!(row.surface, Some(DistributionSurface::Console));

        assert!(
            nt.contains(&triple(
                &console,
                RDF_TYPE,
                &iri(GMEOW_NS, "DocumentationDistribution")
            )),
            "the console must be typed as a shipped distribution"
        );
        assert!(
            nt.contains(&triple_lit(
                &console,
                &iri(GMEOW_NS, "distributionFormat"),
                "console"
            )),
            "the console must carry its distribution format slug"
        );
        assert!(
            nt.contains(&triple(
                &console,
                &iri(GMEOW_NS, "distributionFamily"),
                &family_iri(Family::InteractiveRuntime)
            )),
            "the console must belong to the interactive-runtime family"
        );
        assert!(
            nt.contains(&triple_lit(
                &console,
                &iri(GMEOW_NS, "artifactMediaType"),
                "text/html"
            )),
            "the console must declare its media type"
        );
        assert!(
            nt.contains(&triple(
                &console,
                &iri(GMEOW_NS, "eligibleForConsumer"),
                &consumer_iri("consumerInteractiveConsole")
            )),
            "the console must name its declared audience"
        );
        assert_eq!(
            declared_distribution_slugs().len(),
            9,
            "the nine-slug bijection must include the console"
        );

        // Its ledger is the DERIVED partition, both halves — read from the same authority
        // `site` and `pdf` read, never restated in this table.
        let caps = surface_capabilities(DistributionSurface::Console);
        assert!(
            !caps.dropped.is_empty() && !caps.representable.is_empty(),
            "a vacuous console partition would make this gate meaningless: {caps:?}"
        );
        for cap in &caps.dropped {
            let loss_node = loss_iri("console", cap.slug());
            assert!(
                nt.contains(&triple(
                    &console,
                    &iri(GMEOW_NS, "declaredLoss"),
                    &loss_node
                )),
                "the console's derived loss of {:?} is missing from the catalog",
                cap.slug()
            );
            assert!(nt.contains(&triple(
                &loss_node,
                &iri(GMEOW_NS, "accountsForParameter"),
                &capability_iri(*cap)
            )));
        }
        for cap in &caps.representable {
            assert!(nt.contains(&triple(
                &console,
                &iri(GMEOW_NS, "representableParameter"),
                &capability_iri(*cap)
            )));
        }
    }

    /// Every surface's ledger is TOTAL over the capability vocabulary: each capability is
    /// either represented or accounted for as a loss, never both and never neither. That
    /// pairing is exactly what makes the derived laws below checkable against this graph.
    #[test]
    fn every_surface_ledger_is_total_over_the_capabilities() {
        let nt = ntriples_text();
        for surface in DistributionSurface::ALL {
            let subject = surface_iri(surface);
            for cap in Capability::ALL {
                let represents = nt.contains(&triple(
                    &subject,
                    &iri(GMEOW_NS, "representableParameter"),
                    &capability_iri(cap),
                ));
                let drops = nt.contains(&triple(
                    &subject,
                    &iri(GMEOW_NS, "declaredLoss"),
                    &loss_iri(surface.slug(), cap.slug()),
                ));
                assert!(
                    represents ^ drops,
                    "{}/{cap:?}: the catalog ledger must place it in exactly one side \
                     (represents={represents}, drops={drops})",
                    surface.slug()
                );
            }
        }
    }

    /// The COMPLETE concept lattice is emitted — every derived concept, with its extent and
    /// its intent — and nothing beyond it.
    #[test]
    fn the_complete_concept_lattice_is_emitted() {
        let nt = ntriples_text();
        let derived = authored_concepts();
        assert_eq!(derived.len(), 4, "{derived:?}");

        for concept in &derived {
            let node = concept_iri(*concept);
            assert!(
                nt.contains(&triple(&node, RDF_TYPE, &iri(GMEOW_NS, "FormalConcept"))),
                "{node} is not typed gmeow:FormalConcept — the reader would drop it"
            );
            for surface in concept.extent.members() {
                assert!(nt.contains(&triple(
                    &node,
                    &iri(GMEOW_NS, "conceptExtent"),
                    &surface_iri(surface)
                )));
            }
            for cap in concept.intent.members() {
                assert!(nt.contains(&triple(
                    &node,
                    &iri(GMEOW_NS, "conceptIntent"),
                    &capability_iri(cap)
                )));
            }
        }

        // No EXTRA concept: the count of typed nodes matches the derived set exactly.
        let emitted = nt
            .lines()
            .filter(|line| line.ends_with(&format!("<{}> .", iri(GMEOW_NS, "FormalConcept"))))
            .count();
        assert_eq!(
            emitted,
            derived.len(),
            "the emitted lattice is not complete"
        );
    }

    /// The Duquenne–Guigues basis rides as `logic:Formula` ASTs — the repo's one law
    /// representation — with `logic:antecedent` / `logic:consequent`. No implication
    /// vocabulary is minted.
    #[test]
    fn the_implication_basis_rides_as_logic_formula_asts() {
        let nt = ntriples_text();
        let basis = authored_dg_basis();
        assert_eq!(basis.len(), 6, "{basis:?}");

        for implication in &basis {
            let law = law_iri(*implication);
            let body = format!("{law}/implication");
            assert!(nt.contains(&triple(&law, RDF_TYPE, &iri(LOGIC_NS, "Formula"))));
            assert!(nt.contains(&triple(
                &law,
                &iri(LOGIC_NS, "quantifiedVariable"),
                &law_term_iri("surface")
            )));
            assert!(nt.contains(&triple(&law, &iri(LOGIC_NS, "forall"), &body)));
            assert!(
                nt.lines()
                    .any(|line| line
                        .starts_with(&format!("<{body}> <{}>", iri(LOGIC_NS, "antecedent")))),
                "law {law} has no logic:antecedent"
            );
            assert!(
                nt.lines()
                    .any(|line| line
                        .starts_with(&format!("<{body}> <{}>", iri(LOGIC_NS, "consequent")))),
                "law {law} has no logic:consequent"
            );
            // Every atom argues the shared surface variable against a capability carrier,
            // over the ONE existing predicate — no minted implication vocabulary.
            for cap in implication.premise.members() {
                let atom = format!("{law}/premise/{}", cap.slug());
                assert!(nt.contains(&triple(
                    &atom,
                    &iri(LOGIC_NS, "relation"),
                    &iri(GMEOW_NS, "representableParameter")
                )));
                assert!(nt.contains(&triple(
                    &atom,
                    &iri(LOGIC_NS, "argument"),
                    &law_term_iri(cap.slug())
                )));
            }
        }

        // The one honest-gap marker, and only where the derivation says so: a law whose
        // premise no surface exhibits. Over the authored incidence there is none, and the
        // emitted marker set must agree with that derivation rather than with a guess.
        let unrealized: Vec<_> = basis
            .iter()
            .filter(|i| i.is_unrealized(&AUTHORED_INCIDENCE))
            .collect();
        let emitted_markers = nt
            .lines()
            .filter(|line| line.contains(&iri(LOGIC_NS, "expressivenessBoundary")))
            .count();
        assert_eq!(
            emitted_markers,
            unrealized.len(),
            "the expressiveness-boundary markers must be exactly the vacuously-true laws"
        );
        for implication in unrealized {
            assert!(nt.contains(&triple(
                &law_iri(*implication),
                &iri(LOGIC_NS, "expressivenessBoundary"),
                &iri(LOGIC_NS, "FirstOrder")
            )));
        }
    }

    /// Every DG law actually HOLDS of the emitted ledger — the laws are checkable against
    /// this graph, not floating above it.
    #[test]
    fn every_emitted_law_holds_of_the_emitted_ledger() {
        let nt = ntriples_text();
        let represents = |surface: DistributionSurface, cap: Capability| {
            nt.contains(&triple(
                &surface_iri(surface),
                &iri(GMEOW_NS, "representableParameter"),
                &capability_iri(cap),
            ))
        };
        for implication in authored_dg_basis() {
            for surface in DistributionSurface::ALL {
                let premise_holds = implication
                    .premise
                    .members()
                    .into_iter()
                    .all(|cap| represents(surface, cap));
                if !premise_holds {
                    continue;
                }
                for cap in implication.conclusion.members() {
                    assert!(
                        represents(surface, cap),
                        "law {:?} is violated by the emitted ledger of {}",
                        implication,
                        surface.slug()
                    );
                }
            }
        }
    }

    #[test]
    fn serialization_family_has_no_declared_loss() {
        let nt = ntriples_text();
        let serializations: Vec<&DistRow> = DISTRIBUTIONS
            .iter()
            .filter(|row| row.family == Family::Serialization)
            .collect();
        assert!(
            !serializations.is_empty(),
            "the serialization family must be non-empty, or this gate is vacuous"
        );
        for row in serializations {
            let dist = dist_iri(row.slug);
            let pred = iri(GMEOW_NS, "declaredLoss");
            let needle = format!("<{dist}> <{pred}>");
            assert!(
                !nt.lines().any(|l| l.starts_with(&needle)),
                "{} (serialization family) must not declare loss",
                row.slug
            );
        }
    }

    #[test]
    fn catalog_is_digest_free() {
        let nt = ntriples_text();
        assert!(
            !nt.contains("contentDigest"),
            "distribution catalog schema must stay digest-free (release-time instance, not schema)"
        );
    }

    #[test]
    fn every_distribution_has_a_family_and_consumer() {
        let nt = ntriples_text();
        for row in DISTRIBUTIONS {
            let dist = dist_iri(row.slug);
            assert!(
                nt.contains(&triple(
                    &dist,
                    &iri(GMEOW_NS, "distributionFamily"),
                    &family_iri(row.family)
                )),
                "{} missing distributionFamily {}",
                row.slug,
                row.family.slug()
            );
            assert!(
                nt.contains(&triple(
                    &dist,
                    &iri(GMEOW_NS, "eligibleForConsumer"),
                    &consumer_iri(row.consumer)
                )),
                "{} missing eligibleForConsumer {}",
                row.slug,
                row.consumer
            );
            assert!(
                nt.contains(&triple_lit(
                    &dist,
                    &iri(GMEOW_NS, "artifactMediaType"),
                    row.media_type
                )),
                "{} missing artifactMediaType {}",
                row.slug,
                row.media_type
            );
        }
    }

    /// The distribution-parameterized pricing set is exactly `owners × sub-assets`, and it
    /// stays disjoint from the distribution bijection.
    #[test]
    fn sub_asset_pricing_is_owner_parameterized_and_complete() {
        let owners = sub_asset_owner_slugs();
        let subs = declared_site_sub_asset_slugs();
        let priced = sub_asset_pricing();
        assert_eq!(
            priced.len(),
            owners.len() * subs.len(),
            "pricing must cover every (owner, sub-asset) pair: {priced:?}"
        );
        for owner in &owners {
            for sub in &subs {
                assert!(
                    priced.iter().any(|(o, s, _, _)| o == owner && s == sub),
                    "no pricing row for owner {owner:?} sub-asset {sub:?}"
                );
            }
        }
        // Every priced media type agrees with the emitted schema row, and every priced
        // owner really is a declared distribution.
        let nt = ntriples_text();
        for (owner, sub, prefix, media_type) in &priced {
            assert!(
                distribution_row(owner).is_some(),
                "pricing owner {owner:?} is not a declared distribution"
            );
            assert!(
                !prefix.is_empty(),
                "sub-asset {sub:?} has an empty tree prefix"
            );
            assert!(
                nt.contains(&triple_lit(
                    &sub_asset_iri(sub),
                    &iri(GMEOW_NS, "artifactMediaType"),
                    media_type
                )),
                "priced media type for {sub:?} disagrees with the emitted schema row"
            );
        }
    }
}
