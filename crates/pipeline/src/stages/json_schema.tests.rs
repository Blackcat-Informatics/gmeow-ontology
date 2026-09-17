// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

#[test]
fn json_schema_stage_emits_valid_authenticated_artifacts() {
    let root = repo_root();
    let first = crate::fixture::stage_artifacts(&root, 1, "stage-export-json-schema")
        .expect("authenticated JSON Schema projection");

    let schema_bytes = first
        .get(JSON_SCHEMA_PATH)
        .expect("schema artifact present");
    let openapi_bytes = first.get(OPENAPI_PATH).expect("openapi artifact present");

    // Both parse as JSON.
    let schema: serde_json::Value =
        serde_json::from_slice(schema_bytes).expect("schema is valid JSON");
    let _openapi: serde_json::Value =
        serde_json::from_slice(openapi_bytes).expect("openapi is valid JSON");

    // The schema has a non-empty `$defs` object.
    let defs = schema
        .get("$defs")
        .and_then(|v| v.as_object())
        .expect("schema has a $defs object");
    assert!(!defs.is_empty(), "$defs must be non-empty");

    for path in [CARD_SCHEMA_PATH, FINDING_SCHEMA_PATH] {
        assert!(first.contains_key(path), "missing {path}");
    }
}

/// Recursively collect every `#/$defs/<name>` ref reachable from a value.
fn collect_def_refs(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::Object(map) => {
            if let Some(serde_json::Value::String(r)) = map.get("$ref")
                && let Some(name) = r.strip_prefix("#/$defs/")
            {
                out.push(name.to_owned());
            }
            for child in map.values() {
                collect_def_refs(child, out);
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                collect_def_refs(child, out);
            }
        }
        _ => {}
    }
}

/// Corpus self-consistency invariant: compiling over the REAL repo
/// shape union must produce ZERO dangling `$ref`s — every `#/$defs/<name>`
/// the schema references must resolve to an emitted `$def`. This guards the
/// real corpus against the dangling-ref bug a draft-2020-12 validator rejects
/// (`Pointer '/$defs/<name>' does not exist`).
#[test]
fn json_schema_corpus_has_no_dangling_refs() {
    let root = repo_root();
    let artifacts = crate::fixture::stage_artifacts(&root, 1, "stage-export-json-schema")
        .expect("authenticated JSON Schema projection");
    let schema: serde_json::Value = serde_json::from_slice(
        artifacts
            .get(JSON_SCHEMA_PATH)
            .expect("schema artifact present"),
    )
    .expect("schema is valid JSON");

    let defs: std::collections::BTreeSet<String> = schema
        .get("$defs")
        .and_then(|v| v.as_object())
        .expect("$defs object")
        .keys()
        .cloned()
        .collect();

    let mut refs = Vec::new();
    collect_def_refs(&schema, &mut refs);
    assert!(!refs.is_empty(), "expected refs in the real corpus schema");

    let dangling: Vec<&String> = refs.iter().filter(|r| !defs.contains(*r)).collect();
    assert!(
        dangling.is_empty(),
        "schema references {} dangling $defs over the real corpus: {:?}",
        dangling.len(),
        dangling
    );
}
