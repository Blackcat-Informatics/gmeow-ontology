// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The shared **get leg** of the projection-mapping correspondences: the parsed
//! `gmeow:ProjectionMapping` model that BOTH the EDOAL and the SPARQL-CONSTRUCT
//! lowerings render from.
//!
//! Because the two dialects lower from this one in-memory model — not from two
//! independent reads of the store — they cannot drift: the historical
//! `mapping-compile.spec-drift` lint is moot by construction. Extraction runs over the
//! oxigraph-free [`DslView`]; the model + the property-path / expression rendering are
//! ported verbatim from the historical emitter (byte-parity-critical).

use std::collections::{BTreeMap, BTreeSet};

use crate::ingest::prefixes::ns_to_prefix;
use crate::ingest::{DslTerm, DslView};
use crate::ir::{CorrespondenceRelation, MorphismClass, MorphismKind};

pub(crate) const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

const GM_PROJECTION_MAPPING: &str = "https://blackcatinformatics.ca/gmeow/ProjectionMapping";
const GM_HAS_MAPPING_PATTERN: &str = "https://blackcatinformatics.ca/gmeow/hasMappingPattern";
const GM_HAS_BINDING: &str = "https://blackcatinformatics.ca/gmeow/hasBinding";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";

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
const GM_MORPHISM_CLASS: &str = "https://blackcatinformatics.ca/gmeow/morphismClass";

// ── Model ────────────────────────────────────────────────────────────────────────

/// An expression-algebra node from the mapping DSL.
#[derive(Debug, Clone)]
pub enum Expr {
    Var(String),
    ConstIri(String),
    ConstLiteral(String),
    Op { op: String, args: Vec<Expr> },
}

/// One graph-pattern (or template) atom from the mapping DSL.
#[derive(Debug, Clone)]
pub struct Atom {
    pub subject_var: String,
    pub predicate: Option<String>,
    pub predicate_var: Option<String>,
    pub path: Option<String>,
    pub path_alts: Vec<String>,
    pub object_var: Option<String>,
    pub object_value: Option<String>,
    pub object_literal: Option<(String, Option<String>)>,
    pub optional: bool,
}

/// A pattern item: a flat atom or a nested OPTIONAL group.
#[derive(Debug, Clone)]
pub enum Item {
    Atom(Atom),
    Group(Vec<Item>),
}

/// A derived binding (`BIND expr AS ?var`).
#[derive(Debug, Clone)]
pub struct Bind {
    pub var: String,
    pub expr: Expr,
}

/// One value→class table entry.
#[derive(Debug, Clone)]
pub struct ValueClass {
    pub when_value: String,
    pub to_class: String,
}

/// The GMEOW-side pattern of a projection mapping.
#[derive(Debug, Clone)]
pub struct MappingPattern {
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
    pub fn flat_atoms(&self) -> Vec<Atom> {
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

/// A per-profile output face of a projection mapping.
#[derive(Debug, Clone)]
pub struct ProfileBinding {
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
    /// Optionally-authored `logic:MorphismClass` local name (`gmeow:morphismClass`).
    /// Absent in the committed corpus (so byte-parity holds); when present it lets a
    /// cell declare itself a `BridgeView` even though its EDOAL relation symbol is the
    /// equivalence token `=`, which the overclaim gate then refuses (Principle 5). When
    /// absent the class is DERIVED from the relation lattice ([`relation_lattice`]).
    pub morphism_class: Option<MorphismClass>,
}

impl ProfileBinding {
    /// The `(relation, morphism class, morphism kind)` lattice triple the overclaim gate
    /// enforces: the relation + kind are derived from the EDOAL relation token; the class
    /// is the authored `gmeow:morphismClass` when present, else the lattice default.
    pub fn lattice(&self) -> (CorrespondenceRelation, MorphismClass, MorphismKind) {
        let (relation, derived_class, kind) = relation_lattice(&self.relation);
        (relation, self.morphism_class.unwrap_or(derived_class), kind)
    }
}

/// Resolve a get-leg `relation` token (the EDOAL relation symbol an authored
/// `ProfileBinding` carries) to the typed `logic:` correspondence lattice triple the
/// overclaim gate enforces over: `(relation, morphism class, morphism kind)`.
///
/// The authored frontend tokens are the EDOAL relation symbols: `"="` is equivalence,
/// `"<="`/`">="` are subsumption, `"%"` is disjointness, and anything else is a weaker
/// `RelatedMatch`. The morphism class is the strongest rung the relation can lawfully
/// claim (an honest under-approximation; composition can only weaken it), and the kind
/// is an `InstitutionMorphism` — the authored projection cells are satisfaction-preserving
/// down-projections, not commitment-shifting bridges (a bridge would be authored with an
/// explicit bridge stereotype, which this frontend has no token for, so it can never
/// silently masquerade as equivalence here).
pub fn relation_lattice(relation: &str) -> (CorrespondenceRelation, MorphismClass, MorphismKind) {
    let kind = MorphismKind::InstitutionMorphism;
    match relation.trim() {
        "=" => (
            CorrespondenceRelation::Equiv,
            MorphismClass::WellBehavedLens,
            kind,
        ),
        "<=" => (
            CorrespondenceRelation::SubsumedBy,
            MorphismClass::LossyLens,
            kind,
        ),
        ">=" => (
            CorrespondenceRelation::Subsumes,
            MorphismClass::LossyLens,
            kind,
        ),
        "%" => (
            CorrespondenceRelation::Disjoint,
            MorphismClass::BridgeView,
            kind,
        ),
        _ => (
            CorrespondenceRelation::RelatedMatch,
            MorphismClass::AffineCorrespondence,
            kind,
        ),
    }
}

/// A projection mapping: a pattern + its per-profile bindings.
#[derive(Debug, Clone)]
pub struct ProjectionCell {
    pub iri: String,
    pub label: String,
    pub pattern: MappingPattern,
    pub bindings: Vec<ProfileBinding>,
}

// ── Extraction (over the oxigraph-free DslView) ──────────────────────────────────

/// Parse every `gmeow:ProjectionMapping` into the shared get-leg model.
pub fn projections(view: &DslView) -> Result<Vec<ProjectionCell>, String> {
    let mut cells = Vec::new();
    for cell_iri in view.subjects_of_type(GM_PROJECTION_MAPPING) {
        let Some(pattern_node) = view.first_object(&cell_iri, GM_HAS_MAPPING_PATTERN) else {
            return Err(format!(
                "projection mapping {cell_iri} missing hasMappingPattern"
            ));
        };
        let pattern = parse_pattern(view, &pattern_node)?;
        let mut bindings = Vec::new();
        for binding_node in view.objects_of(&cell_iri, GM_HAS_BINDING) {
            bindings.push(parse_binding(view, &binding_node)?);
        }
        if bindings.is_empty() {
            return Err(format!("projection mapping {cell_iri} has no bindings"));
        }
        let label = view
            .object_literal(&cell_iri, RDFS_LABEL)
            .unwrap_or_default();
        cells.push(ProjectionCell {
            iri: cell_iri,
            label,
            pattern,
            bindings,
        });
    }
    Ok(cells)
}

fn parse_pattern(view: &DslView, node: &DslTerm) -> Result<MappingPattern, String> {
    let Some(anchor) = view.object_literal_of_term(node, GM_ANCHOR) else {
        return Err("mapping pattern missing anchor".to_owned());
    };
    let value = view.object_literal_of_term(node, GM_VALUE);

    let atom_head = view.first_object_of(node, GM_ATOM);
    let mut atoms = Vec::new();
    for item in view.rdf_list(atom_head.as_ref()) {
        atoms.push(parse_item(view, &item)?);
    }

    let mut suppress_when = Vec::new();
    for a in view.objects_of_term(node, GM_SUPPRESS_WHEN) {
        suppress_when.push(parse_atom(view, &a)?);
    }
    suppress_when.sort_by_key(atom_key);

    let mut project_when = Vec::new();
    for a in view.objects_of_term(node, GM_PROJECT_WHEN) {
        project_when.push(parse_atom(view, &a)?);
    }
    project_when.sort_by_key(atom_key);

    let mut exclude_when = Vec::new();
    for a in view.objects_of_term(node, GM_EXCLUDE_WHEN) {
        exclude_when.push(parse_atom(view, &a)?);
    }
    exclude_when.sort_by_key(atom_key);

    let mut filters = Vec::new();
    for f in view.objects_of_term(node, GM_FILTER) {
        filters.push(parse_expr(view, &f)?);
    }
    // Sort by a TOTAL structural key (never fails). Legalization happens at render
    // time, not here: an unsupported operator must not poison the deterministic order
    // (and a filter is dropped as residue by its lowering caller, not silently here).
    filters.sort_by_key(expr_sort_key);

    let mut raw_binds = Vec::new();
    for b in view.objects_of_term(node, GM_BIND) {
        raw_binds.push(parse_bind(view, &b)?);
    }
    let binds = order_binds(raw_binds)?;

    let mut raw_mints = Vec::new();
    for m in view.objects_of_term(node, GM_MINT) {
        raw_mints.push(parse_bind(view, &m)?);
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
        edoal_source: view.object_iri_of_term(node, GM_EDOAL_SOURCE),
        edoal_source_kind: view
            .object_literal_of_term(node, GM_EDOAL_SOURCE_KIND)
            .unwrap_or_else(|| "relation".to_owned()),
        edoal_path: view.object_bool_of_term(node, GM_EDOAL_PATH),
    })
}

fn parse_item(view: &DslView, node: &DslTerm) -> Result<Item, String> {
    if let Some(group_head) = view.first_object_of(node, GM_OPTIONAL_GROUP) {
        let mut inner = Vec::new();
        for item in view.rdf_list(Some(&group_head)) {
            inner.push(parse_item(view, &item)?);
        }
        return Ok(Item::Group(inner));
    }
    Ok(Item::Atom(parse_atom(view, node)?))
}

fn parse_atom(view: &DslView, node: &DslTerm) -> Result<Atom, String> {
    let subj = view
        .object_literal_of_term(node, GM_SUBJECT_VAR)
        .or_else(|| view.object_literal_of_term(node, GM_T_SUBJ));
    let Some(subject_var) = subj else {
        return Err("atom missing subjectVar/tSubj".to_owned());
    };
    let predicate = view
        .object_iri_of_term(node, GM_PREDICATE)
        .or_else(|| view.object_iri_of_term(node, GM_T_PRED));
    let predicate_var = view.object_literal_of_term(node, GM_PREDICATE_VAR);
    let path_node = view.first_object_of(node, GM_PATH);
    let path = match &path_node {
        Some(p) => Some(render_path(view, p)?),
        None => None,
    };
    let path_alts = match &path_node {
        Some(p) => alt_members(view, p),
        None => Vec::new(),
    };
    let object_var = view
        .object_literal_of_term(node, GM_OBJECT_VAR)
        .or_else(|| view.object_literal_of_term(node, GM_T_OBJ));
    let object_value = view
        .object_iri_of_term(node, GM_OBJECT_VALUE)
        .or_else(|| view.object_iri_of_term(node, GM_T_OBJ_VALUE));
    let object_literal = view.literal_of_term(node, GM_OBJECT_LITERAL);
    let optional = view.object_bool_of_term(node, GM_OPTIONAL);
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

fn parse_bind(view: &DslView, node: &DslTerm) -> Result<Bind, String> {
    let Some(var) = view.object_literal_of_term(node, GM_BIND_VAR) else {
        return Err("bind/mint missing bindVar".to_owned());
    };
    let Some(expr_node) = view.first_object_of(node, GM_BIND_EXPR) else {
        return Err("bind/mint missing bindExpr".to_owned());
    };
    Ok(Bind {
        var,
        expr: parse_expr(view, &expr_node)?,
    })
}

fn parse_expr(view: &DslView, node: &DslTerm) -> Result<Expr, String> {
    match node {
        DslTerm::Iri(iri) => return Ok(Expr::ConstIri(iri.clone())),
        DslTerm::Literal { lexical, .. } => return Ok(Expr::ConstLiteral(lexical.clone())),
        DslTerm::Blank { .. } => {}
    }
    if let Some(var) = view.object_literal_of_term(node, GM_EXPR_VAR) {
        return Ok(Expr::Var(var));
    }
    let Some(op) = view.object_iri_of_term(node, GM_EXPR_OP) else {
        return Err("expression node has neither exprVar nor exprOp".to_owned());
    };
    let args_head = view.first_object_of(node, GM_EXPR_ARGS);
    let mut args = Vec::new();
    for a in view.rdf_list(args_head.as_ref()) {
        args.push(parse_expr(view, &a)?);
    }
    Ok(Expr::Op { op, args })
}

fn parse_binding(view: &DslView, node: &DslTerm) -> Result<ProfileBinding, String> {
    let Some(profile) = view.object_literal_of_term(node, GM_PROFILE) else {
        return Err("profile binding missing profile".to_owned());
    };
    let mut template_atoms = Vec::new();
    let ta_head = view.first_object_of(node, GM_TEMPLATE_ATOMS);
    for a in view.rdf_list(ta_head.as_ref()) {
        template_atoms.push(parse_atom(view, &a)?);
    }
    let vcm_head = view.first_object_of(node, GM_VALUE_CLASS_MAP);
    let mut value_class_map = Vec::new();
    for entry in view.rdf_list(vcm_head.as_ref()) {
        let (Some(when), Some(to_class)) = (
            view.object_iri_of_term(&entry, GM_WHEN_VALUE),
            view.object_iri_of_term(&entry, GM_TO_CLASS),
        ) else {
            return Err("value-class entry malformed".to_owned());
        };
        value_class_map.push(ValueClass {
            when_value: when,
            to_class,
        });
    }
    let relation = view
        .object_literal_of_term(node, GM_RELATION)
        .unwrap_or_else(|| "=".to_owned());
    let confidence = match view.object_literal_of_term(node, GM_CONFIDENCE) {
        Some(text) => Some(
            text.parse::<f64>()
                .map_err(|_| "profile binding has non-numeric confidence".to_owned())?,
        ),
        None => None,
    };
    let mut lossy_drops = Vec::new();
    for d in view.objects_of_term(node, GM_LOSSY_DROP) {
        if let Some(text) = d.as_literal() {
            lossy_drops.push(text.to_owned());
        } else if let Some(iri) = d.as_iri() {
            lossy_drops.push(iri.to_owned());
        }
    }
    // Optional authored morphism class (`gmeow:morphismClass`), as either the
    // `logic:`-namespaced individual IRI or a bare local name. Absent in the committed
    // corpus; when present an unknown value is a hard error (no silent fallback).
    let morphism_class = match view
        .object_iri_of_term(node, GM_MORPHISM_CLASS)
        .or_else(|| view.object_literal_of_term(node, GM_MORPHISM_CLASS))
    {
        Some(value) => {
            let local = value.rsplit(['#', '/', ':']).next().unwrap_or(&value);
            Some(MorphismClass::from_local(local).ok_or_else(|| {
                format!("profile binding has unknown gmeow:morphismClass {value}")
            })?)
        }
        None => None,
    };
    Ok(ProfileBinding {
        profile,
        to_predicate: view.object_iri_of_term(node, GM_TO_PREDICATE),
        to_class: view.object_iri_of_term(node, GM_TO_CLASS),
        template_atoms,
        value_class_map,
        relation,
        transform: view.object_iri_of_term(node, GM_TRANSFORM),
        confidence,
        lossy_drops,
        edoal_target: view.object_iri_of_term(node, GM_EDOAL_TARGET),
        edoal_target_kind: view.object_literal_of_term(node, GM_EDOAL_TARGET_KIND),
        morphism_class,
    })
}

// ── Property-path rendering ────────────────────────────────────────────────────

fn render_path(view: &DslView, node: &DslTerm) -> Result<String, String> {
    if let DslTerm::Iri(iri) = node {
        if iri == RDF_TYPE {
            return Ok("rdf:type".to_owned());
        }
        return Ok(curie(iri));
    }
    let types = view.types_of_term(node);
    if types.iter().any(|t| t == GM_ALT_PATH) {
        let head = view.first_object_of(node, GM_PATH_ALTS);
        let mut parts = Vec::new();
        for a in &view.rdf_list(head.as_ref()) {
            parts.push(render_path(view, a)?);
        }
        return Ok(parts.join("|"));
    }
    if types.iter().any(|t| t == GM_SEQ_PATH) {
        let head = view.first_object_of(node, GM_PATH_STEPS);
        let mut parts = Vec::new();
        for s in &view.rdf_list(head.as_ref()) {
            parts.push(render_path(view, s)?);
        }
        return Ok(parts.join("/"));
    }
    if types.iter().any(|t| t == GM_INVERSE_PATH) {
        let step = view.first_object_of(node, GM_PATH_STEP);
        return Ok(format!("^{}", path_primary(view, step.as_ref())?));
    }
    if types.iter().any(|t| t == GM_ZERO_OR_MORE_PATH) {
        let step = view.first_object_of(node, GM_PATH_STEP);
        return Ok(format!("{}*", path_primary(view, step.as_ref())?));
    }
    if types.iter().any(|t| t == GM_ONE_OR_MORE_PATH) {
        let step = view.first_object_of(node, GM_PATH_STEP);
        return Ok(format!("{}+", path_primary(view, step.as_ref())?));
    }
    if types.iter().any(|t| t == GM_ZERO_OR_ONE_PATH) {
        let step = view.first_object_of(node, GM_PATH_STEP);
        return Ok(format!("{}?", path_primary(view, step.as_ref())?));
    }
    if types.iter().any(|t| t == GM_NEGATED_PROPERTY_SET) {
        let head = view.first_object_of(node, GM_PATH_SET);
        let members = view.rdf_list(head.as_ref());
        let mut parts = Vec::new();
        for m in &members {
            parts.push(render_path(view, m)?);
        }
        let inner = parts.join("|");
        return Ok(if members.len() > 1 {
            format!("!({inner})")
        } else {
            format!("!{inner}")
        });
    }
    Err("unknown property-path node".to_owned())
}

fn path_primary(view: &DslView, node: Option<&DslTerm>) -> Result<String, String> {
    let Some(node) = node else {
        return Err("property path missing a step".to_owned());
    };
    let rendered = render_path(view, node)?;
    if rendered.contains('/') || rendered.contains('|') {
        Ok(format!("({rendered})"))
    } else {
        Ok(rendered)
    }
}

/// A top-level AltPath of plain predicates → them, else `()`.
fn alt_members(view: &DslView, node: &DslTerm) -> Vec<String> {
    if !node.is_blank() {
        return Vec::new();
    }
    if !view.types_of_term(node).iter().any(|t| t == GM_ALT_PATH) {
        return Vec::new();
    }
    let head = view.first_object_of(node, GM_PATH_ALTS);
    let members = view.rdf_list(head.as_ref());
    let mut alts = Vec::new();
    for m in &members {
        match m.as_iri() {
            Some(iri) => alts.push(iri.to_owned()),
            None => return Vec::new(),
        }
    }
    alts
}

// ── Expression rendering ───────────────────────────────────────────────────────

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

/// Render one expression-algebra node to its legal SPARQL text.
///
/// Lowering is **legalization** (LOGIC-IR.md § IR commitments): a total function into
/// `⟨ legal output ⊕ flagged residue ⟩`. An expression containing an operator the closed
/// SPARQL algebra cannot express is NOT a legal output, so this returns `Err` carrying the
/// unsupported construct rather than emitting a placeholder token (which would be illegal
/// SPARQL). The lowering caller treats that `Err` as **residue**: it records the dropped
/// construct in the correspondence's loss-ledger residue set and omits it from the legal
/// output — never a malformed placeholder.
pub fn render_expr(expr: &Expr) -> Result<String, String> {
    match expr {
        Expr::Var(v) => Ok(format!("?{v}")),
        Expr::ConstIri(iri) => Ok(curie(iri)),
        Expr::ConstLiteral(text) => Ok(sparql_string(text)),
        Expr::Op { op, args } => {
            let name = op_local(op);
            let rendered: Vec<String> = args
                .iter()
                .map(render_expr)
                .collect::<Result<Vec<_>, _>>()?;
            if name == "opRegex" {
                return Ok(format!("regex({})", rendered.join(", ")));
            }
            if name == "opNot" {
                if rendered.len() != 1 {
                    return Err(format!(
                        "unsupported expression operator: opNot expects exactly 1 argument, \
                         got {}",
                        rendered.len()
                    ));
                }
                return Ok(format!("(!{})", rendered[0]));
            }
            if name == "opIn" {
                if rendered.is_empty() {
                    return Err(
                        "unsupported expression operator: opIn requires at least 1 argument"
                            .to_owned(),
                    );
                }
                return Ok(format!(
                    "({} IN ({}))",
                    rendered[0],
                    rendered[1..].join(", ")
                ));
            }
            if let Some(sym) = infix_op(&name) {
                return Ok(format!("({})", rendered.join(&format!(" {sym} "))));
            }
            if let Some(fn_name) = func_op(&name) {
                return Ok(format!("{fn_name}({})", rendered.join(", ")));
            }
            Err(format!("unsupported expression operator: {name}"))
        }
    }
}

/// A TOTAL structural sort key for an expression — used ONLY for the deterministic
/// filter order, never as output. Unlike [`render_expr`] it cannot fail, so an
/// unsupported operator still sorts deterministically (its legalization-as-residue is
/// handled at render time by the lowering caller).
fn expr_sort_key(expr: &Expr) -> String {
    match expr {
        Expr::Var(v) => format!("var:{v}"),
        Expr::ConstIri(iri) => format!("iri:{iri}"),
        Expr::ConstLiteral(text) => format!("lit:{text}"),
        Expr::Op { op, args } => {
            let inner: Vec<String> = args.iter().map(expr_sort_key).collect();
            format!("op:{}({})", op_local(op), inner.join(","))
        }
    }
}

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

// ── Determinism helpers ──────────────────────────────────────────────────────────

fn atom_key(atom: &Atom) -> Vec<String> {
    let obj_lit = match &atom.object_literal {
        Some((lex, _)) => lex.clone(),
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
pub fn bool_str(b: bool) -> String {
    if b { "True" } else { "False" }.to_owned()
}

fn order_binds(binds: Vec<Bind>) -> Result<Vec<Bind>, String> {
    let mut by_var: BTreeMap<String, Bind> = BTreeMap::new();
    for b in binds {
        if by_var.contains_key(&b.var) {
            return Err(format!("duplicate BIND/mint variable ?{}", b.var));
        }
        by_var.insert(b.var.clone(), b);
    }
    let own: BTreeSet<String> = by_var.keys().cloned().collect();
    let mut deps: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (var, b) in &by_var {
        let mut vars = BTreeSet::new();
        expr_vars(&b.expr, &mut vars);
        let d: BTreeSet<String> = vars
            .into_iter()
            .filter(|v| own.contains(v) && v != var)
            .collect();
        deps.insert(var.clone(), d);
    }
    let mut placed: BTreeSet<String> = BTreeSet::new();
    let mut remaining: BTreeSet<String> = own.clone();
    let mut ordered = Vec::new();
    while !remaining.is_empty() {
        let ready: Vec<String> = remaining
            .iter()
            .filter(|v| deps[*v].is_subset(&placed))
            .cloned()
            .collect();
        if ready.is_empty() {
            let cycle: Vec<String> = remaining.iter().map(|v| format!("?{v}")).collect();
            return Err(format!(
                "cyclic BIND/mint dependency among {}",
                cycle.join(", ")
            ));
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

// ── CURIE shortening + string helpers (shared by both dialects) ──────────────────

/// Shorten an IRI to `prefix:local` via the canonical registry, else `<iri>`.
pub fn curie(iri: &str) -> String {
    for (ns, prefix) in ns_to_prefix() {
        if let Some(local) = iri.strip_prefix(ns) {
            return format!("{prefix}:{local}");
        }
    }
    format!("<{iri}>")
}

/// The local name of an IRI (after the last `#` or `/`).
pub fn local(iri: &str) -> String {
    let cut = iri.rfind(['#', '/']).map(|i| i + 1).unwrap_or(0);
    iri[cut..].to_owned()
}

/// Render a string as a single-line SPARQL string literal.
pub fn sparql_string(text: &str) -> String {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    format!("\"{escaped}\"")
}
