// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The ONE shared enriched-[`CompiledSchema`] builder every SHACL-derived schema
//! surface compiles through.
//!
//! [`crate::stages::json_schema`] and [`crate::stages::schemas`] (LinkML/TypeScript/
//! GraphQL, via `purrdf::shapes::{linkml,typescript,graphql}`) both need the SAME
//! compiled-and-enriched JSON Schema: the native `purrdf::shapes::json_schema::compile`
//! pass over the fresh SHACL shape union, with the ontology's open value vocabularies
//! folded in by [`crate::stages::value_vocab::enrich_value_vocab_enums`] (the SAME
//! enrichment the Pydantic surface applies). Lifting the compile+enrich+reserialize
//! sequence here keeps ONE copy of the byte convention (serde pretty + trailing LF)
//! so every downstream emitter reads byte-identical `$defs`.
//!
//! [`CompiledSchema`]: purrdf::shapes::json_schema::CompiledSchema

use std::path::Path;

/// Compile the SHACL `shapes` graph to a JSON Schema, enrich it with the
/// ontology's open value-vocabulary enums, and return the reconstructed
/// [`purrdf::shapes::json_schema::CompiledSchema`] every schema-surface emitter
/// consumes.
///
/// `schema_json` is re-serialized via `serde_json::to_vec_pretty` plus a
/// trailing newline (the same byte convention `purrdf`'s own compiler uses),
/// so this is a pure dedup of the compile+enrich+pretty-print sequence
/// previously inlined in [`crate::stages::json_schema`] — no behavior change.
pub(crate) fn enriched_compiled_schema(
    root: &Path,
    shapes: &purrdf::shapes::shapes::Shapes,
) -> Result<purrdf::shapes::json_schema::CompiledSchema, gmeow_errors::Diag> {
    let ns = gmeow_ns::gmeow_json_schema_namespaces();
    let compiled = purrdf::shapes::json_schema::compile(shapes, &ns);
    let mut schema: serde_json::Value =
        serde_json::from_str(&compiled.schema_json).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!("parse compiled JSON Schema: {e}"),
            })
        })?;
    let onto = crate::stages::value_vocab::load_ontology_store(root)?;
    let onto_view = crate::stages::export::FoldView::new(&onto);
    crate::stages::value_vocab::enrich_value_vocab_enums(&mut schema, &ns, &onto_view);
    let mut bytes = serde_json::to_vec_pretty(&schema).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("serialize enriched JSON Schema: {e}"),
        })
    })?;
    bytes.push(b'\n');
    let schema_json = String::from_utf8(bytes).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("enriched JSON Schema is not valid UTF-8: {e}"),
        })
    })?;
    Ok(purrdf::shapes::json_schema::CompiledSchema {
        schema_json,
        openapi_json: compiled.openapi_json,
        losses: compiled.losses,
    })
}
