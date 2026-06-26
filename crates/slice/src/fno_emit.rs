// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native FnO emission — the whole of GMEOW's `functions.fno.ttl` emitter, sourced
//! entirely from Rust (#848).
//!
//! This is the SUBSUME/ENHANCE move that keeps the FnO *emission* orchestrator in
//! the slice framework, mirroring the SSSOM emitter ([`crate::mapping_emit`]).
//! Python passes nothing but the repo-root path; every input is discovered
//! natively here:
//!
//! * **Projection functions** (`gmeow:ProjectionFunction`) and **projection cells**
//!   (`gmeow:ProjectionMapping`) ← the DSL source set: the shared `dsl/mappings/`
//!   tree (globbed `*.ttl` recursively) + the slice [`ArtifactRole::Mapping`]
//!   artifacts (their Turtle `content` bytes). This is the SAME source set the
//!   SSSOM path reads — `load_dsl` parses both into one merged graph.
//! * **The ontology** (for each input predicate's `rdfs:range`, the fail-closed
//!   `fno:type` derivation) ← `ontology/gmeow.ttl` + every slice
//!   [`ArtifactRole::Module`] artifact, merged into one store (the
//!   `load_merged_graph(include_imports=False)` source set, the
//!   `validate_all` Module-merge precedent).
//!
//! The emitted Turtle (re-parsed by Python into a fresh rdflib `Graph`) is held to
//! the **graph-isomorphism** parity gate (`gmeow-dev regenerate mappings` must show
//! zero drift on `generated/projections/functions.fno.ttl`). The emission rules
//! (the document node, the per-function `fno:Function`/`fno:Parameter`/`fno:Output`
//! nodes + `fno:expects`/`fno:returns` `rdf:List`s, the `fnom`
//! Implementation/Mapping/PropertyParameterMapping/DefaultReturnMapping graph) are
//! reproduced exactly; the [`gmeow_rdf::fno`] serializer owns the triple shapes,
//! this module owns the derivation (range typing, IRI minting, the `used_by` /
//! `transform_cells` scan, `_var_for_predicate` / `_output_var`, the sort order, and
//! the deterministic blank-node labels).
//!
//! ## Why this lives in `gmeow-slice`
//!
//! `gmeow-slice` is the one crate that depends on `gmeow-rdf` (the FnO catalog
//! model + serializer) *and* owns [`SliceCatalog`], so it is the only place that can
//! both discover the slice mapping + module sources and reuse the serializer.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use gmeow_rdf::fno::{
    self, FnFunction, FnImpl, FnMapping, FnOutput, FnParam, FnParamMapping, FnReturnMapping,
    FnoCatalog,
};
use gmeow_rdf::{turtle, RdfQuad, RdfTerm};
use oxigraph::model::{GraphNameRef, NamedNode, NamedOrBlankNode, Term};
use oxigraph::store::Store;

use crate::artifact::ArtifactRole;
use crate::catalog::SliceCatalog;
use crate::error::SliceError;

// ── Namespace constants ───────────────────────────────────────────────────────

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const ONTOLOGY_IRI: &str = "https://blackcatinformatics.ca/gmeow";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";

const GM_PROJECTION_FUNCTION: &str = "https://blackcatinformatics.ca/gmeow/ProjectionFunction";
const GM_FN_INPUT: &str = "https://blackcatinformatics.ca/gmeow/fnInput";
const GM_FN_INPUT_OPTIONAL: &str = "https://blackcatinformatics.ca/gmeow/fnInputOptional";
const GM_FN_OUTPUT: &str = "https://blackcatinformatics.ca/gmeow/fnOutput";
const GM_FN_OUTPUT_TYPE: &str = "https://blackcatinformatics.ca/gmeow/fnOutputType";

const GM_PROJECTION_MAPPING: &str = "https://blackcatinformatics.ca/gmeow/ProjectionMapping";
const GM_HAS_MAPPING_PATTERN: &str = "https://blackcatinformatics.ca/gmeow/hasMappingPattern";
const GM_HAS_BINDING: &str = "https://blackcatinformatics.ca/gmeow/hasBinding";
const GM_PROFILE: &str = "https://blackcatinformatics.ca/gmeow/profile";
const GM_TRANSFORM: &str = "https://blackcatinformatics.ca/gmeow/transform";

const GM_VALUE: &str = "https://blackcatinformatics.ca/gmeow/value";
const GM_ATOM: &str = "https://blackcatinformatics.ca/gmeow/atom";
const GM_OPTIONAL_GROUP: &str = "https://blackcatinformatics.ca/gmeow/optionalGroup";
const GM_BIND: &str = "https://blackcatinformatics.ca/gmeow/bind";
const GM_BIND_VAR: &str = "https://blackcatinformatics.ca/gmeow/bindVar";
const GM_BIND_EXPR: &str = "https://blackcatinformatics.ca/gmeow/bindExpr";
const GM_EXPR_VAR: &str = "https://blackcatinformatics.ca/gmeow/exprVar";
const GM_EXPR_OP: &str = "https://blackcatinformatics.ca/gmeow/exprOp";
const GM_EXPR_ARGS: &str = "https://blackcatinformatics.ca/gmeow/exprArgs";

const GM_SUBJECT_VAR: &str = "https://blackcatinformatics.ca/gmeow/subjectVar";
const GM_T_SUBJ: &str = "https://blackcatinformatics.ca/gmeow/tSubj";
const GM_PREDICATE: &str = "https://blackcatinformatics.ca/gmeow/predicate";
const GM_T_PRED: &str = "https://blackcatinformatics.ca/gmeow/tPred";
const GM_PATH: &str = "https://blackcatinformatics.ca/gmeow/path";
const GM_PATH_ALTS: &str = "https://blackcatinformatics.ca/gmeow/pathAlts";
const GM_ALT_PATH: &str = "https://blackcatinformatics.ca/gmeow/AltPath";
const GM_OBJECT_VAR: &str = "https://blackcatinformatics.ca/gmeow/objectVar";
const GM_T_OBJ: &str = "https://blackcatinformatics.ca/gmeow/tObj";

/// The generated-banner `rdfs:comment` (mirrors `_GENERATED_BANNER`).
const GENERATED_BANNER: &str =
    "GENERATED by `gmeow regenerate` (mappings) from mapping-dsl/ — DO NOT EDIT.";
/// The document `rdfs:label` (mirrors `emit_fno`'s literal).
const DOC_LABEL: &str = "GMEOW projection functions (FnO)";
/// The fno:Implementation `dcterms:format`.
const SPARQL_FORMAT: &str = "application/sparql-query";
/// The fallback profile when a function is used by no transform-bearing binding
/// (mirrors `profiles = used_by.get(fn_iri) or ["schema-org"]`).
const DEFAULT_PROFILE: &str = "schema-org";

// ── Native DSL model (the FnO-relevant subset) ───────────────────────────────────

/// One `gmeow:ProjectionFunction` declaration (mirrors `ProjectionFunction`).
#[derive(Debug, Clone)]
struct ProjectionFunction {
    iri: String,
    label: String,
    description: String,
    /// Required input predicate IRIs (`gmeow:fnInput`), in store-iteration order.
    inputs: Vec<String>,
    /// Optional input predicate IRIs (`gmeow:fnInputOptional`).
    optional_inputs: Vec<String>,
    output: String,
    output_type: String,
}

/// One pattern atom's FnO-relevant fields (mirrors `Atom` — only the parts
/// `_var_for_predicate` reads: the object var, the plain predicate, and the
/// top-level alternation-path alternatives).
#[derive(Debug, Clone)]
struct Atom {
    predicate: Option<String>,
    /// Alternatives, when the path is a top-level `gmeow:AltPath` of plain
    /// predicates (mirrors `Atom.path_alts`).
    path_alts: Vec<String>,
    object_var: Option<String>,
}

/// A derived binding (`BIND expr AS ?var`) — only the var + the referenced vars of
/// its expression are needed (for `_order_binds`'s topological tiebreak).
#[derive(Debug, Clone)]
struct Bind {
    var: String,
    /// The set of variables this bind's expression references (own-set filtered at
    /// ordering time).
    refs: BTreeSet<String>,
}

/// The FnO-relevant subset of a projection cell's pattern + bindings.
#[derive(Debug, Clone)]
struct ProjectionCell {
    /// The flattened pattern atoms (OPTIONAL groups recursed).
    atoms: Vec<Atom>,
    /// `pattern.value` (the single-value projection variable), if present.
    value: Option<String>,
    /// The deterministically-ordered bind variables (`pattern.binds`); the last is
    /// `_output_var`'s structural-transform fallback.
    binds: Vec<String>,
    /// Per-binding `(profile, transform)` for every binding that names a transform.
    transform_bindings: Vec<(String, String)>,
}

// ── Public API ─────────────────────────────────────────────────────────────────

/// Emit the FnO function catalog (`generated/projections/functions.fno.ttl`) from
/// the repo at `root`, returning its N-Triples text.
///
/// All inputs are sourced natively from `root`:
///
/// * projection functions + cells ← the shared `dsl/mappings/**/*.ttl` tree + slice
///   [`ArtifactRole::Mapping`] artifacts (via [`SliceCatalog::discover`]);
/// * each input predicate's `rdfs:range` ← `ontology/gmeow.ttl` + slice
///   [`ArtifactRole::Module`] artifacts.
///
/// The text is full-IRI N-Triples; the Python caller re-parses it into a fresh
/// rdflib `Graph` (so the downstream `projection_lint` + the Turtle writer are
/// unchanged), and the result is graph-isomorphic to the historical Python emitter.
///
/// # Errors
///
/// Returns [`SliceError`] on any missing/unparsable required source, a malformed
/// function declaration, or — the fail-closed `fno:type` guard — an input predicate
/// with no ontology `rdfs:range` or a param-IRI minting collision. No degraded
/// fallback (CONSTITUTION / no-compromises).
pub fn emit_fno(root: &Path) -> Result<String, SliceError> {
    let dsl_store = collect_dsl_store(root)?;
    let onto_store = collect_ontology_store(root)?;
    let functions = extract_functions(&dsl_store)?;
    let cells = extract_cells(&dsl_store)?;
    let catalog = build_catalog(&functions, &cells, &onto_store)?;

    // Projection boundary: retag every internal `@x-gmeow-*` language tag to its
    // public BCP-47 form before the Turtle write (#287). The serializer in
    // `gmeow_rdf::fno` mints `@x-gmeow-english` literals; the committed
    // `functions.fno.ttl` carries the public `@en` retag, so this is the same
    // internal→public boundary as `edoal_emit`.
    let tag_map = build_tag_map(&onto_store)?;
    let quads: Vec<RdfQuad> = fno::to_quads(&catalog)
        .into_iter()
        .map(|q| retag_quad(q, &tag_map))
        .collect();
    Ok(quads.iter().map(turtle::emit_quad).collect())
}

/// Build the internal→BCP-47 tag map (`gmeow:languageTag` → `gmeow:bcp47Tag`) from
/// the ontology store. Mirrors `edoal_emit::build_tag_map` — the only tag FnO uses
/// is `x-gmeow-english` → `en`, but the map is read from the graph, never hardcoded.
fn build_tag_map(store: &Store) -> Result<BTreeMap<String, String>, SliceError> {
    use crate::sparql_emit::{first_lexical_of_iri, quads_with_predicate, term_iri, term_lexical};
    const GM_LANGUAGE_TAG: &str = "https://blackcatinformatics.ca/gmeow/languageTag";
    const GM_BCP47_TAG: &str = "https://blackcatinformatics.ca/gmeow/bcp47Tag";

    let mut map: BTreeMap<String, String> = BTreeMap::new();
    for (subject, object) in quads_with_predicate(store, GM_LANGUAGE_TAG)? {
        let Some(internal) = term_lexical(&object) else {
            continue;
        };
        let Some(subj_iri) = term_iri(&subject) else {
            continue;
        };
        if let Some(ext) = first_lexical_of_iri(store, &subj_iri, GM_BCP47_TAG)? {
            map.insert(internal, ext);
        }
    }
    Ok(map)
}

/// Retag a quad's language-tagged literal object through `tag_map` (a public tag —
/// no `x-gmeow-` mapping — passes through unchanged). Only the object can carry a
/// localizable literal in the FnO catalog.
fn retag_quad(mut quad: RdfQuad, tag_map: &BTreeMap<String, String>) -> RdfQuad {
    if let RdfTerm::Literal(lit) = &mut quad.object {
        if let Some(lang) = &lit.language {
            if let Some(ext) = tag_map.get(lang) {
                lit.language = Some(ext.clone());
            }
        }
    }
    quad
}

// ── Source collection ──────────────────────────────────────────────────────────

/// Build the merged DSL store: the shared `dsl/mappings/**/*.ttl` tree + the slice
/// `mappings/*.ttl` artifacts (the `load_dsl` source set). Sorted-path insertion
/// keeps store iteration deterministic.
fn collect_dsl_store(root: &Path) -> Result<Store, SliceError> {
    let store = new_store()?;

    let dsl_dir = root.join("dsl").join("mappings");
    let mut dsl_files: Vec<std::path::PathBuf> = Vec::new();
    collect_ttl_files(&dsl_dir, &mut dsl_files)?;
    dsl_files.sort();
    for path in &dsl_files {
        let bytes = std::fs::read(path).map_err(SliceError::Io)?;
        load_into_store(&store, &bytes, path)?;
    }

    let slices_dir = root.join("slices");
    if slices_dir.is_dir() {
        let catalog = SliceCatalog::discover(&slices_dir)?;
        let mut slice_mappings: Vec<(std::path::PathBuf, Vec<u8>)> = Vec::new();
        for record in catalog.records() {
            for artifact in &record.artifacts {
                if artifact.role == ArtifactRole::Mapping {
                    let path = record.slice_dir.join(&artifact.logical_path);
                    slice_mappings.push((path, artifact.content.clone()));
                }
            }
        }
        slice_mappings.sort_by(|a, b| a.0.cmp(&b.0));
        for (path, bytes) in &slice_mappings {
            load_into_store(&store, bytes, path)?;
        }
    }
    Ok(store)
}

/// Build the merged ontology store for `rdfs:range` lookups: `ontology/gmeow.ttl` +
/// every slice [`ArtifactRole::Module`] artifact (the
/// `load_merged_graph(include_imports=False)` source set).
///
/// `pub(crate)` so the projection lint ([`crate::projection_lint`]) reuses the SAME
/// ontology-merge the emitter's `fno:type` derivation reads (one source of truth for
/// `rdfs:range`).
pub(crate) fn collect_ontology_store(root: &Path) -> Result<Store, SliceError> {
    let store = new_store()?;

    let ontology_file = root.join("ontology").join("gmeow.ttl");
    if ontology_file.is_file() {
        let bytes = std::fs::read(&ontology_file).map_err(SliceError::Io)?;
        load_into_store(&store, &bytes, &ontology_file)?;
    }

    let slices_dir = root.join("slices");
    if slices_dir.is_dir() {
        let catalog = SliceCatalog::discover(&slices_dir)?;
        let mut modules: Vec<(std::path::PathBuf, Vec<u8>)> = Vec::new();
        for record in catalog.records() {
            for artifact in &record.artifacts {
                if artifact.role == ArtifactRole::Module {
                    let path = record.slice_dir.join(&artifact.logical_path);
                    modules.push((path, artifact.content.clone()));
                }
            }
        }
        modules.sort_by(|a, b| a.0.cmp(&b.0));
        for (path, bytes) in &modules {
            load_into_store(&store, bytes, path)?;
        }
    }
    Ok(store)
}

fn new_store() -> Result<Store, SliceError> {
    Store::new().map_err(|e| SliceError::Parse(format!("store creation failed: {e}")))
}

/// Recursively collect every `*.ttl` file under `dir` (no-op if `dir` is absent).
fn collect_ttl_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), SliceError> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir).map_err(SliceError::Io)? {
        let entry = entry.map_err(SliceError::Io)?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(SliceError::Io)?;
        if file_type.is_dir() {
            collect_ttl_files(&path, out)?;
        } else if file_type.is_file() && path.extension().and_then(|e| e.to_str()) == Some("ttl") {
            out.push(path);
        }
    }
    Ok(())
}

/// Parse one Turtle source into the shared store (lenient, so GMEOW's `@x-gmeow-*`
/// language tags parse — mirrors `mapping_emit::load_into_store`).
fn load_into_store(store: &Store, bytes: &[u8], path: &Path) -> Result<(), SliceError> {
    crate::rdf_text::turtle_bytes_into_store(store, bytes, &path.display().to_string())
}

// ── Function / cell extraction ───────────────────────────────────────────────────

/// Extract every `gmeow:ProjectionFunction`.
fn extract_functions(store: &Store) -> Result<Vec<ProjectionFunction>, SliceError> {
    let mut out: Vec<ProjectionFunction> = Vec::new();
    for fn_iri in subjects_of_type(store, GM_PROJECTION_FUNCTION)? {
        let node = NamedNode::new(&fn_iri)
            .map_err(|e| SliceError::Parse(format!("invalid function IRI {fn_iri}: {e}")))?;
        let output = object_iri(store, &node, GM_FN_OUTPUT)?;
        let output_type = object_iri(store, &node, GM_FN_OUTPUT_TYPE)?;
        let (Some(output), Some(output_type)) = (output, output_type) else {
            return Err(SliceError::Parse(format!(
                "projection function {fn_iri} missing fnOutput/fnOutputType"
            )));
        };
        out.push(ProjectionFunction {
            iri: fn_iri,
            label: object_literal(store, &node, RDFS_LABEL)?.unwrap_or_default(),
            description: object_literal(store, &node, SKOS_DEFINITION)?.unwrap_or_default(),
            inputs: object_iris(store, &node, GM_FN_INPUT)?,
            optional_inputs: object_iris(store, &node, GM_FN_INPUT_OPTIONAL)?,
            output,
            output_type,
        });
    }
    Ok(out)
}

/// Extract every `gmeow:ProjectionMapping`'s FnO-relevant subset.
fn extract_cells(store: &Store) -> Result<Vec<ProjectionCell>, SliceError> {
    let mut out: Vec<ProjectionCell> = Vec::new();
    for cell_iri in subjects_of_type(store, GM_PROJECTION_MAPPING)? {
        let cell = NamedNode::new(&cell_iri)
            .map_err(|e| SliceError::Parse(format!("invalid mapping IRI {cell_iri}: {e}")))?;
        let Some(pattern_node) = first_object(store, &cell, GM_HAS_MAPPING_PATTERN)? else {
            return Err(SliceError::Parse(format!(
                "projection mapping {cell_iri} missing hasMappingPattern"
            )));
        };

        let (atoms, value, binds) = parse_pattern(store, &pattern_node)?;
        let transform_bindings = parse_transform_bindings(store, &cell)?;
        out.push(ProjectionCell {
            atoms,
            value,
            binds,
            transform_bindings,
        });
    }
    Ok(out)
}

/// The FnO-relevant fields of a parsed pattern: `(atoms, value, ordered binds)`.
type ParsedPattern = (Vec<Atom>, Option<String>, Vec<String>);

/// Parse the FnO-relevant pattern fields: the flattened atoms, the value var, and
/// the deterministically-ordered bind vars.
fn parse_pattern(store: &Store, pattern: &Term) -> Result<ParsedPattern, SliceError> {
    let value = object_literal_of_term(store, pattern, GM_VALUE)?;

    // atoms — the ordered rdf:List of pattern items, flattened over OPTIONAL groups.
    let atom_head = first_object_of_term(store, pattern, GM_ATOM)?;
    let mut atoms: Vec<Atom> = Vec::new();
    for item in rdf_list(store, atom_head.as_ref())? {
        flatten_item(store, &item, &mut atoms)?;
    }

    // binds — an unordered set, ordered deterministically by _order_binds.
    let mut raw_binds: Vec<Bind> = Vec::new();
    for bind_node in objects_of_term(store, pattern, GM_BIND)? {
        raw_binds.push(parse_bind(store, &bind_node)?);
    }
    let binds = order_binds(raw_binds);

    Ok((atoms, value, binds))
}

/// Parse one pattern item into the flattened atom list, recursing OPTIONAL groups.
fn flatten_item(store: &Store, node: &Term, out: &mut Vec<Atom>) -> Result<(), SliceError> {
    if let Some(group_head) = first_object_of_term(store, node, GM_OPTIONAL_GROUP)? {
        for item in rdf_list(store, Some(&group_head))? {
            flatten_item(store, &item, out)?;
        }
        return Ok(());
    }
    out.push(parse_atom(store, node)?);
    Ok(())
}

/// Parse one atom's FnO-relevant fields: the plain predicate, the top-level
/// alt-path alternatives, and the object var. `tSubj/tPred/tObj` template aliases
/// are honoured for parity.
fn parse_atom(store: &Store, node: &Term) -> Result<Atom, SliceError> {
    let predicate = match object_iri_of_term(store, node, GM_PREDICATE)? {
        Some(p) => Some(p),
        None => object_iri_of_term(store, node, GM_T_PRED)?,
    };
    let path_node = first_object_of_term(store, node, GM_PATH)?;
    let path_alts = match &path_node {
        Some(p) => alt_members(store, p)?,
        None => Vec::new(),
    };
    let object_var = match object_literal_of_term(store, node, GM_OBJECT_VAR)? {
        Some(v) => Some(v),
        None => object_literal_of_term(store, node, GM_T_OBJ)?,
    };
    // subject var is parsed for shape parity (subjectVar/tSubj) but FnO does not
    // read it; we still require one to match the Python parser's contract.
    let subj = object_literal_of_term(store, node, GM_SUBJECT_VAR)?
        .or(object_literal_of_term(store, node, GM_T_SUBJ)?);
    if subj.is_none() {
        return Err(SliceError::Parse(
            "atom missing subjectVar/tSubj".to_owned(),
        ));
    }
    Ok(Atom {
        predicate,
        path_alts,
        object_var,
    })
}

/// If `node` is a top-level `gmeow:AltPath` of plain predicates, return them; else
/// `()`.
fn alt_members(store: &Store, node: &Term) -> Result<Vec<String>, SliceError> {
    // A bare predicate IRI (or a literal) is never an alternation node; only a
    // blank-node AltPath structure is.
    if !matches!(node, Term::BlankNode(_)) {
        return Ok(Vec::new());
    }
    let types = types_of_term(store, node)?;
    if !types.iter().any(|t| t == GM_ALT_PATH) {
        return Ok(Vec::new());
    }
    let head = first_object_of_term(store, node, GM_PATH_ALTS)?;
    let members = rdf_list(store, head.as_ref())?;
    // Only when EVERY member is a plain predicate IRI (mirrors the all-URIRef gate).
    let mut alts: Vec<String> = Vec::new();
    for m in &members {
        match m {
            Term::NamedNode(nn) => alts.push(nn.as_str().to_owned()),
            _ => return Ok(Vec::new()),
        }
    }
    Ok(alts)
}

/// Parse one bind node: its var + the set of variables its expression references.
fn parse_bind(store: &Store, node: &Term) -> Result<Bind, SliceError> {
    let Some(var) = object_literal_of_term(store, node, GM_BIND_VAR)? else {
        return Err(SliceError::Parse("bind missing bindVar".to_owned()));
    };
    let mut refs: BTreeSet<String> = BTreeSet::new();
    if let Some(expr_node) = first_object_of_term(store, node, GM_BIND_EXPR)? {
        collect_expr_vars(store, &expr_node, &mut refs)?;
    } else {
        return Err(SliceError::Parse("bind missing bindExpr".to_owned()));
    }
    Ok(Bind { var, refs })
}

/// Recursively gather every `gmeow:exprVar` referenced by an expression subtree.
fn collect_expr_vars(
    store: &Store,
    node: &Term,
    out: &mut BTreeSet<String>,
) -> Result<(), SliceError> {
    // A literal/IRI constant references no variable.
    if matches!(node, Term::NamedNode(_) | Term::Literal(_)) {
        return Ok(());
    }
    if let Some(var) = object_literal_of_term(store, node, GM_EXPR_VAR)? {
        out.insert(var);
        return Ok(());
    }
    // An operator application: recurse its argument list.
    if first_object_of_term(store, node, GM_EXPR_OP)?.is_some() {
        let args_head = first_object_of_term(store, node, GM_EXPR_ARGS)?;
        for arg in rdf_list(store, args_head.as_ref())? {
            collect_expr_vars(store, &arg, out)?;
        }
    }
    Ok(())
}

/// Order BIND declarations deterministically in dependency order with an
/// alphabetical tiebreak among independent binds — the committed canonical order,
/// so `_output_var`'s `binds[-1]` is stable.
fn order_binds(binds: Vec<Bind>) -> Vec<String> {
    let own: BTreeSet<String> = binds.iter().map(|b| b.var.clone()).collect();
    // deps[v] = (expr vars ∩ own) − {v}
    let mut deps: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for b in &binds {
        let d: BTreeSet<String> = b
            .refs
            .iter()
            .filter(|v| own.contains(*v) && **v != b.var)
            .cloned()
            .collect();
        deps.insert(b.var.clone(), d);
    }
    let mut placed: BTreeSet<String> = BTreeSet::new();
    let mut remaining: BTreeSet<String> = own.clone();
    let mut ordered: Vec<String> = Vec::new();
    while !remaining.is_empty() {
        // ready = sorted vars whose deps are all placed; if none, break the cycle
        // deterministically by taking the sorted remaining set.
        let mut ready: Vec<String> = remaining
            .iter()
            .filter(|v| deps[*v].is_subset(&placed))
            .cloned()
            .collect();
        if ready.is_empty() {
            ready = remaining.iter().cloned().collect();
        }
        // (BTreeSet iteration is already sorted; ready is therefore sorted.)
        for var in &ready {
            ordered.push(var.clone());
            placed.insert(var.clone());
        }
        for var in &ready {
            remaining.remove(var);
        }
    }
    ordered
}

/// Parse the `(profile, transform)` of every binding that names a transform
/// transform.
fn parse_transform_bindings(
    store: &Store,
    cell: &NamedNode,
) -> Result<Vec<(String, String)>, SliceError> {
    let mut out: Vec<(String, String)> = Vec::new();
    for binding in objects_of(store, cell, GM_HAS_BINDING)? {
        let Some(transform) = object_iri_of_term(store, &binding, GM_TRANSFORM)? else {
            continue;
        };
        let Some(profile) = object_literal_of_term(store, &binding, GM_PROFILE)? else {
            return Err(SliceError::Parse(
                "profile binding missing profile".to_owned(),
            ));
        };
        out.push((profile, transform));
    }
    Ok(out)
}

// ── Derivation (emit_fno + _emit_fnom) ───────────────────────────────────────────

/// The local name of an IRI — the substring after the last `#` or `/`.
fn local(iri: &str) -> String {
    let cut = iri.rfind(['#', '/']).map(|i| i + 1).unwrap_or(0);
    iri[cut..].to_owned()
}

/// The parameter IRI minted from a source predicate (mirrors `_param_iri`):
/// `gmeow:param<LocalNameCapitalised>`.
fn param_iri(predicate: &str) -> String {
    let local = local(predicate);
    let mut chars = local.chars();
    let head = match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>(),
        None => String::new(),
    };
    let rest: String = chars.collect();
    format!("{GMEOW}param{head}{rest}")
}

/// The output IRI minted from a function IRI (mirrors `_output_iri`): drop a leading
/// `fn` from the local name and prefix `gmeow:out`.
fn output_iri(fn_iri: &str) -> String {
    let local = local(fn_iri);
    let stem = local.strip_prefix("fn").unwrap_or(&local);
    format!("{GMEOW}out{stem}")
}

/// PascalCase a profile token the way `_camel` does (`-`/`_` separators → segments,
/// each capitalised).
fn camel(text: &str) -> String {
    text.replace('_', "-")
        .split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// The implementation IRI for a profile (mirrors `_impl_iri`): `gmeow:impl<Camel>`.
fn impl_iri(profile: &str) -> String {
    format!("{GMEOW}impl{}", camel(profile))
}

/// The profile's projection-query `.rq` IRI (mirrors `rdfs:seeAlso`).
fn profile_query_iri(profile: &str) -> String {
    format!("{ONTOLOGY_IRI}/queries/projections/{profile}.rq")
}

/// The SPARQL object variable an atom binds for `predicate` (mirrors
/// `_var_for_predicate`): a plain-predicate atom, or an alternation-path atom one of
/// whose alternatives is the predicate.
fn var_for_predicate(cell: &ProjectionCell, predicate: &str) -> Option<String> {
    for atom in &cell.atoms {
        let Some(obj) = &atom.object_var else {
            continue;
        };
        let matches = atom.predicate.as_deref() == Some(predicate)
            || atom.path_alts.iter().any(|a| a == predicate);
        if matches {
            return Some(obj.clone());
        }
    }
    None
}

/// The SPARQL variable a cell's output value binds to (mirrors `_output_var`):
/// `pattern.value` if set, else the last (composed) bind var.
fn output_var(cell: &ProjectionCell) -> Option<String> {
    if let Some(v) = &cell.value {
        return Some(v.clone());
    }
    cell.binds.last().cloned()
}

/// Build the [`FnoCatalog`] — the full `emit_fno` + `_emit_fnom` derivation in Rust.
fn build_catalog(
    functions: &[ProjectionFunction],
    cells: &[ProjectionCell],
    onto: &Store,
) -> Result<FnoCatalog, SliceError> {
    // ── used_by + transform_cells (the projection-cell scan) ────────────────
    // used_by: transform IRI → its profiles, FIRST-USE order (the Python list
    // append-if-absent). transform_cells: (transform, profile) → cells, in cell
    // order. Cells are scanned in store-iteration order (matches the Python
    // `for cell in dsl.projections` over the merged graph's set iteration; only the
    // resulting var SETS are used, so per-binding order within this is immaterial,
    // but profile first-use order IS load-bearing for the seeAlso choice).
    let mut used_by: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut transform_cells: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    for (idx, cell) in cells.iter().enumerate() {
        for (profile, transform) in &cell.transform_bindings {
            let profiles = used_by.entry(transform.clone()).or_default();
            if !profiles.contains(profile) {
                profiles.push(profile.clone());
            }
            transform_cells
                .entry((transform.clone(), profile.clone()))
                .or_default()
                .push(idx);
        }
    }

    // ── Functions, parameters (sorted by str(iri)) ──────────────────────────
    let mut sorted_fns: Vec<&ProjectionFunction> = functions.iter().collect();
    sorted_fns.sort_by(|a, b| a.iri.cmp(&b.iri));

    let mut catalog_fns: Vec<FnFunction> = Vec::new();
    let mut catalog_params: Vec<FnParam> = Vec::new();
    // param IRI → source predicate, the dedup + collision guard (params_emitted).
    let mut params_emitted: BTreeMap<String, String> = BTreeMap::new();

    for func in &sorted_fns {
        let profiles = used_by.get(&func.iri);
        let see_also = profile_query_iri(
            profiles
                .and_then(|ps| ps.first())
                .map(String::as_str)
                .unwrap_or(DEFAULT_PROFILE),
        );

        // expects (required first, then optional); each param derives its fno:type.
        let mut expects: Vec<String> = Vec::new();
        for (predicate, required) in func
            .inputs
            .iter()
            .map(|p| (p, true))
            .chain(func.optional_inputs.iter().map(|p| (p, false)))
        {
            let param = param_iri(predicate);
            // The fail-closed type: refuse a param whose source predicate has no
            // ontology rdfs:range.
            let Some(rng) = predicate_range(onto, predicate)? else {
                return Err(SliceError::Parse(format!(
                    "{}: input {predicate} has no rdfs:range — cannot derive its fno:type",
                    func.iri
                )));
            };
            if let Some(prior) = params_emitted.get(&param) {
                if prior != predicate {
                    return Err(SliceError::Parse(format!(
                        "param IRI collision: {param} is minted from both {prior} and {predicate}"
                    )));
                }
            }
            expects.push(param.clone());
            if !params_emitted.contains_key(&param) {
                params_emitted.insert(param.clone(), predicate.clone());
                catalog_params.push(FnParam {
                    iri: param,
                    predicate: predicate.clone(),
                    r#type: rng,
                    required,
                });
            }
        }

        catalog_fns.push(FnFunction {
            iri: func.iri.clone(),
            label: func.label.clone(),
            description: if func.description.is_empty() {
                None
            } else {
                Some(func.description.clone())
            },
            see_also,
            expects,
            output: FnOutput {
                iri: output_iri(&func.iri),
                predicate: func.output.clone(),
                r#type: func.output_type.clone(),
            },
        });
    }

    // ── fnom: implementations + mappings ────────────────────────────────────
    let mut implementations: Vec<FnImpl> = Vec::new();
    let mut impl_emitted: BTreeSet<String> = BTreeSet::new();
    let mut mappings: Vec<FnMapping> = Vec::new();

    for func in &sorted_fns {
        let out_node = output_iri(&func.iri);
        let fn_local = local(&func.iri);
        let Some(profiles) = used_by.get(&func.iri) else {
            continue;
        };
        for profile in profiles {
            let impl_node = impl_iri(profile);
            if impl_emitted.insert(profile.clone()) {
                implementations.push(FnImpl {
                    iri: impl_node.clone(),
                    format: SPARQL_FORMAT.to_owned(),
                    see_also: profile_query_iri(profile),
                });
            }

            // Aggregate every cell's parameter/output var bindings.
            let empty: Vec<usize> = Vec::new();
            let cell_idxs = transform_cells
                .get(&(func.iri.clone(), profile.clone()))
                .unwrap_or(&empty);
            // param IRI → set of vars (sorted: BTreeMap/BTreeSet).
            let mut param_vars: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            let mut out_vars: BTreeSet<String> = BTreeSet::new();
            for &idx in cell_idxs {
                let cell = &cells[idx];
                for predicate in func.inputs.iter().chain(func.optional_inputs.iter()) {
                    if let Some(var) = var_for_predicate(cell, predicate) {
                        param_vars
                            .entry(param_iri(predicate))
                            .or_default()
                            .insert(var);
                    }
                }
                if let Some(out_var) = output_var(cell) {
                    out_vars.insert(out_var);
                }
            }

            let mut parameter_mappings: Vec<FnParamMapping> = Vec::new();
            // params sorted by str(iri) (BTreeMap), vars sorted (BTreeSet).
            for (param, vars) in &param_vars {
                let param_local = local(param);
                for var in vars {
                    parameter_mappings.push(FnParamMapping {
                        bnode_label: format!("param-{fn_local}-{profile}-{param_local}-{var}"),
                        label: format!("{param_local} ↦ ?{var}"),
                        function_parameter: param.clone(),
                        implementation_property: var.clone(),
                    });
                }
            }
            let mut return_mappings: Vec<FnReturnMapping> = Vec::new();
            for var in &out_vars {
                return_mappings.push(FnReturnMapping {
                    bnode_label: format!("return-{fn_local}-{profile}-{var}"),
                    label: format!("{fn_local} output ↦ ?{var}"),
                    function_output: out_node.clone(),
                    implementation_property: var.clone(),
                });
            }

            mappings.push(FnMapping {
                bnode_label: format!("mapping-{fn_local}-{profile}"),
                label: format!("{fn_local} → {profile} (FnO mapping)"),
                function: func.iri.clone(),
                implementation: impl_node,
                parameter_mappings,
                return_mappings,
            });
        }
    }

    Ok(FnoCatalog {
        ontology_iri: ONTOLOGY_IRI.to_owned(),
        document_iri: format!("{ONTOLOGY_IRI}/projections/functions"),
        doc_label: DOC_LABEL.to_owned(),
        banner: GENERATED_BANNER.to_owned(),
        functions: catalog_fns,
        params: catalog_params,
        implementations,
        mappings,
    })
}

/// Every `rdfs:range` IRI of a predicate in the ontology store, in store-iteration
/// order (mirrors `set(onto.objects(predicate, RDFS.range))` restricted to URIRef
/// objects). `pub(crate)` — the single shared range lookup for both the emitter's
/// `fno:type` derivation and the projection lint's `fno-type` check.
pub(crate) fn predicate_ranges(store: &Store, predicate: &str) -> Result<Vec<String>, SliceError> {
    let node = NamedNode::new(predicate)
        .map_err(|e| SliceError::Parse(format!("invalid predicate IRI {predicate}: {e}")))?;
    object_iris(store, &node, RDFS_RANGE)
}

/// The `rdfs:range` IRI of a predicate in the ontology store, or `None` (mirrors
/// `onto.value(predicate, RDFS.range)` restricted to a URIRef object). The emitter
/// asserts a single `fno:type`, so it takes the first range; the lint compares
/// against the full [`predicate_ranges`] set.
fn predicate_range(store: &Store, predicate: &str) -> Result<Option<String>, SliceError> {
    Ok(predicate_ranges(store, predicate)?.into_iter().next())
}

// ── Store helpers ──────────────────────────────────────────────────────────────

/// Every named-node subject of `?s a <type_iri>` (mirrors `subjects_of_type`).
fn subjects_of_type(store: &Store, type_iri: &str) -> Result<Vec<String>, SliceError> {
    let rdf_type = NamedNode::new(RDF_TYPE)
        .map_err(|e| SliceError::Parse(format!("invalid rdf:type IRI: {e}")))?;
    let class = NamedNode::new(type_iri)
        .map_err(|e| SliceError::Parse(format!("invalid type IRI {type_iri}: {e}")))?;
    let mut subjects = Vec::new();
    for quad in store.quads_for_pattern(
        None,
        Some(rdf_type.as_ref()),
        Some(class.as_ref().into()),
        Some(GraphNameRef::DefaultGraph),
    ) {
        let quad = quad.map_err(|e| SliceError::Parse(e.to_string()))?;
        if let NamedOrBlankNode::NamedNode(nn) = &quad.subject {
            subjects.push(nn.as_str().to_owned());
        }
    }
    Ok(subjects)
}

/// The rdf:type IRIs of a (blank or named) term.
fn types_of_term(store: &Store, term: &Term) -> Result<Vec<String>, SliceError> {
    let mut out = Vec::new();
    for obj in objects_of_term(store, term, RDF_TYPE)? {
        if let Term::NamedNode(nn) = obj {
            out.push(nn.as_str().to_owned());
        }
    }
    Ok(out)
}

/// The first IRI object of `subject pred ?o`, or `None`.
fn object_iri(
    store: &Store,
    subject: &NamedNode,
    pred: &str,
) -> Result<Option<String>, SliceError> {
    match first_object(store, subject, pred)? {
        Some(Term::NamedNode(nn)) => Ok(Some(nn.as_str().to_owned())),
        _ => Ok(None),
    }
}

/// All IRI objects of `subject pred ?o`, in store-iteration order.
fn object_iris(store: &Store, subject: &NamedNode, pred: &str) -> Result<Vec<String>, SliceError> {
    let mut out = Vec::new();
    for obj in objects_of(store, subject, pred)? {
        if let Term::NamedNode(nn) = obj {
            out.push(nn.as_str().to_owned());
        }
    }
    Ok(out)
}

/// The lexical form of the first literal object of `subject pred ?o`, or `None`.
fn object_literal(
    store: &Store,
    subject: &NamedNode,
    pred: &str,
) -> Result<Option<String>, SliceError> {
    match first_object(store, subject, pred)? {
        Some(Term::Literal(lit)) => Ok(Some(lit.value().to_owned())),
        _ => Ok(None),
    }
}

/// The first object term of `subject pred ?o` in the default graph.
fn first_object(
    store: &Store,
    subject: &NamedNode,
    pred: &str,
) -> Result<Option<Term>, SliceError> {
    let predicate = NamedNode::new(pred)
        .map_err(|e| SliceError::Parse(format!("invalid predicate IRI {pred}: {e}")))?;
    match store
        .quads_for_pattern(
            Some(subject.as_ref().into()),
            Some(predicate.as_ref()),
            None,
            Some(GraphNameRef::DefaultGraph),
        )
        .next()
    {
        Some(quad) => Ok(Some(
            quad.map_err(|e| SliceError::Parse(e.to_string()))?.object,
        )),
        None => Ok(None),
    }
}

/// All object terms of `subject pred ?o` in the default graph.
fn objects_of(store: &Store, subject: &NamedNode, pred: &str) -> Result<Vec<Term>, SliceError> {
    let predicate = NamedNode::new(pred)
        .map_err(|e| SliceError::Parse(format!("invalid predicate IRI {pred}: {e}")))?;
    let mut out = Vec::new();
    for quad in store.quads_for_pattern(
        Some(subject.as_ref().into()),
        Some(predicate.as_ref()),
        None,
        Some(GraphNameRef::DefaultGraph),
    ) {
        out.push(quad.map_err(|e| SliceError::Parse(e.to_string()))?.object);
    }
    Ok(out)
}

// The term-subject variants (a pattern/atom/bind/expr subject may be a blank node).

fn term_subject(term: &Term) -> Option<NamedOrBlankNode> {
    match term {
        Term::NamedNode(nn) => Some(NamedOrBlankNode::NamedNode(nn.clone())),
        Term::BlankNode(bn) => Some(NamedOrBlankNode::BlankNode(bn.clone())),
        _ => None,
    }
}

fn first_object_of_term(
    store: &Store,
    subject: &Term,
    pred: &str,
) -> Result<Option<Term>, SliceError> {
    let Some(subj) = term_subject(subject) else {
        return Ok(None);
    };
    let predicate = NamedNode::new(pred)
        .map_err(|e| SliceError::Parse(format!("invalid predicate IRI {pred}: {e}")))?;
    match store
        .quads_for_pattern(
            Some(subj.as_ref()),
            Some(predicate.as_ref()),
            None,
            Some(GraphNameRef::DefaultGraph),
        )
        .next()
    {
        Some(quad) => Ok(Some(
            quad.map_err(|e| SliceError::Parse(e.to_string()))?.object,
        )),
        None => Ok(None),
    }
}

fn objects_of_term(store: &Store, subject: &Term, pred: &str) -> Result<Vec<Term>, SliceError> {
    let Some(subj) = term_subject(subject) else {
        return Ok(Vec::new());
    };
    let predicate = NamedNode::new(pred)
        .map_err(|e| SliceError::Parse(format!("invalid predicate IRI {pred}: {e}")))?;
    let mut out = Vec::new();
    for quad in store.quads_for_pattern(
        Some(subj.as_ref()),
        Some(predicate.as_ref()),
        None,
        Some(GraphNameRef::DefaultGraph),
    ) {
        out.push(quad.map_err(|e| SliceError::Parse(e.to_string()))?.object);
    }
    Ok(out)
}

fn object_iri_of_term(
    store: &Store,
    subject: &Term,
    pred: &str,
) -> Result<Option<String>, SliceError> {
    match first_object_of_term(store, subject, pred)? {
        Some(Term::NamedNode(nn)) => Ok(Some(nn.as_str().to_owned())),
        _ => Ok(None),
    }
}

fn object_literal_of_term(
    store: &Store,
    subject: &Term,
    pred: &str,
) -> Result<Option<String>, SliceError> {
    match first_object_of_term(store, subject, pred)? {
        Some(Term::Literal(lit)) => Ok(Some(lit.value().to_owned())),
        _ => Ok(None),
    }
}

/// The members of an rdf:List head node (empty if `head` is `None`).
fn rdf_list(store: &Store, head: Option<&Term>) -> Result<Vec<Term>, SliceError> {
    let mut out: Vec<Term> = Vec::new();
    let mut node = head.cloned();
    while let Some(cur) = node {
        // rdf:nil terminates the list.
        if let Term::NamedNode(nn) = &cur {
            if nn.as_str() == RDF_NIL {
                break;
            }
        }
        if let Some(first) = first_object_of_term(store, &cur, RDF_FIRST)? {
            out.push(first);
        }
        node = first_object_of_term(store, &cur, RDF_REST)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `_local`, `_param_iri`, `_output_iri`, `_camel`, `_impl_iri` mirror the
    /// Python helpers exactly.
    #[test]
    fn iri_minting_matches_python() {
        assert_eq!(
            local("https://blackcatinformatics.ca/gmeow/fullName"),
            "fullName"
        );
        assert_eq!(
            param_iri("https://blackcatinformatics.ca/gmeow/addressLocality"),
            "https://blackcatinformatics.ca/gmeow/paramAddressLocality"
        );
        assert_eq!(
            output_iri("https://blackcatinformatics.ca/gmeow/fnBirthEventToDate"),
            "https://blackcatinformatics.ca/gmeow/outBirthEventToDate"
        );
        // A function local name without the leading `fn` keeps its whole stem.
        assert_eq!(
            output_iri("https://blackcatinformatics.ca/gmeow/composeAddress"),
            "https://blackcatinformatics.ca/gmeow/outcomposeAddress"
        );
        assert_eq!(camel("schema-org"), "SchemaOrg");
        assert_eq!(camel("owl_time"), "OwlTime");
        assert_eq!(camel("foaf"), "Foaf");
        assert_eq!(
            impl_iri("schema-org"),
            "https://blackcatinformatics.ca/gmeow/implSchemaOrg"
        );
    }

    /// `_order_binds`: a dependent bind sorts after the bind it references; the
    /// terminal (composed) bind is therefore last (the `_output_var` fallback).
    #[test]
    fn order_binds_is_dependency_then_alpha() {
        let mut a_refs = BTreeSet::new();
        a_refs.insert("raw".to_owned());
        let binds = vec![
            Bind {
                var: "label".to_owned(),
                refs: {
                    let mut s = BTreeSet::new();
                    s.insert("a".to_owned());
                    s.insert("b".to_owned());
                    s
                },
            },
            Bind {
                var: "a".to_owned(),
                refs: BTreeSet::new(),
            },
            Bind {
                var: "b".to_owned(),
                refs: BTreeSet::new(),
            },
        ];
        let ordered = order_binds(binds);
        // a, b (independent, alpha) then label (depends on both).
        assert_eq!(ordered, vec!["a", "b", "label"]);
        assert_eq!(ordered.last().map(String::as_str), Some("label"));
    }

    /// `_var_for_predicate`: plain predicate match, alternation-path member match,
    /// and the no-object-var skip.
    #[test]
    fn var_for_predicate_matches_plain_and_alt() {
        let cell = ProjectionCell {
            atoms: vec![
                Atom {
                    predicate: Some(format!("{GMEOW}hasName")),
                    path_alts: vec![],
                    object_var: None, // no object var → skipped
                },
                Atom {
                    predicate: None,
                    path_alts: vec![format!("{GMEOW}hasAppellation"), format!("{GMEOW}hasName")],
                    object_var: Some("app".to_owned()),
                },
                Atom {
                    predicate: Some(format!("{GMEOW}eventTime")),
                    path_alts: vec![],
                    object_var: Some("t".to_owned()),
                },
            ],
            value: None,
            binds: vec![],
            transform_bindings: vec![],
        };
        // The alternation atom binds ?app for hasAppellation.
        assert_eq!(
            var_for_predicate(&cell, &format!("{GMEOW}hasAppellation")),
            Some("app".to_owned())
        );
        // The plain atom binds ?t for eventTime.
        assert_eq!(
            var_for_predicate(&cell, &format!("{GMEOW}eventTime")),
            Some("t".to_owned())
        );
        // A predicate bound only by an atom with no object var yields nothing.
        assert_eq!(var_for_predicate(&cell, &format!("{GMEOW}unbound")), None);
    }

    /// `_output_var`: pattern.value wins; else the last bind var; else None.
    #[test]
    fn output_var_prefers_value_then_last_bind() {
        let with_value = ProjectionCell {
            atoms: vec![],
            value: Some("v".to_owned()),
            binds: vec!["a".to_owned(), "b".to_owned()],
            transform_bindings: vec![],
        };
        assert_eq!(output_var(&with_value), Some("v".to_owned()));
        let with_binds = ProjectionCell {
            atoms: vec![],
            value: None,
            binds: vec!["a".to_owned(), "wkt".to_owned()],
            transform_bindings: vec![],
        };
        assert_eq!(output_var(&with_binds), Some("wkt".to_owned()));
        let empty = ProjectionCell {
            atoms: vec![],
            value: None,
            binds: vec![],
            transform_bindings: vec![],
        };
        assert_eq!(output_var(&empty), None);
    }

    /// End-to-end derivation against a synthetic two-source set: a function whose
    /// input predicate has an ontology range, a projection cell that uses it via a
    /// transform binding, and the resulting catalog's function/param/mapping shape.
    #[test]
    fn build_catalog_derives_function_param_and_mapping() {
        let dsl = new_store().unwrap();
        let dsl_ttl = format!(
            r#"
@prefix gmeow: <{GMEOW}> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix skos: <http://www.w3.org/2004/02/skos/core#> .

gmeow:fnDemo a gmeow:ProjectionFunction ;
    rdfs:label "demo"@x-gmeow-english ;
    skos:definition "a demo function"@x-gmeow-english ;
    gmeow:fnInput gmeow:foo ;
    gmeow:fnOutput gmeow:bar ;
    gmeow:fnOutputType <http://www.w3.org/2001/XMLSchema#string> .

gmeow:cellDemo a gmeow:ProjectionMapping ;
    gmeow:hasMappingPattern [
        gmeow:anchor "x" ;
        gmeow:value "fooVal" ;
        gmeow:atom (
            [ gmeow:subjectVar "x" ; gmeow:predicate gmeow:foo ; gmeow:objectVar "fooVal" ]
        ) ;
    ] ;
    gmeow:hasBinding [
        gmeow:profile "schema-org" ;
        gmeow:transform gmeow:fnDemo ;
    ] .
"#
        );
        load_into_store(&dsl, dsl_ttl.as_bytes(), Path::new("synthetic.ttl")).unwrap();

        let onto = new_store().unwrap();
        let onto_ttl = format!(
            r#"
@prefix gmeow: <{GMEOW}> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
gmeow:foo rdfs:range <http://www.w3.org/2001/XMLSchema#string> .
"#
        );
        load_into_store(&onto, onto_ttl.as_bytes(), Path::new("onto.ttl")).unwrap();

        let functions = extract_functions(&dsl).unwrap();
        let cells = extract_cells(&dsl).unwrap();
        assert_eq!(functions.len(), 1);
        assert_eq!(cells.len(), 1);

        let catalog = build_catalog(&functions, &cells, &onto).unwrap();
        assert_eq!(catalog.functions.len(), 1);
        let f = &catalog.functions[0];
        assert_eq!(f.iri, format!("{GMEOW}fnDemo"));
        assert_eq!(f.expects, vec![format!("{GMEOW}paramFoo")]);
        assert_eq!(f.output.iri, format!("{GMEOW}outDemo"));
        // The seeAlso is the using profile's .rq (NOT the schema-org default).
        assert_eq!(f.see_also, profile_query_iri("schema-org"));

        // One param, typed from the ontology range.
        assert_eq!(catalog.params.len(), 1);
        assert_eq!(catalog.params[0].iri, format!("{GMEOW}paramFoo"));
        assert_eq!(
            catalog.params[0].r#type,
            "http://www.w3.org/2001/XMLSchema#string"
        );
        assert!(catalog.params[0].required);

        // One implementation + one mapping with a param + return mapping.
        assert_eq!(catalog.implementations.len(), 1);
        assert_eq!(catalog.mappings.len(), 1);
        let m = &catalog.mappings[0];
        assert_eq!(m.function, format!("{GMEOW}fnDemo"));
        assert_eq!(m.parameter_mappings.len(), 1);
        assert_eq!(m.parameter_mappings[0].implementation_property, "fooVal");
        assert_eq!(m.return_mappings.len(), 1);
        assert_eq!(m.return_mappings[0].implementation_property, "fooVal");
    }

    /// The fail-closed guard: an input predicate with no ontology range is a hard
    /// error (the type-mismatch bug class is structurally impossible).
    #[test]
    fn untyped_input_predicate_is_a_hard_error() {
        let dsl = new_store().unwrap();
        let dsl_ttl = format!(
            r#"
@prefix gmeow: <{GMEOW}> .
gmeow:fnDemo a gmeow:ProjectionFunction ;
    gmeow:fnInput gmeow:untyped ;
    gmeow:fnOutput gmeow:bar ;
    gmeow:fnOutputType <http://www.w3.org/2001/XMLSchema#string> .
"#
        );
        load_into_store(&dsl, dsl_ttl.as_bytes(), Path::new("synthetic.ttl")).unwrap();
        let onto = new_store().unwrap(); // empty — no range anywhere

        let functions = extract_functions(&dsl).unwrap();
        let cells = extract_cells(&dsl).unwrap();
        let err = build_catalog(&functions, &cells, &onto).unwrap_err();
        assert!(
            matches!(err, SliceError::Parse(ref m) if m.contains("no rdfs:range")),
            "expected a fail-closed range error, got {err:?}"
        );
    }

    /// Two predicates minting the same param IRI from different sources are
    /// rejected, never silently merged.
    #[test]
    fn param_iri_collision_is_a_hard_error() {
        let dsl = new_store().unwrap();
        // `gmeow:placeType` and `gmeow:PlaceType` both mint `gmeow:paramPlaceType`.
        let dsl_ttl = format!(
            r#"
@prefix gmeow: <{GMEOW}> .
gmeow:fnA a gmeow:ProjectionFunction ;
    gmeow:fnInput gmeow:placeType ;
    gmeow:fnOutput gmeow:outA ;
    gmeow:fnOutputType <http://www.w3.org/2000/01/rdf-schema#Literal> .
gmeow:fnB a gmeow:ProjectionFunction ;
    gmeow:fnInput gmeow:PlaceType ;
    gmeow:fnOutput gmeow:outB ;
    gmeow:fnOutputType <http://www.w3.org/2000/01/rdf-schema#Literal> .
"#
        );
        load_into_store(&dsl, dsl_ttl.as_bytes(), Path::new("synthetic.ttl")).unwrap();
        let onto = new_store().unwrap();
        let onto_ttl = format!(
            r#"
@prefix gmeow: <{GMEOW}> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
gmeow:placeType rdfs:range gmeow:PlaceType .
gmeow:PlaceType rdfs:range gmeow:PlaceType .
"#
        );
        load_into_store(&onto, onto_ttl.as_bytes(), Path::new("onto.ttl")).unwrap();

        let functions = extract_functions(&dsl).unwrap();
        let cells = extract_cells(&dsl).unwrap();
        let err = build_catalog(&functions, &cells, &onto).unwrap_err();
        assert!(
            matches!(err, SliceError::Parse(ref m) if m.contains("param IRI collision")),
            "expected a param-IRI collision error, got {err:?}"
        );
    }
}
