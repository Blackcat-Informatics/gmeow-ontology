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

use oxigraph::io::{RdfFormat, RdfParser, RdfSerializer};
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
        for quad in RdfParser::from_format(RdfFormat::Turtle)
            .lenient()
            .for_reader(bytes.as_slice())
        {
            let quad = quad.map_err(|e| {
                PipelineError::Parse(format!("syntax error in {}: {e}", path.display()))
            })?;
            store
                .insert(&quad)
                .map_err(|e| PipelineError::Parse(format!("store insert failed: {e}")))?;
        }
    }
    Ok(store)
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
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    out.sort();
    Ok(out)
}

/// Serialize a store to canonical N-Quads bytes (sorted lines) for in-memory
/// passing between stages.
pub fn store_to_nquads(store: &Store) -> Result<Vec<u8>, PipelineError> {
    let mut buf: Vec<u8> = Vec::new();
    let mut serializer = RdfSerializer::from_format(RdfFormat::NQuads).for_writer(&mut buf);
    for quad in store.iter() {
        let quad = quad.map_err(|e| PipelineError::Parse(e.to_string()))?;
        serializer
            .serialize_quad(&quad)
            .map_err(|e| PipelineError::Parse(format!("serialize failed: {e}")))?;
    }
    serializer
        .finish()
        .map_err(|e| PipelineError::Parse(format!("serializer finish failed: {e}")))?;
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
    let store =
        Store::new().map_err(|e| PipelineError::Parse(format!("store creation failed: {e}")))?;
    for quad in RdfParser::from_format(RdfFormat::NQuads)
        .lenient()
        .for_reader(bytes)
    {
        let quad = quad.map_err(|e| PipelineError::Parse(format!("base-graph parse: {e}")))?;
        store
            .insert(&quad)
            .map_err(|e| PipelineError::Parse(format!("store insert failed: {e}")))?;
    }
    Ok(store)
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
