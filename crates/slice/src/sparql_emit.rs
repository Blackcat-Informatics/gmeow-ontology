// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native SPARQL-projection emission — GMEOW's `generated/queries/<profile>.rq`
//! emitter, sourced entirely from Rust (#861).
//!
//! This mirrors the SSSOM ([`crate::mapping_emit`]) and FnO ([`crate::fno_emit`])
//! emitters: the Python `mapping_compile.emit_sparql` orchestrator (plus the
//! `mapping_dsl` model + renderer it consumes) is pulled into the slice framework.
//! Every input is discovered natively from the repo root — the projection cells from
//! the shared `dsl/mappings/**/*.ttl` tree + the slice [`ArtifactRole::Mapping`]
//! artifacts, and the suppression vocabulary from `ontology/gmeow.ttl` + the slice
//! [`ArtifactRole::Module`] artifacts.
//!
//! The emitted `.rq` text is **byte-identical** to the historical Python emitter (the
//! parity gate). This module also owns the shared DSL model + parser + renderer that
//! [`crate::edoal_emit`] reuses (`pub(crate)`), so the two artifact emitters parse the
//! DSL once each from the same source-collection ordering as `fno_emit`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::{GraphNameRef, NamedNode, NamedOrBlankNode, Term};
pub(crate) use oxigraph::store::Store;

use crate::artifact::ArtifactRole;
use crate::catalog::SliceCatalog;
use crate::error::SliceError;
use crate::mapping_emit::PREFIX_REGISTRY;

// ── Namespace constants ───────────────────────────────────────────────────────

#[cfg(test)]
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

// gmeow predicates the parser reads (full IRIs derived from the Python `GM.<name>`).
const GM_PROJECTION_MAPPING: &str = "https://blackcatinformatics.ca/gmeow/ProjectionMapping";
const GM_HAS_MAPPING_PATTERN: &str = "https://blackcatinformatics.ca/gmeow/hasMappingPattern";
const GM_HAS_BINDING: &str = "https://blackcatinformatics.ca/gmeow/hasBinding";
const GM_ANCHOR: &str = "https://blackcatinformatics.ca/gmeow/anchor";
const GM_VALUE: &str = "https://blackcatinformatics.ca/gmeow/value";
const GM_ATOM: &str = "https://blackcatinformatics.ca/gmeow/atom";
const GM_OPTIONAL_GROUP: &str = "https://blackcatinformatics.ca/gmeow/optionalGroup";
const GM_SUPPRESS_WHEN: &str = "https://blackcatinformatics.ca/gmeow/suppressWhen";
const GM_PROJECT_WHEN: &str = "https://blackcatinformatics.ca/gmeow/projectWhen";
const GM_EXCLUDE_WHEN: &str = "https://blackcatinformatics.ca/gmeow/excludeWhen";
const GM_FILTER: &str = "https://blackcatinformatics.ca/gmeow/filter";
const GM_BIND: &str = "https://blackcatinformatics.ca/gmeow/bind";
const GM_MINT: &str = "https://blackcatinformatics.ca/gmeow/mint";
const GM_BIND_VAR: &str = "https://blackcatinformatics.ca/gmeow/bindVar";
const GM_BIND_EXPR: &str = "https://blackcatinformatics.ca/gmeow/bindExpr";
const GM_EXPR_VAR: &str = "https://blackcatinformatics.ca/gmeow/exprVar";
const GM_EXPR_OP: &str = "https://blackcatinformatics.ca/gmeow/exprOp";
const GM_EXPR_ARGS: &str = "https://blackcatinformatics.ca/gmeow/exprArgs";
const GM_EDOAL_SOURCE: &str = "https://blackcatinformatics.ca/gmeow/edoalSource";
const GM_EDOAL_SOURCE_KIND: &str = "https://blackcatinformatics.ca/gmeow/edoalSourceKind";
const GM_EDOAL_PATH: &str = "https://blackcatinformatics.ca/gmeow/edoalPath";

const GM_SUBJECT_VAR: &str = "https://blackcatinformatics.ca/gmeow/subjectVar";
const GM_T_SUBJ: &str = "https://blackcatinformatics.ca/gmeow/tSubj";
const GM_PREDICATE: &str = "https://blackcatinformatics.ca/gmeow/predicate";
const GM_T_PRED: &str = "https://blackcatinformatics.ca/gmeow/tPred";
const GM_PREDICATE_VAR: &str = "https://blackcatinformatics.ca/gmeow/predicateVar";
const GM_PATH: &str = "https://blackcatinformatics.ca/gmeow/path";
const GM_PATH_ALTS: &str = "https://blackcatinformatics.ca/gmeow/pathAlts";
const GM_PATH_STEPS: &str = "https://blackcatinformatics.ca/gmeow/pathSteps";
const GM_PATH_STEP: &str = "https://blackcatinformatics.ca/gmeow/pathStep";
const GM_PATH_SET: &str = "https://blackcatinformatics.ca/gmeow/pathSet";
const GM_ALT_PATH: &str = "https://blackcatinformatics.ca/gmeow/AltPath";
const GM_SEQ_PATH: &str = "https://blackcatinformatics.ca/gmeow/SeqPath";
const GM_INVERSE_PATH: &str = "https://blackcatinformatics.ca/gmeow/InversePath";
const GM_ZERO_OR_MORE_PATH: &str = "https://blackcatinformatics.ca/gmeow/ZeroOrMorePath";
const GM_ONE_OR_MORE_PATH: &str = "https://blackcatinformatics.ca/gmeow/OneOrMorePath";
const GM_ZERO_OR_ONE_PATH: &str = "https://blackcatinformatics.ca/gmeow/ZeroOrOnePath";
const GM_NEGATED_PROPERTY_SET: &str = "https://blackcatinformatics.ca/gmeow/NegatedPropertySet";
const GM_OBJECT_VAR: &str = "https://blackcatinformatics.ca/gmeow/objectVar";
const GM_T_OBJ: &str = "https://blackcatinformatics.ca/gmeow/tObj";
const GM_OBJECT_VALUE: &str = "https://blackcatinformatics.ca/gmeow/objectValue";
const GM_T_OBJ_VALUE: &str = "https://blackcatinformatics.ca/gmeow/tObjValue";
const GM_OBJECT_LITERAL: &str = "https://blackcatinformatics.ca/gmeow/objectLiteral";
const GM_OPTIONAL: &str = "https://blackcatinformatics.ca/gmeow/optional";

const GM_PROFILE: &str = "https://blackcatinformatics.ca/gmeow/profile";
const GM_TO_PREDICATE: &str = "https://blackcatinformatics.ca/gmeow/toPredicate";
const GM_TO_CLASS: &str = "https://blackcatinformatics.ca/gmeow/toClass";
const GM_TEMPLATE_ATOMS: &str = "https://blackcatinformatics.ca/gmeow/templateAtoms";
const GM_VALUE_CLASS_MAP: &str = "https://blackcatinformatics.ca/gmeow/valueClassMap";
const GM_WHEN_VALUE: &str = "https://blackcatinformatics.ca/gmeow/whenValue";
const GM_RELATION: &str = "https://blackcatinformatics.ca/gmeow/relation";
const GM_TRANSFORM: &str = "https://blackcatinformatics.ca/gmeow/transform";
const GM_CONFIDENCE: &str = "https://blackcatinformatics.ca/gmeow/confidence";
const GM_LOSSY_DROP: &str = "https://blackcatinformatics.ca/gmeow/lossyDrop";
const GM_EDOAL_TARGET: &str = "https://blackcatinformatics.ca/gmeow/edoalTarget";
const GM_EDOAL_TARGET_KIND: &str = "https://blackcatinformatics.ca/gmeow/edoalTargetKind";

// suppression-vocab + injected-guard predicates
const GM_APPELLATION: &str = "https://blackcatinformatics.ca/gmeow/Appellation";
const GM_COARSEN_GUARDED: &str = "https://blackcatinformatics.ca/gmeow/coarsenGuarded";
const GM_DISPLAYABLE: &str = "https://blackcatinformatics.ca/gmeow/displayable";
const GM_COARSEN_TO: &str = "https://blackcatinformatics.ca/gmeow/coarsenTo";

// language-retag predicates
const GM_FULL_NAME: &str = "https://blackcatinformatics.ca/gmeow/fullName";
const GM_PART_TEXT: &str = "https://blackcatinformatics.ca/gmeow/partText";
const GM_PART_EXPANSION: &str = "https://blackcatinformatics.ca/gmeow/partExpansion";
const GM_ROMANIZATION: &str = "https://blackcatinformatics.ca/gmeow/romanization";
const GM_DESCRIPTION: &str = "https://blackcatinformatics.ca/gmeow/description";
const GM_DESIGN_GOAL: &str = "https://blackcatinformatics.ca/gmeow/designGoal";
const GM_TITLE: &str = "https://blackcatinformatics.ca/gmeow/title";
const GM_SLOGAN: &str = "https://blackcatinformatics.ca/gmeow/slogan";
const GM_NAME_LANGUAGE: &str = "https://blackcatinformatics.ca/gmeow/nameLanguage";
const GM_HAS_NAME_PART: &str = "https://blackcatinformatics.ca/gmeow/hasNamePart";
const GM_BCP47_TAG: &str = "https://blackcatinformatics.ca/gmeow/bcp47Tag";

pub(crate) const GENERATED_BANNER: &str =
    "GENERATED by `gmeow regenerate` (mappings) from mapping-dsl/ — DO NOT EDIT.";

/// The 45 projection profiles, in `mapping_compile._PROFILES` order.
const PROFILES: &[&str] = &[
    "schema-org",
    "vcard",
    "foaf",
    "geosparql",
    "qb",
    "ical",
    "jcal",
    "schema-org-schedule",
    "owl-time",
    "odrl",
    "cc",
    "dcterms",
    "oai_dc",
    "spdx",
    "ontolex",
    "web-annotation",
    "skos",
    "activitystreams",
    "markdown",
    "bot",
    "sosa",
    "crmarchaeo",
    "ivoa",
    "iptc",
    "loinc",
    "slsa",
    "intoto",
    "sigstore",
    "mailmap",
    "iiif",
    "exif",
    "doap",
    "codemeta",
    "resume",
    "dcat",
    "org",
    "bibo",
    "bibframe",
    "gedcom",
    "sioc",
    "prov",
    "lrmoo",
    "mo",
    "pon",
    "jams",
];

// ── DSL model (shared with edoal_emit) ───────────────────────────────────────────

/// An expression-algebra node (mirrors `mapping_dsl.Expr`).
#[derive(Debug, Clone)]
pub(crate) enum Expr {
    Var(String),
    /// A constant URIRef (full IRI).
    ConstIri(String),
    /// A constant literal (its lexical form).
    ConstLiteral(String),
    /// An operator application.
    Op {
        op: String,
        args: Vec<Expr>,
    },
}

/// One graph-pattern (or template) atom (mirrors `mapping_dsl.Atom`).
#[derive(Debug, Clone)]
pub(crate) struct Atom {
    pub subject_var: String,
    pub predicate: Option<String>,
    pub predicate_var: Option<String>,
    /// Pre-rendered SPARQL property path.
    pub path: Option<String>,
    /// Alternatives, when path is a top-level AltPath of plain predicates.
    pub path_alts: Vec<String>,
    pub object_var: Option<String>,
    pub object_value: Option<String>,
    /// `(lexical, datatype)` of a literal object.
    pub object_literal: Option<(String, Option<String>)>,
    pub optional: bool,
}

/// A pattern item: a flat atom or a nested OPTIONAL group.
#[derive(Debug, Clone)]
pub(crate) enum Item {
    Atom(Atom),
    Group(Vec<Item>),
}

/// A derived binding (`BIND expr AS ?var`) (mirrors `mapping_dsl.Bind`).
#[derive(Debug, Clone)]
pub(crate) struct Bind {
    pub var: String,
    pub expr: Expr,
}

/// One value→class table entry (mirrors `mapping_dsl.ValueClass`).
#[derive(Debug, Clone)]
pub(crate) struct ValueClass {
    pub when_value: String,
    pub to_class: String,
}

/// The GMEOW-side pattern of a projection mapping (mirrors `MappingPattern`).
#[derive(Debug, Clone)]
pub(crate) struct MappingPattern {
    pub anchor: String,
    pub value: Option<String>,
    pub atoms: Vec<Item>,
    pub suppress_when: Vec<Atom>,
    pub project_when: Vec<Atom>,
    pub exclude_when: Vec<Atom>,
    pub filters: Vec<Expr>,
    pub binds: Vec<Bind>,
    pub mints: Vec<Bind>,
    pub edoal_source: Option<String>,
    pub edoal_source_kind: String,
    pub edoal_path: bool,
}

impl MappingPattern {
    /// Flatten the pattern items (recursing OPTIONAL groups) to bare atoms.
    pub(crate) fn flat_atoms(&self) -> Vec<Atom> {
        let mut out = Vec::new();
        flatten_items(&self.atoms, &mut out);
        out
    }
}

fn flatten_items(items: &[Item], out: &mut Vec<Atom>) {
    for item in items {
        match item {
            Item::Group(inner) => flatten_items(inner, out),
            Item::Atom(a) => out.push(a.clone()),
        }
    }
}

/// A per-profile output face of a projection mapping (mirrors `ProfileBinding`).
#[derive(Debug, Clone)]
pub(crate) struct ProfileBinding {
    pub profile: String,
    pub to_predicate: Option<String>,
    pub to_class: Option<String>,
    pub template_atoms: Vec<Atom>,
    pub value_class_map: Vec<ValueClass>,
    pub relation: String,
    pub transform: Option<String>,
    pub confidence: Option<f64>,
    pub lossy_drops: Vec<String>,
    pub edoal_target: Option<String>,
    pub edoal_target_kind: Option<String>,
}

/// A projection mapping: a pattern + its per-profile bindings.
#[derive(Debug, Clone)]
pub(crate) struct ProjectionCell {
    pub iri: String,
    pub label: String,
    pub pattern: MappingPattern,
    pub bindings: Vec<ProfileBinding>,
}

/// The fully parsed projection layer of the DSL.
pub(crate) struct Dsl {
    pub projections: Vec<ProjectionCell>,
}

// ── Source collection (mirrors fno_emit's exact ordering) ────────────────────────

/// Build the merged DSL store: the shared `dsl/mappings/**/*.ttl` tree + the slice
/// `mappings/*.ttl` artifacts (the `load_dsl` source set, sorted-path insertion).
pub(crate) fn collect_dsl_store(root: &Path) -> Result<Store, SliceError> {
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

/// Build the merged ontology store: `ontology/gmeow.ttl` + every slice
/// [`ArtifactRole::Module`] artifact (the `load_merged_graph(include_imports=False)`
/// source set).
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

fn load_into_store(store: &Store, bytes: &[u8], path: &Path) -> Result<(), SliceError> {
    for quad in RdfParser::from_format(RdfFormat::Turtle)
        .lenient()
        .for_reader(bytes)
    {
        let quad = quad
            .map_err(|e| SliceError::Parse(format!("syntax error in {}: {e}", path.display())))?;
        store
            .insert(&quad)
            .map_err(|e| SliceError::Parse(format!("store insert failed: {e}")))?;
    }
    Ok(())
}

// ── DSL parsing (mirrors mapping_dsl._projections / _pattern / _binding / …) ──────

/// Parse every `gmeow:ProjectionMapping` (mirrors `_projections`).
pub(crate) fn projections(store: &Store) -> Result<Vec<ProjectionCell>, SliceError> {
    let mut cells: Vec<ProjectionCell> = Vec::new();
    for cell_iri in subjects_of_type(store, GM_PROJECTION_MAPPING)? {
        let cell = NamedNode::new(&cell_iri)
            .map_err(|e| SliceError::Parse(format!("invalid mapping IRI {cell_iri}: {e}")))?;
        let Some(pattern_node) = first_object(store, &cell, GM_HAS_MAPPING_PATTERN)? else {
            return Err(SliceError::Parse(format!(
                "projection mapping {cell_iri} missing hasMappingPattern"
            )));
        };
        let pattern = parse_pattern(store, &pattern_node)?;
        let mut bindings: Vec<ProfileBinding> = Vec::new();
        for binding_node in objects_of(store, &cell, GM_HAS_BINDING)? {
            bindings.push(parse_binding(store, &binding_node)?);
        }
        if bindings.is_empty() {
            return Err(SliceError::Parse(format!(
                "projection mapping {cell_iri} has no bindings"
            )));
        }
        let label = object_literal(store, &cell, RDFS_LABEL)?.unwrap_or_default();
        cells.push(ProjectionCell {
            iri: cell_iri,
            label,
            pattern,
            bindings,
        });
    }
    Ok(cells)
}

fn parse_pattern(store: &Store, node: &Term) -> Result<MappingPattern, SliceError> {
    let Some(anchor) = object_literal_of_term(store, node, GM_ANCHOR)? else {
        return Err(SliceError::Parse(
            "mapping pattern missing anchor".to_owned(),
        ));
    };
    let value = object_literal_of_term(store, node, GM_VALUE)?;

    let atom_head = first_object_of_term(store, node, GM_ATOM)?;
    let mut atoms: Vec<Item> = Vec::new();
    for item in rdf_list(store, atom_head.as_ref())? {
        atoms.push(parse_item(store, &item)?);
    }

    let mut suppress_when: Vec<Atom> = Vec::new();
    for a in objects_of_term(store, node, GM_SUPPRESS_WHEN)? {
        suppress_when.push(parse_atom(store, &a)?);
    }
    suppress_when.sort_by_key(atom_key);

    let mut project_when: Vec<Atom> = Vec::new();
    for a in objects_of_term(store, node, GM_PROJECT_WHEN)? {
        project_when.push(parse_atom(store, &a)?);
    }
    project_when.sort_by_key(atom_key);

    let mut exclude_when: Vec<Atom> = Vec::new();
    for a in objects_of_term(store, node, GM_EXCLUDE_WHEN)? {
        exclude_when.push(parse_atom(store, &a)?);
    }
    exclude_when.sort_by_key(atom_key);

    let mut filters: Vec<Expr> = Vec::new();
    for f in objects_of_term(store, node, GM_FILTER)? {
        filters.push(parse_expr(store, &f)?);
    }
    filters.sort_by_key(render_expr);

    let mut raw_binds: Vec<Bind> = Vec::new();
    for b in objects_of_term(store, node, GM_BIND)? {
        raw_binds.push(parse_bind(store, &b)?);
    }
    let binds = order_binds(raw_binds)?;

    let mut raw_mints: Vec<Bind> = Vec::new();
    for m in objects_of_term(store, node, GM_MINT)? {
        raw_mints.push(parse_bind(store, &m)?);
    }
    let mints = order_binds(raw_mints)?;

    Ok(MappingPattern {
        anchor,
        value,
        atoms,
        suppress_when,
        project_when,
        exclude_when,
        filters,
        binds,
        mints,
        edoal_source: object_iri_of_term(store, node, GM_EDOAL_SOURCE)?,
        edoal_source_kind: object_literal_of_term(store, node, GM_EDOAL_SOURCE_KIND)?
            .unwrap_or_else(|| "relation".to_owned()),
        edoal_path: object_bool_of_term(store, node, GM_EDOAL_PATH)?,
    })
}

fn parse_item(store: &Store, node: &Term) -> Result<Item, SliceError> {
    if let Some(group_head) = first_object_of_term(store, node, GM_OPTIONAL_GROUP)? {
        let mut inner: Vec<Item> = Vec::new();
        for item in rdf_list(store, Some(&group_head))? {
            inner.push(parse_item(store, &item)?);
        }
        return Ok(Item::Group(inner));
    }
    Ok(Item::Atom(parse_atom(store, node)?))
}

fn parse_atom(store: &Store, node: &Term) -> Result<Atom, SliceError> {
    let subj = object_literal_of_term(store, node, GM_SUBJECT_VAR)?
        .or(object_literal_of_term(store, node, GM_T_SUBJ)?);
    let Some(subject_var) = subj else {
        return Err(SliceError::Parse(
            "atom missing subjectVar/tSubj".to_owned(),
        ));
    };
    let predicate = match object_iri_of_term(store, node, GM_PREDICATE)? {
        Some(p) => Some(p),
        None => object_iri_of_term(store, node, GM_T_PRED)?,
    };
    let predicate_var = object_literal_of_term(store, node, GM_PREDICATE_VAR)?;
    let path_node = first_object_of_term(store, node, GM_PATH)?;
    let path = match &path_node {
        Some(p) => Some(render_path(store, p)?),
        None => None,
    };
    let path_alts = match &path_node {
        Some(p) => alt_members(store, p)?,
        None => Vec::new(),
    };
    let object_var = object_literal_of_term(store, node, GM_OBJECT_VAR)?
        .or(object_literal_of_term(store, node, GM_T_OBJ)?);
    let object_value = match object_iri_of_term(store, node, GM_OBJECT_VALUE)? {
        Some(v) => Some(v),
        None => object_iri_of_term(store, node, GM_T_OBJ_VALUE)?,
    };
    let object_literal = literal_of_term(store, node, GM_OBJECT_LITERAL)?;
    let optional = object_bool_of_term(store, node, GM_OPTIONAL)?;
    Ok(Atom {
        subject_var,
        predicate,
        predicate_var,
        path,
        path_alts,
        object_var,
        object_value,
        object_literal,
        optional,
    })
}

fn parse_bind(store: &Store, node: &Term) -> Result<Bind, SliceError> {
    let Some(var) = object_literal_of_term(store, node, GM_BIND_VAR)? else {
        return Err(SliceError::Parse("bind/mint missing bindVar".to_owned()));
    };
    let Some(expr_node) = first_object_of_term(store, node, GM_BIND_EXPR)? else {
        return Err(SliceError::Parse("bind/mint missing bindExpr".to_owned()));
    };
    Ok(Bind {
        var,
        expr: parse_expr(store, &expr_node)?,
    })
}

fn parse_expr(store: &Store, node: &Term) -> Result<Expr, SliceError> {
    match node {
        Term::NamedNode(nn) => return Ok(Expr::ConstIri(nn.as_str().to_owned())),
        Term::Literal(lit) => return Ok(Expr::ConstLiteral(lit.value().to_owned())),
        _ => {}
    }
    if let Some(var) = object_literal_of_term(store, node, GM_EXPR_VAR)? {
        return Ok(Expr::Var(var));
    }
    let Some(op) = object_iri_of_term(store, node, GM_EXPR_OP)? else {
        return Err(SliceError::Parse(
            "expression node has neither exprVar nor exprOp".to_owned(),
        ));
    };
    let args_head = first_object_of_term(store, node, GM_EXPR_ARGS)?;
    let mut args: Vec<Expr> = Vec::new();
    for a in rdf_list(store, args_head.as_ref())? {
        args.push(parse_expr(store, &a)?);
    }
    Ok(Expr::Op { op, args })
}

fn parse_binding(store: &Store, node: &Term) -> Result<ProfileBinding, SliceError> {
    let Some(profile) = object_literal_of_term(store, node, GM_PROFILE)? else {
        return Err(SliceError::Parse(
            "profile binding missing profile".to_owned(),
        ));
    };
    let mut template_atoms: Vec<Atom> = Vec::new();
    let ta_head = first_object_of_term(store, node, GM_TEMPLATE_ATOMS)?;
    for a in rdf_list(store, ta_head.as_ref())? {
        template_atoms.push(parse_atom(store, &a)?);
    }
    let vcm_head = first_object_of_term(store, node, GM_VALUE_CLASS_MAP)?;
    let mut value_class_map: Vec<ValueClass> = Vec::new();
    for entry in rdf_list(store, vcm_head.as_ref())? {
        let (Some(when), Some(to_class)) = (
            object_iri_of_term(store, &entry, GM_WHEN_VALUE)?,
            object_iri_of_term(store, &entry, GM_TO_CLASS)?,
        ) else {
            return Err(SliceError::Parse("value-class entry malformed".to_owned()));
        };
        value_class_map.push(ValueClass {
            when_value: when,
            to_class,
        });
    }
    let relation =
        object_literal_of_term(store, node, GM_RELATION)?.unwrap_or_else(|| "=".to_owned());
    let confidence = match object_literal_of_term(store, node, GM_CONFIDENCE)? {
        Some(text) => Some(text.parse::<f64>().map_err(|_| {
            SliceError::Parse("profile binding has non-numeric confidence".to_owned())
        })?),
        None => None,
    };
    let mut lossy_drops: Vec<String> = Vec::new();
    for d in objects_of_term(store, node, GM_LOSSY_DROP)? {
        if let Some(text) = term_lexical(&d) {
            lossy_drops.push(text);
        } else if let Some(iri) = term_iri(&d) {
            lossy_drops.push(iri);
        }
    }
    Ok(ProfileBinding {
        profile,
        to_predicate: object_iri_of_term(store, node, GM_TO_PREDICATE)?,
        to_class: object_iri_of_term(store, node, GM_TO_CLASS)?,
        template_atoms,
        value_class_map,
        relation,
        transform: object_iri_of_term(store, node, GM_TRANSFORM)?,
        confidence,
        lossy_drops,
        edoal_target: object_iri_of_term(store, node, GM_EDOAL_TARGET)?,
        edoal_target_kind: object_literal_of_term(store, node, GM_EDOAL_TARGET_KIND)?,
    })
}

// ── Property-path rendering (mirrors mapping_dsl._render_path) ────────────────────

fn render_path(store: &Store, node: &Term) -> Result<String, SliceError> {
    if let Term::NamedNode(nn) = node {
        let iri = nn.as_str();
        if iri == RDF_TYPE {
            return Ok("rdf:type".to_owned());
        }
        return Ok(curie(iri));
    }
    let types = types_of_term(store, node)?;
    if types.iter().any(|t| t == GM_ALT_PATH) {
        let head = first_object_of_term(store, node, GM_PATH_ALTS)?;
        let alts = rdf_list(store, head.as_ref())?;
        let mut parts: Vec<String> = Vec::new();
        for a in &alts {
            parts.push(render_path(store, a)?);
        }
        return Ok(parts.join("|"));
    }
    if types.iter().any(|t| t == GM_SEQ_PATH) {
        let head = first_object_of_term(store, node, GM_PATH_STEPS)?;
        let steps = rdf_list(store, head.as_ref())?;
        let mut parts: Vec<String> = Vec::new();
        for s in &steps {
            parts.push(render_path(store, s)?);
        }
        return Ok(parts.join("/"));
    }
    if types.iter().any(|t| t == GM_INVERSE_PATH) {
        let step = first_object_of_term(store, node, GM_PATH_STEP)?;
        return Ok(format!("^{}", path_primary(store, step.as_ref())?));
    }
    if types.iter().any(|t| t == GM_ZERO_OR_MORE_PATH) {
        let step = first_object_of_term(store, node, GM_PATH_STEP)?;
        return Ok(format!("{}*", path_primary(store, step.as_ref())?));
    }
    if types.iter().any(|t| t == GM_ONE_OR_MORE_PATH) {
        let step = first_object_of_term(store, node, GM_PATH_STEP)?;
        return Ok(format!("{}+", path_primary(store, step.as_ref())?));
    }
    if types.iter().any(|t| t == GM_ZERO_OR_ONE_PATH) {
        let step = first_object_of_term(store, node, GM_PATH_STEP)?;
        return Ok(format!("{}?", path_primary(store, step.as_ref())?));
    }
    if types.iter().any(|t| t == GM_NEGATED_PROPERTY_SET) {
        let head = first_object_of_term(store, node, GM_PATH_SET)?;
        let members = rdf_list(store, head.as_ref())?;
        let mut parts: Vec<String> = Vec::new();
        for m in &members {
            parts.push(render_path(store, m)?);
        }
        let inner = parts.join("|");
        return Ok(if members.len() > 1 {
            format!("!({inner})")
        } else {
            format!("!{inner}")
        });
    }
    Err(SliceError::Parse("unknown property-path node".to_owned()))
}

fn path_primary(store: &Store, node: Option<&Term>) -> Result<String, SliceError> {
    let Some(node) = node else {
        return Err(SliceError::Parse("property path missing a step".to_owned()));
    };
    let rendered = render_path(store, node)?;
    if rendered.contains('/') || rendered.contains('|') {
        Ok(format!("({rendered})"))
    } else {
        Ok(rendered)
    }
}

/// `_alt_members`: a top-level AltPath of plain predicates → them, else ().
fn alt_members(store: &Store, node: &Term) -> Result<Vec<String>, SliceError> {
    if !matches!(node, Term::BlankNode(_)) {
        return Ok(Vec::new());
    }
    let types = types_of_term(store, node)?;
    if !types.iter().any(|t| t == GM_ALT_PATH) {
        return Ok(Vec::new());
    }
    let head = first_object_of_term(store, node, GM_PATH_ALTS)?;
    let members = rdf_list(store, head.as_ref())?;
    let mut alts: Vec<String> = Vec::new();
    for m in &members {
        match m {
            Term::NamedNode(nn) => alts.push(nn.as_str().to_owned()),
            _ => return Ok(Vec::new()),
        }
    }
    Ok(alts)
}

// ── Expression rendering (mirrors mapping_dsl.render_expr) ────────────────────────

/// Function-call operators (`opX` local name → `NAME`).
fn func_op(name: &str) -> Option<&'static str> {
    Some(match name {
        "opConcat" => "CONCAT",
        "opCoalesce" => "COALESCE",
        "opIf" => "IF",
        "opBound" => "BOUND",
        "opStr" => "STR",
        "opIri" => "IRI",
        "opStrDatatype" => "STRDT",
        "opLang" => "LANG",
        "opLangMatches" => "LANGMATCHES",
        "opStrLang" => "STRLANG",
        "opDatatype" => "DATATYPE",
        "opSubstr" => "SUBSTR",
        "opReplace" => "REPLACE",
        "opUcase" => "UCASE",
        "opLcase" => "LCASE",
        "opStrBefore" => "STRBEFORE",
        "opStrAfter" => "STRAFTER",
        "opStrLen" => "STRLEN",
        "opContains" => "CONTAINS",
        "opStrStarts" => "STRSTARTS",
        "opStrEnds" => "STRENDS",
        "opEncodeForUri" => "ENCODE_FOR_URI",
        "opDecimal" => "xsd:decimal",
        _ => return None,
    })
}

/// Infix operators (`opX` local name → `OP`).
fn infix_op(name: &str) -> Option<&'static str> {
    Some(match name {
        "opAdd" => "+",
        "opSub" => "-",
        "opMul" => "*",
        "opDiv" => "/",
        "opEq" => "=",
        "opNe" => "!=",
        "opLt" => "<",
        "opGt" => ">",
        "opLe" => "<=",
        "opGe" => ">=",
        "opAnd" => "&&",
        "opOr" => "||",
        _ => return None,
    })
}

pub(crate) fn render_expr(expr: &Expr) -> String {
    match expr {
        Expr::Var(v) => format!("?{v}"),
        Expr::ConstIri(iri) => curie(iri),
        Expr::ConstLiteral(text) => sparql_string(text),
        Expr::Op { op, args } => {
            let name = op_local(op);
            let rendered: Vec<String> = args.iter().map(render_expr).collect();
            if name == "opRegex" {
                return format!("regex({})", rendered.join(", "));
            }
            if name == "opNot" {
                return format!("(!{})", rendered[0]);
            }
            if name == "opIn" {
                return format!("({} IN ({}))", rendered[0], rendered[1..].join(", "));
            }
            if let Some(sym) = infix_op(&name) {
                return format!("({})", rendered.join(&format!(" {sym} ")));
            }
            if let Some(fn_name) = func_op(&name) {
                return format!("{fn_name}({})", rendered.join(", "));
            }
            // Unknown operator: Python raises CompileError. Mirror by producing a
            // sentinel that cannot match committed bytes (parity test would catch it).
            format!("UNKNOWN_OP({})", rendered.join(", "))
        }
    }
}

/// The local name of an op IRI (after last `/` then last `#`), mirroring
/// `str(op).rsplit("/",1)[-1].rsplit("#",1)[-1]`.
fn op_local(iri: &str) -> String {
    let after_slash = iri.rsplit_once('/').map(|(_, b)| b).unwrap_or(iri);
    after_slash
        .rsplit_once('#')
        .map(|(_, b)| b)
        .unwrap_or(after_slash)
        .to_owned()
}

fn expr_vars(expr: &Expr, out: &mut BTreeSet<String>) {
    match expr {
        Expr::Var(v) => {
            out.insert(v.clone());
        }
        Expr::Op { args, .. } => {
            for a in args {
                expr_vars(a, out);
            }
        }
        _ => {}
    }
}

// ── Determinism helpers (mirrors _atom_key / _order_binds) ───────────────────────

/// `_atom_key`: a stable content-only ordering key for a guard atom.
fn atom_key(atom: &Atom) -> Vec<String> {
    let obj_lit = match &atom.object_literal {
        Some((lex, dt)) => render_literal_str(lex, dt.as_deref()),
        None => String::new(),
    };
    vec![
        atom.subject_var.clone(),
        atom.predicate.clone().unwrap_or_default(),
        atom.predicate_var.clone().unwrap_or_default(),
        atom.path.clone().unwrap_or_default(),
        atom.path_alts.join("|"),
        atom.object_var.clone().unwrap_or_default(),
        atom.object_value.clone().unwrap_or_default(),
        obj_lit,
        bool_str(atom.optional),
    ]
}

/// Python's `str(optional)` → "True"/"False".
fn bool_str(b: bool) -> String {
    if b { "True" } else { "False" }.to_owned()
}

/// Python's `str(Literal)` for a literal in `_atom_key` — the lexical form (rdflib's
/// `str()` of a Literal is its lexical value, datatype/lang ignored).
fn render_literal_str(lexical: &str, _datatype: Option<&str>) -> String {
    lexical.to_owned()
}

/// `_order_binds`: dependency order with an alphabetical tiebreak. Fails closed on a
/// duplicate or cyclic variable.
fn order_binds(binds: Vec<Bind>) -> Result<Vec<Bind>, SliceError> {
    let mut by_var: BTreeMap<String, Bind> = BTreeMap::new();
    for b in binds {
        if by_var.contains_key(&b.var) {
            return Err(SliceError::Parse(format!(
                "duplicate BIND/mint variable ?{}",
                b.var
            )));
        }
        by_var.insert(b.var.clone(), b);
    }
    let own: BTreeSet<String> = by_var.keys().cloned().collect();
    let mut deps: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (var, b) in &by_var {
        let mut vars: BTreeSet<String> = BTreeSet::new();
        expr_vars(&b.expr, &mut vars);
        let d: BTreeSet<String> = vars
            .into_iter()
            .filter(|v| own.contains(v) && v != var)
            .collect();
        deps.insert(var.clone(), d);
    }
    let mut placed: BTreeSet<String> = BTreeSet::new();
    let mut remaining: BTreeSet<String> = own.clone();
    let mut ordered: Vec<Bind> = Vec::new();
    while !remaining.is_empty() {
        let ready: Vec<String> = remaining
            .iter()
            .filter(|v| deps[*v].is_subset(&placed))
            .cloned()
            .collect();
        if ready.is_empty() {
            let cycle: Vec<String> = remaining.iter().map(|v| format!("?{v}")).collect();
            return Err(SliceError::Parse(format!(
                "cyclic BIND/mint dependency among {}",
                cycle.join(", ")
            )));
        }
        for var in &ready {
            ordered.push(by_var[var].clone());
            placed.insert(var.clone());
        }
        for var in &ready {
            remaining.remove(var);
        }
    }
    Ok(ordered)
}

// ── CURIE shortening (mirrors mapping_dsl.curie) ─────────────────────────────────

thread_local! {
    static NS_TO_PREFIX: Vec<(String, String)> = {
        // Stable sort by descending namespace length, registry order as tiebreak.
        let mut pairs: Vec<(String, String)> = PREFIX_REGISTRY
            .iter()
            .map(|(p, ns)| ((*ns).to_owned(), (*p).to_owned()))
            .collect();
        pairs.sort_by_key(|pair| std::cmp::Reverse(pair.0.len()));
        pairs
    };
}

/// Shorten an IRI to `prefix:local` via the canonical registry, else `<iri>`.
pub(crate) fn curie(iri: &str) -> String {
    NS_TO_PREFIX.with(|table| {
        for (ns, prefix) in table {
            if let Some(local) = iri.strip_prefix(ns.as_str()) {
                return format!("{prefix}:{local}");
            }
        }
        format!("<{iri}>")
    })
}

/// The local name of an IRI (after the last `#` or `/`), mirroring `_local`.
pub(crate) fn local(iri: &str) -> String {
    let cut = iri.rfind(['#', '/']).map(|i| i + 1).unwrap_or(0);
    iri[cut..].to_owned()
}

/// Render a Python string as a single-line SPARQL string literal (mirrors
/// `sparql_string`).
fn sparql_string(text: &str) -> String {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    format!("\"{escaped}\"")
}

// ── Suppression vocabulary (mirrors suppression_vocab) ───────────────────────────

struct SuppressionVocab {
    /// Sorted properties whose range ⊑ gmeow:Appellation.
    bearer_props: Vec<String>,
    appellation_domain_props: BTreeSet<String>,
    appellation_classes: BTreeSet<String>,
    coarsen_guarded: BTreeSet<String>,
}

fn suppression_vocab(onto: &Store) -> Result<SuppressionVocab, SliceError> {
    let classes = subclass_closure(onto, GM_APPELLATION)?;
    let mut bearer: BTreeSet<String> = BTreeSet::new();
    for (prop, rng) in subject_objects(onto, RDFS_RANGE)? {
        if let (Some(prop), Some(rng)) = (term_iri(&prop), term_iri(&rng)) {
            if classes.contains(&rng) {
                bearer.insert(prop);
            }
        }
    }
    let mut domain_props: BTreeSet<String> = BTreeSet::new();
    for (prop, dom) in subject_objects(onto, RDFS_DOMAIN)? {
        if let (Some(prop), Some(dom)) = (term_iri(&prop), term_iri(&dom)) {
            if classes.contains(&dom) {
                domain_props.insert(prop);
            }
        }
    }
    let mut coarsen: BTreeSet<String> = BTreeSet::new();
    for (s, o) in subject_objects(onto, GM_COARSEN_GUARDED)? {
        if let Some(s) = term_iri(&s) {
            if term_lexical(&o).as_deref() == Some("true") {
                coarsen.insert(s);
            }
        }
    }
    Ok(SuppressionVocab {
        bearer_props: bearer.into_iter().collect(),
        appellation_domain_props: domain_props,
        appellation_classes: classes,
        coarsen_guarded: coarsen,
    })
}

/// `root` plus every class transitively rdfs:subClassOf it (mirrors
/// `_subclass_closure`).
fn subclass_closure(store: &Store, root: &str) -> Result<BTreeSet<String>, SliceError> {
    let mut closure: BTreeSet<String> = BTreeSet::new();
    closure.insert(root.to_owned());
    let edges = subject_objects(store, RDFS_SUBCLASS_OF)?;
    loop {
        let mut grew = false;
        for (sub, sup) in &edges {
            if let (Some(sub), Some(sup)) = (term_iri(sub), term_iri(sup)) {
                if closure.contains(&sup) && !closure.contains(&sub) {
                    closure.insert(sub);
                    grew = true;
                }
            }
        }
        if !grew {
            break;
        }
    }
    Ok(closure)
}

// ── SPARQL emission (mirrors emit_sparql + helpers) ──────────────────────────────

/// Emit every SPARQL projection query from the repo at `root`, returning
/// `{ "<profile>.rq" → rq_text }` for all 45 profiles.
///
/// All inputs are sourced natively from `root` (the DSL tree + slice mapping
/// artifacts for the cells; `ontology/gmeow.ttl` + slice module artifacts for the
/// suppression vocabulary). The text is byte-identical to the historical Python
/// emitter.
///
/// # Errors
///
/// Returns [`SliceError`] on any missing/unparsable required source, a malformed
/// cell, or a profile with no bindings — no degraded fallback.
pub fn emit_sparql_sets(root: &Path) -> Result<BTreeMap<String, String>, SliceError> {
    let dsl_store = collect_dsl_store(root)?;
    let dsl = Dsl {
        projections: projections(&dsl_store)?,
    };
    let onto_store = collect_ontology_store(root)?;
    let vocab = suppression_vocab(&onto_store)?;

    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for profile in PROFILES {
        let text = emit_sparql(&dsl, profile, &vocab)?;
        out.insert(format!("{profile}.rq"), text);
    }
    Ok(out)
}

fn emit_sparql(dsl: &Dsl, profile: &str, vocab: &SuppressionVocab) -> Result<String, SliceError> {
    let mut templates: Vec<String> = Vec::new();
    let mut branches: Vec<String> = Vec::new();
    let mut drops: Vec<String> = Vec::new();
    let mut seen_branches: BTreeSet<String> = BTreeSet::new();
    for cell in &dsl.projections {
        for b in &cell.bindings {
            if b.profile != profile {
                continue;
            }
            for tmpl in templates_of(cell, b)? {
                if !templates.contains(&tmpl) {
                    templates.push(tmpl);
                }
            }
            let branch = branch_of(cell, b, vocab)?;
            if seen_branches.insert(branch.clone()) {
                branches.push(branch);
            }
            for d in &b.lossy_drops {
                if !drops.contains(d) {
                    drops.push(d.clone());
                }
            }
        }
    }
    if branches.is_empty() {
        return Err(SliceError::Parse(format!(
            "no bindings for profile {profile:?}"
        )));
    }
    let construct = templates
        .iter()
        .map(|t| format!("    {t}"))
        .collect::<Vec<_>>()
        .join("\n");
    let where_clause = branches
        .iter()
        .enumerate()
        .map(|(i, b)| {
            if i == 0 {
                b.clone()
            } else {
                format!("UNION {b}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n    ");
    let body = format!("CONSTRUCT {{\n{construct}\n}}\nWHERE {{\n    {where_clause}\n}}\n");
    let drops_part = if drops.is_empty() {
        ".".to_owned()
    } else {
        format!("; drops: {}.", drops.join("; "))
    };
    let header = format!(
        "# Projection: GMEOW → pure {profile}. {GENERATED_BANNER}\n# Lossy and directional by design{drops_part}\n"
    );
    let prefixes = prefix_block(&body);
    Ok(format!("{header}{prefixes}\n\n{body}"))
}

/// `_class_var`.
fn class_var(p: &MappingPattern) -> String {
    format!("{}Class", p.value.clone().unwrap_or_default())
}

/// `_term`.
fn term_of(atom: &Atom, var_map: &BTreeMap<String, String>) -> Result<String, SliceError> {
    if let Some(obj_var) = &atom.object_var {
        let v = var_map
            .get(obj_var)
            .cloned()
            .unwrap_or_else(|| obj_var.clone());
        return Ok(format!("?{v}"));
    }
    if let Some(val) = &atom.object_value {
        return Ok(curie(val));
    }
    if let Some((lex, dt)) = &atom.object_literal {
        if dt.as_deref() == Some(XSD_BOOLEAN) {
            let lower = lex.to_lowercase();
            return Ok(if lower == "true" || lower == "1" {
                "true".to_owned()
            } else {
                "false".to_owned()
            });
        }
        return Ok(sparql_string(lex));
    }
    Err(SliceError::Parse("atom has no object".to_owned()))
}

/// `_atom_triple`.
fn atom_triple(atom: &Atom, var_map: &BTreeMap<String, String>) -> Result<String, SliceError> {
    let subj_var = var_map
        .get(&atom.subject_var)
        .cloned()
        .unwrap_or_else(|| atom.subject_var.clone());
    let subj = format!("?{subj_var}");
    let pred = if let Some(path) = &atom.path {
        path.clone()
    } else if let Some(pv) = &atom.predicate_var {
        format!("?{pv}")
    } else if atom.predicate.as_deref() == Some(RDF_TYPE) && atom.object_value.is_some() {
        "a".to_owned()
    } else if let Some(p) = &atom.predicate {
        curie(p)
    } else {
        return Err(SliceError::Parse(format!(
            "atom on ?{} has no predicate/path",
            atom.subject_var
        )));
    };
    Ok(format!("{subj} {pred} {} .", term_of(atom, var_map)?))
}

/// `_where_items`.
fn where_items(items: &[Item], indent: &str) -> Result<Vec<String>, SliceError> {
    let empty: BTreeMap<String, String> = BTreeMap::new();
    let mut out: Vec<String> = Vec::new();
    for item in items {
        match item {
            Item::Group(inner) => {
                out.push(format!("{indent}OPTIONAL {{"));
                out.extend(where_items(inner, &format!("{indent}    "))?);
                out.push(format!("{indent}}}"));
            }
            Item::Atom(atom) => {
                let triple = atom_triple(atom, &empty)?;
                let wrapped = if atom.optional {
                    format!("OPTIONAL {{ {triple} }}")
                } else {
                    triple
                };
                out.push(format!("{indent}{wrapped}"));
            }
        }
    }
    Ok(out)
}

/// `_suppression_anchors`.
fn suppression_anchors(p: &MappingPattern) -> Vec<String> {
    let mut anchors: Vec<String> = Vec::new();
    for item in &p.atoms {
        match item {
            Item::Group(_) => continue,
            Item::Atom(atom) => {
                if atom.optional {
                    continue;
                }
                if !anchors.contains(&atom.subject_var) {
                    anchors.push(atom.subject_var.clone());
                }
            }
        }
    }
    anchors
}

/// `_required_atoms`.
fn required_atoms(p: &MappingPattern) -> Vec<Atom> {
    let mut out: Vec<Atom> = Vec::new();
    for item in &p.atoms {
        if let Item::Atom(atom) = item {
            if !atom.optional {
                out.push(atom.clone());
            }
        }
    }
    out
}

/// `_injected_guards`.
fn injected_guards(p: &MappingPattern, vocab: &SuppressionVocab) -> Vec<String> {
    let required = required_atoms(p);
    let mut guards: Vec<String> = Vec::new();

    let mut displayable_authored: BTreeSet<String> = BTreeSet::new();
    for atom in required
        .iter()
        .chain(p.suppress_when.iter())
        .chain(p.project_when.iter())
    {
        if atom.predicate.as_deref() == Some(GM_DISPLAYABLE) {
            displayable_authored.insert(atom.subject_var.clone());
        }
    }
    for var in suppression_anchors(p) {
        if displayable_authored.contains(&var) {
            continue;
        }
        guards.push(format!(
            "FILTER NOT EXISTS {{ ?{var} gmeow:displayable false . }}"
        ));
    }
    let coarsen_suppressed: BTreeSet<String> = p
        .suppress_when
        .iter()
        .filter(|a| a.predicate.as_deref() == Some(GM_COARSEN_TO))
        .map(|a| a.subject_var.clone())
        .collect();
    for atom in &required {
        if let Some(pred) = &atom.predicate {
            if vocab.coarsen_guarded.contains(pred) {
                if coarsen_suppressed.contains(&atom.subject_var) {
                    continue;
                }
                let guard = format!(
                    "FILTER NOT EXISTS {{ ?{} gmeow:coarsenTo [] . }}",
                    atom.subject_var
                );
                if !guards.contains(&guard) {
                    guards.push(guard);
                }
            }
        }
    }
    let bearer_set: BTreeSet<&String> = vocab.bearer_props.iter().collect();
    let mut bearer_visible: BTreeSet<String> = BTreeSet::new();
    for atom in &required {
        let pred_bearer = atom
            .predicate
            .as_ref()
            .map(|p| bearer_set.contains(p))
            .unwrap_or(false);
        let alt_bearer =
            !atom.path_alts.is_empty() && atom.path_alts.iter().any(|a| bearer_set.contains(a));
        if pred_bearer || alt_bearer {
            if let Some(obj) = &atom.object_var {
                bearer_visible.insert(obj.clone());
            }
        }
    }
    let mut appellation_vars: Vec<String> = Vec::new();
    for atom in &required {
        let is_appellation = atom
            .predicate
            .as_ref()
            .map(|p| vocab.appellation_domain_props.contains(p))
            .unwrap_or(false)
            || (atom.predicate.as_deref() == Some(RDF_TYPE)
                && atom
                    .object_value
                    .as_ref()
                    .map(|o| vocab.appellation_classes.contains(o))
                    .unwrap_or(false));
        if is_appellation
            && !bearer_visible.contains(&atom.subject_var)
            && !appellation_vars.contains(&atom.subject_var)
        {
            appellation_vars.push(atom.subject_var.clone());
        }
    }
    if !appellation_vars.is_empty() && !vocab.bearer_props.is_empty() {
        let alternation = vocab
            .bearer_props
            .iter()
            .map(|p| curie(p))
            .collect::<Vec<_>>()
            .join("|");
        for var in &appellation_vars {
            guards.push(format!(
                "FILTER NOT EXISTS {{ ?_supBearer {alternation} ?{var} . ?_supBearer gmeow:displayable false . }}"
            ));
        }
    }
    guards
}

/// `_branch`.
fn branch_of(
    cell: &ProjectionCell,
    b: &ProfileBinding,
    vocab: &SuppressionVocab,
) -> Result<String, SliceError> {
    let p = &cell.pattern;
    let empty: BTreeMap<String, String> = BTreeMap::new();
    let mut lines: Vec<String> = where_items(&p.atoms, "")?;

    if !b.value_class_map.is_empty() {
        let cv = class_var(p);
        let val = p.value.clone().unwrap_or_default();
        lines.push(format!("VALUES ( ?{val} ?{cv} ) {{"));
        for vc in &b.value_class_map {
            lines.push(format!(
                "    ( {} {} )",
                curie(&vc.when_value),
                curie(&vc.to_class)
            ));
        }
        lines.push("}".to_owned());
    }
    for mint in &p.mints {
        lines.push(format!(
            "BIND ( {} AS ?{} )",
            render_expr(&mint.expr),
            mint.var
        ));
    }
    for bind in &p.binds {
        lines.push(format!(
            "BIND ( {} AS ?{} )",
            render_expr(&bind.expr),
            bind.var
        ));
    }
    let mut authored_guards: BTreeSet<String> = BTreeSet::new();
    for atom in p.suppress_when.iter().chain(p.exclude_when.iter()) {
        let guard = format!("FILTER NOT EXISTS {{ {} }}", atom_triple(atom, &empty)?);
        authored_guards.insert(guard.clone());
        lines.push(guard);
    }
    for guard in injected_guards(p, vocab) {
        if !authored_guards.contains(&guard) {
            lines.push(guard);
        }
    }
    for atom in &p.project_when {
        lines.push(format!(
            "FILTER EXISTS {{ {} }}",
            atom_triple(atom, &empty)?
        ));
    }
    for flt in &p.filters {
        lines.push(format!("FILTER( {} )", render_expr(flt)));
    }

    // Language-retag block.
    if let (Some(retag_lines), Some(bind_expr)) = language_retag(p) {
        lines.extend(retag_lines);
        let val = p.value.clone().unwrap_or_default();
        lines.push(format!("BIND ( {bind_expr} AS ?_final_{val} )"));
    }

    let body = lines
        .iter()
        .map(|ln| format!("        {ln}"))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(format!("{{\n{body}\n    }}"))
}

/// The language-retag block (mirrors lines 795-905 of mapping_compile). Returns
/// `(retag_lines, bind_expr)` when both are set.
fn language_retag(p: &MappingPattern) -> (Option<Vec<String>>, Option<String>) {
    let Some(val) = &p.value else {
        return (None, None);
    };
    let flat = p.flat_atoms();
    let src = p.edoal_source.as_deref();

    if src == Some(GM_FULL_NAME) {
        let mut parent_var: Option<String> = None;
        for a in &flat {
            if a.predicate.as_deref() == Some(GM_FULL_NAME) {
                parent_var = Some(a.subject_var.clone());
                break;
            }
        }
        let Some(parent_var) = parent_var else {
            return (None, None);
        };
        let mut lang_var = "_lang".to_owned();
        for a in &flat {
            if a.predicate.as_deref() == Some(GM_NAME_LANGUAGE) && a.subject_var == parent_var {
                if let Some(ov) = &a.object_var {
                    lang_var = ov.clone();
                }
                break;
            }
        }
        let mut ext_tag_var = "_extTag".to_owned();
        for a in &flat {
            if a.predicate.as_deref() == Some(GM_BCP47_TAG) && a.subject_var == lang_var {
                if let Some(ov) = &a.object_var {
                    ext_tag_var = ov.clone();
                }
                break;
            }
        }
        let retag_lines = vec![
            "OPTIONAL {".to_owned(),
            format!("    ?{parent_var} gmeow:nameLanguage ?{lang_var} ."),
            format!("    ?{lang_var} gmeow:languageTag ?_intTag ."),
            format!("    FILTER(isLiteral(?{val}) && LANG(?{val}) = ?_intTag)"),
            format!("    ?{lang_var} gmeow:bcp47Tag ?{ext_tag_var} ."),
            format!("    OPTIONAL {{ ?{parent_var} gmeow:nameScript ?_sc . }}"),
            "}".to_owned(),
        ];
        let bind_expr = format!(
            "IF(BOUND(?{ext_tag_var}), STRLANG(STR(?{val}), IF(BOUND(?_sc), CONCAT(STR(?{ext_tag_var}), '-', ?_sc), STR(?{ext_tag_var}))), ?{val})"
        );
        return (Some(retag_lines), Some(bind_expr));
    }

    if src == Some(GM_PART_TEXT) || src == Some(GM_PART_EXPANSION) || src == Some(GM_ROMANIZATION) {
        let mut np_var: Option<String> = None;
        for a in &flat {
            if matches!(
                a.predicate.as_deref(),
                Some(GM_PART_TEXT) | Some(GM_PART_EXPANSION) | Some(GM_ROMANIZATION)
            ) {
                np_var = Some(a.subject_var.clone());
                break;
            }
        }
        let mut app_var: Option<String> = None;
        if let Some(np) = &np_var {
            for a in &flat {
                if a.predicate.as_deref() == Some(GM_HAS_NAME_PART)
                    && a.object_var.as_deref() == Some(np.as_str())
                {
                    app_var = Some(a.subject_var.clone());
                    break;
                }
            }
            if app_var.is_none() {
                for a in &flat {
                    if a.object_var.as_deref() == Some(np.as_str()) {
                        app_var = Some(a.subject_var.clone());
                        break;
                    }
                }
            }
        }
        if let (Some(app_var), Some(_np_var)) = (&app_var, &np_var) {
            let retag_lines = vec![
                "OPTIONAL {".to_owned(),
                format!("    ?{app_var} gmeow:nameLanguage ?_lang ."),
                "    ?_lang gmeow:languageTag ?_intTag .".to_owned(),
                format!("    FILTER(isLiteral(?{val}) && LANG(?{val}) = ?_intTag)"),
                "    ?_lang gmeow:bcp47Tag ?_extTag .".to_owned(),
                format!("    OPTIONAL {{ ?{app_var} gmeow:nameScript ?_sc . }}"),
                "}".to_owned(),
            ];
            let bind_expr = format!(
                "IF(BOUND(?_extTag), STRLANG(STR(?{val}), IF(BOUND(?_sc), CONCAT(STR(?_extTag), '-', ?_sc), STR(?_extTag))), ?{val})"
            );
            return (Some(retag_lines), Some(bind_expr));
        }
        return (None, None);
    }

    if matches!(
        src,
        Some(GM_DESCRIPTION) | Some(GM_DESIGN_GOAL) | Some(GM_TITLE) | Some(GM_SLOGAN)
    ) {
        let retag_lines = vec![
            "OPTIONAL {".to_owned(),
            "    ?_lang gmeow:languageTag ?_intTag .".to_owned(),
            format!("    FILTER(isLiteral(?{val}) && LANG(?{val}) = ?_intTag)"),
            "    ?_lang gmeow:bcp47Tag ?_extTag .".to_owned(),
            "}".to_owned(),
        ];
        let bind_expr = format!("IF(BOUND(?_extTag), STRLANG(STR(?{val}), STR(?_extTag)), ?{val})");
        return (Some(retag_lines), Some(bind_expr));
    }

    // Generic fallback for standard annotation predicates.
    let annotation = flat.iter().any(|a| {
        matches!(
            a.predicate.as_deref(),
            Some(RDFS_LABEL) | Some(SKOS_DEFINITION) | Some(RDFS_COMMENT)
        ) && a.object_var.as_deref() == Some(val.as_str())
    });
    if annotation {
        let retag_lines = vec![
            "OPTIONAL {".to_owned(),
            "    ?_lang gmeow:languageTag ?_intTag .".to_owned(),
            format!("    FILTER(isLiteral(?{val}) && LANG(?{val}) = ?_intTag)"),
            "    ?_lang gmeow:bcp47Tag ?_extTag .".to_owned(),
            "}".to_owned(),
        ];
        let bind_expr = format!("IF(BOUND(?_extTag), STRLANG(STR(?{val}), STR(?_extTag)), ?{val})");
        return (Some(retag_lines), Some(bind_expr));
    }

    (None, None)
}

/// `_templates`.
fn templates_of(cell: &ProjectionCell, b: &ProfileBinding) -> Result<Vec<String>, SliceError> {
    let p = &cell.pattern;
    let mut var_map: BTreeMap<String, String> = BTreeMap::new();
    let lang_sources = [
        GM_FULL_NAME,
        GM_PART_TEXT,
        GM_PART_EXPANSION,
        GM_ROMANIZATION,
        GM_DESCRIPTION,
        GM_DESIGN_GOAL,
        GM_TITLE,
        GM_SLOGAN,
    ];
    if let Some(val) = &p.value {
        let src_is_lang = p
            .edoal_source
            .as_ref()
            .map(|s| lang_sources.contains(&s.as_str()))
            .unwrap_or(false);
        if src_is_lang {
            var_map.insert(val.clone(), format!("_final_{val}"));
        } else {
            let annotation = p.flat_atoms().iter().any(|a| {
                matches!(
                    a.predicate.as_deref(),
                    Some(RDFS_LABEL) | Some(SKOS_DEFINITION) | Some(RDFS_COMMENT)
                ) && a.object_var.as_deref() == Some(val.as_str())
            });
            if annotation {
                var_map.insert(val.clone(), format!("_final_{val}"));
            }
        }
    }

    if !b.template_atoms.is_empty() {
        let mut out: Vec<String> = Vec::new();
        for a in &b.template_atoms {
            out.push(atom_triple(a, &var_map)?);
        }
        return Ok(out);
    }
    if !b.value_class_map.is_empty() {
        return Ok(vec![format!("?{} a ?{} .", p.anchor, class_var(p))]);
    }
    if let Some(to_class) = &b.to_class {
        return Ok(vec![format!("?{} a {} .", p.anchor, curie(to_class))]);
    }
    if let Some(to_predicate) = &b.to_predicate {
        let raw = p.value.clone().unwrap_or_default();
        let val = var_map.get(&raw).cloned().unwrap_or(raw);
        return Ok(vec![format!(
            "?{} {} ?{val} .",
            p.anchor,
            curie(to_predicate)
        )]);
    }
    Err(SliceError::Parse(format!(
        "{}: binding for {} has no output",
        cell.iri, b.profile
    )))
}

/// `_prefix_block`.
pub(crate) fn prefix_block(text: &str) -> String {
    // Strip `<...>` IRIs first.
    let search_text = strip_iris(text);
    let mut lines: Vec<String> = Vec::new();
    for (prefix, ns) in PREFIX_REGISTRY {
        if has_prefix_token(&search_text, prefix) {
            lines.push(format!("PREFIX {prefix}: <{ns}>"));
        }
    }
    lines.join("\n")
}

/// Remove every `<...>` span (mirrors `re.sub(r"<[^>]*>", "", text)`).
fn strip_iris(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_iri = false;
    for c in text.chars() {
        if in_iri {
            if c == '>' {
                in_iri = false;
            }
            continue;
        }
        if c == '<' {
            in_iri = true;
            continue;
        }
        out.push(c);
    }
    out
}

/// Whether `text` contains `prefix:` at a position not preceded by a word char
/// (mirrors `re.search(rf"(?<!\w){prefix}:", text)`). `\w` = `[A-Za-z0-9_]`.
fn has_prefix_token(text: &str, prefix: &str) -> bool {
    let needle = format!("{prefix}:");
    let bytes = text.as_bytes();
    let nbytes = needle.as_bytes();
    let mut i = 0usize;
    while i + nbytes.len() <= bytes.len() {
        if &bytes[i..i + nbytes.len()] == nbytes {
            let ok = if i == 0 {
                true
            } else {
                let prev = bytes[i - 1];
                !(prev.is_ascii_alphanumeric() || prev == b'_')
            };
            if ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

// ── Store helpers (mirrors fno_emit) ─────────────────────────────────────────────

pub(crate) fn subjects_of_type(store: &Store, type_iri: &str) -> Result<Vec<String>, SliceError> {
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

fn types_of_term(store: &Store, term: &Term) -> Result<Vec<String>, SliceError> {
    let mut out = Vec::new();
    for obj in objects_of_term(store, term, RDF_TYPE)? {
        if let Term::NamedNode(nn) = obj {
            out.push(nn.as_str().to_owned());
        }
    }
    Ok(out)
}

pub(crate) fn term_iri(term: &Term) -> Option<String> {
    match term {
        Term::NamedNode(nn) => Some(nn.as_str().to_owned()),
        _ => None,
    }
}

pub(crate) fn term_lexical(term: &Term) -> Option<String> {
    match term {
        Term::Literal(lit) => Some(lit.value().to_owned()),
        _ => None,
    }
}

/// Every (subject, object) pair of `?s pred ?o` (named/blank subjects, any objects).
fn subject_objects(store: &Store, pred: &str) -> Result<Vec<(Term, Term)>, SliceError> {
    let predicate = NamedNode::new(pred)
        .map_err(|e| SliceError::Parse(format!("invalid predicate IRI {pred}: {e}")))?;
    let mut out = Vec::new();
    for quad in store.quads_for_pattern(
        None,
        Some(predicate.as_ref()),
        None,
        Some(GraphNameRef::DefaultGraph),
    ) {
        let quad = quad.map_err(|e| SliceError::Parse(e.to_string()))?;
        let subj = match quad.subject {
            NamedOrBlankNode::NamedNode(nn) => Term::NamedNode(nn),
            NamedOrBlankNode::BlankNode(bn) => Term::BlankNode(bn),
        };
        out.push((subj, quad.object));
    }
    Ok(out)
}

/// Every (subject, object) of `?s pred ?o` as terms — for the tag-map scan.
pub(crate) fn quads_with_predicate(
    store: &Store,
    pred: &str,
) -> Result<Vec<(Term, Term)>, SliceError> {
    subject_objects(store, pred)
}

/// The first literal lexical of `<subject_iri> pred ?o`.
pub(crate) fn first_lexical_of_iri(
    store: &Store,
    subject_iri: &str,
    pred: &str,
) -> Result<Option<String>, SliceError> {
    let subject = NamedNode::new(subject_iri)
        .map_err(|e| SliceError::Parse(format!("invalid subject IRI {subject_iri}: {e}")))?;
    object_literal(store, &subject, pred)
}

pub(crate) fn object_literal(
    store: &Store,
    subject: &NamedNode,
    pred: &str,
) -> Result<Option<String>, SliceError> {
    match first_object(store, subject, pred)? {
        Some(Term::Literal(lit)) => Ok(Some(lit.value().to_owned())),
        _ => Ok(None),
    }
}

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

/// The `(lexical, datatype)` of the first literal object of `subject pred ?o`.
fn literal_of_term(
    store: &Store,
    subject: &Term,
    pred: &str,
) -> Result<Option<(String, Option<String>)>, SliceError> {
    match first_object_of_term(store, subject, pred)? {
        Some(Term::Literal(lit)) => {
            let dt = lit.datatype().as_str().to_owned();
            // oxigraph types a plain literal as xsd:string; treat that as "no explicit
            // datatype" only matters for the boolean check, which compares to
            // xsd:boolean — so keep the real datatype.
            Ok(Some((lit.value().to_owned(), Some(dt))))
        }
        _ => Ok(None),
    }
}

/// Parse an RDF boolean literal to a bool (mirrors `_as_bool`): an authored `false`
/// stays false; absence is false.
fn object_bool_of_term(store: &Store, subject: &Term, pred: &str) -> Result<bool, SliceError> {
    match first_object_of_term(store, subject, pred)? {
        Some(Term::Literal(lit)) => {
            let v = lit.value().trim().to_lowercase();
            Ok(v == "true" || v == "1")
        }
        Some(_) => Ok(true),
        None => Ok(false),
    }
}

/// The members of an rdf:List head node (mirrors `_rdf_list`).
fn rdf_list(store: &Store, head: Option<&Term>) -> Result<Vec<Term>, SliceError> {
    let mut out: Vec<Term> = Vec::new();
    let mut node = head.cloned();
    while let Some(cur) = node {
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
    use std::path::PathBuf;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    #[test]
    fn curie_shortens_and_falls_back() {
        assert_eq!(curie(&format!("{GMEOW}Place")), "gmeow:Place");
        assert_eq!(curie("http://xmlns.com/foaf/0.1/name"), "foaf:name");
        assert_eq!(curie("https://example.org/x"), "<https://example.org/x>");
    }

    #[test]
    fn prefix_token_respects_word_boundary() {
        assert!(has_prefix_token("foo gmeow:Place", "gmeow"));
        assert!(has_prefix_token("gmeow:Place", "gmeow"));
        assert!(!has_prefix_token("foo_ps:bar", "ps"));
        assert!(has_prefix_token("(ps:x)", "ps"));
    }

    #[test]
    fn sparql_string_escapes() {
        assert_eq!(sparql_string("a\"b"), "\"a\\\"b\"");
        assert_eq!(sparql_string("x\ny"), "\"x\\ny\"");
    }

    #[test]
    fn every_sparql_file_matches_committed() {
        let root = repo_root();
        let sets = emit_sparql_sets(&root).expect("emit sparql");
        let dir = root.join("generated").join("queries");
        let mut mismatches: Vec<String> = Vec::new();
        for (filename, text) in &sets {
            let committed_path = dir.join(filename);
            let committed = std::fs::read_to_string(&committed_path)
                .unwrap_or_else(|e| panic!("read {}: {e}", committed_path.display()));
            if *text != committed {
                mismatches.push(format!("{filename}: {}", first_diff(text, &committed)));
            }
        }
        assert!(
            mismatches.is_empty(),
            "{} SPARQL file(s) differ:\n{}",
            mismatches.len(),
            mismatches.join("\n")
        );
        assert_eq!(sets.len(), 45, "expected 45 SPARQL files");
    }

    fn first_diff(got: &str, want: &str) -> String {
        for (i, (g, w)) in got.lines().zip(want.lines()).enumerate() {
            if g != w {
                return format!("line {}:\n  got:  {g}\n  want: {w}", i + 1);
            }
        }
        format!(
            "length differs (got {} lines, want {} lines)",
            got.lines().count(),
            want.lines().count()
        )
    }
}
