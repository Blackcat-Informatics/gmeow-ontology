// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Value-vocabulary enum enrichment for the SHACL→JSON-Schema surfaces.
//!
//! A `logic:AbstractIndividualType` class (Principle 17's *open value vocabulary*:
//! an "anchor, not a fence") declares its seed statuses as `gmeow:` INDIVIDUALS —
//! e.g. `gmeow:TermStability` with the members `gmeow:stable`, `gmeow:experimental`,
//! `gmeow:deprecated`. Those individuals live in the authored `module.ttl` ABox, not
//! in the SHACL shape graph, so `purrdf`'s shape compiler never sees them and the
//! compiled JSON Schema carries NO enum for them (it only emits an `enum` from an
//! explicit `sh:in`, which these open vocabularies deliberately do NOT declare — a
//! `sh:in` would fence the live validator shut, which doctrine forbids).
//!
//! This module closes that gap for the CLOSED-RECORD projections (the packed
//! `generated/schemas/gmeow.schema.json` and the Pydantic package) WITHOUT touching
//! the open live validator: it reads the value-vocabulary individuals straight from
//! the ontology store and enriches the compiled `$defs` in place —
//!
//! * a standalone `{ClassLocal}Enum` `$def` (`{"type":"string","enum":[…CURIEs]}`)
//!   per value vocabulary, reached even when NO property references it (this is how
//!   the annotation-only `gmeow:TermStability` gets projected); and
//! * every property whose SHACL range is a value-vocabulary class (a node-reference
//!   `$def` with no NodeShape) is repointed at its `{ClassLocal}Enum` `$def`.
//!
//! The SAME [`enrich_value_vocab_enums`] runs at EVERY site that consumes the
//! compiled `$defs` — the JSON-Schema export leaf, the Pydantic render path, and the
//! cross-surface agreement gate — so all three stay in lockstep by construction.
//!
//! The class→members enumeration ([`gmeow_individuals_by_class`]) is the ONE shared
//! derivation the LinkML/TS/GraphQL schema leaf ([`crate::stages::schemas`]) also
//! uses, so the two surfaces read the same individuals off the same store.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use purrdf::{Namespaces, RdfDataset, parse_dataset};
use serde_json::{Value, json};

use crate::gmeow_ns::{GMEOW_NS, LANG_NS, LOGIC_NS, MATH_NS};
use crate::stages::export::{DEFAULT_SCOPE, FoldView};

/// The open-value-vocabulary metaclass: a class typed `logic:AbstractIndividualType`
/// declares its members as `gmeow:` individuals (Principle 17).
const LOGIC_ABSTRACT_INDIVIDUAL_TYPE: &str =
    "https://blackcatinformatics.ca/logic/AbstractIndividualType";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";

/// The `gmeow:` local part of an IRI in a declared ecosystem namespace: `None` for
/// an IRI outside the authored `gmeow`/`logic`/`lang`/`math` namespaces.
fn ecosystem_prefix(prefix: &str) -> Option<&'static str> {
    match prefix {
        "gmeow" => Some(GMEOW_NS),
        "logic" => Some(LOGIC_NS),
        "lang" => Some(LANG_NS),
        "math" => Some(MATH_NS),
        _ => None,
    }
}

/// Expand a `prefix:local` CURIE (an emitted property/field key) to its full IRI in
/// the authored ecosystem namespaces. Returns `None` for a JSON-LD envelope key
/// (`@id`/`@type`/`@annotation`, which carry no `:` split of this shape) or a CURIE
/// in an undeclared namespace.
fn expand_ecosystem_curie(curie: &str) -> Option<String> {
    let (prefix, local) = curie.split_once(':')?;
    let ns = ecosystem_prefix(prefix)?;
    Some(format!("{ns}{local}"))
}

/// A single open value vocabulary: a `logic:AbstractIndividualType` class plus its
/// sorted, compacted member CURIEs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ValueVocab {
    /// The vocabulary class IRI (e.g. `…/gmeow/TermStability`).
    pub class_iri: String,
    /// The class `$defs`/discriminator key (the bare local name for a primary-namespace
    /// class, e.g. `TermStability`).
    pub class_local: String,
    /// The standalone enum `$def` key (`{class_local}Enum`, e.g. `TermStabilityEnum`).
    pub enum_key: String,
    /// The member values: sorted, deduplicated CURIEs (`gmeow:deprecated`, …).
    pub members: Vec<String>,
}

/// Every `gmeow:` individual of each requested class, keyed by the class' local name.
///
/// The ONE shared class→individuals enumeration: [`crate::stages::schemas`] reads it
/// to build the LinkML/TS/GraphQL enums and this module reads it to build the
/// JSON-Schema/Pydantic value-vocabulary enums, so both surfaces enumerate the same
/// individuals off the same store. Individual term ids are returned sorted by their
/// lexical IRI (deterministic).
pub(crate) fn gmeow_individuals_by_class(
    view: &FoldView<'_>,
    class_iris: &BTreeMap<String, String>,
) -> BTreeMap<String, Vec<usize>> {
    let mut by_class: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (cls_local, cls_iri) in class_iris {
        let mut inds: Vec<usize> = view
            .subjects_by_type(cls_iri, DEFAULT_SCOPE)
            .into_iter()
            .filter(|&ind| view.is_iri(ind) && view.lex(ind).starts_with(GMEOW_NS))
            .collect();
        inds.sort_by(|&a, &b| view.lex(a).cmp(view.lex(b)));
        inds.dedup();
        by_class.insert(cls_local.clone(), inds);
    }
    by_class
}

/// Derive every open value vocabulary from the ontology store: each primary-namespace
/// `logic:AbstractIndividualType` class that has ≥1 `gmeow:` individual, with its
/// members compacted to sorted CURIEs. Gated STRICTLY to `logic:AbstractIndividualType`
/// so ordinary structural classes (Agent, Asset, …) are never projected as enums.
pub(crate) fn derive_value_vocabs(view: &FoldView<'_>, ns: &Namespaces) -> Vec<ValueVocab> {
    let mut class_iris: BTreeMap<String, String> = BTreeMap::new();
    for cls in view.subjects_by_type(LOGIC_ABSTRACT_INDIVIDUAL_TYPE, DEFAULT_SCOPE) {
        if !view.is_iri(cls) {
            continue;
        }
        let iri = view.lex(cls);
        // Only the primary `gmeow:` namespace is keyed by bare local name and shipped
        // in these projections; a value vocabulary in another namespace is out of scope.
        if !ns.is_primary(iri) {
            continue;
        }
        let local = ns.def_key(iri);
        if local.is_empty() {
            continue;
        }
        class_iris.insert(local, iri.to_owned());
    }

    let by_class = gmeow_individuals_by_class(view, &class_iris);
    let mut vocabs: Vec<ValueVocab> = Vec::new();
    for (class_local, inds) in by_class {
        if inds.is_empty() {
            continue;
        }
        let Some(class_iri) = class_iris.get(&class_local) else {
            continue;
        };
        let mut members: Vec<String> = inds.iter().map(|&i| ns.compact_iri(view.lex(i))).collect();
        members.sort();
        members.dedup();
        vocabs.push(ValueVocab {
            class_iri: class_iri.clone(),
            enum_key: format!("{class_local}Enum"),
            class_local,
            members,
        });
    }
    vocabs
}

/// Whether a `$def` value is a standalone value-vocabulary enum (a bare
/// `{"type":"string","enum":[…]}`) rather than a model object.
pub(crate) fn is_enum_def(def: &Value) -> bool {
    def.get("enum").is_some() && def.get("properties").is_none() && def.get("$ref").is_none()
}

/// A node-reference-only property schema (`{"properties":{"@id":…},"required":["@id"],
/// "type":"object"}`) — the shape `purrdf` emits for an object property whose range
/// class has no NodeShape (every value vocabulary). Value-vocabulary-ranged fields wear
/// exactly this, so they are the repoint targets.
fn is_node_reference(obj: &serde_json::Map<String, Value>) -> bool {
    obj.get("type").and_then(Value::as_str) == Some("object")
        && obj
            .get("properties")
            .and_then(Value::as_object)
            .is_some_and(|p| p.len() == 1 && p.contains_key("@id"))
}

/// Whether a property value schema is multivalued: either the `anyOf:[single,
/// {type:array,…}]` shape or a bare `type:array`.
fn is_multivalued(pv: &Value) -> bool {
    if let Some(alts) = pv.get("anyOf").and_then(Value::as_array) {
        return alts
            .iter()
            .any(|a| a.get("type").and_then(Value::as_str) == Some("array"));
    }
    pv.get("type").and_then(Value::as_str) == Some("array")
}

/// Rewrite a value-vocabulary-ranged property schema to reference its enum `$def`,
/// preserving the field's cardinality (a single ref, or the multivalued
/// `anyOf:[ref, {array of ref}]`).
fn repoint_to_enum(pv: &mut Value, enum_key: &str) {
    let make_ref = || json!({ "$ref": format!("#/$defs/{enum_key}") });
    *pv = if is_multivalued(pv) {
        json!({
            "anyOf": [
                make_ref(),
                { "type": "array", "items": make_ref() }
            ]
        })
    } else {
        make_ref()
    };
}

/// Enrich a compiled JSON-Schema value in place with the ontology's open value
/// vocabularies: add one standalone `{ClassLocal}Enum` `$def` per vocabulary and
/// repoint every value-vocabulary-ranged property at its enum `$def`.
///
/// Applied IDENTICALLY at every consumer of the compiled `$defs`, so the shipped
/// JSON Schema, the Pydantic package, and the agreement gate agree by construction.
/// Deterministic: vocabularies and members are sorted; `$def` names are stable.
pub(crate) fn enrich_value_vocab_enums(
    schema: &mut Value,
    ns: &Namespaces,
    view: &FoldView<'_>,
) -> Vec<ValueVocab> {
    let vocabs = derive_value_vocabs(view, ns);
    if vocabs.is_empty() {
        return vocabs;
    }

    let Some(defs) = schema.get_mut("$defs").and_then(Value::as_object_mut) else {
        return vocabs;
    };

    // 1. A standalone enum `$def` per vocabulary. A value-vocabulary class that ALSO
    //    carries a NodeShape keeps its model `$def`; the enum `$def` is a distinct key.
    for v in &vocabs {
        defs.entry(v.enum_key.clone())
            .or_insert_with(|| json!({ "type": "string", "enum": v.members.clone() }));
    }

    // 2. Repoint value-vocabulary-ranged fields. A field is repointed only when it is a
    //    node-reference-only schema (its range class has NO NodeShape); a range class
    //    that has its own model `$def` keeps the structural `$ref`.
    let enum_by_class_iri: BTreeMap<&str, &str> = vocabs
        .iter()
        .filter(|v| !defs.contains_key(&v.class_local))
        .map(|v| (v.class_iri.as_str(), v.enum_key.as_str()))
        .collect();

    // Property CURIE → enum key, resolved from the property's `rdfs:range` in the store.
    let mut prop_to_enum: BTreeMap<String, String> = BTreeMap::new();
    let mut resolve_prop = |curie: &str| -> Option<String> {
        if let Some(hit) = prop_to_enum.get(curie) {
            return Some(hit.clone());
        }
        let iri = expand_ecosystem_curie(curie)?;
        let ptid = view.tid_of_iri(&iri)?;
        for r in view.objects(ptid, RDFS_RANGE, DEFAULT_SCOPE) {
            if view.is_iri(r)
                && let Some(&enum_key) = enum_by_class_iri.get(view.lex(r))
            {
                prop_to_enum.insert(curie.to_owned(), enum_key.to_owned());
                return Some(enum_key.to_owned());
            }
        }
        None
    };

    // Collect the (def_key, prop_key) sites to repoint, then apply — two passes so the
    // immutable range lookup and the mutable rewrite never borrow `defs` at once.
    let mut targets: Vec<(String, String, String)> = Vec::new();
    for (def_key, body) in defs.iter() {
        if is_enum_def(body) {
            continue;
        }
        let Some(props) = body.get("properties").and_then(Value::as_object) else {
            continue;
        };
        for (prop_key, pv) in props {
            if prop_key.starts_with('@') {
                continue;
            }
            let is_ref = pv.as_object().is_some_and(is_node_reference)
                || is_multivalued(pv)
                    && pv
                        .get("anyOf")
                        .and_then(Value::as_array)
                        .and_then(|a| a.first())
                        .and_then(Value::as_object)
                        .is_some_and(is_node_reference);
            if !is_ref {
                continue;
            }
            if let Some(enum_key) = resolve_prop(prop_key) {
                targets.push((def_key.clone(), prop_key.clone(), enum_key));
            }
        }
    }
    for (def_key, prop_key, enum_key) in targets {
        if let Some(pv) = defs
            .get_mut(&def_key)
            .and_then(Value::as_object_mut)
            .and_then(|b| b.get_mut("properties"))
            .and_then(Value::as_object_mut)
            .and_then(|p| p.get_mut(&prop_key))
        {
            repoint_to_enum(pv, &enum_key);
        }
    }

    vocabs
}

/// Recursively collect every authored `slices/**/module.ttl` (the canonical ABox that
/// declares the value-vocabulary classes and their `gmeow:` individuals). Test
/// fixtures under `tests/` carry no `module.ttl`, so the projection universe never
/// reaches them.
pub(crate) fn ontology_module_files(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                walk(&path, out);
            } else if path.file_name().and_then(|n| n.to_str()) == Some("module.ttl") {
                out.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(&root.join("slices"), &mut files);
    files
}

/// Parse the authored `module.ttl` ABox into ONE frozen dataset — the store the value
/// vocabularies are read from. HARD-fails if a source is unreadable or unparsable
/// (no silent fallback: a missing vocabulary must surface, never degrade).
pub(crate) fn load_ontology_store(root: &Path) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    let files = ontology_module_files(root);
    let mut parsed: Vec<Arc<RdfDataset>> = Vec::with_capacity(files.len());
    for file in &files {
        let bytes = std::fs::read(file)
            .map_err(|e| err(format!("read ontology source {}: {e}", file.display())))?;
        let ds = parse_dataset(&bytes, "text/turtle", None)
            .map_err(|e| err(format!("parse ontology source {}: {e}", file.display())))?;
        parsed.push(ds);
    }
    let refs: Vec<&RdfDataset> = parsed.iter().map(AsRef::as_ref).collect();
    Ok(Arc::new(RdfDataset::union(&refs)))
}

fn err(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "value-vocab-enrich".into(),
        message: message.into(),
    })
}
