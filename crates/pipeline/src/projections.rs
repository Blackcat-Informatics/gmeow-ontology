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

use purrdf::sparql::NativeSparqlEngine;
use purrdf::{
    RdfLiteral, RdfQuad, RdfTerm, SerializeGraph, SparqlEngine, SparqlRequest, SparqlResult,
};

use crate::transform::{CellInput, TransformReportNative};
use crate::up_projection_corpus::{canon_qname, PREFIXES};

const GM: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
const NT_MEDIA_TYPE: &str = "application/n-triples";

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
fn flat_quads_from_nt(nt: &str) -> Result<Vec<RdfQuad>, String> {
    if nt.trim().is_empty() {
        return Ok(Vec::new());
    }
    let parsed = purrdf::parse_dataset(nt.as_bytes(), NT_MEDIA_TYPE, None)
        .map_err(|e| format!("N-Triples parse failed: {e}"))?;
    Ok(purrdf::flat_rdf_quads_from_dataset(parsed.as_ref()))
}

/// Serialize a flat default-graph quad stream to canonical N-Triples.
fn quads_to_nt(quads: &[RdfQuad]) -> Result<String, String> {
    let flat = purrdf::flat_dataset_from_quads(quads)
        .map_err(|e| format!("N-Triples flatten failed: {e}"))?;
    let bytes =
        purrdf::serialize_dataset(flat.as_ref(), NT_MEDIA_TYPE, SerializeGraph::DefaultGraph)
            .map_err(|e| format!("N-Triples serialization failed: {e}"))?;
    String::from_utf8(bytes).map_err(|e| format!("N-Triples output is not UTF-8: {e}"))
}

/// Rewrite the language tag of every literal object whose current tag is a key of
/// `tag_map`, in place over the owned quad stream. The projection-boundary retag: an
/// empty map is a no-op. Idempotent for already-remapped literals.
fn retag_quads(quads: &mut [RdfQuad], tag_map: &TagMap) {
    if tag_map.is_empty() {
        return;
    }
    for quad in quads.iter_mut() {
        if let RdfTerm::Literal(lit) = &quad.object {
            if let Some(lang) = &lit.language {
                if let Some(new_lang) = tag_map.get(lang) {
                    quad.object = RdfTerm::Literal(RdfLiteral {
                        lexical_form: lit.lexical_form.clone(),
                        datatype: lit.datatype.clone(),
                        language: Some(new_lang.clone()),
                        direction: lit.direction,
                    });
                }
            }
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
pub fn project_graph(source_nt: &str, query: &str, tag_map: &TagMap) -> Result<String, String> {
    let source_quads = flat_quads_from_nt(source_nt)?;
    let ds = purrdf::flat_dataset_from_quads(&source_quads)
        .map_err(|e| format!("source dataset build failed: {e}"))?;
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
        .map_err(|e| format!("projection query evaluation failed: {e}"))?;
    let SparqlResult::Graph(triples) = result else {
        return Err("projection query did not return a graph".to_owned());
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
pub fn gts_base_graph(gts_bytes: &[u8]) -> Result<Vec<RdfQuad>, String> {
    let dataset = purrdf::gts::flattened_dataset_from_bytes(gts_bytes)
        .map_err(|e| format!("gts read failed: {e}"))?;
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

/// The IRI namespaces a single-vocab view keeps (empty = keep everything).
///
/// The Rust port of `projections._view_namespaces`. `all` / `maximal` keep the whole
/// maximal product; `gmeow` keeps only the pure GMEOW base; any other name is a
/// projection profile, whose registered prefixes resolve to their namespaces.
pub fn view_namespaces(view: &str) -> Result<BTreeSet<String>, String> {
    if GTS_VIEW_ALL.contains(&view) {
        return Ok(BTreeSet::new());
    }
    if view == GTS_VIEW_GMEOW {
        return Ok(BTreeSet::from([GM.to_owned()]));
    }
    let profiles = profiles();
    let profile = profiles
        .get(view)
        .ok_or_else(|| format!("unknown gts view / projection profile: {view}"))?;
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
) -> Result<String, String> {
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
    if quad.predicate == RDF_TYPE {
        if let RdfTerm::Iri(object) = &quad.object {
            return namespaces.iter().any(|ns| object.starts_with(ns));
        }
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
    /// The projection/EDOAL TTL sources.
    pub projection_ttls: Vec<String>,
    /// The asserted ontology, as N-Triples.
    pub ontology_nt: String,
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
) -> Result<UpProjection, String> {
    let report = crate::put_executor::execute_put_legs(
        source_nt,
        &inputs.sssom_texts,
        &inputs.projection_ttls,
        &inputs.ontology_nt,
    )?;
    let graph_nt = if internal_tag_map.is_empty() {
        report.graph_nt
    } else {
        let mut quads = flat_quads_from_nt(&report.graph_nt)?;
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
) -> Result<TranspileReport, String> {
    if stem.trim().is_empty() {
        return Err("transpile_graph: stem must be a non-empty string".to_owned());
    }

    let lift = up_project(source_nt, up_inputs, internal_tag_map)?;
    if lift.graph_nt.trim().is_empty() {
        return Err(format!(
            "transpile: nothing lifted to GMEOW from {stem} — empty draft"
        ));
    }

    let gap_report_md = gap_report(source_nt, &lift, stem)?;

    let transform = crate::transform::transform_nt(
        &lift.graph_nt,
        &maximal_inputs.ontology_nt,
        &maximal_inputs.cells,
        &maximal_inputs.denied,
        &maximal_inputs.projection_queries,
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
fn gap_report(source_nt: &str, lift: &UpProjection, stem: &str) -> Result<String, String> {
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
        assert!(err.contains("nothing lifted"), "{err}");
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
        )
        .unwrap()
    }
}
