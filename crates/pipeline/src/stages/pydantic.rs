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
//! * `gmeow_models/_base.py` — the shared `ConfiguredBaseModel` config.
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
//! The [`PydanticStage`] production leaf (`stage-export-pydantic`) renders through
//! [`render_models_python_from_shapes`] over the FRESH shape union
//! ([`crate::stages::shape_union_fresh`] — the generated members are THIS run's
//! consumed product bytes, never a stale disk read); the standalone
//! `gmeow-dev sync --mode update --outputs docs` entry ([`render_models_python`]) reads
//! the committed union. The carrier folds the stage output into the `models-python`
//! blob and writes the package tree to [`PACKAGE_DISK_PREFIX`] on disk.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::Value;

use gmeow_docs::model::{DocSlice, DocsModel};
use purrdf::shapes::shapes::Target;

use crate::gmeow_ns::{GMEOW_NS, LANG_NS, LOGIC_NS, MATH_NS, gmeow_json_schema_namespaces};
use crate::node::{Stage, StageInput, StageOutput, StageProduct};
use crate::stages::export::FoldView;
use crate::stages::schema_ident::{
    class_render_order, finish_text, local_name, py_string, sanitize_identifier, sanitize_type,
};
use crate::stages::value_vocab::{self, ValueVocab, enum_member_idents};

/// The synthetic JSON-LD-envelope module (`Node`) — not a slice.
const ENVELOPE_MODULE: &str = "_envelope";
/// The shared-base module (`ConfiguredBaseModel`).
const BASE_MODULE: &str = "_base";
/// The single-source wheel-version module (`__version__`), read by
/// `pyproject.toml`'s `[tool.hatch.version]`.
const ABOUT_MODULE: &str = "__about__";
/// The home for gmeow-namespaced shape-bearing classes declared only in the
/// shared test-DSL vocabulary (`dsl/`) that own no slice.
const SPEC_MODULE: &str = "_spec";
/// The package directory prefix every artifact key carries.
const PKG: &str = "gmeow_models";
/// The on-disk directory the shipped package lives under (the source tree the
/// `gmeow-ontology` wheel builds from). Blob members are package-relative
/// (`gmeow_models/...`); on disk they live under this prefix.
pub const PACKAGE_DISK_PREFIX: &str = "packages/python/";

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
    /// The allowlisted datatype widenings (empty in the current corpus). Read by
    /// the honesty surface (the notation-profile `declaredLoss` builder).
    #[allow(dead_code)]
    pub declared_datatype_losses: Vec<DeclaredDatatypeLoss>,
    /// Class IRI → the model's importable dotted path
    /// (`gmeow_models.<module>.<Class>`). Read by the docs surface to link a term
    /// page to its model (the term↔model bidirectional link).
    #[allow(dead_code)]
    pub dotted_paths: BTreeMap<String, String>,
}

/// The docstring wrap column (deterministic, char-boundary safe).
const WRAP_COLUMN: usize = 88;

/// The canonical docs-site base for a term page (`documentation/term/<slug>`).
const DOCS_TERM_BASE: &str = "https://blackcatinformatics.ca/gmeow/documentation/term/";

/// Word-wrap `text` to `WRAP_COLUMN` columns on whitespace boundaries, counting
/// Unicode scalar values (the prose corpus carries non-ASCII), so wrapping is a
/// pure function of the content and never platform-dependent. Blank input → no
/// lines.
fn wrap_prose(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    let mut cur_len = 0usize;
    for word in text.split_whitespace() {
        let wlen = word.chars().count();
        if cur.is_empty() {
            cur.push_str(word);
            cur_len = wlen;
        } else if cur_len + 1 + wlen <= WRAP_COLUMN {
            cur.push(' ');
            cur.push_str(word);
            cur_len += 1 + wlen;
        } else {
            lines.push(std::mem::take(&mut cur));
            cur.push_str(word);
            cur_len = wlen;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Make `s` safe to embed inside a Python triple-quoted docstring: escape
/// backslashes, then escape any `"""` run so it cannot terminate the string
/// (Python reads `\"` as `"`). Content is preserved, never elided.
fn doc_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace("\"\"\"", "\\\"\\\"\\\"")
}

fn err(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-export-pydantic".into(),
        message: message.into(),
    })
}

// ── Wheel version (owl:versionInfo → __about__.py) ──────────────────────────

/// The bare ontology IRI (no trailing slash) — the subject carrying
/// `owl:versionInfo` in `ontology/gmeow.ttl`'s header. Mirrors the same subject
/// `crate::stages::term_manifest`/`crate::stages::carrier`/`crate::stages::metadata`
/// each independently read `owl:versionInfo` off.
const ONTOLOGY_IRI: &str = "https://blackcatinformatics.ca/gmeow";
const OWL_VERSION_INFO: &str = "http://www.w3.org/2002/07/owl#versionInfo";

/// Load the authored ontology header (`ontology/gmeow.ttl`) and return its
/// `owl:versionInfo` literal verbatim — the SINGLE source of the `gmeow-ontology`
/// wheel version, stamped into the generated `gmeow_models/__about__.py`
/// (`pyproject.toml`'s `[tool.hatch.version]` reads `__version__` from there). A
/// hard requirement: never defaulted, and hard-fails when the value is missing or
/// is not a PEP 440 PUBLIC version identifier (never a local version — PyPI
/// rejects a `+local` segment, so this policy forbids one outright).
fn ontology_version_info(root: &Path) -> Result<String, gmeow_errors::Diag> {
    let path = root.join("ontology").join("gmeow.ttl");
    let bytes = std::fs::read(&path)
        .map_err(|e| err(format!("read ontology header {}: {e}", path.display())))?;
    let dataset = purrdf::parse_dataset(&bytes, "text/turtle", None)
        .map_err(|e| err(format!("parse ontology header {}: {e}", path.display())))?;
    for quad in dataset.owned_quads() {
        if quad.graph_name.is_some() {
            continue; // the ontology header lives in the default graph only.
        }
        let purrdf::model::RdfTerm::Iri(subject) = &quad.subject else {
            continue;
        };
        if subject != ONTOLOGY_IRI || quad.predicate != OWL_VERSION_INFO {
            continue;
        }
        let purrdf::model::RdfTerm::Literal(literal) = &quad.object else {
            continue;
        };
        let version = literal.lexical_form.clone();
        if !is_pep440_public_version(&version) {
            return Err(err(format!(
                "ontology {ONTOLOGY_IRI} owl:versionInfo {version:?} is not a PEP 440 public \
                 version identifier — refusing to stamp a malformed wheel version"
            )));
        }
        return Ok(version);
    }
    Err(err(format!(
        "authored ontology {ONTOLOGY_IRI} has no owl:versionInfo — cannot derive the wheel version"
    )))
}

/// Whether `raw` is a PEP 440–compliant PUBLIC version identifier — i.e. it has
/// NO local-version segment (`+...`), which this emitter forbids outright (PyPI
/// rejects a local version, so a wheel version can never legitimately carry one).
/// Hand-rolled (no external regex dependency): mirrors the canonical PEP 440
/// `VERSION_PATTERN` public-version grammar,
///
/// ```text
/// [v] [N!] N(.N)* [{a|b|c|rc|alpha|beta|pre|preview}[N]] [{.post|-|post|rev|r}[N]] [{.dev}[N]]
/// ```
///
/// case-insensitively, with the documented `-`/`_`/`.` separator flexibility
/// between segments. Any trailing byte after the matched grammar (in particular a
/// `+local` segment) fails the match.
fn is_pep440_public_version(raw: &str) -> bool {
    if raw.is_empty() || !raw.is_ascii() {
        return false;
    }
    let lower = raw.to_ascii_lowercase();
    let b = lower.as_bytes();
    let n = b.len();
    let mut i = 0usize;

    fn digits(b: &[u8], i: &mut usize) -> bool {
        let start = *i;
        while *i < b.len() && b[*i].is_ascii_digit() {
            *i += 1;
        }
        *i > start
    }
    fn sep(b: &[u8], i: &mut usize) {
        if *i < b.len() && matches!(b[*i], b'-' | b'_' | b'.') {
            *i += 1;
        }
    }
    fn tag(b: &[u8], i: &mut usize, tags: &[&str]) -> bool {
        for t in tags {
            if b[*i..].starts_with(t.as_bytes()) {
                *i += t.len();
                return true;
            }
        }
        false
    }

    // Optional leading "v" (PEP 440 permits it, e.g. `v1.0`).
    if i < n && b[i] == b'v' {
        i += 1;
    }
    // Optional epoch: digits "!".
    let save = i;
    if digits(b, &mut i) && i < n && b[i] == b'!' {
        i += 1;
    } else {
        i = save;
    }
    // Release segment: digits ("." digits)*  — at least one group, required.
    if !digits(b, &mut i) {
        return false;
    }
    loop {
        let save = i;
        if i < n && b[i] == b'.' {
            i += 1;
            if digits(b, &mut i) {
                continue;
            }
        }
        i = save;
        break;
    }
    // Optional pre-release: [-_.]? (preview|alpha|beta|pre|rc|a|b|c) [-_.]? N?
    // (tags ordered longest-first so a short alias never shadows a longer one).
    {
        let save = i;
        let mut j = i;
        sep(b, &mut j);
        const PRE_TAGS: &[&str] = &["preview", "alpha", "beta", "pre", "rc", "a", "b", "c"];
        if tag(b, &mut j, PRE_TAGS) {
            sep(b, &mut j);
            digits(b, &mut j);
            i = j;
        } else {
            i = save;
        }
    }
    // Optional post-release: ("-" N) | ([-_.]? (post|rev|r) [-_.]? N?).
    {
        let save = i;
        if i < n && b[i] == b'-' {
            let mut j = i + 1;
            if digits(b, &mut j) {
                i = j;
            }
        } else {
            let mut j = i;
            sep(b, &mut j);
            const POST_TAGS: &[&str] = &["post", "rev", "r"];
            if tag(b, &mut j, POST_TAGS) {
                sep(b, &mut j);
                digits(b, &mut j);
                i = j;
            } else {
                i = save;
            }
        }
    }
    // Optional dev-release: [-_.]? "dev" [-_.]? N?
    {
        let save = i;
        let mut j = i;
        sep(b, &mut j);
        if b[j..].starts_with(b"dev") {
            j += 3;
            sep(b, &mut j);
            digits(b, &mut j);
            i = j;
        } else {
            i = save;
        }
    }
    // No local-version segment permitted: the whole string must be consumed.
    i == n
}

// ── Public entry point ───────────────────────────────────────────────────────

/// Render the `gmeow_models` package as a `{package-relative-path: bytes}` map
/// (keys `gmeow_models/...`), the public entry point for
/// `gmeow-dev sync --mode update --outputs docs` (writes the tree to disk) — the same
/// bytes the pipeline stage folds into the `models-python` blob.
pub fn render_models_python_package(
    root: &Path,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    Ok(render_models_python(root)?.artifacts)
}

/// Render the Pydantic package from the COMMITTED (on-disk) shape union — the
/// standalone `make sync SYNC_OUTPUTS=docs` entry, which runs post-pipeline against
/// the fanout-refreshed committed files. The in-DAG [`PydanticStage`] must NOT use
/// this: it routes through [`crate::stages::shape_union_fresh::load_shapes_fresh`]
/// so the union's generated members are THIS run's product bytes (the
/// stale-disk-fold class).
pub(crate) fn render_models_python(root: &Path) -> Result<ModelsPython, gmeow_errors::Diag> {
    let (_store, shapes) = purrdf::shapes::shape_union::load_shapes(root)
        .map_err(|m| err(format!("load shape union: {m}")))?;
    render_models_python_from_shapes(root, &shapes)
}

/// Render the `gmeow_models` Pydantic package from an already-loaded shape union.
///
/// Deterministic: every collection is `BTreeMap`/sorted, there are no timestamps,
/// and the `$defs` iteration order is the compiler's sorted key order.
pub(crate) fn render_models_python_from_shapes(
    root: &Path,
    shapes: &purrdf::shapes::shapes::Shapes,
) -> Result<ModelsPython, gmeow_errors::Diag> {
    // 1. THE co-derivation point: compile the shape union with the exact call the
    //    JSON-Schema stage makes, so both surfaces read one `$defs`.
    let ns = gmeow_json_schema_namespaces();
    let compiled = purrdf::shapes::json_schema::compile(shapes, &ns);
    let mut schema: Value = serde_json::from_str(&compiled.schema_json)
        .map_err(|e| err(format!("parse compiled JSON Schema: {e}")))?;

    // Enrich the compiled `$defs` with the ontology's open value vocabularies (the
    // `logic:AbstractIndividualType` enums the SHACL shapes cannot carry) — the SAME
    // enrichment the JSON-Schema export leaf and the agreement gate apply, so all three
    // surfaces read one enriched `$defs`.
    let onto = value_vocab::load_ontology_store(root)?;
    let onto_view = FoldView::new(&onto);
    let value_vocabs = value_vocab::enrich_value_vocab_enums(&mut schema, &ns, &onto_view);

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

    // 3b. The wheel version: the ontology's `owl:versionInfo`, verbatim — the
    //     single source `gmeow_models/__about__.py` stamps and `pyproject.toml`
    //     reads through `[tool.hatch.version]`. Hard-fails if absent/malformed.
    let version = ontology_version_info(root)?;

    build_package(
        defs,
        &defkey_to_iri,
        &ns,
        &term_index,
        &slice_by_iri,
        &value_vocabs,
        &version,
    )
}

/// Transliterate one compiled `$defs` map into the Pydantic package. Split from
/// [`render_models_python`] so it can be exercised over a synthetic `$defs`
/// (e.g. a closed/`extra="forbid"` class the real corpus does not yet carry).
#[allow(clippy::too_many_arguments)]
fn build_package(
    defs: &serde_json::Map<String, Value>,
    defkey_to_iri: &BTreeMap<String, String>,
    ns: &purrdf::Namespaces,
    term_index: &BTreeMap<&str, &gmeow_docs::model::DocTerm>,
    slice_by_iri: &BTreeMap<&str, &DocSlice>,
    value_vocabs: &[ValueVocab],
    version: &str,
) -> Result<ModelsPython, gmeow_errors::Diag> {
    // The standalone value-vocabulary enums: their `(ident, value)` StrEnum members
    // (read once, deterministically) and the module each owns. A field repointed at an
    // enum registers it in the FIELD's module (via `resolve`); the owner module hosts it
    // even when NO property references it. Per-slice modules never import one another, so
    // an enum used across modules is emitted in each — harmless (identical StrEnum text).
    let value_enum_members: BTreeMap<String, Vec<(String, String)>> = value_vocabs
        .iter()
        .map(|v| (v.enum_key.clone(), enum_member_idents(&v.members)))
        .collect();
    let mut enum_owner_module: BTreeMap<String, String> = BTreeMap::new();
    for v in value_vocabs {
        let owner = route_class(&v.class_local, Some(&v.class_iri), ns, term_index)?;
        enum_owner_module.insert(v.enum_key.clone(), owner.module);
    }

    // The model `$defs` are every entry that is NOT a standalone value-vocabulary enum.
    let is_model_def = |key: &str, def: &Value| {
        !value_enum_members.contains_key(key) && !value_vocab::is_enum_def(def)
    };

    // 4. PASS 1 — route every model def to a module and resolve its identity.
    let defkey_to_class: BTreeMap<String, String> = defs
        .iter()
        .filter(|(k, v)| is_model_def(k, v))
        .map(|(k, _)| (k.clone(), py_type_name(k, "GmeowModel")))
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
    // CURIE → term, so a field (whose alias is the property CURIE) can pull its
    // `skos:definition` for the `Field(description=...)`.
    let curie_index: BTreeMap<&str, &gmeow_docs::model::DocTerm> = term_index
        .values()
        .filter(|t| !t.curie.is_empty())
        .map(|t| (t.curie.as_str(), *t))
        .collect();

    let mut routes: BTreeMap<String, ClassRoute> = BTreeMap::new();
    for (key, def) in defs {
        if !is_model_def(key, def) {
            continue;
        }
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
    // A value vocabulary's OWNING module hosts its enum even when the module carries no
    // model class (a slice whose only gmeow terms are value vocabularies) — seed it so
    // the enum is never dropped.
    for module in enum_owner_module.values() {
        module_keys.entry(module.clone()).or_default();
    }

    let mut losses: Vec<DeclaredDatatypeLoss> = Vec::new();
    let mut dotted_paths: BTreeMap<String, String> = BTreeMap::new();
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
            let class_term = if route.iri.is_empty() {
                None
            } else {
                term_index.get(route.iri.as_str()).copied()
            };
            let model = build_model(
                route,
                def,
                parent,
                &defkey_to_class,
                module,
                class_term,
                &curie_index,
                &value_enum_members,
                &mut enums,
                &mut needs,
                &mut losses,
            )?;
            if !route.iri.is_empty() {
                dotted_paths.insert(
                    route.iri.clone(),
                    format!("{PKG}.{module}.{}", route.class_name),
                );
            }
            models.push(model);
        }

        // Seed the value-vocabulary enums this module OWNS (reached even when no field
        // in the module references them — the annotation-only vocabulary case).
        for (enum_key, owner) in &enum_owner_module {
            if owner == module && !enums.contains_key(enum_key) {
                enums.insert(
                    enum_key.clone(),
                    PyEnum {
                        name: enum_key.clone(),
                        members: value_enum_members[enum_key].clone(),
                    },
                );
            }
        }

        let header = module_header(module, slice_by_iri);
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
    artifacts.insert(format!("{PKG}/{ABOUT_MODULE}.py"), render_about(version));
    artifacts.insert(format!("{PKG}/py.typed"), Vec::new());
    for module in &modules {
        artifacts.insert(format!("{PKG}/{}.py", module.slug), module.text.clone());
    }
    artifacts.insert(format!("{PKG}/__init__.py"), render_init(&modules));
    artifacts.insert(format!("{PKG}/README.md"), render_readme(&modules, version));

    Ok(ModelsPython {
        artifacts,
        declared_datatype_losses: losses,
        dotted_paths,
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
    let class_name = py_type_name(key, "GmeowModel");
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
    /// The standalone value-vocabulary enums (`enum_key → members`); a field whose
    /// `$ref` targets one registers that `StrEnum` into the current module's [`enums`].
    ///
    /// [`enums`]: FieldCtx::enums
    value_enums: &'a BTreeMap<String, Vec<(String, String)>>,
    enums: &'a mut BTreeMap<String, PyEnum>,
    needs: &'a mut ModuleNeeds,
    losses: &'a mut Vec<DeclaredDatatypeLoss>,
}

/// Resolve a `#/$defs/…` `$ref`. When it targets a standalone value-vocabulary enum,
/// register that `StrEnum` into the current module (so the field's module carries the
/// type it references — modules never import one another) and return the enum name;
/// otherwise resolve it to the referenced model class name.
fn resolve_ref(reference: &str, ctx: &mut FieldCtx<'_>) -> String {
    let target = reference.strip_prefix("#/$defs/").unwrap_or(reference);
    if let Some(members) = ctx.value_enums.get(target) {
        ctx.enums
            .entry(target.to_owned())
            .or_insert_with(|| PyEnum {
                name: target.to_owned(),
                members: members.clone(),
            });
        return target.to_owned();
    }
    ref_to_name(reference, ctx.defkey_to_class)
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
        return Ok(Resolved::scalar(resolve_ref(reference, ctx)));
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
            return Ok(Resolved::scalar(resolve_ref(r, ctx)));
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
        .unwrap_or_else(|| py_type_name(target, "GmeowModel"))
}

/// Extract the `StrEnum` member VALUE from one JSON-Schema `enum` member.
///
/// purrdf's value-schema convention (`purrdf::shapes::instance::project_value`,
/// shared with the `sh:in` enum emitter so a value and its enum member can never
/// drift) encodes an IRI/blank-node member as `{"@id": curie}` and a lang-tagged
/// or non-native typed literal as `{"@value": lexical, ...}`; plain strings and
/// numeric/boolean scalars stay bare. A `StrEnum` member is the identifying
/// string, so unwrap an object member to its inner `@id`/`@value` — never the
/// serialized object (that is how a bumped `sh:in` enum used to yield a member
/// like `"{\"@id\":\"gmeow:...\"}"`). Bare strings and numeric/boolean scalars
/// pass through unchanged (our value-vocabulary enums stay bare CURIEs). An
/// object with neither a string `@id` nor a string `@value`, or a `null`/array
/// member, is not a shape purrdf's value-schema convention produces — it is a
/// HARD FAIL (never silently re-serialized back into a StrEnum value).
fn enum_member_value(v: &Value) -> Result<String, gmeow_errors::Diag> {
    match v {
        Value::String(s) => Ok(s.clone()),
        Value::Number(_) | Value::Bool(_) => Ok(v.to_string()),
        Value::Object(map) => match (map.get("@id"), map.get("@value")) {
            (Some(Value::String(id)), _) => Ok(id.clone()),
            (_, Some(Value::String(lexical))) => Ok(lexical.clone()),
            _ => Err(err(format!(
                "enum member {v} is an object with neither a string \"@id\" nor a string \
                 \"@value\" — not a shape purrdf's value-schema convention produces; refusing to \
                 silently serialize it into a StrEnum value"
            ))),
        },
        Value::Null | Value::Array(_) => Err(err(format!(
            "enum member {v} is null or a nested array — not a valid StrEnum member; refusing to \
             silently serialize it into a StrEnum value"
        ))),
    }
}

/// Register (once) and name the `StrEnum` for an `{"enum": [...]}` schema. Member
/// VALUES are the identifying IRI/CURIE/literal (object members unwrapped to
/// their `@id`/`@value` inner string via [`enum_member_value`]); two members
/// sharing a value is a HARD FAIL (StrEnum would silently alias them).
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
    let mut values: Vec<String> = match obj.get("enum").and_then(Value::as_array) {
        Some(a) => a
            .iter()
            .map(enum_member_value)
            .collect::<Result<Vec<_>, _>>()?,
        None => Vec::new(),
    };
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
        let mut ident = py_ident(local_name(&value), "value");
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
    /// The property's `skos:definition` for `Field(description=...)`, when the
    /// property is documented.
    description: Option<String>,
    /// SHACL-derived `Field(...)` constraint kwargs (`kwarg`, `python-literal`),
    /// e.g. `("pattern", "\"^x\"")`, `("ge", "0")`, `("min_length", "1")`.
    constraints: Vec<(String, String)>,
}

/// A resolved model ready to render.
struct PyModel {
    class_name: String,
    parent: String,
    /// The fully-rendered class docstring body (already triple-quoted).
    docstring: String,
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
    class_term: Option<&gmeow_docs::model::DocTerm>,
    curie_index: &BTreeMap<&str, &gmeow_docs::model::DocTerm>,
    value_enums: &BTreeMap<String, Vec<(String, String)>>,
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
                        value_enums,
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
            let mut py_name = py_ident(&py_base, "field");
            while !used.insert(py_name.clone()) {
                py_name.push('_');
            }
            let is_envelope = matches!(key.as_str(), "@id" | "@type" | "@annotation");
            // The property's definition (its alias is the property CURIE), refined
            // by any class-local node target carried by this property's schema, and
            // its SHACL-derived Field constraints (pattern/min/max/length/items).
            let description = if is_envelope {
                None
            } else {
                let property_definition = curie_index
                    .get(key.as_str())
                    .and_then(|t| t.definition.clone());
                class_local_field_description(route, pv, property_definition)
            };
            let constraints = if is_envelope {
                Vec::new()
            } else {
                extract_constraints(pv)
            };
            fields.push(PyField {
                py_name,
                type_expr,
                alias: key.clone(),
                required: !is_envelope && required.contains(key.as_str()),
                description,
                constraints,
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

    let docstring = build_class_docstring(route, class_term, module);

    Ok(PyModel {
        class_name: route.class_name.clone(),
        parent,
        docstring,
        extra,
        jse,
        fields,
    })
}

/// Refine a property's global definition with any node target imposed by this
/// particular class shape. JSON Schema represents an unshaped RDF node target as a
/// small `{"@id": ...}` object whose `$comment` names the target class; retaining
/// that target in `Field(description=...)` prevents a class-local restriction such as
/// `ConceptCategorization -> observationResult only Concept` from being flattened into
/// the property's much broader global range prose.
fn class_local_field_description(
    route: &ClassRoute,
    property_schema: &Value,
    property_definition: Option<String>,
) -> Option<String> {
    fn scan(value: &Value, targets: &mut BTreeSet<String>) {
        match value {
            Value::Object(object) => {
                if let Some(comment) = object.get("$comment").and_then(Value::as_str)
                    && let Some(target) =
                        comment.strip_suffix(" has no NodeShape; node reference only")
                    && !target.is_empty()
                {
                    targets.insert(target.to_owned());
                }
                for child in object.values() {
                    scan(child, targets);
                }
            }
            Value::Array(array) => {
                for child in array {
                    scan(child, targets);
                }
            }
            _ => {}
        }
    }

    let mut targets = BTreeSet::new();
    scan(property_schema, &mut targets);
    if targets.is_empty() {
        return property_definition;
    }

    let class = if route.curie.is_empty() {
        route.class_name.as_str()
    } else {
        route.curie.as_str()
    };
    let local = format!(
        "Within {class}, values are node references constrained to {}.",
        targets.into_iter().collect::<Vec<_>>().join(" or ")
    );
    Some(match property_definition {
        Some(global) if !global.is_empty() => format!("{local} {global}"),
        _ => local,
    })
}

/// Extract the SHACL-derived JSON-Schema constraints from a property value
/// schema into Pydantic `Field(...)` kwargs. Scans the top-level object and any
/// `anyOf` alternative (the multivalued shape carries the element constraints on
/// the array alt's `items` and the cardinality on the array alt itself). First
/// writer wins per kwarg; the result is sorted for determinism.
fn extract_constraints(pv: &Value) -> Vec<(String, String)> {
    fn num_lit(v: &Value) -> Option<String> {
        v.as_i64()
            .map(|n| n.to_string())
            .or_else(|| v.as_f64().map(|n| n.to_string()))
    }
    fn scan(v: &Value, out: &mut BTreeMap<String, String>) {
        let Some(obj) = v.as_object() else {
            return;
        };
        let mut put = |k: &str, lit: String| {
            out.entry(k.to_owned()).or_insert(lit);
        };
        if let Some(p) = obj.get("pattern").and_then(Value::as_str) {
            put("pattern", py_string(p));
        }
        if let Some(n) = obj.get("minimum").and_then(num_lit) {
            put("ge", n);
        }
        if let Some(n) = obj.get("maximum").and_then(num_lit) {
            put("le", n);
        }
        if let Some(n) = obj.get("exclusiveMinimum").and_then(num_lit) {
            put("gt", n);
        }
        if let Some(n) = obj.get("exclusiveMaximum").and_then(num_lit) {
            put("lt", n);
        }
        if let Some(n) = obj.get("minLength").and_then(num_lit) {
            put("min_length", n);
        }
        if let Some(n) = obj.get("maxLength").and_then(num_lit) {
            put("max_length", n);
        }
        // Array cardinality maps to the list's length bounds.
        if let Some(n) = obj.get("minItems").and_then(num_lit) {
            put("min_length", n);
        }
        if let Some(n) = obj.get("maxItems").and_then(num_lit) {
            put("max_length", n);
        }
        if let Some(alts) = obj.get("anyOf").and_then(Value::as_array) {
            for a in alts {
                scan(a, out);
            }
        }
        if let Some(items) = obj.get("items") {
            scan(items, out);
        }
    }
    let mut collected: BTreeMap<String, String> = BTreeMap::new();
    scan(pv, &mut collected);
    collected.into_iter().collect()
}

/// Build the full, indented, triple-quoted class docstring — the *documentation
/// surface*: definition + when-to-use / avoid / how-to-use + examples + a runnable
/// usage doctest + the IRI/CURIE/docs back-link. A documented class draws its
/// prose from the term; a generated/undocumented class (openEHR archetype, spec)
/// gets an honest structural docstring naming its IRI — never a silent empty one.
/// Append `text`, doc-escaped and word-wrapped, into a docstring `body`. A bullet
/// item prefixes the first wrapped line with `- ` and continuations with two
/// spaces (both under a 4-space section indent).
fn wrap_into(body: &mut Vec<String>, text: &str, bullet: bool) {
    for (i, line) in wrap_prose(&doc_escape(text)).into_iter().enumerate() {
        if bullet {
            body.push(format!("    {}{line}", if i == 0 { "- " } else { "  " }));
        } else {
            body.push(line);
        }
    }
}

fn build_class_docstring(
    route: &ClassRoute,
    term: Option<&gmeow_docs::model::DocTerm>,
    module: &str,
) -> String {
    // A content line, indented 4 spaces (the class-body column); a blank string
    // becomes a truly blank line (no trailing whitespace).
    let mut body: Vec<String> = Vec::new();

    let summary = term
        .and_then(|t| t.label.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| route.class_name.clone());

    if let Some(term) = term {
        if let Some(def) = term.definition.as_deref().filter(|s| !s.is_empty()) {
            wrap_into(&mut body, def, false);
        }
        let sections: [(&str, &Vec<String>); 3] = [
            ("When to use", &term.use_when),
            ("When to avoid", &term.avoid_when),
            ("How to use", &term.how_to_use),
        ];
        for (heading, items) in sections {
            let items: Vec<&String> = items.iter().filter(|s| !s.is_empty()).collect();
            if items.is_empty() {
                continue;
            }
            body.push(String::new());
            body.push(format!("{heading}:"));
            for item in items {
                wrap_into(&mut body, item, true);
            }
        }
        let examples: Vec<&String> = term.examples.iter().filter(|s| !s.is_empty()).collect();
        if !examples.is_empty() {
            body.push(String::new());
            body.push("Examples:".to_owned());
            for ex in examples {
                wrap_into(&mut body, ex, true);
            }
        }
    } else if route.synthetic {
        body.push("Synthetic JSON-LD envelope type (not an authored ontology class).".to_owned());
    } else {
        body.push(format!(
            "Generated class projected from shapes; no authored definition ({}).",
            route.iri
        ));
    }

    // A runnable usage doctest (construct-then-inspect): importing IS using the
    // ontology. Skipped for the synthetic envelope (no CURIE identity).
    if !route.synthetic && !route.curie.is_empty() {
        body.push(String::new());
        body.push("Usage:".to_owned());
        body.push(format!(
            "    >>> from {PKG}.{module} import {}",
            route.class_name
        ));
        body.push(format!(
            "    >>> {}.model_config[\"json_schema_extra\"][\"curie\"]",
            route.class_name
        ));
        body.push(format!("    '{}'", route.curie));
    }

    // Identity + bidirectional docs back-link.
    if !route.iri.is_empty() {
        body.push(String::new());
        body.push(format!("IRI:    {}", route.iri));
    }
    if !route.curie.is_empty() {
        body.push(format!("CURIE:  {}", route.curie));
    }
    if let Some(term) = term
        && !term.slug.is_empty()
    {
        body.push(format!("Docs:   {DOCS_TERM_BASE}{}", term.slug));
    }

    // Assemble: summary on the opening line, body indented under it.
    let mut out = String::from("    \"\"\"");
    out.push_str(&doc_escape(&summary));
    out.push_str(".\n");
    if !body.is_empty() {
        out.push('\n');
        for line in &body {
            if line.is_empty() {
                out.push('\n');
            } else {
                out.push_str("    ");
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    out.push('\n');
    out.push_str("    GENERATED by the gmeow pydantic emitter — DO NOT EDIT.\n");
    out.push_str("    \"\"\"\n");
    out
}

/// A Python identifier from a raw token: [`sanitize_identifier`] then a
/// leading-digit guard. `sanitize_identifier` trims the leading `_` it would add,
/// so a purely-numeric token (e.g. an openEHR terminology code `433`) would emit
/// an invalid bare-number identifier; prefix a letter so it is a legal name.
fn py_ident(raw: &str, fallback: &str) -> String {
    let mut ident = sanitize_identifier(raw, fallback);
    if matches!(ident.chars().next(), Some(c) if c.is_ascii_digit()) {
        ident.insert(0, 'n');
    }
    ident
}

/// A Python class name from a raw token: [`sanitize_type`] then the same
/// leading-digit guard as [`py_ident`] (a numeric-leading class local name would
/// otherwise emit an invalid `class 433Foo`).
fn py_type_name(raw: &str, fallback: &str) -> String {
    let name = sanitize_type(raw, fallback);
    if matches!(name.chars().next(), Some(c) if c.is_ascii_digit()) {
        format!("N{name}")
    } else {
        name
    }
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

fn module_header(module: &str, slice_by_iri: &BTreeMap<&str, &DocSlice>) -> ModuleHeader {
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
    // A documented module: recover the owning slice by matching its module name (the
    // route stored the module as `module_slug(local_name(slice.iri))`). Independent of
    // the class list, so a value-vocabulary-only module still recovers its slice header.
    let slice = slice_by_iri
        .values()
        .find(|s| module_slug(local_name(&s.iri)) == module)
        .copied();
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
    // The Pydantic / shared-base imports are used ONLY by models; a value-vocabulary-only
    // module (StrEnums, no model classes) omits them so it carries no unused import.
    if models.is_empty() {
        out.push('\n');
    } else {
        out.push_str("from pydantic import ConfigDict, Field\n\n");
        out.push_str(&format!(
            "from .{BASE_MODULE} import ConfiguredBaseModel\n\n\n"
        ));
    }

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
    out.push_str(&model.docstring);
    out.push('\n');
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
            // Field kwargs, in a fixed order: default → constraints → description
            // → alias.
            let mut kwargs: Vec<String> = Vec::new();
            if !field.required {
                kwargs.push("default=None".to_owned());
            }
            for (kw, lit) in &field.constraints {
                kwargs.push(format!("{kw}={lit}"));
            }
            if let Some(desc) = &field.description {
                kwargs.push(format!("description={}", py_string(desc)));
            }
            kwargs.push(format!("alias={}", py_string(&field.alias)));
            let ty = if field.required {
                field.type_expr.clone()
            } else {
                format!("{} | None", field.type_expr)
            };
            out.push_str(&format!(
                "    {}: {} = Field({})\n",
                field.py_name,
                ty,
                kwargs.join(", ")
            ));
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

/// Render `gmeow_models/__about__.py` — the SINGLE source of the wheel version,
/// the ontology's `owl:versionInfo` verbatim. `pyproject.toml`'s
/// `[tool.hatch.version] path = "gmeow_models/__about__.py"` reads `__version__`
/// straight from here, so hatchling and the package agree by construction.
///
/// # Bump policy
///
/// Bump `owl:versionInfo` in `ontology/gmeow.ttl` and `make sync` — never
/// hand-edit this file or set `version` in `pyproject.toml` directly.
fn render_about(version: &str) -> Vec<u8> {
    let mut out = String::new();
    out.push_str(&render_docstring(
        "The gmeow-ontology wheel version — a single-source projection",
        &[
            "This is the ontology's owl:versionInfo (ontology/gmeow.ttl), verbatim.".to_owned(),
            "pyproject.toml's [tool.hatch.version] reads __version__ from here. To".to_owned(),
            "release a new wheel version, bump owl:versionInfo in ontology/gmeow.ttl".to_owned(),
            "and run `make sync` — never hand-edit this file or set `version`".to_owned(),
            "in pyproject.toml directly.".to_owned(),
        ],
    ));
    out.push_str("from __future__ import annotations\n\n");
    out.push_str(&format!("__version__ = {}\n", py_string(version)));
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
        "GMEOW Pydantic v2 model package — a functional documentation surface",
        &[
            "Reading these models IS reading the GMEOW ontology, and validating data".to_owned(),
            "with them IS using it. One Pydantic model per documented class, carrying".to_owned(),
            "its full definition, usage guidance, and worked examples in the docstring;".to_owned(),
            "SHACL-derived Field constraints; StrEnum value vocabularies; and a".to_owned(),
            "content-addressed IRI/CURIE/definitionDigest in each model's".to_owned(),
            "json_schema_extra.".to_owned(),
            String::new(),
            "Loss stance (Principle 17): this is a closed-record VALIDATION projection".to_owned(),
            "of the open-world ontology — it validates instance shape, it does not".to_owned(),
            "reason. See the per-term projection-fidelity table in the GMEOW docs.".to_owned(),
            String::new(),
            "Usage:".to_owned(),
            "    from gmeow_models.<slice> import <Class>".to_owned(),
            "    obj = <Class>.model_validate(payload)  # closed-world validation".to_owned(),
            String::new(),
            "Every model is re-exported here; after all imports a single".to_owned(),
            "model_rebuild() sweep resolves the deferred cross-slice type references,".to_owned(),
            "so per-slice modules never import one another (no import cycle).".to_owned(),
        ],
    ));
    out.push_str("from __future__ import annotations\n\n");
    out.push_str(&format!(
        "from .{BASE_MODULE} import ConfiguredBaseModel as ConfiguredBaseModel\n"
    ));
    out.push_str(&format!(
        "from .{ABOUT_MODULE} import __version__ as __version__\n"
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
    all_names.insert("__version__".to_owned());
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

/// The package README — a self-explaining orientation shipped alongside the code
/// so the extracted package documents itself.
fn render_readme(modules: &[RenderedModule], version: &str) -> Vec<u8> {
    let model_total: usize = modules.iter().map(|m| m.model_names.len()).sum();
    let mut out = String::new();
    out.push_str("# gmeow_models — the GMEOW ontology as a Pydantic v2 package\n\n");
    out.push_str(
        "This package is a GENERATED, deterministic projection of the GMEOW ontology.\n\
         Reading these models is reading the ontology; validating data with them is\n\
         using it. It is emitted from the SAME SHACL shape compilation as the GMEOW\n\
         JSON Schema, so a model's `model_json_schema()` agrees with the packed schema.\n\n",
    );
    out.push_str("## What each model carries\n\n");
    out.push_str(
        "- A docstring = the term's definition, when-to-use / avoid / how-to-use guidance,\n\
         \x20 worked examples, and a docs back-link.\n\
         - SHACL-derived `Field(...)` constraints (cardinality, min/max, length, pattern).\n\
         - `StrEnum` value vocabularies for the ontology's value families.\n\
         - `json_schema_extra` with the class `iri`, `curie`, and content-addressed\n\
         \x20 `definitionDigest` for traceability back to the canonical term.\n\n",
    );
    out.push_str("## Loss stance (Principle 17)\n\n");
    out.push_str(
        "This is a closed-record VALIDATION projection of an open-world ontology: it\n\
         validates instance shape, it does not reason. The per-term projection-fidelity\n\
         table in the GMEOW documentation records exactly what this surface preserves\n\
         and drops relative to the canonical `logic:` core.\n\n",
    );
    out.push_str("## Versioning\n\n");
    out.push_str(&format!(
        "The wheel version ({version}) is the ontology's `owl:versionInfo`\n\
         (`ontology/gmeow.ttl`), stamped verbatim into `gmeow_models/__about__.py` and\n\
         read by `pyproject.toml`'s `[tool.hatch.version]`. To release a new version,\n\
         bump `owl:versionInfo` and `make sync` — never hand-edit `__about__.py`\n\
         or set `version` in `pyproject.toml` directly.\n\n",
    ));
    out.push_str("## Usage\n\n");
    out.push_str(
        "```python\n\
         from gmeow_models.<slice> import <Class>\n\n\
         obj = <Class>.model_validate(payload)  # closed-world validation\n\
         schema = <Class>.model_json_schema()   # agrees with the packed GMEOW JSON Schema\n\
         ```\n\n",
    );
    out.push_str(&format!(
        "The package ships {model_total} models across {} modules (one module per slice,\n\
         plus the shared `_base`/`_envelope` scaffolding). Do not edit by hand — it is\n\
         regenerated from the ontology.\n",
        modules.len()
    ));
    finish_text(out)
}

// ── Pipeline stage ───────────────────────────────────────────────────────────

/// The committed on-disk root of the shipped package (the wheel source tree).
pub const PACKAGE_ROOT: &str = "packages/python/gmeow_models";

/// The `stage-export-pydantic` export-leaf stage: a fresh-union leaf (like
/// `stage-export-json-schema`) that renders the Pydantic model package from the
/// shape union + docs model, with the union's `generated/shapes/*.ttl` members
/// sourced from THIS run's consumed producer products (never the stale committed
/// files — the stale-disk-fold class). Its artifacts are written to disk under
/// [`PACKAGE_DISK_PREFIX`] (the wheel source) and folded into the `models-python`
/// blob by the carrier.
pub struct PydanticStage {
    consumes: Vec<String>,
}

impl PydanticStage {
    /// Construct the stage. It reads the AUTHORED shape/docs sources from disk and
    /// consumes the four generated-shape producers so the compiled union folds THIS
    /// run's fresh `generated/shapes/*.ttl` bytes.
    pub fn new() -> Self {
        Self {
            consumes: crate::stages::shape_union_fresh::producer_consumes(),
        }
    }
}

impl Default for PydanticStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for PydanticStage {
    fn id(&self) -> &str {
        "stage-export-pydantic"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        // v2: the union's generated/shapes/*.ttl members are product-sourced from the
        // consumed producer stages (shape_union_fresh) instead of read off disk, so a
        // shape-source edit reaches the emitted package in ONE regenerate.
        "pydantic.v2-fresh-shape-union"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
        // The emitter reads BOTH the AUTHORED half of the shape union (constraints)
        // and the docs model (prose/digests/slice routing), so both source sets bust
        // this leaf's cache. The GENERATED union members are NOT declared: they are
        // product-sourced off the consumed producer stages, whose product digests
        // already key the cache (a `generated/` path here would itself be the
        // stale-disk-fold bug class).
        let mut files = crate::stages::shape_union_fresh::authored_shape_files(root)?;
        files.extend(docs_source_files(root));
        files.sort();
        files.dedup();
        Ok(files)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let fresh = crate::stages::shape_union_fresh::fresh_generated_shape_members(
            self.id(),
            input.upstream,
        )?;
        let (_store, shapes) =
            crate::stages::shape_union_fresh::load_shapes_fresh(input.root, &fresh)?;
        let rendered = render_models_python_from_shapes(input.root, &shapes)?;
        // Key every member at its on-disk package path (the wheel source tree); the
        // carrier strips the prefix back to the package-relative blob member key.
        let artifacts: BTreeMap<String, Vec<u8>> = rendered
            .artifacts
            .into_iter()
            .map(|(k, v)| (format!("{PACKAGE_DISK_PREFIX}{k}"), v))
            .collect();
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            artifacts,
        )))
    }
}

/// The docs-model source files whose edits must bust this leaf's cache: every
/// slice `module.ttl` (definitions/prose). A term's content digest changes only
/// when its slice content changes, so the slice sources are the complete
/// change-detection set — the leaf never reaches into a `generated/` path (which
/// would be the stale-disk-fold bug class the pipeline-static lint guards).
fn docs_source_files(root: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let slices = root.join("slices");
    collect_named(&slices, "module.ttl", &mut files);
    files
}

/// Recursively collect every file named `name` under `dir`.
fn collect_named(dir: &Path, name: &str, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_named(&path, name, out);
        } else if path.file_name().and_then(|n| n.to_str()) == Some(name) {
            out.push(path);
        }
    }
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

    /// The canonical logic restriction vocabulary must reach the functional-model
    /// surface without relying on a pre-generated OWL/RDFS copy.  These three lang
    /// classes are intentionally restriction-only Pydantic targets: losing the
    /// `logic:subClassOf [ a logic:Restriction ; ... ]` derivation removes their
    /// JSON-Schema `$defs` and therefore their Python models/re-exports.
    #[test]
    fn logic_authored_lang_restrictions_remain_exported() {
        let source = r#"
@prefix lang:  <https://blackcatinformatics.ca/lang/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix owl:   <http://www.w3.org/2002/07/owl#> .

# The package's universal @annotation field references this infrastructure model.
gmeow:Annotation a owl:Class ;
    logic:subClassOf [ a logic:Restriction ;
        logic:onProperty gmeow:annotationTarget ;
        logic:allValuesFrom gmeow:Entity ] .

lang:WordForm a owl:Class ;
    logic:subClassOf [ a logic:Restriction ;
        logic:onProperty lang:lexemeOf ;
        logic:allValuesFrom lang:Lexeme ] .

lang:Grammar a owl:Class ;
    logic:subClassOf [ a logic:Restriction ;
        logic:onProperty lang:grammarFor ;
        logic:allValuesFrom lang:SignSystem ] .

lang:GrammarRule a owl:Class ;
    logic:subClassOf [ a logic:Restriction ;
        logic:onProperty lang:grammarRuleOf ;
        logic:allValuesFrom lang:Grammar ] .
"#;
        let ontology = purrdf::parse_dataset(source.as_bytes(), "text/turtle", None)
            .expect("parse canonical logic restriction fixture");
        let validation_shapes =
            gmeow_logic_compile::frontend::derive_validation_shapes(ontology.as_ref())
                .expect("derive validation shapes from logic restrictions");
        let program = gmeow_logic_compile::ir::LogicProgram::new(vec![], vec![], vec![], None)
            .with_validation_shapes(validation_shapes);
        let shacl =
            gmeow_logic_compile::projections::shapes::project_validation_shapes_shacl(&program);
        let shape_dataset = purrdf::parse_dataset(shacl.as_bytes(), "text/turtle", None)
            .expect("parse projected SHACL");
        let prefixes = purrdf::shapes::text_ingest::extract_prefixes(&shacl);
        let shapes = purrdf::shapes::shapes::from_dataset_with_prefixes(&shape_dataset, &prefixes)
            .expect("type projected SHACL");

        let rendered = render_models_python_from_shapes(&repo_root(), &shapes)
            .expect("render Pydantic models from logic-derived shapes");
        let init = utf8(&rendered.artifacts, "gmeow_models/__init__.py");
        let lang = utf8(&rendered.artifacts, "gmeow_models/lang.py");
        for (iri, class) in [
            (
                "https://blackcatinformatics.ca/lang/Grammar",
                "Lang_Grammar",
            ),
            (
                "https://blackcatinformatics.ca/lang/GrammarRule",
                "Lang_GrammarRule",
            ),
            (
                "https://blackcatinformatics.ca/lang/WordForm",
                "Lang_WordForm",
            ),
        ] {
            let dotted = format!("gmeow_models.lang.{class}");
            assert_eq!(
                rendered.dotted_paths.get(iri).map(String::as_str),
                Some(dotted.as_str()),
                "{iri} must retain its importable Pydantic model"
            );
            assert!(
                lang.contains(&format!("class {class}(")),
                "lang module must define {class}"
            );
            assert!(
                init.contains(&format!("\"{class}\"")),
                "package __init__ must re-export {class}"
            );
        }
    }

    /// A class-local exact-one dimension restriction must survive the complete
    /// canonical logic -> derived SHACL -> JSON Schema -> Pydantic path.  The
    /// committed generated schemas are deliberately not involved here: this test
    /// proves the fresh projection used by `make sync` without requiring generated
    /// artifacts to be refreshed during a focused source-only review fix.
    #[test]
    fn logic_authored_exact_one_dimension_is_a_required_scalar() {
        let source = r#"
@prefix math:  <https://blackcatinformatics.ca/math/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix owl:   <http://www.w3.org/2002/07/owl#> .

# The package's universal @annotation field references this infrastructure model.
gmeow:Annotation a owl:Class ;
    logic:subClassOf [ a logic:Restriction ;
        logic:onProperty gmeow:annotationTarget ;
        logic:allValuesFrom gmeow:Entity ] .

math:Dimension a owl:Class .
math:hasDimension a owl:ObjectProperty .
math:dimensionless a math:Dimension .

math:OddsValue a owl:Class ;
    logic:subClassOf
        [ a logic:Restriction ;
            logic:onProperty math:hasDimension ;
            logic:hasValue math:dimensionless ] ,
        [ a logic:Restriction ;
            logic:onProperty math:hasDimension ;
            logic:minQualifiedCardinality 1 ;
            logic:onClass owl:Thing ] ,
        [ a logic:Restriction ;
            logic:onProperty math:hasDimension ;
            logic:allValuesFrom math:Dimension ] ,
        [ a logic:Restriction ;
            logic:onProperty math:hasDimension ;
            logic:maxQualifiedCardinality 1 ;
            logic:onClass owl:Thing ] ,
        [ a logic:Restriction ;
            logic:onProperty math:hasDimension ;
            logic:maxQualifiedCardinality 1 ;
            logic:onClass math:Dimension ] .

math:LogOddsValue a owl:Class ;
    logic:subClassOf
        [ a logic:Restriction ;
            logic:onProperty math:hasDimension ;
            logic:hasValue math:dimensionless ] ,
        [ a logic:Restriction ;
            logic:onProperty math:hasDimension ;
            logic:minQualifiedCardinality 1 ;
            logic:onClass owl:Thing ] ,
        [ a logic:Restriction ;
            logic:onProperty math:hasDimension ;
            logic:allValuesFrom math:Dimension ] ,
        [ a logic:Restriction ;
            logic:onProperty math:hasDimension ;
            logic:maxQualifiedCardinality 1 ;
            logic:onClass owl:Thing ] ,
        [ a logic:Restriction ;
            logic:onProperty math:hasDimension ;
            logic:maxQualifiedCardinality 1 ;
            logic:onClass math:Dimension ] .
"#;
        let ontology = purrdf::parse_dataset(source.as_bytes(), "text/turtle", None)
            .expect("parse canonical exact-one dimension fixture");
        let validation_shapes =
            gmeow_logic_compile::frontend::derive_validation_shapes(ontology.as_ref())
                .expect("derive validation shapes from exact-one dimension restrictions");
        let program = gmeow_logic_compile::ir::LogicProgram::new(vec![], vec![], vec![], None)
            .with_validation_shapes(validation_shapes);
        let shacl =
            gmeow_logic_compile::projections::shapes::project_validation_shapes_shacl(&program);
        let shape_dataset = purrdf::parse_dataset(shacl.as_bytes(), "text/turtle", None)
            .expect("parse projected exact-one dimension SHACL");
        let prefixes = purrdf::shapes::text_ingest::extract_prefixes(&shacl);
        let shapes = purrdf::shapes::shapes::from_dataset_with_prefixes(&shape_dataset, &prefixes)
            .expect("type projected exact-one dimension SHACL");

        let rendered = render_models_python_from_shapes(&repo_root(), &shapes)
            .expect("render Pydantic models from exact-one dimension shapes");
        let math = utf8(&rendered.artifacts, "gmeow_models/math.py");
        for class_name in ["Math_LogOddsValue", "Math_OddsValue"] {
            let marker = format!("class {class_name}(");
            let start = math
                .find(&marker)
                .unwrap_or_else(|| panic!("missing {class_name}"));
            let rest = &math[start..];
            let end = rest.find("\nclass ").unwrap_or(rest.len());
            let class_body = &rest[..end];
            assert!(
                class_body.contains("hasDimension: str = Field("),
                "{class_name}.hasDimension must be a required scalar projected from its exact-one canonical restriction"
            );
        }
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
        // Count only MODEL `$defs`; the standalone value-vocabulary enums render as
        // `StrEnum` classes (excluded from `model_class_count`), not models.
        let defs_count = schema["$defs"]
            .as_object()
            .unwrap()
            .values()
            .filter(|v| !value_vocab::is_enum_def(v))
            .count();
        assert_eq!(
            model_class_count(a),
            defs_count,
            "expected exactly one model per model $def"
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
        // A required field = a `Field(...)` with NO `default=None` (may now carry a
        // description / constraint kwargs).
        let has_required = a.values().any(|b| {
            String::from_utf8_lossy(b).lines().any(|l| {
                let l = l.trim_start();
                l.contains(": ") && l.contains(" = Field(") && !l.contains("default=None")
            })
        });
        assert!(has_required, "expected a required field (no default)");

        // json_schema_extra iri/curie/definitionDigest/$id for a known class.
        let accessibility = utf8(a, "gmeow_models/accessibility.py");
        assert!(accessibility.contains(
            "json_schema_extra={\"$id\": \"https://blackcatinformatics.ca/gmeow/AccessibilityAssertion\", \
             \"curie\": \"gmeow:AccessibilityAssertion\", \"definitionDigest\": \"blake3:"
        ), "AccessibilityAssertion must carry a full json_schema_extra identity with a digest");

        // Task 2 — the package is EXEMPLARY documentation.
        // A documented class docstring carries structured guidance, a runnable usage
        // doctest, and a bidirectional docs back-link.
        assert!(
            all_text.contains("When to use:"),
            "expected at least one documented class with a 'When to use' section"
        );
        assert!(
            all_text.contains("    >>> from gmeow_models."),
            "expected a runnable usage doctest in class docstrings"
        );
        assert!(
            all_text.contains("Docs:   https://blackcatinformatics.ca/gmeow/documentation/term/"),
            "expected a docs back-link in documented class docstrings"
        );
        // Field-level documentation: a documented property carries a description.
        assert!(
            all_text.contains(", description=\""),
            "expected Field(description=...) from property definitions"
        );
        assert!(
            all_text.lines().any(|line| {
                line.contains("observationResult:")
                    && line.contains(
                        "Within gmeow:ConceptCategorization, values are node references constrained to gmeow:Concept."
                    )
            }),
            "ConceptCategorization.observationResult must retain its class-local Concept target in the Pydantic field description"
        );
        // Package README + enriched __init__ docstring are self-explaining.
        let readme = utf8(a, "gmeow_models/README.md");
        assert!(
            readme.contains("# gmeow_models") && readme.contains("Principle 17"),
            "README.md must orient the reader and state the loss stance"
        );
        assert!(
            utf8(a, "gmeow_models/__init__.py").contains("functional documentation surface"),
            "__init__ docstring frames the package as a functional documentation surface"
        );

        // The class → dotted-path map links a term IRI to its importable model.
        assert_eq!(
            first
                .dotted_paths
                .get("https://blackcatinformatics.ca/gmeow/AccessibilityAssertion"),
            Some(&"gmeow_models.accessibility.AccessibilityAssertion".to_owned()),
            "dotted-path map must link a class IRI to its importable model path"
        );

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

        let out = build_package(
            defs,
            &defkey_to_iri,
            &ns,
            &term_index,
            &slice_by_iri,
            &[],
            "0.0.0",
        )
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

    /// Regression: a StrEnum member whose value's local name is purely numeric (an
    /// openEHR terminology code) must emit a VALID Python identifier — `sanitize_*`
    /// trims the leading `_` it adds, so the guard prefixes a letter (`433` → `n433`)
    /// instead of an illegal bare-number member.
    #[test]
    fn numeric_enum_member_is_a_valid_python_identifier() {
        let ns = gmeow_json_schema_namespaces();
        let iri = "https://blackcatinformatics.ca/gmeow/CodeHolder";
        let defs_value = json!({
            "CodeHolder": {
                "type": "object",
                "properties": {
                    "gmeow:code": { "enum": ["gmeow:openehr/x/433", "gmeow:openehr/x/250"] }
                }
            }
        });
        let defs = defs_value.as_object().unwrap();
        let defkey_to_iri = BTreeMap::from([("CodeHolder".to_owned(), iri.to_owned())]);
        let terms = [gmeow_docs::model::DocTerm {
            iri: iri.to_owned(),
            curie: "gmeow:CodeHolder".to_owned(),
            owner_slice: "https://blackcatinformatics.ca/gmeow/slices/demo".to_owned(),
            content_digest: "blake3:code".to_owned(),
            ..Default::default()
        }];
        let term_index: BTreeMap<&str, &gmeow_docs::model::DocTerm> =
            terms.iter().map(|t| (t.iri.as_str(), t)).collect();
        let slice_by_iri: BTreeMap<&str, &DocSlice> = BTreeMap::new();
        let out = build_package(
            defs,
            &defkey_to_iri,
            &ns,
            &term_index,
            &slice_by_iri,
            &[],
            "0.0.0",
        )
        .expect("build package");
        let demo = utf8(&out.artifacts, "gmeow_models/demo.py");
        assert!(
            demo.contains("n433 = \"gmeow:openehr/x/433\""),
            "numeric enum member must be prefixed to a valid identifier"
        );
        assert!(
            !demo.contains("\n    433 = "),
            "must not emit a bare-number enum member"
        );
    }

    /// Size-budget gate (req 31): the rendered `models-python` surface stays under a
    /// generous uncompressed ceiling. The package is ~text, which compresses
    /// extremely well in the zstd-framed bundle blob, so the ceiling bounds the
    /// uncompressed render; inflating the package past it hard-fails HERE (a
    /// falsifiable ceiling, not a rubber stamp).
    #[test]
    fn models_python_stays_under_size_budget() {
        // 16 MiB uncompressed: comfortably above the current corpus render with
        // headroom for growth, far below any bundle concern once zstd-compressed.
        const CEILING_BYTES: usize = 16 * 1024 * 1024;
        let out = render_models_python(&repo_root()).expect("render models");
        let total: usize = out.artifacts.values().map(Vec::len).sum();
        assert!(
            total < CEILING_BYTES,
            "models-python render is {total} bytes, exceeding the {CEILING_BYTES}-byte budget ceiling"
        );
    }

    /// The constraint core of one emitted model, reconstructed from its rendered
    /// `.py` text — the Pydantic side of the Task-8a normalizer.
    #[derive(Default)]
    struct ModelCore {
        /// `alias` (the CURIE property key) → is-required (no `default=None`).
        fields: BTreeMap<String, bool>,
        /// The `ConfigDict(extra=...)` policy.
        extra: String,
    }

    /// Scan the whole rendered package into `class_name -> ModelCore` by walking each
    /// module's text: a `class X(...)` that is not a `StrEnum` opens a model; its
    /// `extra="..."` and each `Field(...)` line (alias = last kwarg; `default=None`
    /// ⇒ optional) populate the core. This is the emitted-side normalizer.
    fn scan_cores(artifacts: &BTreeMap<String, Vec<u8>>) -> BTreeMap<String, ModelCore> {
        fn quoted_after<'a>(line: &'a str, key: &str) -> Option<&'a str> {
            let start = line.rfind(key)? + key.len();
            let rest = &line[start..];
            let inner = rest.strip_prefix('"')?;
            inner.split('"').next()
        }
        let mut cores: BTreeMap<String, ModelCore> = BTreeMap::new();
        for (path, bytes) in artifacts {
            if !path.ends_with(".py") {
                continue;
            }
            let text = std::str::from_utf8(bytes).unwrap();
            let mut current: Option<String> = None;
            for line in text.lines() {
                if let Some(rest) = line.strip_prefix("class ") {
                    // `Name(Parent):`
                    current = None;
                    if !rest.contains("(StrEnum)")
                        && let Some(name) = rest.split('(').next()
                        && name != "ConfiguredBaseModel"
                    {
                        current = Some(name.to_owned());
                        cores.entry(name.to_owned()).or_default();
                    }
                    continue;
                }
                let Some(cls) = current.as_deref() else {
                    continue;
                };
                let trimmed = line.trim_start();
                if let Some(extra) = quoted_after(trimmed, "extra=") {
                    cores.get_mut(cls).unwrap().extra = extra.to_owned();
                } else if trimmed.contains(" = Field(")
                    && let Some(alias) = quoted_after(trimmed, "alias=")
                {
                    let required = !trimmed.contains("default=None");
                    cores
                        .get_mut(cls)
                        .unwrap()
                        .fields
                        .insert(alias.to_owned(), required);
                }
            }
        }
        cores
    }

    /// CROSS-SURFACE CONFORMANCE GATE (on-gate, Rust, no Python — Task 8a/8b):
    /// every packed `$def` and its emitted Pydantic model agree on the normalized
    /// constraint core — the field-alias set, the required set, and the
    /// `additionalProperties`/`extra` polarity. Both sides come from the ONE compiled
    /// `$defs`, so this proves the emitter faithfully renders that compilation into
    /// Python text (the live `model_json_schema()` confirmation runs off-gate). A
    /// dropped required field / property / wrong extra polarity hard-fails HERE.
    #[test]
    fn emitted_models_agree_with_packed_schema_defs() {
        let root = repo_root();
        let (_store, shapes) =
            purrdf::shapes::shape_union::load_shapes(&root).expect("load shapes");
        let ns = gmeow_json_schema_namespaces();
        let compiled = purrdf::shapes::json_schema::compile(&shapes, &ns);
        let mut schema: Value = serde_json::from_str(&compiled.schema_json).unwrap();
        // Apply the SAME value-vocabulary enrichment the render path applies, so both
        // sides read one enriched `$defs` and agreement holds by construction.
        let onto = value_vocab::load_ontology_store(&root).expect("load ontology store");
        let onto_view = FoldView::new(&onto);
        value_vocab::enrich_value_vocab_enums(&mut schema, &ns, &onto_view);
        let defs = schema["$defs"].as_object().unwrap();

        let artifacts = render_models_python(&root).expect("render").artifacts;
        let cores = scan_cores(&artifacts);

        let mut mismatches: Vec<String> = Vec::new();
        for (key, body) in defs {
            // The standalone value-vocabulary enums are StrEnums, not models — the
            // constraint-core agreement is about model `$defs`.
            if value_vocab::is_enum_def(body) {
                continue;
            }
            let class = py_type_name(key, "GmeowModel");
            let Some(core) = cores.get(&class) else {
                mismatches.push(format!("$def {key}: no emitted model {class}"));
                continue;
            };
            let want_extra = if body.get("additionalProperties") == Some(&Value::Bool(false)) {
                "forbid"
            } else {
                "allow"
            };
            if core.extra != want_extra {
                mismatches.push(format!(
                    "{class}: extra={:?} but $def wants {want_extra:?}",
                    core.extra
                ));
            }
            let want_props: BTreeSet<&str> = body
                .get("properties")
                .and_then(Value::as_object)
                .map(|p| p.keys().map(String::as_str).collect())
                .unwrap_or_default();
            let got_props: BTreeSet<&str> = core.fields.keys().map(String::as_str).collect();
            if want_props != got_props {
                mismatches.push(format!(
                    "{class}: property-alias set disagrees (missing {:?}, extra {:?})",
                    want_props.difference(&got_props).collect::<Vec<_>>(),
                    got_props.difference(&want_props).collect::<Vec<_>>()
                ));
            }
            let want_req: BTreeSet<&str> = body
                .get("required")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let got_req: BTreeSet<&str> = core
                .fields
                .iter()
                .filter(|(_, req)| **req)
                .map(|(a, _)| a.as_str())
                .collect();
            if want_req != got_req {
                mismatches.push(format!(
                    "{class}: required set disagrees (missing {:?}, extra {:?})",
                    want_req.difference(&got_req).collect::<Vec<_>>(),
                    got_req.difference(&want_req).collect::<Vec<_>>()
                ));
            }
        }
        assert!(
            mismatches.is_empty(),
            "Pydantic ⇄ JSON-Schema constraint-core disagreement ({} classes):\n{}",
            mismatches.len(),
            mismatches.join("\n")
        );
    }

    /// PEP 440 validator: known-good public version identifiers accept, and
    /// malformed / local-version strings are hard-rejected (req: no `+local`
    /// segment ever reaches the wheel, since PyPI rejects one).
    #[test]
    fn pep440_public_version_accepts_known_good_and_rejects_bad() {
        for good in [
            "0.1.0",
            "1.0",
            "1.0.0",
            "2026.7.12",
            "1!1.0",
            "1.0a1",
            "1.0b2",
            "1.0rc1",
            "1.0.dev0",
            "1.0.post1",
            "1.0-1",
            "1.0.0-alpha",
            "v1.0",
        ] {
            assert!(
                is_pep440_public_version(good),
                "expected {good:?} to be a valid PEP 440 public version"
            );
        }
        for bad in [
            "",
            "abc",
            "not-a-version",
            "1..0",
            "1.0+local",
            "1.0.0+local.1",
            "v1.0.beta.blah.blah",
        ] {
            assert!(
                !is_pep440_public_version(bad),
                "expected {bad:?} to be rejected as an invalid/local PEP 440 version"
            );
        }
    }

    /// The wheel version is sourced from the ontology header's `owl:versionInfo`
    /// (`ontology/gmeow.ttl`), verbatim — never a hand-maintained duplicate.
    #[test]
    fn ontology_version_info_reads_owl_version_info() {
        let version = ontology_version_info(&repo_root()).expect("ontology version");
        assert!(
            is_pep440_public_version(&version),
            "ontology owl:versionInfo {version:?} must be PEP 440-valid"
        );
    }

    /// The rendered `__about__.py` carries `__version__` set to the ontology's
    /// `owl:versionInfo` verbatim, and `__init__.py` re-exports it — the single
    /// source `pyproject.toml`'s `[tool.hatch.version]` reads.
    #[test]
    fn about_module_carries_ontology_version() {
        let root = repo_root();
        let version = ontology_version_info(&root).expect("ontology version");
        let out = render_models_python(&root).expect("render models");
        let about = utf8(&out.artifacts, "gmeow_models/__about__.py");
        assert!(
            about.contains(&format!("__version__ = \"{version}\"")),
            "__about__.py must stamp __version__ = {version:?} verbatim:\n{about}"
        );
        let init = utf8(&out.artifacts, "gmeow_models/__init__.py");
        assert!(
            init.contains("from .__about__ import __version__ as __version__"),
            "__init__.py must re-export __version__ from __about__.py"
        );
    }

    /// A `sh:in` enum member arrives from purrdf's value-schema convention as a
    /// JSON-LD node object `{"@id": curie}` (and a lang/typed literal as
    /// `{"@value": lexical, ...}`). The StrEnum VALUE must be the inner
    /// identifying string, never the serialized object — regression anchor for
    /// the purrdf 0.6.0 `sh:in`-enum object-encoding bump that otherwise yields a
    /// member value of `"{\"@id\":\"gmeow:...\"}"`.
    #[test]
    fn enum_member_value_unwraps_id_and_value_objects() {
        // IRI node object → its @id CURIE (the openEHR defining-code case).
        assert_eq!(
            enum_member_value(
                &json!({ "@id": "gmeow:openehr/bloodpressure/terminology/local/at0010" })
            )
            .unwrap(),
            "gmeow:openehr/bloodpressure/terminology/local/at0010"
        );
        // Typed / lang literal object → its @value lexical.
        assert_eq!(
            enum_member_value(&json!({ "@value": "mmHg", "@type": "xsd:string" })).unwrap(),
            "mmHg"
        );
        assert_eq!(
            enum_member_value(&json!({ "@value": "haut", "@language": "fr" })).unwrap(),
            "haut"
        );
        // Bare string / scalar members (our value-vocabulary enums) pass through.
        assert_eq!(
            enum_member_value(&json!("math:twoSidedAlternative")).unwrap(),
            "math:twoSidedAlternative"
        );
        assert_eq!(enum_member_value(&json!(1)).unwrap(), "1");
        assert_eq!(enum_member_value(&json!(true)).unwrap(), "true");
    }

    /// An object with neither a string `@id` nor a string `@value`, or a
    /// `null`/array member, is not a shape purrdf's value-schema convention
    /// produces — `enum_member_value` must hard-fail rather than silently
    /// serialize it into a StrEnum value (that is exactly the bug the
    /// unwrapping above fixes; a fallthrough re-introduces it for any shape
    /// that slips through the recognized cases).
    #[test]
    fn enum_member_value_hard_fails_on_unrecognized_shapes() {
        assert!(enum_member_value(&json!({ "foo": "bar" })).is_err());
        assert!(enum_member_value(&json!({ "@id": 5 })).is_err());
        assert!(enum_member_value(&json!(null)).is_err());
        assert!(enum_member_value(&json!(["x"])).is_err());
    }
}
