// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The SHACL-derived Pydantic v2 model package (`gmeow_models/<slice>.py`).
//!
//! This emitter is CO-DERIVED from the SAME shape compilation the JSON-Schema
//! stage runs ([`crate::stages::json_schema`]): it loads the shape union, compiles
//! it with the identical [`purrdf::shapes::json_schema::compile`] call, parses the
//! resulting JSON Schema, and transliterates each `$defs` entry into one Pydantic
//! model. Because both surfaces read the ONE `$defs`, a class's Pydantic
//! `model_json_schema()` agrees with the packed JSON Schema by construction
//! (Task 8's cross-surface conformance gate).
//!
//! It REPLACES the flat OWL→LinkML Pydantic file the schemas leaf used to emit
//! (`generated/schemas/gmeow.py`): that surface was a projection of a DIFFERENT
//! model and could drift from the closed-world validator.
//!
//! # Package layout
//!
//! * `gmeow_models/_base.py` — the shared [`ConfiguredBaseModel`] config.
//! * `gmeow_models/_envelope.py` — the synthetic JSON-LD envelope `Node` def.
//! * `gmeow_models/<slice>.py` — one module per owning slice, its models + the
//!   value-vocabulary `StrEnum`s they reference.
//! * `gmeow_models/__init__.py` — re-exports every model + a single
//!   `model_rebuild()` sweep that resolves the deferred cross-slice references
//!   (so per-slice modules never import one another → no import cycle).
//! * `gmeow_models/py.typed` — the PEP 561 marker.
//!
//! # Slice routing
//!
//! Each class routes to a module by its slice ([`route_class`]), by a fixed
//! precedence of RELIABLE signals so nothing lands in a silent catch-all:
//!
//! 1. A class [`gmeow_docs::model::DocsModel`] documents → its owning slice's
//!    module (carrying that term's `gmeow:definitionDigest`).
//! 2. A generated archetype class (`gmeow/openehr/<archetype>/<Local>`) → its IRI
//!    namespace PATH, e.g. the `openehr_bloodpressure` module.
//! 3. An undocumented class in an authored ecosystem namespace
//!    (`logic:`/`lang:`/`math:`) → that namespace's single grounding module.
//! 4. A bare gmeow class declared only in the shared test-DSL vocabulary (`dsl/`)
//!    that owns no slice → the named, documented `_spec` module.
//!
//! Anything else (a class in no documented slice, no gmeow path, and no authored
//! ecosystem namespace) is a HARD FAIL.
//!
//! The public entry point [`render_models_python`] is exercised by this module's
//! own golden tests but is not yet called from a production `Stage` (the
//! `stage-export-pydantic` leaf lands in a follow-up task), so the emitter is dead
//! code in a non-test build until then — hence the module-scoped `dead_code`
//! allowance, removed when the stage is wired.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::Value;

use gmeow_docs::model::{DocSlice, DocsModel};
use purrdf::shapes::shapes::Target;

use crate::gmeow_ns::{GMEOW_NS, LANG_NS, LOGIC_NS, MATH_NS, gmeow_json_schema_namespaces};
use crate::stages::schema_ident::{
    class_render_order, finish_text, local_name, py_string, sanitize_identifier, sanitize_type,
};

/// The synthetic JSON-LD-envelope module (`Node`) — not a slice.
const ENVELOPE_MODULE: &str = "_envelope";
/// The shared-base module (`ConfiguredBaseModel`).
const BASE_MODULE: &str = "_base";
/// The home for gmeow-namespaced shape-bearing classes declared only in the
/// shared test-DSL vocabulary (`dsl/`) that own no slice.
const SPEC_MODULE: &str = "_spec";
/// The package directory prefix every artifact key carries.
const PKG: &str = "gmeow_models";

/// JSON-Schema `format` strings the emitter is EXPLICITLY allowed to widen to
/// `str` (recording a declared datatype loss) rather than hard-failing. A format
/// outside BOTH the primitive mapping table AND this allowlist is a hard fail —
/// the emitter never silently widens an unknown datatype.
const KNOWN_LOSSY_TO_STR: &[&str] = &[
    "binary",
    "byte",
    "duration",
    "email",
    "hostname",
    "idn-email",
    "iri",
    "iri-reference",
    "regex",
    "uri-reference",
    "uri-template",
    "uuid",
];

/// One widened datatype/format, retained for a later "declared loss" surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeclaredDatatypeLoss {
    /// The module the widened field lives in.
    pub module: String,
    /// The model class carrying the field.
    pub class: String,
    /// The Python field name.
    pub field: String,
    /// The JSON-Schema `format` that was widened to `str`.
    pub datatype: String,
}

/// The rendered Pydantic package: every artifact keyed by its logical path, plus
/// the declared datatype-widening list plumbed for a later task.
#[derive(Debug, Clone)]
pub(crate) struct ModelsPython {
    /// Logical path → file bytes (`gmeow_models/__init__.py`, `<slice>.py`, …).
    pub artifacts: BTreeMap<String, Vec<u8>>,
    /// The allowlisted datatype widenings (empty in the current corpus).
    pub declared_datatype_losses: Vec<DeclaredDatatypeLoss>,
}

fn err(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-export-pydantic".into(),
        message: message.into(),
    })
}

// ── Public entry point ───────────────────────────────────────────────────────

/// Render the `gmeow_models` Pydantic package from the repo's shape union.
///
/// Deterministic: every collection is `BTreeMap`/sorted, there are no timestamps,
/// and the `$defs` iteration order is the compiler's sorted key order.
pub(crate) fn render_models_python(root: &Path) -> Result<ModelsPython, gmeow_errors::Diag> {
    // 1. THE co-derivation point: load the shape union and compile it with the
    //    exact call the JSON-Schema stage makes, so both surfaces read one `$defs`.
    let (_store, shapes) = purrdf::shapes::shape_union::load_shapes(root)
        .map_err(|m| err(format!("load shape union: {m}")))?;
    let ns = gmeow_json_schema_namespaces();
    let compiled = purrdf::shapes::json_schema::compile(&shapes, &ns);
    let schema: Value = serde_json::from_str(&compiled.schema_json)
        .map_err(|e| err(format!("parse compiled JSON Schema: {e}")))?;
    let defs = schema
        .get("$defs")
        .and_then(Value::as_object)
        .ok_or_else(|| err("compiled JSON Schema has no $defs object"))?;

    // 2. Map every `$defs` key back to its class IRI via the SAME `def_key`
    //    function the compiler keyed with — the co-derived class identity.
    let mut defkey_to_iri: BTreeMap<String, String> = BTreeMap::new();
    for shape in &shapes.node_shapes {
        if shape.deactivated {
            continue;
        }
        for target in &shape.targets {
            if let Target::Class(c) = target {
                defkey_to_iri.insert(ns.def_key(c.as_str()), c.as_str().to_owned());
            }
        }
    }

    // 3. The documentation model: term IRI → owning slice + content digest, and
    //    the per-slice header facts (tier / DOI / label).
    let docs = DocsModel::discover(root).map_err(|e| err(format!("docs model discover: {e}")))?;
    let term_index: BTreeMap<&str, &gmeow_docs::model::DocTerm> =
        docs.terms.iter().map(|t| (t.iri.as_str(), t)).collect();
    let slice_by_iri: BTreeMap<&str, &DocSlice> =
        docs.slices.iter().map(|s| (s.iri.as_str(), s)).collect();

    build_package(defs, &defkey_to_iri, &ns, &term_index, &slice_by_iri)
}

/// Transliterate one compiled `$defs` map into the Pydantic package. Split from
/// [`render_models_python`] so it can be exercised over a synthetic `$defs`
/// (e.g. a closed/`extra="forbid"` class the real corpus does not yet carry).
fn build_package(
    defs: &serde_json::Map<String, Value>,
    defkey_to_iri: &BTreeMap<String, String>,
    ns: &purrdf::Namespaces,
    term_index: &BTreeMap<&str, &gmeow_docs::model::DocTerm>,
    slice_by_iri: &BTreeMap<&str, &DocSlice>,
) -> Result<ModelsPython, gmeow_errors::Diag> {
    // 4. PASS 1 — route every def to a module and resolve its identity.
    let defkey_to_class: BTreeMap<String, String> = defs
        .keys()
        .map(|k| (k.clone(), sanitize_type(k, "GmeowModel")))
        .collect();
    // Model-name collision guard: two distinct `$defs` keys must not sanitize to
    // one class name (would silently clobber a model).
    {
        let mut seen: BTreeMap<&str, &str> = BTreeMap::new();
        for (key, class) in &defkey_to_class {
            if let Some(prev) = seen.insert(class.as_str(), key.as_str()) {
                return Err(err(format!(
                    "two $defs keys collide on Pydantic class name {class:?}: {prev} vs {key}"
                )));
            }
        }
    }
    let iri_to_defkey: BTreeMap<&str, &str> = defkey_to_iri
        .iter()
        .map(|(k, v)| (v.as_str(), k.as_str()))
        .collect();

    let mut routes: BTreeMap<String, ClassRoute> = BTreeMap::new();
    for key in defs.keys() {
        let route = route_class(
            key,
            defkey_to_iri.get(key).map(String::as_str),
            ns,
            term_index,
        )?;
        routes.insert(key.clone(), route);
    }

    // 5. PASS 2 — per module, choose each class's single same-module parent and
    //    compute the parent-before-child render order.
    let mut module_keys: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (key, route) in &routes {
        module_keys
            .entry(route.module.clone())
            .or_default()
            .push(key.clone());
    }

    let mut losses: Vec<DeclaredDatatypeLoss> = Vec::new();
    let mut modules: Vec<RenderedModule> = Vec::new();
    for (module, keys) in &module_keys {
        // Parent link per class (same module + single emitted gmeow superclass).
        let mut parent_of: BTreeMap<String, Option<String>> = BTreeMap::new();
        for key in keys {
            let route = &routes[key];
            let parent = single_same_module_parent(route, &routes, &iri_to_defkey, module);
            parent_of.insert(route.class_name.clone(), parent);
        }
        let class_order = class_render_order(&parent_of);
        let classname_to_key: BTreeMap<&str, &str> = keys
            .iter()
            .map(|k| (routes[k].class_name.as_str(), k.as_str()))
            .collect();

        let mut needs = ModuleNeeds::default();
        let mut enums: BTreeMap<String, PyEnum> = BTreeMap::new();
        let mut models: Vec<PyModel> = Vec::new();
        for class_name in &class_order {
            let key = classname_to_key[class_name.as_str()];
            let route = &routes[key];
            let def = defs
                .get(key)
                .and_then(Value::as_object)
                .ok_or_else(|| err(format!("$defs entry {key:?} is not an object")))?;
            let parent = parent_of
                .get(class_name)
                .cloned()
                .flatten()
                .unwrap_or_else(|| "ConfiguredBaseModel".to_owned());
            let model = build_model(
                route,
                def,
                parent,
                &defkey_to_class,
                module,
                &mut enums,
                &mut needs,
                &mut losses,
            )?;
            models.push(model);
        }

        let header = module_header(module, keys, &routes, slice_by_iri);
        modules.push(RenderedModule {
            slug: module.clone(),
            text: render_module(&header, &needs, &enums, &models),
            model_names: models.iter().map(|m| m.class_name.clone()).collect(),
            enum_names: enums.keys().cloned().collect(),
        });
    }

    // 6. Assemble the package artifacts.
    let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    artifacts.insert(format!("{PKG}/{BASE_MODULE}.py"), render_base());
    artifacts.insert(format!("{PKG}/py.typed"), Vec::new());
    for module in &modules {
        artifacts.insert(format!("{PKG}/{}.py", module.slug), module.text.clone());
    }
    artifacts.insert(format!("{PKG}/__init__.py"), render_init(&modules));

    Ok(ModelsPython {
        artifacts,
        declared_datatype_losses: losses,
    })
}

// ── Class routing / identity ─────────────────────────────────────────────────

/// A routed class: its module, Pydantic class name, and RDF identity.
struct ClassRoute {
    class_name: String,
    module: String,
    /// The full class IRI, or empty for the synthetic `Node` envelope def.
    iri: String,
    /// The compact CURIE, or empty for a synthetic def.
    curie: String,
    /// `gmeow:definitionDigest` from the docs model; empty for a non-documented
    /// (generated archetype) or synthetic def.
    digest: String,
    /// Superclass IRIs (documented `rdfs:subClassOf`), used for same-module
    /// inheritance; empty for non-documented / synthetic defs.
    parents: Vec<String>,
    /// Synthetic envelope def (`Node`) — not backed by a class IRI.
    synthetic: bool,
}

/// Route ONE `$defs` key to a module and resolve its RDF identity.
///
/// A documented term routes to its owning slice's module and carries the term's
/// content digest. A class from a generated archetype namespace
/// (`gmeow/openehr/<archetype>/<Local>`, ≥ 2 path segments under the gmeow
/// namespace, and NOT documented) routes by its IRI namespace path to an
/// `openehr_<archetype>`-style module — the deterministic, reliable structural
/// signal for a generated class that has no authored slice. The synthetic
/// discriminated-`Node` def routes to the envelope module. Anything else (a bare
/// gmeow class with no documented term) is a HARD FAIL.
fn route_class(
    key: &str,
    iri: Option<&str>,
    ns: &purrdf::Namespaces,
    term_index: &BTreeMap<&str, &gmeow_docs::model::DocTerm>,
) -> Result<ClassRoute, gmeow_errors::Diag> {
    let class_name = sanitize_type(key, "GmeowModel");
    let Some(iri) = iri else {
        // No target-class IRI backs this def: the only such def is the synthetic
        // discriminated `Node`. Any other unbacked def is unexpected.
        if key == "Node" {
            return Ok(ClassRoute {
                class_name,
                module: ENVELOPE_MODULE.to_owned(),
                iri: String::new(),
                curie: String::new(),
                digest: String::new(),
                parents: Vec::new(),
                synthetic: true,
            });
        }
        return Err(err(format!(
            "$defs key {key:?} is backed by no target class and is not the synthetic Node envelope"
        )));
    };

    let curie = ns.compact_iri(iri);
    if let Some(term) = term_index.get(iri) {
        return Ok(ClassRoute {
            class_name,
            module: module_slug(local_name(&term.owner_slice)),
            iri: iri.to_owned(),
            curie,
            digest: term.content_digest.clone(),
            parents: term
                .parents
                .iter()
                .filter(|p| p.starts_with(GMEOW_NS))
                .cloned()
                .collect(),
            synthetic: false,
        });
    }

    // Not a documented term. Route by the reliable structural signal.
    let unrouted = |module: &str| ClassRoute {
        class_name: class_name.clone(),
        module: module.to_owned(),
        iri: iri.to_owned(),
        curie: curie.clone(),
        digest: String::new(),
        parents: Vec::new(),
        synthetic: false,
    };

    // (a) A generated archetype class routes by its gmeow namespace PATH
    //     (`openehr/bloodpressure/Diastolic` → the `openehr_bloodpressure` module).
    if let Some(rest) = iri.strip_prefix(GMEOW_NS) {
        let segments: Vec<&str> = rest.split('/').collect();
        if segments.len() >= 2 {
            return Ok(unrouted(&module_slug(
                &segments[..segments.len() - 1].join("_"),
            )));
        }
    }
    // (b) An authored ecosystem namespace (logic/lang/math) has exactly ONE
    //     grounding slice, so an UNDOCUMENTED class there (e.g. the test-DSL
    //     `lang:FlagshipScenario`) routes to that grounding module — the same
    //     module its documented siblings land in.
    if let Some(module) = ecosystem_module(iri) {
        return Ok(unrouted(module));
    }
    // (c) A bare gmeow class that is neither documented, path-structured, nor in an
    //     ecosystem namespace is declared only in the shared test-DSL vocabulary
    //     (`dsl/`) and owns no slice (today: `gmeow:FlagshipScenario`). It goes to
    //     the documented `_spec` module — a NAMED, explained home, not a silent
    //     catch-all.
    if iri.starts_with(GMEOW_NS) {
        return Ok(unrouted(SPEC_MODULE));
    }

    Err(err(format!(
        "class IRI {iri:?} ($defs key {key:?}) is in no documented slice, no gmeow \
         namespace path, and no authored ecosystem namespace — refusing to route it \
         into a silent catch-all"
    )))
}

/// The single grounding-slice module for an authored ecosystem namespace, or
/// `None` for the primary gmeow namespace / a builtin. Each of `logic:`, `lang:`,
/// `math:` has exactly one grounding slice whose module name is its short prefix,
/// so an undocumented class in that namespace routes to it deterministically.
fn ecosystem_module(iri: &str) -> Option<&'static str> {
    if iri.starts_with(LOGIC_NS) {
        Some("logic")
    } else if iri.starts_with(LANG_NS) {
        Some("lang")
    } else if iri.starts_with(MATH_NS) {
        Some("math")
    } else {
        None
    }
}

/// Normalize a slice / namespace-path token into a valid, lowercase Python
/// module name (`slice-quality-rubric` → `slice_quality_rubric`).
fn module_slug(raw: &str) -> String {
    sanitize_identifier(raw, "slice").to_ascii_lowercase()
}

/// The single same-module emitted gmeow superclass to extend, or `None` (→
/// `ConfiguredBaseModel`). Cross-module parents fall back to
/// `ConfiguredBaseModel` so a slice module never has to import a sibling module's
/// class as a base (which cannot be a deferred forward ref and would risk an
/// import cycle).
fn single_same_module_parent(
    route: &ClassRoute,
    routes: &BTreeMap<String, ClassRoute>,
    iri_to_defkey: &BTreeMap<&str, &str>,
    module: &str,
) -> Option<String> {
    let mut candidates: Vec<&ClassRoute> = Vec::new();
    for parent_iri in &route.parents {
        if parent_iri == &route.iri {
            continue;
        }
        if let Some(defkey) = iri_to_defkey.get(parent_iri.as_str())
            && let Some(parent_route) = routes.get(*defkey)
            && parent_route.module == module
        {
            candidates.push(parent_route);
        }
    }
    if candidates.len() == 1 {
        Some(candidates[0].class_name.clone())
    } else {
        None
    }
}

// ── Property → Python type resolution ────────────────────────────────────────

/// A resolved field type: the inner Python type plus whether it is list-wrapped.
struct Resolved {
    base: String,
    multivalued: bool,
}

impl Resolved {
    fn scalar(base: impl Into<String>) -> Self {
        Self {
            base: base.into(),
            multivalued: false,
        }
    }
}

/// Per-module import needs, accumulated while resolving field types.
#[derive(Default)]
struct ModuleNeeds {
    any: bool,
    datetime: bool,
    date: bool,
    time: bool,
}

/// A generated value-vocabulary enum (`StrEnum`).
struct PyEnum {
    name: String,
    /// `(python-member-ident, member-value)`; the value is the full IRI/CURIE or
    /// the literal lexical form — NEVER a bare local name (StrEnum aliases on
    /// equal values).
    members: Vec<(String, String)>,
}

struct FieldCtx<'a> {
    class_name: &'a str,
    field_local: &'a str,
    module: &'a str,
    defkey_to_class: &'a BTreeMap<String, String>,
    enums: &'a mut BTreeMap<String, PyEnum>,
    needs: &'a mut ModuleNeeds,
    losses: &'a mut Vec<DeclaredDatatypeLoss>,
}

/// Resolve a property value schema into a Python field type.
fn resolve(schema: &Value, ctx: &mut FieldCtx<'_>) -> Result<Resolved, gmeow_errors::Diag> {
    let Some(obj) = schema.as_object() else {
        // A bare `true`/`false` value schema (a `sh:closed` ignored key) is
        // permissive → `Any`.
        ctx.needs.any = true;
        return Ok(Resolved::scalar("Any"));
    };
    if obj.is_empty() {
        ctx.needs.any = true;
        return Ok(Resolved::scalar("Any"));
    }
    if let Some(reference) = obj.get("$ref").and_then(Value::as_str) {
        return Ok(Resolved::scalar(ref_to_name(
            reference,
            ctx.defkey_to_class,
        )));
    }
    if obj.contains_key("enum") {
        return Ok(Resolved::scalar(register_enum(obj, ctx)?));
    }
    if let Some(alts) = obj.get("anyOf").and_then(Value::as_array) {
        // The multivalued shape `anyOf:[single, {type:array, items:single}]`:
        // the array alternative carries the element schema.
        if let Some(array_alt) = alts
            .iter()
            .find(|a| a.get("type").and_then(Value::as_str) == Some("array"))
        {
            let items = array_alt.get("items").unwrap_or(&Value::Null);
            let inner = resolve(items, ctx)?;
            return Ok(Resolved {
                base: inner.base,
                multivalued: true,
            });
        }
        return resolve_union(alts, ctx);
    }
    match obj.get("type").and_then(Value::as_str) {
        Some("array") => {
            let items = obj.get("items").unwrap_or(&Value::Null);
            let inner = resolve(items, ctx)?;
            Ok(Resolved {
                base: inner.base,
                multivalued: true,
            })
        }
        Some("object") => {
            if is_iri_ref(obj) || is_typed_literal(obj) {
                // A JSON-LD node reference (`{"@id": ...}`) or typed literal — the
                // faithful bare Python carrier is `str`.
                Ok(Resolved::scalar("str"))
            } else {
                ctx.needs.any = true;
                Ok(Resolved::scalar("Any"))
            }
        }
        Some(_) => map_primitive(obj, ctx),
        None => {
            // A constraints-only schema (e.g. a bare `pattern`) applies to a
            // string value space.
            Ok(Resolved::scalar("str"))
        }
    }
}

/// Resolve a scalar `anyOf` union (no array alternative) by priority:
/// enum > model `$ref` > primitive > IRI/typed-literal (`str`) > `Any`.
fn resolve_union(alts: &[Value], ctx: &mut FieldCtx<'_>) -> Result<Resolved, gmeow_errors::Diag> {
    for a in alts {
        if let Some(o) = a.as_object()
            && o.contains_key("enum")
        {
            return Ok(Resolved::scalar(register_enum(o, ctx)?));
        }
    }
    for a in alts {
        if let Some(r) = a.get("$ref").and_then(Value::as_str) {
            return Ok(Resolved::scalar(ref_to_name(r, ctx.defkey_to_class)));
        }
    }
    for a in alts {
        if let Some(o) = a.as_object()
            && is_scalar_primitive(o)
        {
            return map_primitive(o, ctx);
        }
    }
    for a in alts {
        if let Some(o) = a.as_object()
            && (is_iri_ref(o) || is_typed_literal(o))
        {
            return Ok(Resolved::scalar("str"));
        }
    }
    ctx.needs.any = true;
    Ok(Resolved::scalar("Any"))
}

/// Map a primitive value schema (`{"type": ..., "format": ...}`) to a Python
/// scalar, hard-failing on a datatype/format outside the mapping table AND the
/// [`KNOWN_LOSSY_TO_STR`] allowlist.
fn map_primitive(
    obj: &serde_json::Map<String, Value>,
    ctx: &mut FieldCtx<'_>,
) -> Result<Resolved, gmeow_errors::Diag> {
    let ty = obj.get("type").and_then(Value::as_str).unwrap_or("");
    match ty {
        "boolean" => Ok(Resolved::scalar("bool")),
        "integer" => Ok(Resolved::scalar("int")),
        "number" => Ok(Resolved::scalar("float")),
        "string" => match obj.get("format").and_then(Value::as_str) {
            None => Ok(Resolved::scalar("str")),
            Some("date-time") => {
                ctx.needs.datetime = true;
                Ok(Resolved::scalar("datetime"))
            }
            Some("date") => {
                ctx.needs.date = true;
                Ok(Resolved::scalar("date"))
            }
            Some("time") => {
                ctx.needs.time = true;
                Ok(Resolved::scalar("time"))
            }
            // A URI lexical form is faithfully a `str`, not a datatype loss.
            Some("uri") => Ok(Resolved::scalar("str")),
            Some(fmt) if KNOWN_LOSSY_TO_STR.contains(&fmt) => {
                ctx.losses.push(DeclaredDatatypeLoss {
                    module: ctx.module.to_owned(),
                    class: ctx.class_name.to_owned(),
                    field: ctx.field_local.to_owned(),
                    datatype: fmt.to_owned(),
                });
                Ok(Resolved::scalar("str"))
            }
            Some(fmt) => Err(err(format!(
                "field {}.{} carries string format {fmt:?} outside the mapping table and the \
                 KNOWN_LOSSY_TO_STR allowlist — refusing to silently widen it to str",
                ctx.class_name, ctx.field_local
            ))),
        },
        other => Err(err(format!(
            "field {}.{} carries unsupported primitive type {other:?}",
            ctx.class_name, ctx.field_local
        ))),
    }
}

fn is_scalar_primitive(obj: &serde_json::Map<String, Value>) -> bool {
    matches!(
        obj.get("type").and_then(Value::as_str),
        Some("string" | "integer" | "number" | "boolean")
    )
}

fn is_iri_ref(obj: &serde_json::Map<String, Value>) -> bool {
    obj.get("type").and_then(Value::as_str) == Some("object")
        && obj
            .get("properties")
            .and_then(Value::as_object)
            .is_some_and(|p| p.contains_key("@id"))
}

fn is_typed_literal(obj: &serde_json::Map<String, Value>) -> bool {
    obj.get("type").and_then(Value::as_str) == Some("object")
        && obj
            .get("properties")
            .and_then(Value::as_object)
            .is_some_and(|p| p.contains_key("@value"))
}

/// `#/$defs/Foo` → the Pydantic class name for `Foo` (a deferred forward ref
/// resolved by the `__init__` rebuild sweep).
fn ref_to_name(reference: &str, defkey_to_class: &BTreeMap<String, String>) -> String {
    let target = reference.strip_prefix("#/$defs/").unwrap_or(reference);
    defkey_to_class
        .get(target)
        .cloned()
        .unwrap_or_else(|| sanitize_type(target, "GmeowModel"))
}

/// Register (once) and name the `StrEnum` for an `{"enum": [...]}` schema. Member
/// VALUES are the full IRIs/CURIEs/literals verbatim; two members sharing a value
/// is a HARD FAIL (StrEnum would silently alias them).
fn register_enum(
    obj: &serde_json::Map<String, Value>,
    ctx: &mut FieldCtx<'_>,
) -> Result<String, gmeow_errors::Diag> {
    let name = format!(
        "{}{}Enum",
        ctx.class_name,
        sanitize_type(ctx.field_local, "Field")
    );
    if ctx.enums.contains_key(&name) {
        return Ok(name);
    }
    let mut values: Vec<String> = obj
        .get("enum")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|v| match v.as_str() {
                    Some(s) => s.to_owned(),
                    None => v.to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
    values.sort();
    values.dedup();

    let mut members: Vec<(String, String)> = Vec::new();
    let mut used_idents: BTreeSet<String> = BTreeSet::new();
    let mut seen_values: BTreeSet<String> = BTreeSet::new();
    for value in values {
        if !seen_values.insert(value.clone()) {
            return Err(err(format!(
                "enum {name} has two members with the same value {value:?} — a StrEnum would \
                 silently alias them"
            )));
        }
        let mut ident = sanitize_identifier(local_name(&value), "value");
        while !used_idents.insert(ident.clone()) {
            ident.push('_');
        }
        members.push((ident, value));
    }
    ctx.enums.insert(
        name.clone(),
        PyEnum {
            name: name.clone(),
            members,
        },
    );
    Ok(name)
}

// ── Model building ───────────────────────────────────────────────────────────

/// A resolved field ready to render.
struct PyField {
    py_name: String,
    /// The pre-`| None` type expression (`str`, `list[Foo]`, `str | list[str]`).
    type_expr: String,
    alias: String,
    required: bool,
}

/// A resolved model ready to render.
struct PyModel {
    class_name: String,
    parent: String,
    /// The class IRI for the docstring (empty for the synthetic envelope).
    iri: String,
    extra: &'static str,
    /// `json_schema_extra` identity.
    jse: JsonSchemaExtra,
    fields: Vec<PyField>,
}

enum JsonSchemaExtra {
    /// A real ontology class: `iri` / `curie` / `definitionDigest` / `$id`.
    Real {
        iri: String,
        curie: String,
        digest: String,
    },
    /// The synthetic JSON-LD envelope def.
    Envelope,
}

#[allow(clippy::too_many_arguments)]
fn build_model(
    route: &ClassRoute,
    def: &serde_json::Map<String, Value>,
    parent: String,
    defkey_to_class: &BTreeMap<String, String>,
    module: &str,
    enums: &mut BTreeMap<String, PyEnum>,
    needs: &mut ModuleNeeds,
    losses: &mut Vec<DeclaredDatatypeLoss>,
) -> Result<PyModel, gmeow_errors::Diag> {
    let extra = if def.get("additionalProperties") == Some(&Value::Bool(false)) {
        "forbid"
    } else {
        "allow"
    };
    let required: BTreeSet<&str> = def
        .get("required")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    let mut fields: Vec<PyField> = Vec::new();
    let mut used: BTreeSet<String> = BTreeSet::new();
    if let Some(props) = def.get("properties").and_then(Value::as_object) {
        for (key, pv) in props {
            let (py_base, type_expr) = match key.as_str() {
                // JSON-LD envelope keys: faithfully represented (so they appear in
                // `model_json_schema()`), always optional.
                "@id" => ("id".to_owned(), "str".to_owned()),
                "@type" => ("type".to_owned(), "str | list[str]".to_owned()),
                "@annotation" => ("annotation".to_owned(), "Annotation".to_owned()),
                _ => {
                    let field_local = field_local(key);
                    let mut ctx = FieldCtx {
                        class_name: &route.class_name,
                        field_local: &field_local,
                        module,
                        defkey_to_class,
                        enums,
                        needs,
                        losses,
                    };
                    let resolved = resolve(pv, &mut ctx)?;
                    let expr = if resolved.multivalued {
                        format!("list[{}]", resolved.base)
                    } else {
                        resolved.base
                    };
                    (field_local, expr)
                }
            };
            let mut py_name = sanitize_identifier(&py_base, "field");
            while !used.insert(py_name.clone()) {
                py_name.push('_');
            }
            let is_envelope = matches!(key.as_str(), "@id" | "@type" | "@annotation");
            fields.push(PyField {
                py_name,
                type_expr,
                alias: key.clone(),
                required: !is_envelope && required.contains(key.as_str()),
            });
        }
    }

    let jse = if route.synthetic {
        JsonSchemaExtra::Envelope
    } else {
        JsonSchemaExtra::Real {
            iri: route.iri.clone(),
            curie: route.curie.clone(),
            digest: route.digest.clone(),
        }
    };

    Ok(PyModel {
        class_name: route.class_name.clone(),
        parent,
        iri: route.iri.clone(),
        extra,
        jse,
        fields,
    })
}

/// The local part of a property key: the segment after the first `:` (dropping a
/// CURIE prefix) then after the last `#`/`/` (`gmeow:openehr/…/units` → `units`,
/// `gmeow:decidedLabel` → `decidedLabel`).
fn field_local(key: &str) -> String {
    let after_prefix = key.split_once(':').map_or(key, |(_, rest)| rest);
    local_name(after_prefix).to_owned()
}

// ── Rendering ────────────────────────────────────────────────────────────────

struct ModuleHeader {
    /// The human-facing title line (slice label / archetype namespace / envelope).
    title: String,
    /// Extra header body lines (slice IRI / tier / DOI, or a note).
    lines: Vec<String>,
}

fn module_header(
    module: &str,
    keys: &[String],
    routes: &BTreeMap<String, ClassRoute>,
    slice_by_iri: &BTreeMap<&str, &DocSlice>,
) -> ModuleHeader {
    if module == ENVELOPE_MODULE {
        return ModuleHeader {
            title: "JSON-LD envelope types".to_owned(),
            lines: vec![
                "The synthetic discriminated-node envelope def shared by every".to_owned(),
                "instance projection (not an authored ontology class).".to_owned(),
            ],
        };
    }
    if module == SPEC_MODULE {
        return ModuleHeader {
            title: "GMEOW spec / test-DSL classes".to_owned(),
            lines: vec![
                "gmeow-namespaced classes that carry SHACL shapes but are declared".to_owned(),
                "only in the shared test-DSL vocabulary (dsl/) and own no slice.".to_owned(),
            ],
        };
    }
    // A documented module: recover the owning slice from any of its classes.
    let slice = keys.iter().find_map(|k| {
        let iri = &routes[k].iri;
        if iri.is_empty() {
            None
        } else {
            // Match by owner-slice via the term's slice; the route stored the
            // module, so locate the DocSlice whose module_slug equals this module.
            slice_by_iri
                .values()
                .find(|s| module_slug(local_name(&s.iri)) == module)
                .copied()
        }
    });
    if let Some(slice) = slice {
        let name = slice
            .title
            .clone()
            .or_else(|| slice.label.clone())
            .unwrap_or_else(|| local_name(&slice.iri).to_owned());
        let tier = match &slice.tier {
            Some(purrdf::slice::SliceTier::Core) => "core".to_owned(),
            Some(purrdf::slice::SliceTier::Extension) => "extension".to_owned(),
            Some(purrdf::slice::SliceTier::Domain) => "domain".to_owned(),
            Some(purrdf::slice::SliceTier::Unknown(iri)) => iri.clone(),
            None => "—".to_owned(),
        };
        let doi = slice.identifier.clone().unwrap_or_else(|| "—".to_owned());
        return ModuleHeader {
            title: format!("GMEOW models — slice {name}"),
            lines: vec![
                format!("Slice IRI:  {}", slice.iri),
                format!("Tier:       {tier}"),
                format!("DOI:        {doi}"),
            ],
        };
    }
    // A generated archetype namespace module (no authored slice).
    ModuleHeader {
        title: format!("GMEOW models — generated archetype namespace {module}"),
        lines: vec![
            "Classes projected from a GENERATED archetype shape namespace; these".to_owned(),
            "carry no authored slice identity (no gmeow:definitionDigest).".to_owned(),
        ],
    }
}

fn render_docstring(title: &str, lines: &[String]) -> String {
    let mut out = String::new();
    out.push_str("\"\"\"");
    out.push_str(title);
    out.push_str(".\n");
    if !lines.is_empty() {
        out.push('\n');
        for line in lines {
            out.push_str(line);
            out.push('\n');
        }
    }
    out.push('\n');
    out.push_str("GENERATED by the gmeow pydantic emitter — DO NOT EDIT.\n");
    out.push_str("\"\"\"\n");
    out
}

fn render_module(
    header: &ModuleHeader,
    needs: &ModuleNeeds,
    enums: &BTreeMap<String, PyEnum>,
    models: &[PyModel],
) -> Vec<u8> {
    let mut out = String::new();
    out.push_str(&render_docstring(&header.title, &header.lines));
    out.push_str("from __future__ import annotations\n\n");

    // The standard-library import group (only the lines actually needed).
    let mut stdlib: Vec<String> = Vec::new();
    if needs.datetime || needs.date || needs.time {
        let mut parts: Vec<&str> = Vec::new();
        if needs.date {
            parts.push("date");
        }
        if needs.datetime {
            parts.push("datetime");
        }
        if needs.time {
            parts.push("time");
        }
        stdlib.push(format!("from datetime import {}", parts.join(", ")));
    }
    if !enums.is_empty() {
        stdlib.push("from enum import StrEnum".to_owned());
    }
    if needs.any {
        stdlib.push("from typing import Any".to_owned());
    }
    if !stdlib.is_empty() {
        for line in &stdlib {
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
    }
    out.push_str("from pydantic import ConfigDict, Field\n\n");
    out.push_str(&format!(
        "from .{BASE_MODULE} import ConfiguredBaseModel\n\n\n"
    ));

    for py_enum in enums.values() {
        out.push_str(&format!("class {}(StrEnum):\n", py_enum.name));
        if py_enum.members.is_empty() {
            out.push_str("    pass\n\n\n");
            continue;
        }
        for (ident, value) in &py_enum.members {
            out.push_str(&format!("    {ident} = {}\n", py_string(value)));
        }
        out.push_str("\n\n");
    }

    for model in models {
        render_model(&mut out, model);
    }

    finish_text(out)
}

fn render_model(out: &mut String, model: &PyModel) {
    out.push_str(&format!("class {}({}):\n", model.class_name, model.parent));
    let summary = if model.iri.is_empty() {
        model.class_name.clone()
    } else {
        format!("{} — {}", model.class_name, model.iri)
    };
    out.push_str(&format!("    \"\"\"{summary}.\"\"\"\n\n"));
    out.push_str("    model_config = ConfigDict(\n");
    out.push_str(&format!("        extra={},\n", py_string(model.extra)));
    out.push_str(&format!(
        "        json_schema_extra={},\n",
        render_jse(&model.jse)
    ));
    out.push_str("    )\n");

    if !model.fields.is_empty() {
        out.push('\n');
        for field in &model.fields {
            if field.required {
                out.push_str(&format!(
                    "    {}: {} = Field(alias={})\n",
                    field.py_name,
                    field.type_expr,
                    py_string(&field.alias)
                ));
            } else {
                out.push_str(&format!(
                    "    {}: {} | None = Field(default=None, alias={})\n",
                    field.py_name,
                    field.type_expr,
                    py_string(&field.alias)
                ));
            }
        }
    }
    out.push_str("\n\n");
}

fn render_jse(jse: &JsonSchemaExtra) -> String {
    match jse {
        JsonSchemaExtra::Real { iri, curie, digest } => format!(
            "{{\"$id\": {}, \"curie\": {}, \"definitionDigest\": {}, \"iri\": {}}}",
            py_string(iri),
            py_string(curie),
            py_string(digest),
            py_string(iri),
        ),
        JsonSchemaExtra::Envelope => "{\"envelope\": True}".to_owned(),
    }
}

fn render_base() -> Vec<u8> {
    let mut out = String::new();
    out.push_str(&render_docstring(
        "Shared Pydantic base for the GMEOW model package",
        &[
            "Every generated model inherits this alias/validation config; a".to_owned(),
            "per-class model_config only overrides `extra` and `json_schema_extra`.".to_owned(),
        ],
    ));
    out.push_str("from __future__ import annotations\n\n");
    out.push_str("from pydantic import BaseModel, ConfigDict\n\n\n");
    out.push_str("class ConfiguredBaseModel(BaseModel):\n");
    out.push_str("    model_config = ConfigDict(\n");
    out.push_str("        serialize_by_alias=True,\n");
    out.push_str("        validate_by_name=True,\n");
    out.push_str("        populate_by_name=True,\n");
    out.push_str("        arbitrary_types_allowed=True,\n");
    out.push_str("    )\n");
    finish_text(out)
}

struct RenderedModule {
    slug: String,
    text: Vec<u8>,
    model_names: Vec<String>,
    enum_names: Vec<String>,
}

fn render_init(modules: &[RenderedModule]) -> Vec<u8> {
    let mut out = String::new();
    out.push_str(&render_docstring(
        "GMEOW Pydantic model package",
        &[
            "Every model is re-exported here; after all imports a single".to_owned(),
            "`model_rebuild()` sweep resolves the deferred cross-slice type".to_owned(),
            "references, so per-slice modules never import one another.".to_owned(),
        ],
    ));
    out.push_str("from __future__ import annotations\n\n");
    out.push_str(&format!(
        "from .{BASE_MODULE} import ConfiguredBaseModel as ConfiguredBaseModel\n"
    ));
    // Slugs arrive sorted (modules built from a BTreeMap key iteration).
    for module in modules {
        out.push_str(&format!(
            "from .{} import *  # noqa: F401,F403\n",
            module.slug
        ));
    }
    out.push('\n');

    // Deterministic public surface.
    let mut all_names: BTreeSet<String> = BTreeSet::new();
    all_names.insert("ConfiguredBaseModel".to_owned());
    for module in modules {
        all_names.extend(module.model_names.iter().cloned());
        all_names.extend(module.enum_names.iter().cloned());
    }
    out.push_str("__all__ = [\n");
    for name in &all_names {
        out.push_str(&format!("    {},\n", py_string(name)));
    }
    out.push_str("]\n\n");

    // Resolve every deferred forward reference in one sweep over the package
    // namespace. StrEnum members carry no `model_rebuild`, so the guard skips
    // them.
    out.push_str(
        "_REBUILD_NS = {_k: _v for _k, _v in dict(globals()).items() if not _k.startswith(\"_\")}\n",
    );
    out.push_str("for _obj in list(_REBUILD_NS.values()):\n");
    out.push_str("    _rebuild = getattr(_obj, \"model_rebuild\", None)\n");
    out.push_str("    if callable(_rebuild):\n");
    out.push_str("        _rebuild(force=True, _types_namespace=_REBUILD_NS)\n");
    finish_text(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn repo_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    fn utf8<'a>(artifacts: &'a BTreeMap<String, Vec<u8>>, key: &str) -> &'a str {
        std::str::from_utf8(
            artifacts
                .get(key)
                .unwrap_or_else(|| panic!("missing {key}")),
        )
        .expect("utf8")
    }

    /// Count model class declarations across every module (a `class X(...)` whose
    /// base is NOT `StrEnum`), minus the one shared `ConfiguredBaseModel` base.
    fn model_class_count(artifacts: &BTreeMap<String, Vec<u8>>) -> usize {
        let mut n = 0usize;
        for (key, bytes) in artifacts {
            if !key.ends_with(".py") {
                continue;
            }
            for line in std::str::from_utf8(bytes).unwrap().lines() {
                if line.starts_with("class ") && line.ends_with("):") && !line.contains("(StrEnum)")
                {
                    if line == "class ConfiguredBaseModel(BaseModel):" {
                        continue;
                    }
                    n += 1;
                }
            }
        }
        n
    }

    /// The whole package, rendered over the real repo, is deterministic and
    /// structurally well-formed: one model per compiled `$def`, the package
    /// scaffolding is present, and the value-vocabulary / field / identity
    /// surfaces all appear. The `tags` slice module + the `__init__` sweep are
    /// snapshotted for regression.
    #[test]
    fn models_python_over_repo_is_deterministic_and_well_formed() {
        let root = repo_root();
        let first = render_models_python(&root).expect("render models");
        let second = render_models_python(&root).expect("render models again");
        assert_eq!(
            first.artifacts, second.artifacts,
            "pydantic package output is non-deterministic"
        );

        let a = &first.artifacts;
        for scaffold in [
            "gmeow_models/__init__.py",
            "gmeow_models/_base.py",
            "gmeow_models/_envelope.py",
            "gmeow_models/py.typed",
        ] {
            assert!(a.contains_key(scaffold), "missing {scaffold}");
        }

        // One model per class: the number of emitted models equals the number of
        // `$defs` the committed JSON Schema carries (the same compile output).
        let schema: Value = serde_json::from_slice(
            &std::fs::read(root.join("generated/schemas/gmeow.schema.json")).unwrap(),
        )
        .unwrap();
        let defs_count = schema["$defs"].as_object().unwrap().len();
        assert_eq!(
            model_class_count(a),
            defs_count,
            "expected exactly one model per $def"
        );

        // A StrEnum with an IRI/CURIE-valued member (the openEHR defining-code
        // value vocabularies).
        let has_iri_enum = a.values().any(|bytes| {
            let text = std::str::from_utf8(bytes).unwrap();
            let mut in_enum = false;
            for line in text.lines() {
                if line.starts_with("class ") {
                    in_enum = line.ends_with("(StrEnum):");
                }
                if in_enum && line.trim_start().contains(" = \"gmeow:") {
                    return true;
                }
            }
            false
        });
        assert!(has_iri_enum, "expected a StrEnum with an IRI/CURIE member");

        // A required field (no default) and an optional field (default=None).
        let all_text: String = a
            .values()
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .collect();
        assert!(
            all_text.contains(" = Field(default=None, alias="),
            "expected an optional field"
        );
        let has_required = a.values().any(|b| {
            String::from_utf8_lossy(b).lines().any(|l| {
                let l = l.trim_start();
                l.contains(": ") && l.ends_with(')') && l.contains(" = Field(alias=")
            })
        });
        assert!(has_required, "expected a required field (no default)");

        // json_schema_extra iri/curie/definitionDigest/$id for a known class.
        let accessibility = utf8(a, "gmeow_models/accessibility.py");
        assert!(accessibility.contains(
            "json_schema_extra={\"$id\": \"https://blackcatinformatics.ca/gmeow/AccessibilityAssertion\", \
             \"curie\": \"gmeow:AccessibilityAssertion\", \"definitionDigest\": \"blake3:"
        ), "AccessibilityAssertion must carry a full json_schema_extra identity with a digest");

        insta::assert_snapshot!("models_python_tags_module", utf8(a, "gmeow_models/tags.py"));
        insta::assert_snapshot!("models_python_init", utf8(a, "gmeow_models/__init__.py"));
    }

    /// A synthetic `$defs` exercises the transliteration paths the real corpus
    /// does not yet carry: a CLOSED (`additionalProperties:false`) model
    /// (`extra="forbid"`) beside an OPEN one (`extra="allow"`), a StrEnum whose
    /// member value is the full CURIE, a required-vs-optional field pair, and a
    /// typed cross-ref. Snapshotted + asserted.
    #[test]
    fn synthetic_defs_cover_closed_open_enum_and_fields() {
        let ns = gmeow_json_schema_namespaces();
        let closed_iri = "https://blackcatinformatics.ca/gmeow/DemoClosed";
        let open_iri = "https://blackcatinformatics.ca/gmeow/DemoOpen";

        let defs_value = json!({
            "DemoClosed": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "@id": { "type": "string" },
                    "@type": { "anyOf": [ { "type": "string" }, { "type": "array", "items": { "type": "string" } } ] },
                    "@annotation": { "$ref": "#/$defs/Annotation" },
                    "gmeow:label": { "type": "string" },
                    "gmeow:count": { "type": "integer" }
                },
                "required": ["gmeow:label"]
            },
            "DemoOpen": {
                "type": "object",
                "properties": {
                    "@id": { "type": "string" },
                    "gmeow:kind": { "enum": ["gmeow:demo/Blue", "gmeow:demo/Red"] },
                    "gmeow:closed": { "$ref": "#/$defs/DemoClosed" }
                }
            }
        });
        let defs = defs_value.as_object().unwrap();

        let defkey_to_iri: BTreeMap<String, String> = BTreeMap::from([
            ("DemoClosed".to_owned(), closed_iri.to_owned()),
            ("DemoOpen".to_owned(), open_iri.to_owned()),
        ]);

        let terms = [
            gmeow_docs::model::DocTerm {
                iri: closed_iri.to_owned(),
                curie: "gmeow:DemoClosed".to_owned(),
                owner_slice: "https://blackcatinformatics.ca/gmeow/slices/demo".to_owned(),
                content_digest: "blake3:demo-closed".to_owned(),
                ..Default::default()
            },
            gmeow_docs::model::DocTerm {
                iri: open_iri.to_owned(),
                curie: "gmeow:DemoOpen".to_owned(),
                owner_slice: "https://blackcatinformatics.ca/gmeow/slices/demo".to_owned(),
                content_digest: "blake3:demo-open".to_owned(),
                ..Default::default()
            },
        ];
        let term_index: BTreeMap<&str, &gmeow_docs::model::DocTerm> =
            terms.iter().map(|t| (t.iri.as_str(), t)).collect();
        let slice_by_iri: BTreeMap<&str, &DocSlice> = BTreeMap::new();

        let out = build_package(defs, &defkey_to_iri, &ns, &term_index, &slice_by_iri)
            .expect("build synthetic package");
        let demo = utf8(&out.artifacts, "gmeow_models/demo.py");

        assert!(
            demo.contains("extra=\"forbid\""),
            "closed class → extra=forbid"
        );
        assert!(demo.contains("extra=\"allow\""), "open class → extra=allow");
        assert!(
            demo.contains("class DemoOpenKindEnum(StrEnum):"),
            "enum property → StrEnum"
        );
        assert!(
            demo.contains("Red = \"gmeow:demo/Red\""),
            "enum member value must be the full CURIE, never a bare local name"
        );
        assert!(
            demo.contains("label: str = Field(alias=\"gmeow:label\")"),
            "required field has no default"
        );
        assert!(
            demo.contains("count: int | None = Field(default=None, alias=\"gmeow:count\")"),
            "optional field is `T | None = None`"
        );
        assert!(
            demo.contains("\"definitionDigest\": \"blake3:demo-closed\""),
            "json_schema_extra carries the term digest"
        );

        insta::assert_snapshot!("models_python_synthetic_demo", demo);
    }
}
