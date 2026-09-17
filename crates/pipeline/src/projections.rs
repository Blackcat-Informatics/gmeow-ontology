// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native CLI-glue drivers for the projection / up-projection / transpile surfaces.
//!
//! These are the Rust ports of the former Python `gmeow_tools.projections`,
//! `gmeow_tools.up_projection`, and `gmeow_tools.transpile` orchestration modules.
//! They are the *drivers* — the per-profile CONSTRUCT execution, the lawful
//! up-projection wiring, and the `MAXIMAL(G) = G + E(G) + P(G)` assembly plus its
//! gap report. The heavy lifting stays in the native building blocks they call:
//!
//!   * [`crate::transform`] — the MAXIMAL(G) transform kernel (skolemize / saturate
//!     / projection CONSTRUCT / GTS emission).
//!   * [`crate::put_executor::execute_put_legs`] — the lawful up-projection executor.
//!   * [`purrdf`] — the native SPARQL engine, the flat RDF dataset codec, and the
//!     GTS reader (`flattened_dataset_from_bytes`).
//!
//! Every function is PyO3-free and consumer-safe: the repo/bundle-derived inputs
//! (SSSOM texts, projection TTLs, ontology, cells, denied rows, compiled CONSTRUCT
//! queries) are passed in by the caller (a `gmeow` / `gmeow-dev` binary), so this
//! module never reads the repo or the bundle itself. The projections are lossy,
//! directional, consumable views — never the canonical model.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_errors::{Diag, ResultExt};
use purrdf::sparql::NativeSparqlEngine;
use purrdf::{RdfQuad, RdfTerm, SerializeGraph, SparqlEngine, SparqlRequest, SparqlResult};

use crate::error::Projection;
use crate::transform::{CellInput, TransformReportNative};
use crate::up_projection_corpus::{PREFIXES, canon_qname};

const GM: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
const NT_MEDIA_TYPE: &str = "application/n-triples";

// The TBox predicates whose axioms drive property-domain typing: a
// property assertion is typed by `rdfs:domain` (its subject) / `rdfs:range` (its
// object) under prp-dom, and that derived type is then propagated UPWARD through
// `rdfs:subClassOf`; `rdfs:subPropertyOf` lets a sub-property inherit its
// super-property's domain/range. This is the exact closure the reasoned harvest
// needs — nothing else in the bundle TBox can contribute a sound `rdf:type`.
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDFS_SUBPROPERTYOF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const DOMAIN_TYPING_TBOX_PREDICATES: &[&str] =
    &[RDFS_DOMAIN, RDFS_RANGE, RDFS_SUBCLASSOF, RDFS_SUBPROPERTYOF];

/// A projection-boundary language-tag remap (internal `x-gmeow-*` → public BCP-47,
/// or the inverse), keyed by the *source* tag. Empty = no retag.
pub type TagMap = BTreeMap<String, String>;

/// A target projection profile: its registry name and the output prefixes a Turtle
/// serialization would bind. The CONSTRUCT query itself is supplied by the caller
/// (from `generated/queries/<name>.rq` or the bundle), mirroring the Python
/// `_load_projection_query`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    pub prefixes: Vec<String>,
}

impl Profile {
    fn new(name: &str, prefixes: &[&str]) -> Self {
        Self {
            name: name.to_owned(),
            prefixes: prefixes.iter().map(|p| (*p).to_owned()).collect(),
        }
    }
}

/// Single-vocab GTS view selectors that are not projection profiles: the whole
/// maximal product (`all` / `maximal`) and the pure GMEOW base (`gmeow`).
pub const GTS_VIEW_ALL: &[&str] = &["all", "maximal"];
pub const GTS_VIEW_GMEOW: &str = "gmeow";

/// The registry of target profiles, keyed by name. Each maps to a
/// `generated/queries/<name>.rq` CONSTRUCT the caller loads. This mirrors the
/// Python `PROFILES` dict exactly (same names, same output prefixes).
pub fn profiles() -> BTreeMap<String, Profile> {
    let rows: &[(&str, &[&str])] = &[
        ("schema-org", &["schema", "rdfs"]),
        ("geosparql", &["geo"]),
        ("vcard", &["vcard"]),
        ("foaf", &["foaf", "wgs84"]),
        ("ical", &["ical"]),
        ("owl-time", &["time"]),
        ("odrl", &["odrl"]),
        ("cc", &["cc"]),
        ("dcterms", &["dcterms"]),
        ("oai_dc", &["dc"]),
        ("spdx", &["spdx"]),
        ("ontolex", &["ontolex", "lime", "rdf"]),
        ("web-annotation", &["oa"]),
        ("skos", &["skos"]),
        ("bot", &["bot"]),
        ("mailmap", &["gmeow"]),
        ("exif", &["exif"]),
        ("iiif", &["iiif", "oa", "rdf"]),
        ("dcat", &["dcat", "dcterms", "prov", "spdx"]),
        ("org", &["org"]),
        ("bibo", &["bibo"]),
        ("bibframe", &["bibframe", "rdf"]),
        ("gedcom", &["gedcom"]),
        ("sioc", &["sioc"]),
        ("doap", &["doap"]),
        ("codemeta", &["codemeta"]),
        ("prov", &["prov"]),
    ];
    rows.iter()
        .map(|(name, prefixes)| (name.to_string(), Profile::new(name, prefixes)))
        .collect()
}

/// The IRI namespace of a registered prefix, if known.
fn namespace_of(prefix: &str) -> Option<&'static str> {
    PREFIXES
        .iter()
        .find(|(p, _)| *p == prefix)
        .map(|(_, ns)| *ns)
}

/// Parse an N-Triples document into its flat quad stream (RDF 1.2 `rdf:reifies` /
/// quoted-triple rows stay plain quads, matching the transform's flat store view).
fn flat_quads_from_nt(nt: &str) -> gmeow_errors::Result<Vec<RdfQuad>> {
    if nt.trim().is_empty() {
        return Ok(Vec::new());
    }
    let parsed = purrdf::parse_dataset(nt.as_bytes(), NT_MEDIA_TYPE, None)
        .with_ctx(|| "N-Triples parse failed")?;
    Ok(purrdf::flat_rdf_quads_from_dataset(parsed.as_ref()))
}

/// Serialize a flat default-graph quad stream to canonical N-Triples.
fn quads_to_nt(quads: &[RdfQuad]) -> gmeow_errors::Result<String> {
    let flat = purrdf::flat_dataset_from_quads(quads).map_err(|e| {
        Diag::of_kind(Projection {
            message: format!("N-Triples flatten failed: {e}"),
        })
    })?;
    let bytes =
        purrdf::serialize_dataset(flat.as_ref(), NT_MEDIA_TYPE, SerializeGraph::DefaultGraph)
            .with_ctx(|| "N-Triples serialization failed")?;
    String::from_utf8(bytes).with_ctx(|| "N-Triples output is not UTF-8")
}

/// Rewrite language-tagged literals, including those nested in quoted triples, whose tag is a key of
/// `tag_map`, in place over the owned quad stream. The projection-boundary retag: an
/// empty map is a no-op. Idempotent for already-remapped literals.
///
/// `pub(crate)` so [`crate::transform::transform_nt`] can apply the identical retag
/// to the MAXIMAL(G) base+derived quad stream before GTS emission — the same
/// projection-boundary law this module already enforces for `project`/`export`,
/// reused rather than re-derived (Principle 4: one canonical source).
pub(crate) fn retag_quads(quads: &mut [RdfQuad], tag_map: &TagMap) {
    if tag_map.is_empty() {
        return;
    }
    for quad in quads.iter_mut() {
        retag_term(&mut quad.subject, tag_map);
        retag_term(&mut quad.object, tag_map);
        if let Some(graph) = &mut quad.graph_name {
            retag_term(graph, tag_map);
        }
    }
}

fn retag_term(term: &mut RdfTerm, tag_map: &TagMap) {
    match term {
        RdfTerm::Literal(literal) => {
            if let Some(language) = &literal.language
                && let Some(mapped) = tag_map.get(language)
            {
                literal.language = Some(mapped.clone());
            }
        }
        RdfTerm::Triple(triple) => {
            retag_term(&mut triple.subject, tag_map);
            retag_term(&mut triple.object, tag_map);
        }
        RdfTerm::Iri(_) | RdfTerm::BlankNode(_) => {}
    }
}

/// Run a profile's CONSTRUCT over a source, returning the pure-profile projection as
/// N-Triples.
///
/// The Rust port of `projections.project_graph`. The CONSTRUCT is evaluated by the
/// native [`NativeSparqlEngine`] over the source (ontology + instance data). The
/// projection-boundary retag maps every internal `x-gmeow-*` literal tag to its
/// public BCP-47 form via `tag_map` (empty = none), so consumer parsers read the
/// projected text as the real language.
///
/// * `source_nt` — the data to project, as N-Triples (ontology + instance data).
/// * `query` — the compiled CONSTRUCT for the profile (the caller loads it).
/// * `tag_map` — internal→public language-tag remap for emitted literals.
pub fn project_graph(
    source_nt: &str,
    query: &str,
    tag_map: &TagMap,
) -> gmeow_errors::Result<String> {
    let source_quads = flat_quads_from_nt(source_nt)?;
    let ds = purrdf::flat_dataset_from_quads(&source_quads).map_err(|e| {
        Diag::of_kind(Projection {
            message: format!("source dataset build failed: {e}"),
        })
    })?;
    let engine = NativeSparqlEngine::new();
    let result = engine
        .query(
            &ds,
            SparqlRequest {
                query,
                base_iri: None,
                substitutions: &[],
            },
        )
        .with_ctx(|| "projection query evaluation failed")?;
    let SparqlResult::Graph(triples) = result else {
        return Err(Diag::of_kind(Projection {
            message: "projection query did not return a graph".to_owned(),
        }));
    };
    let mut out: Vec<RdfQuad> = triples.owned_quads().collect();
    retag_quads(&mut out, tag_map);
    quads_to_nt(&out)
}

/// Extract the asserted base triples from a transpiled `.gts`, as a flat quad stream.
///
/// The Rust port of `projections.gts_base_graph`. A `.gts` is the canonical RDF-1.2
/// product — base/derived triples *plus* their provenance reifiers. This returns just
/// the plain asserted triples: the `rdf:reifies` reifier rows and any quad with a
/// quoted-triple endpoint (in subject OR object) are dropped, exactly as the Python
/// routed through purrdf. Reads through the native GTS loader
/// ([`purrdf::gts::flattened_dataset_from_bytes`]) + the flat unfold, so no codec text
/// sits in the middle.
pub fn gts_base_graph(gts_bytes: &[u8]) -> gmeow_errors::Result<Vec<RdfQuad>> {
    let dataset =
        purrdf::gts::flattened_dataset_from_bytes(gts_bytes).with_ctx(|| "gts read failed")?;
    let flat = purrdf::flat_rdf_quads_from_dataset(dataset.as_ref());
    let mut base = Vec::with_capacity(flat.len());
    for quad in flat {
        if quad.predicate == RDF_REIFIES
            || matches!(quad.subject, RdfTerm::Triple(_))
            || matches!(quad.object, RdfTerm::Triple(_))
        {
            continue;
        }
        base.push(RdfQuad::new(quad.subject, quad.predicate, quad.object));
    }
    Ok(base)
}

/// The A→B authorization set folded into a `.gts`: the `gmeow:ProjectionMapping` cell IRIs whose
/// EXECUTED lens-law discharge carried an `ObligationDischarged` `logic:SectionLaw`, read from the
/// bundle's `graph/correspondence-laws` named graph (the mappings stage's
/// `stages::mappings::discharge_correspondence_laws` output).
///
/// This is the production consumer of Deliverable A: the bundle carries the executed discharge
/// verdicts, and the up-projection executor consumes THIS set to promote each mnemomorphic `=` cell
/// to a lawful FACT rename. Returns the empty set only if the bundle carries no correspondence-laws
/// graph (a bundle with no discharged section laws) — never a silent partial read.
pub fn discharged_section_cells_from_bundle(
    gts_bytes: &[u8],
) -> gmeow_errors::Result<BTreeSet<String>> {
    // The correspondence-laws NAMED graph survives only through the structural GTS reader
    // (`read_graph`); the flattened-dataset fold collapses to the object-level default graph and
    // would silently drop it. Read the named graph's triples by term value and extract.
    let graph = purrdf::gts::read_graph(gts_bytes, true).map_err(|e| {
        Diag::of_kind(Projection {
            message: format!("gts read_graph failed: {e}"),
        })
    })?;
    let corr_graph = crate::stages::carrier::GRAPH_CORRESPONDENCE_LAWS;
    let term = |id: usize| -> String {
        graph
            .terms
            .get(id)
            .and_then(|t| t.value.clone())
            .unwrap_or_default()
    };
    let triples: Vec<(String, String, String)> = graph
        .quads
        .iter()
        .filter_map(|&(s, p, o, gname)| {
            let gid = gname?;
            (term(gid) == corr_graph).then(|| (term(s), term(p), term(o)))
        })
        .collect();
    Ok(crate::up_projection_gates::discharged_section_cells_from_triples(&triples))
}

/// The IRI namespaces a single-vocab view keeps (empty = keep everything).
///
/// The Rust port of `projections._view_namespaces`. `all` / `maximal` keep the whole
/// maximal product; `gmeow` keeps only the pure GMEOW base; any other name is a
/// projection profile, whose registered prefixes resolve to their namespaces.
pub fn view_namespaces(view: &str) -> gmeow_errors::Result<BTreeSet<String>> {
    if GTS_VIEW_ALL.contains(&view) {
        return Ok(BTreeSet::new());
    }
    if view == GTS_VIEW_GMEOW {
        return Ok(BTreeSet::from([GM.to_owned()]));
    }
    let profiles = profiles();
    let profile = profiles.get(view).ok_or_else(|| {
        Diag::of_kind(Projection {
            message: format!("unknown gts view / projection profile: {view}"),
        })
    })?;
    Ok(profile
        .prefixes
        .iter()
        .filter_map(|p| namespace_of(p).map(str::to_owned))
        .collect())
}

/// Emit the single-vocabulary view of a transpiled `.gts` — a *filter*, not a
/// re-projection — as N-Triples.
///
/// The Rust port of `projections.project_gts_subset`. The `.gts` is already maximal
/// (GMEOW + every vocab), so a vocab view is the subset of its base triples in that
/// vocab's namespaces. A triple is kept when its predicate is in the view's
/// namespaces, or when it types a subject into a class of those namespaces
/// (`rdf:type` to a kept class). `all` / `maximal` keeps everything.
pub fn project_gts_subset(
    gts_bytes: &[u8],
    view: &str,
    tag_map: &TagMap,
) -> gmeow_errors::Result<String> {
    let base = gts_base_graph(gts_bytes)?;
    let namespaces = view_namespaces(view)?;
    let mut out: Vec<RdfQuad> = if namespaces.is_empty() {
        base
    } else {
        base.into_iter()
            .filter(|q| keep_in_view(q, &namespaces))
            .collect()
    };
    retag_quads(&mut out, tag_map);
    quads_to_nt(&out)
}

/// Whether a base quad belongs to a namespace-scoped view.
fn keep_in_view(quad: &RdfQuad, namespaces: &BTreeSet<String>) -> bool {
    if namespaces.iter().any(|ns| quad.predicate.starts_with(ns)) {
        return true;
    }
    if quad.predicate == RDF_TYPE
        && let RdfTerm::Iri(object) = &quad.object
    {
        return namespaces.iter().any(|ns| object.starts_with(ns));
    }
    false
}

// ── Up-projection: consumer RDF → pure GMEOW ─────────────────────────────────────

/// The repo/bundle-derived inputs the lawful up-projection needs — the same inputs
/// the public CLI already resolves. Passed in so this driver stays consumer-safe.
#[derive(Debug, Clone, Default)]
pub struct UpProjectionInputs {
    /// The SSSOM lift maps (`generated/mappings/*.sssom.tsv` text).
    pub sssom_texts: Vec<String>,
    /// The projection/EDOAL TTL sources (the authored `gmeow:ProjectionMapping` cells).
    pub projection_ttls: Vec<String>,
    /// The asserted ontology, as N-Triples.
    pub ontology_nt: String,
    /// The A→B authorization channel: the set of `gmeow:ProjectionMapping` cell IRIs whose
    /// EXECUTED lens-law discharge (folded into `graph/correspondence-laws`) carried an
    /// `ObligationDischarged` `logic:SectionLaw`. Every mnemomorphic `=` cell so authorized lifts
    /// as a lawful FACT rename (not a lossy close-match claim); a mnemomorphic `=` cell absent from
    /// this set is a HARD FAIL in [`crate::up_projection_gates::gate_verified_lift_program`].
    pub discharged_section_cells: std::collections::BTreeSet<String>,
}

/// The result of an up-projection: the lifted GMEOW graph plus native accounting.
///
/// The Rust port of `up_projection.UpProjection`. `lifted`/`claimed` count lawful
/// facts and reified claim cells; `gap_terms` maps each un-liftable projection-namespace
/// source term to its true occurrence count; `residue` is the honest loss-ledger of
/// dropped heuristic categories.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpProjection {
    /// The lifted pure-GMEOW graph, as N-Triples.
    pub graph_nt: String,
    pub lifted: usize,
    pub claimed: usize,
    pub gap_terms: BTreeMap<String, usize>,
    pub residue: Vec<String>,
}

/// Lift a consumer graph up to GMEOW through the lawful native put executor.
///
/// The Rust port of `up_projection.up_project`. Runs every lawful put leg (rename,
/// inverse, and lossy reified claim) as a native SPARQL `CONSTRUCT` via
/// [`crate::put_executor::execute_put_legs`]; the kernel derives the authoritative
/// lawful rule set from the supplied inputs on every call. `internal_tag_map` remaps
/// the lifted literals' public tags back to the internal `x-gmeow-*` form (empty =
/// none), mirroring the Python `retag_graph_to_internal`.
pub fn up_project(
    source_nt: &str,
    inputs: &UpProjectionInputs,
    internal_tag_map: &TagMap,
) -> gmeow_errors::Result<UpProjection> {
    let report = crate::put_executor::execute_put_legs(
        source_nt,
        &inputs.sssom_texts,
        &inputs.projection_ttls,
        &inputs.ontology_nt,
        &inputs.discharged_section_cells,
    )?;

    // Reasoned superclass recovery. The lawful put legs return only the
    // renamed/inverse facts; an entailed superclass (e.g. a subject bearing
    // `gmeow:partOfThread`/`gmeow:inReplyTo` — both `rdfs:domain gmeow:Message` — IS a
    // `gmeow:Message`) is lost because the put path never reasons. Run a scoped,
    // deterministic prp-dom harvest over `[lifted ∪ the property-domain TBox fragment]`
    // and union the sound derived `rdf:type` triples back into the lifted graph. This is
    // sound-only and upward-closing: it never fabricates a SubKind (`gmeow:EmailMessage`)
    // or a sibling (`gmeow:FeedPosting`), because `dl:type-propagation` only walks
    // subClassOf UPWARD.
    let mut quads = flat_quads_from_nt(&report.graph_nt)?;
    let harvested = harvest_reasoned_types(&quads, &inputs.ontology_nt)?;
    let mut merged_new = false;
    if !harvested.is_empty() {
        let existing: std::collections::HashSet<RdfQuad> = quads.iter().cloned().collect();
        for typed in harvested {
            if existing.contains(&typed) {
                continue;
            }
            quads.push(typed);
            merged_new = true;
        }
    }

    // Byte-stability: only re-serialize when we actually changed the graph (harvest or
    // retag). `quads_to_nt` freeze-sorts by (s,p,o), so the union is canonical; when
    // nothing changed we return the put executor's already-sorted bytes untouched.
    let graph_nt = if internal_tag_map.is_empty() {
        if merged_new {
            quads_to_nt(&quads)?
        } else {
            report.graph_nt
        }
    } else {
        retag_quads(&mut quads, internal_tag_map);
        quads_to_nt(&quads)?
    };
    Ok(UpProjection {
        graph_nt,
        lifted: report.lifted,
        claimed: report.claimed,
        gap_terms: report.gap_terms,
        residue: report.residue,
    })
}

/// Harvest the sound derived `rdf:type` triples entailed by the lifted assertions
/// against the bundle's property-domain TBox fragment — the reasoned superclass
/// recovery for the inverse-ingest (put) path.
///
/// The world-scoping is LOAD-BEARING: the native reasoner is world/graph-indexed with
/// no cross-world union, so `dl:domain` (prp-dom) only fires when the property
/// assertion and its `rdfs:domain` axiom share the SAME world. Both the lifted
/// assertions and the extracted TBox fragment are default-graph N-Triples, so
/// [`purrdf::native_quads::flat_dataset_from_quads`] lands them in the same (default)
/// world and [`reason_all_with_data`](gmeow_logic::reason::reason_all_with_data)'s
/// merge keeps them co-located — prp-dom fires, and the derived type propagates upward
/// through the fragment's `rdfs:subClassOf` axioms.
///
/// Performance: the TBox is scoped to [`DOMAIN_TYPING_TBOX_PREDICATES`] (the closure
/// property-domain typing needs), never the whole bundle, so ingest never pays a
/// full-bundle chase.
///
/// Determinism: every sound derived `rdf:type` is harvested (maximal information flow)
/// but keyed into a [`BTreeMap`] on `(subject, class)` so the returned vector is
/// sorted/canonical with no wall-clock or randomness.
fn harvest_reasoned_types(
    lifted: &[RdfQuad],
    ontology_nt: &str,
) -> gmeow_errors::Result<Vec<RdfQuad>> {
    if lifted.is_empty() {
        return Ok(Vec::new());
    }
    let fragment = domain_typing_tbox_fragment(ontology_nt)?;
    if fragment.is_empty() {
        // No property-domain axioms in scope ⇒ nothing prp-dom could entail. Skip the
        // chase entirely (also keeps the empty-ontology callers byte-identical).
        return Ok(Vec::new());
    }

    let user = purrdf::flat_dataset_from_quads(lifted).map_err(|e| {
        Diag::of_kind(Projection {
            message: format!("reasoned harvest: lifted dataset build failed: {e}"),
        })
    })?;
    let bundle = purrdf::flat_dataset_from_quads(&fragment).map_err(|e| {
        Diag::of_kind(Projection {
            message: format!("reasoned harvest: TBox fragment dataset build failed: {e}"),
        })
    })?;
    // The scoped typing TBox and lifted ABox deliberately share one default theory.
    use gmeow_logic::reason::{DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld};
    let domains = SelectedDomains::new([SelectedLogicalWorld::new(
        LogicalGraph::Default,
        DomainProfile::NonemptyObjectDomainV1,
        "gmeow.pipeline.reasoned-type-harvest.v1".to_owned(),
        *blake3::hash(b"gmeow.pipeline.reasoned-type-harvest.v1/default/nonempty-object-domain-v1")
            .as_bytes(),
    )?])?;
    let result =
        gmeow_logic::reason::reason_all_with_data(bundle.as_ref(), user.as_ref(), &domains)
            .map_err(|e| {
                Diag::of_kind(Projection {
                    message: format!("reasoned harvest: native reasoning failed: {e}"),
                })
            })?;

    // Dedup on the canonical (subject, class) key, then emit sorted by that key so the
    // harvested block is byte-stable regardless of the reasoner's row order. `RdfQuad`
    // is `Hash + Eq` but not `Ord`, so the string key carries the total order.
    let mut out: BTreeMap<(String, String), RdfQuad> = BTreeMap::new();
    for axiom in result.inferred() {
        // Only rule-DERIVED types (`is_edb == false`) are new information; asserted
        // rows are already in `lifted`. Restrict to `rdf:type` with an IRI class object.
        if axiom.is_edb || axiom.predicate != RDF_TYPE {
            continue;
        }
        let Some(class_iri) = axiom.object.as_iri() else {
            continue;
        };
        out.entry((axiom.subject.clone(), class_iri.to_owned()))
            .or_insert_with(|| {
                RdfQuad::new(
                    RdfTerm::iri(axiom.subject.clone()),
                    RDF_TYPE.to_owned(),
                    RdfTerm::iri(class_iri.to_owned()),
                )
            });
    }
    Ok(out.into_values().collect())
}

/// Extract the property-domain typing TBox fragment from the bundle ontology
/// N-Triples: every quad whose predicate is one of [`DOMAIN_TYPING_TBOX_PREDICATES`].
/// This is the bounded, deterministic closure the reasoned harvest reasons over
/// (never the whole bundle).
fn domain_typing_tbox_fragment(ontology_nt: &str) -> gmeow_errors::Result<Vec<RdfQuad>> {
    let quads = flat_quads_from_nt(ontology_nt)?;
    Ok(quads
        .into_iter()
        .filter(|q| DOMAIN_TYPING_TBOX_PREDICATES.contains(&q.predicate.as_str()))
        .collect())
}

// ── Transpile: consumer RDF → pure GMEOW → MAXIMAL multi-vocab ────────────────────

/// The repo/bundle-derived inputs the MAXIMAL(G) back-half needs, passed in so the
/// transpile driver stays consumer-safe. These flow straight into
/// [`crate::transform::transform_nt`].
#[derive(Debug, Clone, Default)]
pub struct MaximalInputs {
    /// The asserted ontology, as N-Triples.
    pub ontology_nt: String,
    /// The strong-equivalence cells (the alignment corpus).
    pub cells: Vec<CellInput>,
    /// The saturation refusal set (the alignment-lint ERROR rows), as CURIE triples.
    pub denied: Vec<(String, String, String)>,
    /// The compiled projection CONSTRUCT queries, `(profile_name, query_text)`.
    pub projection_queries: Vec<(String, String)>,
}

/// The result of a full transpile: the up-projection account, the pure-GMEOW draft,
/// the gap report, and the MAXIMAL(G) transform report.
///
/// The Rust port of `transpile.TranspileReport`. Where the Python wrote files, this
/// returns the bytes; the calling binary owns the filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranspileReport {
    /// Source triples lifted as bare facts.
    pub lifted: usize,
    /// Source triples lifted as provenance-stamped claims.
    pub claimed: usize,
    /// Distinct source terms with no lift rule.
    pub gap_terms: usize,
    /// The pure-GMEOW intermediate draft, as N-Triples.
    pub draft_nt: String,
    /// The Markdown gap report (every un-lifted source triple).
    pub gap_report_md: String,
    /// The MAXIMAL(G) report.
    pub transform: TransformReportNative,
}

/// Transpile an in-memory consumer-vocabulary graph to MAXIMAL GMEOW.
///
/// The Rust port of `transpile.transpile_graph`, chaining the two halves end to end:
///
/// 1. **Up-projection** — lift the non-GMEOW `source_nt` up into pure GMEOW via
///    [`up_project`].
/// 2. **Maximal down-projection** — run `MAXIMAL(G) = G + E(G) + P(G)` over that
///    pure-GMEOW draft via [`crate::transform::transform_nt`].
///
/// # Errors
///
/// - `stem` is empty/blank.
/// - Nothing lifts to GMEOW (an empty draft has nothing to project — surfaced, never
///   a silent empty publication).
pub fn transpile_graph(
    source_nt: &str,
    stem: &str,
    up_inputs: &UpProjectionInputs,
    maximal_inputs: &MaximalInputs,
    internal_tag_map: &TagMap,
) -> gmeow_errors::Result<TranspileReport> {
    if stem.trim().is_empty() {
        return Err(Diag::of_kind(Projection {
            message: "transpile_graph: stem must be a non-empty string".to_owned(),
        }));
    }

    let lift = up_project(source_nt, up_inputs, internal_tag_map)?;
    if lift.graph_nt.trim().is_empty() {
        return Err(Diag::of_kind(Projection {
            message: format!("transpile: nothing lifted to GMEOW from {stem} — empty draft"),
        }));
    }

    let gap_report_md = gap_report(source_nt, &lift, stem)?;

    let transform = crate::transform::transform_nt(
        &lift.graph_nt,
        &maximal_inputs.ontology_nt,
        &maximal_inputs.cells,
        &maximal_inputs.denied,
        &maximal_inputs.projection_queries,
        internal_tag_map,
    )?;

    Ok(TranspileReport {
        lifted: lift.lifted,
        claimed: lift.claimed,
        gap_terms: lift.gap_terms.len(),
        draft_nt: lift.graph_nt,
        gap_report_md,
        transform,
    })
}

/// Render a Markdown gap report — every un-lifted source triple, listed under its
/// term. The Rust port of `transpile._gap_report`. A triple is un-lifted because its
/// term has **no lift rule** (a coverage gap); never silently dropped.
fn gap_report(source_nt: &str, lift: &UpProjection, stem: &str) -> gmeow_errors::Result<String> {
    let gaps = &lift.gap_terms;
    let source_quads = flat_quads_from_nt(source_nt)?;
    let mut held: BTreeMap<String, Vec<(String, String, String)>> = BTreeMap::new();
    for quad in &source_quads {
        let predicate = quad.predicate.as_str();
        let is_type = predicate == RDF_TYPE && matches!(quad.object, RdfTerm::Iri(_));
        let term = if is_type {
            match &quad.object {
                RdfTerm::Iri(object) => canon_qname(object),
                _ => continue,
            }
        } else {
            canon_qname(predicate)
        };
        if gaps.contains_key(&term) {
            held.entry(term).or_default().push((
                n3_term(&quad.subject),
                format!("<{predicate}>"),
                n3_term(&quad.object),
            ));
        }
    }

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("# Transpile gap report — {stem}\n"));
    lines.push(format!(
        "Lifted **{}** facts + **{}** claims. The terms below could not be faithfully \
lifted to GMEOW — recorded here, never silently dropped.\n",
        lift.lifted, lift.claimed
    ));

    let total: usize = gaps.values().sum();
    lines.push(format!(
        "## Gap terms — {total} triples / {} terms\n",
        gaps.len()
    ));
    lines.push("_no GMEOW lift rule — a coverage gap_\n".to_owned());
    if gaps.is_empty() {
        lines.push("(none)\n".to_owned());
    } else {
        lines.push("| term | triples |".to_owned());
        lines.push("|---|---|".to_owned());
        let mut ordered: Vec<(&String, &usize)> = gaps.iter().collect();
        ordered.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        for (term, count) in ordered {
            lines.push(format!("| `{term}` | {count} |"));
        }
        lines.push(String::new());
    }

    if !held.is_empty() {
        lines.push("## Un-lifted source triples\n".to_owned());
        for term in held.keys() {
            lines.push(format!("### `{term}`\n"));
            lines.push("```turtle".to_owned());
            let mut rows = held[term].clone();
            rows.sort();
            for (s, p, o) in rows {
                lines.push(format!("{s} {p} {o} ."));
            }
            lines.push("```\n".to_owned());
        }
    }

    Ok(lines.join("\n"))
}

/// The N3/N-Triples token of a term — `<iri>` / `_:label` / a typed/lang literal —
/// via the native term renderer.
fn n3_term(term: &RdfTerm) -> String {
    term.to_string()
}

#[path = "projections.tests.rs"]
#[cfg(test)]
mod tests;
