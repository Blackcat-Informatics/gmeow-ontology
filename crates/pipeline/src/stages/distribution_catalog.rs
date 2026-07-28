// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The canonical **distribution catalog** — a meta-level named graph declaring WHICH
//! documentation distributions exist, their FAMILY, their CONSUMER class, and (for the
//! doc-render family) their declared capability LOSS.
//!
//! This is a SCHEMA, not a release record: it declares the eight distributions gmeow
//! ships and their static properties. It carries NO per-release digests — a shipped
//! release's concrete `gmeow:contentDigest` is a later release-time instance, never
//! folded into this carrier-time graph (a render-derived digest here would be a
//! non-converging fixpoint: the digest of THIS bundle would have to be known before
//! THIS bundle is serialized).
//!
//! ## The two families
//!
//! * **doc-render** — `site`, `mdbook`, `pdf`, `snippets`. These are EXACTLY the
//!   [`gmeow_docs::formats::DocFormat`] variants, and their declared loss is read from
//!   the SINGLE authority ([`gmeow_docs::formats::format_capabilities`]) — the same
//!   table [`crate::stages::docs_format_rendering`] reads — so the catalog's loss
//!   claims can never drift from the renderer's.
//! * **serialization** — `okf`, `jsonld`, `yamlld`, `pydantic`. These are structured
//!   re-serializations of the ontology, not lossy prose renderings; they carry no
//!   `gmeow:declaredLoss` here (their own loss lattice, if any, is a future
//!   declaration on their own graphs, never fabricated here).
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

/// A distribution family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Family {
    DocRender,
    Serialization,
}

impl Family {
    fn slug(&self) -> &'static str {
        match self {
            Family::DocRender => "doc-render",
            Family::Serialization => "serialization",
        }
    }
}

/// A serialization-family distribution (the doc-render family is exactly
/// [`DocFormat::ALL`]; this covers the remaining four).
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

/// A `site` sub-asset: one of the vendored interactive engines or the browser bundle
/// the site (and the packed mdbook) ships EXTERNALLY, content-addressed. These are
/// SUB-ASSETS of the `site` distribution — never new top-level distributions — so they
/// are NOT in [`declared_distribution_slugs`] and the eight-slug bijection is preserved.
/// Their schema rows (family / consumer / media-type) ride here DIGEST-FREE; the
/// per-release content digests live only in the `dist/` instance manifest
/// ([`crate::docs_distribution`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SiteSubAsset {
    PurrdfWasm,
    McpCoreWasm,
    McpWasm,
    CoreBundle,
    ConjectureDemo,
}

impl SiteSubAsset {
    const ALL: [SiteSubAsset; 5] = [
        SiteSubAsset::PurrdfWasm,
        SiteSubAsset::McpCoreWasm,
        SiteSubAsset::McpWasm,
        SiteSubAsset::CoreBundle,
        SiteSubAsset::ConjectureDemo,
    ];

    fn slug(&self) -> &'static str {
        match self {
            SiteSubAsset::PurrdfWasm => "purrdf-wasm",
            SiteSubAsset::McpCoreWasm => "mcp-core-wasm",
            SiteSubAsset::McpWasm => "mcp-wasm",
            SiteSubAsset::CoreBundle => "core-bundle",
            SiteSubAsset::ConjectureDemo => "conjectures",
        }
    }

    /// The wasm engines are `application/wasm`; the browser bundle is object-level
    /// N-Quads text; the conjecture demo library is Turtle.
    fn media_type(&self) -> &'static str {
        match self {
            SiteSubAsset::CoreBundle => "application/n-quads",
            SiteSubAsset::ConjectureDemo => "text/turtle",
            _ => "application/wasm",
        }
    }

    /// The site-tree path (or directory prefix) the sub-asset's bytes ship at, so the
    /// release-time digest producer content-addresses exactly what the catalog prices.
    fn site_path_prefix(&self) -> &'static str {
        match self {
            SiteSubAsset::PurrdfWasm => "assets/purrdf/",
            SiteSubAsset::McpCoreWasm => "assets/mcp-core/",
            SiteSubAsset::McpWasm => "assets/mcp/",
            SiteSubAsset::CoreBundle => "assets/gmeow-core.nq",
            SiteSubAsset::ConjectureDemo => "assets/conjectures.ttl",
        }
    }

    /// A human label for the schema row.
    fn label(&self) -> &'static str {
        match self {
            SiteSubAsset::PurrdfWasm => "vendored purrdf SPARQL/RDF wasm engine",
            SiteSubAsset::McpCoreWasm => "MCP core segment (the console's first-load engine)",
            SiteSubAsset::McpWasm => "MCP reasoning segment (demand-loaded)",
            SiteSubAsset::CoreBundle => "object-level browser bundle (N-Quads)",
            SiteSubAsset::ConjectureDemo => "curated conjecture playground demo library (Turtle)",
        }
    }
}

/// The doc-render family's per-format media type.
fn doc_render_media_type(fmt: DocFormat) -> &'static str {
    match fmt {
        DocFormat::Site => "text/html",
        DocFormat::Mdbook => "text/markdown",
        DocFormat::Pdf => "application/pdf",
        DocFormat::Snippets => "text/markdown",
    }
}

/// The doc-render family's per-format consumer.
fn doc_render_consumer(fmt: DocFormat) -> &'static str {
    match fmt {
        DocFormat::Site => "consumerPublicSite",
        DocFormat::Mdbook => "consumerOfflineBook",
        DocFormat::Pdf => "consumerPrintArchive",
        DocFormat::Snippets => "consumerAgentMemory",
    }
}

/// Build the distribution catalog graph: the eight distributions, their two families,
/// and (doc-render only) their declared capability loss, folded into
/// [`GRAPH_DISTRIBUTION_CATALOG`].
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

/// The declared `gmeow:artifactMediaType` for a distribution slug — the SAME
/// media type this module folds onto the carrier-time catalog subject, exposed as
/// the single cross-crate authority so a release-time producer (the external docs
/// fanout, `gmeow-dev sync`) never re-authors a second media-type table that could
/// drift from the schema. Returns `None` for an unrecognized slug (not one of the
/// eight declared distributions).
pub fn media_type_for_slug(slug: &str) -> Option<&'static str> {
    if let Some(fmt) = DocFormat::ALL.into_iter().find(|fmt| fmt.slug() == slug) {
        return Some(doc_render_media_type(fmt));
    }
    SerializationDist::ALL
        .into_iter()
        .find(|dist| dist.slug() == slug)
        .map(|dist| dist.media_type())
}

/// Every distribution slug the catalog declares — the doc-render family
/// ([`DocFormat::ALL`]) plus the serialization family ([`SerializationDist::ALL`]),
/// the two authorities [`emit_ntriples`] folds. Exposed so a bijection gate can compare
/// against the WHOLE declared set: an added or dropped distribution surfaces as a
/// set-size mismatch rather than passing a subset-presence check.
pub fn declared_distribution_slugs() -> std::collections::BTreeSet<&'static str> {
    DocFormat::ALL
        .into_iter()
        .map(|fmt| fmt.slug())
        .chain(SerializationDist::ALL.into_iter().map(|dist| dist.slug()))
        .collect()
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

/// The canonical catalog subject for a `site` sub-asset
/// (`…/distribution/dist/site/sub-asset/<slug>`). `pub(crate)` so the release-time
/// instance producer ([`crate::docs_distribution`]) hangs each sub-asset's
/// `gmeow:contentDigest` off the SAME subject, never a re-derived string.
///
/// This is the ONE identity helper that stayed on the writer's side of the
/// `gmeow-docs-catalog` split: it is defined over [`DocFormat::Site`], so hoisting it into
/// the wasm-clean reader leaf would drag `gmeow-docs` (and its embedded vendored wasm) in
/// with it. It is still a single definition site, built on the moved [`dist_iri`].
pub(crate) fn site_sub_asset_iri(slug: &str) -> String {
    format!("{}/sub-asset/{slug}", dist_iri(DocFormat::Site.slug()))
}

/// Every `site` sub-asset slug the catalog declares (the vendored interactive engines +
/// the browser bundle). Exposed so the release-time instance producer prices the SAME
/// set, and a contract gate can assert these are sub-assets of `site` — NOT members of
/// the eight-slug distribution bijection.
pub fn declared_site_sub_asset_slugs() -> std::collections::BTreeSet<&'static str> {
    SiteSubAsset::ALL.into_iter().map(|s| s.slug()).collect()
}

/// The `(slug, site-tree path prefix, media type)` pricing tuple for every `site`
/// sub-asset, in a fixed order. The release-time digest producer (`gmeow-dev sync`)
/// iterates this to content-address each sub-asset from the rendered site tree and hang
/// its `gmeow:contentDigest` off the SAME [`site_sub_asset_iri`] subject the carrier
/// catalog prices digest-free — the single authority for what a site sub-asset IS.
pub fn site_sub_asset_pricing() -> Vec<(&'static str, &'static str, &'static str)> {
    SiteSubAsset::ALL
        .into_iter()
        .map(|s| (s.slug(), s.site_path_prefix(), s.media_type()))
        .collect()
}

/// The DECLARED `gmeow:ProjectionCapability` individual a capability is. The kernel owns
/// this value vocabulary, exactly as it owns the `gmeow:ProjectionContext` consumers this
/// module already references by name through [`consumer_iri`] — so a loss node's accounted
/// parameter and a concept's intent member are the SAME six declared individuals, never a
/// second, catalog-local capability namespace.
fn capability_iri(cap: Capability) -> String {
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
/// the formal context. For a rendered format that IS its distribution subject; the console
/// is a surface without being a shipped distribution, so it gets its own subject outside the
/// eight-slug bijection.
fn surface_iri(surface: DistributionSurface) -> String {
    match surface {
        DistributionSurface::Format(fmt) => dist_iri(fmt.slug()),
        DistributionSurface::Console => {
            format!("{DISTRIBUTION_BASE}surface/{}", surface.slug())
        }
    }
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

/// Emit the interactive console as a capability-bearing projection SURFACE.
///
/// It is typed `gmeow:LossBearingProfile` directly rather than
/// `gmeow:DocumentationDistribution`: the console is not one of the eight artifacts the
/// release ships, so typing it as a distribution would silently widen
/// `read_distribution_matrix` to nine rows. It still needs a loss ledger and an audience,
/// because it is an object of the formal context the concept lattice is derived over.
fn emit_console_surface(lines: &mut Vec<String>) {
    let console = surface_iri(DistributionSurface::Console);
    skeleton(
        lines,
        &console,
        &iri(GMEOW_NS, "LossBearingProfile"),
        "interactive console projection surface",
    );
    lines.push(triple(
        &console,
        &iri(GMEOW_NS, "eligibleForConsumer"),
        &consumer_iri("consumerInteractiveConsole"),
    ));
    emit_capability_ledger(lines, &console, DistributionSurface::Console);
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

    // ── family nodes ──
    for family in [Family::DocRender, Family::Serialization] {
        let fam = family_iri(family);
        skeleton(
            &mut lines,
            &fam,
            &iri(GMEOW_NS, "DistributionFamily"),
            &format!("documentation distribution family {}", family.slug()),
        );
    }

    // ── doc-render distributions: EXACTLY the DocFormat variants ──
    for fmt in DocFormat::ALL {
        let slug = fmt.slug();
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
            &family_iri(Family::DocRender),
        ));
        lines.push(triple_lit(
            &dist,
            &iri(GMEOW_NS, "artifactMediaType"),
            doc_render_media_type(fmt),
        ));
        lines.push(triple(
            &dist,
            &iri(GMEOW_NS, "eligibleForConsumer"),
            &consumer_iri(doc_render_consumer(fmt)),
        ));

        // The capability ledger, SOURCED FROM the single authority — never re-authored
        // here. `gmeow:DocumentationDistribution` is a `gmeow:LossBearingProfile`, which is
        // what makes it a legal subject of `gmeow:declaredLoss`.
        emit_capability_ledger(&mut lines, &dist, DistributionSurface::Format(fmt));
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

    // ── site sub-assets: the vendored interactive engines + the browser bundle ──
    // First-class schema rows, DIGEST-FREE (the per-release content digests ride only in
    // the `dist/` instance manifest). Hung off the `site` distribution via
    // gmeow:hasSubAsset, so they are sub-assets — NOT top-level distributions — and the
    // eight-slug bijection is untouched.
    let site_dist = dist_iri(DocFormat::Site.slug());
    for sub in SiteSubAsset::ALL {
        let slug = sub.slug();
        let node = site_sub_asset_iri(slug);
        skeleton(
            &mut lines,
            &node,
            &iri(GMEOW_NS, "SiteSubAsset"),
            sub.label(),
        );
        lines.push(triple(&site_dist, &iri(GMEOW_NS, "hasSubAsset"), &node));
        lines.push(triple_lit(
            &node,
            &iri(GMEOW_NS, "distributionFormat"),
            slug,
        ));
        lines.push(triple(
            &node,
            &iri(GMEOW_NS, "distributionFamily"),
            &family_iri(Family::DocRender),
        ));
        lines.push(triple_lit(
            &node,
            &iri(GMEOW_NS, "artifactMediaType"),
            sub.media_type(),
        ));
        lines.push(triple(
            &node,
            &iri(GMEOW_NS, "eligibleForConsumer"),
            &consumer_iri(doc_render_consumer(DocFormat::Site)),
        ));
    }

    // ── the interactive console: a capability-bearing surface, NOT a distribution ──
    emit_console_surface(&mut lines);

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
        let expected = [
            "site", "mdbook", "pdf", "snippets", "okf", "jsonld", "yamlld", "pydantic",
        ];
        for slug in expected {
            let needle = format!("<{}> \"{slug}\" .", pred);
            assert!(
                nt.lines().any(|l| l.contains(&needle)),
                "missing gmeow:distributionFormat for slug {slug:?}"
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
            subjects.len() >= 8 + 2,
            "expected at least 8 distributions + 2 families, got {}",
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
    fn site_sub_assets_are_priced_digest_free_and_outside_the_bijection() {
        let nt = ntriples_text();
        let site_dist = dist_iri(DocFormat::Site.slug());
        let bijection = declared_distribution_slugs();
        assert_eq!(
            bijection.len(),
            8,
            "the eight-slug bijection must be untouched"
        );

        for slug in declared_site_sub_asset_slugs() {
            // NOT a top-level distribution — the bijection is preserved.
            assert!(
                !bijection.contains(slug),
                "site sub-asset {slug:?} must NOT be a top-level distribution slug"
            );
            let node = site_sub_asset_iri(slug);
            // Typed as a SiteSubAsset and hung off the site distribution.
            assert!(
                nt.contains(&triple(&node, RDF_TYPE, &iri(GMEOW_NS, "SiteSubAsset"))),
                "sub-asset {slug:?} missing rdf:type gmeow:SiteSubAsset"
            );
            assert!(
                nt.contains(&triple(&site_dist, &iri(GMEOW_NS, "hasSubAsset"), &node)),
                "site distribution must declare gmeow:hasSubAsset {slug:?}"
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

    /// The console rides as a capability-bearing SURFACE: it has an audience and a loss
    /// ledger, and it is deliberately not a distribution, so the eight-slug bijection and
    /// the consumer-facing matrix are untouched by its arrival.
    #[test]
    fn the_console_is_a_surface_with_a_ledger_and_not_a_distribution() {
        let nt = ntriples_text();
        let console = surface_iri(DistributionSurface::Console);
        assert!(
            nt.contains(&triple(
                &console,
                RDF_TYPE,
                &iri(GMEOW_NS, "LossBearingProfile")
            )),
            "the console surface must be typed gmeow:LossBearingProfile"
        );
        assert!(
            !nt.contains(&triple(
                &console,
                RDF_TYPE,
                &iri(GMEOW_NS, "DocumentationDistribution")
            )),
            "the console must NOT be typed as a shipped distribution"
        );
        assert!(
            nt.contains(&triple(
                &console,
                &iri(GMEOW_NS, "eligibleForConsumer"),
                &consumer_iri("consumerInteractiveConsole")
            )),
            "the console surface must name its declared audience"
        );
        assert!(
            !nt.lines().any(|line| line.starts_with(&format!(
                "<{console}> <{}>",
                iri(GMEOW_NS, "distributionFormat")
            ))),
            "the console carries no distribution format slug — it is outside the bijection"
        );
        assert_eq!(
            declared_distribution_slugs().len(),
            8,
            "the eight-slug bijection must survive the console's arrival"
        );

        // Its ledger is the authored partition, both halves.
        let caps = surface_capabilities(DistributionSurface::Console);
        for cap in &caps.dropped {
            let loss_node = loss_iri("console", cap.slug());
            assert!(nt.contains(&triple(
                &console,
                &iri(GMEOW_NS, "declaredLoss"),
                &loss_node
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
        for dist_enum in SerializationDist::ALL {
            let dist = dist_iri(dist_enum.slug());
            let pred = iri(GMEOW_NS, "declaredLoss");
            let needle = format!("<{dist}> <{pred}>");
            assert!(
                !nt.lines().any(|l| l.starts_with(&needle)),
                "{} (serialization family) must not declare loss",
                dist_enum.slug()
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
        let all_slugs = [
            ("site", Family::DocRender),
            ("mdbook", Family::DocRender),
            ("pdf", Family::DocRender),
            ("snippets", Family::DocRender),
            ("okf", Family::Serialization),
            ("jsonld", Family::Serialization),
            ("yamlld", Family::Serialization),
            ("pydantic", Family::Serialization),
        ];
        for (slug, family) in all_slugs {
            let dist = dist_iri(slug);
            assert!(
                nt.contains(&triple(
                    &dist,
                    &iri(GMEOW_NS, "distributionFamily"),
                    &family_iri(family)
                )),
                "{slug} missing distributionFamily {}",
                family.slug()
            );
            let pred = iri(GMEOW_NS, "eligibleForConsumer");
            let needle = format!("<{dist}> <{pred}>");
            assert!(
                nt.lines().any(|l| l.starts_with(&needle)),
                "{slug} missing eligibleForConsumer"
            );
        }
    }
}
