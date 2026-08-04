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
use purrdf::{
    RdfLiteral, RdfQuad, RdfTerm, SerializeGraph, SparqlEngine, SparqlRequest, SparqlResult,
};

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

/// Rewrite the language tag of every literal object whose current tag is a key of
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
        if let RdfTerm::Literal(lit) = &quad.object
            && let Some(lang) = &lit.language
            && let Some(new_lang) = tag_map.get(lang)
        {
            quad.object = RdfTerm::Literal(RdfLiteral {
                lexical_form: lit.lexical_form.clone(),
                datatype: lit.datatype.clone(),
                language: Some(new_lang.clone()),
                direction: lit.direction,
            });
        }
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
    let result = gmeow_logic::reason::reason_all_with_data(bundle.as_ref(), user.as_ref())
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
        let Some(class_iri) = strip_iri_brackets(&axiom.object) else {
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

/// Strip the angle brackets from an `<iri>` display token, returning the bare IRI.
/// The reasoner renders an IRI class object as `<iri>`; anything else (a literal or
/// blank display) is not a class and yields `None`.
fn strip_iri_brackets(token: &str) -> Option<&str> {
    token.strip_prefix('<').and_then(|t| t.strip_suffix('>'))
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

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEMA_PERSON: &str = "https://schema.org/Person";
    const GM_PERSON: &str = "https://blackcatinformatics.ca/gmeow/Person";
    const GM_FULL_NAME: &str = "https://blackcatinformatics.ca/gmeow/fullName";
    const SCHEMA_NAME: &str = "https://schema.org/name";
    const EX_ME: &str = "https://example.org/me";

    fn nt(s: &str, p: &str, o: &str) -> String {
        format!("<{s}> <{p}> <{o}> .\n")
    }

    fn lit(s: &str, p: &str, literal: &str) -> String {
        format!("<{s}> <{p}> {literal} .\n")
    }

    #[test]
    fn profiles_registry_matches_python_names() {
        let profiles = profiles();
        // A representative spread across the phases + the identity/base profiles.
        for name in [
            "schema-org",
            "foaf",
            "vcard",
            "oai_dc",
            "dcat",
            "codemeta",
            "prov",
            "mailmap",
        ] {
            assert!(profiles.contains_key(name), "missing profile {name}");
        }
        assert_eq!(profiles.len(), 27, "profile count drifted");
        assert_eq!(
            profiles["schema-org"].prefixes,
            vec!["schema".to_owned(), "rdfs".to_owned()]
        );
    }

    /// Pin the registry to the suppression leak sweep's coverage set. This
    /// crate registers the projection profiles; `gmeow-validate` declares which
    /// profiles the P10 leak sweep covers. They MUST be identical — otherwise a
    /// newly registered projection profile silently escapes the leak sweep (or a
    /// swept name has no CONSTRUCT). Full set-equality, so add / remove / swap all
    /// trip the gate, restoring the dynamic coverage the Python parametrization had.
    #[test]
    fn registry_equals_the_suppression_leak_sweep_set() {
        let registered: std::collections::BTreeSet<String> = profiles().into_keys().collect();
        let swept: std::collections::BTreeSet<String> =
            gmeow_validate::projection_profiles::PROJECTION_PROFILES
                .iter()
                .map(|s| (*s).to_owned())
                .collect();
        let unswept: Vec<&String> = registered.difference(&swept).collect();
        let unregistered: Vec<&String> = swept.difference(&registered).collect();
        assert!(
            unswept.is_empty() && unregistered.is_empty(),
            "projection registry drifted from the suppression leak sweep — \
             registered-but-unswept: {unswept:?}; swept-but-unregistered: {unregistered:?}"
        );
    }

    #[test]
    fn project_graph_runs_a_construct_to_schema_org() {
        // A minimal gmeow A-box projected by a hand-written CONSTRUCT emits the
        // expected pure schema.org triples — proving the native SPARQL driver runs.
        let source = {
            let mut s = String::new();
            s.push_str(&nt(EX_ME, RDF_TYPE, GM_PERSON));
            s.push_str(&lit(EX_ME, GM_FULL_NAME, "\"Ada Lovelace\""));
            s
        };
        let query = format!(
            "CONSTRUCT {{ ?s a <{SCHEMA_PERSON}> . ?s <{SCHEMA_NAME}> ?n . }} \
             WHERE {{ ?s a <{GM_PERSON}> . ?s <{GM_FULL_NAME}> ?n . }}"
        );
        let out = project_graph(&source, &query, &TagMap::new()).unwrap();
        assert!(
            out.contains(&format!("<{EX_ME}> <{RDF_TYPE}> <{SCHEMA_PERSON}> .")),
            "missing schema:Person type: {out}"
        );
        assert!(
            out.contains(&format!("<{EX_ME}> <{SCHEMA_NAME}> \"Ada Lovelace\" .")),
            "missing schema:name: {out}"
        );
        // Directional & lossy: no gmeow: predicate leaks into the projection.
        assert!(
            !out.contains(GM_FULL_NAME),
            "internal gmeow predicate leaked: {out}"
        );
    }

    #[test]
    fn project_graph_retags_public_language() {
        // The projection-boundary retag rewrites an internal x-gmeow-* literal tag to
        // its public BCP-47 form; a bare pass-through (empty map) leaves it internal.
        let source = lit(EX_ME, GM_FULL_NAME, "\"Ada\"@x-gmeow-english");
        let query =
            format!("CONSTRUCT {{ ?s <{SCHEMA_NAME}> ?n . }} WHERE {{ ?s <{GM_FULL_NAME}> ?n . }}");

        let untagged = project_graph(&source, &query, &TagMap::new()).unwrap();
        assert!(untagged.contains("@x-gmeow-english"), "{untagged}");

        let mut map = TagMap::new();
        map.insert("x-gmeow-english".to_owned(), "en".to_owned());
        let retagged = project_graph(&source, &query, &map).unwrap();
        assert!(retagged.contains("\"Ada\"@en"), "not retagged: {retagged}");
        assert!(!retagged.contains("x-gmeow-english"), "leak: {retagged}");
    }

    #[test]
    fn view_namespaces_resolves_selectors() {
        assert!(view_namespaces("all").unwrap().is_empty());
        assert!(view_namespaces("maximal").unwrap().is_empty());
        assert_eq!(
            view_namespaces("gmeow").unwrap(),
            BTreeSet::from([GM.to_owned()])
        );
        assert_eq!(
            view_namespaces("schema-org").unwrap(),
            BTreeSet::from([
                "https://schema.org/".to_owned(),
                "http://www.w3.org/2000/01/rdf-schema#".to_owned(),
            ])
        );
        assert!(view_namespaces("not-a-profile").is_err());
    }

    #[test]
    fn keep_in_view_filters_by_predicate_and_type() {
        let namespaces = BTreeSet::from(["https://schema.org/".to_owned()]);
        // predicate in namespace → keep
        let name_edge = RdfQuad::new(
            RdfTerm::iri(EX_ME),
            SCHEMA_NAME.to_owned(),
            RdfTerm::literal(RdfLiteral::simple("Ada".to_owned())),
        );
        assert!(keep_in_view(&name_edge, &namespaces));
        // rdf:type to a class in namespace → keep
        let type_edge = RdfQuad::new(
            RdfTerm::iri(EX_ME),
            RDF_TYPE.to_owned(),
            RdfTerm::iri(SCHEMA_PERSON),
        );
        assert!(keep_in_view(&type_edge, &namespaces));
        // gmeow predicate → drop
        let gmeow_edge = RdfQuad::new(
            RdfTerm::iri(EX_ME),
            GM_FULL_NAME.to_owned(),
            RdfTerm::literal(RdfLiteral::simple("Ada".to_owned())),
        );
        assert!(!keep_in_view(&gmeow_edge, &namespaces));
        // rdf:type to a gmeow class → drop
        let gmeow_type = RdfQuad::new(
            RdfTerm::iri(EX_ME),
            RDF_TYPE.to_owned(),
            RdfTerm::iri(GM_PERSON),
        );
        assert!(!keep_in_view(&gmeow_type, &namespaces));
    }

    #[test]
    fn gts_subset_filters_a_maximal_gts_by_view() {
        // Build a maximal-style .gts (gmeow base + a schema.org projection + a
        // provenance reifier), then prove each view keeps exactly its slice.
        let mut maximal = String::new();
        maximal.push_str(&nt(EX_ME, RDF_TYPE, GM_PERSON));
        maximal.push_str(&nt(EX_ME, RDF_TYPE, SCHEMA_PERSON));
        maximal.push_str(&lit(EX_ME, SCHEMA_NAME, "\"Ada\""));
        // an RDF-1.2 reifier row over the derived schema:Person triple
        maximal.push_str(&format!(
            "<{GM}derivations/abcd> <{RDF_REIFIES}> \
             <<( <{EX_ME}> <{RDF_TYPE}> <{SCHEMA_PERSON}> )>> .\n"
        ));

        let gts = build_gts(&maximal);

        // gmeow view: only the pure gmeow base; the schema.org projection + reifier drop.
        let gmeow_view = project_gts_subset(&gts, "gmeow", &TagMap::new()).unwrap();
        assert!(gmeow_view.contains(&format!("<{EX_ME}> <{RDF_TYPE}> <{GM_PERSON}> .")));
        assert!(
            !gmeow_view.contains(SCHEMA_PERSON),
            "schema leaked: {gmeow_view}"
        );
        assert!(
            !gmeow_view.contains(RDF_REIFIES),
            "reifier leaked: {gmeow_view}"
        );

        // schema-org view: the schema triples, not the gmeow-only base type.
        let schema_view = project_gts_subset(&gts, "schema-org", &TagMap::new()).unwrap();
        assert!(schema_view.contains(&format!("<{EX_ME}> <{RDF_TYPE}> <{SCHEMA_PERSON}> .")));
        assert!(schema_view.contains(&format!("<{EX_ME}> <{SCHEMA_NAME}> \"Ada\" .")));
        assert!(
            !schema_view.contains(&format!("<{EX_ME}> <{RDF_TYPE}> <{GM_PERSON}> .")),
            "gmeow-only base type leaked: {schema_view}"
        );

        // all view: everything in the base, but never the reifier rows.
        let all_view = project_gts_subset(&gts, "all", &TagMap::new()).unwrap();
        assert!(all_view.contains(GM_PERSON));
        assert!(all_view.contains(SCHEMA_PERSON));
        assert!(
            !all_view.contains(RDF_REIFIES),
            "reifier leaked into all: {all_view}"
        );
    }

    const FOAF_KNOWS: &str = "http://xmlns.com/foaf/0.1/knows";
    const GM_KNOWS: &str = "https://blackcatinformatics.ca/gmeow/knows";

    /// A hermetic SSSOM lift map: `foaf:knows` cleanly renames to `gmeow:knows`.
    fn knows_sssom() -> String {
        concat!(
            "#curie_map:\n",
            "#  gmeow: https://blackcatinformatics.ca/gmeow/\n",
            "#  foaf: http://xmlns.com/foaf/0.1/\n",
            "#  skos: http://www.w3.org/2004/02/skos/core#\n",
            "subject_id\tpredicate_id\tobject_id\n",
            "gmeow:knows\tskos:exactMatch\tfoaf:knows\n",
        )
        .to_owned()
    }

    #[test]
    fn up_project_lifts_consumer_vocab_to_gmeow() {
        // A foaf:knows source triple lifts up to gmeow:knows through the lawful put
        // executor — the up-projection recall smoke.
        let source_nt = format!("<{EX_ME}> <{FOAF_KNOWS}> <https://example.org/you> .\n");
        let inputs = UpProjectionInputs {
            sssom_texts: vec![knows_sssom()],
            projection_ttls: Vec::new(),
            ontology_nt: String::new(),
            discharged_section_cells: BTreeSet::new(),
        };
        let up = up_project(&source_nt, &inputs, &TagMap::new()).unwrap();
        assert!(up.lifted >= 1, "nothing lifted: {up:?}");
        assert!(
            up.graph_nt.contains(GM_KNOWS),
            "renamed predicate absent: {}",
            up.graph_nt
        );
        assert!(
            !up.graph_nt.contains(FOAF_KNOWS),
            "consumer predicate leaked into GMEOW draft: {}",
            up.graph_nt
        );
    }

    #[test]
    fn transpile_graph_chains_up_then_maximal() {
        // The full transpile: a foaf: source is lifted to pure GMEOW, then run through
        // MAXIMAL(G). A strong equivalentProperty cell mirrors gmeow:knows back out to
        // foaf:knows in the projected layer — the round-trip closes.
        let source_nt = format!("<{EX_ME}> <{FOAF_KNOWS}> <https://example.org/you> .\n");
        let up_inputs = UpProjectionInputs {
            sssom_texts: vec![knows_sssom()],
            projection_ttls: Vec::new(),
            ontology_nt: String::new(),
            discharged_section_cells: BTreeSet::new(),
        };
        let maximal_inputs = MaximalInputs {
            ontology_nt: format!(
                "<{GM_KNOWS}> <{RDF_TYPE}> <http://www.w3.org/2002/07/owl#ObjectProperty> .\n"
            ),
            cells: vec![CellInput {
                iri: "https://blackcatinformatics.ca/gmeow/te/knows-foaf".to_owned(),
                subject: GM_KNOWS.to_owned(),
                predicate_curie: "owl:equivalentProperty".to_owned(),
                object: FOAF_KNOWS.to_owned(),
                confidence: "0.9".to_owned(),
            }],
            denied: Vec::new(),
            projection_queries: Vec::new(),
        };

        let report = transpile_graph(
            &source_nt,
            "smoke",
            &up_inputs,
            &maximal_inputs,
            &TagMap::new(),
        )
        .unwrap();

        assert!(report.lifted >= 1, "nothing lifted: {report:?}");
        assert!(
            report.draft_nt.contains(GM_KNOWS),
            "draft missing gmeow:knows"
        );
        // E(G) mirrored the gmeow:knows edge back out to foaf:knows.
        assert!(report.transform.saturated >= 1, "no saturation: {report:?}");
        assert!(
            report.transform.base_plus_derived_nt.contains(FOAF_KNOWS),
            "equivalentProperty mirror missing: {}",
            report.transform.base_plus_derived_nt
        );
        assert!(!report.transform.gts_bytes.is_empty(), "empty gts");

        // An empty stem is a hard fail, not a silent default.
        assert!(transpile_graph("", " ", &up_inputs, &maximal_inputs, &TagMap::new()).is_err());
    }

    #[test]
    fn transpile_graph_fans_out_without_x_gmeow_leak() {
        // The on-gate, fixture-scale twin of the off-gate CLI process test
        // (`gmeow-cli/tests/self_sufficiency.rs::transpile_blinded_lifts_and_fans_out_without_x_gmeow_leak`),
        // driven through the REAL production chain (`up_project` → `transform_nt`, via
        // `transpile_graph`) — no CLI process, no bundle load. Proves the two halves
        // together, through the FULL chain (not `transform_nt` alone):
        //
        //  * fan-out: an equivalentProperty cell makes MAXIMAL(G) mirror `gmeow:knows`
        //    back out to `foaf:knows` — more triples come out than were asserted.
        //  * zero-leak: a GMEOW-native literal carrying an internal `x-gmeow-*` tag
        //    (passed through `up_project`'s gmeow-namespace passthrough leg unchanged,
        //    per `put_executor::fact_queries`) survives to the MAXIMAL(G) boundary
        //    retagged to its public BCP-47 form — never as the internal tag.
        let source_nt = format!(
            "{}{}",
            nt(EX_ME, FOAF_KNOWS, "https://example.org/you"),
            lit(EX_ME, GM_FULL_NAME, "\"Ada Lovelace\"@x-gmeow-english"),
        );
        let up_inputs = UpProjectionInputs {
            sssom_texts: vec![knows_sssom()],
            projection_ttls: Vec::new(),
            ontology_nt: String::new(),
            discharged_section_cells: BTreeSet::new(),
        };
        let maximal_inputs = MaximalInputs {
            ontology_nt: format!(
                "<{GM_KNOWS}> <{RDF_TYPE}> <http://www.w3.org/2002/07/owl#ObjectProperty> .\n"
            ),
            cells: vec![CellInput {
                iri: "https://blackcatinformatics.ca/gmeow/te/knows-foaf".to_owned(),
                subject: GM_KNOWS.to_owned(),
                predicate_curie: "owl:equivalentProperty".to_owned(),
                object: FOAF_KNOWS.to_owned(),
                confidence: "0.9".to_owned(),
            }],
            denied: Vec::new(),
            projection_queries: Vec::new(),
        };

        // Non-vacuity / negative control: with NO tag_map, the internal tag survives
        // the full chain untouched — proving the fixture genuinely would have leaked
        // before the retag boundary existed (not a vacuously-passing assertion below).
        let unmapped = transpile_graph(
            &source_nt,
            "leak-control",
            &up_inputs,
            &maximal_inputs,
            &TagMap::new(),
        )
        .unwrap();
        assert!(
            unmapped
                .transform
                .base_plus_derived_nt
                .contains("x-gmeow-english"),
            "fixture is vacuous: with an empty tag_map the internal tag should still \
             be present: {}",
            unmapped.transform.base_plus_derived_nt
        );

        let mut tag_map = TagMap::new();
        tag_map.insert("x-gmeow-english".to_owned(), "en".to_owned());

        let report = transpile_graph(
            &source_nt,
            "fanout-no-leak",
            &up_inputs,
            &maximal_inputs,
            &tag_map,
        )
        .unwrap();

        // Fan-out: MAXIMAL(G) genuinely produced MORE than was asserted — the
        // equivalentProperty cell mirrors gmeow:knows back out to foaf:knows.
        assert!(
            report.transform.saturated >= 1,
            "no saturation fan-out fired: {report:?}"
        );
        assert!(
            report.transform.base_plus_derived_nt.contains(&format!(
                "<{EX_ME}> <{FOAF_KNOWS}> <https://example.org/you> ."
            )),
            "equivalentProperty mirror missing: {}",
            report.transform.base_plus_derived_nt
        );

        // Zero-leak: no x-gmeow-* internal tag survives the full chain, on ANY literal
        // (parsed, not just a substring scan) — using the same `is_internal_tag`
        // predicate the P10 suppression leak sweep uses.
        let out_quads = flat_quads_from_nt(&report.transform.base_plus_derived_nt).unwrap();
        assert!(
            out_quads.iter().all(|q| match &q.object {
                RdfTerm::Literal(literal) => literal
                    .language
                    .as_deref()
                    .is_none_or(|lang| !gmeow_validate::language_tags::is_internal_tag(lang)),
                _ => true,
            }),
            "an internal x-gmeow-* tag leaked into MAXIMAL(G): {}",
            report.transform.base_plus_derived_nt
        );
        assert!(
            !report.transform.base_plus_derived_nt.contains("x-gmeow"),
            "x-gmeow substring leaked: {}",
            report.transform.base_plus_derived_nt
        );

        // The properly-tagged public form DOES appear, routed through tag_map.
        assert!(
            report
                .transform
                .base_plus_derived_nt
                .contains("\"Ada Lovelace\"@en"),
            "public BCP-47 retag missing: {}",
            report.transform.base_plus_derived_nt
        );
    }

    #[test]
    fn transpile_graph_rejects_empty_lift() {
        // A source with no lawful lift rule produces an empty draft — surfaced, never
        // a silent empty publication.
        let source_nt = "<https://example.org/x> <https://unknown.example/p> \"v\" .\n";
        let err = transpile_graph(
            source_nt,
            "empty",
            &UpProjectionInputs::default(),
            &MaximalInputs::default(),
            &TagMap::new(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("nothing lifted"), "{err}");
    }

    // ── SIOC reasoned-superclass recovery (Deliverable B) ────────────────
    //
    // The real SIOC ↔ email-thread correspondence: a subject bearing `sioc:has_container`
    // / `sioc:reply_of` lifts to `gmeow:partOfThread` / `gmeow:inReplyTo` (both
    // `rdfs:domain gmeow:Message`), so prp-dom entails it IS a `gmeow:Message`. The IRIs
    // and axioms below are the real ones from `slices/extensions/email/module.ttl`
    // (partOfThread/inReplyTo domain=Message, subPropertyOf=partOf, range=Thread;
    // EmailMessage⊑Message; Message⊑InformationObject) and
    // `slices/core/documents/module.ttl` (FeedPosting⊑Work).
    const SIOC_HAS_CONTAINER: &str = "http://rdfs.org/sioc/ns#has_container";
    const SIOC_REPLY_OF: &str = "http://rdfs.org/sioc/ns#reply_of";
    const GM_PART_OF_THREAD: &str = "https://blackcatinformatics.ca/gmeow/partOfThread";
    const GM_IN_REPLY_TO: &str = "https://blackcatinformatics.ca/gmeow/inReplyTo";
    const GM_PART_OF: &str = "https://blackcatinformatics.ca/gmeow/partOf";
    const GM_MESSAGE: &str = "https://blackcatinformatics.ca/gmeow/Message";
    const GM_EMAIL_MESSAGE: &str = "https://blackcatinformatics.ca/gmeow/EmailMessage";
    const GM_FEED_POSTING: &str = "https://blackcatinformatics.ca/gmeow/FeedPosting";
    const GM_INFORMATION_OBJECT: &str = "https://blackcatinformatics.ca/gmeow/InformationObject";
    const GM_WORK: &str = "https://blackcatinformatics.ca/gmeow/Work";
    const GM_THREAD: &str = "https://blackcatinformatics.ca/gmeow/Thread";

    const SIOC_X: &str = "https://example.org/msg/1";
    const SIOC_THREAD: &str = "https://example.org/thread/1";
    const SIOC_PARENT: &str = "https://example.org/msg/0";

    /// The real property-domain TBox fragment the reasoned harvest reasons over. Uses
    /// the actual email/documents module axioms and IRIs. Crucially it INCLUDES the
    /// `EmailMessage ⊑ Message` and `FeedPosting ⊑ Work` axioms, so AC5's negative
    /// assertions are non-vacuous: EmailMessage/FeedPosting are absent only because
    /// prp-dom + subClassOf propagate UPWARD, never because the axiom is missing.
    fn email_thread_tbox() -> String {
        let mut t = String::new();
        t.push_str(&nt(GM_PART_OF_THREAD, RDFS_DOMAIN, GM_MESSAGE));
        t.push_str(&nt(GM_PART_OF_THREAD, RDFS_RANGE, GM_THREAD));
        t.push_str(&nt(GM_PART_OF_THREAD, RDFS_SUBPROPERTYOF, GM_PART_OF));
        t.push_str(&nt(GM_IN_REPLY_TO, RDFS_DOMAIN, GM_MESSAGE));
        t.push_str(&nt(GM_IN_REPLY_TO, RDFS_RANGE, GM_MESSAGE));
        t.push_str(&nt(GM_MESSAGE, RDFS_SUBCLASSOF, GM_INFORMATION_OBJECT));
        t.push_str(&nt(GM_EMAIL_MESSAGE, RDFS_SUBCLASSOF, GM_MESSAGE));
        t.push_str(&nt(GM_FEED_POSTING, RDFS_SUBCLASSOF, GM_WORK));
        t
    }

    /// The committed shipped bundle (`generated/dist/gmeow.gts`) — the exact snapshot the real
    /// `gmeow-dev up-project` folds.
    fn committed_gts() -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("generated")
            .join("dist")
            .join("gmeow.gts");
        std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
    }

    /// The lifted SIOC image plus the bundle ontology it reasoned against — cached across AC4/AC5.
    struct SiocRun {
        up: UpProjection,
        ontology_nt: String,
    }

    /// Drive the REAL production inverse-ingest entry `up_project` on the SIOC image with inputs
    /// assembled from the committed bundle — the exact `(SSSOM, projection cells, ontology,
    /// discharged verdicts)` the shipped `gmeow-dev up-project` consumes. There is NO synthetic
    /// exactMatch SSSOM: the SIOC thread predicates ship as closeMatch + EDOAL `=` cells, and the
    /// executed discharged `logic:SectionLaw` (Deliverable A) is the sole authorization for the
    /// lawful FACT lift. This test therefore FAILS if the A→B promotion regresses (the SIOC facts
    /// vanish) and PASSES because the promotion lifts them. Cached (the bundle fold + gate machinery
    /// runs once for both acceptance criteria).
    fn sioc_run() -> &'static SiocRun {
        static CACHE: std::sync::OnceLock<SiocRun> = std::sync::OnceLock::new();
        CACHE.get_or_init(|| {
            let gts = committed_gts();
            let sssom_texts: Vec<String> = crate::bundle_blobs::Bundle::from_snapshot(&gts)
                .expect("fold bundle")
                .archive(crate::bundle_blobs::REP_MAPPINGS)
                .expect("mappings archive")
                .into_values()
                .map(|v| String::from_utf8_lossy(&v).into_owned())
                .collect();
            let projection_ttls: Vec<String> = crate::bundle_blobs::Bundle::from_snapshot(&gts)
                .expect("fold bundle")
                .archive(crate::bundle_blobs::REP_CELLS)
                .expect("cells archive")
                .into_iter()
                .filter(|(k, _)| k.ends_with(".ttl"))
                .map(|(_, v)| String::from_utf8_lossy(&v).into_owned())
                .collect();
            let base = gts_base_graph(&gts).expect("base graph");
            let ontology_nt = quads_to_nt(&base).expect("ontology nt");
            let discharged_section_cells =
                discharged_section_cells_from_bundle(&gts).expect("discharged cells");
            let source_nt = format!(
                "{}{}",
                nt(SIOC_X, SIOC_HAS_CONTAINER, SIOC_THREAD),
                nt(SIOC_X, SIOC_REPLY_OF, SIOC_PARENT),
            );
            let inputs = UpProjectionInputs {
                sssom_texts,
                projection_ttls,
                ontology_nt: ontology_nt.clone(),
                discharged_section_cells,
            };
            let up = up_project(&source_nt, &inputs, &TagMap::new())
                .expect("up_project over the real bundle inputs");
            SiocRun { up, ontology_nt }
        })
    }

    /// The `<s> a <class> .` N-Triples line for the SIOC subject.
    fn x_type_line(class: &str) -> String {
        format!("<{SIOC_X}> <{RDF_TYPE}> <{class}> .")
    }

    #[test]
    fn up_project_recovers_message_superclass_via_prp_dom() {
        // AC4 (positive): the real inverse-ingest surface over the SHIPPED bundle. `sioc:has_container`
        // / `sioc:reply_of` lift to `gmeow:partOfThread` / `gmeow:inReplyTo` — as lawful FACTS,
        // authorized by their executed discharged `logic:SectionLaw` (Deliverable A), NOT by any
        // synthetic exactMatch SSSOM. Both lifted predicates have `rdfs:domain gmeow:Message`, so
        // the reasoned harvest recovers the entailed `<X> a gmeow:Message`.
        let up = &sioc_run().up;
        assert!(
            up.graph_nt.contains(GM_PART_OF_THREAD),
            "sioc:has_container did not lift to gmeow:partOfThread: {}",
            up.graph_nt
        );
        assert!(
            up.graph_nt.contains(GM_IN_REPLY_TO),
            "sioc:reply_of did not lift to gmeow:inReplyTo: {}",
            up.graph_nt
        );
        assert!(
            up.lifted >= 2,
            "the two SIOC thread predicates must lift as FACTS (not lossy claims): {up:?}"
        );
        assert!(
            up.graph_nt.contains(&x_type_line(GM_MESSAGE)),
            "entailed gmeow:Message superclass NOT recovered: {}",
            up.graph_nt
        );
    }

    #[test]
    fn up_project_never_fabricates_subkind_or_sibling() {
        // AC5 (negative control, SAME output as AC4): prp-dom + subClassOf are
        // upward-only, so the recovered type is exactly `gmeow:Message` (and its
        // superclasses) — never the `gmeow:EmailMessage` SubKind below it, nor the
        // unrelated `gmeow:FeedPosting` sibling.
        let run = sioc_run();
        let up = &run.up;
        // Sanity: the positive recovery still holds in this same output (guards against
        // the negatives passing only because nothing was reasoned at all).
        assert!(
            up.graph_nt.contains(&x_type_line(GM_MESSAGE)),
            "sanity: gmeow:Message must be recovered here too: {}",
            up.graph_nt
        );
        assert!(
            !up.graph_nt.contains(&x_type_line(GM_EMAIL_MESSAGE)),
            "fabricated a SubKind (gmeow:EmailMessage) — downward invention: {}",
            up.graph_nt
        );
        assert!(
            !up.graph_nt.contains(&x_type_line(GM_FEED_POSTING)),
            "fabricated a sibling (gmeow:FeedPosting): {}",
            up.graph_nt
        );
        // Non-vacuity: EmailMessage ⊑ Message and FeedPosting ⊑ Work ARE in the SHIPPED bundle
        // ontology, so the SubKind/sibling absence above is sound upward-only reasoning, not a
        // missing axiom.
        assert!(
            run.ontology_nt.contains(GM_EMAIL_MESSAGE) && run.ontology_nt.contains(GM_FEED_POSTING),
            "negative control would be vacuous: bundle ontology lacks the SubKind/sibling axioms"
        );
    }

    #[test]
    fn harvest_reasoned_types_needs_world_colocation() {
        // Mis-scope regression — driven at the `harvest_reasoned_types` level (not
        // through `up_project`). WHY this level: the public `up_project` input cannot
        // mis-scope — `harvest_reasoned_types` always lands the lifted assertions and the
        // extracted TBox fragment in the SAME (default) world via
        // `flat_dataset_from_quads`, so co-location is structurally guaranteed for every
        // public caller. To exercise the silent-failure mode we feed the harvest a
        // deliberately mis-scoped dataset: the `gmeow:partOfThread` assertion pinned to a
        // NAMED graph (a distinct reasoning world) while the `rdfs:domain` axiom stays in
        // the default world. The native reasoner is world-indexed with no cross-world
        // union, so prp-dom cannot fire and `gmeow:Message` is NOT derived. If a future
        // refactor breaks world co-location, prp-dom silently no-ops and THIS test fails.
        let tbox = email_thread_tbox();

        // Co-located control: default-graph lifted assertion DOES recover Message — proves
        // the harvest genuinely fires when the worlds coincide (so the negative below is
        // about co-location, not a dead harvest).
        let colocated = vec![RdfQuad::new(
            RdfTerm::iri(SIOC_X),
            GM_PART_OF_THREAD.to_owned(),
            RdfTerm::iri(SIOC_THREAD),
        )];
        let recovered = harvest_reasoned_types(&colocated, &tbox).unwrap();
        assert!(
            recovered.iter().any(|q| q.predicate == RDF_TYPE
                && matches!(&q.object, RdfTerm::Iri(c) if c == GM_MESSAGE)),
            "co-located harvest must recover gmeow:Message: {recovered:?}"
        );

        // Mis-scoped: the SAME assertion in a named graph (a different world) than the
        // domain axiom ⇒ prp-dom no-ops, nothing derived.
        let misscoped = vec![
            RdfQuad::new(
                RdfTerm::iri(SIOC_X),
                GM_PART_OF_THREAD.to_owned(),
                RdfTerm::iri(SIOC_THREAD),
            )
            .in_graph(RdfTerm::iri("https://example.org/other-world")),
        ];
        let harvested = harvest_reasoned_types(&misscoped, &tbox).unwrap();
        assert!(
            !harvested
                .iter()
                .any(|q| matches!(&q.object, RdfTerm::Iri(c) if c == GM_MESSAGE)),
            "mis-scoped assertion must NOT derive gmeow:Message — world co-location is \
             load-bearing: {harvested:?}"
        );
    }

    /// Compose a `.gts` from an N-Triples (base + RDF-1.2 statement layer) document,
    /// via the same native `gts_compose` path the transform kernel uses.
    fn build_gts(nt: &str) -> Vec<u8> {
        let dataset = purrdf::parse_dataset(nt.as_bytes(), NT_MEDIA_TYPE, None).unwrap();
        let mut builder = purrdf::gts_compose::SnapshotBuilder::new();
        builder.add_dataset(&dataset).unwrap();
        purrdf::gts_compose::emit_gts(
            &builder,
            "dist",
            None,
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
            purrdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
            &purrdf::gts_compose::MediumPlan::dist_default(None),
        )
        .unwrap()
    }
}
