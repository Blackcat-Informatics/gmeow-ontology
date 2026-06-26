// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `source_load` stage (#861 P3): parse the authored ontology sources into
//! one in-memory base graph.
//!
//! This is the root of the build DAG. It loads `ontology/gmeow.ttl`, every
//! `slices/<group>/<name>/module.ttl`, and every `imports/*.ttl` into a single
//! oxigraph store — the RDF 1.1 base graph the Python build assembled via
//! `load_merged_graph(include_imports=…)`. The store is published as an in-memory
//! N-Quads artifact so downstream stages parse it from memory instead of
//! re-reading `gmeow.gts` from disk per generator (the bottleneck #861 removes).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_rdf::oxigraph::{
    flat_oxigraph_quads_from_dataset, flat_oxigraph_quads_from_dataset_scoped, GraphPolicy,
};
use gmeow_rdf::{parse_dataset, serialize_dataset, SerializeGraph};
use oxigraph::store::Store;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};

/// Logical path of the published base graph (N-Quads, in-memory dataflow).
pub const BASE_GRAPH_PATH: &str = "pipeline/base-graph.nq";

/// Load `ontology/gmeow.ttl` + all slice modules + all imports into one store.
pub fn load_authored_store(root: &Path) -> Result<Store, PipelineError> {
    let store =
        Store::new().map_err(|e| PipelineError::Parse(format!("store creation failed: {e}")))?;
    for path in authored_files(root)? {
        let bytes = std::fs::read(&path)?;
        // SCOPE by the source path: each authored file is a distinct RDF document, so
        // its anonymous blanks (`_:gts_<counter>`, restarting per parse) must not merge
        // with another file's identically-labelled blanks when they share this store.
        let scope = path.display().to_string();
        rdf_bytes_into_store_scoped(&store, &bytes, "text/turtle", &scope, &scope)?;
    }
    Ok(store)
}

/// Like [`rdf_bytes_into_store`] but scopes blank node labels by `scope_key` (a stable
/// per-source identity) so documents accumulated into one store keep disjoint blanks.
pub fn rdf_bytes_into_store_scoped(
    store: &Store,
    bytes: &[u8],
    media_type: &str,
    scope_key: &str,
    context: &str,
) -> Result<(), PipelineError> {
    let dataset = parse_dataset(bytes, media_type, None)
        .map_err(|e| PipelineError::Parse(format!("syntax error in {context}: {e}")))?;
    for quad in flat_oxigraph_quads_from_dataset_scoped(&dataset, scope_key)
        .map_err(|e| PipelineError::Parse(format!("IR → quads in {context}: {e}")))?
    {
        store
            .insert(&quad)
            .map_err(|e| PipelineError::Parse(format!("store insert failed: {e}")))?;
    }
    Ok(())
}

/// The sorted authored Turtle files that form the base graph (the hidden-input
/// closure `source_load` declares so the cache key cannot go stale).
pub fn authored_files(root: &Path) -> Result<Vec<PathBuf>, PipelineError> {
    let mut files: Vec<PathBuf> = Vec::new();
    let onto = root.join("ontology").join("gmeow.ttl");
    if onto.exists() {
        files.push(onto);
    }
    files.extend(module_files(root)?);
    files.extend(ttl_files_in(&root.join("imports"))?);
    files.sort();
    Ok(files)
}

/// Every `slices/<group>/<name>/module.ttl`.
pub fn module_files(root: &Path) -> Result<Vec<PathBuf>, PipelineError> {
    let mut out = Vec::new();
    let slices = root.join("slices");
    if !slices.is_dir() {
        return Ok(out);
    }
    for group in sorted_dirs(&slices)? {
        for slice_dir in sorted_dirs(&group)? {
            let module = slice_dir.join("module.ttl");
            if module.is_file() {
                out.push(module);
            }
        }
    }
    Ok(out)
}

/// Every `slices/<group>/<name>/manifest.ttl` (the sibling of each `module.ttl`),
/// for export leaves whose cache key must reflect the slice manifests they read
/// directly from disk (catalog, profiles, matrix — `gmeow:sliceProfile` /
/// `sliceTier` / `sliceDependsOn` live in the manifest, NOT the composed fold).
pub fn manifest_files(root: &Path) -> Result<Vec<PathBuf>, PipelineError> {
    let mut out = Vec::new();
    for module in module_files(root)? {
        let manifest = module.with_file_name("manifest.ttl");
        if manifest.is_file() {
            out.push(manifest);
        }
    }
    Ok(out)
}

fn ttl_files_in(dir: &Path) -> Result<Vec<PathBuf>, PipelineError> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|x| x == "ttl") {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

fn sorted_dirs(dir: &Path) -> Result<Vec<PathBuf>, PipelineError> {
    // Fail-fast on a read_dir entry error: a transient FS error must surface, not
    // silently drop a slice group/dir (no-optionality, #863).
    let mut out: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

/// Serialize a store to canonical N-Quads bytes (sorted lines) for in-memory
/// passing between stages.
pub fn store_to_nquads(store: &Store) -> Result<Vec<u8>, PipelineError> {
    let dataset = gmeow_rdf::oxigraph::dataset_from_store(store)
        .map_err(|e| PipelineError::Parse(e.to_string()))?;
    let buf = serialize_dataset(&dataset, "application/n-quads", SerializeGraph::Dataset)
        .map_err(|e| PipelineError::Parse(format!("serialize failed: {e}")))?;
    // Sort lines for determinism (oxigraph iteration order is not guaranteed).
    let text = String::from_utf8(buf)
        .map_err(|e| PipelineError::Parse(format!("non-utf8 n-quads: {e}")))?;
    let mut lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    lines.sort_unstable();
    let mut out = lines.join("\n");
    out.push('\n');
    Ok(out.into_bytes())
}

/// Parse the published base-graph N-Quads artifact back into a store (the
/// in-memory hand-off downstream stages use instead of re-reading from disk).
pub fn parse_base_graph(bytes: &[u8]) -> Result<Store, PipelineError> {
    let dataset = parse_dataset(bytes, "application/n-quads", None)
        .map_err(|e| PipelineError::Parse(format!("base-graph parse: {e}")))?;
    gmeow_rdf::oxigraph::store_from_dataset(&dataset, GraphPolicy::PreserveNamedGraphs)
        .map_err(|e| PipelineError::Parse(format!("base-graph store: {e}")))
}

/// Parse RDF text `bytes` of `media_type` into a fresh oxigraph store via the
/// native codecs, preserving named graphs. `context` labels parse errors.
pub fn rdf_bytes_to_store(
    bytes: &[u8],
    media_type: &str,
    context: &str,
) -> Result<Store, PipelineError> {
    let dataset = parse_dataset(bytes, media_type, None)
        .map_err(|e| PipelineError::Parse(format!("syntax error in {context}: {e}")))?;
    gmeow_rdf::oxigraph::store_from_dataset(&dataset, GraphPolicy::PreserveNamedGraphs)
        .map_err(|e| PipelineError::Parse(format!("store build for {context}: {e}")))
}

/// Parse RDF text `bytes` of `media_type` and insert the resulting quads into an
/// existing `store`, accumulating across calls. `context` labels parse errors.
pub fn rdf_bytes_into_store(
    store: &Store,
    bytes: &[u8],
    media_type: &str,
    context: &str,
) -> Result<(), PipelineError> {
    let dataset = parse_dataset(bytes, media_type, None)
        .map_err(|e| PipelineError::Parse(format!("syntax error in {context}: {e}")))?;
    for quad in flat_oxigraph_quads_from_dataset(&dataset)
        .map_err(|e| PipelineError::Parse(format!("IR → quads in {context}: {e}")))?
    {
        store
            .insert(&quad)
            .map_err(|e| PipelineError::Parse(format!("store insert failed: {e}")))?;
    }
    Ok(())
}

/// Parse Turtle `bytes` into a fresh oxigraph store via the native codecs.
///
/// The native `parse_dataset` folds the RDF 1.2 statement layer, and
/// `store_from_dataset` materialises the result with named graphs preserved (a
/// stand-alone Turtle document only ever populates the default graph, so the
/// policy is equivalent to flattening here). `context` labels parse errors.
pub fn turtle_bytes_to_store(bytes: &[u8], context: &str) -> Result<Store, PipelineError> {
    rdf_bytes_to_store(bytes, "text/turtle", context)
}

/// Parse Turtle `bytes` and insert the resulting quads into an existing `store`,
/// accumulating across calls. `context` labels parse errors.
pub fn turtle_bytes_into_store(
    store: &Store,
    bytes: &[u8],
    context: &str,
) -> Result<(), PipelineError> {
    rdf_bytes_into_store(store, bytes, "text/turtle", context)
}

/// Parse Turtle `bytes` from a distinct source document (`scope_key` = its stable
/// identity, e.g. the file path) and insert into `store`, scoping blank-node labels
/// so several source files accumulated into one store keep disjoint anonymous blanks.
pub fn turtle_bytes_into_store_scoped(
    store: &Store,
    bytes: &[u8],
    scope_key: &str,
) -> Result<(), PipelineError> {
    rdf_bytes_into_store_scoped(store, bytes, "text/turtle", scope_key, scope_key)
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `source_load` pipeline stage.
pub struct SourceLoadStage;

impl Stage for SourceLoadStage {
    fn id(&self) -> &str {
        "stage-source-load"
    }
    fn kind(&self) -> StageKind {
        StageKind::SourceLoad
    }
    fn consumes(&self) -> &[String] {
        &[]
    }
    fn impl_version(&self) -> &str {
        "source_load.v1"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        let store = load_authored_store(input.root)?;
        let nq = store_to_nquads(&store)?;
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(BASE_GRAPH_PATH.to_string(), nq);
        Ok(StageOutput {
            product: StageProduct::from_artifacts(self.id(), artifacts),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn source_load_parses_the_whole_ontology() {
        let root = repo_root();
        let store = load_authored_store(&root).expect("load");
        // The merged authored graph is substantial (50+ slices); sanity-floor it.
        assert!(
            store.len().unwrap() > 5_000,
            "authored base graph unexpectedly small: {} quads",
            store.len().unwrap()
        );
        // Round-trips through the in-memory N-Quads hand-off.
        let nq = store_to_nquads(&store).expect("serialize");
        let back = parse_base_graph(&nq).expect("reparse");
        assert_eq!(store.len().unwrap(), back.len().unwrap());
    }

    #[test]
    fn authored_files_includes_root_and_modules() {
        let root = repo_root();
        let files = authored_files(&root).unwrap();
        assert!(files.iter().any(|p| p.ends_with("ontology/gmeow.ttl")));
        assert!(files
            .iter()
            .any(|p| p.ends_with("slices/core/pipeline/module.ttl")));
        assert!(
            files.len() > 50,
            "expected 50+ authored files, got {}",
            files.len()
        );
    }
}
